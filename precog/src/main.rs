//! PRECOG — camera → H.264/RTP/UDP-multicast encoder daemon.

mod config;

use anyhow::{Context, Result};
use config::PrecogConfig;
use temple::{Ball, BallV1, RtpInfo, Sender as BallSender, VideoInfo, BALL_PERIOD_SECS};
use gstreamer::prelude::*;
use std::env;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
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
    cfg.validate()
        .with_context(|| format!("validating config from {config_path}"))?;

    info!(?cfg, "PRECOG starting");

    gstreamer::init()?;

    let pipeline_str = build_pipeline_string(&cfg);
    info!(pipeline = %pipeline_str, "pipeline");

    let pipeline = gstreamer::parse::launch(&pipeline_str)
        .context("parse pipeline")?
        .downcast::<gstreamer::Pipeline>()
        .map_err(|_| anyhow::anyhow!("downcast to Pipeline"))?;

    pipeline.set_state(gstreamer::State::Playing)?;

    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, shutdown.clone())?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, shutdown.clone())?;

    spawn_ball_thread(&cfg, shutdown.clone())?;

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

/// Build the gst-launch pipeline string. Software H.264 encode via `x264enc`
/// (Pi 5 has no HW H.264 encoder; Pi 4 did, Pi 5 dropped it).
/// `tune=zerolatency speed-preset=ultrafast` + `key-int-max=30` (1s IDR).
pub fn build_pipeline_string(cfg: &PrecogConfig) -> String {
    let caps = format!(
        "video/x-raw,format={fmt},width={w},height={h},framerate={fr}",
        fmt = cfg.format,
        w = cfg.width,
        h = cfg.height,
        fr = cfg.framerate
    );
    let src = source_element_str(&cfg.device);
    let bitrate = cfg.bitrate_kbps;
    let mcast = cfg.rtp_mcast;
    let port = cfg.rtp_port;
    format!(
        "{src} ! {caps} ! videoconvert ! \
         x264enc tune=zerolatency speed-preset=ultrafast bitrate={bitrate} key-int-max=30 ! \
         video/x-h264,profile=baseline ! \
         h264parse config-interval=1 ! \
         rtph264pay pt=96 config-interval=1 mtu=1400 ! \
         udpsink host={mcast} port={port} auto-multicast=true ttl-mc=1 sync=false async=false"
    )
}

#[cfg(target_os = "linux")]
fn source_element_str(device: &str) -> String {
    format!(r#"v4l2src device="{device}""#)
}

#[cfg(target_os = "macos")]
fn source_element_str(device: &str) -> String {
    let idx: u32 = device.parse().unwrap_or(0);
    format!("avfvideosrc device-index={idx}")
}

/// Spawn a thread that emits a `Ball::V1` every `BALL_PERIOD_SECS`.
fn spawn_ball_thread(cfg: &PrecogConfig, shutdown: Arc<AtomicBool>) -> Result<()> {
    let ball = Ball::V1(BallV1 {
        name: cfg.source_name.clone(),
        host: cfg.host.clone(),
        rtp: RtpInfo {
            mcast: cfg.rtp_mcast.to_string(),
            port: cfg.rtp_port,
            pt: 96,
            clock_rate: 90000,
            encoding_name: "H264".into(),
        },
        video: VideoInfo {
            width: cfg.width,
            height: cfg.height,
            framerate: cfg.framerate.clone(),
        },
    });
    let sender = BallSender::new(cfg.temple_group, cfg.temple_port)
        .context("create ball sender")?;
    std::thread::Builder::new()
        .name("precog-ball-tx".into())
        .spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                if let Err(e) = sender.send(&ball) {
                    warn!(error = ?e, "ball send failed");
                }
                std::thread::sleep(Duration::from_secs(BALL_PERIOD_SECS));
            }
        })
        .context("spawn ball thread")?;
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::{build_pipeline_string, source_element_str};
    use crate::config::PrecogConfig;

    fn cfg() -> PrecogConfig {
        let raw = r#"
source_name = "PRECOG-99-TEST"
device = "/dev/video0"
format = "UYVY"
width = 1920
height = 1080
framerate = "30/1"
rtp_mcast = "239.42.1.1"
rtp_port = 5000
"#;
        PrecogConfig::from_toml(raw).unwrap()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_uses_v4l2src_with_device_path() {
        assert_eq!(source_element_str("/dev/video0"), r#"v4l2src device="/dev/video0""#);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_uses_avfvideosrc_with_index() {
        assert_eq!(source_element_str("0"), "avfvideosrc device-index=0");
    }

    #[test]
    fn pipeline_contains_x264enc_with_zerolatency() {
        let s = build_pipeline_string(&cfg());
        assert!(s.contains("x264enc tune=zerolatency speed-preset=ultrafast bitrate=4000"));
    }

    #[test]
    fn pipeline_contains_rtph264pay_with_pt96() {
        let s = build_pipeline_string(&cfg());
        assert!(s.contains("rtph264pay pt=96 config-interval=1 mtu=1400"));
    }

    #[test]
    fn pipeline_targets_configured_mcast_and_port() {
        let s = build_pipeline_string(&cfg());
        assert!(s.contains("udpsink host=239.42.1.1 port=5000"));
        assert!(s.contains("auto-multicast=true ttl-mc=1"));
    }

    #[test]
    fn pipeline_has_no_ndi_references() {
        let s = build_pipeline_string(&cfg());
        assert!(!s.contains("ndisink"));
        assert!(!s.contains("ndisinkcombiner"));
    }
}
