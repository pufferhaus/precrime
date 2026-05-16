//! REPORT — PRECRIME switcher daemon entry point.

use anyhow::{Context, Result};
use report::config::ReportConfig;
use report::daemon::Daemon;
use std::env;
use std::fs;

fn main() -> Result<()> {
    init_tracing();

    let config_path =
        env::var("REPORT_CONFIG").unwrap_or_else(|_| "/etc/precrime/report.conf".into());
    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("reading config from {config_path}"))?;
    let cfg = ReportConfig::from_toml(&raw)
        .with_context(|| format!("parsing config from {config_path}"))?;

    tracing::info!(?cfg, "REPORT starting");
    Daemon::new(cfg).run()
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
