use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::Context;
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt},
    process::{self, Command},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{info_span, Instrument};

use crate::{bar, conf};

#[derive(Debug)]
pub struct Feed {
    proc: process::Child,
    life: CancellationToken,
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
            .spawn()
            .context(format!(
                "Failed to spawn feed. Dir: {:?}. Feed: {:?}",
                dir, cfg,
            ))?;
        let stdout = proc.stdout.take().unwrap_or_else(|| {
            unreachable!("stdout not requested at process spawn.")
        });
        let stderr = proc.stderr.take().unwrap_or_else(|| {
            unreachable!("stderr not requested at process spawn.")
        });
        let life = CancellationToken::new();
        let feed_span = info_span!("feed", name = cfg.name);
        let out = tokio::spawn(
            route_out(stdout, pos, dst, life.clone())
                .instrument(feed_span.clone())
                .in_current_span(),
        );
        let err = tokio::spawn(
            route_err(
                stderr,
                dir.join(conf::FEED_LOG_FILE_NAME),
                life.clone(),
            )
            .instrument(feed_span.clone())
            .in_current_span(),
        );
        let selph = Self {
            proc,
            life,
            out: Some(out),
            err: Some(err),
        };
        Ok(selph)
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        self.proc.kill().await?;

        // TODO Does stdio exit automatically on kill?
        // TODO Should still work without cancellation token?
        self.life.cancel();

        self.out
            .take()
            .unwrap_or_else(|| unreachable!("Redundant feed stop attempt."))
            .await??;
        self.err
            .take()
            .unwrap_or_else(|| unreachable!("Redundant feed stop attempt."))
            .await??;
        Ok(())
    }
}

#[tracing::instrument(name = "out", skip_all)]
async fn route_out(
    stdout: process::ChildStdout,
    pos: usize,
    dst_tx: bar::server::ApiSender,
    life: CancellationToken,
) -> anyhow::Result<()> {
    tracing::info!("Starting.");
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    loop {
        tokio::select! {
            _ = life.cancelled() => {
                tracing::warn!("Cancelled.");
                break;
            }
            line_opt_res = lines.next_line() => {
                let line_opt = line_opt_res?;
                match line_opt {
                    None => {
                        tracing::warn!("stderr closed");
                        break;
                    },
                    Some(line) => {
                        tracing::debug!(?line, "New");
                        bar::server::input(&dst_tx, pos, line).await?;
                    }
                }
            }
        }
    }
    tracing::warn!("Exiting.");
    Ok(())
}

#[tracing::instrument(name = "err", skip_all)]
async fn route_err(
    err: process::ChildStderr,
    dst: PathBuf,
    life: CancellationToken,
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
        loop {
            tokio::select! {
                _ = life.cancelled() => {
                    tracing::warn!("Cancelled");
                    break;
                }
                line_opt_res = lines.next_line() => {
                    let line_opt = line_opt_res?;
                    match line_opt {
                        None => {
                            tracing::warn!("stderr closed");
                            break;
                        },
                        Some(line) => {
                            tracing::debug!(?line, "New");
                            stderr_log.write_all(line.as_bytes()).await?;
                            stderr_log.write_all(&[b'\n']).await?;
                            stderr_log.flush().await?;
                        }
                    }
                }
            }
        }
        tracing::warn!("Exiting");
        Ok::<(), anyhow::Error>(())
    }
    .await
    .with_context(|| format!("route_err failed. dst: {:?}", &dst))
}
