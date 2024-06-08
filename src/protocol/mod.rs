pub mod client;
pub mod server;

use std::result;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
pub enum Error {}

pub type Result<T> = result::Result<T, Error>;

#[tarpc::service]
pub trait Bar {
    async fn start() -> Result<()>;
    async fn stop() -> Result<()>;
    async fn status() -> Result<()>;
    async fn reload() -> Result<()>;
}
