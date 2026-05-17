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
    install_panic_hook();

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
    let src = source_element_str(&cfg.device);
    if cfg.use_combiner {
        format!(
            r#"{src} ! {caps} ! videoconvert ! ndisinkcombiner name=c c.src ! ndisink ndi-name="{name_escaped}""#
        )
    } else {
        format!(r#"{src} ! {caps} ! videoconvert ! ndisink ndi-name="{name_escaped}""#)
    }
}

/// Platform-specific video source element. Linux uses V4L2 with a device path;
/// macOS (dev only) uses AVFoundation with a numeric device index parsed from
/// `device` (fallback 0). The mac branch exists so the precog binary can be
/// smoke-tested against the host webcam; production deploys are Pi-only.
#[cfg(target_os = "linux")]
fn source_element_str(device: &str) -> String {
    format!(r#"v4l2src device="{device}""#)
}

#[cfg(target_os = "macos")]
fn source_element_str(device: &str) -> String {
    let idx: u32 = device.parse().unwrap_or(0);
    format!("avfvideosrc device-index={idx}")
}

/// Install a panic hook that logs via tracing then exits with code 101 so
/// `systemd Restart=on-failure` fires. Without this, a panic on a worker
/// thread silently dies and the daemon keeps running in a degraded state.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()));
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

#[cfg(test)]
mod tests {
    use super::{build_pipeline_string, source_element_str};
    use crate::config::PrecogConfig;

    fn cfg(use_combiner: bool, device: &str) -> PrecogConfig {
        let raw = format!(
            r#"
ndi_name = "PRECOG-99-MAC-TEST"
device = "{device}"
format = "UYVY"
width = 1280
height = 720
framerate = "30/1"
use_combiner = {use_combiner}
"#
        );
        PrecogConfig::from_toml(&raw).unwrap()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_uses_v4l2src_with_device_path() {
        let s = source_element_str("/dev/video0");
        assert_eq!(s, r#"v4l2src device="/dev/video0""#);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_uses_avfvideosrc_with_index() {
        assert_eq!(source_element_str("0"), "avfvideosrc device-index=0");
        assert_eq!(source_element_str("2"), "avfvideosrc device-index=2");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_falls_back_to_zero_for_non_numeric_device() {
        assert_eq!(
            source_element_str("/dev/video0"),
            "avfvideosrc device-index=0"
        );
    }

    #[test]
    fn pipeline_uses_combiner_when_set() {
        let s = build_pipeline_string(&cfg(true, "0"));
        assert!(s.contains("ndisinkcombiner name=c c.src ! ndisink"));
    }

    #[test]
    fn pipeline_skips_combiner_when_unset() {
        let s = build_pipeline_string(&cfg(false, "0"));
        assert!(!s.contains("ndisinkcombiner"));
        assert!(s.contains("videoconvert ! ndisink"));
    }

    #[test]
    fn pipeline_escapes_quotes_and_backslashes_in_ndi_name() {
        let raw = r#"
ndi_name = "BAD\"NAME"
device = "0"
format = "UYVY"
width = 1280
height = 720
framerate = "30/1"
"#;
        let c = PrecogConfig::from_toml(raw).unwrap();
        let s = build_pipeline_string(&c);
        assert!(s.contains(r#"ndi-name="BAD\"NAME""#));
    }
}
