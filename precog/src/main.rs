//! PRECOG — analog CCTV → NDI encoder daemon.

mod config;

use anyhow::{Context, Result};
use config::PrecogConfig;
use gstreamer::prelude::*;
use std::env;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{error, info, warn};

fn main() -> Result<()> {
    init_tracing();

    let config_path =
        env::var("PRECOG_CONFIG").unwrap_or_else(|_| "/etc/precog/precog.conf".into());
    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("reading config from {config_path}"))?;
    let cfg = PrecogConfig::from_toml(&raw)
        .with_context(|| format!("parsing config from {config_path}"))?;

    info!(?cfg, "PRECOG starting");

    gstreamer::init()?;

    let pipeline_str = build_pipeline_string(&cfg);
    info!(pipeline = %pipeline_str, "pipeline");

    let pipeline = gstreamer::parse::launch(&pipeline_str)
        .context("parse pipeline")?
        .downcast::<gstreamer::Pipeline>()
        .map_err(|_| anyhow::anyhow!("downcast to Pipeline"))?;

    pipeline.set_state(gstreamer::State::Playing)?;

    // Shutdown flag flipped by SIGTERM/SIGINT handlers.
    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, shutdown.clone())?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, shutdown.clone())?;

    let bus = pipeline.bus().context("pipeline bus")?;
    while !shutdown.load(Ordering::Relaxed) {
        let Some(msg) = bus.timed_pop(gstreamer::ClockTime::from_mseconds(500)) else {
            continue;
        };
        use gstreamer::MessageView;
        match msg.view() {
            MessageView::Eos(..) => {
                warn!("EOS received — exiting non-zero so systemd restarts (camera disconnect?)");
                let _ = pipeline.set_state(gstreamer::State::Null);
                return Err(anyhow::anyhow!("unexpected EOS"));
            }
            MessageView::Error(err) => {
                error!(
                    src = ?err.src().map(|s| s.path_string()),
                    error = %err.error(),
                    debug = ?err.debug(),
                    "pipeline error"
                );
                let _ = pipeline.set_state(gstreamer::State::Null);
                return Err(anyhow::anyhow!(err.error().to_string()));
            }
            _ => {}
        }
    }

    info!("shutdown signal received — tearing down pipeline");
    let _ = pipeline.set_state(gstreamer::State::Null);
    Ok(())
}

fn build_pipeline_string(cfg: &PrecogConfig) -> String {
    let caps = format!(
        "video/x-raw,format={fmt},width={w},height={h},framerate={fr}",
        fmt = cfg.format,
        w = cfg.width,
        h = cfg.height,
        fr = cfg.framerate
    );
    let name_escaped = cfg.ndi_name.replace('\\', "\\\\").replace('"', "\\\"");
    if cfg.use_combiner {
        format!(
            r#"v4l2src device="{dev}" ! {caps} ! videoconvert ! ndisinkcombiner name=c c.src ! ndisink ndi-name="{name}""#,
            dev = cfg.device,
            caps = caps,
            name = name_escaped,
        )
    } else {
        format!(
            r#"v4l2src device="{dev}" ! {caps} ! videoconvert ! ndisink ndi-name="{name}""#,
            dev = cfg.device,
            caps = caps,
            name = name_escaped,
        )
    }
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
