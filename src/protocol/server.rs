use std::{future::Future, path::Path};

use futures_util::StreamExt;
use tarpc::{
    context,
    server::{BaseChannel, Channel},
    tokio_serde::formats::Bincode,
    tokio_util::codec::LengthDelimitedCodec,
};
use tokio::{fs, net::UnixSocket};

use crate::protocol::{self, Bar};

#[derive(Clone)]
struct Server;

impl protocol::Bar for Server {
    #[tracing::instrument(skip(self))]
    async fn start(self, _: context::Context) -> protocol::Result<()> {
        tracing::debug!("Received start req.");
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn stop(self, _: context::Context) -> protocol::Result<()> {
        tracing::debug!("Received stop req.");
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn status(self, _: context::Context) -> protocol::Result<()> {
        tracing::debug!("Received status req.");
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn reload(self, _: context::Context) -> protocol::Result<()> {
        tracing::debug!("Received reload req.");
        Ok(())
    }
}

pub async fn run(sock_path: &Path, backlog: u32) -> anyhow::Result<()> {
    if let Err(error) = fs::remove_file(sock_path).await {
        tracing::warn!(
            ?sock_path,
            ?error,
            "Failed to remove existing sock file."
        );
    }
    let socket = UnixSocket::new_stream()?;
    socket.bind(sock_path)?;
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
            .execute(Server.serve())
            .for_each(spawn);
        tokio::spawn(fut);
    }
}

async fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
    tokio::spawn(fut);
}
