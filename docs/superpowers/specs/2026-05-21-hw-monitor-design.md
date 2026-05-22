# Hardware Monitor — Design Spec
_2026-05-21_

## Goal

REPORT gathers CPU temp, CPU load, memory usage, and WiFi RSSI from every connected PRECOG Pi and from the WITNESS iPhone app, then:
- renders the stats as live text overlays on the HDMI preview output
- logs all stats every 30 s to journald
- keeps journald from filling the REPORT Pi's SD card

REPORT also monitors its own hardware using the same read path.

---

## 1. Wire Format (temple crate)

### 1.1 `HwStats`

New struct, added to `temple/src/ball.rs` and re-exported from the crate root:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwStats {
    pub cpu_temp_mc: u32,       // millidegrees C — 42300 = 42.3 °C
    pub cpu_load_pct: u8,       // 0–100
    pub mem_used_mb: u32,
    pub mem_total_mb: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_rssi_dbm: Option<i16>,  // None if wired or read fails
}
```

Integer-only so `HwStats` is `Eq`-derivable (not used on `Source`, but keeps the type consistent).

### 1.2 `BallV1` extension

```rust
pub struct BallV1 {
    // all existing fields unchanged
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hw: Option<HwStats>,
}
```

Old REPORTs ignore the unknown field. Old PRECOGs emit no `hw` field — REPORT handles `None` gracefully.

### 1.3 `WitnessStats`

WITNESS can't send a Ball (iOS blocks multicast). Its stats travel via UDP push to a new port (see §4). Defined in the `temple` crate for shared use:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessStats {
    pub battery_pct: u8,   // 0–100
    pub charging: bool,
    pub thermal: ThermalState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ThermalState { Nominal, Fair, Serious, Critical }
```

Wire format: JSON, same as Ball — small and human-readable in logs.

---

## 2. New Workspace Crate: `hw-stats`

Shared by both `precog` and `report`. Three pure Linux read functions + one type:

```
hw-stats/
  src/lib.rs   — pub fns + CpuSnapshot type
  Cargo.toml
```

```rust
pub struct CpuSnapshot { pub idle: u64, pub total: u64 }

pub fn read_cpu_temp_mc() -> Option<u32>
// reads /sys/class/thermal/thermal_zone0/temp

pub fn read_cpu_load_pct(prev: &CpuSnapshot) -> (u8, CpuSnapshot)
// reads /proc/stat, computes Δidle/Δtotal delta vs prev snapshot
// returns (load_pct 0–100, new_snapshot)

pub fn read_mem_mb() -> Option<(u32, u32)>
// reads /proc/meminfo → (used_mb, total_mb)
// used = total − available

pub fn read_wifi_rssi_dbm() -> Option<i16>
// reads /proc/net/wireless, first interface line, signal column
// normalises old-kernel +256 offset where value > 0
```

All functions return `Option` — any file-read or parse error yields `None`. The caller embeds the result in `HwStats` as-is; REPORT renders missing fields as `--`.

---

## 3. PRECOG Changes

### 3.1 Ball thread becomes dynamic

Ball is currently constructed once before the loop. With stats it must be rebuilt each cycle:

```rust
let mut cpu_snap = CpuSnapshot::default();
loop {
    let (load, new_snap) = read_cpu_load_pct(&cpu_snap);
    cpu_snap = new_snap;
    let hw = Some(HwStats {
        cpu_temp_mc: read_cpu_temp_mc().unwrap_or(0),
        cpu_load_pct: load,
        mem_used_mb:  read_mem_mb().map(|(u,_)| u).unwrap_or(0),
        mem_total_mb: read_mem_mb().map(|(_,t)| t).unwrap_or(0),
        wifi_rssi_dbm: read_wifi_rssi_dbm(),
    });
    let ball = Ball::V1(BallV1 { hw, ..base.clone() });
    sender.send(&ball)?;
    sleep(BALL_PERIOD_SECS);
}
```

`base` holds the static fields (name, host, rtp, video) cloned once before the loop.

### 3.2 Dependency

`precog/Cargo.toml` adds `hw-stats = { path = "../hw-stats" }`.

---

## 4. REPORT Changes

### 4.1 New shared state in `daemon.rs`

```rust
let hw_stats:      Arc<Mutex<HashMap<String, HwStats>>>      = default();
let witness_stats: Arc<Mutex<HashMap<String, WitnessStats>>> = default();
let self_hw:       Arc<Mutex<Option<HwStats>>>               = default();
```

### 4.2 Temple-rx thread update

After `rx.poll()` returns new balls, update both maps:

```rust
let balls  = rx.snapshot();
let sources = balls_to_sources(balls.clone());
let stats: HashMap<String, HwStats> = balls.into_iter()
    .filter_map(|b| match b {
        Ball::V1(v) if v.name.starts_with("PRECOG-") => v.hw.map(|hw| (v.name, hw)),
        _ => None,
    })
    .collect();
*temple_snap_tx.lock() = sources;
*hw_stats_tx.lock()    = stats;
```

### 4.3 REPORT self-poll thread (`report-hw-self`)

Spawned at startup, polls every 2 s:

```rust
let mut cpu_snap = CpuSnapshot::default();
loop {
    let (load, new_snap) = read_cpu_load_pct(&cpu_snap);
    cpu_snap = new_snap;
    *self_hw_tx.lock() = Some(HwStats {
        cpu_temp_mc:   read_cpu_temp_mc().unwrap_or(0),
        cpu_load_pct:  load,
        mem_used_mb:   read_mem_mb().map(|(u,_)| u).unwrap_or(0),
        mem_total_mb:  read_mem_mb().map(|(_,t)| t).unwrap_or(0),
        wifi_rssi_dbm: read_wifi_rssi_dbm(),
    });
    sleep(Duration::from_secs(2));
}
```

### 4.4 WITNESS stats UDP listener

`registration.rs` registration response gains `stats_port: u16` (configured in `report.conf`, default `4998`). WITNESS sends JSON `WitnessStats` packets to `report_host:stats_port` every 2 s.

New thread `report-witness-stats-rx`:

```rust
let sock = UdpSocket::bind(("0.0.0.0", cfg.stats_port))?;
let mut buf = [0u8; 512];
loop {
    let (n, addr) = sock.recv_from(&mut buf)?;
    if let Ok(ws) = serde_json::from_slice::<WitnessStatsPacket>(&buf[..n]) {
        witness_stats.lock().insert(ws.name, ws.stats);
    }
}
```

`WitnessStatsPacket` is `{ name: String, stats: WitnessStats }` — source name from WITNESS matches its registered name.

### 4.5 Stats logger thread (`report-stats-log`)

Wakes every 30 s. Logs one structured `info!` line per source plus one for self:

```
info!(source="PRECOG-01-CCTV", temp_c=42.3, cpu_pct=67, mem_mb=280, mem_total=480, rssi_dbm=-54)
info!(source="WITNESS-STAGE",  battery_pct=84, charging=false, thermal="Nominal")
info!(source="REPORT-SELF",    temp_c=51.0, cpu_pct=23, mem_mb=340, mem_total=2000, rssi_dbm=-61)
```

### 4.6 Preview overlay changes in `pipeline.rs`

`build_preview` gains two extra closures:

```rust
pub fn build_preview(
    sources: &[Source],
    connector_id: u32,
    get_active_slot:  Arc<dyn Fn() -> Option<u8> + Send + Sync>,
    get_hw_stats:     Arc<dyn Fn() -> HashMap<String, HwStats> + Send + Sync>,
    get_self_hw:      Arc<dyn Fn() -> Option<HwStats> + Send + Sync>,
) -> Result<PreviewPipeline>
```

WITNESS stats are not shown per-tile (WITNESS has no video tile in the preview grid). They are included in logs only.

Cairo draw callback additions:

**Per-tile PRECOG text** (bottom-left of each tile, rendered after the tally border):
```
PRECOG-01-CCTV
42.3°C  CPU 67%  RAM 280/480M  -54dBm
```
White text, 18px, with a translucent black backing rectangle for legibility.

**Fixed REPORT self strip** (top-right corner of the full 1920×1080 frame, always rendered):
```
REPORT  51.0°C  CPU 23%  RAM 340/2000M
```
White text, 16px, same backing style.

If a stat value is unavailable (`None` / 0), that field renders as `--`.

### 4.7 Log rotation

New file `report/journald-precrime.conf` installed to `/etc/systemd/journald.conf.d/precrime.conf` by `report/install.sh`:

```ini
[Journal]
SystemMaxUse=200M
SystemMaxFileSize=20M
```

---

## 5. WITNESS (iOS) Changes

### 5.1 Registration response

`RegistrationResponse` Swift struct gains `statsPort: UInt16`. Parsed from the existing TCP registration response JSON.

### 5.2 `StatsPublisher`

New Swift class, started after successful registration:

```swift
class StatsPublisher {
    func start(reportHost: String, statsPort: UInt16, sourceName: String)
    // sends WitnessStatsPacket JSON via UDP every 2s
    // battery via UIDevice.current.batteryLevel / .batteryState
    // thermal via ProcessInfo.processInfo.thermalState
}
```

`UIDevice.current.isBatteryMonitoringEnabled = true` is set on init. Battery level quantised to `UInt8` (multiply by 100, clamp).

### 5.3 Teardown

`StatsPublisher` stops when WITNESS disconnects or app backgrounds.

---

## 6. Config Changes

`report.conf` gains one new key:

```toml
stats_port = 4998   # WITNESS stats UDP receiver port
```

`report.conf.example` updated accordingly.

---

## 7. Affected Files

| File | Change |
|---|---|
| `temple/src/ball.rs` | Add `HwStats`, `WitnessStats`, `ThermalState`; extend `BallV1` |
| `temple/src/lib.rs` | Re-export new types |
| `hw-stats/` | New crate (4 fns + `CpuSnapshot`) |
| `Cargo.toml` (workspace) | Add `hw-stats` member |
| `precog/Cargo.toml` | Add `hw-stats` dep |
| `precog/src/main.rs` | Dynamic ball rebuild with hw stats |
| `report/Cargo.toml` | Add `hw-stats` dep |
| `report/src/daemon.rs` | 3 new shared-state arcs, 3 new threads, pass closures to `build_preview` |
| `report/src/registration.rs` | Response gains `stats_port` |
| `report/src/pipeline.rs` | `build_preview` signature + cairo text rendering |
| `report/src/config.rs` | `stats_port` field |
| `report/install.sh` | Install journald drop-in |
| `report/journald-precrime.conf` | New file |
| `report/report.conf.example` | Add `stats_port` |
| `ios/Witness/Witness/` | `StatsPublisher.swift` (new), `RegistrationResponse` update |

---

## 8. Testing

- `hw-stats` crate: unit tests with mock `/proc` file content (temp fixtures)
- `temple` crate: roundtrip JSON tests for `HwStats` and `WitnessStats` with `hw = None` (backward compat)
- `report/pipeline.rs`: `build_preview` called with mock closures returning known stats; assert no panic (cairo rendering tested at integration level only)
- Manual: deploy to Pi 5 pair, confirm overlay text visible on preview HDMI, confirm 30s log entries appear in `make logs-report`
