// QuectoClaw — Structured logging via tracing

use crate::tui::app::{LogLevel, TuiState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Layer};

static DASHBOARD_ACTIVE: AtomicBool = AtomicBool::new(false);
static TUI_STATE: Mutex<Option<TuiState>> = Mutex::new(None);

/// Set whether the dashboard is active (to silence stdout logs).
pub fn set_dashboard_active(active: bool) {
    DASHBOARD_ACTIVE.store(active, Ordering::Relaxed);
}

/// Initialize the global tracing subscriber.
///
/// Log level is controlled by the `QUECTOCLAW_LOG` env var (default: `info`).
/// Examples:
///   QUECTOCLAW_LOG=debug
///   QUECTOCLAW_LOG=quectoclaw::tool=trace,info
pub fn init() {
    let filter =
        EnvFilter::try_from_env("QUECTOCLAW_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    let stdout_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .with_filter(tracing_subscriber::filter::filter_fn(|_| {
            !DASHBOARD_ACTIVE.load(Ordering::Relaxed)
        }));

    let tui_layer =
        tracing_subscriber::filter::filter_fn(|_| DASHBOARD_ACTIVE.load(Ordering::Relaxed))
            .and_then(InternalTuiLayer);

    tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(tui_layer)
        .init();
}

struct InternalTuiLayer;

impl<S> Layer<S> for InternalTuiLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if let Ok(state_opt) = TUI_STATE.lock() {
            if let Some(state) = state_opt.as_ref() {
                let mut visitor = LogVisitor::default();
                event.record(&mut visitor);

                let level = match *event.metadata().level() {
                    tracing::Level::ERROR => LogLevel::Error,
                    tracing::Level::WARN => LogLevel::Warn,
                    tracing::Level::INFO => LogLevel::Info,
                    _ => LogLevel::Debug,
                };

                if !visitor.message.is_empty() {
                    state.push_log_sync(level, visitor.message);
                }
            }
        }
    }
}

#[derive(Default)]
struct LogVisitor {
    message: String,
}

impl tracing::field::Visit for LogVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

/// Add TUI logging layer. This should be called if --dashboard is enabled.
pub fn attach_tui(state: TuiState) {
    if let Ok(mut state_opt) = TUI_STATE.lock() {
        *state_opt = Some(state);
    }
    set_dashboard_active(true);
}

/// Initialize logger for tests (does not panic if called multiple times).
#[cfg(test)]
pub fn init_test() {
    let _ = fmt()
        .with_env_filter(EnvFilter::new("debug"))
        .with_test_writer()
        .try_init();
}
