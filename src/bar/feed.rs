use std::{
    io,
    path::{Path, PathBuf},
    process::Stdio,
    time::SystemTime,
};

use anyhow::{anyhow, bail, Context};
use tokio::{
    fs,
    io::AsyncBufReadExt,
    process::{self, Command},
    task::{spawn_blocking, JoinHandle},
};
use tokio_util::sync::CancellationToken;
use tracing::{info_span, Instrument};

use crate::{bar, conf};

#[derive(Debug)]
pub struct Feed {
    pos: usize,
    name: String,
    dir: PathBuf,
    log_file: PathBuf,
    pid_file: PathBuf,
    life: CancellationToken,
    pid: u32,
    pgid: nix::unistd::Pid,
    output_reader: Option<JoinHandle<anyhow::Result<()>>>,
    waiter_and_killer: Option<JoinHandle<anyhow::Result<()>>>,
    last_output: Option<SystemTime>,
}

impl Feed {
    pub fn get_name(&self) -> &str {
        self.name.as_str()
    }

    pub fn get_dir_path(&self) -> &Path {
        self.dir.as_path()
    }

    pub fn get_log_path(&self) -> &Path {
        self.log_file.as_path()
    }

    pub fn get_last_output_time(&self) -> Option<SystemTime> {
        self.last_output
    }

    pub fn get_pid(&self) -> u32 {
        self.pid
    }

    pub fn get_pgid(&self) -> u32 {
        self.pgid.as_raw().unsigned_abs()
    }

    pub fn set_last_output_time(&mut self) {
        self.last_output = Some(SystemTime::now())
    }

    pub async fn start(
        cfg: &conf::Feed,
        dir: &Path,
        pos: usize,
        dst: bar::server::ApiSender,
    ) -> anyhow::Result<Self> {
        let dir = dir.to_path_buf();
        fs::create_dir_all(&dir).await.context(format!(
            "Failed to create all directories in path: {:?}",
            &dir
        ))?;
        let log_file_path = dir.join(conf::FEED_LOG_FILE_NAME);
        let log_file: std::fs::File = {
            // XXX Can't use tokio::fs::File because std::process::Stdio::from
            //     can't work with it and tokio offers no analogue. Possible
            //     workarounds:
            //     a. use std inside spawn_blocking;
            //     b. use tokio and then unsafely convert to raw fd
            //        and then use Stdio::from_raw_fd.
            let log_file_path = log_file_path.clone();
            spawn_blocking(move || {
                std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(log_file_path)
            })
            .await??
        };
        let shell = cfg.shell.clone().unwrap_or(conf::default_shell());
        let mut child = Command::new(shell)
            .arg("-c") // FIXME Some shells may use a different argument flag?
            .arg(&cfg.cmd)
            .current_dir(&dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log_file))
            .process_group(0) // XXX Sets PGID to PID.
            .spawn()
            .context(format!(
                "Failed to spawn feed. Dir: {:?}. Feed: {:?}",
                &dir, cfg,
            ))?;

        let pid = child.id().ok_or(anyhow!(
            "Failed to get child process PID for feed: {:?}",
            cfg
        ))?;
        let pid_file = dir.join(conf::FEED_PID_FILE_NAME);
        fs::write(&pid_file, pid.to_string())
            .await
            .context(format!("Failed to write PID file: {:?}", &pid_file))?;

        // XXX Assuming Command.process_group(0) was called.
        let pgid = nix::unistd::Pid::from_raw(pid as i32);

        let stdout = child.stdout.take().unwrap_or_else(|| {
            unreachable!("stdout not requested at process spawn.")
        });
        let span = info_span!("feed", pos = pos + 1, name = cfg.name, pid);
        let output_reader = tokio::spawn(
            output_reader(stdout, pos, dst.clone())
                .instrument(span.clone())
                .in_current_span(),
        );
        let life = CancellationToken::new();
        let waiter_and_killer = tokio::spawn(
            waiter_and_killer(dst.clone(), life.clone(), pos, pgid, child)
                .instrument(span)
                .in_current_span(),
        );
        let selph = Self {
            pos,
            name: cfg.name.to_string(),
            dir,
            log_file: log_file_path,
            pid_file,
            life,
            pid,
            pgid,
            output_reader: Some(output_reader),
            waiter_and_killer: Some(waiter_and_killer),
            last_output: None,
        };
        Ok(selph)
    }

    #[tracing::instrument(
        name = "feed_stop",
        skip_all,
        fields(
            pos = self.pos + 1,
            name = self.name
        )
    )]
    pub fn stop(&self) {
        tracing::debug!("Stopping");
        self.life.cancel();
    }

    #[tracing::instrument(
        name = "feed_clean",
        skip_all,
        fields(
            pos = self.pos + 1,
            name = self.name
        )
    )]
    pub async fn clean_up(&mut self) -> anyhow::Result<()> {
        tracing::debug!("Starting.");
        self.waiter_and_killer
            .take()
            .unwrap_or_else(|| unreachable!("Redundant feed stop attempt."))
            .await??;
        self.output_reader
            .take()
            .unwrap_or_else(|| unreachable!("Redundant feed stop attempt."))
            .await??;
        fs::remove_file(self.pid_file.as_path()).await?;
        tracing::info!("Done.");
        Ok(())
    }
}

#[tracing::instrument(skip_all)]
async fn waiter_and_killer(
    dst_tx: bar::server::ApiSender,
    life: CancellationToken,
    pos: usize,
    pgid: nix::unistd::Pid,
    mut child: process::Child,
) -> anyhow::Result<()> {
    tracing::info!("Starting.");
    let result: io::Result<std::process::ExitStatus> = async {
        tokio::select! {
            _ = life.cancelled() => {
                nix::sys::signal::killpg(
                    pgid,
                    nix::sys::signal::Signal::SIGKILL
                ).map_err(|errno| {
                    let desc = errno.desc();
                    let errno = errno as i32;
                    let pgid = pgid.as_raw();
                    tracing::error!(
                        pos,
                        pgid,
                        errno,
                        desc,
                        "Failed to kill process group.",
                    );
                    io::Error::from_raw_os_error(errno)
                })?;
                tracing::debug!("Process group killed.");
                child.start_kill()?;
                child.wait().await
            }
            // XXX .wait() drops stdin, but we can first .take() it
            //     after .spawn() if/when we actually need it.
            result = child.wait() => {
                tracing::error!(?result, "Unsolicited feed process exit.");
                // TODO Post notification.
                // TODO Should we try to kill the process group here anyway?
                result
            }
        }
    }
    .await;
    if let Err(error) = bar::server::exit(&dst_tx, pos, result) {
        tracing::error!(
            ?error,
            "Failed to report feed exit back to the bar server."
        );
    }
    tracing::debug!("Exiting.");
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn output_reader(
    stdout: process::ChildStdout,
    pos: usize,
    dst_tx: bar::server::ApiSender,
) -> anyhow::Result<()> {
    tracing::info!("Starting.");
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await? {
        tracing::debug!(?line, "New");
        bar::server::input(&dst_tx, pos, line)?;
    }
    tracing::debug!("Exiting.");
    Ok(())
}

/// Try to find and kill all previously saved PIDs.
pub async fn try_kill_all(dir: &Path) -> anyhow::Result<()> {
    tracing::warn!(
        ?dir,
        "Attempting to find and kill PIDs in feed PID files."
    );
    let feeds_dir = dir.join(conf::FEEDS_DIR_NAME);
    let mut feeds_dir_entries = fs::read_dir(&feeds_dir).await?;
    let mut total: usize = 0;
    let mut failed: usize = 0;
    while let Some(entry) = feeds_dir_entries.next_entry().await? {
        total += 1;
        let path = entry.path();
        if let Err(error) = try_kill(entry).await {
            failed += 1;
            tracing::error!(
                ?error,
                ?path,
                "Failed to lookup and kill feed process group."
            )
        }
    }
    if failed > 0 {
        Err(anyhow!("{} out of {} kill attempts failed.", failed, total))
    } else {
        tracing::info!(
            "Killed all found process groups of previously started feeds."
        );
        Ok(())
    }
}

async fn try_kill(entry: fs::DirEntry) -> anyhow::Result<()> {
    let entry_path = entry.path();
    if !entry
        .file_type()
        .await
        .context(format!(
            "Failed to check directory entry file type. Path: {:?}",
            &entry_path
        ))?
        .is_dir()
    {
        bail!("Non-directory sub-entry: {:?}", &entry_path);
    }
    let pid_file = entry_path.join(conf::FEED_PID_FILE_NAME);
    if !fs::try_exists(&pid_file).await.context(format!(
        "Failed to check feed PID file existence: {:?}",
        &pid_file
    ))? {
        bail!("Feed PID file not found: {:?}", &pid_file);
    }
    tracing::warn!(path = ?pid_file, "Attempting to kill PID from feed PID file.");
    let pid = fs::read_to_string(&pid_file)
        .await
        .context(format!("Failed to read feed PID file: {:?}", &pid_file))?;
    let pid: u32 = pid
        .parse()
        .context(format!("Failed to parse feed PID file: {:?}", &pid_file))?;
    let pid = nix::unistd::Pid::from_raw(pid as i32);
    let pgrp = pid;
    nix::sys::signal::killpg(pgrp, nix::sys::signal::Signal::SIGKILL)
        .context(format!(
            "Failed to kill process group: {}. PID: {}. PID file: {:?}.",
            pgrp, pid, &pid_file
        ))?;
    fs::remove_file(&pid_file).await.context(format!(
        "Failed to remove feed PID file: {:?}",
        &pid_file
    ))?;
    Ok(())
}
