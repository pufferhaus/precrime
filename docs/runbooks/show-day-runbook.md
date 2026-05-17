# PRECRIME — Show-Day Runbook

Full system bring-up, verification, and tear-down for a live performance.

**System:** Router → REPORT (Pi 5) → HDMI program + multiview. Camera sources: WITNESS (iPhones) + Pi PRECOGs (CCTV/analog).

---

## Timeline

```
T-60 min   Power on router, REPORT Pi, PRECOG Pis
T-45 min   Verify network, REPORT logs, multiview display
T-30 min   Deploy/verify each camera source, check WITNESS LIVE state
T-15 min   Final camera framing, white balance lock, focus lock
T-5 min    WITNESS phones → stage mode
T-0        Show starts
```

---

## Pre-show bring-up sequence

Order matters: router first, then REPORT, then cameras.

### 1. Router

- Power on Flint 3 (GL-BE9300)
- Wait ~60s for boot
- Confirm: 5GHz SSID is broadcasting
- Confirm: IGMP snooping is enabled (GL-iNet admin → Network → IGMP Snooping → ON)
  - Without this, multicast Pi PRECOG streams won't route correctly

### 2. REPORT Pi

- Power on, wait ~30s for boot
- Verify service is running:
  ```bash
  make logs-report REPORT_HOST=report.local
  ```
  Look for: `INFO report::bonjour: published REPORT-MAIN._precrime-report._tcp`
- Confirm multiview display shows a grid (empty slots = correct, not an error)
- Confirm program display is black (no source selected yet = correct)

### 3. Pi PRECOGs (if present)

For each CCTV/EasyCap unit:
- Power on, wait ~30s for boot
- Verify service: `make logs-precog PRECOG_HOST=precog-01.local`
- Look for: ball tx thread sending, RTP multicast emitting
- In REPORT multiview: source should appear within ~5s of PRECOG booting
- Confirm video is live (not black) — check CCTV cam is powered and cabled

### 4. WITNESS iPhones

For each phone:
- Open WITNESS app
- Watch status bar: SEARCHING → CONNECTING → STREAMING → **LIVE ●**
  - Should reach LIVE within 5–8s on a clean network
- In REPORT multiview: WITNESS source appears alongside Pi sources
- If LIVE not reached in 30s: see Troubleshooting section

### 5. Source verification

With all sources up, verify in REPORT multiview:
- [ ] All expected sources visible as slots in multiview
- [ ] Each slot shows live video (not black or frozen)
- [ ] Test keyboard switching: press `1`, `2`, etc. — program HDMI cuts correctly
- [ ] Switch latency feels instant (should be ≤200ms)

---

## Camera setup (T-30 to T-15)

Do this with lighting and staging in final position.

**For each WITNESS:**

1. Frame the shot — adjust phone position/angle
2. Tap-to-focus on the key subject — reticle appears, then locks
3. Adjust exposure slider if scene is over/underexposed
4. Lock white balance: tap **AWB** → **WB ■** (yellow)
   - Lock WB on all cameras at the same time, under final stage lighting
   - Consistent WB = consistent look across cuts
5. Adjust zoom if needed
6. Verify in REPORT multiview that the frame looks correct

**For Pi PRECOGs:**

- Frame adjustment is physical (move cam/cable)
- Video signal quality is fixed (EasyCap + CCTV)

---

## Pre-show final checks (T-5)

- [ ] All sources showing **LIVE ●** on their WITNESS phones (or confirmed active in multiview for Pi sources)
- [ ] Program HDMI showing correct source for show open
- [ ] Keyboard switching tested one final time
- [ ] Operator knows the slot numbers for each camera
- [ ] WITNESS phones → stage mode (☾ button) to prevent accidental touches
- [ ] Phones power connected or battery checked (stage mode drains less, but camera is still on)

---

## During show

**Switching:** Number keys `1`–`9` on the USB keyboard. No other interaction needed.

**If a source drops mid-show:**
- Its multiview slot goes blank
- Other sources are unaffected
- For WITNESS: LOST state auto-reconnects within ~15s; slot reappears
- For Pi PRECOG: systemd restarts the service; reconnects within ~30s
- Don't switch to a dropped source — multiview slot will be blank/black

**If REPORT crashes:**
- systemd restarts it automatically (~2s)
- All sources reconnect automatically (TEMPLE balls retry, WITNESS retries registration)
- Expect ~10–20s blackout on program output while pipeline rebuilds
- If REPORT doesn't come back: `make restart-report REPORT_HOST=report.local` from operator laptop

**Network dropout:**
- WITNESS: STREAMING → LOST → auto-reconnects on network restore
- Pi PRECOG: TEMPLE ball stops → source evicted from REPORT → reappears when network restores
- REPORT pipeline adapts automatically (source set changes trigger rebuild)

---

## Tear-down sequence

1. **WITNESS phones:** exit stage mode (double-tap), close WITNESS app
2. **REPORT:** `sudo systemctl stop report.service` (or `make stop-report`)
3. **Pi PRECOGs:** `ssh user@precog-01.local 'sudo shutdown -h now'` — wait for activity LED to go dark
4. **REPORT Pi:** `ssh user@report.local 'sudo shutdown -h now'` — wait for activity LED to go dark
5. **Router:** power off last
6. Disconnect all HDMI, keyboard, power cables
7. Coil and bag all cables with their source units

---

## Troubleshooting quick reference

| Symptom | First check | Fix |
|---|---|---|
| WITNESS stuck SEARCHING | REPORT running? Bonjour visible on LAN? | `make logs-report`; restart if needed |
| WITNESS stuck STREAMING (no ack) | Port 9998 blocked? | Check mac firewall; `nc -ul 9998` to test |
| Source in multiview but black video | RTP packets arriving? | `tcpdump -n udp port 5000`; check PRECOG/WITNESS side |
| No multiview display at all | Connector IDs wrong? | `modetest | grep connected`; update report.conf |
| Keyboard not switching | Wrong event device? | `ls /dev/input/by-id/`; update keyboard_device |
| REPORT pipeline crash loop | Caps mismatch? | `make logs-report`; check encoding params match |
| High latency / lag | jitterbuffer too low? | Raise `latency` in pipeline.rs and redeploy |
| Pi PRECOG not in multiview | IGMP snooping off? | Enable in router admin; or check PRECOG logs |

---

## Emergency: REPORT full restart

If REPORT is unrecoverable mid-show:

```bash
# From operator laptop on the network:
ssh user@report.local
sudo systemctl restart report.service
# Watch logs:
sudo journalctl -u report.service -f
```

Sources reconnect automatically within ~20s. Expect brief program blackout.

---

## Network requirements

| Requirement | Value | Why |
|---|---|---|
| WiFi band | 5GHz preferred | Less congestion; lower latency |
| IGMP snooping | Enabled on router | Required for Pi PRECOG multicast routing |
| Ports open (LAN) | UDP 9998, TCP 4999, UDP 5000–5099, UDP 9999, UDP 239.42.x.x | Discovery + RTP |
| iPhone WiFi | Same SSID as REPORT Pi | Bonjour is link-local; must be same L2 segment |

---

## Contact / escalation

If something is broken that this runbook doesn't cover: check `docs/superpowers/specs/` for the relevant design doc, or `git log --oneline` for recent changes.
