# PRECOG Kit A (Pi NDI Encoder for Analog CCTV) Implementation Plan

**Goal:** Build a Pi 5 + EasyCap-based NDI encoder that publishes a vintage analog CCTV camera's BNC composite feed as the NDI source `PRECOG-02-CCTV-DOOR` on the PRECRIME LAN.

**Architecture:** The Pi runs Raspberry Pi OS Lite headless. A single GStreamer pipeline reads `/dev/video0` (the EasyCap USB capture device), encodes to NDI|HX2 via `ndisink` from the `gstreamer1.0-plugins-rs` package, and is wrapped in a `systemd` service with auto-restart. Configuration (NDI name, resolution, framerate) lives in `/etc/precog/precog.conf` and is interpolated into the pipeline at service start. No Python; a single shell launcher keeps the runtime tiny and debuggable.

**Tech Stack:** Raspberry Pi OS 12 (Debian bookworm, 64-bit, Lite), GStreamer 1.22+, `gstreamer1.0-plugins-rs` (provides `ndisink`), NewTek NDI SDK runtime libraries (`libndi.so`), `v4l2-utils` for device probing, `systemd`.

**Milestones covered:** M3, M4 from system spec.

**Depends on:** Network Brain plan complete (M1).

---

### Task 1: Flash Pi OS and headless first-boot

**Files:**
- Create: `precog/kit-a-cctv-runbook.md`

- [ ] **Step 1: Download Raspberry Pi Imager on your laptop**

  From `https://www.raspberrypi.com/software/`. Install.

- [ ] **Step 2: Insert the 32GB microSD into a card reader and launch Pi Imager**

- [ ] **Step 3: Configure the image with headless options before flashing**

  - Device: Raspberry Pi 5
  - OS: Raspberry Pi OS Lite (64-bit)
  - Storage: the microSD
  - Click the gear icon (advanced settings):
    - Hostname: `precog-02-cctv-door` (lowercase; we set the NDI display name separately)
    - Enable SSH with password auth
    - Username: `cody`, password: pick a strong one (record in password manager)
    - Configure wireless LAN: SSID `precrime-lan`, password from password manager, country code your locale
    - Set locale settings

  Flash.

- [ ] **Step 4: Insert microSD into Pi 5, attach active cooler, boot**

  The Pi takes ~60 seconds on first boot to expand the filesystem.

- [ ] **Step 5: SSH from your laptop**

  ```bash
  ssh cody@precog-02-cctv-door.local
  ```

  Expected: prompt for password (set in Step 3), login succeeds. If `.local` resolution fails, find the IP via the router (browse to `http://192.168.50.1`, look at the connected clients list) and SSH to that IP.

- [ ] **Step 6: Update the system and reboot**

  ```bash
  sudo apt update && sudo apt full-upgrade -y
  sudo reboot
  ```

  Wait ~30 seconds, reconnect via SSH.

- [ ] **Step 7: Create the runbook stub**

  In the repo on your laptop, create `precog/kit-a-cctv-runbook.md`:

  ```markdown
  # PRECOG Kit A — CCTV Encoder Runbook

  ## Identity
  - Hostname: `precog-02-cctv-door`
  - NDI display name: `PRECOG-02-CCTV-DOOR`
  - Hardware: Pi 5 4GB + active cooler + EasyCap UTV007 + vintage CCTV cam

  ## Setup history
  - 2026-05-16: Initial provision per precog-kit-a-cctv plan
  ```

- [ ] **Step 8: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add precog/kit-a-cctv-runbook.md
  git commit -m "precog kit-a: initial Pi 5 runbook"
  ```

---

### Task 2: Install GStreamer + NDI dependencies

**Files:**
- Create: `precog/install.sh`

- [ ] **Step 1: SSH to the Pi and install GStreamer + the Rust plugins**

  ```bash
  ssh cody@precog-02-cctv-door.local
  sudo apt install -y \
      gstreamer1.0-tools \
      gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad \
      gstreamer1.0-plugins-ugly \
      gstreamer1.0-plugins-rs \
      v4l-utils \
      curl
  ```

- [ ] **Step 2: Download and install the NDI SDK runtime**

  The Rust `ndi` plugin dynamically loads `libndi.so` from the NDI Advanced SDK. Download from `https://ndi.video/sdk/` (free, requires email signup, accept the EULA).

  On the Pi:
  ```bash
  cd ~
  # Replace URL with the current ARM Linux SDK link from the NDI download page
  curl -L -o ndi-sdk.tar.gz "<URL_FROM_NDI_DOWNLOAD_PAGE>"
  tar xzf ndi-sdk.tar.gz
  cd "NDI SDK for Linux/lib/aarch64-rpi4-linux-gnueabi"
  sudo cp libndi.so* /usr/local/lib/
  sudo ldconfig
  ```

  If the SDK download path differs by version, navigate to whatever directory contains `libndi.so.*` for `aarch64`. Pi 5 is ARM64; pick the `aarch64` variant.

- [ ] **Step 3: Verify the ndisink element is available**

  ```bash
  gst-inspect-1.0 ndisink
  ```

  Expected: a long output describing the `ndisink` element, its pads, and properties including `ndi-name`. **If you see `No such element or plugin 'ndisink'`, libndi did not load — check `ldconfig -p | grep libndi` and re-run Step 2.**

- [ ] **Step 4: Write the install script for reproducibility**

  In the repo on your laptop, create `precog/install.sh`:

  ```bash
  #!/bin/sh
  # PRECOG Kit A installer — run on a fresh Pi OS Lite 64-bit with internet access
  set -e

  echo "Installing GStreamer + Rust plugins + build deps + v4l-utils..."
  sudo apt update
  sudo apt install -y \
      build-essential pkg-config curl \
      libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
      gstreamer1.0-tools \
      gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad \
      gstreamer1.0-plugins-ugly \
      gstreamer1.0-plugins-rs \
      libudev-dev \
      v4l-utils

  if ! command -v rustc >/dev/null 2>&1; then
      echo "Installing rustup + stable Rust..."
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  fi

  echo ""
  echo "Next: manually install the NDI SDK runtime libndi.so per the runbook."
  echo "Then run: gst-inspect-1.0 ndisink"
  echo "If it succeeds, install is complete."
  ```

  Make it executable:
  ```bash
  chmod +x precog/install.sh
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add precog/install.sh
  git commit -m "precog kit-a: install script + NDI SDK setup documented"
  ```

---

### Task 3: Identify EasyCap capture device and supported formats

**Files:**
- Modify: `precog/kit-a-cctv-runbook.md`

- [ ] **Step 1: Plug the EasyCap UTV007 into a USB port on the Pi**

  No CCTV cam attached yet — empty input is fine.

- [ ] **Step 2: List V4L2 devices**

  ```bash
  ssh cody@precog-02-cctv-door.local
  v4l2-ctl --list-devices
  ```

  Expected: an entry like
  ```
  USB2.0 PC CAMERA: USB2.0 PC CAM (usb-xhci-hcd.0-1):
      /dev/video0
  ```

  Note the device path. **If no device shows up, the EasyCap is not recognized — try another USB port, check `dmesg | tail -20` for kernel errors, and confirm the chipset is UTV007 (some lookalikes need different drivers).**

- [ ] **Step 3: List supported formats and resolutions**

  ```bash
  v4l2-ctl -d /dev/video0 --list-formats-ext
  ```

  Expected output includes `UYVY` (or `YUYV`) at `720x480` and/or `720x576` (NTSC/PAL respectively), typically at 30fps for NTSC or 25fps for PAL.

  **Record the exact format string and resolution you'll use** — you'll plug them into the pipeline next task.

- [ ] **Step 4: Set the input source (composite vs S-Video)**

  EasyCap exposes multiple inputs. List them:
  ```bash
  v4l2-ctl -d /dev/video0 --list-inputs
  ```

  Expected: at least one input, often "Composite" at index 0 and "S-Video" at index 1. Select composite:
  ```bash
  v4l2-ctl -d /dev/video0 -i 0
  ```

- [ ] **Step 5: Set TV norm (NTSC vs PAL) based on your CCTV cam**

  Most US/Japan CCTV cams are NTSC; most European cams are PAL.
  ```bash
  v4l2-ctl -d /dev/video0 -s NTSC      # or PAL
  ```

- [ ] **Step 6: Append findings to the runbook**

  Append to `precog/kit-a-cctv-runbook.md`:

  ```markdown
  ## Capture hardware
  - Device: `/dev/video0` (EasyCap UTV007)
  - Input: 0 = Composite
  - TV norm: <NTSC or PAL>
  - Pixel format: `UYVY` (or `YUYV`)
  - Resolution: 720x480 (NTSC) or 720x576 (PAL)
  - Frame rate: 30/1 (NTSC) or 25/1 (PAL)
  ```

- [ ] **Step 7: Commit**

  ```bash
  git add precog/kit-a-cctv-runbook.md
  git commit -m "precog kit-a: documented EasyCap V4L2 device + format"
  ```

---

### Task 4: M3 — test pattern → NDI (no real cam yet)

This task proves the entire NDI publishing pipeline works using GStreamer's built-in test source. Smoke-tests the install before adding hardware variables.

- [ ] **Step 1: On the Pi, run a test pattern to NDI as a one-liner**

  ```bash
  gst-launch-1.0 -v \
      videotestsrc is-live=true \
      ! video/x-raw,format=UYVY,width=720,height=480,framerate=30/1 \
      ! ndisinkcombiner name=c \
      c.src ! ndisink ndi-name="PRECOG-02-CCTV-DOOR"
  ```

  Expected: console output shows pipeline state transitioning to PLAYING and frames flowing. Leave it running.

  **If you see "no element 'ndisinkcombiner'":** older `gst-plugin-ndi` versions don't ship the combiner. Try:
  ```bash
  gst-launch-1.0 -v \
      videotestsrc is-live=true \
      ! video/x-raw,format=UYVY,width=720,height=480,framerate=30/1 \
      ! ndisink ndi-name="PRECOG-02-CCTV-DOOR"
  ```

  Use whichever variant works and note it in the runbook.

- [ ] **Step 2: From a laptop on `precrime-lan`, open NDI Studio Monitor**

  Expected: `PRECOG-02-CCTV-DOOR` appears in the source list. Click it. The SMPTE color bar test pattern appears. **M3 milestone passes.**

- [ ] **Step 3: Append M3 verification to runbook**

  Append to `precog/kit-a-cctv-runbook.md`:

  ```markdown
  ## M3 verification — YYYY-MM-DD
  - Test pattern (`videotestsrc`) successfully published as NDI source
  - Visible in NDI Studio Monitor on laptop
  - Pipeline variant used: <with combiner | without combiner>
  ```

- [ ] **Step 4: Commit**

  ```bash
  git add precog/kit-a-cctv-runbook.md
  git commit -m "precog kit-a: M3 verified, test pattern published as NDI"
  ```

---

### Task 5: M4 — real CCTV cam capture and publish

- [ ] **Step 1: Connect the vintage CCTV cam**

  Power on the CCTV cam (12V wall wart or whatever its power source is). Connect its BNC composite output through a BNC→RCA adapter into the EasyCap's yellow RCA video input.

- [ ] **Step 2: Probe the live signal with a one-shot frame grab**

  ```bash
  ssh cody@precog-02-cctv-door.local
  v4l2-ctl -d /dev/video0 --stream-mmap=3 --stream-count=1 --stream-to=/tmp/frame.raw
  ls -la /tmp/frame.raw
  ```

  Expected: file of ~700KB (720*480*2 bytes for UYVY). If the file is empty or all-zero, the EasyCap isn't seeing signal — check cabling and that the cam is producing video.

- [ ] **Step 3: Run the v4l2 → NDI pipeline live**

  ```bash
  gst-launch-1.0 -v \
      v4l2src device=/dev/video0 \
      ! video/x-raw,format=UYVY,width=720,height=480,framerate=30/1 \
      ! videoconvert \
      ! ndisinkcombiner name=c \
      c.src ! ndisink ndi-name="PRECOG-02-CCTV-DOOR"
  ```

  (Omit `ndisinkcombiner` if Task 4 Step 1 fallback was needed.)

  Adjust resolution/framerate to whatever you recorded in Task 3 Step 6.

  Expected: pipeline runs, leave it.

- [ ] **Step 4: Verify in NDI Studio Monitor on the laptop**

  Expected: real CCTV image streams from the camera. The 480i analog look is unmistakable — interlace artifacts, mild color bleeding, low resolution. **This is the aesthetic; this is M4 passing.**

- [ ] **Step 5: Append M4 verification to runbook**

  Append to `precog/kit-a-cctv-runbook.md`:

  ```markdown
  ## M4 verification — YYYY-MM-DD
  - Real CCTV camera <model/notes> connected via BNC→RCA → EasyCap
  - Live composite video successfully published as NDI source
  - Pipeline:
    ```
    <paste exact gst-launch-1.0 command that worked>
    ```
  ```

- [ ] **Step 6: Commit**

  ```bash
  git add precog/kit-a-cctv-runbook.md
  git commit -m "precog kit-a: M4 verified, live CCTV → NDI working"
  ```

---

### Task 6: Build the precog Rust binary + wrap in systemd

The precog binary is a small Rust program that reads a TOML config, constructs a v4l2→NDI GStreamer pipeline via `gstreamer-rs`, runs it, watches the bus, and exits non-zero on fatal errors (systemd restarts it).

**Files:**
- Create: `precog/Cargo.toml`
- Create: `precog/src/main.rs`
- Create: `precog/src/config.rs`
- Create: `precog/precog.conf.example`
- Create: `precog/precog.service`
- Modify: `precog/kit-a-cctv-runbook.md`
- Modify: workspace `Cargo.toml` to include `precog` (already done if Task 3 of REPORT plan ran first)

- [ ] **Step 1: Create `precog/Cargo.toml`**

  ```toml
  [package]
  name = "precog"
  version = "0.1.0"
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  authors.workspace = true
  description = "PRECRIME PRECOG: analog CCTV → NDI encoder daemon"

  [dependencies]
  anyhow = "1"
  serde = { version = "1", features = ["derive"] }
  toml = "0.8"
  tracing = "0.1"
  tracing-subscriber = { version = "0.3", features = ["env-filter"] }
  tracing-journald = "0.3"
  gstreamer = "0.23"

  [lints]
  workspace = true
  ```

- [ ] **Step 2: Create `precog/src/config.rs`**

  ```rust
  //! TOML config parsing for precog.

  use serde::Deserialize;

  #[derive(Debug, Deserialize)]
  pub struct PrecogConfig {
      /// NDI display name, e.g. "PRECOG-02-CCTV-DOOR".
      pub ndi_name: String,
      /// V4L2 device path, e.g. "/dev/video0".
      pub device: String,
      /// Pixel format string, e.g. "UYVY" or "YUYV".
      pub format: String,
      pub width: u32,
      pub height: u32,
      /// "30/1" for NTSC, "25/1" for PAL.
      pub framerate: String,
      /// Some installs of gst-plugin-rs need `ndisinkcombiner`; others go straight to `ndisink`.
      /// Default true (with combiner).
      #[serde(default = "default_combiner")]
      pub use_combiner: bool,
  }

  fn default_combiner() -> bool {
      true
  }

  impl PrecogConfig {
      pub fn from_toml(raw: &str) -> Result<Self, toml::de::Error> {
          toml::from_str(raw)
      }
  }
  ```

- [ ] **Step 3: Create `precog/src/main.rs`**

  ```rust
  //! PRECOG — analog CCTV → NDI encoder daemon.

  mod config;

  use anyhow::{Context, Result};
  use config::PrecogConfig;
  use gstreamer::prelude::*;
  use std::env;
  use std::fs;
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

      let bus = pipeline.bus().context("pipeline bus")?;
      for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
          use gstreamer::MessageView;
          match msg.view() {
              MessageView::Eos(..) => {
                  warn!("EOS received, exiting");
                  break;
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
      let name_escaped = cfg.ndi_name.replace('"', "");
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
  ```

- [ ] **Step 4: Create `precog/precog.conf.example`**

  ```toml
  # /etc/precog/precog.conf — copy and edit per unit.

  ndi_name  = "PRECOG-02-CCTV-DOOR"
  device    = "/dev/video0"
  format    = "UYVY"
  width     = 720
  height    = 480
  framerate = "30/1"
  # Set to false if `ndisinkcombiner` is unavailable in your gst-plugin-rs build (see kit-a plan Task 4).
  use_combiner = true
  ```

- [ ] **Step 5: Create `precog/precog.service`**

  ```ini
  [Unit]
  Description=PRECOG NDI encoder (analog CCTV via EasyCap)
  After=network-online.target
  Wants=network-online.target

  [Service]
  Type=simple
  ExecStart=/usr/local/bin/precog
  Environment=PRECOG_CONFIG=/etc/precog/precog.conf
  Environment=RUST_LOG=info
  Restart=on-failure
  RestartSec=3
  StandardOutput=journal
  StandardError=journal
  User=root

  [Install]
  WantedBy=multi-user.target
  ```

  Note: running as root for v4l2 + udev. To run as a non-root user, add them to the `video` and `render` groups; defer this hardening to a future iteration.

- [ ] **Step 6: Build the binary on the PRECOG Pi**

  This Pi needs the same toolchain as REPORT — install rustup + GStreamer dev packages. Use the REPORT install.sh as a reference (it's identical for our purposes):

  ```bash
  rsync -av --delete --exclude target/ /Users/cody/Dev/precrime/ cody@precog-02-cctv-door.local:/home/cody/precrime/
  ssh cody@precog-02-cctv-door.local '
      cd ~/precrime &&
      ./report/install.sh &&
      cargo build --release -p precog
  '
  ```

  (NDI SDK runtime must also be installed on this Pi — see REPORT plan Task 2 Step 3 for the manual libndi.so install.)

  Expected: `~/precrime/target/release/precog` exists.

- [ ] **Step 7: Deploy binary, config, service**

  ```bash
  ssh cody@precog-02-cctv-door.local '
      sudo cp ~/precrime/target/release/precog /usr/local/bin/precog &&
      sudo mkdir -p /etc/precog &&
      sudo cp ~/precrime/precog/precog.conf.example /etc/precog/precog.conf &&
      sudo cp ~/precrime/precog/precog.service /etc/systemd/system/precog.service &&
      sudo systemctl daemon-reload &&
      sudo systemctl enable precog.service &&
      sudo systemctl start precog.service
  '
  ```

  Edit `/etc/precog/precog.conf` on the Pi with the actual per-unit values (NDI name, NTSC vs PAL framerate, etc).

- [ ] **Step 8: Verify the service is running**

  ```bash
  ssh cody@precog-02-cctv-door.local 'sudo systemctl status precog.service'
  ```

  Expected: `active (running)`. If failed:
  ```bash
  ssh cody@precog-02-cctv-door.local 'sudo journalctl -u precog.service -n 80 --no-pager'
  ```

- [ ] **Step 9: Verify NDI source is live**

  From the laptop, NDI Studio Monitor should show `PRECOG-02-CCTV-DOOR` with the live CCTV feed.

- [ ] **Step 10: Reboot and verify boot survival**

  ```bash
  ssh cody@precog-02-cctv-door.local 'sudo reboot'
  ```

  Wait ~45 seconds. NDI source reappears in Studio Monitor.

- [ ] **Step 11: Append boot-survival check to runbook**

  Append to `precog/kit-a-cctv-runbook.md`:

  ```markdown
  ## Boot-survival verification — YYYY-MM-DD
  - `precog.service` (Rust binary) enabled, autostarts at boot
  - Cold-boot → NDI source live: ~<observed seconds>
  ```

- [ ] **Step 12: Commit**

  ```bash
  git add precog/Cargo.toml precog/src/ precog/precog.conf.example precog/precog.service precog/kit-a-cctv-runbook.md
  git commit -m "precog kit-a: Rust binary + systemd service + boot-survival verified"
  ```

---

### Task 7: Show-day checklist and tear-down notes

**Files:**
- Modify: `precog/kit-a-cctv-runbook.md`

- [ ] **Step 1: Append show-day procedures to the runbook**

  Append to `precog/kit-a-cctv-runbook.md`:

  ```markdown
  ## Show-day pre-flight checklist (PRECOG-02-CCTV-DOOR)

  - [ ] CCTV cam powered up, video signal generating (verify cam's own activity LED if present)
  - [ ] BNC → RCA adapter seated, RCA in EasyCap yellow jack
  - [ ] EasyCap plugged into Pi 5 USB
  - [ ] Pi 5 plugged into USB-C PD power (wall or 10000mAh PD bank)
  - [ ] Pi 5 boots within ~30s
  - [ ] On the operator laptop / REPORT multiview: confirm `PRECOG-02-CCTV-DOOR` is live and showing the cam

  ## Tear-down
  - [ ] Power down Pi: `ssh cody@precog-02-cctv-door.local 'sudo shutdown -h now'`, wait for green LED to stop
  - [ ] Disconnect EasyCap and CCTV cam, coil cables
  - [ ] Pack into kit case

  ## Troubleshooting
  - **NDI source not appearing on the network:** check `sudo systemctl status precog.service` on the Pi. If failed, `sudo journalctl -u precog.service -n 50 --no-pager`.
  - **NDI source appears but no video (just black):** `v4l2-ctl -d /dev/video0 --stream-mmap=3 --stream-count=1 --stream-to=/tmp/frame.raw` and inspect. Likely a cam power or BNC cable issue.
  - **Color is off / interlace artifacts harsh:** correct, that is the CCTV aesthetic.
  ```

- [ ] **Step 2: Commit**

  ```bash
  git add precog/kit-a-cctv-runbook.md
  git commit -m "precog kit-a: show-day checklist + troubleshooting"
  ```

---

## File Structure Summary

```
precog/
├── Cargo.toml               # Rust binary crate (workspace member)
├── src/
│   ├── main.rs              # entry point + pipeline build + bus watch
│   └── config.rs            # TOML config
├── install.sh               # Reproducible apt + rustup installer
├── precog.conf.example      # Template config for /etc/precog/precog.conf
├── precog.service           # systemd unit (execs /usr/local/bin/precog)
├── kit-a-cctv-runbook.md    # This kit's operator runbook
└── kit-b-iphone-runbook.md  # (from Kit B plan)
```

The Rust binary is the same for every analog-CCTV PRECOG — only `/etc/precog/precog.conf` differs per unit.

## Done means

- M3 verified: test pattern → NDI works
- M4 verified: real CCTV cam → NDI works
- systemd service runs reliably across reboot
- Show-day checklist written
