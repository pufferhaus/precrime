# WITNESS — Operator Manual

iOS camera sender for PRECRIME. Streams H.264/RTP to REPORT over WiFi. Zero-config — finds REPORT automatically.

---

## First-time setup (sideload)

Do this once per phone, and again every 7 days.

1. Install Xcode on a mac (free, App Store)
2. Clone the repo: `git clone https://github.com/pblca/precrime`
3. `cd ios/Witness && make open`
4. In Xcode: select **Witness** target → **Signing & Capabilities** → **Team** → pick your Apple ID (free Personal Team is fine)
5. Plug iPhone in via cable. Unlock phone. Trust the mac if prompted.
6. Select your iPhone in the run-destination dropdown (top bar)
7. Press ⌘R. Xcode builds and installs (~60s first time, ~15s after).
8. On phone: **Settings → General → VPN & Device Management → [Your Apple ID] → Trust**
9. Open WITNESS. Grant camera permission.

**7-day cert:** The free sideload cert expires every 7 days. Re-run ⌘R from Xcode to refresh (phone stays connected, ~15s). Show-day strategy: redeploy the morning of the show.

---

## Main screen

```
┌──────────────────────────────────────────────────┐
│ ● LIVE  WITNESS-F7EB-CAM    REPORT-MAIN · 5003   │  ← status bar
├──────────────────────────────────────────────────┤
│                                                  │
│  [EV▼]       camera viewfinder           [Z▲]   │
│  [AWB]                  ┌──┐                     │  ← focus reticle on tap
│              ──── ──    │  │                     │
│              ──── ──    └──┘                     │
│                                                  │
├──────────────────────────────────────────────────┤
│      [↩︎ flip]     [☾ stage]     [⚙ settings]    │  ← bottom bar
└──────────────────────────────────────────────────┘
```

---

## Status bar states

| State | Color | Meaning |
|---|---|---|
| SEARCHING | Gray | Browsing network for REPORT via Bonjour |
| CONNECTING | Gray | TCP registration in progress |
| STREAMING | Orange | RTP sending, waiting for first ack from REPORT |
| LIVE | Green | RTP flowing, REPORT is receiving and acknowledging |
| LOST | Yellow | Acks stopped (>6s gap) — auto-reconnecting |

Connection is fully automatic. No manual IP entry needed. SEARCHING → LIVE typically takes 3–8s after both REPORT and WITNESS are running on the same network.

---

## Camera controls

### Zoom (right edge slider)
- Drag up to zoom in, down to zoom out
- Range: 1×–10× (hardware-limited; front camera may have less)
- Label at top shows current zoom level
- **1×** button resets to no zoom

### Exposure (left edge slider, yellow)
- Drag up to brighten, down to darken
- Range: device minimum to maximum EV
- Label shows current EV value (e.g. `+1.0`, `-0.5`)
- **0EV** button resets to neutral
- ☀ icon at top = maximum, ☀ at bottom = minimum

### White balance (AWB / WB ■ button, below exposure slider)
- **AWB** (white/gray): camera adjusts white balance automatically
- **WB ■** (yellow): white balance locked at current scene temperature
- Tap to toggle. Lock WB when the lighting is set and you want consistent colour across cameras.

### Tap-to-focus
- Tap anywhere in the centre of the viewfinder
- Corner-bracket reticle appears — pulses while focusing, holds still when locked
- Camera focuses at that point and locks
- Tap again to release back to continuous autofocus
- Focus resets to continuous when camera is flipped

### Camera flip (↩︎ button, bottom bar)
- Switches between back and front camera
- Stream restarts briefly (~0.3s drop) — avoid during active broadcast
- All camera controls reset after flip

---

## Stage mode

Use when the phone is deployed on a rig and you need to check stream health from a distance, or to prevent accidental touches.

**Enter:** tap **☾** in the bottom bar.

Screen dims to ~15% brightness. Camera preview disappears. A status dashboard appears:

```
WITNESS-F7EB-CAM

STATUS   LIVE ●
REPORT   REPORT-MAIN
PORT     5003
LOCAL    192.168.86.47

STREAM   640×480 / 30 fps
BITRATE  2000 kbps
ZOOM     1.0×
EV       +0.0

PACKETS  1,247,832
SENT     174.6 MB

         double-tap to exit stage mode
```

Stream continues running. Camera is still active — iOS requires the display to remain on for the camera to work. Stage mode keeps the display technically on at near-zero brightness.

**Exit:** double-tap anywhere on the screen.

**Tip:** Combine with iOS Guided Access (Settings → Accessibility → Guided Access) to lock the phone in stage mode for the full show — prevents accidental button presses and keeps the display from turning off.

---

## Settings sheet

Open via **⚙** in the bottom bar.

| Setting | Notes |
|---|---|
| Source name | Shown in REPORT logs and stage dashboard. Auto-generated from device ID. Change to something meaningful (e.g. `WITNESS-STAGE-LEFT`). |
| Resolution | 640×480 (default) or 1280×720. Requires stream restart. 720p needs ~4000 kbps bitrate. |
| Frame rate | 30 fps default. 60 fps available on newer iPhones. Requires restart. |
| Bitrate | 2000 kbps for 480p. Raise to 4000+ for 720p. Higher = better quality, more WiFi load. |
| Fallback host | Leave blank. Set only if REPORT has no Bonjour support (e.g. testing with `nc`). |
| Zoom / EV | Same as main screen sliders. |
| Stage mode | Same as ☾ button. |

**Apply:** close the sheet — settings take effect. Stream-affecting changes (resolution, fps, bitrate, camera side) restart the stream automatically.

---

## Troubleshooting

**SEARCHING for more than 30 seconds**
- Confirm REPORT is running and on the same WiFi network
- Run `dns-sd -B _precrime-report._tcp local.` on a mac on the same network — if REPORT-MAIN doesn't appear, REPORT's Bonjour publish failed
- Check REPORT logs: `make logs-report`
- Fallback: enter REPORT's IP manually in Settings → Fallback host

**LOST after previously LIVE**
- REPORT stopped sending acks — check if REPORT process died: `make logs-report`
- Brief network dropout: WITNESS auto-reconnects within ~15s
- If stuck on LOST: force-quit WITNESS and reopen

**STREAMING but not LIVE (acks not arriving)**
- REPORT received the registration but ack UDP packets aren't reaching the phone
- Check firewall on mac (if using mock_report.py): System Settings → Network → Firewall
- `nc -ul 9998` on mac to verify acks are being sent

**Video quality poor / blocky**
- Raise bitrate in Settings (try 4000 kbps)
- Check WiFi signal — phone should be on 5GHz band
- Reduce resolution to 640×480 if on congested network

**Stream freezes on REPORT end but WITNESS shows LIVE**
- RTP packets are arriving but pipeline stalled — restart REPORT: `make restart-report`
- Check for jitterbuffer underruns in REPORT logs

**Sideload cert expired (app won't open)**
- Open Xcode, plug in phone, press ⌘R — takes ~15s
- If Apple rejects bundle ID: change to a unique one in Signing settings

---

## Dev testing (without Pi)

Run the mock REPORT server on a mac on the same network:

```bash
cd /path/to/precrime
python3 ios/Witness/scripts/mock_report.py
```

WITNESS will discover it, register, and reach LIVE within ~5s. Mock shows packet rate every 5s.

---

## Quick reference card

```
CONTROLS          ACTION
──────────────    ──────────────────────────────
Right slider      Zoom (1×–10×)
Left slider       Exposure (-EV to +EV)
AWB/WB■ button    White balance auto / locked
Tap viewfinder    Focus lock (tap again to release)
↩︎ button          Flip camera (brief stream drop)
☾ button          Enter stage mode
⚙ button          Settings sheet
Double-tap        Exit stage mode

STATES            MEANING
──────────────    ──────────────────────────────
SEARCHING ·       Looking for REPORT on network
CONNECTING ·      Registering with REPORT
STREAMING ●       Sending RTP, no ack yet
LIVE ●            Fully connected, REPORT receiving
LOST ⚠            Ack timeout — auto-reconnecting
```
