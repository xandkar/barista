use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{anyhow, Context};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt},
    process::{self, Command},
    task::JoinHandle,
};
use tracing::{info_span, Instrument};

use crate::{bar, conf};

#[derive(Debug)]
pub struct Feed {
    pub name: String,
    proc: process::Child,
    pgid: nix::unistd::Pid,
    out: Option<JoinHandle<anyhow::Result<()>>>,
    err: Option<JoinHandle<anyhow::Result<()>>>,
}

impl Feed {
    pub async fn start(
        cfg: &conf::Feed,
        dir: &Path,
        pos: usize,
        dst: bar::server::ApiSender,
    ) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir).await.context(format!(
            "Failed to create all directories in path: {:?}",
            dir
        ))?;
        let shell = cfg.shell.clone().unwrap_or(conf::default_shell());
        let mut proc = Command::new(shell)
            .arg("-c") // FIXME Some shells may use a different argument flag?
            .arg(&cfg.cmd)
            .current_dir(dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0) // XXX Sets PGID to PID.
            .spawn()
            .context(format!(
                "Failed to spawn feed. Dir: {:?}. Feed: {:?}",
                dir, cfg,
            ))?;

        let pid = proc.id().ok_or(anyhow!(
            "Failed to get child process PID for feed: {:?}",
            cfg
        ))?;
        let pid = nix::unistd::Pid::from_raw(pid as i32);

        let stdout = proc.stdout.take().unwrap_or_else(|| {
            unreachable!("stdout not requested at process spawn.")
        });
        let stderr = proc.stderr.take().unwrap_or_else(|| {
            unreachable!("stderr not requested at process spawn.")
        });
        let feed_span = info_span!("feed", name = cfg.name);
        let out = tokio::spawn(
            route_out(stdout, pos, dst)
                .instrument(feed_span.clone())
                .in_current_span(),
        );
        let err = tokio::spawn(
            route_err(stderr, dir.join(conf::FEED_LOG_FILE_NAME))
                .instrument(feed_span.clone())
                .in_current_span(),
        );
        let selph = Self {
            name: cfg.name.to_string(),
            proc,
            pgid: pid, // XXX Assuming Command.process_group(0) was called.
            out: Some(out),
            err: Some(err),
        };
        Ok(selph)
    }

    #[tracing::instrument(name = "feed_stop", skip_all, fields(name = self.name))]
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        tracing::debug!("Stopping");

        nix::sys::signal::killpg(
            self.pgid,
            nix::sys::signal::Signal::SIGKILL,
        )
        .context(format!("Failed to kill process group for: {:?}", self))?;
        tracing::debug!("Process group killed.");

        self.proc.kill().await?;
        tracing::debug!("Child proc killed. Waiting for exit.");

        self.proc.wait().await?;
        tracing::debug!("Child proc exited.");

        self.out
            .take()
            .unwrap_or_else(|| unreachable!("Redundant feed stop attempt."))
            .await??;
        tracing::debug!("stdout router exited");

        self.err
            .take()
            .unwrap_or_else(|| unreachable!("Redundant feed stop attempt."))
            .await??;
        tracing::debug!("stderr router exited");

        tracing::info!("Stopped.");
        Ok(())
    }
}

#[tracing::instrument(name = "out", skip_all)]
async fn route_out(
    stdout: process::ChildStdout,
    pos: usize,
    dst_tx: bar::server::ApiSender,
) -> anyhow::Result<()> {
    tracing::info!("Starting.");
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await? {
        tracing::debug!(?line, "New");
        bar::server::input(&dst_tx, pos, line).await?;
    }
    tracing::debug!("Closed. Exiting.");
    tracing::debug!("Exiting.");
    Ok(())
}

#[tracing::instrument(name = "err", skip_all)]
async fn route_err(
    err: process::ChildStderr,
    dst: PathBuf,
) -> anyhow::Result<()> {
    tracing::info!("Starting.");
    async {
        let dst = dst.clone();
        let mut stderr_log = fs::OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(dst)
            .await?;
        let mut lines = tokio::io::BufReader::new(err).lines();
        while let Some(line) = lines.next_line().await? {
            tracing::debug!(?line, "New");
            stderr_log.write_all(line.as_bytes()).await?;
            stderr_log.write_all(&[b'\n']).await?;
            stderr_log.flush().await?;
        }
        tracing::debug!("Closed. Exiting.");
        Ok::<(), anyhow::Error>(())
    }
    .await
    .with_context(|| format!("route_err failed. dst: {:?}", &dst))
}
