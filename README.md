# PRECRIME

A redeployable multi-camera NDI switching rig for live performance visuals and
livestreaming, built around a surveillance/CCTV aesthetic.

Operator switches between heterogeneous NDI sources (phones, vintage analog CCTV
cams through Pi-based encoders, modern IP cams, HDMI sources via dedicated
encoders) from a custom Pi-based hardware switcher with dual HDMI out (program +
multiview preview).

## Status

**Phase 1 software complete.** Hardware bring-up pending.

- ✅ Rust workspace with two binary crates (`report`, `precog`)
- ✅ TDD-covered pure-Rust modules (config parsing, slot mapping, NDI name filtering)
- ✅ libndi FFI for source discovery
- ✅ GStreamer pipeline builders (program input-selector, multiview compositor, cairo tally overlay)
- ✅ evdev USB keyboard input
- ✅ Daemon orchestration with per-pipeline bus watch
- ✅ systemd units + deploy artifacts
- ⏳ Pi-side build + M3–M11 hardware verification
- ⏳ Network Brain (Flint 3 router config)
- ⏳ PRECOG Kit B (iPhone setup)
- ⏳ Phase 2 next-up: smart plug bus + phone provisioning (see spec §11)

## Architecture

```
   [PRECOG N]──┐
      ...      ├── WiFi/Ethernet ── [Router] ── [REPORT (Pi 5)] ──┬─> HDMI Program (capture/stream)
   [PRECOG 1]──┘                                                  └─> HDMI Multiview (operator)
```

**Hard interface:** every camera source publishes itself as an NDI|HX2 source on
the LAN, discoverable via mDNS, named `PRECOG-NN-<TYPE>-<LOC>` (e.g.
`PRECOG-01-IPHONE-STAGE`). Any device that meets this contract is a valid PRECRIME
camera — phones, Pi+EasyCap encoders, IP cams via RTSP→NDI bridge, HDMI sources
via dedicated encoder.

**System naming:**

- **PRECRIME** — the system as a whole
- **PRECOG** — a camera unit (each a vision feed)
- **REPORT** — the switcher Pi (assembles the prediction)
- **MEZZANINE** — future custom hardware controller (Phase 2)

## Repo layout

```
.
├── Cargo.toml                  # workspace root (members: report, precog)
├── report/                     # switcher binary crate + deploy artifacts
│   ├── src/{main,daemon,pipeline,input,ndi_find,config,mapping,naming}.rs
│   ├── tests/                  # pure-Rust integration tests
│   ├── build.rs                # libndi link directives
│   ├── install.sh              # apt + rustup installer (run on Pi)
│   ├── report.conf.example     # /etc/precrime/report.conf template
│   ├── report.service          # systemd unit
│   └── runbook.md
├── precog/                     # CCTV encoder binary crate + deploy artifacts
│   ├── src/{main,config}.rs
│   ├── install.sh
│   ├── precog.conf.example
│   ├── precog.service
│   └── kit-a-cctv-runbook.md
├── network/                    # router config exports, mDNS notes
├── hardware/                   # MEZZANINE controller firmware (Phase 2+)
└── docs/
    ├── specs/                  # system-level design spec
    └── plans/                  # implementation plans per sub-project
```

## Build

Build happens on the Pi via SSH; no local cross-compile setup. Drive everything
from the Makefile at the repo root.

**First-time install on a Pi:**

```bash
make install-report REPORT_HOST=report.local
# then SSH in and install NDI SDK libndi.so per docs/plans/2026-05-16-report-switcher.md Task 2
# edit /etc/precrime/report.conf (connector IDs, keyboard device)
make deploy-report   REPORT_HOST=report.local
```

Same shape for PRECOG units:

```bash
make install-precog PRECOG_HOST=precog-02-cctv-door.local
# edit /etc/precog/precog.conf on the Pi (NDI name, format, framerate)
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
brew install gstreamer
make check    # cargo check whole workspace
make test     # cargo test (pure-Rust modules only; libndi/GStreamer link blocked on macOS)
```

## Documentation

- **System design spec:** [`docs/specs/2026-05-16-precrime-system-design.md`](docs/specs/2026-05-16-precrime-system-design.md)
- **Implementation plans:**
  - [Network Brain](docs/plans/2026-05-16-network-brain.md) — Flint 3 router config, mDNS reflector
  - [PRECOG Kit B (iPhone)](docs/plans/2026-05-16-precog-kit-b-iphone.md) — NDI HX Camera setup
  - [PRECOG Kit A (CCTV)](docs/plans/2026-05-16-precog-kit-a-cctv.md) — Pi encoder for analog CCTV
  - [REPORT switcher](docs/plans/2026-05-16-report-switcher.md) — Rust daemon, dual HDMI
- **Operator runbooks** are colocated with each component (`report/runbook.md`, `precog/kit-a-cctv-runbook.md`).

## Hardware

Phase 1 bill of materials (~$490 + tax/shipping):

- GL.iNet Flint 3 (GL-BE9300) WiFi 7/6E router — ~$200-300
- Raspberry Pi 5 8GB (REPORT) + accessories — ~$130
- Raspberry Pi 5 4GB (PRECOG encoder) + accessories — ~$100
- EasyCap UTV007 USB analog video capture — $15
- iPhone 15 (owned) + NDI HX Camera app (free)
- Vintage CCTV cam (owned)
- Misc cables, mounts, power — ~$50

See spec §8 for the full itemized list.

## License

MIT OR Apache-2.0
