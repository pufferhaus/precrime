//! REPORT — PRECRIME switcher daemon entry point.

use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tracing::info!("REPORT starting");
    Ok(())
}
