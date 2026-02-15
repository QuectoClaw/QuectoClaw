// QuectoClaw — Structured logging via tracing

use tracing_subscriber::{fmt, EnvFilter};

/// Initialize the global tracing subscriber.
///
/// Log level is controlled by the `QUECTOCLAW_LOG` env var (default: `info`).
/// Examples:
///   QUECTOCLAW_LOG=debug
///   QUECTOCLAW_LOG=quectoclaw::tool=trace,info
pub fn init() {
    let filter =
        EnvFilter::try_from_env("QUECTOCLAW_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();
}

/// Initialize logger for tests (does not panic if called multiple times).
#[cfg(test)]
pub fn init_test() {
    let _ = fmt()
        .with_env_filter(EnvFilter::new("debug"))
        .with_test_writer()
        .try_init();
}
