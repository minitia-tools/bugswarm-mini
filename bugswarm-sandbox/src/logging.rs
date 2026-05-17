//! Structured logging configuration.
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

pub fn init_logging(default_level: &str, log_file: Option<&str>) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .json();

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer);

    if let Some(path) = log_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("Failed to open log file");
        let file_layer = fmt::layer()
            .with_writer(std::sync::Mutex::new(file))
            .json();
        subscriber.with(file_layer).init();
    } else {
        subscriber.init();
    }
}
