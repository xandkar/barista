use std::time::Duration;

use tokio::time::sleep;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Layer};

pub fn init(level: tracing::Level) -> anyhow::Result<()> {
    let layer_stderr = fmt::Layer::new()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_file(false)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_filter(
            EnvFilter::from_default_env().add_directive(level.into()),
        );
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(layer_stderr),
    )?;
    Ok(())
}

pub async fn finish() {
    // Terrible approximation of flushing.
    sleep(Duration::from_micros(5)).await;
}
