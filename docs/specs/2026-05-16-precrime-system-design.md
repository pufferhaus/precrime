# PRECRIME — System Design Spec

**Date:** 2026-05-16
**Status:** Approved (Phase 1 scope)
**Author:** Cody (with Claude)

PRECRIME is a redeployable wireless multi-camera switching rig with a CCTV/surveillance aesthetic, built for live performance visuals and livestreaming. The system accepts any camera type that publishes itself as an NDI source on the local network. A custom switcher (REPORT) ingests those sources and outputs a single program feed over HDMI for capture or projection, alongside a multiview preview for the operator.

This spec defines the system-level architecture, the hard interface contract between components, and the Phase 1 build scope. Each sub-project listed in §2 will get its own focused spec before implementation begins on that piece.

## 1. Architecture and Hard Interface

```
┌──────────────────────────────────────────────────────────────────────────┐
│                           PRECRIME LAN                                   │
│                                                                          │
│   [PRECOG N]──┐                                                          │
│      ...      ├── WiFi/Ethernet ── [Router] ── [REPORT (switcher Pi)] ──┼─> HDMI Program
│   [PRECOG 2]──┤                       │                                  │   (capture/projector)
│   [PRECOG 1]──┘                       │                                  │
│                                       │                                  ├─> HDMI Preview
│                                Wired Cat6 from                           │   (multiview, operator)
│                                router to REPORT                          │
└──────────────────────────────────────────────────────────────────────────┘
```

### Hard interface contract

Every camera source MUST present as:

- An **NDI|HX2 source** on the LAN
- Discoverable via **mDNS/Bonjour** (no manual URL entry)
- Named per the convention: `PRECOG-NN-<TYPE>-<LOC>`
  - `NN` is a two-digit ordinal (`01`, `02`, …)
  - `<TYPE>` is one of `IPHONE`, `ANDROID`, `CCTV`, `IPCAM`, `HDMI`, `NDI`
  - `<LOC>` is a short location tag (`STAGE`, `DOOR`, `BAR`, `BOOTH`, …)
  - Example: `PRECOG-01-IPHONE-STAGE`, `PRECOG-02-CCTV-DOOR`

REPORT cares about nothing else. Any device that meets this contract is a valid PRECRIME camera. This makes the camera roster extensible — phones, Pi-encoded analog cams, IP PoE cams via RTSP→NDI bridge, HDMI sources via dedicated encoder all coexist freely.

## 2. Sub-Project Decomposition

| # | Sub-project | Phase 1 scope | Future spec |
|---|---|---|---|
| 1 | **PRECOG Kit A** — Pi NDI encoder for analog CCTV | Pi 5 + USB analog capture + headless encoder service | own spec |
| 2 | **PRECOG Kit B** — Phone NDI source | iPhone 15 + NDI HX Camera app + power/mount | own spec |
| 3 | **PRECOG Kit C** — IP PoE cam + RTSP→NDI bridge | not in Phase 1 | own spec, later |
| 4 | **PRECOG Kit D** — HDMI source encoder (DSLR, camcorder) | not in Phase 1 | own spec, later |
| 5 | **Network Brain** — router, mDNS, DHCP, naming | Flint 2 + reflector + reservations | own spec |
| 6 | **REPORT** — headless switcher with dual HDMI | Pi 5 + GStreamer + Python daemon + USB keyboard | own spec, largest |
| 7 | **MEZZANINE** — custom hardware operator controller | not in Phase 1 (USB keyboard stands in) | own spec, Phase 2 |
| 8 | **Output / Capture / Stream Chain** | program HDMI → external capture (existing gear) | own spec, Phase 2 |

Each sub-project gets its own brainstorm → spec → plan cycle when work on it begins.

## 3. REPORT — Switcher Stack

REPORT is the most complex Phase 1 sub-project. Detailed here because its decisions shape the rest of the system.

### Operating system

- Raspberry Pi OS Lite 64-bit (no desktop environment)
- Pi 5 8GB
- Active cooler mandatory, vented case

### Runtime

- GStreamer 1.22+ (system-installed via apt)
- `gst-plugin-rs` (Rust GStreamer plugins, ships `ndisrc`/`ndisink` via apt as `gstreamer1.0-plugins-rs`)
- `libndi` (NewTek SDK, free with EULA acceptance) — loaded by `gst-plugin-rs` for stream transport, and via direct FFI from the report binary for `NDIlib_find_*` discovery
- **Single Rust binary** `report` (cargo workspace member) using:
  - `gstreamer`, `gstreamer-app`, `gstreamer-video` crates for pipeline construction
  - `evdev` crate for USB keyboard input
  - `cairo-rs` for the tally overlay draw callback
  - `serde` + `toml` for config parsing
  - `tracing` + `tracing-journald` for logging (lands cleanly in `journalctl`)
  - `anyhow` for error handling, `thiserror` for typed error boundaries
  - Small FFI module wrapping libndi's Find API for source discovery
- `systemd` service: `report.service` execs `/usr/local/bin/report` directly, autostarts at boot

### Pipelines

REPORT runs two independent GStreamer pipelines targeting the Pi 5's two HDMI connectors via `kmssink` (direct DRM/KMS framebuffer write — no X11, no Wayland).

**Pipeline A — Program out (HDMI-A-1):**
- N `ndisrc` inputs (one per discovered PRECOG)
- `input-selector` element switches active source on keyboard event
- Output: full-screen active source to `kmssink connector-id=<HDMI-A-1 id>`
- Optional brief crossfade transition between selections (Phase 2 polish)

**Pipeline B — Multiview preview (HDMI-A-2):**
- Same N `ndisrc` inputs fed in parallel
- `compositor` element tiles them in a grid (2×2 for ≤4, 3×3 for ≤9)
- Tally overlay (`cairooverlay` or `gdkpixbufoverlay`) draws a red border on the currently-live tile
- Output: composited multiview to `kmssink connector-id=<HDMI-A-2 id>`

### Control

- USB keyboard plugged directly into REPORT Pi
- Number keys `1`–`N` map deterministically to PRECOG slots based on alphabetical ordering of discovered source names (stable across reboots once cameras are named)
- Mapping persisted to `/etc/precrime/report.conf` so the operator's muscle memory holds across sessions
- Future MEZZANINE controller appears to REPORT as a USB HID keyboard — no software changes needed when hardware controller is built

### Discovery

- Daemon listens for NDI source announcements via `libndi`'s `NDIlib_find_*` API on a background thread
- New `PRECOG-NN-*` sources are bound to the next free input-selector pad automatically and appear in the multiview
- Sources that disappear are gracefully unbound; their tile shows a "SIGNAL LOST" placeholder until they return

### Boot behavior

- Power on → systemd starts `report.service`
- GStreamer pipelines launch immediately, before any sources exist
- "WAITING FOR NDI SOURCES" placeholder shown on both HDMI outputs
- As PRECOGs appear on the network they bind automatically and become switchable
- Total cold-boot to operator-ready: target <30 seconds

### Failure modes

| Failure | Behavior |
|---|---|
| Source disappears mid-show | Tile shows "SIGNAL LOST"; if it was the active program source, REPORT auto-cuts to the lowest-numbered live source and beeps the keyboard speaker |
| All sources disappear | Black with "NO SOURCES" overlay on both HDMI; daemon continues running, will re-bind sources as they return |
| `libndi` init failure | systemd restarts the service; if it fails 3× in 60s, surfaces error on HDMI-A-1 and waits for manual intervention |
| HDMI cable unplugged | `kmssink` survives; reattaches on replug |

## 4. PRECOG Kit A — Pi NDI Encoder for Analog CCTV

### Hardware

- Raspberry Pi 5 4GB
- Active cooler + vented case
- 32GB A2 microSD
- USB analog video capture: **EasyCap UTV007** chipset (deliberately lo-fi 480i look that suits the aesthetic)
  - Backup option if UTV007 unstable on Pi 5: StarTech SVID2USB23 (~$80, cleaner driver path)
- USB-C PD power brick (wall) or 10000mAh PD power bank (untethered)
- 1/4" mount on case for tripod attachment
- BNC → RCA adapter to bridge vintage CCTV cam output to EasyCap RCA input

### Software

- Raspberry Pi OS Lite 64-bit
- GStreamer 1.22+ with `gstreamer1.0-plugins-rs` (provides `ndisink`)
- **Single Rust binary** `precog` (cargo workspace member) that:
  - Reads `/etc/precog/precog.conf` (TOML)
  - Builds a `v4l2src → caps → videoconvert → ndisinkcombiner → ndisink` GStreamer pipeline via `gstreamer-rs`
  - Watches the GStreamer bus for errors and logs via `tracing`
  - Exits non-zero on fatal pipeline failure; `systemd` restarts
- `systemd` service: `precog.service` execs `/usr/local/bin/precog` directly, autostarts at boot
- NDI source name set per `/etc/precog/precog.conf` (typically `PRECOG-NN-CCTV-<LOC>`)
- Same binary used for any analog-CCTV PRECOG — only the config file differs per unit

### Boot

Plug in power → joins WiFi from saved credentials → publishes NDI source within ~20 seconds of cold boot.

### Power

- Wall power preferred for shows
- Battery (10000mAh PD bank): ~4 hours runtime, fine for setup/short pieces

## 5. PRECOG Kit B — Phone NDI Source

### Hardware

- iPhone 15 (owned, Phase 1)
- Phone tripod mount/clamp with 1/4" thread
- USB-C PD cable + power brick (NDI HX Camera drains battery quickly — always wired for shows)

### Software

- **NDI HX Camera** (free, App Store, by NewTek)
- iPhone device name set to `PRECOG-NN-IPHONE-<LOC>` in Settings → General → About → Name
  (NDI HX Camera broadcasts using the device name)
- Launch app → tap "Start" → it is now an NDI source

### Power management

- iPhone 15 streaming NDI|HX: roughly 3 hours on battery alone
- Always run plugged-in for shows over 1 hour

### Remote control

- NDI HX Camera exposes exposure, focus, and white balance over NDI itself; REPORT or a separate monitoring station can adjust these per source
- Deeper phone-level control (force-restart the app, lock to single-app mode, reboot) requires **Apple Configurator** profile or iOS Shortcuts automation — deferred to Phase 2

## 6. Network Brain (Phase 1)

### Hardware

- **GL.iNet Flint 2** (GL-MT6000) router

### Configuration

| Setting | Value | Reason |
|---|---|---|
| SSID (5 GHz) | `precrime-lan` | Primary band for cams + REPORT |
| SSID (2.4 GHz) | hidden | Reduce interference, only enable if needed for compatibility |
| Encryption | WPA3 (WPA2 fallback) | |
| DHCP range | `192.168.50.100`–`192.168.50.200` | |
| Static reservations | REPORT at `.10`, each PRECOG by MAC at `.20`+ | Predictable addressing, easier debugging |
| **mDNS reflector** | **ENABLED**, all interfaces | **NDI auto-discovery depends on this** |
| **Client isolation** | **DISABLED** | Cams must reach REPORT |
| **IGMP snooping** | **ENABLED with querier** | Clean multicast for NDI |
| Boot config backup | exported to USB stick | Can flash a replacement Flint 2 in <5 min if router fails |

### Future expansion

- WAN port plugs into venue ethernet → uplink for streaming + remote config without exposing the cam LAN
- Phase 3: swap to Flint 3 (WiFi 6E/7, 6 GHz band) for venue spectrum win once 4+ cams routinely on WiFi

## 7. Phase 1 Build Order and Milestones

### Week 1 — Network and camera validation

- **M1** — Flint 2 configured with `precrime-lan` SSID up; mDNS reflector verified by running `avahi-browse -a` from a laptop
- **M2** — iPhone 15 with NDI HX Camera visible on the network from a laptop running NDI Studio Monitor
- **M3** — Pi 5 CCTV encoder built; `ffmpeg`+`libndi` (or GStreamer) service publishes a test pattern → visible in Studio Monitor
- **M4** — Real CCTV cam connected via EasyCap, live video in the NDI stream

### Week 2-3 — REPORT switcher software

- **M5** — Pi 5 REPORT boots headless; GStreamer pipeline displays a single NDI source on HDMI-A-1
- **M6** — Multi-source `input-selector` working; USB keyboard switches between sources
- **M7** — HDMI-A-2 pipeline shows the multiview compositor (2×2 grid)
- **M8** — Tally overlay on multiview marks the active source

### Week 4 — Integration

- **M9** — Two PRECOGs (phone + CCTV) running, REPORT cuts between them live
- **M10** — Boot-time autostart via `systemd` verified; survives unplug/replug of cameras
- **M11** — Documented startup/teardown procedure for touring deployment

Completion of M11 marks Phase 1 done.

## 8. Phase 1 Bill of Materials

| Item | Source | Cost |
|---|---|---|
| iPhone 15 | owned | $0 |
| Vintage CCTV cam | owned | $0 |
| GL.iNet Flint 2 (GL-MT6000) | Amazon / GL.iNet direct | $150 |
| Pi 5 8GB (REPORT) | Adafruit / PiShop | $80 |
| Pi 5 4GB (PRECOG encoder) | Adafruit / PiShop | $60 |
| Active cooler × 2 | $5 ea | $10 |
| Pi case × 2 (vented) | $15 ea | $30 |
| microSD A2 (32GB + 64GB) | $10 + $15 | $25 |
| EasyCap UTV007 USB capture | Amazon | $15 |
| BNC → RCA adapter | Amazon | $5 |
| USB-C PD power brick × 2 | $15 ea | $30 |
| 10000mAh PD power bank | $25 | $25 |
| Phone tripod mount + clamp | $15 | $15 |
| Cheap USB keyboard | $15 | $15 |
| Micro-HDMI → HDMI cable × 2 | $10 ea | $20 |
| Short Cat6 patch cable × 2 | $5 ea | $10 |
| **Subtotal** | | **~$490** |

Output monitor and HDMI capture for Phase 1 borrow from existing gear.

## 9. Risks and Mitigations

| Risk | Mitigation |
|---|---|
| WiFi congestion at venues kills NDI streams | Phase 3 upgrade to Flint 3 (6 GHz band). Phase 1: keep one spare 100ft Cat6 to wire a critical PRECOG if WiFi degrades |
| `gst-plugin-ndi` has Pi 5 / ARM build issues | Fall back to `ffmpeg`+`libndi` on the encoder side; validate the toolchain early at M3 |
| NDI HX Camera app freezes mid-show | Apple Configurator "single app mode" + remote reboot path; deferred to Phase 2 spec |
| Power runs out mid-show | All cams on USB-C PD wall power for shows over 1 hour; battery only for setup or short pieces |
| Pi 5 thermal throttling under sustained load | Active cooler mandatory, vented cases, no stacking |
| EasyCap UTV007 driver quirks on Pi 5 | Test at M3 before committing; backup capture device (StarTech SVID2USB23) on standby |
| Single GL.iNet unit dies mid-tour | Boot config backed up to USB stick; spare Flint 2 ($150) optional but cheap insurance once touring regularly |

## 10. Repository Layout

```
/Users/cody/Dev/precrime/
├── Cargo.toml                 # cargo workspace root (members: report, precog)
├── Cargo.lock
├── .docs/
│   ├── ROADMAP.md             # compact index (per global roadmap pattern)
│   ├── ROADMAP-SPECS.md       # roadmap detail specs
│   └── COMPLETED.md
├── docs/
│   └── superpowers/
│       ├── specs/
│       └── plans/
├── report/                    # REPORT switcher binary crate + deployment artifacts
│   ├── Cargo.toml
│   ├── src/                   # Rust source (main.rs, daemon.rs, pipeline.rs, ...)
│   ├── tests/                 # integration tests for pure-Rust modules
│   ├── report.conf.example    # /etc/precrime/report.conf template
│   ├── report.service         # systemd unit
│   ├── install.sh             # apt deps + libndi + rustup installer (run on Pi)
│   └── runbook.md
├── precog/                    # PRECOG encoder binary crate + deployment artifacts
│   ├── Cargo.toml
│   ├── src/
│   ├── precog.conf.example
│   ├── precog.service
│   ├── install.sh
│   └── kit-a-cctv-runbook.md
├── network/                   # Router config exports, mDNS notes, deployment runbooks
├── hardware/                  # MEZZANINE controller firmware (Phase 2+)
└── README.md
```

Git initialized at the project root. Phase 1 work happens primarily in `report/`, `precog/`, and `network/`. Build with `cargo build --release` from the workspace root; deploy binaries via `scp target/release/{report,precog} <host>:/usr/local/bin/`.

## 11. Out of Scope (Phase 1)

Explicitly deferred. **Phase 2 immediate next** is the Remote Phone Control + Smart Plug Bus — to be tackled as soon as Phase 1 verifies end-to-end connectivity.

### Phase 2 — immediate next

- **Remote phone control + smart plug bus** — Apple Configurator + Single App Mode for iPhone PRECOGs to lock them to NDI HX Camera and auto-relaunch on crash. Tasmota/Shelly smart plugs per permanently-deployed PRECOG, controlled via a `mosquitto` MQTT broker hosted on REPORT or the router. A small `precog` CLI (`precog reboot 01`) to power-cycle any PRECOG by name when it freezes. Sub-projects: § Phone Provisioning Profiles, § Smart Plug Bus. Bespoke iOS/Android apps deferred further — off-the-shelf kiosk tooling covers the need at PRECRIME's current scale.

### Other deferrals

- MEZZANINE custom hardware controller (USB keyboard placeholder)
- PRECOG Kits C (IP PoE) and D (HDMI source)
- RTSP→NDI bridge service
- Recording-to-disk archive on REPORT
- Multi-operator / remote control of REPORT
- Software-applied CCTV aesthetic filters (scanlines, timestamp burn, downscale) — defer until at least one show has happened and the aesthetic call is informed
- Pelican case / road case packout
- Streaming output config (RTMP/SRT to Twitch/YouTube)
- Bespoke Android companion app (consider only if fleet grows past 4 Android units)

Each of these gets a future brainstorm pass before any work begins.
