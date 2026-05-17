# PRECRIME

A redeployable multi-camera H.264/RTP switching rig for live performance visuals
and livestreaming, built around a surveillance/CCTV aesthetic. Fully FOSS — no
NDI SDK, no proprietary licensing.

Operator switches between heterogeneous video sources (phones, vintage analog
CCTV cams through Pi-based encoders, modern IP cams, HDMI sources via dedicated
encoders) from a custom Pi-based hardware switcher with dual HDMI out (program +
multiview preview).

## Status

**Phase 1 software complete + RTP/multicast migration shipped.** Hardware bring-up pending.

- ✅ Rust workspace with three crates (`report`, `precog`, `temple`)
- ✅ TDD-covered pure-Rust modules (config parsing, slot mapping, ball discovery) — 42 tests, all green
- ✅ TEMPLE crate — JSON ball wire format + UDP multicast send/receive for source discovery
- ✅ GStreamer pipeline builders (RTP/UDP sources, program input-selector, multiview compositor, cairo tally overlay)
- ✅ evdev USB keyboard input
- ✅ Daemon orchestration with per-pipeline bus watch
- ✅ systemd units + deploy artifacts
- ⏳ Pi-side build + M3'–M11' hardware verification
- ⏳ Network Brain (Flint 3 router config — IGMP snooping enable required)
- ⏳ PRECOG Kit B (phone publisher app + SRT/RTMP→RTP bridge — separate plan)
- ⏳ Phase 2 next-up: smart plug bus + phone provisioning

## Architecture

```
   [PRECOG N]──┐
      ...      ├── WiFi/Ethernet ── [Router] ── [REPORT (Pi 5)] ──┬─> HDMI Program (capture/stream)
   [PRECOG 1]──┘                                                  └─> HDMI Multiview (operator)
```

**Hard interface:** every camera source publishes H.264 over RTP to a per-source
IPv4 multicast group (`239.42.x.y`, TTL=1, admin-scoped) on the LAN. Each source
also emits a JSON "ball" every 2 seconds on the TEMPLE discovery channel
(`239.42.0.1:9999`) advertising its name, multicast endpoint, and video format.
Source names follow `PRECOG-NN-<TYPE>-<LOC>` (e.g. `PRECOG-01-IPHONE-STAGE`).
Any device that meets this contract is a valid PRECRIME camera — phones,
Pi+EasyCap encoders, IP cams via RTSP→RTP bridge, HDMI sources via dedicated
encoder.

**System naming** (Minority Report themed):

- **PRECRIME** — the system as a whole
- **PRECOG** — a camera unit (each a vision feed)
- **REPORT** — the switcher Pi (assembles the prediction)
- **TEMPLE** — the discovery channel where precogs announce themselves via named balls
- **MEZZANINE** — future custom hardware controller (Phase 2)

## Repo layout

```
.
├── Cargo.toml                  # workspace root (members: report, precog, temple)
├── temple/                     # discovery beacon library crate (ball wire format + UDP mcast)
│   └── src/{lib,ball,send,recv}.rs
├── report/                     # switcher binary crate + deploy artifacts
│   ├── src/{main,daemon,pipeline,input,config,mapping}.rs
│   ├── tests/                  # pure-Rust integration tests
│   ├── install.sh              # apt + rustup installer (run on Pi)
│   ├── report.conf.example     # /etc/precrime/report.conf template
│   ├── report.service          # systemd unit
│   └── runbook.md
├── precog/                     # camera encoder binary crate + deploy artifacts
│   ├── src/{main,config,lib}.rs
│   ├── install.sh
│   ├── precog.conf.example
│   ├── precog.service
│   └── kit-a-cctv-runbook.md
├── network/                    # router config exports, IGMP-snooping notes
├── hardware/                   # MEZZANINE controller firmware (Phase 2+)
└── docs/
    ├── specs/                  # system-level design specs
    ├── plans/                  # implementation plans per sub-project
    └── runbooks/               # operator + dev runbooks
```

## Build

Build happens on the Pi via SSH; no local cross-compile setup. Drive everything
from the Makefile at the repo root.

**First-time install on a Pi:**

```bash
make install-report REPORT_HOST=report.local
# SSH in and edit /etc/precrime/report.conf (connector IDs, keyboard device, temple_group/port if non-default)
make deploy-report   REPORT_HOST=report.local
```

Same shape for PRECOG units:

```bash
make install-precog PRECOG_HOST=precog-02-cctv-door.local
# edit /etc/precog/precog.conf on the Pi (source_name, device, format, rtp_mcast, rtp_port)
make deploy-precog  PRECOG_HOST=precog-02-cctv-door.local
```

**Iterate:**

```bash
make deploy-report   # rsync + cargo build --release + restart service
make logs-report     # tail journalctl -u report.service -f
make restart-report  # restart only, no rebuild
```

`make help` lists every target. All targets take `REPORT_HOST=...` or
`PRECOG_HOST=...` overrides.

**On macOS (dev only):**

```bash
brew install gstreamer gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav
make check    # cargo check whole workspace
make test     # cargo test --workspace (all 42 tests pass on macOS)
```

For a full RTP loopback smoke test against the built-in webcam, follow
[`docs/runbooks/2026-05-17-rtp-mac-smoke.md`](docs/runbooks/2026-05-17-rtp-mac-smoke.md).

## Documentation

- **Current architecture:** [`docs/plans/2026-05-17-rtp-multicast-migration.md`](docs/plans/2026-05-17-rtp-multicast-migration.md) — full RTP+multicast + TEMPLE design and the 13-task implementation plan that landed it.
- **System design spec (NDI era, superseded):** [`docs/specs/2026-05-16-precrime-system-design.md`](docs/specs/2026-05-16-precrime-system-design.md) — retained for historical context; transport sections are out of date.
- **macOS dev smoke:** [`docs/runbooks/2026-05-17-rtp-mac-smoke.md`](docs/runbooks/2026-05-17-rtp-mac-smoke.md) — local RTP loopback procedure.
- **Operator runbooks** are colocated with each component (`report/runbook.md`, `precog/kit-a-cctv-runbook.md`).

## Hardware

Phase 1 bill of materials (~$490 + tax/shipping):

- GL.iNet Flint 3 (GL-BE9300) WiFi 7/6E router — ~$200-300
- Raspberry Pi 5 8GB (REPORT) + accessories — ~$130
- Raspberry Pi 5 4GB (PRECOG encoder) + accessories — ~$100
- EasyCap UTV007 USB analog video capture — $15
- iPhone 15 (owned) + FOSS-compatible publisher app (Moblin recommended; Larix Broadcaster freeware as fallback) — needs a thin SRT/RTMP→RTP-multicast bridge daemon, separate plan pending
- Vintage CCTV cam (owned)
- Misc cables, mounts, power — ~$50

See spec §8 for the full itemized list (note: spec transport section is superseded by RTP migration).

## License

MIT OR Apache-2.0
