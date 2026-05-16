//! REPORT — PRECRIME switcher daemon entry point.

use anyhow::{Context, Result};
use report::config::ReportConfig;
use report::daemon::Daemon;
use std::env;
use std::fs;

fn main() -> Result<()> {
    init_tracing();
    install_panic_hook();

    let config_path =
        env::var("REPORT_CONFIG").unwrap_or_else(|_| "/etc/precrime/report.conf".into());
    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("reading config from {config_path}"))?;
    let cfg = ReportConfig::from_toml(&raw)
        .with_context(|| format!("parsing config from {config_path}"))?;

    tracing::info!(?cfg, "REPORT starting");
    Daemon::new(cfg).run()
}

/// Install a panic hook that logs via tracing then exits with code 101 so
/// `systemd Restart=on-failure` fires. Without this, a panic on a worker
/// thread (keyboard / bus-watch / discovery) silently dies and the daemon
/// keeps running in a degraded state.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line()));
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("(unknown payload)");
        let backtrace = std::backtrace::Backtrace::capture();
        tracing::error!(?location, payload, %backtrace, "panic — exiting non-zero");
        std::process::exit(101);
    }));
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    match tracing_journald::layer() {
        Ok(layer) => {
            use tracing_subscriber::prelude::*;
            tracing_subscriber::registry()
                .with(env_filter)
                .with(layer)
                .init();
        }
        Err(_) => {
            fmt().with_env_filter(env_filter).init();
        }
    }
}
