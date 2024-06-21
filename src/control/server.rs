use std::{future::Future, path::PathBuf};

use futures_util::StreamExt;
use tarpc::{
    context,
    server::{BaseChannel, Channel},
    tokio_serde::formats::Bincode,
    tokio_util::codec::LengthDelimitedCodec,
};
use tokio::{fs, net::UnixSocket};
use tracing::Instrument;

use crate::{
    bar, conf,
    control::{self, BarCtl},
};

#[derive(Clone)]
struct BarCtlServer {
    bar_tx: bar::server::ApiSender,
}

impl control::BarCtl for BarCtlServer {
    #[tracing::instrument(skip_all)]
    async fn on(self, _: context::Context) -> control::Result<()> {
        tracing::debug!("Received start req.");
        bar::server::on(self.bar_tx).await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn off(self, _: context::Context) -> control::Result<()> {
        tracing::debug!("Received stop req.");
        bar::server::off(self.bar_tx).await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn status(
        self,
        _: context::Context,
    ) -> control::Result<bar::status::Status> {
        tracing::debug!("Received status req.");
        let status = bar::server::status(self.bar_tx).await?;
        Ok(status)
    }

    #[tracing::instrument(skip_all)]
    async fn reload(self, _: context::Context) -> control::Result<()> {
        tracing::debug!("Received reload req.");
        bar::server::reload(self.bar_tx).await?;
        Ok(())
    }
}

#[tracing::instrument(name = "control", skip_all)]
pub async fn run(
    dir: PathBuf,
    backlog: u32,
    bar_tx: bar::server::ApiSender,
) -> anyhow::Result<()> {
    let sock_file = conf::sock_file(&dir);
    if let Err(error) = fs::remove_file(&sock_file).await {
        tracing::warn!(
            ?sock_file,
            ?error,
            "Failed to remove existing sock file."
        );
    }
    let bar_ctl_srv = BarCtlServer { bar_tx };
    let socket = UnixSocket::new_stream()?;
    socket.bind(&sock_file)?;
    let listener = socket.listen(backlog)?;
    let codec_builder = LengthDelimitedCodec::builder();
    loop {
        tracing::debug!("Waiting ...");
        let (conn, _addr) = match listener.accept().await {
            Ok((conn, addr)) => {
                tracing::debug!(from = ?addr, "Accepted");
                (conn, addr)
            }
            Err(error) => {
                tracing::error!(?error, "Error accepting connection");
                continue;
            }
        };
        let framed = codec_builder.new_framed(conn);
        let transport =
            tarpc::serde_transport::new(framed, Bincode::default());

        let fut = BaseChannel::with_defaults(transport)
            .execute(bar_ctl_srv.clone().serve())
            .for_each(spawn);
        tokio::spawn(fut.in_current_span());
    }
}

async fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
    tokio::spawn(fut);
}
