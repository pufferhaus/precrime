# Network Brain Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Configure a GL.iNet Flint 2 router as the PRECRIME LAN, with mDNS reflector, IGMP snooping, no client isolation, and predictable DHCP — so that NDI auto-discovery works reliably across the network.

**Architecture:** Flint 2 runs OpenWRT under GL.iNet's admin UI. Configuration is done via UCI (OpenWRT's CLI config system) over SSH for reproducibility, with the GL.iNet web UI used only for initial bring-up. Final config is exported to a USB stick so the router can be reflashed and restored in under 5 minutes if it fails on tour.

**Tech Stack:** OpenWRT (22.03+ base), UCI, `avahi-daemon` for mDNS reflector, `dnsmasq` for DHCP, `wpad-openssl` for WPA3.

**Milestone covered:** M1 from system spec.

---

### Task 1: Initial bring-up and firmware

**Files:**
- Create: `network/runbook.md` (will be appended through the plan)
- Create: `network/uci-config.sh` (UCI commands to apply config)

- [ ] **Step 1: Unbox Flint 2, connect WAN to internet, plug a laptop into LAN port 1**

- [ ] **Step 2: Browse to `http://192.168.8.1`, complete the GL.iNet setup wizard**

  - Set admin password (record in your password manager)
  - Set timezone
  - Choose default WiFi names you don't care about (we replace them via UCI shortly)

- [ ] **Step 3: Check firmware version, upgrade if needed**

  In the GL.iNet UI: System → Firmware Upgrade → Online Upgrade. Apply the latest stable. Wait for reboot.

- [ ] **Step 4: Enable SSH access**

  In the GL.iNet UI: System → Advanced Settings (or directly browse `http://192.168.8.1/cgi-bin/luci`).
  Set a root password for LuCI/SSH access. This password is independent from the GL.iNet admin password — use a different value.

- [ ] **Step 5: Verify SSH works from laptop**

  Run:
  ```bash
  ssh root@192.168.8.1
  ```
  Expected: prompt for password, login succeeds, you land at `root@GL-MT6000:~#` shell.

- [ ] **Step 6: Create the project runbook file**

  Create `network/runbook.md` with this initial content:

  ```markdown
  # PRECRIME Network Runbook

  ## Default Credentials Locations
  - Flint 2 admin (GL.iNet UI): password manager → "PRECRIME router admin"
  - Flint 2 root (SSH/LuCI): password manager → "PRECRIME router root"

  ## Setup History
  - 2026-05-16: Initial setup, firmware version <fill in>, configured per network-brain plan
  ```

- [ ] **Step 7: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add network/runbook.md
  git commit -m "network: initial runbook stub"
  ```

---

### Task 2: Define the UCI config script

The rest of the network setup is captured in a single shell script of UCI commands. This makes the config reproducible and easy to reapply to a replacement router.

**Files:**
- Create: `network/uci-config.sh`

- [ ] **Step 1: Create `network/uci-config.sh` with shebang and header**

  ```bash
  #!/bin/sh
  # PRECRIME network configuration — applies to GL.iNet Flint 2 (GL-MT6000)
  # Run via: scp this file to router, then `sh /tmp/uci-config.sh`
  set -e
  ```

- [ ] **Step 2: Commit the empty script**

  ```bash
  git add network/uci-config.sh
  git commit -m "network: scaffold UCI config script"
  ```

---

### Task 3: Wireless — SSID, WPA3, hide 2.4 GHz

**Files:**
- Modify: `network/uci-config.sh`

- [ ] **Step 1: Append wireless config to `network/uci-config.sh`**

  ```bash
  # ----- Wireless -----
  # 5 GHz: primary SSID for PRECRIME devices
  uci set wireless.@wifi-iface[1].ssid='precrime-lan'
  uci set wireless.@wifi-iface[1].encryption='sae'      # WPA3-only
  uci set wireless.@wifi-iface[1].key='REPLACE_WITH_STRONG_PASSWORD'
  uci set wireless.@wifi-iface[1].hidden='0'
  uci set wireless.@wifi-iface[1].disabled='0'
  uci set wireless.@wifi-iface[1].isolate='0'           # client isolation OFF — cams must reach REPORT

  # 2.4 GHz: hidden, same SSID, WPA2/WPA3 mixed for fallback compatibility
  uci set wireless.@wifi-iface[0].ssid='precrime-lan'
  uci set wireless.@wifi-iface[0].encryption='sae-mixed'
  uci set wireless.@wifi-iface[0].key='REPLACE_WITH_STRONG_PASSWORD'
  uci set wireless.@wifi-iface[0].hidden='1'
  uci set wireless.@wifi-iface[0].disabled='0'
  uci set wireless.@wifi-iface[0].isolate='0'

  uci commit wireless
  ```

- [ ] **Step 2: Manually set a strong password for both SSIDs (do NOT commit the password to git)**

  Replace both `REPLACE_WITH_STRONG_PASSWORD` placeholders with a strong password before running the script on the router. Record the password in your password manager.

- [ ] **Step 3: SCP the (passworded) script to the router**

  ```bash
  scp network/uci-config.sh root@192.168.8.1:/tmp/uci-config.sh
  ```

  Expected: file copies successfully.

- [ ] **Step 4: SSH in and run it**

  ```bash
  ssh root@192.168.8.1
  sh /tmp/uci-config.sh
  wifi reload
  ```

  Expected: no errors, both radios reload.

- [ ] **Step 5: Verify from a separate device**

  From a phone or laptop, scan for WiFi networks. Expected: `precrime-lan` visible on 5 GHz, 2.4 GHz hidden but joinable if you enter the SSID manually. Join with the password. Expected: device gets an IP in the `192.168.8.x` range (default range; we change it next task).

- [ ] **Step 6: Restore the placeholder before committing**

  Before `git add`, edit `network/uci-config.sh` and re-replace your real password with `REPLACE_WITH_STRONG_PASSWORD`. The script in git is a template; the real password lives only on the router and in your password manager.

- [ ] **Step 7: Commit**

  ```bash
  git add network/uci-config.sh
  git commit -m "network: wireless SSID + WPA3 + no isolation"
  ```

---

### Task 4: LAN addressing and DHCP

**Files:**
- Modify: `network/uci-config.sh`

- [ ] **Step 1: Append LAN config to `network/uci-config.sh`**

  ```bash
  # ----- LAN -----
  uci set network.lan.ipaddr='192.168.50.1'
  uci set network.lan.netmask='255.255.255.0'

  uci set dhcp.lan.start='100'
  uci set dhcp.lan.limit='100'            # range .100–.199 for dynamic
  uci set dhcp.lan.leasetime='12h'
  uci commit network
  uci commit dhcp
  ```

- [ ] **Step 2: Re-SCP and run on router**

  ```bash
  scp network/uci-config.sh root@192.168.8.1:/tmp/uci-config.sh
  ssh root@192.168.8.1 'sh /tmp/uci-config.sh && /etc/init.d/network restart && /etc/init.d/dnsmasq restart'
  ```

  Expected: brief network blip. You'll need to reconnect to the router on `192.168.50.1` after this.

- [ ] **Step 3: Verify new LAN**

  Update your laptop's WiFi connection. Run:
  ```bash
  ssh root@192.168.50.1
  ip addr show br-lan
  ```
  Expected: `inet 192.168.50.1/24` shown.

- [ ] **Step 4: Commit**

  ```bash
  git add network/uci-config.sh
  git commit -m "network: LAN subnet 192.168.50.0/24, DHCP .100-.199"
  ```

---

### Task 5: mDNS reflector (the NDI-critical piece)

This is the single most important change in the plan. Without an mDNS reflector spanning WiFi and ethernet, NDI auto-discovery fails when REPORT is wired and PRECOGs are wireless.

**Files:**
- Modify: `network/uci-config.sh`

- [ ] **Step 1: Install `avahi-daemon` on the router via SSH**

  ```bash
  ssh root@192.168.50.1
  opkg update
  opkg install avahi-daemon
  ```

  Expected: package installs successfully. If `opkg update` fails (no internet on WAN), plug the WAN port into your home internet first.

- [ ] **Step 2: Configure avahi as reflector**

  On the router, edit `/etc/avahi/avahi-daemon.conf`. Set:

  ```ini
  [server]
  use-ipv4=yes
  use-ipv6=no
  allow-interfaces=br-lan
  ratelimit-interval-usec=1000000
  ratelimit-burst=1000

  [reflector]
  enable-reflector=yes
  reflect-ipv=no
  ```

  Restart:
  ```bash
  /etc/init.d/avahi-daemon restart
  /etc/init.d/avahi-daemon enable
  ```

- [ ] **Step 3: Append the install command to `uci-config.sh` for reproducibility**

  ```bash
  # ----- mDNS reflector -----
  opkg update
  opkg install avahi-daemon
  # NOTE: avahi-daemon.conf must be manually placed; see network/avahi-daemon.conf
  /etc/init.d/avahi-daemon enable
  /etc/init.d/avahi-daemon restart
  ```

- [ ] **Step 4: Save the avahi config to git as a reference**

  Create `network/avahi-daemon.conf` in the repo containing the same content as Step 2.

- [ ] **Step 5: Verify mDNS reflector works**

  From a laptop on WiFi (`precrime-lan`), run:
  ```bash
  # macOS:
  dns-sd -B _services._dns-sd._udp local.

  # OR (any platform with avahi-utils installed):
  avahi-browse -a
  ```

  Then plug a second device (another laptop or a phone) into the LAN port on the router with an ethernet cable. Have it broadcast something via Bonjour (Apple devices announce themselves automatically). Within 5 seconds the WiFi laptop should see the wired device's announcements.

  Expected: cross-interface announcements visible. **If this test fails, the rest of the system fails — do not proceed past this point until the test passes.**

- [ ] **Step 6: Commit**

  ```bash
  git add network/uci-config.sh network/avahi-daemon.conf
  git commit -m "network: avahi-daemon mDNS reflector for NDI auto-discovery"
  ```

---

### Task 6: IGMP snooping and multicast hygiene

**Files:**
- Modify: `network/uci-config.sh`

- [ ] **Step 1: Append IGMP config to `uci-config.sh`**

  ```bash
  # ----- IGMP / multicast -----
  uci set network.@device[0].igmp_snooping='1'
  uci set network.@device[0].multicast_querier='1'
  uci set network.@device[0].robustness='2'
  uci set network.@device[0].query_interval='12500'  # centiseconds, = 125s
  uci commit network
  ```

  Note: `@device[0]` refers to the LAN bridge device. Verify by running `uci show network | grep device` on the router. The bridge entry will show `option name 'br-lan'`.

- [ ] **Step 2: Apply on router**

  ```bash
  scp network/uci-config.sh root@192.168.50.1:/tmp/uci-config.sh
  ssh root@192.168.50.1 'sh /tmp/uci-config.sh && /etc/init.d/network restart'
  ```

- [ ] **Step 3: Verify**

  ```bash
  ssh root@192.168.50.1
  cat /sys/class/net/br-lan/bridge/multicast_snooping
  ```

  Expected output: `1`.

- [ ] **Step 4: Commit**

  ```bash
  git add network/uci-config.sh
  git commit -m "network: IGMP snooping + multicast querier on br-lan"
  ```

---

### Task 7: Backup the router config to USB

**Files:**
- Modify: `network/runbook.md`

- [ ] **Step 1: Plug a USB stick into the Flint 2's USB-A port**

- [ ] **Step 2: SSH in and identify the USB device**

  ```bash
  ssh root@192.168.50.1
  ls /dev/sd*
  ```

  Expected: `/dev/sda1` or similar.

- [ ] **Step 3: Mount the USB stick**

  ```bash
  mkdir -p /mnt/usb
  mount /dev/sda1 /mnt/usb
  ```

  Expected: no errors. If mount fails on FAT32/exFAT due to missing kernel modules, install `kmod-fs-vfat kmod-fs-exfat` via `opkg`.

- [ ] **Step 4: Generate and save a config backup**

  ```bash
  sysupgrade -b /mnt/usb/precrime-router-backup-$(date +%Y%m%d).tar.gz
  ls -la /mnt/usb/
  umount /mnt/usb
  ```

  Expected: a `.tar.gz` file ~10-50KB.

- [ ] **Step 5: Document the restore procedure in `network/runbook.md`**

  Append the following section to `network/runbook.md`:

  ```markdown
  ## Restore from USB backup

  If the Flint 2 is lost or broken and a replacement is on hand:

  1. Flash latest GL.iNet firmware on the new unit via web UI
  2. After first boot, browse to `http://192.168.8.1`, complete setup wizard
  3. Plug in the USB stick with the latest `precrime-router-backup-*.tar.gz`
  4. SSH in, then:
     ```
     mount /dev/sda1 /mnt/usb
     sysupgrade -r /mnt/usb/precrime-router-backup-YYYYMMDD.tar.gz
     reboot
     ```
  5. Router reboots with the saved config. Total recovery time: ~5 minutes.

  ## Current backup
  - Date: 2026-05-16
  - File on USB: `precrime-router-backup-20260516.tar.gz`
  - USB stick label: "PRECRIME-NET-BAK"
  ```

- [ ] **Step 6: Commit**

  ```bash
  git add network/runbook.md
  git commit -m "network: document USB backup + restore procedure"
  ```

---

### Task 8: End-to-end mDNS validation (M1 milestone gate)

This is the M1 completion test. Do not consider Network Brain done until this passes.

**Files:**
- Modify: `network/runbook.md`

- [ ] **Step 1: Connect three devices to the LAN simultaneously**

  - Laptop A: WiFi to `precrime-lan`
  - Laptop B (or another phone): ethernet via short Cat6 patch to a LAN port on Flint 2
  - Any third Bonjour-aware device (iPhone, AirPlay speaker) on WiFi

- [ ] **Step 2: Run mDNS discovery from Laptop A**

  ```bash
  # macOS:
  dns-sd -B _airplay._tcp local.

  # OR any platform:
  avahi-browse -a -t -r
  ```

  Expected: announcements from devices on both interfaces show up within 5 seconds. Specifically, Laptop A on WiFi sees Laptop B's announcements over ethernet (and vice versa).

- [ ] **Step 3: Verify cross-interface ping works**

  From Laptop A (WiFi), ping Laptop B's `.local` hostname:
  ```bash
  ping laptop-b.local
  ```
  Expected: replies. If this fails, mDNS reflector is broken — re-verify Task 5.

- [ ] **Step 4: Append the M1 verification log to runbook**

  Append to `network/runbook.md`:
  ```markdown
  ## M1 verification — completed YYYY-MM-DD
  - mDNS reflector confirmed: cross-interface .local resolution works
  - DHCP confirmed: clients get 192.168.50.100–.199 addresses
  - Tested with: <list devices used>
  ```

- [ ] **Step 5: Commit**

  ```bash
  git add network/runbook.md
  git commit -m "network: M1 milestone passed, mDNS verified end-to-end"
  ```

---

## File Structure Summary

```
network/
├── uci-config.sh          # All UCI commands, reproducible on replacement hardware
├── avahi-daemon.conf      # Reference copy of the mDNS reflector config
└── runbook.md             # Operator runbook: credentials map, restore procedure, verification logs
```

`uci-config.sh` is the source of truth for "how is this router configured?" Anyone with a fresh Flint 2 + this script + the password from the password manager can recreate the PRECRIME LAN in under 30 minutes.

## Done means

- M1 verified end-to-end (Task 8)
- USB backup written, restore procedure documented
- All UCI commands captured in `uci-config.sh`
- Runbook accurate
