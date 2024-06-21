use std::time::Duration;

use tokio::time::sleep;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Layer};

pub fn init(
    level: Option<tracing::Level>,
    debug: bool,
) -> anyhow::Result<()> {
    let level = level.unwrap_or(if debug {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    });
    let extra_filter_directive = if debug {
        None
    } else {
        let directive = "tarpc=off".parse().unwrap_or_else(|_| {
            unreachable!("Invalid directive in tracinig init")
        });
        Some(directive)
    };
    let base_env_filter =
        || EnvFilter::from_default_env().add_directive(level.into());
    let env_filter = extra_filter_directive
        .map(|d| base_env_filter().add_directive(d))
        .unwrap_or_else(base_env_filter);
    let layer_stderr = fmt::Layer::new()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_file(false)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_filter(env_filter);
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(layer_stderr),
    )?;
    Ok(())
}

pub async fn finish() {
    // Terrible approximation of flushing.
    sleep(Duration::from_micros(5)).await;
}
