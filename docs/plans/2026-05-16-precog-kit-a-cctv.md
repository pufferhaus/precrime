# PRECOG Kit A (Pi NDI Encoder for Analog CCTV) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

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

  echo "Installing GStreamer + Rust plugins + v4l-utils..."
  sudo apt update
  sudo apt install -y \
      gstreamer1.0-tools \
      gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad \
      gstreamer1.0-plugins-ugly \
      gstreamer1.0-plugins-rs \
      v4l-utils \
      curl

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

### Task 6: Wrap the pipeline in a systemd service

**Files:**
- Create: `precog/precog.conf.example`
- Create: `precog/precog-launcher.sh`
- Create: `precog/precog.service`
- Modify: `precog/kit-a-cctv-runbook.md`

- [ ] **Step 1: Create the example config file `precog/precog.conf.example`**

  ```bash
  # PRECOG runtime config. Copy to /etc/precog/precog.conf on each unit and edit per cam.
  # Loaded by /usr/local/bin/precog-launcher.sh, which exports these as env vars to gst-launch.

  PRECOG_NAME="PRECOG-02-CCTV-DOOR"
  PRECOG_DEVICE="/dev/video0"
  PRECOG_FORMAT="UYVY"
  PRECOG_WIDTH="720"
  PRECOG_HEIGHT="480"
  PRECOG_FRAMERATE="30/1"

  # Pipeline variant: "with-combiner" or "video-only"
  # Determined by Task 4 fallback in the kit-a plan.
  PRECOG_PIPELINE_VARIANT="with-combiner"
  ```

- [ ] **Step 2: Create the launcher script `precog/precog-launcher.sh`**

  ```bash
  #!/bin/sh
  # PRECOG pipeline launcher. Reads /etc/precog/precog.conf and execs gst-launch-1.0.
  set -e

  CONFIG=/etc/precog/precog.conf
  if [ ! -f "$CONFIG" ]; then
      echo "Missing $CONFIG" >&2
      exit 1
  fi
  . "$CONFIG"

  : "${PRECOG_NAME:?must set PRECOG_NAME}"
  : "${PRECOG_DEVICE:?must set PRECOG_DEVICE}"
  : "${PRECOG_FORMAT:?must set PRECOG_FORMAT}"
  : "${PRECOG_WIDTH:?must set PRECOG_WIDTH}"
  : "${PRECOG_HEIGHT:?must set PRECOG_HEIGHT}"
  : "${PRECOG_FRAMERATE:?must set PRECOG_FRAMERATE}"

  CAPS="video/x-raw,format=${PRECOG_FORMAT},width=${PRECOG_WIDTH},height=${PRECOG_HEIGHT},framerate=${PRECOG_FRAMERATE}"

  case "${PRECOG_PIPELINE_VARIANT:-with-combiner}" in
      with-combiner)
          exec gst-launch-1.0 \
              v4l2src device="$PRECOG_DEVICE" \
              ! "$CAPS" \
              ! videoconvert \
              ! ndisinkcombiner name=c \
              c.src ! ndisink ndi-name="$PRECOG_NAME"
          ;;
      video-only)
          exec gst-launch-1.0 \
              v4l2src device="$PRECOG_DEVICE" \
              ! "$CAPS" \
              ! videoconvert \
              ! ndisink ndi-name="$PRECOG_NAME"
          ;;
      *)
          echo "Unknown PRECOG_PIPELINE_VARIANT: $PRECOG_PIPELINE_VARIANT" >&2
          exit 2
          ;;
  esac
  ```

- [ ] **Step 3: Create the systemd unit `precog/precog.service`**

  ```ini
  [Unit]
  Description=PRECOG NDI encoder (analog CCTV via EasyCap)
  After=network-online.target
  Wants=network-online.target

  [Service]
  Type=simple
  ExecStart=/usr/local/bin/precog-launcher.sh
  Restart=on-failure
  RestartSec=3
  StandardOutput=journal
  StandardError=journal
  User=root

  [Install]
  WantedBy=multi-user.target
  ```

  Note: running as root because v4l2 + udev rules vary; if you prefer non-root, add the user to the `video` group on the Pi.

- [ ] **Step 4: Deploy these files to the Pi**

  From your laptop:
  ```bash
  scp precog/precog.conf.example cody@precog-02-cctv-door.local:/tmp/
  scp precog/precog-launcher.sh cody@precog-02-cctv-door.local:/tmp/
  scp precog/precog.service cody@precog-02-cctv-door.local:/tmp/

  ssh cody@precog-02-cctv-door.local '
      sudo mkdir -p /etc/precog &&
      sudo cp /tmp/precog.conf.example /etc/precog/precog.conf &&
      sudo cp /tmp/precog-launcher.sh /usr/local/bin/precog-launcher.sh &&
      sudo chmod +x /usr/local/bin/precog-launcher.sh &&
      sudo cp /tmp/precog.service /etc/systemd/system/precog.service &&
      sudo systemctl daemon-reload &&
      sudo systemctl enable precog.service &&
      sudo systemctl start precog.service
  '
  ```

- [ ] **Step 5: Verify the service is running**

  ```bash
  ssh cody@precog-02-cctv-door.local 'sudo systemctl status precog.service'
  ```

  Expected: `active (running)` with recent log lines showing GStreamer pipeline output. If failed, check:
  ```bash
  ssh cody@precog-02-cctv-door.local 'sudo journalctl -u precog.service -n 50 --no-pager'
  ```

- [ ] **Step 6: Verify the NDI source still appears in Studio Monitor**

  Same as Task 5 Step 4. Source should be live again without you running anything manually.

- [ ] **Step 7: Verify it survives a reboot**

  ```bash
  ssh cody@precog-02-cctv-door.local 'sudo reboot'
  ```

  Wait ~45 seconds. Open NDI Studio Monitor on the laptop. Expected: `PRECOG-02-CCTV-DOOR` reappears within ~30 seconds of the Pi finishing boot. Stream is live.

- [ ] **Step 8: Append boot-survival check to runbook**

  Append to `precog/kit-a-cctv-runbook.md`:

  ```markdown
  ## Boot-survival verification — YYYY-MM-DD
  - `precog.service` enabled, autostarts at boot
  - Cold-boot → NDI source live: ~<observed seconds>
  ```

- [ ] **Step 9: Commit**

  ```bash
  git add precog/precog.conf.example precog/precog-launcher.sh precog/precog.service precog/kit-a-cctv-runbook.md
  git commit -m "precog kit-a: systemd service + launcher + boot-survival verified"
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
├── install.sh               # Reproducible package install + NDI SDK pointer
├── precog.conf.example      # Template config for /etc/precog/precog.conf
├── precog-launcher.sh       # Shell script that execs gst-launch-1.0 from config
├── precog.service           # systemd unit
├── kit-a-cctv-runbook.md    # This kit's operator runbook
└── kit-b-iphone-runbook.md  # (from Kit B plan)
```

`precog-launcher.sh` and `precog.conf.example` are reusable for additional analog-CCTV PRECOGs — just change `PRECOG_NAME` and the hostname per unit.

## Done means

- M3 verified: test pattern → NDI works
- M4 verified: real CCTV cam → NDI works
- systemd service runs reliably across reboot
- Show-day checklist written
