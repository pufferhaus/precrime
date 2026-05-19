# PRECOG Kit A — CCTV Encoder Runbook

Pi (1GB or 4GB) + USB capture card + vintage CCTV cam. Captures analog composite video, encodes H.264, sends RTP multicast to REPORT. Announces itself via TEMPLE JSON ball.

## Identity

- Hostname: `precog-01` (set in `user-data.template` before flash)
- Source name: `PRECOG-01-CCTV` (set in `/etc/precog/precog.conf`)
- Hardware: Pi 5 + USB capture card (EasyCap UTV007 or similar) + vintage CCTV cam

---

## Hardware setup

**Connections:**
1. CCTV cam → BNC cable → BNC-to-RCA adapter → capture card yellow (video) jack
2. Capture card USB-A → Pi USB port (any; USB 2 is fine for composite)
3. Pi → USB-C PD power supply (≥3A)
4. Pi → Ethernet or WiFi to same network as REPORT (Ethernet preferred)

**Verify capture card is detected (once Pi is booted):**
```bash
v4l2-ctl --list-devices
# Expect: "USB Video" or "EasyCAP" on /dev/video0
v4l2-ctl -d /dev/video0 --list-formats-ext
# Expect: YUYV at 720x480 (NTSC) or 720x576 (PAL)
```

**Verify video signal:**
```bash
v4l2-ctl -d /dev/video0 --stream-mmap=3 --stream-count=1 --stream-to=/tmp/frame.raw
ls -lh /tmp/frame.raw   # should be ~600KB for a 720x480 frame
```
If size is near zero: check CCTV cam power, BNC cable, and adapter seating.

---

## First-time Pi setup

### 1. Flash SD card

Use Raspberry Pi Imager with **Raspberry Pi OS Lite (64-bit)**. In the OS customisation screen:
- Set hostname (e.g. `precog-01`)
- Set username: `pi`
- Enable SSH / paste your public key
- Do **not** set a password (key-only auth)

Then copy `precog/user-data.template` to the FAT32 boot partition as `user-data` before inserting the card. This file bakes in passwordless sudo so provisioning works from the mac without a terminal.

> **If you already flashed without the template:** See the SD card recovery procedure at the bottom of this file.

### 2. Boot Pi and verify SSH

```bash
ssh pi@precog-01.local 'hostname && sudo hostname'
# Both should print precog-01 with no password prompt
```

### 3. Provision from dev mac

Prerequisites on mac: Docker (Colima or Docker Desktop) must be running.

```bash
cd /path/to/precrime

# One-time: build the cross-compile image
make build-image

# Install GStreamer + systemd unit on Pi
make install-precog PRECOG_HOST=precog-01.local PRECOG_USER=pi

# Build binary and deploy
make deploy-precog PRECOG_HOST=precog-01.local PRECOG_USER=pi
```

> `install-precog` must run before `deploy-precog` — it installs the systemd unit.

### 4. Write config on Pi

```bash
ssh pi@precog-01.local 'sudo tee /etc/precog/precog.conf' << 'EOF'
source_name  = "PRECOG-01-CCTV"
device       = "/dev/video0"
format       = "YUY2"        # GStreamer name for YUYV — do not use "YUYV"
width        = 720
height       = 480
framerate    = "30/1"        # NTSC; use height=576 + framerate="25/1" for PAL

rtp_mcast    = "239.42.1.1"  # unique per unit — see multiple-unit table below
rtp_port     = 5000
bitrate_kbps = 1500
EOF
```

### 5. Restart and verify

```bash
make restart-precog PRECOG_HOST=precog-01.local PRECOG_USER=pi
make logs-precog    PRECOG_HOST=precog-01.local PRECOG_USER=pi
# Expect: "PRECOG starting" then pipeline running with no errors
```

---

## Verify stream on dev mac

```bash
gst-launch-1.0 \
  udpsrc address=239.42.1.1 port=5000 auto-multicast=true \
  caps="application/x-rtp,media=video,clock-rate=90000,encoding-name=H264,payload=96" ! \
  rtpjitterbuffer latency=200 ! rtph264depay ! avdec_h264 ! videoconvert ! \
  osxvideosink sync=false
```

> `sync=false` is required — the Pi's encoder clock doesn't align with the mac's system clock and frames will be dropped without it.

**Multicast note:** Consumer routers often drop multicast between clients (especially over WiFi). For local testing, use unicast directly to the Pi's IP:
```bash
# On Pi (replace 192.168.x.x with Mac IP):
gst-launch-1.0 v4l2src device=/dev/video0 ! "video/x-raw,format=YUY2,width=720,height=576,framerate=25/1" ! \
  deinterlace ! videoconvert ! x264enc tune=zerolatency speed-preset=ultrafast bitrate=1500 key-int-max=25 ! \
  video/x-h264,profile=baseline ! h264parse config-interval=1 ! rtph264pay pt=96 ! \
  udpsink host=192.168.x.x port=5100 sync=false async=false

# On Mac:
gst-launch-1.0 udpsrc port=5100 \
  caps="application/x-rtp,media=video,clock-rate=90000,encoding-name=H264,payload=96" ! \
  rtpjitterbuffer latency=200 ! rtph264depay ! avdec_h264 ! videoconvert ! osxvideosink sync=false
```
For the show (all devices on Ethernet to a dedicated switch), multicast works correctly.

**Verify TEMPLE ball:**
```bash
gst-launch-1.0 udpsrc address=239.42.0.1 port=9999 auto-multicast=true ! \
  fakesink dump=true 2>&1 | head -40
# Should see JSON: {"name":"PRECOG-01-CCTV","mcast":"239.42.1.1",...}
```

---

## Iterate

```bash
make deploy-precog  PRECOG_HOST=precog-01.local PRECOG_USER=pi   # build + rsync + restart
make logs-precog    PRECOG_HOST=precog-01.local PRECOG_USER=pi   # follow logs
make restart-precog PRECOG_HOST=precog-01.local PRECOG_USER=pi   # restart only
```

---

## Boot survival

`precog.service` uses `Restart=on-failure`. Crashes auto-restart within 2s.

```bash
ssh pi@precog-01.local 'sudo reboot'
# Wait 40s
make logs-precog PRECOG_HOST=precog-01.local PRECOG_USER=pi
# Confirm service active and streaming without manual intervention
```

---

## Show-day checklist

**Setup:**
- [ ] CCTV cam powered, video signal generating
- [ ] BNC → RCA adapter seated firmly
- [ ] Capture card plugged into Pi USB
- [ ] Pi powered (USB-C PD, ≥3A)
- [ ] Pi on same network as REPORT (Ethernet preferred)
- [ ] `sudo systemctl status precog.service` → active (running)
- [ ] Source visible in REPORT multiview within ~5s of Pi booting

**Tear-down:**
- [ ] `ssh pi@precog-01.local 'sudo shutdown -h now'` — wait for activity LED to stop
- [ ] Unplug capture card, disconnect CCTV cam, coil BNC cable
- [ ] Pack Pi + accessories

---

## Troubleshooting

**Service crashes with "could not link v4l2src0 to videoconvert0"**
- Format name wrong. Use `YUY2`, not `YUYV` — they are the same format but GStreamer uses its own name.

**Stream arrives but video drops (osxvideosink "too late" warnings)**
- Add `sync=false` to the osxvideosink command — see verify command above.

**Source not appearing in REPORT multiview**
1. Check service: `sudo systemctl status precog.service`
2. IGMP snooping on router must pass multicast — verify with the TEMPLE ball command above
3. Check REPORT logs: `make logs-report | grep PRECOG`

**Source in multiview but video is black**
- EasyCap signal test: `v4l2-ctl --stream-count=1` above
- Check CCTV cam power and BNC cable
- Try `input=1` in v4l2src if multiple capture card inputs

**Wrong frame rate / interlace artifacts**
- PAL: 25fps (`framerate = "25/1"`). NTSC: 30fps (`framerate = "30/1"`).
- Interlace comb artifacts are expected from composite CCTV — this is the aesthetic.

---

## SD card recovery (passwordless sudo)

If the Pi was flashed without `user-data.template` and sudo requires a password:

1. Shut down Pi, pull SD card, plug into mac.
2. Open `/Volumes/bootfs/user-data`. Add this section:
   ```yaml
   write_files:
     - path: /etc/sudoers.d/010_pi-nopasswd
       content: "pi ALL=(ALL) NOPASSWD:ALL\n"
       permissions: '0440'
       owner: root:root
   ```
3. In `/Volumes/bootfs/meta-data`, increment the `instance-id` by 1 (so cloud-init re-runs).
4. Do the same increment in `/Volumes/bootfs/cmdline.txt` (`i=rpi-imager-XXXXXXXXXX`).
5. Reinsert SD, boot Pi (~60s), then verify: `ssh pi@<ip> 'sudo hostname'` — no password prompt.

---

## Multiple PRECOG units

Each Pi needs a unique multicast group and source name:

| Unit | source_name | rtp_mcast |
|---|---|---|
| Kit A (CCTV door) | PRECOG-01-CCTV | 239.42.1.1 |
| Kit B (CCTV stage) | PRECOG-02-CCTV | 239.42.1.2 |
| Kit C (HDMI) | PRECOG-03-HDMI | 239.42.1.3 |
