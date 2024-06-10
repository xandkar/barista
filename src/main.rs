use std::time::Duration;

use clap::Parser;

use tokio::fs;
use tracing::{debug_span, Instrument};

#[derive(Parser, Debug)]
struct Cli {
    #[clap(short, long, default_value = "~/.barista/sock")]
    sock_file: String,

    #[clap(short, long, default_value_t = tracing::Level::DEBUG)]
    log_level: tracing::Level,

    #[clap(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Subcommand, Debug)]
enum Cmd {
    /// Run the server.
    Server {
        #[clap(long, default_value_t = 1024)]
        backlog: u32,
    },

    /// Ask the running server to start the feeds.
    Start,

    /// Ask the running server to stop the feeds.
    Stop,

    /// Ask the running server for its current status.
    Status,

    /// Ask the running server to stop, reload config and start.
    Reload,
}

impl Cli {
    #[tokio::main]
    #[tracing::instrument(skip_all)]
    async fn run(&self) -> anyhow::Result<()> {
        tracing::info!(?self, "Starting");

        let sock_file = expanduser::expanduser(&self.sock_file)?;
        if let Some(sock_dir) = sock_file.parent() {
            tracing::debug!(
                dir = ?sock_dir,
                file = ?sock_file,
                "Creating parent directory for the socket file."
            );
            fs::create_dir_all(sock_dir).await?;
        }

        if let Cmd::Server { backlog } = self.cmd {
            barista::control::server::run(&sock_file, backlog)
                .instrument(debug_span!("server"))
                .await
        } else {
            let timeout = Duration::from_secs(5);
            let client =
                barista::control::client::Client::new(&sock_file, timeout)
                    .await?;
            async {
                match &self.cmd {
                    Cmd::Server { .. } => unreachable!(),
                    Cmd::Start => client.start().await,
                    Cmd::Stop => client.stop().await,
                    Cmd::Status => client.status().await,
                    Cmd::Reload => client.reload().await,
                }
            }
            .instrument(debug_span!("client"))
            .await
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    barista::tracing::init(cli.log_level)?;
    cli.run()
}
