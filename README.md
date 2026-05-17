# PRECRIME

A redeployable multi-camera H.264/RTP switching rig for live performance visuals and livestreaming, built around a surveillance/CCTV aesthetic. Fully FOSS — no NDI SDK, no proprietary licensing.

Operator switches between heterogeneous video sources (iPhones, vintage analog CCTV cams through Pi-based encoders, modern IP cams, HDMI sources) from a custom Pi-based hardware switcher with dual HDMI out (program + multiview preview).

## Status

**Phase 1 + 1.5 complete. WITNESS iOS app shipped. Hardware bring-up pending.**

- ✅ Rust workspace — `report`, `precog`, `temple` crates, 42 tests green
- ✅ TEMPLE — JSON ball wire format + UDP multicast send/receive for source discovery
- ✅ GStreamer pipelines — RTP/UDP sources, program input-selector, multiview compositor, cairo tally overlay, V4L2 HW decode on Pi 5
- ✅ REPORT — dynamic source registration (TCP), Bonjour publish, UDP ack to all active sources
- ✅ WITNESS — iOS camera app, zero-config Bonjour discovery, H.264/RTP unicast, camera controls, stage mode
- ✅ CI — GitHub Actions (fmt + clippy + check + test)
- ⏳ Hardware: Flint 3, Pi 5s, EasyCap — Phase 2 blocked on procurement
- ⏳ Titler GStreamer element — Phase T1.3

## Architecture

```
   [WITNESS (iPhone)]──┐
   [PRECOG Pi+EasyCap]─┼── WiFi/LAN ── [REPORT (Pi 5)] ──┬─▶ HDMI Program
   [PRECOG Pi+...]─────┘                                  └─▶ HDMI Multiview
```

**Camera → REPORT transport:**

- **Pi PRECOGs**: H.264/RTP → IPv4 multicast group (`239.42.x.y`, TTL=1). Discovery via JSON ball every 2s on TEMPLE channel (`239.42.0.1:9999`).
- **WITNESS (iPhone)**: H.264/RTP → UDP unicast to REPORT. Discovery via Bonjour (`_precrime-report._tcp`). iOS blocks multicast sends without Apple entitlement.

Both transports feed the same REPORT pipeline — same caps, same `rtpjitterbuffer → rtph264depay → decoder → input-selector` chain.

**Zero-config connection (WITNESS):**

```
WITNESS                              REPORT
  │ browse _precrime-report._tcp ──▶ publish _precrime-report._tcp
  │ TCP register (name, res, fps) ──▶ assign port from pool (5000–5099)
  │ ◀── {assigned_port, ack_port} ──│
  │ RTP → REPORT:assigned_port ────▶ udpsrc on assigned port
  │ ◀── UDP ack every 2s ──────────│
```

**System naming** (Minority Report themed):

| Name | Role |
|---|---|
| **PRECRIME** | The system as a whole |
| **PRECOG** | A camera unit — any H.264/RTP source on the LAN |
| **REPORT** | The Pi 5 switcher daemon |
| **WITNESS** | The iOS camera sender app |
| **TEMPLE** | Discovery channel — precogs announce via JSON balls |
| **MEZZANINE** | Future hardware controller |

## Repo layout

```
.
├── Cargo.toml                  # workspace root (report + precog + temple)
├── temple/                     # discovery library (ball wire format + UDP multicast)
├── report/                     # switcher daemon + deploy artifacts
│   ├── src/
│   │   ├── daemon.rs           # event loop, thread orchestration
│   │   ├── pipeline.rs         # GStreamer builders (program + preview)
│   │   ├── registration.rs     # TCP registration server, port pool
│   │   ├── ack.rs              # UDP ack sender (2s loop → all sources)
│   │   ├── bonjour.rs          # avahi-publish-service subprocess
│   │   ├── config.rs           # TOML config parsing
│   │   ├── mapping.rs          # source → slot assignment
│   │   └── input.rs            # evdev keyboard
│   ├── report.conf.example
│   ├── report.service
│   └── runbook.md
├── precog/                     # Pi camera encoder daemon + deploy artifacts
│   ├── src/{main,config,lib}.rs
│   ├── precog.conf.example
│   ├── precog.service
│   └── kit-a-cctv-runbook.md
├── ios/
│   └── Witness/                # iOS WITNESS camera app
│       ├── project.yml         # xcodegen spec
│       ├── Makefile
│       ├── scripts/mock_report.py  # dev REPORT mock
│       └── Witness/            # Swift sources
└── docs/
    ├── runbooks/
    └── superpowers/specs/      # feature design docs
```

## Build — Rust (REPORT + PRECOG)

Build happens on the Pi via SSH. Drive from the repo root Makefile.

**First-time Pi setup:**

```bash
make install-report REPORT_HOST=report.local
# SSH in → edit /etc/precrime/report.conf
make deploy-report REPORT_HOST=report.local
```

```bash
make install-precog PRECOG_HOST=precog-01.local
# SSH in → edit /etc/precog/precog.conf
make deploy-precog PRECOG_HOST=precog-01.local
```

**Iterate:**

```bash
make deploy-report   # rsync + cargo build --release + restart
make logs-report     # tail journalctl -u report.service -f
make restart-report  # restart only
```

**macOS dev:**

```bash
brew install gstreamer gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav
make check && make test
```

## Build — WITNESS (iOS)

```bash
brew install xcodegen
cd ios/Witness && make open
```

WITNESS auto-discovers REPORT via Bonjour — no manual IP config. Dev testing without a Pi:

```bash
python3 ios/Witness/scripts/mock_report.py
```

See [`ios/Witness/README.md`](ios/Witness/README.md) for full docs.

## REPORT configuration

`/etc/precrime/report.conf`:

```toml
program_connector_id = 32
preview_connector_id = 34
keyboard_device      = "/dev/input/event0"

report_name  = "REPORT-MAIN"   # shown in WITNESS status bar
reg_port     = 4999            # WITNESS registration port
rtp_port_min = 5000
rtp_port_max = 5099
ack_port     = 9998

temple_group = "239.42.0.1"
temple_port  = 9999
```

## Documentation

| Doc | What |
|---|---|
| [`ios/Witness/README.md`](ios/Witness/README.md) | WITNESS build, controls, stage mode, mock server |
| [`report/runbook.md`](report/runbook.md) | REPORT operator runbook |
| [`precog/kit-a-cctv-runbook.md`](precog/kit-a-cctv-runbook.md) | Pi CCTV encoder setup |
| [`docs/runbooks/2026-05-17-rtp-mac-smoke.md`](docs/runbooks/2026-05-17-rtp-mac-smoke.md) | macOS RTP loopback test |
| [`docs/superpowers/specs/`](docs/superpowers/specs/) | Feature design specs |

## Hardware (Phase 2 BOM, ~$490)

- GL.iNet Flint 3 (GL-BE9300) WiFi 7/6E router — ~$200–300
- 2× Raspberry Pi 5 8GB + coolers + SD cards — ~$230
- EasyCap UTV007 USB analog capture — $15
- Cat6a, mounts, power — ~$50

## License

MIT OR Apache-2.0
