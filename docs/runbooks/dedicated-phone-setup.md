# Dedicated Phone Setup — Apple Configurator 2 + Single App Mode

This runbook turns an iPhone into a dedicated WITNESS camera unit that boots directly into the app, locks itself to it, and requires a passcode to exit. No home screen, no notifications, no accidental exits.

**Prerequisites:**
- Mac with Apple Configurator 2 installed (free, Mac App Store)
- Paid Apple Developer Program ($99/yr) — required for the `com.apple.developer.guided-access` entitlement
- iPhone to dedicate (will be wiped during supervision)
- USB cable (Lightning or USB-C depending on iPhone model)

---

## Overview

Two layers of lockdown:

| Layer | What it does | How configured |
|---|---|---|
| Supervision (MDM-like) | Allows configuration profiles; disables pairing with other macs | Apple Configurator 2 |
| Single App Mode profile | Forces phone to boot into WITNESS and stay there | Configuration profile pushed via Configurator |
| In-app Guided Access | App locks itself via `UIAccessibility.requestGuidedAccessSession` | WITNESS setting + entitlement |

Supervision + Single App Mode is the hardware layer. In-app Guided Access is the software layer. Together they make the phone effectively a single-purpose appliance.

---

## Step 1 — Prepare the iPhone

**Back up anything you want to keep** — supervision wipes the device.

1. Factory reset the phone: Settings → General → Transfer or Reset iPhone → Erase All Content and Settings
2. Complete initial iOS setup (language, region) — **stop before signing into Apple ID**
3. Connect to WiFi (required for activation)

---

## Step 2 — Supervise with Apple Configurator 2

1. Open **Apple Configurator 2** on mac
2. Connect iPhone via USB
3. Phone appears in Configurator as an unsupervised device
4. Right-click the phone → **Prepare**
5. Configuration: **Manual**
6. Enrol in MDM: **No** (skip — we don't need a full MDM server)
7. Supervise: **Yes**
8. Allow devices to pair with other computers: **No** (locks to this mac for management)
9. Assign to organisation: create a placeholder org (your name is fine)
10. Configure iOS Setup Assistant: skip as many screens as possible (WiFi, Passcode, etc.)
11. Click **Prepare** — Configurator downloads a supervision identity and re-provisions the phone (~5 min)

The phone reboots into a supervised state. It now accepts configuration profiles pushed from this mac.

---

## Step 3 — Install WITNESS

With the phone supervised and connected:

1. Build WITNESS in Xcode (signed with your paid Apple Developer team)
2. In Xcode: target the supervised iPhone → ⌘R to install
3. Verify WITNESS opens and reaches **LIVE ●** (mock_report.py running on mac)
4. Open WITNESS → Settings → **Kiosk mode → ON**
5. Also enable: iOS Settings → Accessibility → Guided Access → ON, set a passcode

---

## Step 4 — Push a Single App Mode profile

Single App Mode locks the phone to one app at the OS level — enforced by the supervision profile, independent of the app itself.

**Create the profile:**

1. In Apple Configurator 2: select the phone → **Add** → **Profiles** → **+**
2. General tab: name it `WITNESS Kiosk`
3. Left sidebar: click **Single App Mode**
4. App bundle ID: `art.precrime.witness`
5. Click **Save**
6. Configurator pushes the profile to the phone

The phone immediately locks to WITNESS. Home button, App Switcher, Control Centre — all disabled at the OS level.

**To exit Single App Mode** (for maintenance):
- In Configurator: select phone → Profiles → remove the Single App Mode profile
- Phone returns to normal home screen

---

## Step 5 — Final lockdown settings

With the phone in Single App Mode, push additional restrictions via profile:

1. In Configurator: **Add** → **Profiles** → **+** → **Restrictions**
2. Recommended restrictions:

| Restriction | Setting | Why |
|---|---|---|
| Allow installing apps | OFF | Prevents App Store access |
| Allow removing apps | OFF | Can't delete WITNESS |
| Allow Control Centre in apps | OFF | Prevents WiFi toggle etc. |
| Allow Notification Centre | OFF | No distracting banners |
| Allow screen capture | OFF | No screenshots |
| Allow Siri | OFF | Prevents voice commands |
| Require passcode on device | ON | Prevents physical access to settings |

3. Save and push

---

## Day-to-day operation

**Normal show-day:** plug in power, phone boots → Single App Mode → WITNESS opens automatically → kiosk mode activates → LIVE ●.

**Re-deploy WITNESS** (after cert renewal or app update):
1. Connect phone to the provisioning mac
2. In Configurator: remove Single App Mode profile (phone returns to home screen temporarily)
3. In Xcode: ⌘R to install updated build
4. In Configurator: re-push Single App Mode profile
5. Done — ~3 min total

**If phone needs maintenance** (iOS update, etc.):
1. Remove Single App Mode profile in Configurator
2. Perform maintenance
3. Re-push profile
4. Reconnect to Xcode and re-verify WITNESS

---

## Naming convention

For multiple dedicated phones, use consistent source names so REPORT slots stay predictable:

| Phone | Source name | REPORT slot |
|---|---|---|
| iPhone A (stage left) | WITNESS-STAGE-LEFT | 1 |
| iPhone B (stage right) | WITNESS-STAGE-RIGHT | 2 |
| iPhone C (wide) | WITNESS-WIDE | 3 |

Set source name in WITNESS Settings before pushing the Single App Mode profile.

Use `source_slot_overrides` in `report.conf` to pin these names to fixed slots so keyboard switch numbers stay consistent show to show.

---

## Checklist summary

- [ ] iPhone factory reset and through initial setup (no Apple ID)
- [ ] Supervised via Apple Configurator 2
- [ ] WITNESS installed (signed with paid developer cert)
- [ ] WITNESS Settings: Kiosk mode ON, source name set
- [ ] iOS Settings: Guided Access ON, passcode set
- [ ] Single App Mode profile pushed via Configurator
- [ ] Restrictions profile pushed
- [ ] Phone tested: boot → auto-opens WITNESS → LIVE ● within 10s
- [ ] Exit tested: triple-click → Guided Access passcode → exits kiosk
- [ ] Source name appears in REPORT multiview at correct slot

---

## Troubleshooting

**Phone won't be supervised (Configurator error)**
- Ensure phone is not already supervised by another mac
- Try different USB cable/port
- Full factory reset and retry from Step 1

**Single App Mode profile won't install**
- Phone must be supervised — unsupervised phones reject Single App Mode profiles
- Check Configurator shows phone as "Supervised"

**WITNESS doesn't auto-open after profile push**
- Verify bundle ID in profile exactly matches: `art.precrime.witness`
- Check WITNESS is installed: remove + re-push profile after installing

**Kiosk mode toggle has no effect**
- Guided Access must be enabled in iOS Settings → Accessibility → Guided Access first
- No special entitlement needed — the API works on any signed build

**Can't exit kiosk mode**
- Triple-click side button → enter Guided Access passcode
- If passcode forgotten: remove Single App Mode profile in Configurator to break out, then reset Guided Access passcode in iOS Settings
