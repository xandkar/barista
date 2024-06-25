use std::path::{Path, PathBuf};

use anyhow::Context;
use tokio::fs;

pub const FEEDS_DIR_NAME: &str = "feeds";
pub const FEED_LOG_FILE_NAME: &str = "log";
pub const FEED_PID_FILE_NAME: &str = "pid";
pub const SERVER_PID_FILE_NAME: &str = "pid";
pub const SERVER_SOCK_FILE_NAME: &str = "socket";
pub const CONF_FILE_NAME: &str = "conf.toml";
pub const DEFAULT_DST: Dst = Dst::X11RootWindowName;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Conf {
    pub feeds: Vec<Feed>,
    pub dst: Option<Dst>,
    pub sep: String,
    pub pad_left: String,
    pub pad_right: String,
    pub expiry_character: char,
    pub output_interval: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Dst {
    StdOut,
    StdErr,
    File { path: PathBuf },
    X11RootWindowName,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Feed {
    pub name: String,
    pub cmd: String,

    pub ttl: Option<f64>,
    pub shell: Option<PathBuf>,
}

pub fn default_shell() -> PathBuf {
    PathBuf::from("/bin/bash")
}

impl Default for Conf {
    fn default() -> Self {
        Self {
            feeds: vec![
                Feed {
                    name: "uptime".to_string(),
                    cmd: "while :; do uptime; sleep 1; done".to_string(),
                    ttl: Some(1.0),
                    shell: None,
                },
                Feed {
                    name: "time".to_string(),
                    cmd: "while :; do date; sleep 1; done".to_string(),
                    ttl: Some(1.0),
                    shell: None,
                },
            ],
            dst: Some(DEFAULT_DST),
            sep: "   ".to_string(),
            pad_left: " ".to_string(),
            pad_right: " ".to_string(),
            expiry_character: '_',
            output_interval: 1.0,
        }
    }
}

impl Conf {
    pub async fn from_file(file: &Path) -> anyhow::Result<Self> {
        let data: String = fs::read_to_string(file)
            .await
            .context(format!("Failed to read file: {:?}", file))?;
        let selph: Self = toml::from_str(&data)
            .context(format!("Failed to parse TOML from: {:?}", file))?;
        Ok(selph)
    }

    pub async fn load_or_init(dir: &Path) -> anyhow::Result<Self> {
        let file = conf_file(dir);
        if fs::try_exists(&file).await.context(format!(
            "Failed to check existance of path: {:?}",
            &file
        ))? {
            Self::from_file(&file).await
        } else {
            let default = Self::default();
            fs::write(&file, toml::to_string_pretty(&default)?).await?;
            Ok(default)
        }
    }

    pub fn get_dst(&self) -> Dst {
        self.dst.as_ref().unwrap_or(&DEFAULT_DST).to_owned()
    }
}

pub fn pid_file(dir: &Path) -> PathBuf {
    dir.join(SERVER_PID_FILE_NAME)
}

pub fn sock_file(dir: &Path) -> PathBuf {
    dir.join(SERVER_SOCK_FILE_NAME)
}

pub fn conf_file(dir: &Path) -> PathBuf {
    dir.join(CONF_FILE_NAME)
}
