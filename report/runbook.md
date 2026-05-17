# REPORT — Operator Runbook

Pi 5 switcher daemon. Receives H.264/RTP from all camera sources, presents program output on HDMI-A-1 and multiview grid on HDMI-A-2. Discovers Pi PRECOGs via TEMPLE multicast balls; discovers WITNESS iPhones via Bonjour + TCP registration.

---

## Hardware

- Raspberry Pi 5 8GB + active cooler
- Two HDMI cables: HDMI-A-1 → program display, HDMI-A-2 → operator multiview
- USB keyboard (evdev input — `/dev/input/event0` by default)
- Power: USB-C PD, ≥5A supply recommended

---

## Config file

`/etc/precrime/report.conf` (TOML):

```toml
# Display outputs — find connector IDs with: modetest | grep connected
program_connector_id  = 32
preview_connector_id  = 34

# Keyboard device
keyboard_device = "/dev/input/event0"

# WITNESS (iPhone) discovery
report_name   = "REPORT-MAIN"   # shown in WITNESS status bar
reg_port      = 4999            # TCP registration port
rtp_port_min  = 5000            # RTP port pool start
rtp_port_max  = 5099            # RTP port pool end
ack_port      = 9998            # UDP ack port (sources listen here)

# TEMPLE (Pi PRECOG) discovery
temple_group  = "239.42.0.1"
temple_port   = 9999

# Optional: pin a source to a specific slot (1-indexed)
[source_slot_overrides]
# "PRECOG-01-CCTV-DOOR" = 1
# "WITNESS-F7EB-CAM"    = 2
```

**Finding connector IDs:**
```bash
modetest | grep -A2 "connected"
# Look for "HDMI-A-1" and "HDMI-A-2" — note the ID number on the left
```

**Finding keyboard device:**
```bash
ls /dev/input/by-id/       # look for USB keyboard
# or: evtest (shows which /dev/input/eventN responds to keypresses)
```

---

## Deploy + start

From the dev mac (repo root):

```bash
# First-time install on a fresh Pi:
make install-report REPORT_HOST=report.local

# Deploy (rsync sources + build + restart service):
make deploy-report REPORT_HOST=report.local

# Restart without rebuild:
make restart-report REPORT_HOST=report.local

# Follow live logs:
make logs-report REPORT_HOST=report.local
```

Or directly on the Pi:

```bash
sudo systemctl start report.service
sudo systemctl stop report.service
sudo systemctl restart report.service
sudo journalctl -u report.service -f
```

REPORT starts within ~3s of the service starting. GStreamer pipeline initialises, then sources appear as camera feeds discover it.

---

## Keyboard switching

The USB keyboard controls which camera source is on program output.

| Key | Action |
|---|---|
| `1` | Switch to slot 1 (first discovered source) |
| `2` | Switch to slot 2 |
| … | … |
| `0` | Switch to slot 10 |

Slots are assigned in discovery order, or pinned via `source_slot_overrides` in config. The multiview preview always shows all active sources with a tally indicator on the currently selected program.

Switch latency: ≤200ms (measured from keypress to HDMI output change).

---

## What to expect in logs

**Normal startup:**
```
INFO report::daemon: starting
INFO report::bonjour: published REPORT-MAIN._precrime-report._tcp port 4999
INFO report::daemon: temple receiver started
INFO report::registration: TCP registration server listening on :4999
INFO report::ack: ack sender started
```

**WITNESS connecting:**
```
INFO report::registration: registered: 'WITNESS-F7EB-CAM' @ 192.168.86.47 → port 5003
INFO report::daemon: sources changed [WITNESS-F7EB-CAM]
INFO report::pipeline: rebuilding program pipeline (1 source)
```

**Pi PRECOG appearing:**
```
INFO report::daemon: sources changed [PRECOG-01-CCTV-DOOR, WITNESS-F7EB-CAM]
INFO report::pipeline: rebuilding program pipeline (2 sources)
```

**Source lost:**
```
WARN report::daemon: WITNESS-F7EB-CAM evicted (idle 30s)
INFO report::daemon: sources changed [PRECOG-01-CCTV-DOOR]
```

---

## Monitoring

While running, check source health:

```bash
# Live logs
sudo journalctl -u report.service -f

# Check if service is up
sudo systemctl status report.service

# Check which sources are active (grep recent logs)
sudo journalctl -u report.service --since "5 minutes ago" | grep "sources changed"

# UDP ack traffic (confirms REPORT is acking sources)
sudo tcpdump -n udp port 9998

# RTP traffic on assigned port
sudo tcpdump -n udp port 5000
```

---

## Troubleshooting

**No video on program HDMI**
- Check `connector_id` values — `modetest | grep connected` and re-verify
- Confirm at least one source is registered: look for `sources changed` in logs
- If pipeline error in logs: service auto-restarts on failure (systemd `Restart=on-failure`)

**Source appears but video is black/frozen**
- Check RTP traffic: `sudo tcpdump -n udp port 5000` — confirm packets arriving
- Jitterbuffer underrun: look for `rtpjitterbuffer` warnings in logs; increase `latency` in pipeline if on lossy WiFi
- PRECOG may have crashed — check its logs: `make logs-precog`

**WITNESS not connecting / SEARCHING forever**
- Confirm `avahi-daemon` is running: `sudo systemctl status avahi-daemon`
- Test Bonjour publish from another machine: `dns-sd -B _precrime-report._tcp local.`
- Check port 4999 is not firewalled: `sudo ufw status`

**WITNESS connects but no video in pipeline**
- Registration assigned a port; confirm udpsrc opened on that port: grep logs for `rebuilding program pipeline`
- WITNESS may be sending RTP to wrong IP — check WITNESS stage mode for assigned port vs what report logs show

**Keyboard not working**
- Verify device path: `ls /dev/input/by-id/` — USB keyboard may be on different eventN
- Check permissions: `sudo evtest /dev/input/event0`
- Update `keyboard_device` in config, redeploy

**Pipeline crashes on source add/remove**
- GStreamer element error in logs — usually a caps mismatch
- WITNESS bitrate or resolution changed mid-session — stream restart will trigger pipeline rebuild
- `make restart-report` to clear and rebuild

**High latency / audio-video sync**
- Default `rtpjitterbuffer latency=20` (ms) — raise to 50–80 on congested network
- Change in `pipeline.rs` and redeploy

---

## Show-day checklist

**Before gates open:**
- [ ] REPORT Pi connected to both HDMI displays
- [ ] USB keyboard plugged in
- [ ] Pi on same WiFi network as all sources
- [ ] `sudo systemctl status report.service` — shows `active (running)`
- [ ] Multiview display shows grid (even if no sources yet — blank grid is correct)
- [ ] Each camera source appears in multiview as it comes online
- [ ] Test keyboard switching between slots — confirm program HDMI cuts correctly
- [ ] WITNESS phones show **LIVE ●** in status bar

**During show:**
- Number keys switch program source
- If a source drops: multiview goes blank for that slot; other slots unaffected
- If REPORT crashes: systemd restarts it within ~2s; sources reconnect automatically

**Tear-down:**
- [ ] `make stop-report` or `sudo systemctl stop report.service`
- [ ] Shut down Pi: `ssh user@report.local 'sudo shutdown -h now'`, wait for activity LED to stop
- [ ] Disconnect HDMI, keyboard, power
