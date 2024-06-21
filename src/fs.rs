use std::{path::Path, time::SystemTime};

use tokio::fs;

pub async fn size_in_bytes<P: AsRef<Path>>(path: P) -> anyhow::Result<u64> {
    let path = path.as_ref();
    let meta = fs::metadata(path).await?;
    let size = meta.len();
    Ok(size)
}

pub async fn mtime<P: AsRef<Path>>(path: P) -> anyhow::Result<SystemTime> {
    let path = path.as_ref();
    let meta = fs::metadata(path).await?;
    let mtime = meta.modified()?;
    Ok(mtime)
}
