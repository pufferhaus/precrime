# WITNESS — iOS camera sender for PRECRIME

iPhone → H.264/RTP/UDP → REPORT receiver on LAN. Zero-config: the app discovers REPORT automatically via Bonjour and registers itself. Side-loadable with a free Apple ID. No subscription, no third-party SDK.

Aligned with the post-NDI PRECRIME architecture: all video transport is H.264 over RTP (PT=96, 90 kHz clock, RFC 6184 single-NAL + FU-A). Pi-based PRECOG instances send multicast; WITNESS sends **UDP unicast** because iOS requires Apple's `com.apple.developer.networking.multicast` entitlement for multicast sends (verified — `sendto()` returns `EHOSTUNREACH` without it). The unicast path is wire-identical; swap is a one-line change in `RtpSender.swift` once the entitlement is approved.

## Discovery + connection

WITNESS requires no manual IP configuration. At startup:

1. Publishes itself as `_precog._tcp` on Bonjour (visible via `dns-sd -B _precog._tcp local.` from any mac on the LAN)
2. Browses for `_precrime-report._tcp` — REPORT publishes this when it starts
3. Opens a TCP connection to REPORT's registration port (default 4999), sends source name + stream params, receives an assigned RTP port
4. Starts sending RTP to REPORT's IP on the assigned port
5. Listens for UDP acks from REPORT (port 9998) — shows **LIVE ●** when acks flow, **LOST** if they stop

Status pill at top of screen reflects the current state: `SEARCHING → CONNECTING → STREAMING → LIVE ●`.

If no REPORT is found via Bonjour within 15s and a manual fallback IP is set in Settings, WITNESS falls back to that address.

## Pipeline

```
AVCaptureSession (NV12 @ configured resolution/fps)
    ↓ CVPixelBuffer + CMTime
H264Encoder (VideoToolbox, real-time, no B-frames, 1s IDR, baseline)
    ↓ [NALU…] + pts + isKeyframe
RtpPacketizer (RFC 6184; single-NAL or FU-A; 90 kHz timestamps; marker on last pkt)
    ↓ [RTP packets]
RtpSender (UDP unicast, bound to en0, no-op until REPORT assigns a port)
```

Glass-to-glass target on LAN: **~50–80 ms** (capture 33ms + VT encode ~10ms + RTP+net ~5ms + receiver jitterbuffer/decode ~25ms).

## Build

```bash
brew install xcodegen        # one-time
cd ios/Witness
make open                    # generates Witness.xcodeproj, opens in Xcode
```

In Xcode:
1. Select **Witness** target → **Signing & Capabilities**
2. **Team**: pick your free Personal Team
3. **Bundle Identifier**: change to something unique if Apple rejects default (e.g. `art.precrime.yourname.witness`)
4. Plug iPhone in (cabled, unlocked, trusted)
5. Choose iPhone in run-destination dropdown → ⌘R

First launch on phone: **Settings → General → VPN & Device Management → [Your Apple ID] → Trust**, then re-run.

## Free Apple ID sideload cert

- **7-day lifetime** before iOS refuses to launch the app. Re-deploy via Xcode (~30s) to refresh.
- Limits: 3 active sideloaded apps per Apple ID, 10 device registrations/week.
- Show-day strategy: re-deploy day-of-show — full 7-day buffer. Or pay $99/yr Apple Developer for 1-year certs.

## Camera controls

All controls on the main screen — no settings sheet needed during a show.

| Control | Location | What it does |
|---|---|---|
| Zoom slider | Right edge (vertical) | 1×–10× optical zoom. Label + 1× reset button. |
| Exposure slider | Left edge (vertical) | EV bias. Yellow tint. 0EV reset button. |
| AWB / WB ■ | Below exposure slider | Toggle auto white balance vs locked. Yellow = locked. |
| Tap viewfinder | Anywhere in centre | Tap-to-focus: AF then lock. Corner-bracket reticle appears. Tap again to release. |
| ↩︎ (flip) | Bottom bar | Toggle front/back camera. Brief stream restart (~0.3s). |
| ☾ (stage) | Bottom bar | Enter stage mode. |
| ⚙ (settings) | Bottom bar | Open settings sheet. |

## Stage mode

Stage mode hides the camera preview and replaces it with a connection status dashboard — useful when the phone is deployed on a rig and you need to check stream health from across the room.

**Enter:** tap ☾ in the bottom bar. Screen dims to ~15% brightness.

**Dashboard shows:**
- Source name, REPORT name + port
- Connection state (LIVE / LOST / SEARCHING)
- Local WiFi IP
- Stream params (resolution, fps, bitrate, zoom, EV)
- Live packet + byte counters (updated every second)

**Exit:** double-tap anywhere on the screen.

iOS prohibits background camera access — the screen must remain on. Stage mode is the workaround: the display is technically on but dim, keeping the camera pipeline alive.

## Settings sheet

| Setting | Default | Notes |
|---|---|---|
| Source name | `WITNESS-{4-char}-CAM` | Shown in REPORT logs + stage dashboard. |
| Resolution | 640×480 | Or 1280×720. Requires stream restart. |
| Frame rate | 30 fps | 60 fps available. Requires restart. |
| Bitrate | 2000 kbps | Raise to 4000+ for 720p. |
| Fallback host | (empty) | Manual IP if Bonjour unavailable. |
| Zoom | 1.0× | Also on main screen. |
| Exposure bias | 0.0 EV | Also on main screen. |
| Stage mode | off | Also on main screen. |

Target host and port are no longer manually configured — REPORT assigns them dynamically. The fallback host field is for testing without a Bonjour-capable REPORT (e.g. using netcat).

## Dev testing without a Pi

Use the included mock report server. Implements full Bonjour + registration + ack protocol:

```bash
python3 ios/Witness/scripts/mock_report.py
```

Output when WITNESS connects:
```
[bonjour] published REPORT-MAIN._precrime-report._tcp port 4999
[REPORT] REPORT-MAIN online
[reg] registered: 'WITNESS-F7EB-CAM' @ 192.168.86.47 → port 5000  (640x480 30fps 2000kbps)
[rtp] listening on :5000 for 'WITNESS-F7EB-CAM'
[rtp] 'WITNESS-F7EB-CAM' port 5000: 30.1 pkt/s ≈ 336 kbps
```

WITNESS status pill transitions to **LIVE ●** within ~4s of mock_report starting.

Options: `--name REPORT-STAGE --reg-port 4999`

## REPORT-side receive (gst-launch test, without mock)

```bash
gst-launch-1.0 \
  udpsrc port=5000 caps="application/x-rtp,media=video,clock-rate=90000,encoding-name=H264,payload=96" ! \
  rtpjitterbuffer latency=80 ! rtph264depay ! avdec_h264 ! videoconvert ! osxvideosink
```

Replace `osxvideosink` with `autovideosink` on Linux.

## Multicast (future)

When the multicast entitlement is approved:

1. Add `com.apple.developer.networking.multicast` entitlement in Xcode
2. In `RtpSender.swift`: add `IP_MULTICAST_IF` + `IP_MULTICAST_TTL=1` setsockopt, target `239.42.x.x`
3. Add temple ball broadcaster thread (JSON ball → `239.42.0.1:9999` every 2s)

## File layout

```
ios/Witness/
  project.yml                  — xcodegen spec (target: Witness, bundle: art.precrime.witness)
  Makefile                     — make open / make clean
  scripts/
    mock_report.py             — dev REPORT mock (Bonjour + registration + ack + RTP listener)
  Witness/
    WitnessApp.swift           — @main entry
    Capture/
      CaptureSession.swift     — AVCapture NV12, zoom/EV/focus/WB controls
    Encoder/
      H264Encoder.swift        — VTCompressionSession → [NALU]
    RTP/
      RtpPacketizer.swift      — RFC 6184 single-NAL + FU-A
      RtpSender.swift          — UDP unicast, bound to en0, targeted flag
    Network/
      BonjourPublisher.swift   — publishes _precog._tcp
      ReportDiscovery.swift    — browses _precrime-report._tcp
      RegistrationClient.swift — TCP JSON registration, keep-alive every 15s
      AckReceiver.swift        — UDP :9998, LIVE/LOST state transitions
    UI/
      ContentView.swift        — viewfinder, status bar, sliders, buttons, focus reticle
      SettingsView.swift       — settings sheet
      StageStatusView.swift    — stage mode dashboard
      CameraPreview.swift      — AVCaptureVideoPreviewLayer + tap gesture
    Util/
      AppModel.swift           — ConnectionState machine, camera controls, lifecycle
      AppSettings.swift        — UserDefaults-backed prefs
```
