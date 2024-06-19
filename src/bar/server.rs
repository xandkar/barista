use std::{
    path::{Path, PathBuf},
    result,
    time::Duration,
};

use tokio::{
    fs,
    sync::{
        mpsc::{self, error::SendError, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    task::{JoinHandle, JoinSet},
};
use tracing::Instrument;

use crate::{
    bar::feed::Feed,
    conf::{self, Conf},
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
    Off(oneshot::Sender<anyhow::Result<()>>),
    Status(oneshot::Sender<anyhow::Result<()>>),
    Reload(oneshot::Sender<anyhow::Result<()>>),
    Expiration { pos: usize },
    Input { pos: usize, data: String },
    Output,
}

pub async fn on(api_tx: ApiSender) -> ApiResult<()> {
    let (reply_tx, reply_rx) = oneshot::channel();
    api_tx.send(Api {
        msg: Msg::On(reply_tx),
    })?;
    reply_rx.await??;
    Ok(())
}

pub async fn off(api_tx: ApiSender) -> ApiResult<()> {
    let (reply_tx, reply_rx) = oneshot::channel();
    api_tx.send(Api {
        msg: Msg::Off(reply_tx),
    })?;
    reply_rx.await??;
    Ok(())
}

pub async fn status(api_tx: ApiSender) -> ApiResult<()> {
    let (reply_tx, reply_rx) = oneshot::channel();
    api_tx.send(Api {
        msg: Msg::Status(reply_tx),
    })?;
    reply_rx.await??;
    Ok(())
}

pub async fn reload(api_tx: ApiSender) -> ApiResult<()> {
    let (reply_tx, reply_rx) = oneshot::channel();
    api_tx.send(Api {
        msg: Msg::Reload(reply_tx),
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

// TODO State enum.
struct Server {
    self_tx: ApiSender,
    dir: PathBuf,
    conf: Conf,
    bar: Bar,
    feeds: Vec<Feed>,
    expiration_timers: Vec<Option<JoinHandle<()>>>,
    output_interval: Duration,
    output_timer: Option<JoinHandle<()>>,
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

    async fn output(&mut self) -> anyhow::Result<()> {
        if let Some(data) = self.bar.show_unshown() {
            self.output_data(&data).await?;
        }
        Ok(())
    }

    async fn output_blank(&mut self) -> anyhow::Result<()> {
        self.output_data("").await
    }

    async fn output_data(&mut self, data: &str) -> anyhow::Result<()> {
        match &self.conf.dst {
            conf::Dst::StdOut => println!("{}", &data),
            conf::Dst::StdErr => eprintln!("{}", &data),
            conf::Dst::File { path } => fs::write(path, data).await?,
            conf::Dst::X11RootWindowName => {
                if self.x11.is_none() {
                    self.x11 = Some(X11::init()?);
                }
                let x11 = self.x11.take().unwrap_or_else(|| {
                    unreachable!(
                        "x11 failure shoudl have caused a return above."
                    );
                });
                x11.set_root_window_name(&data)?;
                self.x11.replace(x11);
            }
        }
        Ok(())
    }

    async fn on(&mut self) -> anyhow::Result<()> {
        self.off().await?;
        self.bar = Bar::from_conf(&self.conf);
        self.feeds = Vec::new();
        self.expiration_timers = Vec::new();
        let conf = self.conf.clone();
        for (pos, feed_cfg) in conf.feeds.iter().enumerate() {
            let feed_dir = self
                .dir
                .join(conf::FEEDS_DIR_NAME)
                .join(format!("{:03}-{}", pos, &feed_cfg.name));
            let feed_proc =
                Feed::start(feed_cfg, &feed_dir, pos, self.self_tx.clone())
                    .await?;
            self.feeds.push(feed_proc);
            self.expiration_timers.push(None);
            self.reschedule_expiration(pos);
            self.ensure_output_scheduled();
        }
        Ok(())
    }

    async fn off(&mut self) -> anyhow::Result<()> {
        self.bar.clear_all();
        for mut feed in self.feeds.drain(0..) {
            feed.stop().await?;
        }
        for timer_opt in self.expiration_timers.drain(0..) {
            timer_opt.map(|timer| timer.abort());
        }
        self.output_timer.take().map(|timer| timer.abort());
        self.output_blank().await?;
        self.x11.take();
        Ok(())
    }

    async fn reload(&mut self) -> anyhow::Result<()> {
        self.off().await?;
        self.conf = Conf::load_or_init(&self.dir).await?;
        self.on().await?;
        Ok(())
    }

    async fn status(&mut self) -> anyhow::Result<()> {
        todo!("status")
    }

    async fn handle(&mut self, msg: Msg) -> anyhow::Result<()> {
        tracing::debug!(?msg, "Handling message.");
        match msg {
            Msg::Expiration { pos } => {
                self.expiration_timers[pos]
                    .take()
                    .unwrap_or_else(|| unreachable!())
                    .await?;
                self.bar.expire(pos);
                self.ensure_output_scheduled();
            }
            Msg::Input { pos, data } => {
                self.reschedule_expiration(pos);
                self.bar.set(pos, &data);
                self.ensure_output_scheduled();
            }
            Msg::Output => {
                self.output_timer.take().unwrap_or_else(|| {
                    unreachable!(
                        "Output msg arrived without being scheduled."
                    )
                });
                if let Err(error) = self.output().await {
                    tracing::error!(?error, "Failed to output.");
                }
            }
            Msg::On(reply_tx) => {
                let result = self.on().await;
                reply_tx.send(result).unwrap_or_else(|error| {
                    tracing::error!(
                        ?error,
                        "Failed to reply. Sender dropped."
                    )
                })
            }
            Msg::Off(reply_tx) => {
                let result = self.off().await;
                reply_tx.send(result).unwrap_or_else(|error| {
                    tracing::error!(
                        ?error,
                        "Failed to reply. Sender dropped."
                    )
                })
            }
            Msg::Status(reply_tx) => {
                let result = self.status().await;
                reply_tx.send(result).unwrap_or_else(|error| {
                    tracing::error!(
                        ?error,
                        "Failed to reply. Sender dropped."
                    )
                })
            }
            Msg::Reload(reply_tx) => {
                let result = self.reload().await;
                reply_tx.send(result).unwrap_or_else(|error| {
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

pub async fn run(
    tx: ApiSender,
    mut rx: ApiReceiver,
    dir: PathBuf,
    conf: Conf,
) -> anyhow::Result<()> {
    tracing::info!(?conf, "Starting server");
    let mut server = Server::new(conf, dir, tx);
    while let Some(Api { msg }) = rx.recv().await {
        server.handle(msg).await?;
    }
    Ok(())
}
