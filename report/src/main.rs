//! REPORT — PRECRIME switcher daemon entry point.

use anyhow::{Context, Result};
use report::config::ReportConfig;
use std::env;
use std::fs;

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let config_path =
        env::var("REPORT_CONFIG").unwrap_or_else(|_| "/etc/precrime/report.conf".into());
    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("reading config from {config_path}"))?;
    let cfg = ReportConfig::from_toml(&raw)
        .with_context(|| format!("parsing config from {config_path}"))?;

    tracing::info!(?cfg, "REPORT starting");
    Ok(())
}
