# PRECOG Kit A — CCTV Encoder Runbook

Pi 5 + EasyCap UTV007 + vintage CCTV cam. Captures analog composite video, encodes H.264, sends RTP multicast to REPORT. Announces itself via TEMPLE JSON ball.

## Identity

- Hostname: `precog-01-cctv-door` (set during Pi OS setup)
- Source name: `PRECOG-01-CCTV-DOOR` (set in `/etc/precog/precog.conf`)
- Hardware: Pi 5 4GB + active cooler + EasyCap UTV007 + vintage CCTV cam

---

## Hardware setup

**Connections:**
1. CCTV cam → BNC cable → BNC-to-RCA adapter → EasyCap yellow (video) jack
2. EasyCap USB-A → Pi 5 USB port (any; USB 2 is fine for composite)
3. Pi 5 → USB-C PD power supply (≥3A; 5A recommended with active cooler)
4. Pi 5 → Ethernet or WiFi to same network as REPORT

**Verify EasyCap is detected (once Pi is booted):**
```bash
v4l2-ctl --list-devices
# Expect: "EasyCAP" or similar on /dev/video0
v4l2-ctl -d /dev/video0 --list-formats-ext
# Expect: YUYV or similar, 640x480
```

**Verify video signal:**
```bash
v4l2-ctl -d /dev/video0 --stream-mmap=3 --stream-count=1 --stream-to=/tmp/frame.raw
ls -lh /tmp/frame.raw   # should be ~600KB for a 640x480 YUYV frame
```
If size is near zero: check CCTV cam power, BNC cable, and adapter seating.

---

## Software setup (first time)

From dev mac (repo root):

```bash
make install-precog PRECOG_HOST=precog-01-cctv-door.local
```

This runs `precog/install.sh` on the Pi: installs GStreamer, Rust toolchain, creates `/etc/precog/`, installs `precog.service` systemd unit.

**Edit config on Pi:**
```bash
ssh user@precog-01-cctv-door.local
sudo nano /etc/precog/precog.conf
```

```toml
source_name   = "PRECOG-01-CCTV-DOOR"
device        = "/dev/video0"
input_format  = "YUYV"
width         = 640
height        = 480
fps           = 25          # PAL composite; use 30 for NTSC

# RTP multicast endpoint for this source
rtp_mcast     = "239.42.1.1"   # unique per source — change for each PRECOG
rtp_port      = 5000
rtp_ttl       = 1

# TEMPLE discovery
temple_group  = "239.42.0.1"
temple_port   = 9999

# Encoding
bitrate_bps   = 2000000     # 2 Mbps
```

**Deploy and start:**
```bash
make deploy-precog PRECOG_HOST=precog-01-cctv-door.local
```

---

## Verify stream is live

**Check service:**
```bash
make logs-precog PRECOG_HOST=precog-01-cctv-door.local
```

Expected in logs:
```
INFO precog: starting PRECOG-01-CCTV-DOOR
INFO precog: v4l2src device=/dev/video0 → H264 → RTP → 239.42.1.1:5000
INFO precog::temple: ball tx started (group=239.42.0.1:9999, interval=2s)
```

**Verify multicast RTP on dev mac:**
```bash
gst-launch-1.0 \
  udpsrc address=239.42.1.1 port=5000 auto-multicast=true \
  caps="application/x-rtp,media=video,clock-rate=90000,encoding-name=H264,payload=96" ! \
  rtpjitterbuffer latency=80 ! rtph264depay ! avdec_h264 ! videoconvert ! osxvideosink
```

**Verify TEMPLE ball on dev mac:**
```bash
gst-launch-1.0 udpsrc address=239.42.0.1 port=9999 auto-multicast=true ! \
  fakesink dump=true 2>&1 | head -40
# Should see JSON: {"name":"PRECOG-01-CCTV-DOOR","mcast":"239.42.1.1",...}
```

---

## Iterate

```bash
make deploy-precog PRECOG_HOST=precog-01-cctv-door.local   # rsync + build + restart
make logs-precog   PRECOG_HOST=precog-01-cctv-door.local   # follow logs
make restart-precog PRECOG_HOST=precog-01-cctv-door.local  # restart only
```

---

## Boot survival

`precog.service` uses `Restart=on-failure`. If the process crashes, systemd restarts within 2s.

**Verify boot survival:**
```bash
ssh user@precog-01-cctv-door.local 'sudo reboot'
# Wait 40s
make logs-precog PRECOG_HOST=precog-01-cctv-door.local
# Confirm service active and streaming without manual intervention
```

---

## Show-day checklist

**Setup:**
- [ ] CCTV cam powered, video signal generating
- [ ] BNC → RCA adapter seated firmly
- [ ] EasyCap plugged into Pi USB
- [ ] Pi powered (USB-C PD, ≥3A)
- [ ] Pi on same network as REPORT
- [ ] `sudo systemctl status precog.service` → active (running)
- [ ] Source appears in REPORT multiview within ~5s of Pi booting

**Tear-down:**
- [ ] `ssh user@precog-01-cctv-door.local 'sudo shutdown -h now'` — wait for activity LED to stop
- [ ] Unplug EasyCap, disconnect CCTV cam, coil BNC cable
- [ ] Pack Pi + accessories

---

## Troubleshooting

**Source not appearing in REPORT multiview**
1. Check service: `ssh user@precog-01-cctv-door.local 'sudo systemctl status precog.service'`
2. IGMP snooping on router must be enabled for multicast routing
3. Verify TEMPLE ball: listen on `239.42.0.1:9999` from mac (see above)
4. Check REPORT logs: `make logs-report | grep PRECOG-01`

**Source in multiview but video is black**
- EasyCap signal test: `v4l2-ctl --stream-count=1 ...` above
- Try `input=1` in v4l2-ctl if multiple EasyCap inputs
- CCTV cam power or BNC cable issue

**Harsh interlace / comb artefacts**
- Expected from composite CCTV — this is the aesthetic
- Add deinterlace filter in precog config if needed

**Wrong frame rate (PAL vs NTSC)**
- PAL: 25fps. NTSC: 29.97fps. Set `fps` in precog.conf to match camera output.

---

## Multiple PRECOG units

Each Pi needs a unique multicast group and source name:

| Unit | source_name | rtp_mcast |
|---|---|---|
| Kit A (CCTV door) | PRECOG-01-CCTV-DOOR | 239.42.1.1 |
| Kit B (CCTV stage) | PRECOG-02-CCTV-STAGE | 239.42.1.2 |
| Kit C (HDMI) | PRECOG-03-HDMI-WIDE | 239.42.1.3 |
