# Hardware Monitor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** REPORT collects CPU temp/load/memory/WiFi RSSI from each PRECOG Pi, WITNESS iPhone battery+thermal, and its own hardware — renders live stats on the HDMI preview overlay and logs every 30s to journald, with log rotation to protect the Pi SD card.

**Architecture:** Stats travel via the existing TEMPLE Ball (extended with `hw: Option<HwStats>`) for PRECOGs; WITNESS sends a UDP stats push to a new `stats_port` communicated in the registration response. REPORT maintains a separate `HashMap<String, HwStats>` (not merged into `Source`) to preserve `Source`'s `Eq` derive. The cairo overlay callback gains two new closures for PRECOG stats and REPORT self-stats. A new `hw-stats` workspace crate holds the system-file parsers shared by `precog` and `report`.

**Tech Stack:** Rust (Cargo workspace), GStreamer `cairooverlay`, `temple` UDP multicast, iOS Swift with `Network.framework`, POSIX `/proc` + `/sys` file reads.

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `hw-stats/Cargo.toml` | New workspace crate |
| Create | `hw-stats/src/lib.rs` | CPU/mem/wifi parsers + `CpuSnapshot` |
| Modify | `Cargo.toml` | Add `hw-stats` workspace member |
| Modify | `temple/src/ball.rs` | Add `HwStats`, `WitnessStats`, `ThermalState`, `WitnessStatsPacket`; extend `BallV1` |
| Modify | `temple/src/lib.rs` | Re-export new types |
| Modify | `precog/Cargo.toml` | Add `hw-stats` dep |
| Modify | `precog/src/main.rs` | Dynamic ball rebuild with hw stats |
| Modify | `report/Cargo.toml` | Add `hw-stats` dep |
| Modify | `report/src/config.rs` | Add `stats_port` field |
| Modify | `report/report.conf.example` | Document `stats_port` |
| Modify | `report/src/registration.rs` | Add `stats_port` to response + spawn signature |
| Modify | `report/src/daemon.rs` | New state fields, threads, closures to `build_preview` |
| Create | `report/src/witness_stats.rs` | WITNESS UDP stats listener |
| Create | `report/src/stats_log.rs` | 30s periodic stats logger |
| Modify | `report/src/pipeline.rs` | `build_preview` gains `get_hw_stats`+`get_self_hw`; cairo text overlay |
| Create | `report/journald-precrime.conf` | Log rotation drop-in |
| Modify | `report/install.sh` | Install journald drop-in |
| Modify | `ios/Witness/Witness/Network/RegistrationClient.swift` | Add `statsPort` to `Registration` |
| Modify | `ios/Witness/scripts/mock_report.py` | Add `stats_port` to mock response |
| Create | `ios/Witness/Witness/Network/StatsPublisher.swift` | Battery+thermal UDP publisher |
| Modify | `ios/Witness/Witness/Util/AppModel.swift` | `StatsPublisher` lifecycle |

---

## Task 1: Temple — HwStats, WitnessStats, ThermalState types

**Files:**
- Modify: `temple/src/ball.rs`
- Modify: `temple/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Add to the bottom of `temple/src/ball.rs` `#[cfg(test)] mod tests` block (after the existing tests):

```rust
#[test]
fn hw_stats_round_trips_json() {
    let hw = HwStats {
        cpu_temp_mc: 42300,
        cpu_load_pct: 67,
        mem_used_mb: 280,
        mem_total_mb: 480,
        wifi_rssi_dbm: Some(-54),
    };
    let json = serde_json::to_string(&hw).unwrap();
    let parsed: HwStats = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.cpu_temp_mc, 42300);
    assert_eq!(parsed.wifi_rssi_dbm, Some(-54));
}

#[test]
fn hw_stats_omits_rssi_when_none() {
    let hw = HwStats {
        cpu_temp_mc: 50000,
        cpu_load_pct: 10,
        mem_used_mb: 100,
        mem_total_mb: 1000,
        wifi_rssi_dbm: None,
    };
    let json = serde_json::to_string(&hw).unwrap();
    assert!(!json.contains("wifi_rssi_dbm"), "should be omitted when None, got {json}");
}

#[test]
fn witness_stats_round_trips_json() {
    let pkt = WitnessStatsPacket {
        name: "WITNESS-STAGE".into(),
        stats: WitnessStats {
            battery_pct: 84,
            charging: false,
            thermal: ThermalState::Nominal,
        },
    };
    let json = serde_json::to_string(&pkt).unwrap();
    let parsed: WitnessStatsPacket = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "WITNESS-STAGE");
    assert_eq!(parsed.stats.battery_pct, 84);
    assert!(!parsed.stats.charging);
}

#[test]
fn thermal_state_serialises_as_pascal_case() {
    let s = serde_json::to_string(&ThermalState::Serious).unwrap();
    assert_eq!(s, r#""Serious""#);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd /Users/cody/Dev/precrime && cargo test -p temple 2>&1 | tail -20
```

Expected: compile errors — `HwStats`, `WitnessStats`, etc. not found.

- [ ] **Step 3: Add the new types to ball.rs**

In `temple/src/ball.rs`, add after the `VideoInfo` struct (before the `impl Ball` block):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HwStats {
    pub cpu_temp_mc: u32,
    pub cpu_load_pct: u8,
    pub mem_used_mb: u32,
    pub mem_total_mb: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_rssi_dbm: Option<i16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThermalState {
    Nominal,
    Fair,
    Serious,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessStats {
    pub battery_pct: u8,
    pub charging: bool,
    pub thermal: ThermalState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessStatsPacket {
    pub name: String,
    pub stats: WitnessStats,
}
```

- [ ] **Step 4: Re-export from temple/src/lib.rs**

In `temple/src/lib.rs`, update the `pub use ball::...` line:

```rust
pub use ball::{Ball, BallV1, HwStats, RtpInfo, ThermalState, VideoInfo, WitnessStats, WitnessStatsPacket};
```

- [ ] **Step 5: Run tests to confirm they pass**

```bash
cd /Users/cody/Dev/precrime && cargo test -p temple 2>&1 | tail -20
```

Expected: all tests pass including the 4 new ones.

- [ ] **Step 6: Commit**

```bash
git add temple/src/ball.rs temple/src/lib.rs
git commit -m "feat(temple): add HwStats, WitnessStats, ThermalState wire types"
```

---

## Task 2: Temple — Extend BallV1 with hw field

**Files:**
- Modify: `temple/src/ball.rs`

- [ ] **Step 1: Write the failing tests**

Add to the test block in `temple/src/ball.rs`:

```rust
#[test]
fn ballv1_with_hw_round_trips() {
    let b = Ball::V1(BallV1 {
        name: "PRECOG-01".into(),
        host: "10.0.0.1".into(),
        rtp: RtpInfo {
            mcast: "239.42.1.1".into(),
            port: 5000,
            pt: 96,
            clock_rate: 90000,
            encoding_name: "H264".into(),
        },
        video: VideoInfo { width: 1920, height: 1080, framerate: "30/1".into() },
        hw: Some(HwStats {
            cpu_temp_mc: 42300,
            cpu_load_pct: 67,
            mem_used_mb: 280,
            mem_total_mb: 480,
            wifi_rssi_dbm: Some(-54),
        }),
    });
    let bytes = b.to_json().unwrap();
    let parsed = Ball::from_json(&bytes).unwrap();
    assert_eq!(b, parsed);
}

#[test]
fn ballv1_without_hw_is_backward_compatible() {
    // A ball JSON produced by an old PRECOG (no hw field) must parse cleanly.
    let raw = br#"{"v":"1","name":"PRECOG-01","host":"10.0.0.1","rtp":{"mcast":"239.42.1.1","port":5000,"pt":96,"clock_rate":90000,"encoding_name":"H264"},"video":{"width":1920,"height":1080,"framerate":"30/1"}}"#;
    let b = Ball::from_json(raw).unwrap();
    if let Ball::V1(v) = b {
        assert!(v.hw.is_none());
    } else {
        panic!("expected V1");
    }
}

#[test]
fn ballv1_hw_omitted_from_json_when_none() {
    let b = BallV1 {
        name: "PRECOG-01".into(),
        host: "10.0.0.1".into(),
        rtp: RtpInfo { mcast: "239.42.1.1".into(), port: 5000, pt: 96, clock_rate: 90000, encoding_name: "H264".into() },
        video: VideoInfo { width: 1920, height: 1080, framerate: "30/1".into() },
        hw: None,
    };
    let ball = Ball::V1(b);
    let json = std::str::from_utf8(&ball.to_json().unwrap()).unwrap().to_owned();
    assert!(!json.contains("hw"), "hw should be absent when None, got {json}");
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd /Users/cody/Dev/precrime && cargo test -p temple 2>&1 | tail -20
```

Expected: compile errors — `BallV1` has no field `hw`.

- [ ] **Step 3: Add the hw field to BallV1**

In `temple/src/ball.rs`, update the `BallV1` struct:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BallV1 {
    pub name: String,
    pub host: String,
    pub rtp: RtpInfo,
    pub video: VideoInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hw: Option<HwStats>,
}
```

- [ ] **Step 4: Fix existing test that constructs BallV1**

In `temple/src/ball.rs`, in the `fn sample()` helper inside `#[cfg(test)]`:

```rust
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
        hw: None,
    })
}
```

Also update `report/src/daemon.rs` — the `fn ball()` test helper in `mod tests` at the bottom:

```rust
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
        hw: None,
    })
}
```

And update `precog/src/main.rs` — in `mod tests`, the `fn cfg()` helper builds a config, not a Ball directly. But `spawn_ball_thread` builds a Ball internally. No test helper changes needed there yet (Task 5 updates that code).

- [ ] **Step 5: Run the full test suite**

```bash
cd /Users/cody/Dev/precrime && cargo test 2>&1 | tail -30
```

Expected: all tests pass. Any `BallV1` struct literal missing the `hw` field will be a compile error — fix them to `hw: None`.

- [ ] **Step 6: Commit**

```bash
git add temple/src/ball.rs temple/src/lib.rs report/src/daemon.rs
git commit -m "feat(temple): extend BallV1 with optional hw stats field"
```

---

## Task 3: hw-stats crate

**Files:**
- Create: `hw-stats/Cargo.toml`
- Create: `hw-stats/src/lib.rs`

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "hw-stats"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "MIT OR Apache-2.0"
authors = ["Cody <byrnes.cody@gmail.com>"]
description = "PRECRIME: Pi hardware stats readers (CPU temp/load, memory, WiFi RSSI)"

[lints]
workspace = true
```

- [ ] **Step 2: Write the failing tests in hw-stats/src/lib.rs**

Create `hw-stats/src/lib.rs`:

```rust
pub struct CpuSnapshot {
    pub idle: u64,
    pub total: u64,
}

impl Default for CpuSnapshot {
    fn default() -> Self {
        Self { idle: 0, total: 0 }
    }
}

impl Clone for CpuSnapshot {
    fn clone(&self) -> Self {
        Self { idle: self.idle, total: self.total }
    }
}

// ── Public read API ──────────────────────────────────────────────────────────

pub fn read_cpu_temp_mc() -> Option<u32> {
    let s = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok()?;
    parse_cpu_temp_mc(&s)
}

pub fn read_cpu_load_pct(prev: &CpuSnapshot) -> (u8, CpuSnapshot) {
    let content = std::fs::read_to_string("/proc/stat").ok();
    let current = content
        .as_deref()
        .and_then(parse_cpu_snapshot)
        .unwrap_or(CpuSnapshot {
            idle: prev.idle,
            total: prev.total.saturating_add(1),
        });
    compute_load(prev, &current)
}

pub fn read_mem_mb() -> Option<(u32, u32)> {
    let s = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_mem_mb(&s)
}

pub fn read_wifi_rssi_dbm() -> Option<i16> {
    let s = std::fs::read_to_string("/proc/net/wireless").ok()?;
    parse_wifi_rssi(&s)
}

// ── Parsers (pub(crate) for testing) ─────────────────────────────────────────

pub(crate) fn parse_cpu_temp_mc(s: &str) -> Option<u32> {
    s.trim().parse().ok()
}

pub(crate) fn parse_cpu_snapshot(s: &str) -> Option<CpuSnapshot> {
    let line = s.lines().find(|l| l.starts_with("cpu "))?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|t| t.parse().ok())
        .collect();
    let idle = *fields.get(3)?;
    let total: u64 = fields.iter().sum();
    Some(CpuSnapshot { idle, total })
}

pub(crate) fn compute_load(prev: &CpuSnapshot, current: &CpuSnapshot) -> (u8, CpuSnapshot) {
    let d_total = current.total.saturating_sub(prev.total);
    let d_idle = current.idle.saturating_sub(prev.idle);
    let load = if d_total == 0 {
        0u8
    } else {
        ((100 * (d_total - d_idle)) / d_total).min(100) as u8
    };
    (load, current.clone())
}

pub(crate) fn parse_mem_mb(s: &str) -> Option<(u32, u32)> {
    let mut total_kb: Option<u64> = None;
    let mut avail_kb: Option<u64> = None;
    for line in s.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        } else if line.starts_with("MemAvailable:") {
            avail_kb = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        }
        if total_kb.is_some() && avail_kb.is_some() {
            break;
        }
    }
    let t = total_kb?;
    let a = avail_kb?;
    Some(((t - a) as u32 / 1024, t as u32 / 1024))
}

pub(crate) fn parse_wifi_rssi(s: &str) -> Option<i16> {
    // /proc/net/wireless has 2 header lines then data lines:
    // " wlan0: 0000   60.  -51.  -256. ..."
    // tokens[0]=iface, [1]=status, [2]=link, [3]=level (signal dBm)
    let line = s.lines().nth(2)?;
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let raw: i32 = tokens.get(3)?.trim_end_matches('.').parse().ok()?;
    // Old kernels report unsigned 0–255 where actual_dBm = raw - 256 for raw > 127
    let dbm = if raw > 0 { raw - 256 } else { raw };
    Some(dbm as i16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cpu_temp_mc_parses_millidegrees() {
        assert_eq!(parse_cpu_temp_mc("42300\n"), Some(42300));
        assert_eq!(parse_cpu_temp_mc("  51000  "), Some(51000));
        assert_eq!(parse_cpu_temp_mc("not_a_number\n"), None);
    }

    #[test]
    fn parse_cpu_snapshot_extracts_idle_and_total() {
        let proc_stat = "cpu  100 20 30 200 10 5 5 0 0 0\n";
        // fields: user=100 nice=20 system=30 idle=200 iowait=10 irq=5 softirq=5 ...
        // total = 100+20+30+200+10+5+5 = 370, idle = 200
        let snap = parse_cpu_snapshot(proc_stat).unwrap();
        assert_eq!(snap.idle, 200);
        assert_eq!(snap.total, 370);
    }

    #[test]
    fn compute_load_calculates_correct_percentage() {
        let prev = CpuSnapshot { idle: 200, total: 370 };
        // After: idle+20, total+100 → d_total=100, d_idle=20, busy=80 → 80%
        let current = CpuSnapshot { idle: 220, total: 470 };
        let (pct, _) = compute_load(&prev, &current);
        assert_eq!(pct, 80);
    }

    #[test]
    fn compute_load_zero_when_no_delta() {
        let snap = CpuSnapshot { idle: 100, total: 200 };
        let (pct, _) = compute_load(&snap, &snap.clone());
        assert_eq!(pct, 0);
    }

    #[test]
    fn parse_mem_mb_extracts_used_and_total() {
        let meminfo = "\
MemTotal:        2048000 kB\n\
MemFree:          512000 kB\n\
MemAvailable:     768000 kB\n\
Buffers:           64000 kB\n";
        // total=2048000 kB=2000 MB, avail=768000 kB=750 MB, used=2000-750=1250 MB
        let (used, total) = parse_mem_mb(meminfo).unwrap();
        assert_eq!(total, 2000);
        assert_eq!(used, 1250);
    }

    #[test]
    fn parse_mem_mb_returns_none_on_missing_fields() {
        assert!(parse_mem_mb("MemTotal: 1000 kB\n").is_none()); // no MemAvailable
    }

    #[test]
    fn parse_wifi_rssi_extracts_signal_level() {
        let wireless = "\
Inter-| sta-|   Quality\n\
 face | tus | link level\n\
 wlan0: 0000   60.  -54.  -256.        0\n";
        assert_eq!(parse_wifi_rssi(wireless), Some(-54));
    }

    #[test]
    fn parse_wifi_rssi_normalises_old_kernel_unsigned() {
        // Old kernels: 202 means 202 - 256 = -54 dBm
        let wireless = "\
Inter-| sta-|\n\
 face | tus |\n\
 wlan0: 0000   60.  202.  0.\n";
        assert_eq!(parse_wifi_rssi(wireless), Some(-54));
    }

    #[test]
    fn parse_wifi_rssi_returns_none_on_missing_data() {
        assert!(parse_wifi_rssi("no data here\n").is_none());
    }
}
```

- [ ] **Step 3: Register hw-stats in workspace Cargo.toml**

In `Cargo.toml` (workspace root), update `members`:

```toml
[workspace]
resolver = "2"
members = ["report", "precog", "temple", "hw-stats"]
```

- [ ] **Step 4: Run the tests**

```bash
cd /Users/cody/Dev/precrime && cargo test -p hw-stats 2>&1 | tail -30
```

Expected: all 9 tests pass. If `/proc/stat` doesn't exist (macOS dev), only the integration `read_*` functions return `None` — the parse unit tests still pass.

- [ ] **Step 5: Commit**

```bash
git add hw-stats/ Cargo.toml Cargo.lock
git commit -m "feat: add hw-stats workspace crate — CPU/mem/wifi Pi stats readers"
```

---

## Task 4: PRECOG — dynamic ball rebuild with hw stats

**Files:**
- Modify: `precog/Cargo.toml`
- Modify: `precog/src/main.rs`

- [ ] **Step 1: Add hw-stats dependency**

In `precog/Cargo.toml`, add to `[dependencies]`:

```toml
hw-stats = { path = "../hw-stats" }
```

- [ ] **Step 2: Write a test for the ball-building logic**

Add to `precog/src/main.rs` inside `#[cfg(test)] mod tests`:

```rust
#[test]
fn pipeline_contains_x264enc_with_zerolatency() {
    let s = build_pipeline_string(&cfg());
    assert!(s.contains("x264enc tune=zerolatency speed-preset=ultrafast bitrate=4000"));
}
// (existing tests stay — just confirm nothing broke after the hw-stats import)
```

Actually the existing tests already cover this. The new thing to test is that `build_ball` produces a `Ball::V1` with the correct static fields. Add:

```rust
#[test]
fn build_ball_static_fields_match_config() {
    let c = cfg();
    let snap = hw_stats::CpuSnapshot::default();
    let b = build_ball(&c, &snap).1; // returns (new_snap, ball)
    if let temple::Ball::V1(v) = b {
        assert_eq!(v.name, c.source_name);
        assert_eq!(v.rtp.port, c.rtp_port);
    } else {
        panic!("expected V1");
    }
}
```

- [ ] **Step 3: Refactor spawn_ball_thread to use build_ball helper**

In `precog/src/main.rs`, add this function before `spawn_ball_thread`:

```rust
fn build_ball(cfg: &PrecogConfig, prev_snap: &hw_stats::CpuSnapshot) -> (hw_stats::CpuSnapshot, Ball) {
    let (load, new_snap) = hw_stats::read_cpu_load_pct(prev_snap);
    let hw = temple::HwStats {
        cpu_temp_mc: hw_stats::read_cpu_temp_mc().unwrap_or(0),
        cpu_load_pct: load,
        mem_used_mb: hw_stats::read_mem_mb().map(|(u, _)| u).unwrap_or(0),
        mem_total_mb: hw_stats::read_mem_mb().map(|(_, t)| t).unwrap_or(0),
        wifi_rssi_dbm: hw_stats::read_wifi_rssi_dbm(),
    };
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
        hw: Some(hw),
    });
    (new_snap, ball)
}
```

- [ ] **Step 4: Update spawn_ball_thread to rebuild each iteration**

Replace the existing `spawn_ball_thread` function:

```rust
fn spawn_ball_thread(cfg: &PrecogConfig, shutdown: Arc<AtomicBool>) -> Result<()> {
    let cfg = cfg.clone();  // need owned copy for thread
    let sender =
        BallSender::new(cfg.temple_group, cfg.temple_port).context("create ball sender")?;
    std::thread::Builder::new()
        .name("precog-ball-tx".into())
        .spawn(move || {
            let mut cpu_snap = hw_stats::CpuSnapshot::default();
            while !shutdown.load(Ordering::Relaxed) {
                let (new_snap, ball) = build_ball(&cfg, &cpu_snap);
                cpu_snap = new_snap;
                if let Err(e) = sender.send(&ball) {
                    warn!(error = ?e, "ball send failed");
                }
                std::thread::sleep(Duration::from_secs(BALL_PERIOD_SECS));
            }
        })
        .context("spawn ball thread")?;
    Ok(())
}
```

- [ ] **Step 5: Add use imports**

At the top of `precog/src/main.rs`, the existing imports include:
```rust
use temple::{Ball, BallV1, RtpInfo, Sender as BallSender, VideoInfo, BALL_PERIOD_SECS};
```
Add `HwStats` to that use line:
```rust
use temple::{Ball, BallV1, HwStats, RtpInfo, Sender as BallSender, VideoInfo, BALL_PERIOD_SECS};
```

Also add the `PrecogConfig` clone derive. In `precog/src/config.rs`, the `PrecogConfig` struct needs `Clone`. Update the derive:
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct PrecogConfig {
```

- [ ] **Step 6: Run the tests**

```bash
cd /Users/cody/Dev/precrime && cargo test -p precog 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add precog/Cargo.toml precog/src/main.rs precog/src/config.rs Cargo.lock
git commit -m "feat(precog): embed hw stats in ball — dynamic rebuild each 2s cycle"
```

---

## Task 5: REPORT — config stats_port + report.conf.example

**Files:**
- Modify: `report/src/config.rs`
- Modify: `report/report.conf.example`

- [ ] **Step 1: Write the failing test**

In `report/src/config.rs`, add to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn stats_port_defaults_to_4998() {
    let raw = r#"
program_connector_id = 32
preview_connector_id = 34
keyboard_device = "/dev/input/event0"
"#;
    let c = ReportConfig::from_toml(raw).unwrap();
    assert_eq!(c.stats_port, 4998);
}

#[test]
fn stats_port_can_be_overridden() {
    let raw = r#"
program_connector_id = 32
preview_connector_id = 34
keyboard_device = "/dev/input/event0"
stats_port = 5555
"#;
    let c = ReportConfig::from_toml(raw).unwrap();
    assert_eq!(c.stats_port, 5555);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd /Users/cody/Dev/precrime && cargo test -p report -- config 2>&1 | tail -20
```

Expected: compile error — `ReportConfig` has no field `stats_port`.

- [ ] **Step 3: Add stats_port to ReportConfig**

In `report/src/config.rs`, add to `ReportConfig`:

```rust
/// UDP port for WITNESS hardware stats push. Default: 4998.
#[serde(default = "default_stats_port")]
pub stats_port: u16,
```

And add the default function:

```rust
fn default_stats_port() -> u16 {
    4998
}
```

- [ ] **Step 4: Update report.conf.example**

In `report/report.conf.example`, add after the `ack_port` comment block:

```toml
# /etc/precrime/report.conf — copy and edit per deployment.

program_connector_id = 32   # set per report-switcher plan Task 2 Step 5 output
preview_connector_id = 34
keyboard_device = "/dev/input/event0"

# stats_port = 4998          # UDP port for WITNESS hw stats push (default: 4998)

# Optional: pin specific PRECOG names to specific number keys.
# Sources not listed here fall into the remaining slots alphabetically.
# [source_slot_overrides]
# "PRECOG-01-IPHONE-STAGE" = 1
# "PRECOG-02-CCTV-DOOR" = 2
```

- [ ] **Step 5: Run tests**

```bash
cd /Users/cody/Dev/precrime && cargo test -p report -- config 2>&1 | tail -20
```

Expected: all config tests pass including the 2 new ones.

- [ ] **Step 6: Commit**

```bash
git add report/src/config.rs report/report.conf.example
git commit -m "feat(report): add stats_port config field for WITNESS hw stats UDP receiver"
```

---

## Task 6: REPORT — registration response gains stats_port

**Files:**
- Modify: `report/src/registration.rs`

- [ ] **Step 1: Write the failing test**

The existing tests don't test the TCP wire protocol end-to-end (they test `PortPool` and `RegisteredSources`). Add a unit test for the `RegistrationResponse` serialisation:

```rust
#[test]
fn registration_response_includes_stats_port() {
    let resp = RegistrationResponse {
        assigned_port: 5001,
        report_name: "REPORT-MAIN".into(),
        ack_port: 9998,
        stats_port: 4998,
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"stats_port\":4998"), "got {json}");
    assert!(json.contains("\"assigned_port\":5001"));
}
```

Add this to the `#[cfg(test)] mod tests` block in `report/src/registration.rs`.

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd /Users/cody/Dev/precrime && cargo test -p report -- registration 2>&1 | tail -20
```

Expected: compile error — `RegistrationResponse` has no field `stats_port`.

- [ ] **Step 3: Update RegistrationResponse**

In `report/src/registration.rs`, update the struct:

```rust
#[derive(serde::Serialize)]
struct RegistrationResponse {
    assigned_port: u16,
    report_name: String,
    ack_port: u16,
    stats_port: u16,
}
```

- [ ] **Step 4: Update handle_registration to accept stats_port**

Update `handle_registration` signature:

```rust
fn handle_registration(
    stream: TcpStream,
    registered: Arc<Mutex<RegisteredSources>>,
    change_tx: Sender<()>,
    report_name: String,
    ack_port: u16,
    stats_port: u16,
) {
```

And update the response construction inside `handle_registration`:

```rust
let resp = RegistrationResponse {
    assigned_port: port,
    report_name,
    ack_port,
    stats_port,
};
```

- [ ] **Step 5: Update spawn_registration_server signature**

```rust
pub fn spawn_registration_server(
    reg_port: u16,
    report_name: String,
    ack_port: u16,
    stats_port: u16,
    registered: Arc<Mutex<RegisteredSources>>,
    change_tx: Sender<()>,
) -> Result<()> {
```

Inside the thread's `for stream` loop, update the `spawn` closure:

```rust
let report_name = report_name.clone();
let stats_port = stats_port;
std::thread::Builder::new()
    .name("report-reg-conn".into())
    .spawn(move || {
        handle_registration(
            s,
            registered,
            change_tx,
            report_name,
            ack_port,
            stats_port,
        );
    })
    .ok();
```

- [ ] **Step 6: Fix the call site in daemon.rs**

In `report/src/daemon.rs`, find the call to `spawn_registration_server` and add `stats_port`:

```rust
crate::registration::spawn_registration_server(
    self.cfg.reg_port,
    self.cfg.report_name.clone(),
    self.cfg.ack_port,
    self.cfg.stats_port,
    registered.clone(),
    change_tx.clone(),
)?;
```

- [ ] **Step 7: Run all report tests**

```bash
cd /Users/cody/Dev/precrime && cargo test -p report 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add report/src/registration.rs report/src/daemon.rs
git commit -m "feat(report): include stats_port in WITNESS registration response"
```

---

## Task 7: REPORT — hw_stats map + Daemon struct fields

**Files:**
- Modify: `report/src/daemon.rs`
- Modify: `report/Cargo.toml`

- [ ] **Step 1: Add hw-stats dependency to report**

In `report/Cargo.toml`, add to `[dependencies]`:

```toml
hw-stats = { path = "../hw-stats" }
```

- [ ] **Step 2: Write a test for hw stats extraction from balls**

Add to `#[cfg(test)] mod tests` in `report/src/daemon.rs`:

```rust
#[test]
fn extract_hw_stats_from_balls_returns_map_keyed_by_name() {
    use temple::{Ball, BallV1, HwStats, RtpInfo, VideoInfo};
    let balls = vec![
        Ball::V1(BallV1 {
            name: "PRECOG-01".into(),
            host: "10.0.0.1".into(),
            rtp: RtpInfo { mcast: "239.42.1.1".into(), port: 5000, pt: 96, clock_rate: 90000, encoding_name: "H264".into() },
            video: VideoInfo { width: 1920, height: 1080, framerate: "30/1".into() },
            hw: Some(HwStats { cpu_temp_mc: 42300, cpu_load_pct: 67, mem_used_mb: 280, mem_total_mb: 480, wifi_rssi_dbm: Some(-54) }),
        }),
        Ball::V1(BallV1 {
            name: "PRECOG-02".into(),
            host: "10.0.0.2".into(),
            rtp: RtpInfo { mcast: "239.42.1.2".into(), port: 5000, pt: 96, clock_rate: 90000, encoding_name: "H264".into() },
            video: VideoInfo { width: 1920, height: 1080, framerate: "30/1".into() },
            hw: None,  // old PRECOG without hw stats
        }),
    ];
    let map = extract_hw_stats(&balls);
    assert_eq!(map.len(), 1, "only PRECOG-01 has hw");
    assert_eq!(map["PRECOG-01"].cpu_temp_mc, 42300);
    assert!(!map.contains_key("PRECOG-02"));
}
```

- [ ] **Step 3: Run test to confirm it fails**

```bash
cd /Users/cody/Dev/precrime && cargo test -p report -- extract_hw_stats 2>&1 | tail -20
```

Expected: compile error — `extract_hw_stats` not found.

- [ ] **Step 4: Add hw_stats fields to Daemon + extract_hw_stats helper**

In `report/src/daemon.rs`, add to the imports:

```rust
use hw_stats::CpuSnapshot;
use std::collections::HashMap;
use temple::{HwStats, WitnessStats};
```

Update the `Daemon` struct:

```rust
pub struct Daemon {
    cfg: ReportConfig,
    state: Arc<Mutex<DaemonState>>,
    hw_stats: Arc<Mutex<HashMap<String, HwStats>>>,
    witness_stats: Arc<Mutex<HashMap<String, WitnessStats>>>,
    self_hw: Arc<Mutex<Option<HwStats>>>,
}
```

Update `Daemon::new()`:

```rust
pub fn new(cfg: ReportConfig) -> Self {
    Self {
        cfg,
        state: Arc::new(Mutex::new(DaemonState {
            sources_in_order: Vec::new(),
            active_slot: None,
            program: None,
            preview: None,
        })),
        hw_stats: Arc::new(Mutex::new(HashMap::new())),
        witness_stats: Arc::new(Mutex::new(HashMap::new())),
        self_hw: Arc::new(Mutex::new(None)),
    }
}
```

Add the `extract_hw_stats` function (after `balls_to_sources`):

```rust
fn extract_hw_stats(balls: &[temple::Ball]) -> HashMap<String, HwStats> {
    balls
        .iter()
        .filter_map(|b| match b {
            temple::Ball::V1(v) if v.name.starts_with("PRECOG-") => {
                v.hw.as_ref().map(|hw| (v.name.clone(), hw.clone()))
            }
            _ => None,
        })
        .collect()
}
```

- [ ] **Step 5: Update temple-rx thread to populate hw_stats**

In `Daemon::run()`, find the temple-rx thread spawn. Change:

```rust
let temple_snap_tx = temple_snapshot.clone();
let change_tx_temple = change_tx.clone();
```

to:

```rust
let temple_snap_tx = temple_snapshot.clone();
let hw_stats_tx = self.hw_stats.clone();
let change_tx_temple = change_tx.clone();
```

Inside the `Ok(true)` branch of the temple-rx thread, change:

```rust
Ok(true) => {
    let sources = balls_to_sources(rx.snapshot());
    *temple_snap_tx.lock() = sources;
    if change_tx_temple.send(()).is_err() {
        break;
    }
}
```

to:

```rust
Ok(true) => {
    let balls = rx.snapshot();
    let sources = balls_to_sources(balls.clone());
    let stats = extract_hw_stats(&balls);
    *temple_snap_tx.lock() = sources;
    *hw_stats_tx.lock() = stats;
    if change_tx_temple.send(()).is_err() {
        break;
    }
}
```

- [ ] **Step 6: Run all report tests**

```bash
cd /Users/cody/Dev/precrime && cargo test -p report 2>&1 | tail -30
```

Expected: all tests pass including the new `extract_hw_stats` test.

- [ ] **Step 7: Commit**

```bash
git add report/Cargo.toml report/src/daemon.rs Cargo.lock
git commit -m "feat(report): track PRECOG hw stats from temple balls in hw_stats map"
```

---

## Task 8: REPORT — self-hw polling thread

**Files:**
- Modify: `report/src/daemon.rs`

- [ ] **Step 1: Add the self-hw thread in Daemon::run()**

In `Daemon::run()`, after the eviction thread spawn and before the ack sender spawn, add:

```rust
// ── Self hw-stats thread ──────────────────────────────────────────────────
{
    let self_hw_tx = self.self_hw.clone();
    std::thread::Builder::new()
        .name("report-hw-self".into())
        .spawn(move || {
            let mut cpu_snap = CpuSnapshot::default();
            loop {
                let (load, new_snap) = hw_stats::read_cpu_load_pct(&cpu_snap);
                cpu_snap = new_snap;
                let hw = HwStats {
                    cpu_temp_mc: hw_stats::read_cpu_temp_mc().unwrap_or(0),
                    cpu_load_pct: load,
                    mem_used_mb: hw_stats::read_mem_mb().map(|(u, _)| u).unwrap_or(0),
                    mem_total_mb: hw_stats::read_mem_mb().map(|(_, t)| t).unwrap_or(0),
                    wifi_rssi_dbm: hw_stats::read_wifi_rssi_dbm(),
                };
                *self_hw_tx.lock() = Some(hw);
                std::thread::sleep(Duration::from_secs(2));
            }
        })?;
}
```

- [ ] **Step 2: Compile-check**

```bash
cd /Users/cody/Dev/precrime && cargo build -p report 2>&1 | tail -20
```

Expected: compiles without errors or warnings (clippy may note the thread never exits — that's fine, it runs until process death).

- [ ] **Step 3: Commit**

```bash
git add report/src/daemon.rs
git commit -m "feat(report): poll own CPU temp/load/memory every 2s (report-hw-self thread)"
```

---

## Task 9: REPORT — WITNESS stats UDP listener

**Files:**
- Create: `report/src/witness_stats.rs`
- Modify: `report/src/daemon.rs`

- [ ] **Step 1: Write the failing test**

Create `report/src/witness_stats.rs` with the test only (no implementation yet):

```rust
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use temple::{ThermalState, WitnessStats, WitnessStatsPacket};
use tracing::warn;

pub fn spawn_witness_stats_receiver(
    stats_port: u16,
    witness_stats: Arc<Mutex<HashMap<String, WitnessStats>>>,
) -> anyhow::Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn witness_stats_packet_deserialises() {
        let json = r#"{"name":"WITNESS-STAGE","stats":{"battery_pct":84,"charging":false,"thermal":"Nominal"}}"#;
        let pkt: WitnessStatsPacket = serde_json::from_str(json).unwrap();
        assert_eq!(pkt.name, "WITNESS-STAGE");
        assert_eq!(pkt.stats.battery_pct, 84);
        assert!(!pkt.stats.charging);
        assert_eq!(pkt.stats.thermal, ThermalState::Nominal);
    }
}
```

- [ ] **Step 2: Register the module in daemon.rs**

In `report/src/daemon.rs`, the modules are declared in `main.rs`. Check `report/src/main.rs`:

```bash
cat /Users/cody/Dev/precrime/report/src/main.rs
```

Add `mod witness_stats;` and `mod stats_log;` alongside the other module declarations.

- [ ] **Step 3: Run the failing test**

```bash
cd /Users/cody/Dev/precrime && cargo test -p report -- witness_stats 2>&1 | tail -20
```

Expected: panics with `not yet implemented` from the `todo!()`. The deserialise test itself passes.

- [ ] **Step 4: Implement spawn_witness_stats_receiver**

Replace the `todo!()` in `report/src/witness_stats.rs`:

```rust
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use temple::{ThermalState, WitnessStats, WitnessStatsPacket};
use tracing::{info, warn};

pub fn spawn_witness_stats_receiver(
    stats_port: u16,
    witness_stats: Arc<Mutex<HashMap<String, WitnessStats>>>,
) -> anyhow::Result<()> {
    let sock = UdpSocket::bind(("0.0.0.0", stats_port))?;
    info!(port = stats_port, "witness stats receiver listening");

    std::thread::Builder::new()
        .name("report-witness-stats-rx".into())
        .spawn(move || {
            let mut buf = [0u8; 512];
            loop {
                match sock.recv_from(&mut buf) {
                    Ok((n, _addr)) => match serde_json::from_slice::<WitnessStatsPacket>(&buf[..n]) {
                        Ok(pkt) => {
                            witness_stats
                                .lock()
                                .expect("witness_stats lock")
                                .insert(pkt.name, pkt.stats);
                        }
                        Err(e) => {
                            warn!(error = ?e, "malformed witness stats packet");
                        }
                    },
                    Err(e) => {
                        warn!(error = ?e, "witness stats recv error");
                        break;
                    }
                }
            }
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn witness_stats_packet_deserialises() {
        let json = r#"{"name":"WITNESS-STAGE","stats":{"battery_pct":84,"charging":false,"thermal":"Nominal"}}"#;
        let pkt: WitnessStatsPacket = serde_json::from_str(json).unwrap();
        assert_eq!(pkt.name, "WITNESS-STAGE");
        assert_eq!(pkt.stats.battery_pct, 84);
        assert!(!pkt.stats.charging);
        assert_eq!(pkt.stats.thermal, ThermalState::Nominal);
    }
}
```

- [ ] **Step 5: Spawn the listener in Daemon::run()**

In `report/src/daemon.rs`, after the self-hw thread spawn:

```rust
// ── WITNESS stats UDP listener ────────────────────────────────────────────
crate::witness_stats::spawn_witness_stats_receiver(
    self.cfg.stats_port,
    self.witness_stats.clone(),
)?;
```

- [ ] **Step 6: Run all report tests**

```bash
cd /Users/cody/Dev/precrime && cargo test -p report 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add report/src/witness_stats.rs report/src/daemon.rs
git commit -m "feat(report): WITNESS stats UDP receiver on stats_port"
```

---

## Task 10: REPORT — periodic stats logger

**Files:**
- Create: `report/src/stats_log.rs`
- Modify: `report/src/daemon.rs`

- [ ] **Step 1: Create stats_log.rs**

```rust
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use temple::{HwStats, WitnessStats};
use tracing::info;

pub fn spawn_stats_logger(
    hw_stats: Arc<Mutex<HashMap<String, HwStats>>>,
    witness_stats: Arc<Mutex<HashMap<String, WitnessStats>>>,
    self_hw: Arc<Mutex<Option<HwStats>>>,
    shutdown: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("report-stats-log".into())
        .spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(30));
                log_all(&hw_stats, &witness_stats, &self_hw);
            }
        })?;
    Ok(())
}

fn log_all(
    hw_stats: &Mutex<HashMap<String, HwStats>>,
    witness_stats: &Mutex<HashMap<String, WitnessStats>>,
    self_hw: &Mutex<Option<HwStats>>,
) {
    if let Ok(map) = hw_stats.lock() {
        for (name, hw) in map.iter() {
            info!(
                source = %name,
                temp_c = hw.cpu_temp_mc as f64 / 1000.0,
                cpu_pct = hw.cpu_load_pct,
                mem_used_mb = hw.mem_used_mb,
                mem_total_mb = hw.mem_total_mb,
                rssi_dbm = ?hw.wifi_rssi_dbm,
                "precog hw stats"
            );
        }
    }

    if let Ok(map) = witness_stats.lock() {
        for (name, ws) in map.iter() {
            info!(
                source = %name,
                battery_pct = ws.battery_pct,
                charging = ws.charging,
                thermal = ?ws.thermal,
                "witness stats"
            );
        }
    }

    if let Ok(guard) = self_hw.lock() {
        if let Some(hw) = guard.as_ref() {
            info!(
                source = "REPORT-SELF",
                temp_c = hw.cpu_temp_mc as f64 / 1000.0,
                cpu_pct = hw.cpu_load_pct,
                mem_used_mb = hw.mem_used_mb,
                mem_total_mb = hw.mem_total_mb,
                rssi_dbm = ?hw.wifi_rssi_dbm,
                "report self hw stats"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use temple::ThermalState;

    #[test]
    fn log_all_does_not_panic_with_empty_maps() {
        let hw: Arc<Mutex<HashMap<String, HwStats>>> = Arc::new(Mutex::new(HashMap::new()));
        let ws: Arc<Mutex<HashMap<String, WitnessStats>>> = Arc::new(Mutex::new(HashMap::new()));
        let self_hw: Arc<Mutex<Option<HwStats>>> = Arc::new(Mutex::new(None));
        log_all(&hw, &ws, &self_hw); // must not panic
    }

    #[test]
    fn log_all_does_not_panic_with_populated_data() {
        let hw: Arc<Mutex<HashMap<String, HwStats>>> = Arc::new(Mutex::new({
            let mut m = HashMap::new();
            m.insert("PRECOG-01".into(), HwStats {
                cpu_temp_mc: 42300,
                cpu_load_pct: 67,
                mem_used_mb: 280,
                mem_total_mb: 480,
                wifi_rssi_dbm: Some(-54),
            });
            m
        }));
        let ws: Arc<Mutex<HashMap<String, WitnessStats>>> = Arc::new(Mutex::new({
            let mut m = HashMap::new();
            m.insert("WITNESS-STAGE".into(), WitnessStats {
                battery_pct: 84,
                charging: false,
                thermal: ThermalState::Nominal,
            });
            m
        }));
        let self_hw: Arc<Mutex<Option<HwStats>>> = Arc::new(Mutex::new(Some(HwStats {
            cpu_temp_mc: 51000,
            cpu_load_pct: 23,
            mem_used_mb: 340,
            mem_total_mb: 2000,
            wifi_rssi_dbm: Some(-61),
        })));
        log_all(&hw, &ws, &self_hw); // must not panic
    }
}
```

- [ ] **Step 2: Spawn in Daemon::run()**

In `report/src/daemon.rs`, after the witness stats listener spawn:

```rust
// ── Stats logger thread ───────────────────────────────────────────────────
crate::stats_log::spawn_stats_logger(
    self.hw_stats.clone(),
    self.witness_stats.clone(),
    self.self_hw.clone(),
    shutdown.clone(),
)?;
```

- [ ] **Step 3: Run all tests**

```bash
cd /Users/cody/Dev/precrime && cargo test -p report 2>&1 | tail -20
```

Expected: all tests pass including the 2 new stats_log tests.

- [ ] **Step 4: Commit**

```bash
git add report/src/stats_log.rs report/src/daemon.rs
git commit -m "feat(report): log hw stats every 30s from all connected sources + self"
```

---

## Task 11: REPORT — preview overlay with hw stats text

**Files:**
- Modify: `report/src/pipeline.rs`
- Modify: `report/src/daemon.rs`

- [ ] **Step 1: Write failing tests**

Add to `#[cfg(test)] mod tests` in `report/src/pipeline.rs`:

```rust
#[test]
fn format_hw_line_formats_temp_and_load() {
    use temple::HwStats;
    let hw = HwStats {
        cpu_temp_mc: 42300,
        cpu_load_pct: 67,
        mem_used_mb: 280,
        mem_total_mb: 480,
        wifi_rssi_dbm: Some(-54),
    };
    let line = format_hw_line(&hw);
    assert!(line.contains("42.3"), "temp: {line}");
    assert!(line.contains("67%"), "load: {line}");
    assert!(line.contains("280/480M"), "mem: {line}");
    assert!(line.contains("-54dBm"), "rssi: {line}");
}

#[test]
fn format_hw_line_omits_rssi_when_none() {
    use temple::HwStats;
    let hw = HwStats { cpu_temp_mc: 50000, cpu_load_pct: 10, mem_used_mb: 100, mem_total_mb: 1000, wifi_rssi_dbm: None };
    let line = format_hw_line(&hw);
    assert!(!line.contains("dBm"), "no rssi expected: {line}");
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd /Users/cody/Dev/precrime && cargo test -p report -- format_hw_line 2>&1 | tail -20
```

Expected: compile error — `format_hw_line` not found.

- [ ] **Step 3: Add imports and helper functions to pipeline.rs**

At the top of `report/src/pipeline.rs`, add to the existing use block:

```rust
use std::collections::HashMap;
use temple::HwStats;
```

After the `grid_for` function, add:

```rust
pub(crate) fn format_hw_line(hw: &HwStats) -> String {
    let temp = hw.cpu_temp_mc as f64 / 1000.0;
    let rssi = hw
        .wifi_rssi_dbm
        .map(|r| format!("  {r}dBm"))
        .unwrap_or_default();
    format!(
        "{:.1}°C  CPU {}%  RAM {}/{}M{rssi}",
        temp, hw.cpu_load_pct, hw.mem_used_mb, hw.mem_total_mb
    )
}

fn draw_tile_stats(ctx: &cairo::Context, x: f64, y: f64, tile_h: f64, name: &str, hw: &HwStats) {
    let line2 = format_hw_line(hw);
    let text_y = y + tile_h - 8.0;
    let line_h = 22.0_f64;

    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.65);
    ctx.rectangle(x + 4.0, text_y - line_h * 2.0 - 4.0, 500.0, line_h * 2.0 + 8.0);
    let _ = ctx.fill();

    ctx.select_font_face("Monospace", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(14.0);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 1.0);
    ctx.move_to(x + 8.0, text_y - line_h);
    let _ = ctx.show_text(name);
    ctx.move_to(x + 8.0, text_y);
    let _ = ctx.show_text(&line2);
}

fn draw_self_stats(ctx: &cairo::Context, hw: &HwStats) {
    let temp = hw.cpu_temp_mc as f64 / 1000.0;
    let text = format!(
        "REPORT  {:.1}°C  CPU {}%  RAM {}/{}M",
        temp, hw.cpu_load_pct, hw.mem_used_mb, hw.mem_total_mb
    );

    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.65);
    ctx.rectangle(1920.0 - 430.0, 8.0, 422.0, 28.0);
    let _ = ctx.fill();

    ctx.select_font_face("Monospace", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(14.0);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 1.0);
    ctx.move_to(1920.0 - 426.0, 28.0);
    let _ = ctx.show_text(&text);
}
```

- [ ] **Step 4: Update build_preview signature**

Update the `build_preview` function signature:

```rust
pub fn build_preview(
    sources: &[Source],
    connector_id: u32,
    get_active_slot: Arc<dyn Fn() -> Option<u8> + Send + Sync>,
    get_hw_stats: Arc<dyn Fn() -> HashMap<String, HwStats> + Send + Sync>,
    get_self_hw: Arc<dyn Fn() -> Option<HwStats> + Send + Sync>,
) -> Result<PreviewPipeline> {
```

- [ ] **Step 5: Update the cairo draw callback**

Inside `build_preview`, after the `let cb = get_active_slot.clone();` line, add:

```rust
let hw_cb = get_hw_stats.clone();
let self_hw_cb = get_self_hw.clone();
let source_names: Vec<String> = sources.iter().map(|s| s.name.clone()).collect();
```

Replace the entire `overlay.connect("draw", true, move |args| { ... });` block with:

```rust
overlay.connect("draw", true, move |args| {
    let ctx = unsafe {
        let ptr = gstreamer::glib::gobject_ffi::g_value_get_boxed(
            gstreamer::glib::translate::ToGlibPtr::to_glib_none(&args[1]).0,
        ) as *mut cairo::ffi::cairo_t;
        cairo::Context::from_raw_borrow(ptr)
    };

    // Tally border (existing behaviour)
    if let Some(slot) = cb() {
        if (1..=n as u8).contains(&slot) {
            let idx = (slot - 1) as u32;
            let col = idx % cols as u32;
            let row = idx / cols as u32;
            let x = (col * tile_w) as f64;
            let y = (row * tile_h) as f64;
            ctx.set_source_rgba(1.0, 0.0, 0.0, 1.0);
            ctx.set_line_width(8.0);
            ctx.rectangle(
                x + 4.0,
                y + 4.0,
                (tile_w as f64) - 8.0,
                (tile_h as f64) - 8.0,
            );
            let _ = ctx.stroke();
        }
    }

    // PRECOG hw stats per tile
    let stats_snapshot = hw_cb();
    for (i, name) in source_names.iter().enumerate() {
        if let Some(hw) = stats_snapshot.get(name) {
            let col = (i % cols) as u32;
            let row = (i / cols) as u32;
            let x = (col * tile_w) as f64;
            let y = (row * tile_h) as f64;
            draw_tile_stats(&ctx, x, y, tile_h as f64, name, hw);
        }
    }

    // REPORT self stats — top-right corner
    if let Some(hw) = self_hw_cb() {
        draw_self_stats(&ctx, &hw);
    }

    None
});
```

- [ ] **Step 6: Fix daemon.rs — update install_pipelines to pass closures**

In `report/src/daemon.rs`, update `install_pipelines`:

```rust
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
    let hw_for_preview = self.hw_stats.clone();
    let self_hw_for_preview = self.self_hw.clone();

    let preview_result = build_preview(
        sources,
        self.cfg.preview_connector_id,
        Arc::new(move || state_for_tally.lock().active_slot),
        Arc::new(move || hw_for_preview.lock().expect("hw_stats lock").clone()),
        Arc::new(move || self_hw_for_preview.lock().expect("self_hw lock").clone()),
    )
    .and_then(|p| {
        p.pipeline
            .set_state(gstreamer::State::Playing)
            .map_err(|e| anyhow::anyhow!("preview set_state: {e}"))?;
        spawn_bus_watch("preview", &p.pipeline, bus_tx.clone())?;
        Ok(p)
    });
    // ... (rest of install_pipelines unchanged)
```

- [ ] **Step 7: Run all tests**

```bash
cd /Users/cody/Dev/precrime && cargo test 2>&1 | tail -30
```

Expected: all tests pass. The `format_hw_line` tests pass. Pipeline string tests still pass (they don't call `build_preview` directly).

- [ ] **Step 8: Commit**

```bash
git add report/src/pipeline.rs report/src/daemon.rs
git commit -m "feat(report): render hw stats overlay on preview — per-tile PRECOG text + REPORT corner strip"
```

---

## Task 12: Log rotation — journald drop-in

**Files:**
- Create: `report/journald-precrime.conf`
- Modify: `report/install.sh`

- [ ] **Step 1: Create the journald drop-in**

Create `report/journald-precrime.conf`:

```ini
[Journal]
SystemMaxUse=200M
SystemMaxFileSize=20M
```

- [ ] **Step 2: Update install.sh to install it**

In `report/install.sh`, add after the `apt install` block and before the final `echo`:

```sh
sudo mkdir -p /etc/systemd/journald.conf.d
sudo cp "$(dirname "$0")/journald-precrime.conf" /etc/systemd/journald.conf.d/precrime.conf
sudo systemctl restart systemd-journald
echo "journald log rotation configured (200M max)"
```

The full updated `install.sh`:

```sh
#!/bin/sh
# REPORT installer — run on a fresh Pi OS Lite 64-bit with internet access.
# Binaries are built on macOS via cross and deployed via rsync.
set -e

sudo apt update
sudo apt install -y \
    gstreamer1.0-tools \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    libcairo2 \
    v4l-utils \
    avahi-utils \
    libdrm-tests

sudo mkdir -p /etc/systemd/journald.conf.d
sudo cp "$(dirname "$0")/journald-precrime.conf" /etc/systemd/journald.conf.d/precrime.conf
sudo systemctl restart systemd-journald
echo "journald log rotation configured (200M max)"

echo "Install complete. Run 'make deploy-report REPORT_HOST=$(hostname)' from the macOS dev machine."
```

- [ ] **Step 3: Commit**

```bash
git add report/journald-precrime.conf report/install.sh
git commit -m "ops(report): add journald log rotation — 200M cap, 20M per file"
```

---

## Task 13: iOS — statsPort in RegistrationClient

**Files:**
- Modify: `ios/Witness/Witness/Network/RegistrationClient.swift`
- Modify: `ios/Witness/scripts/mock_report.py`

- [ ] **Step 1: Add statsPort to the Registration struct**

In `RegistrationClient.swift`, update the `Registration` struct:

```swift
struct Registration {
    let assignedPort: UInt16
    let reportName: String
    let ackPort: UInt16
    let statsPort: UInt16
}
```

- [ ] **Step 2: Parse stats_port from the response JSON**

In `parseResponse(_:)`, update the guard to also extract `stats_port`:

```swift
private func parseResponse(_ data: Data) {
    guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
        deliver(.failure(RegistrationError.invalidJSON))
        return
    }

    guard let assignedPort = json["assigned_port"] as? Int,
          assignedPort > 0, assignedPort < 65536,
          let reportName = json["report_name"] as? String,
          let ackPort = json["ack_port"] as? Int,
          ackPort > 0, ackPort < 65536,
          let statsPort = json["stats_port"] as? Int,
          statsPort > 0, statsPort < 65536 else {
        deliver(.failure(RegistrationError.missingFields))
        return
    }

    let reg = Registration(
        assignedPort: UInt16(assignedPort),
        reportName: reportName,
        ackPort: UInt16(ackPort),
        statsPort: UInt16(statsPort)
    )
    deliver(.success(reg))
}
```

- [ ] **Step 3: Update mock_report.py registration response**

In `ios/Witness/scripts/mock_report.py`, find the response dict (around line 134) and add `stats_port`:

```python
response = {
    "assigned_port": port,
    "report_name": self.name,
    "ack_port": ACK_PORT,
    "stats_port": 4998,
}
```

- [ ] **Step 4: Build WITNESS to confirm it compiles**

```bash
cd /Users/cody/Dev/precrime/ios/Witness && xcodegen generate 2>&1 | tail -5
xcodebuild -project Witness.xcodeproj -scheme Witness -sdk iphonesimulator build 2>&1 | grep -E "error:|Build succeeded"
```

Expected: `Build succeeded`.

- [ ] **Step 5: Commit**

```bash
git add ios/Witness/Witness/Network/RegistrationClient.swift ios/Witness/scripts/mock_report.py
git commit -m "feat(witness): parse stats_port from registration response"
```

---

## Task 14: iOS — StatsPublisher

**Files:**
- Create: `ios/Witness/Witness/Network/StatsPublisher.swift`

- [ ] **Step 1: Create StatsPublisher.swift**

```swift
import Foundation
import Network
import UIKit

// Wire types — must match temple::WitnessStatsPacket JSON exactly.
private struct WitnessStatsPacket: Encodable {
    let name: String
    let stats: WitnessStatsPayload
}

private struct WitnessStatsPayload: Encodable {
    let battery_pct: UInt8
    let charging: Bool
    let thermal: String
}

/// Sends battery level + thermal state to REPORT every 2s via UDP.
/// Start after successful registration; stop on disconnect or app background.
final class StatsPublisher {
    private let sourceName: String
    private var timer: DispatchSourceTimer?
    private var connection: NWConnection?

    init(sourceName: String) {
        self.sourceName = sourceName
        UIDevice.current.isBatteryMonitoringEnabled = true
    }

    func start(reportHost: String, statsPort: UInt16) {
        stop()
        let host = NWEndpoint.Host(reportHost)
        guard let port = NWEndpoint.Port(rawValue: statsPort) else { return }
        let conn = NWConnection(host: host, port: port, using: .udp)
        conn.start(queue: .global(qos: .utility))
        connection = conn

        let t = DispatchSource.makeTimerSource(queue: .global(qos: .utility))
        t.schedule(deadline: .now(), repeating: 2.0)
        t.setEventHandler { [weak self] in self?.sendStats() }
        t.resume()
        timer = t
    }

    func stop() {
        timer?.cancel()
        timer = nil
        connection?.cancel()
        connection = nil
    }

    private func sendStats() {
        let level = UIDevice.current.batteryLevel
        let battery: UInt8 = level < 0 ? 0 : UInt8(min(100, Int(level * 100)))
        let state = UIDevice.current.batteryState
        let charging = state == .charging || state == .full
        let thermalStr = thermalStateString(ProcessInfo.processInfo.thermalState)

        let pkt = WitnessStatsPacket(
            name: sourceName,
            stats: WitnessStatsPayload(
                battery_pct: battery,
                charging: charging,
                thermal: thermalStr
            )
        )
        guard let data = try? JSONEncoder().encode(pkt) else { return }
        connection?.send(content: data, completion: .idempotent)
    }

    private func thermalStateString(_ state: ProcessInfo.ThermalState) -> String {
        switch state {
        case .nominal:  return "Nominal"
        case .fair:     return "Fair"
        case .serious:  return "Serious"
        case .critical: return "Critical"
        @unknown default: return "Nominal"
        }
    }

    deinit { stop() }
}
```

- [ ] **Step 2: Build WITNESS to confirm it compiles**

```bash
cd /Users/cody/Dev/precrime/ios/Witness && xcodebuild -project Witness.xcodeproj -scheme Witness -sdk iphonesimulator build 2>&1 | grep -E "error:|Build succeeded"
```

Expected: `Build succeeded`.

- [ ] **Step 3: Commit**

```bash
git add ios/Witness/Witness/Network/StatsPublisher.swift
git commit -m "feat(witness): StatsPublisher — UDP battery+thermal stats to REPORT every 2s"
```

---

## Task 15: iOS — AppModel integration

**Files:**
- Modify: `ios/Witness/Witness/Util/AppModel.swift`

- [ ] **Step 1: Add StatsPublisher property to AppModel**

In `AppModel`, alongside the other `private var` network properties:

```swift
private var statsPublisher: StatsPublisher?
```

- [ ] **Step 2: Start StatsPublisher on successful initial registration**

In `onRegistrationResult(_:isKeepAlive:)`, inside the `case .success(let reg):` branch, after the `ackReceiver` setup block (the `if ackNeedsRestart { ... }` block), add:

```swift
// Start stats publisher on initial registration only.
if !isKeepAlive, let host = currentReportHost {
    statsPublisher?.stop()
    let pub = StatsPublisher(sourceName: settings.sourceName)
    pub.start(reportHost: host, statsPort: reg.statsPort)
    statsPublisher = pub
}
```

- [ ] **Step 3: Stop StatsPublisher on disconnect**

In `restartDiscovery()`, after `ackReceiver = nil`, add:

```swift
statsPublisher?.stop()
statsPublisher = nil
```

- [ ] **Step 4: Build WITNESS**

```bash
cd /Users/cody/Dev/precrime/ios/Witness && xcodebuild -project Witness.xcodeproj -scheme Witness -sdk iphonesimulator build 2>&1 | grep -E "error:|Build succeeded"
```

Expected: `Build succeeded`.

- [ ] **Step 5: Commit**

```bash
git add ios/Witness/Witness/Util/AppModel.swift
git commit -m "feat(witness): start/stop StatsPublisher on registration/disconnect"
```

---

## Self-Review

**Spec coverage check:**

| Spec requirement | Task |
|---|---|
| `HwStats` wire type (cpu_temp_mc, cpu_load_pct, mem_used_mb, mem_total_mb, wifi_rssi_dbm) | Task 1 |
| `WitnessStats` + `WitnessStatsPacket` + `ThermalState` | Task 1 |
| `BallV1.hw: Option<HwStats>` backward-compatible | Task 2 |
| `hw-stats` crate with parse fns + `CpuSnapshot` | Task 3 |
| PRECOG dynamic ball rebuild with stats | Task 4 |
| REPORT `stats_port` config | Task 5 |
| Registration response includes `stats_port` | Task 6 |
| REPORT `hw_stats` map populated from temple balls | Task 7 |
| REPORT self-hw thread polling every 2s | Task 8 |
| REPORT WITNESS stats UDP listener | Task 9 |
| 30s stats logger (PRECOG + WITNESS + self) | Task 10 |
| Preview overlay — per-tile PRECOG text + REPORT corner strip | Task 11 |
| Log rotation — journald 200M cap | Task 12 |
| iOS `statsPort` in registration response | Task 13 |
| iOS `StatsPublisher` UDP publisher | Task 14 |
| iOS `AppModel` lifecycle | Task 15 |
| `mock_report.py` updated | Task 13 |

All spec requirements covered.

**Type consistency check:** `HwStats` defined in Task 1 and used consistently across Tasks 2–11 with the same field names. `WitnessStatsPacket` JSON snake_case fields match Rust serde defaults and Swift `Encodable` field names. `format_hw_line` defined and tested in Task 11, called only in Task 11. `CpuSnapshot` defined in Task 3, used in Tasks 4, 8.

**Placeholder scan:** No TBD, TODO, or "similar to Task N" shortcuts found.
