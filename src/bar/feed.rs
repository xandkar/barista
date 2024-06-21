use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::SystemTime,
};

use anyhow::{anyhow, Context};
use tokio::{
    fs,
    io::AsyncBufReadExt,
    process::{self, Command},
    task::{spawn_blocking, JoinHandle},
};
use tracing::{info_span, Instrument};

use crate::{bar, conf};

#[derive(Debug)]
pub struct Feed {
    name: String,
    dir: PathBuf,
    log: PathBuf,
    proc: process::Child,
    pid: nix::unistd::Pid,
    pgid: nix::unistd::Pid,
    out: Option<JoinHandle<anyhow::Result<()>>>,
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
        self.log.as_path()
    }

    pub fn get_last_output_time(&self) -> Option<SystemTime> {
        self.last_output
    }

    pub fn get_pid(&self) -> u32 {
        self.pid.as_raw().unsigned_abs()
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
                    .write(true)
                    .append(true)
                    .create(true)
                    .open(&log_file_path)
            })
            .await??
        };
        let shell = cfg.shell.clone().unwrap_or(conf::default_shell());
        let mut proc = Command::new(shell)
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

        let pid = proc.id().ok_or(anyhow!(
            "Failed to get child process PID for feed: {:?}",
            cfg
        ))?;
        let pid = nix::unistd::Pid::from_raw(pid as i32);

        let stdout = proc.stdout.take().unwrap_or_else(|| {
            unreachable!("stdout not requested at process spawn.")
        });
        let feed_span = info_span!("feed", name = cfg.name);
        let out = tokio::spawn(
            route_out(stdout, pos, dst)
                .instrument(feed_span.clone())
                .in_current_span(),
        );
        let selph = Self {
            name: cfg.name.to_string(),
            dir,
            log: log_file_path,
            proc,
            pid,
            pgid: pid, // XXX Assuming Command.process_group(0) was called.
            out: Some(out),
            last_output: None,
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
