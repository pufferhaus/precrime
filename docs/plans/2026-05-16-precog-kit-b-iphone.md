# PRECOG Kit B (iPhone NDI Source) Implementation Plan

**Goal:** Configure an iPhone 15 as `PRECOG-01-IPHONE-STAGE`, an NDI|HX2 source discoverable on the PRECRIME LAN via the NDI HX Camera app, validated end-to-end from a laptop running NDI Studio Monitor.

**Architecture:** The iPhone is the camera, encoder, and NDI publisher all in one. The free NDI HX Camera app (NewTek) broadcasts the rear camera as an NDI source using the device's iOS hostname. Wiring, power, and remote control are addressed at the kit level so the iPhone runs unattended at a show.

**Tech Stack:** iOS 17+, NDI HX Camera (App Store), iOS Settings, optionally Shortcuts for power-on automation.

**Milestone covered:** M2 from system spec.

**Depends on:** Network Brain plan must be complete (Task 8 / M1 verified).

---

### Task 1: Install the app and set device name

**Files:**
- Create: `precog/kit-b-iphone-runbook.md`

- [ ] **Step 1: On the iPhone, install NDI HX Camera from the App Store**

  Search "NDI HX Camera" by NewTek. Free. Install.

- [ ] **Step 2: Rename the iPhone to the PRECRIME convention**

  Settings → General → About → Name → set to:
  ```
  PRECOG-01-IPHONE-STAGE
  ```
  Note: NDI HX Camera broadcasts using this device name. The naming convention is what makes auto-discovery deterministic on the operator side.

- [ ] **Step 3: Confirm the new name shows up in the NDI HX Camera app**

  Open NDI HX Camera. In the app's title bar or settings panel, the source name should now read `PRECOG-01-IPHONE-STAGE`.

- [ ] **Step 4: Create the runbook file**

  Create `precog/kit-b-iphone-runbook.md`:

  ```markdown
  # PRECOG Kit B — iPhone Setup Runbook

  ## Identity
  - Device name: `PRECOG-01-IPHONE-STAGE`
  - Hardware: iPhone 15 (owned)
  - Software: NDI HX Camera (NewTek, App Store, free)

  ## Setup history
  - 2026-05-16: Initial deployment per precog-kit-b-iphone plan
  ```

- [ ] **Step 5: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add precog/kit-b-iphone-runbook.md
  git commit -m "precog kit-b: initial iPhone runbook"
  ```

---

### Task 2: Join the PRECRIME LAN

- [ ] **Step 1: On the iPhone, join the `precrime-lan` SSID**

  Settings → Wi-Fi → `precrime-lan` → enter the password from your password manager.

- [ ] **Step 2: Verify the iPhone got a `192.168.50.x` address**

  Settings → Wi-Fi → tap the (i) info icon next to `precrime-lan`. Expected: IP address in `192.168.50.100`–`192.168.50.199`.

- [ ] **Step 3: Optional — set a DHCP reservation on the router**

  Note the iPhone's MAC address (same screen, "Wi-Fi Address"). SSH to the router and add a reservation so the iPhone always lands at the same IP:

  ```bash
  ssh root@192.168.50.1
  uci add dhcp host
  uci set dhcp.@host[-1].name='precog-01-iphone-stage'
  uci set dhcp.@host[-1].mac='XX:XX:XX:XX:XX:XX'   # iPhone MAC
  uci set dhcp.@host[-1].ip='192.168.50.21'
  uci commit dhcp
  /etc/init.d/dnsmasq restart
  ```

  On the iPhone: Settings → Wi-Fi → forget `precrime-lan` then rejoin to pick up the new IP.

- [ ] **Step 4: Append the IP reservation to the network runbook**

  Add to `network/runbook.md` under a new section `## DHCP Reservations`:
  ```markdown
  ## DHCP Reservations
  | Hostname | MAC | IP |
  |---|---|---|
  | precog-01-iphone-stage | XX:XX:XX:XX:XX:XX | 192.168.50.21 |
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add network/runbook.md precog/kit-b-iphone-runbook.md
  git commit -m "precog kit-b: iPhone on precrime-lan, DHCP reservation .21"
  ```

---

### Task 3: Validate the iPhone is publishing NDI

**Files:**
- Modify: `precog/kit-b-iphone-runbook.md`

- [ ] **Step 1: On a laptop also joined to `precrime-lan`, install NDI Tools**

  Download from `https://ndi.video/tools/`. Install NDI Tools (includes Studio Monitor). Free, requires only an email signup.

- [ ] **Step 2: Launch NDI Studio Monitor on the laptop**

  Expected: the app opens fullscreen black with a menu/burger icon to pick a source.

- [ ] **Step 3: On the iPhone, launch NDI HX Camera and tap "Start"**

  Camera preview appears in the app. The iPhone is now publishing NDI.

- [ ] **Step 4: In NDI Studio Monitor on the laptop, open the source menu**

  Expected: `PRECOG-01-IPHONE-STAGE` appears in the list (sometimes shown as `PRECOG-01-IPHONE-STAGE (Channel 1)` or similar).

  Click it. The laptop's screen should now show the iPhone's camera feed live. **Latency target: <300ms glass-to-glass over the network.** Wave your hand in front of the iPhone and confirm the screen tracks it with no noticeable lag beyond that range.

- [ ] **Step 5: Append the M2 verification log to the runbook**

  Append to `precog/kit-b-iphone-runbook.md`:
  ```markdown
  ## M2 verification — completed YYYY-MM-DD
  - Source `PRECOG-01-IPHONE-STAGE` visible in NDI Studio Monitor on laptop
  - Latency observed: <fill in ballpark, e.g., 150ms>
  - Stream stable for: <duration tested, e.g., 10 min continuous>
  ```

- [ ] **Step 6: Commit**

  ```bash
  git add precog/kit-b-iphone-runbook.md
  git commit -m "precog kit-b: M2 verified, NDI source visible in Studio Monitor"
  ```

---

### Task 4: Power and sleep management for show use

**Files:**
- Modify: `precog/kit-b-iphone-runbook.md`

- [ ] **Step 1: Disable auto-lock while NDI HX Camera runs**

  Settings → Display & Brightness → Auto-Lock → **Never**.

  Note: this is global, so remember to set it back when not on a show. Document this in the runbook below.

- [ ] **Step 2: Enable Low Power Mode override (optional, recommended)**

  Settings → Battery → Low Power Mode → off (always-on radio for stable NDI).

- [ ] **Step 3: Plug the iPhone into a USB-C PD charger before each show**

  NDI HX Camera at 1080p drains roughly 30% battery per hour. Wired power is mandatory for shows over 90 minutes.

- [ ] **Step 4: Test 30-minute unattended run**

  Place the iPhone on its tripod mount, launch NDI HX Camera, tap Start, leave it. Verify on the laptop that the stream is still live and stable after 30 minutes. Expected: no drops, no app crash, no battery warning if wired.

- [ ] **Step 5: Document the show-day pre-flight checklist in the runbook**

  Append to `precog/kit-b-iphone-runbook.md`:

  ```markdown
  ## Show-day pre-flight checklist (PRECOG-01-IPHONE-STAGE)

  - [ ] Plug iPhone into USB-C PD charger
  - [ ] Settings → Display & Brightness → Auto-Lock = Never
  - [ ] Settings → Battery → Low Power Mode = OFF
  - [ ] Settings → Wi-Fi → connected to `precrime-lan`
  - [ ] Launch NDI HX Camera app
  - [ ] Tap Start
  - [ ] Verify in NDI Studio Monitor on laptop / multiview on REPORT
  - [ ] Mount iPhone in position, lock orientation, do not bump

  ## Post-show
  - [ ] Tap Stop in NDI HX Camera
  - [ ] Settings → Display & Brightness → Auto-Lock = back to normal (e.g., 5 min)
  ```

- [ ] **Step 6: Commit**

  ```bash
  git add precog/kit-b-iphone-runbook.md
  git commit -m "precog kit-b: power management + show-day checklist"
  ```

---

### Task 5: Physical mount and packaging

**Files:**
- Modify: `precog/kit-b-iphone-runbook.md`

- [ ] **Step 1: Verify your phone tripod mount fits an iPhone 15 case-on**

  If you don't have a tripod mount yet, the BOM lists a $15 Ulanzi or similar 1/4"-thread clamp. Test that the iPhone fits with whatever case you use day-to-day. If the case is too thick, plan to remove it for shows.

- [ ] **Step 2: Identify a small tripod or clamp mount option for venue use**

  Document what you intend to use (tabletop tripod, magic-arm clamp, gorillapod, etc).

- [ ] **Step 3: Append to runbook**

  Append to `precog/kit-b-iphone-runbook.md`:

  ```markdown
  ## Physical setup
  - Tripod mount: <model>
  - Tripod or clamp: <model>
  - USB-C PD cable: 6ft minimum recommended
  - Charger: any 20W+ USB-C PD brick
  - Storage: <where the kit lives between shows>
  ```

- [ ] **Step 4: Commit**

  ```bash
  git add precog/kit-b-iphone-runbook.md
  git commit -m "precog kit-b: physical mount + storage notes"
  ```

---

## File Structure Summary

```
precog/
└── kit-b-iphone-runbook.md   # Setup, power management, show-day checklist, post-show cleanup
```

No code in Kit B — the iPhone is its own self-contained NDI device.

## Done means

- M2 verified: iPhone visible in NDI Studio Monitor with <300ms latency
- DHCP reservation set so the IP is stable
- Show-day checklist documented
- 30-minute unattended run confirmed
