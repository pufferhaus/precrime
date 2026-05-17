# RTP+Multicast Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace NDI with a fully-FOSS LAN video stack — H.264-over-RTP on IPv4 multicast for streams + a custom UDP-multicast JSON ball for temple. Drops the NDI SDK license dependency entirely while improving glass-to-glass latency.

**Architecture:**
- **Transport.** Each precog encodes one camera to H.264, packetizes with RTP, sends via UDP to a per-source multicast group on the local LAN segment (TTL=1, admin-scoped `239.42.0.0/16`). Report joins those groups via `udpsrc` and decodes.
- **Temple.** Each precog also emits a small JSON ball every 2s on a fixed multicast temple channel (`239.42.0.1:9999`). Report listens on that channel, deserializes balls, evicts entries unheard for 6s.
- **Shared crate.** A new workspace member `temple/` owns the ball wire format, sender, and receiver. Both binaries depend on it.
- **No more NDI.** `report/src/ndi_find.rs`, `report/build.rs`, the `PRECRIME_NDI_STUB` env knob, and every `ndisink`/`ndisrc`/`ndisinkcombiner`/`ndisrcdemux` reference get deleted.

**Tech Stack:**
- Rust 1.75, workspace with three members: `temple`, `precog`, `report`
- `serde` + `serde_json` (new) for ball wire format
- `socket2` (new) for UDP-multicast joins on the receive side (the stdlib `UdpSocket` API can't join a multicast group on a specific interface; `socket2` exposes `setsockopt(IP_ADD_MEMBERSHIP)`)
- GStreamer 1.22+ with `x264enc`, `rtph264pay/depay`, `rtpjitterbuffer`, `udpsink`, `udpsrc`, `avdec_h264` (baseline) or `v4l2slh264dec` (Pi 5 HW decode, future tune)

**Latency targets (LAN, Pi 5 both ends, 1080p30):**
- Glass-to-glass steady state: **75–150 ms**
- Temple → first frame after precog boot: **2–4 s** (worst case = one ball interval + IDR wait)
- Switch (cut) → program out: **same as today** (input-selector is unchanged)

**Why software H.264 encode on the Pi 5:** Pi 4 had a hardware H.264 encoder (`v4l2h264enc`); Pi 5 dropped it. Encode is now `x264enc tune=zerolatency speed-preset=ultrafast`. The Pi 5's 4×Cortex-A76 at 2.4 GHz handles 1080p30 ultrafast comfortably (~30–50% of one core).

---

## Task 1: Bootstrap `temple` workspace crate

**Files:**
- Create: `temple/Cargo.toml`
- Create: `temple/src/lib.rs`
- Modify: `Cargo.toml` (workspace root, members list)

- [ ] **Step 1: Add the new crate to the workspace**

Edit `Cargo.toml` at the repo root, change `members`:

```toml
[workspace]
resolver = "2"
members = ["report", "precog", "temple"]
```

- [ ] **Step 2: Write `temple/Cargo.toml`**

```toml
[package]
name = "temple"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
description = "PRECRIME temple: ball wire format + UDP multicast send/receive"

[dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
socket2 = { version = "0.5", features = ["all"] }
tracing = "0.1"

[dev-dependencies]
pretty_assertions = "1"

[lints]
workspace = true
```

- [ ] **Step 3: Stub `temple/src/lib.rs`**

```rust
//! PRECRIME temple ball.

pub const DEFAULT_TEMPLE_GROUP: &str = "239.42.0.1";
pub const DEFAULT_TEMPLE_PORT: u16 = 9999;
pub const BALL_PERIOD_SECS: u64 = 2;
pub const BALL_EVICTION_SECS: u64 = 6;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p temple`
Expected: PASS, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml temple/
git commit -m "temple: bootstrap workspace crate"
```

---

## Task 2: Ball wire format + serde tests

**Files:**
- Create: `temple/src/ball.rs`
- Modify: `temple/src/lib.rs`

- [ ] **Step 1: Write `temple/src/ball.rs`**

```rust
//! Ball wire format. JSON over UDP. Versioned envelope.

use serde::{Deserialize, Serialize};

/// Versioned envelope. Internal tag `v` lets receivers reject unknown versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "v")]
pub enum Ball {
    #[serde(rename = "1")]
    V1(BallV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BallV1 {
    /// Source name, e.g. "PRECOG-01-IPHONE-STAGE".
    pub name: String,
    /// Sender host IP (informational; not used for joining the multicast group).
    pub host: String,
    pub rtp: RtpInfo,
    pub video: VideoInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RtpInfo {
    /// IPv4 multicast group for the stream, e.g. "239.42.1.1".
    pub mcast: String,
    pub port: u16,
    /// RTP payload type (96 for dynamic H.264).
    pub pt: u8,
    /// RTP clock rate in Hz (90000 for H.264).
    pub clock_rate: u32,
    /// "H264" — kept as a string so future codecs slot in without enum bump.
    pub encoding_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    /// e.g. "30/1"
    pub framerate: String,
}

impl Ball {
    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn name(&self) -> &str {
        match self {
            Ball::V1(b) => &b.name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Ball {
        Ball::V1(BallV1 {
            name: "PRECOG-01-IPHONE-STAGE".into(),
            host: "10.0.0.11".into(),
            rtp: RtpInfo {
                mcast: "239.42.1.1".into(),
                port: 5000,
                pt: 96,
                clock_rate: 90000,
                encoding_name: "H264".into(),
            },
            video: VideoInfo {
                width: 1920,
                height: 1080,
                framerate: "30/1".into(),
            },
        })
    }

    #[test]
    fn round_trips_through_json() {
        let b = sample();
        let bytes = b.to_json().unwrap();
        let parsed = Ball::from_json(&bytes).unwrap();
        assert_eq!(b, parsed);
    }

    #[test]
    fn name_accessor_returns_v1_name() {
        assert_eq!(sample().name(), "PRECOG-01-IPHONE-STAGE");
    }

    #[test]
    fn json_includes_version_tag() {
        let bytes = sample().to_json().unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains(r#""v":"1""#), "missing v tag, got {s}");
    }

    #[test]
    fn unknown_version_fails_to_parse() {
        let raw = br#"{"v":"99","name":"X"}"#;
        assert!(Ball::from_json(raw).is_err());
    }

    #[test]
    fn payload_under_one_mtu() {
        let bytes = sample().to_json().unwrap();
        assert!(bytes.len() < 600, "ball grew: {} bytes", bytes.len());
    }
}
```

- [ ] **Step 2: Wire `ball` module into `temple/src/lib.rs`**

```rust
//! PRECRIME temple ball.
//!
//! Wire format and UDP multicast send/receive helpers shared between PRECOG
//! (sender) and REPORT (receiver). The ball advertises one PRECOG source
//! per JSON message, sent every `BALL_PERIOD_SECS` on the temple channel.

pub mod ball;

pub use ball::{Ball, BallV1, RtpInfo, VideoInfo};

pub const DEFAULT_TEMPLE_GROUP: &str = "239.42.0.1";
pub const DEFAULT_TEMPLE_PORT: u16 = 9999;
pub const BALL_PERIOD_SECS: u64 = 2;
pub const BALL_EVICTION_SECS: u64 = 6;
```

- [ ] **Step 3: Run tests, verify they pass**

Run: `cargo test -p temple`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add temple/
git commit -m "temple: ball wire format with v1 envelope + serde tests"
```

---

## Task 3: Ball sender (UDP multicast TX)

**Files:**
- Create: `temple/src/send.rs`
- Modify: `temple/src/lib.rs`

- [ ] **Step 1: Write `temple/src/send.rs`**

```rust
//! UDP multicast ball sender. Owns a socket configured for IPv4 multicast
//! TX with TTL=1 (admin-scoped, never leaves the LAN segment).

use crate::ball::Ball;
use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddrV4};

pub struct Sender {
    socket: std::net::UdpSocket,
    dest: SocketAddrV4,
}

impl Sender {
    /// Bind an ephemeral UDP socket and configure it to send to `group:port`
    /// with multicast TTL=1.
    pub fn new(group: Ipv4Addr, port: u16) -> Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("create UDP socket")?;
        socket.set_multicast_ttl_v4(1).context("set mcast TTL")?;
        socket.set_multicast_loop_v4(true).context("enable mcast loopback")?;
        socket
            .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())
            .context("bind ephemeral port")?;
        let std_socket: std::net::UdpSocket = socket.into();
        Ok(Self {
            socket: std_socket,
            dest: SocketAddrV4::new(group, port),
        })
    }

    pub fn send(&self, ball: &Ball) -> Result<()> {
        let bytes = ball.to_json().context("serialize ball")?;
        self.socket
            .send_to(&bytes, self.dest)
            .context("send ball datagram")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ball::{BallV1, RtpInfo, VideoInfo};

    #[test]
    fn sender_constructs_with_valid_group() {
        let s = Sender::new(Ipv4Addr::new(239, 42, 0, 1), 9999);
        assert!(s.is_ok());
    }

    #[test]
    fn send_does_not_error_when_no_listener() {
        let s = Sender::new(Ipv4Addr::new(239, 42, 0, 1), 9999).unwrap();
        let b = Ball::V1(BallV1 {
            name: "PRECOG-99-TEST".into(),
            host: "127.0.0.1".into(),
            rtp: RtpInfo {
                mcast: "239.42.99.1".into(),
                port: 5000,
                pt: 96,
                clock_rate: 90000,
                encoding_name: "H264".into(),
            },
            video: VideoInfo {
                width: 1280,
                height: 720,
                framerate: "30/1".into(),
            },
        });
        assert!(s.send(&b).is_ok());
    }
}
```

- [ ] **Step 2: Wire `send` module into `temple/src/lib.rs`**

```rust
pub mod ball;
pub mod send;

pub use ball::{Ball, BallV1, RtpInfo, VideoInfo};
pub use send::Sender;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p temple`
Expected: 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add temple/
git commit -m "temple: UDP multicast ball sender"
```

---

## Task 4: Ball receiver with TTL eviction

**Files:**
- Create: `temple/src/recv.rs`
- Modify: `temple/src/lib.rs`

- [ ] **Step 1: Write `temple/src/recv.rs`**

```rust
//! UDP multicast ball receiver with last-seen tracking + TTL eviction.

use crate::ball::Ball;
use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};
use tracing::warn;

pub struct Receiver {
    socket: std::net::UdpSocket,
    sources: HashMap<String, (Ball, Instant)>,
    eviction: Duration,
}

impl Receiver {
    /// Bind to the temple group on all interfaces. `eviction` is the
    /// silence interval after which a source is dropped from the map.
    pub fn new(group: Ipv4Addr, port: u16, eviction: Duration) -> Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("create UDP socket")?;
        socket.set_reuse_address(true).context("SO_REUSEADDR")?;
        #[cfg(unix)]
        socket.set_reuse_port(true).context("SO_REUSEPORT")?;
        socket
            .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())
            .context("bind to temple port")?;
        socket
            .join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)
            .context("join temple multicast group")?;
        let std_socket: std::net::UdpSocket = socket.into();
        Ok(Self {
            socket: std_socket,
            sources: HashMap::new(),
            eviction,
        })
    }

    /// Block up to `timeout` for one datagram, parse it, and update the source
    /// map. Returns true if the source set changed (added, removed, or fields
    /// changed). Always evicts expired entries before returning.
    pub fn poll(&mut self, timeout: Duration) -> Result<bool> {
        self.socket
            .set_read_timeout(Some(timeout))
            .context("set read timeout")?;
        let mut buf = [0u8; 2048];
        let mut changed = false;
        match self.socket.recv_from(&mut buf) {
            Ok((n, _addr)) => match Ball::from_json(&buf[..n]) {
                Ok(b) => {
                    let name = b.name().to_owned();
                    let now = Instant::now();
                    let updated = match self.sources.get(&name) {
                        Some((existing, _)) => existing != &b,
                        None => true,
                    };
                    self.sources.insert(name, (b, now));
                    if updated {
                        changed = true;
                    }
                }
                Err(e) => {
                    warn!(error = ?e, "discarded malformed ball");
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(anyhow::Error::from(e).context("recv_from")),
        }
        changed |= self.evict_expired();
        Ok(changed)
    }

    fn evict_expired(&mut self) -> bool {
        let now = Instant::now();
        let before = self.sources.len();
        self.sources
            .retain(|_, (_, ts)| now.duration_since(*ts) < self.eviction);
        self.sources.len() != before
    }

    /// Snapshot of currently-live sources.
    pub fn snapshot(&self) -> Vec<Ball> {
        self.sources.values().map(|(b, _)| b.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ball::{Ball, BallV1, RtpInfo, VideoInfo};
    use crate::send::Sender;

    fn b(name: &str) -> Ball {
        Ball::V1(BallV1 {
            name: name.into(),
            host: "127.0.0.1".into(),
            rtp: RtpInfo {
                mcast: "239.42.99.1".into(),
                port: 5000,
                pt: 96,
                clock_rate: 90000,
                encoding_name: "H264".into(),
            },
            video: VideoInfo {
                width: 1280,
                height: 720,
                framerate: "30/1".into(),
            },
        })
    }

    #[test]
    fn sender_to_receiver_loopback() {
        let group = Ipv4Addr::new(239, 42, 0, 200);
        let port = 19999;
        let mut rx = Receiver::new(group, port, Duration::from_secs(6)).unwrap();
        let tx = Sender::new(group, port).unwrap();

        tx.send(&b("PRECOG-01-X")).unwrap();
        let changed = rx.poll(Duration::from_millis(500)).unwrap();
        assert!(changed, "first ball should be a change");
        let snap = rx.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].name(), "PRECOG-01-X");

        tx.send(&b("PRECOG-01-X")).unwrap();
        let changed = rx.poll(Duration::from_millis(500)).unwrap();
        assert!(!changed);
    }

    #[test]
    fn eviction_drops_silent_sources() {
        let group = Ipv4Addr::new(239, 42, 0, 201);
        let port = 19998;
        let mut rx = Receiver::new(group, port, Duration::from_millis(100)).unwrap();
        let tx = Sender::new(group, port).unwrap();

        tx.send(&b("PRECOG-02-Y")).unwrap();
        rx.poll(Duration::from_millis(500)).unwrap();
        assert_eq!(rx.snapshot().len(), 1);

        std::thread::sleep(Duration::from_millis(200));
        let changed = rx.poll(Duration::from_millis(50)).unwrap();
        assert!(changed, "eviction should report change");
        assert_eq!(rx.snapshot().len(), 0);
    }
}
```

- [ ] **Step 2: Wire `recv` into `temple/src/lib.rs`**

```rust
pub mod ball;
pub mod recv;
pub mod send;

pub use ball::{Ball, BallV1, RtpInfo, VideoInfo};
pub use recv::Receiver;
pub use send::Sender;

pub const DEFAULT_TEMPLE_GROUP: &str = "239.42.0.1";
pub const DEFAULT_TEMPLE_PORT: u16 = 9999;
pub const BALL_PERIOD_SECS: u64 = 2;
pub const BALL_EVICTION_SECS: u64 = 6;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p temple`
Expected: 9 tests pass.
Note: If `sender_to_receiver_loopback` fails on a host with multicast loopback disabled (some hardened CI), confirm by running `cargo test -p temple -- --nocapture` and inspecting for `recv_from` timing out. On macOS dev: a one-time `sudo route -n add -net 239 -interface lo0` may be required if loopback multicast isn't routed. On Linux: usually works out of the box.

- [ ] **Step 4: Commit**

```bash
git add temple/
git commit -m "temple: receiver with TTL eviction + loopback test"
```

---

## Task 5: New precog config schema

**Files:**
- Modify: `precog/src/config.rs`
- Modify: `precog/Cargo.toml`

- [ ] **Step 1: Rewrite `precog/src/config.rs`**

```rust
//! TOML config parsing for precog.

use serde::Deserialize;
use std::net::Ipv4Addr;

#[derive(Debug, Deserialize)]
pub struct PrecogConfig {
    /// Source display name, e.g. "PRECOG-02-CCTV-DOOR".
    pub source_name: String,
    /// V4L2 device path (Linux) or AVFoundation device index (macOS dev).
    pub device: String,
    /// Pixel format string, e.g. "UYVY" or "YUYV".
    pub format: String,
    pub width: u32,
    pub height: u32,
    /// "30/1" for NTSC, "25/1" for PAL.
    pub framerate: String,

    /// Per-source RTP multicast group, e.g. "239.42.1.1".
    pub rtp_mcast: Ipv4Addr,
    /// Per-source RTP UDP port, e.g. 5000.
    pub rtp_port: u16,
    /// Target H.264 bitrate in kbps. 4000 ≈ 1080p30 broadcast quality.
    #[serde(default = "default_bitrate_kbps")]
    pub bitrate_kbps: u32,

    /// Temple multicast group. Default: 239.42.0.1.
    #[serde(default = "default_temple_group")]
    pub temple_group: Ipv4Addr,
    #[serde(default = "default_temple_port")]
    pub temple_port: u16,

    /// Sender host IP advertised in balls (informational only).
    #[serde(default = "default_host")]
    pub host: String,
}

fn default_bitrate_kbps() -> u32 { 4000 }
fn default_temple_group() -> Ipv4Addr { "239.42.0.1".parse().unwrap() }
fn default_temple_port() -> u16 { 9999 }
fn default_host() -> String { "0.0.0.0".into() }

impl PrecogConfig {
    pub fn from_toml(raw: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimum_config() {
        let raw = r#"
source_name = "PRECOG-01-IPHONE-STAGE"
device = "/dev/video0"
format = "UYVY"
width = 1920
height = 1080
framerate = "30/1"
rtp_mcast = "239.42.1.1"
rtp_port = 5000
"#;
        let c = PrecogConfig::from_toml(raw).unwrap();
        assert_eq!(c.source_name, "PRECOG-01-IPHONE-STAGE");
        assert_eq!(c.rtp_port, 5000);
        assert_eq!(c.bitrate_kbps, 4000);
        assert_eq!(c.temple_port, 9999);
    }

    #[test]
    fn rejects_non_ipv4_mcast() {
        let raw = r#"
source_name = "X"
device = "0"
format = "UYVY"
width = 1
height = 1
framerate = "30/1"
rtp_mcast = "not-an-ip"
rtp_port = 5000
"#;
        assert!(PrecogConfig::from_toml(raw).is_err());
    }
}
```

- [ ] **Step 2: Add the `temple` dep to `precog/Cargo.toml`**

Append under `[dependencies]`:

```toml
temple = { path = "../temple" }
```

- [ ] **Step 3: Run config tests**

Run: `cargo test -p precog --lib config`
Expected: 2 tests pass.

Note: `cargo test -p precog` will fail to build `main.rs` because it still references `ndi_name` / `use_combiner`. That's fixed in Task 6. Run the scoped test above only.

- [ ] **Step 4: Commit**

```bash
git add precog/
git commit -m "precog: config schema for RTP transport (replaces ndi_name)"
```

---

## Task 6: Precog RTP pipeline + ball thread

**Files:**
- Modify: `precog/src/main.rs`

- [ ] **Step 1: Replace `precog/src/main.rs`**

```rust
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
```

- [ ] **Step 2: Run all precog tests**

Run: `cargo test -p precog`
Expected: 6 tests pass (2 config + 4 pipeline-string).

- [ ] **Step 3: Build smoke**

Run: `cargo build -p precog`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add precog/
git commit -m "precog: H.264 RTP multicast pipeline + ball thread"
```

---

## Task 7: Drop NDI FFI + build.rs from report (with temporary stub)

**Files:**
- Delete: `report/src/ndi_find.rs`
- Delete: `report/build.rs`
- Modify: `report/src/lib.rs`
- Modify: `report/Cargo.toml`
- Modify: `.github/workflows/ci.yml` (drop `PRECRIME_NDI_STUB`)

- [ ] **Step 1: Delete the NDI files**

```bash
rm report/src/ndi_find.rs report/build.rs
```

- [ ] **Step 2: Add the temporary in-tree stub to `report/src/lib.rs`**

So Task 7 can commit a clean state, replace the `pub mod ndi_find;` line in `report/src/lib.rs` with:

```rust
// Temporarily stubbed until Task 10 replaces with temple-backed source map.
#[allow(dead_code)]
pub(crate) mod ndi_find {
    pub struct Temple;
    impl Temple {
        pub fn new() -> anyhow::Result<Self> {
            anyhow::bail!("NDI removed; rewire daemon.rs to temple::Receiver")
        }
        pub fn poll(&self, _t: std::time::Duration) -> Vec<String> {
            Vec::new()
        }
    }
}
```

- [ ] **Step 3: Update `report/Cargo.toml`**

Append under `[dependencies]`:

```toml
temple = { path = "../temple" }
socket2 = { version = "0.5", features = ["all"] }
```

- [ ] **Step 4: Strip `PRECRIME_NDI_STUB` from CI**

Edit `.github/workflows/ci.yml`. Find any env block setting `PRECRIME_NDI_STUB: 1` and delete the line.

Verify nothing else references it:

```bash
grep -rn PRECRIME_NDI_STUB .github/ report/ precog/
```
Expected: zero matches.

- [ ] **Step 5: Confirm report builds**

Run: `cargo build -p report`
Expected: PASS (uses the stub; will be replaced in Task 10).

- [ ] **Step 6: Commit**

```bash
git add -A report/ .github/
git commit -m "report: drop libndi FFI + build.rs (temporary in-tree stub)"
```

---

## Task 8: Report config — drop NDI knobs, add temple channel

**Files:**
- Modify: `report/src/config.rs`

- [ ] **Step 1: Update the config struct**

Replace `report/src/config.rs`:

```rust
//! TOML config parsing for REPORT.

use serde::Deserialize;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
}

#[derive(Debug, Deserialize)]
pub struct ReportConfig {
    pub program_connector_id: u32,
    pub preview_connector_id: u32,
    pub keyboard_device: String,
    #[serde(default)]
    pub source_slot_overrides: HashMap<String, u8>,

    /// Temple multicast group. Default: 239.42.0.1.
    #[serde(default = "default_temple_group")]
    pub temple_group: Ipv4Addr,
    #[serde(default = "default_temple_port")]
    pub temple_port: u16,
}

fn default_temple_group() -> Ipv4Addr { "239.42.0.1".parse().unwrap() }
fn default_temple_port() -> u16 { 9999 }

impl ReportConfig {
    pub fn from_toml(raw: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(raw)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimum_config_with_defaults() {
        let raw = r#"
program_connector_id = 32
preview_connector_id = 34
keyboard_device = "/dev/input/event0"
"#;
        let c = ReportConfig::from_toml(raw).unwrap();
        assert_eq!(c.temple_port, 9999);
        assert_eq!(c.temple_group.to_string(), "239.42.0.1");
    }

    #[test]
    fn parses_explicit_temple_overrides() {
        let raw = r#"
program_connector_id = 32
preview_connector_id = 34
keyboard_device = "/dev/input/event0"
temple_group = "239.42.0.99"
temple_port = 12345
"#;
        let c = ReportConfig::from_toml(raw).unwrap();
        assert_eq!(c.temple_group.to_string(), "239.42.0.99");
        assert_eq!(c.temple_port, 12345);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p report --lib config`
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add report/src/config.rs
git commit -m "report: config schema for temple channel (drops NDI knobs)"
```

---

## Task 9: Report — RTP pipeline builders

**Files:**
- Modify: `report/src/pipeline.rs`

- [ ] **Step 1: Add the `Source` descriptor at the top of `report/src/pipeline.rs`**

Above `ProgramPipeline`:

```rust
/// One discovered source as seen by the pipeline builders. Cloned from the
/// `temple::Ball` payload at the moment pipelines are rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub mcast: String,   // IPv4 multicast group, e.g. "239.42.1.1"
    pub port: u16,
    pub payload_type: u8,
    pub clock_rate: u32,
    pub encoding_name: String, // "H264"
}
```

- [ ] **Step 2: Rewrite `program_pipeline_string`**

Replace the existing function:

```rust
pub fn program_pipeline_string(sources: &[Source], connector_id: u32) -> String {
    let mut parts = String::from("input-selector name=sel");
    for (i, s) in sources.iter().enumerate() {
        parts.push_str(&format!(
            " udpsrc address={mcast} port={port} auto-multicast=true \
             caps=\"application/x-rtp,media=video,clock-rate={cr},encoding-name={enc},payload={pt}\" ! \
             rtpjitterbuffer latency=20 ! \
             rtph264depay ! h264parse ! avdec_h264 ! \
             queue max-size-buffers=4 leaky=downstream ! \
             videoconvert ! sel.sink_{i}",
            mcast = s.mcast,
            port = s.port,
            cr = s.clock_rate,
            enc = s.encoding_name,
            pt = s.payload_type,
        ));
    }
    parts.push_str(&format!(
        " sel. ! videoconvert ! kmssink connector-id={connector_id}"
    ));
    parts
}
```

- [ ] **Step 3: Rewrite `preview_pipeline_string`**

```rust
pub fn preview_pipeline_string(sources: &[Source], connector_id: u32) -> String {
    if sources.is_empty() {
        return format!(
            "videotestsrc pattern=black is-live=true ! video/x-raw,width=1920,height=1080 ! videoconvert ! kmssink connector-id={connector_id}"
        );
    }

    let n = sources.len();
    let (cols, rows) = grid_for(n);
    let tile_w: u32 = 1920 / cols as u32;
    let tile_h: u32 = 1080 / rows as u32;

    let mut s = String::from("compositor name=mix background=black");
    for (i, _) in sources.iter().enumerate() {
        let col = (i % cols) as u32;
        let row = (i / cols) as u32;
        s.push_str(&format!(
            " sink_{i}::xpos={x} sink_{i}::ypos={y} sink_{i}::width={tile_w} sink_{i}::height={tile_h}",
            x = col * tile_w,
            y = row * tile_h,
        ));
    }
    for (i, src) in sources.iter().enumerate() {
        s.push_str(&format!(
            " udpsrc address={mcast} port={port} auto-multicast=true \
             caps=\"application/x-rtp,media=video,clock-rate={cr},encoding-name={enc},payload={pt}\" ! \
             rtpjitterbuffer latency=20 ! \
             rtph264depay ! h264parse ! avdec_h264 ! \
             queue max-size-buffers=4 leaky=downstream ! \
             videoconvert ! videoscale ! video/x-raw,width={tile_w},height={tile_h} ! mix.sink_{i}",
            mcast = src.mcast,
            port = src.port,
            cr = src.clock_rate,
            enc = src.encoding_name,
            pt = src.payload_type,
        ));
    }
    s.push_str(&format!(
        " mix. ! videoconvert ! cairooverlay name=tally ! videoconvert ! kmssink connector-id={connector_id}"
    ));
    s
}
```

- [ ] **Step 4: Update `build_program` and `build_preview` signatures**

Change parameter type from `&[String]` to `&[Source]`. Body stays structurally the same; thread `sources` through, use `sources.len()` where `source_names.len()` was used. The tally-overlay closure signature is unchanged.

Delete the `escape_ndi_name` helper and its tests — UDP/IP fields don't need escaping.

- [ ] **Step 5: Replace the test module**

```rust
#[cfg(test)]
mod tests {
    use super::{preview_pipeline_string, program_pipeline_string, Source};

    fn s(name: &str, mcast: &str, port: u16) -> Source {
        Source {
            name: name.into(),
            mcast: mcast.into(),
            port,
            payload_type: 96,
            clock_rate: 90000,
            encoding_name: "H264".into(),
        }
    }

    #[test]
    fn program_pipeline_zero_sources() {
        let p = program_pipeline_string(&[], 32);
        assert_eq!(
            p,
            "input-selector name=sel sel. ! videoconvert ! kmssink connector-id=32"
        );
    }

    #[test]
    fn program_pipeline_one_source_uses_rtp_chain() {
        let p = program_pipeline_string(&[s("A", "239.42.1.1", 5000)], 32);
        assert!(p.contains("udpsrc address=239.42.1.1 port=5000 auto-multicast=true"));
        assert!(p.contains("rtpjitterbuffer latency=20"));
        assert!(p.contains("rtph264depay ! h264parse ! avdec_h264"));
        assert!(p.contains("sel.sink_0"));
        assert!(p.ends_with("kmssink connector-id=32"));
        assert!(!p.contains("ndisrc"));
    }

    #[test]
    fn program_pipeline_two_sources_use_distinct_groups() {
        let p = program_pipeline_string(
            &[s("A", "239.42.1.1", 5000), s("B", "239.42.1.2", 5000)],
            32,
        );
        assert!(p.contains("address=239.42.1.1"));
        assert!(p.contains("address=239.42.1.2"));
        assert!(p.contains("sel.sink_0"));
        assert!(p.contains("sel.sink_1"));
    }

    #[test]
    fn preview_pipeline_zero_sources_is_black_test_pattern() {
        let p = preview_pipeline_string(&[], 34);
        assert!(p.starts_with("videotestsrc pattern=black is-live=true"));
        assert!(!p.contains("compositor"));
    }

    #[test]
    fn preview_pipeline_four_sources_uses_2x2_grid() {
        let sources = vec![
            s("A", "239.42.1.1", 5000),
            s("B", "239.42.1.2", 5000),
            s("C", "239.42.1.3", 5000),
            s("D", "239.42.1.4", 5000),
        ];
        let p = preview_pipeline_string(&sources, 34);
        assert!(p.contains("sink_0::xpos=0 sink_0::ypos=0 sink_0::width=960 sink_0::height=540"));
        assert!(p.contains("sink_3::xpos=960 sink_3::ypos=540 sink_3::width=960 sink_3::height=540"));
        assert!(p.ends_with(
            "mix. ! videoconvert ! cairooverlay name=tally ! videoconvert ! kmssink connector-id=34"
        ));
    }
}
```

- [ ] **Step 6: Run pipeline tests only**

Run: `cargo test -p report --lib pipeline`
Expected: 5 tests pass.

(`cargo build -p report` will fail because `daemon.rs` still passes `&[String]`. That's Task 10. Hold the commit until then so the switchover is atomic.)

---

## Task 10: Report daemon — wire `temple::Receiver` + `Source`

**Files:**
- Modify: `report/src/daemon.rs`
- Modify: `report/src/lib.rs` (drop the stub + `naming` module)
- Delete: `report/src/naming.rs`

- [ ] **Step 1: Replace `report/src/daemon.rs`**

```rust
//! REPORT daemon: owns pipelines, handles source-set changes and keypresses.

use crate::config::ReportConfig;
use crate::mapping::assign_slots;
use crate::pipeline::{
    build_preview, build_program, select_slot, PreviewPipeline, ProgramPipeline, Source,
};
use anyhow::Result;
use temple::{Ball, Receiver as TempleReceiver, BALL_EVICTION_SECS};
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

pub struct Daemon {
    cfg: ReportConfig,
    state: Arc<Mutex<DaemonState>>,
}

struct DaemonState {
    sources_in_order: Vec<Source>,
    active_slot: Option<u8>,
    program: Option<ProgramPipeline>,
    preview: Option<PreviewPipeline>,
}

#[derive(Debug)]
enum BusEvent {
    Error { which: &'static str, msg: String },
    Eos { which: &'static str },
}

impl Daemon {
    pub fn new(cfg: ReportConfig) -> Self {
        Self {
            cfg,
            state: Arc::new(Mutex::new(DaemonState {
                sources_in_order: Vec::new(),
                active_slot: None,
                program: None,
                preview: None,
            })),
        }
    }

    pub fn run(self) -> Result<()> {
        gstreamer::init()?;

        let shutdown = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(signal_hook::consts::SIGTERM, shutdown.clone())?;
        signal_hook::flag::register(signal_hook::consts::SIGINT, shutdown.clone())?;

        let (src_tx, src_rx) = channel::<Vec<Source>>();
        let (key_tx, key_rx) = channel::<u8>();
        let (bus_tx, bus_rx) = channel::<BusEvent>();

        let group = self.cfg.temple_group;
        let port = self.cfg.temple_port;
        let eviction = Duration::from_secs(BALL_EVICTION_SECS);
        std::thread::Builder::new()
            .name("report-temple-rx".into())
            .spawn(move || {
                let mut rx = match TempleReceiver::new(group, port, eviction) {
                    Ok(r) => r,
                    Err(e) => {
                        error!(error = ?e, "temple receiver init failed; thread exiting");
                        return;
                    }
                };
                loop {
                    match rx.poll(Duration::from_secs(1)) {
                        Ok(true) => {
                            let sources = balls_to_sources(rx.snapshot());
                            if src_tx.send(sources).is_err() {
                                break;
                            }
                        }
                        Ok(false) => {}
                        Err(e) => {
                            warn!(error = ?e, "temple poll error");
                        }
                    }
                }
            })?;

        let kbd_device = self.cfg.keyboard_device.clone();
        std::thread::Builder::new()
            .name("report-keyboard".into())
            .spawn(move || {
                if let Err(e) = crate::input::run_keyboard_loop(&kbd_device, key_tx) {
                    warn!(error = ?e, "keyboard loop exited");
                }
            })?;

        self.event_loop(src_rx, key_rx, bus_rx, bus_tx, shutdown)
    }

    fn event_loop(
        &self,
        src_rx: Receiver<Vec<Source>>,
        key_rx: Receiver<u8>,
        bus_rx: Receiver<BusEvent>,
        bus_tx: Sender<BusEvent>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<()> {
        while !shutdown.load(Ordering::Relaxed) {
            if let Ok(sources) = src_rx.recv_timeout(Duration::from_millis(50)) {
                self.on_sources_changed(sources, &bus_tx)?;
            }
            while let Ok(slot) = key_rx.try_recv() {
                self.handle_keypress(slot)?;
            }
            let mut needs_rebuild = false;
            while let Ok(event) = bus_rx.try_recv() {
                match event {
                    BusEvent::Error { which, msg } => {
                        error!(pipeline = which, %msg, "pipeline error from bus");
                    }
                    BusEvent::Eos { which } => {
                        warn!(pipeline = which, "pipeline EOS from bus");
                    }
                }
                needs_rebuild = true;
            }
            if needs_rebuild {
                if let Err(e) = self.force_rebuild(&bus_tx) {
                    error!(error = ?e, "rebuild after bus event failed");
                }
            }
        }

        info!("shutdown signal received — tearing down pipelines");
        let mut st = self.state.lock();
        if let Some(p) = st.program.take() {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }
        if let Some(p) = st.preview.take() {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }
        Ok(())
    }

    fn on_sources_changed(&self, raw_sources: Vec<Source>, bus_tx: &Sender<BusEvent>) -> Result<()> {
        let names: Vec<String> = raw_sources.iter().map(|s| s.name.clone()).collect();
        let mapping = assign_slots(&names, &self.cfg.source_slot_overrides);
        let max = mapping.values().copied().max().unwrap_or(0);
        let mut ordered: Vec<Option<Source>> = vec![None; max as usize];
        for src in &raw_sources {
            if let Some(&slot) = mapping.get(&src.name) {
                ordered[(slot - 1) as usize] = Some(src.clone());
            }
        }
        let new_sources: Vec<Source> = ordered.into_iter().flatten().collect();

        {
            let st = self.state.lock();
            if new_sources == st.sources_in_order {
                return Ok(());
            }
        }
        info!(new = ?new_sources.iter().map(|s| &s.name).collect::<Vec<_>>(), "sources changed");
        self.install_pipelines(&new_sources, bus_tx)
    }

    fn force_rebuild(&self, bus_tx: &Sender<BusEvent>) -> Result<()> {
        let sources = self.state.lock().sources_in_order.clone();
        info!(?sources, "forced rebuild after bus event");
        self.install_pipelines(&sources, bus_tx)
    }

    fn install_pipelines(&self, sources: &[Source], bus_tx: &Sender<BusEvent>) -> Result<()> {
        let (old_program, old_preview) = {
            let mut st = self.state.lock();
            st.sources_in_order = sources.to_vec();
            st.active_slot = if sources.is_empty() { None } else { Some(1) };
            (st.program.take(), st.preview.take())
        };

        if let Some(p) = old_program {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }
        if let Some(p) = old_preview {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }

        let state_for_tally = self.state.clone();
        let preview = build_preview(
            sources,
            self.cfg.preview_connector_id,
            Arc::new(move || state_for_tally.lock().active_slot),
        )?;
        spawn_bus_watch("preview", &preview.pipeline, bus_tx.clone())?;
        preview.pipeline.set_state(gstreamer::State::Playing)?;

        if sources.is_empty() {
            let mut st = self.state.lock();
            st.preview = Some(preview);
            return Ok(());
        }

        let program = build_program(sources, self.cfg.program_connector_id)?;
        spawn_bus_watch("program", &program.pipeline, bus_tx.clone())?;
        program.pipeline.set_state(gstreamer::State::Playing)?;
        let _ = select_slot(&program.selector, 0);

        let mut st = self.state.lock();
        st.preview = Some(preview);
        st.program = Some(program);
        Ok(())
    }

    fn handle_keypress(&self, slot: u8) -> Result<()> {
        let mut st = self.state.lock();
        if st.program.is_none() {
            return Ok(());
        }
        if (slot as usize) > st.sources_in_order.len() || slot == 0 {
            return Ok(());
        }
        info!(slot, source = %st.sources_in_order[(slot - 1) as usize].name, "cut");
        st.active_slot = Some(slot);
        if let Some(program) = st.program.as_ref() {
            select_slot(&program.selector, (slot - 1) as usize)?;
        }
        Ok(())
    }
}

/// Convert balls into pipeline-ready Source records, keeping only PRECOG-named
/// entries (defense in depth — non-PRECOG balls should not reach this channel
/// in production).
fn balls_to_sources(balls: Vec<Ball>) -> Vec<Source> {
    balls
        .into_iter()
        .filter_map(|b| match b {
            Ball::V1(v) if v.name.starts_with("PRECOG-") => Some(Source {
                name: v.name,
                mcast: v.rtp.mcast,
                port: v.rtp.port,
                payload_type: v.rtp.pt,
                clock_rate: v.rtp.clock_rate,
                encoding_name: v.rtp.encoding_name,
            }),
            _ => None,
        })
        .collect()
}

fn spawn_bus_watch(
    which: &'static str,
    pipeline: &gstreamer::Pipeline,
    tx: Sender<BusEvent>,
) -> Result<()> {
    let bus = pipeline
        .bus()
        .ok_or_else(|| anyhow::anyhow!("no bus on pipeline"))?;
    std::thread::Builder::new()
        .name(format!("report-bus-{which}"))
        .spawn(move || {
            for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
                use gstreamer::MessageView;
                match msg.view() {
                    MessageView::Error(e) => {
                        let _ = tx.send(BusEvent::Error { which, msg: e.error().to_string() });
                        break;
                    }
                    MessageView::Eos(..) => {
                        let _ = tx.send(BusEvent::Eos { which });
                        break;
                    }
                    _ => {}
                }
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use temple::{BallV1, RtpInfo, VideoInfo};

    fn ball(name: &str, mcast: &str) -> Ball {
        Ball::V1(BallV1 {
            name: name.into(),
            host: "10.0.0.1".into(),
            rtp: RtpInfo {
                mcast: mcast.into(),
                port: 5000,
                pt: 96,
                clock_rate: 90000,
                encoding_name: "H264".into(),
            },
            video: VideoInfo {
                width: 1920,
                height: 1080,
                framerate: "30/1".into(),
            },
        })
    }

    #[test]
    fn balls_to_sources_keeps_precog_names() {
        let sources = balls_to_sources(vec![
            ball("PRECOG-01-X", "239.42.1.1"),
            ball("OTHER-DEVICE", "239.42.1.2"),
        ]);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "PRECOG-01-X");
        assert_eq!(sources[0].mcast, "239.42.1.1");
        assert_eq!(sources[0].payload_type, 96);
    }

    #[test]
    fn balls_to_sources_empty_when_no_balls() {
        assert!(balls_to_sources(Vec::new()).is_empty());
    }
}
```

- [ ] **Step 2: Drop the stub from `report/src/lib.rs`**

Open `report/src/lib.rs`. Remove the entire `pub(crate) mod ndi_find { ... }` block from Task 7. Also remove `pub mod naming;` (no longer used). Confirm `lib.rs` exposes only: `config`, `daemon`, `input`, `mapping`, `pipeline`.

- [ ] **Step 3: Delete `report/src/naming.rs`**

```bash
rm report/src/naming.rs
```

PRECOG-name filtering moved into `balls_to_sources`. The " (Channel N)" suffix stripping was NDI-specific; balls carry clean names.

- [ ] **Step 4: Run the full report test suite**

Run: `cargo test -p report`
Expected: all tests pass (2 config + 5 pipeline + 2 daemon ball tests).

- [ ] **Step 5: Build the full workspace**

Run: `cargo build --workspace`
Expected: PASS.

- [ ] **Step 6: Clippy + fmt**

Run: `cargo clippy --workspace --all-targets -- -W clippy::pedantic`
Expected: no errors (pedantic warnings acceptable per workspace lint config).

Run: `cargo fmt --all`
Expected: no diff.

- [ ] **Step 7: Commit the full switchover**

```bash
git add -A
git commit -m "report: replace NDI find + ndisrc with temple ball + RTP/UDP pipelines"
```

---

## Task 11: macOS dev smoke — local loopback end-to-end

**Files:**
- Create: `docs/runbooks/2026-05-17-rtp-mac-smoke.md`

This task is manual verification + a runbook commit.

- [ ] **Step 1: Write the runbook**

```markdown
# Local RTP+Multicast Smoke Test (macOS)

Verifies precog → RTP/multicast → standalone gst-launch receiver without a Pi.

## Prereqs
- gstreamer + plugins-good/bad/ugly + libav installed via Homebrew:
  `brew install gstreamer gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav`
- `cargo build --workspace` succeeds.
- Loopback interface allows multicast (one-time per boot if missing):
  `sudo route -n add -net 239 -interface lo0`

## Procedure

Terminal A — precog publishes the built-in webcam as PRECOG-99-MAC-TEST:

```bash
cat > /tmp/precog-mac.toml <<'EOF'
source_name = "PRECOG-99-MAC-TEST"
device = "0"
format = "UYVY"
width = 1280
height = 720
framerate = "30/1"
rtp_mcast = "239.42.1.99"
rtp_port = 5000
EOF
PRECOG_CONFIG=/tmp/precog-mac.toml RUST_LOG=info cargo run -p precog
```

Expect: log line "PRECOG starting"; webcam LED on.

Terminal B — gst-launch standalone RTP receiver:

```bash
gst-launch-1.0 -v \
  udpsrc address=239.42.1.99 port=5000 auto-multicast=true \
  caps="application/x-rtp,media=video,clock-rate=90000,encoding-name=H264,payload=96" ! \
  rtpjitterbuffer latency=20 ! \
  rtph264depay ! h264parse ! avdec_h264 ! \
  videoconvert ! osxvideosink
```

Expect: webcam frames appear within ~3 seconds of `PLAYING`.

Terminal C — verify ball is being emitted:

```bash
gst-launch-1.0 -v \
  udpsrc address=239.42.0.1 port=9999 auto-multicast=true ! \
  fakesink dump=true
```

Expect: hex dump containing `"name":"PRECOG-99-MAC-TEST"` every ~2 seconds.

## Failure modes
- udpsink "Could not get/set settings from/on resource": macOS multicast route missing — see prereqs.
- No frames in B but precog log clean: `tcpdump -i lo0 -nn udp port 5000`. Zero packets = route issue; packets present but no frames = caps mismatch (verify width/height/framerate).
- Ball visible (C) but no RTP (B): per-source `rtp_mcast`/`rtp_port` mismatch between TOML and the udpsrc line.
```

- [ ] **Step 2: Commit the runbook**

```bash
mkdir -p docs/runbooks
git add docs/runbooks/
git commit -m "docs: macOS smoke runbook for RTP+multicast loopback"
```

- [ ] **Step 3: Walk through the runbook on the mac**

Execute every step. Confirm: precog logs `udpsink host=239.42.1.99 port=5000`; gst-launch in Terminal B shows webcam; Terminal C dumps ball JSON.

**Architectural gate:** if local loopback doesn't render video, no Pi deploy will. Do not proceed to Task 12 until this passes.

---

## Task 12: Update sample configs, ROADMAP, mark NDI specs superseded

**Files:**
- Modify: `.docs/ROADMAP.md`
- Modify: `docs/specs/2026-05-16-*.md` (banner only)

- [ ] **Step 1: Find lingering NDI references**

```bash
grep -rn "ndi_name\|NDI HX\|ndisink\|ndisrc\|PRECRIME_NDI_STUB" \
  --include="*.rs" --include="*.toml" --include="*.md" \
  --exclude-dir=target --exclude-dir=.git .
```

Expected: hits only in `docs/specs/2026-05-16-*.md` and `.docs/ROADMAP.md`.

- [ ] **Step 2: Update `.docs/ROADMAP.md`**

In `## Design Notes`, replace the **NDI:** bullet with:

```markdown
- **Transport:** Every camera publishes as H.264 over RTP on a per-source IPv4 multicast group (`239.42.x.y`, TTL=1). Temple via JSON ball on `239.42.0.1:9999`. Source names: `PRECOG-NN-<TYPE>-<LOC>`.
```

In `## Recently Shipped`, add at the top:

```markdown
- RTP+multicast transport replaces NDI — FOSS-clean, ~80ms LAN latency
```

(Roll the oldest entry to `COMPLETED.md` if the list exceeds 3.)

In `## Execution Order`, add a phase row for the migration:

```markdown
### Phase 1.5 — NDI → RTP/multicast migration ✅

| Step | Description | Files |
|---|---|---|
| ✅ 1.5.1 | `temple` workspace crate (ball + send + recv) | `temple/` |
| ✅ 1.5.2 | precog: RTP/multicast pipeline + ball thread | `precog/src/{config,main}.rs` |
| ✅ 1.5.3 | report: temple receiver + RTP pipelines | `report/src/{config,daemon,pipeline}.rs` |
| ✅ 1.5.4 | macOS dev smoke | `docs/runbooks/2026-05-17-rtp-mac-smoke.md` |
```

- [ ] **Step 3: Prepend superseded banner to NDI-era specs**

For each of `docs/specs/2026-05-16-precog-kit-a-cctv.md`, `docs/specs/2026-05-16-precog-kit-b-iphone.md`, `docs/specs/2026-05-16-report-switcher.md`, `docs/specs/2026-05-16-precrime-system-design.md`, prepend:

```markdown
> **STATUS — SUPERSEDED 2026-05-17:** NDI transport replaced by RTP+multicast. See `docs/plans/2026-05-17-rtp-multicast-migration.md` for the current architecture. NDI-specific details below are retained for historical context only.
```

Banner only — do not edit the bodies.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "docs: mark NDI-era specs superseded; ROADMAP notes RTP transport"
```

---

## Task 13: On-Pi verification milestones (manual, deferred to hardware)

No code in this task — it's the integration checklist that supersedes the existing roadmap M3–M9 milestones.

- [ ] **M3':** precog on a Pi 5; `videotestsrc` → RTP confirmed via `tcpdump -i eth0 -nn host 239.42.1.99 and udp port 5000` from a laptop on the same switch.
- [ ] **M4':** real CCTV via EasyCap → RTP confirmed by playing back from the laptop with the gst-launch line from Task 11.
- [ ] **M5':** report on a second Pi 5; HDMI program output shows precog frames within 3s of both daemons starting.
- [ ] **M6':** multiview tally overlay survives the transport change unchanged (compositor + cairooverlay are untouched).
- [ ] **M7':** keyboard slots 1..N switch program output. Switch latency ≤200ms.
- [ ] **M9':** two PRECOGs publishing to different multicast groups; report discovers both via ball and shows both tiles in multiview.
- [ ] **Soak:** 2-hour run with no manual intervention. Watch for `rtpjitterbuffer` underrun warnings in journald.

Mark each ✅ in `.docs/ROADMAP.md` as it completes.

---

## Post-merge cleanup checklist

After Task 13 lands on hardware:

- [ ] Remove `libndi*` packages from any provisioning scripts / Ansible / Pi image recipes.
- [ ] Remove NDI SDK from developer-machine setup docs (`README.md`, any onboarding notes).
- [ ] Decide on a phone-publisher app for Phase 3. Options:
  - **Moblin** — open source iOS, SRT/RTMP. Aligns with FOSS goal but needs a thin SRT→RTP-multicast bridge daemon on the Flint (or a Pi) since report only speaks RTP/multicast directly.
  - **Larix Broadcaster** — freeware iOS/Android, SRT/RTMP/WHIP. Same bridge requirement.
  - Plan that bridge as a separate doc (`docs/plans/YYYY-MM-DD-phone-bridge.md`); it's not part of this migration.

---

## Self-review notes

- **Spec coverage:** transport (Tasks 5–6, 9), temple (Tasks 2–4, 10), library removal (Task 7), config rename (Tasks 5, 8), tests at every layer, manual smoke (Task 11), docs (Task 12).
- **Type consistency:** `Source` defined Task 9, used Task 10. `Ball::V1` + `RtpInfo` + `VideoInfo` defined Task 2, used Tasks 3–4, 6, 10. Field names match across the wire (`mcast`, `port`, `pt`, `clock_rate`, `encoding_name`).
- **Atomicity:** Tasks 7 and 9 leave the tree in a deliberately half-built state. Task 7 commits a temporary stub so the repo builds; Task 10 removes the stub and ships the working switchover. The atomic landing point is Task 10's commit.
- **Order dependencies:** Tasks 1→2→3→4 build the temple crate from the inside out. Task 5 must precede Task 6 (config types referenced in main.rs). Tasks 7–10 must run in order on the report side. Task 11 gates Tasks 12–13.
