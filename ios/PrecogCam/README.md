# PrecogCam — iOS H.264 RTP sender for PRECRIME REPORT

iPhone → H.264 over RTP/UDP → REPORT receiver on LAN. Side-loadable with a free Apple ID. No subscription, no third-party SDK.

Aligned with the post-NDI PRECRIME architecture (`main` branch): all video transport is H.264 over RTP (PT=96, 90 kHz clock, RFC 6184 single-NAL + FU-A). The Pi-based PRECOG instances broadcast on UDP multicast `239.42.x.x`; iPhone PRECOGs currently send **UDP unicast** because iOS 14+ requires Apple's `com.apple.developer.networking.multicast` entitlement to send to `239.x.x.x` (verified empirically — `sendto()` returns `EHOSTUNREACH` without it). The unicast path is identical packet-level; only the destination address differs. Swap-point is a one-line change in `RtpSender.swift` once the entitlement is approved.

## Pipeline

```
AVCaptureSession (NV12 @ 640×480 / 30fps)
    ↓ CVPixelBuffer + CMTime
H264Encoder (VideoToolbox, real-time, no B-frames, 1s IDR, baseline)
    ↓ [NALU…] + pts + isKeyframe
RtpPacketizer (RFC 6184; single-NAL or FU-A; 90 kHz timestamps; marker on last pkt)
    ↓ [RTP packets]
RtpSender (UDP unicast, bound to en0, configurable target host:port)
```

Glass-to-glass target on LAN: **~50–80 ms** (capture 33ms + VT encode ~10ms + RTP+net ~5ms + receiver jitterbuffer/decode ~25ms).

## Build

```bash
brew install xcodegen        # one-time
cd ios/PrecogCam
make open                    # generates project, opens in Xcode
```

In Xcode:
1. Select **PrecogCam** target → **Signing & Capabilities**
2. **Team**: pick your free Personal Team
3. **Bundle Identifier**: if Apple rejects the default, change to something unique to you (e.g. `art.precrime.yourname.PrecogCam`)
4. Plug iPhone in (cabled, unlocked, trusted)
5. Choose iPhone in run-destination dropdown → ⌘R

First launch on phone: **Settings → General → VPN & Device Management → [Your Apple ID] → Trust**, then re-run.

## Free Apple ID sideload cert

- **7-day lifetime** before iOS refuses to launch the app. Re-deploy via Xcode (~30 s) to refresh.
- Limits: 3 active sideloaded apps per Apple ID, 10 device registrations / week.
- Show-day strategy: re-deploy day-of-show — full 7-day buffer. Or pay $99/yr Apple Developer for 1-year certs.

## In-app settings

| Setting | Default | Purpose |
|---|---|---|
| Source name | `PRECOG-{4-char hash}-CAM` | Identity for logs. Becomes RTP SSRC (FNV hash). |
| Target host | `192.168.86.21` | REPORT receiver IP. **Change per LAN.** |
| Target port | `5000` | Per-source RTP port. Match REPORT config. |
| Resolution | 640×480 | Or 1280×720 |
| Frame rate | 30 fps | Toggle 60 fps |
| Bitrate | 2000 kbps | 480p baseline. Raise to 4000 for 720p. |

## REPORT-side receive (gst-launch test)

```bash
gst-launch-1.0 \
  udpsrc port=5000 caps="application/x-rtp,media=video,clock-rate=90000,encoding-name=H264,payload=96" ! \
  rtpjitterbuffer latency=80 ! rtph264depay ! avdec_h264 ! videoconvert ! osxvideosink
```

Replace `osxvideosink` with `autovideosink` on Linux. The Rust REPORT daemon's `udpsrc`-driven RTP pipeline is the equivalent.

## Multicast (future)

When the multicast entitlement lands:

1. Add `com.apple.developer.networking.multicast` to the app's entitlements file (via Xcode → target → Signing & Capabilities → +Capability → Multicast Networking)
2. In `RtpSender.swift`, add `IP_MULTICAST_IF` setsockopt with the WiFi interface IPv4 and `IP_MULTICAST_TTL=1` (template already in `MulticastProbe.swift` history)
3. Target `239.42.x.x` in settings instead of the unicast IP
4. Add temple ball broadcaster (sibling crate, JSON over UDP multicast to `239.42.0.1:9999`)

## File layout

```
ios/PrecogCam/
  project.yml                  — xcodegen spec
  Makefile                     — project generate / open
  PrecogCam/
    PrecogCamApp.swift         — @main entry
    Capture/CaptureSession.swift — AVCapture NV12 → (CVPixelBuffer, CMTime)
    Encoder/H264Encoder.swift  — VTCompressionSession → [NALU]
    RTP/RtpPacketizer.swift    — RFC 6184 single-NAL + FU-A
    RTP/RtpSender.swift        — UDP unicast, bound to en0
    UI/ContentView.swift       — viewfinder + status pill
    UI/SettingsView.swift      — name, host, port, bitrate, camera
    UI/CameraPreview.swift     — AVCaptureVideoPreviewLayer bridge
    Util/AppModel.swift        — lifecycle + capture→encode→send wiring
    Util/AppSettings.swift     — UserDefaults-backed prefs
```
