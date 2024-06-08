use std::{
    path::Path,
    time::{Duration, SystemTime},
};

use anyhow::anyhow;
use tarpc::{
    tokio_serde::formats::Bincode, tokio_util::codec::LengthDelimitedCodec,
};
use tokio::net::UnixStream;

use crate::protocol;

pub struct Client {
    client: protocol::BarClient,
    ctx: tarpc::context::Context,
}

impl Client {
    pub async fn new(
        sock_file: &Path,
        timeout: Duration,
    ) -> anyhow::Result<Self> {
        let conn = UnixStream::connect(sock_file).await?;
        let codec_builder = LengthDelimitedCodec::builder();
        let transport = tarpc::serde_transport::new(
            codec_builder.new_framed(conn),
            Bincode::default(),
        );
        let client = protocol::BarClient::new(
            tarpc::client::Config::default(),
            transport,
        )
        .spawn();
        let mut ctx = tarpc::context::current();
        ctx.deadline = SystemTime::now()
            .checked_add(timeout)
            .ok_or(anyhow!("Bad timeout value"))?;
        let selph = Self { client, ctx };
        Ok(selph)
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.client.start(self.ctx).await??;
        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        self.client.stop(self.ctx).await??;
        Ok(())
    }

    pub async fn status(&self) -> anyhow::Result<()> {
        self.client.status(self.ctx).await??;
        Ok(())
    }

    pub async fn reload(&self) -> anyhow::Result<()> {
        self.client.reload(self.ctx).await??;
        Ok(())
    }
}
