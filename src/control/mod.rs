pub mod client;
pub mod server;

use std::result;

use serde::{Deserialize, Serialize};

use crate::bar;

#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
#[error("{text:?}")]
pub struct Error {
    text: String,
}

impl From<bar::server::ApiError> for Error {
    fn from(e: bar::server::ApiError) -> Self {
        let text = e.to_string();
        Self { text }
    }
}

pub type Result<T> = result::Result<T, Error>;

#[tarpc::service]
pub trait BarCtl {
    async fn on() -> Result<()>;
    async fn off() -> Result<()>;
    async fn status() -> Result<bar::status::Status>;
    async fn reload() -> Result<()>;
}
