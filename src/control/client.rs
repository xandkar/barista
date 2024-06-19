use std::{
    path::Path,
    time::{Duration, SystemTime},
};

use anyhow::anyhow;
use tarpc::{
    tokio_serde::formats::Bincode, tokio_util::codec::LengthDelimitedCodec,
};
use tokio::net::UnixStream;

use crate::{conf, control};

pub struct Client {
    client: control::BarCtlClient,
    ctx: tarpc::context::Context,
}

impl Client {
    pub async fn new(dir: &Path, timeout: Duration) -> anyhow::Result<Self> {
        let conn = UnixStream::connect(conf::sock_file(dir)).await?;
        let codec_builder = LengthDelimitedCodec::builder();
        let transport = tarpc::serde_transport::new(
            codec_builder.new_framed(conn),
            Bincode::default(),
        );
        let client = control::BarCtlClient::new(
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

    pub async fn on(&self) -> anyhow::Result<()> {
        self.client.on(self.ctx).await??;
        Ok(())
    }

    pub async fn off(&self) -> anyhow::Result<()> {
        self.client.off(self.ctx).await??;
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
