use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    result,
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::anyhow;
use tokio::{
    fs,
    sync::{
        mpsc::{self, error::SendError, UnboundedReceiver, UnboundedSender},
        oneshot, Notify,
    },
    task::{JoinHandle, JoinSet},
};
use tracing::Instrument;

use crate::{
    bar::{self, feed::Feed},
    conf::{self, Conf},
    ps,
    x11::X11,
};

use super::Bar;

pub type ApiSender = UnboundedSender<Api>;
pub type ApiReceiver = UnboundedReceiver<Api>;
pub type ApiResult<T> = result::Result<T, ApiError>;

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("Bar server operation failed: {0:?}")]
    OpFailed(#[from] anyhow::Error),

    #[error("Bar server is dead")]
    Dead(#[from] tokio::sync::mpsc::error::SendError<Api>),

    // TODO When else can this happen?
    #[error("Bar server exited before replying")]
    Crashed(#[from] oneshot::error::RecvError),
}

#[derive(Debug)]
pub struct Api {
    msg: Msg,
}

#[derive(Debug)]
enum Msg {
    On(oneshot::Sender<anyhow::Result<()>>),
    Off(oneshot::Sender<()>),
    Status(oneshot::Sender<anyhow::Result<bar::status::Status>>),
    Reconf(oneshot::Sender<anyhow::Result<()>>),
    FeedExit {
        pos: usize,
        result: io::Result<std::process::ExitStatus>,
    },
    Expiration {
        pos: usize,
    },
    Input {
        pos: usize,
        data: String,
    },
    Output,
}

pub async fn on(api_tx: &ApiSender) -> ApiResult<()> {
    let (reply_tx, reply_rx) = oneshot::channel();
    api_tx.send(Api {
        msg: Msg::On(reply_tx),
    })?;
    reply_rx.await??;
    Ok(())
}

pub async fn off(api_tx: &ApiSender) -> ApiResult<()> {
    let (reply_tx, reply_rx) = oneshot::channel();
    api_tx.send(Api {
        msg: Msg::Off(reply_tx),
    })?;
    reply_rx.await?;
    Ok(())
}

pub async fn status(api_tx: &ApiSender) -> ApiResult<bar::status::Status> {
    let (reply_tx, reply_rx) = oneshot::channel();
    api_tx.send(Api {
        msg: Msg::Status(reply_tx),
    })?;
    let status = reply_rx.await??;
    Ok(status)
}

pub async fn reload(api_tx: &ApiSender) -> ApiResult<()> {
    off(api_tx).await?;
    reconf(api_tx).await?;
    on(api_tx).await?;
    Ok(())
}

async fn reconf(api_tx: &ApiSender) -> ApiResult<()> {
    let (reply_tx, reply_rx) = oneshot::channel();
    api_tx.send(Api {
        msg: Msg::Reconf(reply_tx),
    })?;
    reply_rx.await??;
    Ok(())
}

pub async fn input(
    api_tx: &ApiSender,
    pos: usize,
    data: String,
) -> ApiResult<()> {
    api_tx.send(Api {
        msg: Msg::Input { pos, data },
    })?;
    Ok(())
}

pub async fn exit(
    api_tx: &ApiSender,
    pos: usize,
    result: io::Result<std::process::ExitStatus>,
) -> ApiResult<()> {
    api_tx.send(Api {
        msg: Msg::FeedExit { pos, result },
    })?;
    Ok(())
}

pub async fn start(
    siblings: &mut JoinSet<anyhow::Result<()>>,
    dir: &Path,
) -> anyhow::Result<ApiSender> {
    let conf = Conf::load_or_init(dir).await?;
    let (tx, rx) = mpsc::unbounded_channel();
    siblings.spawn(
        run(tx.clone(), rx, dir.to_path_buf(), conf).in_current_span(),
    );
    Ok(tx)
}

// TODO Move data fields from Server to appropriate State variants.
#[derive(Debug)]
enum State {
    On,
    Offing { notify: Arc<Notify> },
    Off,
}

struct Server {
    self_tx: ApiSender,
    dir: PathBuf,
    conf: Conf,
    state: State,
    bar: Bar,
    feeds: Vec<Option<Feed>>,
    expiration_timers: Vec<Option<JoinHandle<()>>>,
    output_timer: Option<JoinHandle<()>>,
    output_interval: Duration,
    x11: Option<X11>,
}

impl Server {
    fn new(conf: Conf, dir: PathBuf, self_tx: ApiSender) -> Self {
        let bar = Bar::from_conf(&conf);
        let output_interval = Duration::from_secs_f64(conf.output_interval);
        let mut selph = Self {
            self_tx,
            dir,
            conf,
            state: State::Off,
            bar,
            feeds: Vec::new(),
            expiration_timers: Vec::new(),
            output_interval,
            output_timer: None,
            x11: None,
        };
        selph.ensure_output_scheduled();
        selph
    }

    async fn output(&mut self) {
        if let Some(data) = self.bar.show_unshown() {
            self.output_data(&data).await
        }
    }

    async fn output_blank(&mut self) {
        self.output_data("").await
    }

    async fn output_data(&mut self, data: &str) {
        let result: anyhow::Result<()> = async {
            match self.conf.get_dst() {
                conf::Dst::StdOut => println!("{}", &data),
                conf::Dst::StdErr => eprintln!("{}", &data),
                conf::Dst::File { path } => fs::write(path, data).await?,
                conf::Dst::X11RootWindowName => {
                    if self.x11.is_none() {
                        self.x11 = Some(X11::init()?);
                    }
                    let x11 = self.x11.take().unwrap_or_else(|| {
                        unreachable!(
                            "X11 failure should have caused a return above."
                        );
                    });
                    x11.set_root_window_name(&data)?;
                    self.x11.replace(x11);
                }
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            tracing::error!(?error, "Output failed");
            // TODO Post notification.
        }
    }

    async fn on(&mut self) -> anyhow::Result<()> {
        self.bar = Bar::from_conf(&self.conf);
        self.feeds = Vec::new();
        self.expiration_timers = Vec::new();
        let conf = self.conf.clone();
        for (pos, feed_cfg) in conf.feeds.iter().enumerate() {
            let feed_dir = self
                .dir
                .join(conf::FEEDS_DIR_NAME)
                .join(format!("{:02}-{}", pos, &feed_cfg.name));
            let feed =
                Feed::start(feed_cfg, &feed_dir, pos, self.self_tx.clone())
                    .await?;
            self.feeds.push(Some(feed));
            self.expiration_timers.push(None);
            self.reschedule_expiration(pos);
            self.ensure_output_scheduled();
        }
        self.state = State::On;
        Ok(())
    }

    fn off_begin(&mut self) -> Arc<Notify> {
        tracing::info!("Shutdown begin.");
        for feed in self.feeds.iter().filter_map(|x| x.as_ref()) {
            feed.stop();
        }
        let notify = Arc::new(Notify::new());
        self.state = State::Offing {
            notify: notify.clone(),
        };
        notify
    }

    async fn off_feed(
        &mut self,
        pos: usize,
        result: io::Result<std::process::ExitStatus>,
    ) -> anyhow::Result<()> {
        let mut feed = self.feeds[pos].take().unwrap_or_else(|| {
            unreachable!(
                "Feed exited more than once. pos={}. result={:?}",
                pos, result
            )
        });
        let name = feed.get_name();
        match result {
            Err(error) => {
                tracing::error!(pos, name, ?error, "Feed stop failure.");
                // TODO Post notification.
            }
            Ok(exit_status) => {
                tracing::info!(pos, name, ?exit_status, "Feed stop success.");
            }
        }
        feed.clean_up().await?;
        self.bar.clear(pos);
        self.output().await;
        let num_feeds_still_running =
            self.feeds.iter().filter(|x| x.is_some()).count();
        match &self.state {
            State::Offing { notify } if num_feeds_still_running == 0 => {
                for timer_opt in self.expiration_timers.drain(0..) {
                    timer_opt.map(|timer| timer.abort());
                }
                self.output_timer.take().map(|timer| timer.abort());
                self.x11.take();
                notify.notify_waiters();
                self.output_blank().await;
                self.state = State::Off;
            }
            _ => (),
        }
        Ok(())
    }

    async fn status(&mut self) -> anyhow::Result<bar::status::Status> {
        let status = match (&self.feeds[..], &self.expiration_timers[..]) {
            ([], []) => bar::status::Status::UpOff,
            (procs, _) => {
                let ps_list = ps::list().await?;
                let mut pdescendants = ps::descendants(ps_list.as_slice());
                let mut states = ps::states(ps_list.as_slice());
                let mut stati = Vec::new();
                for (pos, cfg) in self.conf.feeds.iter().enumerate() {
                    let info = match &procs[pos] {
                        None => None,
                        Some(feed) => {
                            let log_file = feed.get_log_path();
                            let log_mtime =
                                crate::fs::mtime(&log_file).await?;
                            let log_size_bytes =
                                crate::fs::size_in_bytes(&log_file).await?;
                            let now = SystemTime::now();
                            let age_of_output = feed
                                .get_last_output_time()
                                .and_then(|last| {
                                    now.duration_since(last)
                                        .map_err(|error| {
                                            tracing::warn!(
                                        ?error,
                                        "Last output is from the future. \
                                         This far away: {}",
                                        humantime::format_duration(
                                            error.duration()
                                        )
                                    );
                                            // TODO Post notification.
                                        })
                                        .ok()
                                });
                            let age_of_log = (log_size_bytes > 0)
                                .then(|| {
                                    now.duration_since(log_mtime)
                                    .map_err(|error| {
                                        tracing::warn!(
                                            ?error,
                                            "Log was modified in the future. \
                                             This far away: {}",
                                            humantime::format_duration(error.duration())
                                        );
                                        // TODO Post notification.
                                    })
                                    .ok()
                                })
                                .flatten();
                            let log_lines =
                                match fs::read_to_string(&log_file).await {
                                    Ok(log) => log
                                        .lines()
                                        .map(|line| line.to_string())
                                        .count(),
                                    Err(err) => {
                                        tracing::error!(
                                            ?log_file,
                                            ?err,
                                            "Failed to read log file",
                                        );
                                        // TODO Post notification.
                                        0
                                    }
                                };

                            // Removing to reuse existing set allocation,
                            // since we'll never look it up more than once
                            // anyway.
                            let pdescendants: HashSet<ps::Proc> =
                                pdescendants
                                    .remove(&feed.get_pid())
                                    .unwrap_or_default();
                            let state: Option<ps::State> =
                                states.remove(&feed.get_pid());

                            Some(bar::status::Info {
                                name: cfg.name.to_string(),
                                dir: feed.get_dir_path().to_owned(),
                                age_of_output,
                                age_of_log,
                                log_size_bytes,
                                log_lines,
                                pid: feed.get_pid(),
                                state,
                                pdescendants,
                            })
                        }
                    };
                    stati.push(bar::status::Feed {
                        position: pos + 1,
                        info,
                    });
                }
                bar::status::Status::UpOn { feeds: stati }
            }
        };
        Ok(status)
    }

    async fn handle(&mut self, msg: Msg) -> anyhow::Result<()> {
        tracing::debug!(?msg, "Handling message.");
        match (&self.state, msg) {
            (State::Offing { .. }, Msg::FeedExit { pos, result }) => {
                self.off_feed(pos, result).await?;
            }
            (_, Msg::FeedExit { pos, result }) => {
                tracing::warn!(pos, ?result, "Unsolicited feed exit.");
                self.off_feed(pos, result).await?;
            }
            (
                State::Off,
                msg @ (Msg::Expiration { pos: _ }
                | Msg::Input { pos: _, data: _ }
                | Msg::Output),
            ) => {
                tracing::warn!(?msg, "Ignoring in off state.");
            }
            (State::On | State::Offing { .. }, Msg::Expiration { pos }) => {
                self.expiration_timers[pos]
                    .take()
                    .unwrap_or_else(|| unreachable!())
                    .await?;
                self.bar.expire(pos);
                self.ensure_output_scheduled();
            }
            (
                State::On | State::Offing { notify: _ },
                Msg::Input { pos, data },
            ) => {
                self.reschedule_expiration(pos);
                self.bar.set(pos, &data);
                self.ensure_output_scheduled();
                if let Some(feed) = self.feeds[pos].as_mut() {
                    feed.set_last_output_time();
                }
            }
            (State::On | State::Offing { .. }, Msg::Output) => {
                self.output_timer.take().unwrap_or_else(|| {
                    unreachable!(
                        "Output msg arrived without being scheduled."
                    )
                });
                self.output().await
            }
            (State::On, Msg::On(reply_tx)) => {
                tracing::warn!("Already on. Ignoring request to turn on.");
                // TODO Let client know we're already on?
                reply_tx.send(Ok(())).unwrap_or_else(|error| {
                    tracing::error!(
                        ?error,
                        "Failed to reply. Sender dropped."
                    )
                })
            }
            (State::Offing { .. }, Msg::On(reply_tx)) => {
                tracing::warn!("Still offing. Ignoring request to turn on.");
                reply_tx
                    .send(Err(anyhow!(
                        "Still offing. Not ready to turn back on."
                    )))
                    .unwrap_or_else(|error| {
                        tracing::error!(
                            ?error,
                            "Failed to reply. Sender dropped."
                        )
                    })
            }
            (State::Off, Msg::On(reply_tx)) => {
                let result = self.on().await;
                reply_tx.send(result).unwrap_or_else(|error| {
                    tracing::error!(
                        ?error,
                        "Failed to reply. Sender dropped."
                    )
                })
            }
            (State::On, Msg::Off(reply_tx)) => {
                let notify = self.off_begin();
                tokio::spawn(async move {
                    notify.notified().await;
                    reply_tx.send(()).unwrap_or_else(|error| {
                        tracing::error!(
                            ?error,
                            "Failed to reply. Sender dropped."
                        )
                    });
                });
            }
            (State::Off | State::Offing { .. }, Msg::Off(reply_tx)) => {
                tracing::warn!(
                    "Already off or offing. Ignoring request to turn off."
                );
                // TODO Let client know we're already off?
                reply_tx.send(()).unwrap_or_else(|error| {
                    tracing::error!(
                        ?error,
                        "Failed to reply. Sender dropped."
                    )
                })
            }
            (_, Msg::Status(reply_tx)) => {
                let result = self.status().await;
                reply_tx.send(result).unwrap_or_else(|error| {
                    tracing::error!(
                        ?error,
                        "Failed to reply. Sender dropped."
                    )
                })
            }
            (State::Off, Msg::Reconf(reply_tx)) => {
                let result =
                    Conf::load_or_init(&self.dir).await.map(|conf| {
                        self.conf = conf;
                    });
                reply_tx.send(result).unwrap_or_else(|error| {
                    tracing::error!(
                        ?error,
                        "Failed to reply. Sender dropped."
                    )
                })
            }
            (State::On | State::Offing { .. }, Msg::Reconf(reply_tx)) => {
                reply_tx
                    .send(Err(anyhow!("Can only reconfig in off state.")))
                    .unwrap_or_else(|error| {
                        tracing::error!(
                            ?error,
                            "Failed to reply. Sender dropped."
                        )
                    })
            }
        }
        Ok(())
    }

    fn ensure_output_scheduled(&mut self) {
        if self.output_timer.is_none() {
            let output_timer =
                self.schedule(Msg::Output, self.output_interval);
            self.output_timer = Some(output_timer);
        }
    }

    fn reschedule_expiration(&mut self, pos: usize) {
        if let Some(ttl) = self.conf.feeds[pos].ttl {
            let ttl = Duration::from_secs_f64(ttl);
            let new = self.schedule(Msg::Expiration { pos }, ttl);
            self.expiration_timers[pos]
                .replace(new)
                .map(|old| old.abort());
        }
    }

    fn schedule(&self, msg: Msg, delay: Duration) -> JoinHandle<()> {
        let tx = self.self_tx.clone();
        tokio::spawn(
            async move {
                tokio::time::sleep(delay).await;
                if let Err(SendError(msg)) = tx.send(Api { msg }) {
                    tracing::warn!(
                        ?msg,
                        "Self-scheduled msg activated after worker's exit."
                    );
                }
            }
            .in_current_span(),
        )
    }
}

#[tracing::instrument(name = "bar", skip_all)]
pub async fn run(
    tx: ApiSender,
    mut rx: ApiReceiver,
    dir: PathBuf,
    conf: Conf,
) -> anyhow::Result<()> {
    tracing::info!("Starting");
    tracing::debug!("Initial conf: {:#?}", conf);
    let mut server = Server::new(conf, dir, tx);
    while let Some(Api { msg }) = rx.recv().await {
        server.handle(msg).await?;
    }
    Ok(())
}
