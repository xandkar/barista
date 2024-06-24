use std::{path::Path, time::Duration};

use anyhow::{anyhow, Context};
use barista::conf;
use clap::Parser;

use tokio::{fs, task::JoinSet};
use tracing::Instrument;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Path to the working directory.
    #[clap(long, default_value = concat!("~/.", barista::NAME!()))]
    dir: String,

    /// Enables RPC logging. Sets level to DEBUG.
    #[clap(short, long, default_value_t = false)]
    debug: bool,

    /// Specify log level. Overrides level set by the debug flag.
    #[clap(short, long = "log")]
    log_level: Option<tracing::Level>,

    #[clap(short, long, default_value_t = 5.0)]
    timeout: f64,

    #[clap(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Subcommand, Debug)]
enum Cmd {
    /// Run the server.
    Server {
        #[clap(long, default_value_t = 1024)]
        backlog: u32,

        /// Turn-on the feeds immediately after start.
        #[clap(long, default_value_t = true)]
        on: bool,
    },

    /// Ask the server to turn-on the bar feeds.
    On,

    /// Ask the server to turn-off the bar feeds.
    Off,

    /// Ask the server for its current status.
    Status {
        /// Machine-friendly output - i.e. no spaces in table cells.
        #[clap(short, long, default_value_t = false)]
        machine: bool,
    },

    /// Ask the server to:
    /// (1) turn-off feeds
    /// (2) re-read config
    /// (3) turn-on feeds
    Reload,
}

impl Cli {
    #[tokio::main]
    #[tracing::instrument(name = "barista", skip_all)]
    async fn run(&self) -> anyhow::Result<()> {
        barista::tracing::init(self.log_level, self.debug)?;
        tracing::debug!(?self, "Running");

        let dir = expanduser::expanduser(&self.dir).context(format!(
            "Failed to expand tilde in path: {:?}",
            &self.dir
        ))?;
        fs::create_dir_all(&dir).await?;
        let dir = dir.canonicalize().context(format!(
            "Failed to canonicalize path: {:?}",
            &self.dir
        ))?;
        let timeout = Duration::from_secs_f64(self.timeout);

        if let Cmd::Server { backlog, on } = &self.cmd {
            // TODO Use timeout in the server?
            server(&dir, *backlog, *on).await
        } else {
            client(&self.cmd, &dir, timeout).await
        }
    }
}

#[tracing::instrument(skip_all)]
async fn server(dir: &Path, backlog: u32, on: bool) -> anyhow::Result<()> {
    tracing::info!(?dir, backlog, on, "Starting");
    let mut siblings = JoinSet::new();
    let bar_tx = barista::bar::server::start(&mut siblings, dir).await?;
    siblings.spawn(
        barista::control::server::run(
            dir.to_path_buf(),
            backlog,
            bar_tx.clone(),
        )
        .in_current_span(),
    );
    if on {
        barista::bar::server::on(&bar_tx).await?;
    }
    let mut sigterm = tokio::signal::unix::signal(
        tokio::signal::unix::SignalKind::terminate(),
    )?;
    let pid_file = dir.join(conf::SERVER_PID_FILE_NAME);
    fs::write(&pid_file, std::process::id().to_string()).await?;
    let result = tokio::select! {
        num_errors = join(&mut siblings) => {
            tracing::error!(num_errors, "Server workers exited.");
            // TODO Post notification.
            Err(anyhow!("Premature server exit"))
        },
        _ = tokio::signal::ctrl_c() => {
            tracing::warn!("Caught Ctrl+C. Shutting down.");
            Ok(())
        }
        _ = sigterm.recv() => {
            tracing::warn!("Caught SIGTERM. Shutting down.");
            Ok(())
        }
    };
    if let Err(error) = barista::bar::server::off(&bar_tx).await {
        tracing::error!(
            ?error,
            "Clean shutdown attempt failed - \
            orphaned processes might be remaining. \
            Attempting to kill PIDs found in feed PID files."
        );
        if let Err(error) = barista::bar::feed::try_kill_all(dir).await {
            tracing::error!(
                ?error,
                "Killing feed processes failed - \
                orphaned processes might still be remaining."
            );
        }
    }
    fs::remove_file(&pid_file).await.context(format!(
        "Failed to remove main PID file: {:?}",
        &pid_file
    ))?;
    result
}

async fn join(siblings: &mut JoinSet<anyhow::Result<()>>) -> usize {
    let mut errors = 0;
    while let Some(join_result) = siblings.join_next().await {
        match join_result {
            Ok(Ok(())) => unreachable!("A server worker exited normally."),
            Ok(Err(error)) => {
                errors += 1;
                tracing::error!(?error, "Worker failed.");
            }
            Err(join_error) if join_error.is_panic() => {
                errors += 1;
                tracing::error!(?join_error, "Worker paniced.");
            }
            Err(join_error) if join_error.is_cancelled() => {
                errors += 1;
                tracing::error!(?join_error, "Worker cancelled.");
            }
            Err(join_error) => {
                tracing::error!(
                    ?join_error,
                    "Worker failed to execute to completion, \
                    but neither paniced nor was cancelled.",
                );
                unreachable!(
                    "Worker failed to execute to completion, \
                    but neither paniced nor was cancelled.",
                );
            }
        }
        siblings.abort_all();
    }
    errors
}

#[tracing::instrument(skip_all)]
async fn client(
    cmd: &Cmd,
    dir: &Path,
    timeout: Duration,
) -> anyhow::Result<()> {
    tracing::debug!(?cmd, ?dir, ?timeout, "Starting");
    let client = barista::control::client::Client::new(&dir, timeout).await?;
    match cmd {
        Cmd::Server { .. } => {
            unreachable!("Server command passed to the client function.")
        }
        Cmd::On => client.on().await,
        Cmd::Off => client.off().await,
        Cmd::Status { machine } => client.status(*machine).await,
        Cmd::Reload => client.reload().await,
    }
}

fn main() -> anyhow::Result<()> {
    Cli::parse().run()
}
