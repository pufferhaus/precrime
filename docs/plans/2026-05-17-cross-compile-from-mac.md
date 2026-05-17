# Cross-Compile from macOS Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move all `cargo build` for `precog` and `report` off the Raspberry Pis and onto the macOS dev machine. Pis only receive pre-built aarch64 ELF binaries via `rsync`. This eliminates on-Pi toolchain RAM pressure (rustc + LLVM peak ~1.4–2.2 GB) and lets the rig run on the cheapest available Pi 5 SKUs (1GB precog, 2GB or 4GB report) — meaningful savings during the current LPDDR4 price spike.

**Architecture:**
- macOS dev box runs `cross build --release --target aarch64-unknown-linux-gnu` for each crate.
- `cross` runs a Debian-Bookworm-based Docker container (aarch64) that mirrors Pi OS Bookworm's GStreamer 1.22 / glibc / native lib ABI.
- A workspace-root `Cross.toml` declares pre-build apt installs for the aarch64-arch sysroot dev packages: `libgstreamer1.0-dev`, `libgstreamer-plugins-base1.0-dev`, `libcairo2-dev`, `libevdev-dev`, `libudev-dev`. Runtime plugins (x264, v4l2, rtp, kmssink, avdec_h264, compositor, cairooverlay) are dlopen'd at runtime on the Pi from apt-installed `gstreamer1.0-plugins-{good,bad,ugly,libav}`; the cross sysroot only needs the dev headers/libs for compile-time linking.
- Docker engine on the mac is provided by **colima** (free, OSS, lightweight Linux VM under the hood). Docker Desktop also works but is heavier and now license-encumbered for commercial use above $10M revenue.
- The existing `make install-*` provisioning steps (apt + systemd unit drop, run once per Pi) keep their SSH semantics. Only the iterative `build` + `deploy` loop moves to the mac.

**Tech Stack:**
- `cross` 0.2.x (https://github.com/cross-rs/cross)
- colima (Homebrew; runs a tiny Lima VM with Docker socket)
- Docker CLI (Homebrew; client only — colima provides the daemon)
- `target/aarch64-unknown-linux-gnu/release/{precog,report}` as the final artifacts
- Pi OS Bookworm 64-bit on all Pis (precog 1GB and report 2GB/4GB)

**Latency / size targets:**
- Clean `cross build --release` of full workspace on M-series mac: target < 3 min cold, < 30 s warm.
- Stripped release binary size: precog < 12 MB, report < 18 MB (uses cairo + extra gstreamer-video). `rsync` to Pi < 5 s on gigabit LAN.

**Cost impact on hardware BOM** (per LPDDR4 pricing as of 2026-05-17):
- precog: Pi 5 1GB ($45) instead of Pi 5 4GB ($70). Save $25 per precog.
- report: Pi 5 2GB ($55) or 4GB ($70) instead of Pi 5 8GB ($130). Save $60–75.
- For a 2-precog rig: ~$110 total Pi 5 savings. For a 4-precog rig: ~$175.

---

## Task 1: Install + smoke-test colima + Docker + cross on the mac

**Files:** none in this task — environment setup only.

- [ ] **Step 1: Install colima + docker-client + cross via Homebrew**

```bash
brew install colima docker
cargo install cross --git https://github.com/cross-rs/cross
```

`cargo install --git` rather than `cargo install cross` because the published cross crate lags behind main; the git tip has better M-series mac support and a newer default image.

- [ ] **Step 2: Start the colima Docker VM**

```bash
colima start --cpu 4 --memory 8 --disk 60
```

4 cores + 8 GB is enough headroom for the cross container + rustc parallelism without starving the host mac. 60 GB disk lets the image + target dir grow without hitting limits.

- [ ] **Step 3: Verify `cross` finds Docker**

```bash
cross --version
docker info | head -20
```

Expected: `cross 0.2.x` plus a Docker `Server Version: ...` (proves the daemon is reachable via colima's socket).

- [ ] **Step 4: Smoke-test cross on a trivial target**

```bash
cd $(mktemp -d) && cargo init hello && cd hello
cross build --release --target aarch64-unknown-linux-gnu
file target/aarch64-unknown-linux-gnu/release/hello
```

Expected: `file` reports `ELF 64-bit LSB pie executable, ARM aarch64`. This proves the toolchain works before we wire in GStreamer.

- [ ] **Step 5: No commit yet** — environment setup is per-developer state, not repo state. Move to Task 2.

---

## Task 2: Workspace `Cross.toml` with GStreamer / cairo / evdev sysroot deps

**Files:**
- Create: `Cross.toml`

- [ ] **Step 1: Write `Cross.toml` at the workspace root**

```toml
# Cross-compile config for `cross build --target aarch64-unknown-linux-gnu`.
# Mirrors the Debian Bookworm aarch64 sysroot used by Raspberry Pi OS Bookworm 64-bit
# on the Pi 5s. Each pre-build step installs the dev headers/libs needed for
# compile-time linking of gstreamer-rs, cairo-rs, and evdev-rs. Runtime plugins
# (x264, v4l2, rtp, etc.) are dlopen'd from apt-installed packages on the Pi.

[target.aarch64-unknown-linux-gnu]
image = "ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main"

pre-build = [
    "dpkg --add-architecture $CROSS_DEB_ARCH",
    "apt-get update",
    "apt-get install --assume-yes --no-install-recommends \
        libgstreamer1.0-dev:$CROSS_DEB_ARCH \
        libgstreamer-plugins-base1.0-dev:$CROSS_DEB_ARCH \
        libcairo2-dev:$CROSS_DEB_ARCH \
        libevdev-dev:$CROSS_DEB_ARCH \
        libudev-dev:$CROSS_DEB_ARCH \
        pkg-config",
]

[target.aarch64-unknown-linux-gnu.env]
# pkg-config lookups inside the cross container must resolve aarch64 .pc files
# from the multiarch path; without this, pkg-config can silently pick host .pc.
PKG_CONFIG_PATH = "/usr/lib/aarch64-linux-gnu/pkgconfig:/usr/share/pkgconfig"
PKG_CONFIG_ALLOW_CROSS = "1"
```

- [ ] **Step 2: Build `temple` first (no native deps — proves the toolchain)**

```bash
cross build --release --target aarch64-unknown-linux-gnu --package temple
```

Expected: success in < 60 s. No native deps to satisfy.

- [ ] **Step 3: Build `precog` (gstreamer-rs core)**

```bash
cross build --release --target aarch64-unknown-linux-gnu --package precog
file target/aarch64-unknown-linux-gnu/release/precog
```

Expected: ELF aarch64 binary, size < 12 MB. If pkg-config fails to find gstreamer, re-check the `PKG_CONFIG_PATH` env entry above. If linking fails with "undefined reference to gst_*", the apt install missed `libgstreamer1.0-dev:$CROSS_DEB_ARCH`.

- [ ] **Step 4: Build `report` (gstreamer + gstreamer-video + cairo + evdev)**

```bash
cross build --release --target aarch64-unknown-linux-gnu --package report
file target/aarch64-unknown-linux-gnu/release/report
```

Expected: ELF aarch64 binary, size < 18 MB. Cairo and evdev linkages happen here for the first time — verify those .pc files were installed by the pre-build.

- [ ] **Step 5: Strip + verify size**

```bash
aarch64-linux-gnu-strip target/aarch64-unknown-linux-gnu/release/precog
aarch64-linux-gnu-strip target/aarch64-unknown-linux-gnu/release/report
ls -la target/aarch64-unknown-linux-gnu/release/{precog,report}
```

Note: `aarch64-linux-gnu-strip` may not exist on macOS without `brew install aarch64-elf-binutils` or similar. If unavailable, do `strip` inside the cross container or accept unstripped binaries (~30–40 MB) for now — Pi storage is plentiful.

- [ ] **Step 6: Commit**

```bash
git add Cross.toml
git commit -m "build: cross-compile config for aarch64 (Pi 5) from macOS"
```

---

## Task 3: Rewrite Makefile — cross-build + rsync deploy, no SSH-side cargo

**Files:**
- Modify: `Makefile`

- [ ] **Step 1: Read current Makefile**

`make help` should list the existing targets. We're replacing the `deploy-*` and adding new `build-*` and `clean-*` targets. `install-*` targets (apt deps + systemd unit drop, run once per Pi) keep their SSH semantics.

- [ ] **Step 2: Add aarch64 build + deploy targets**

Insert near the top of the Makefile (or replace existing build/deploy):

```make
# ---- Cross-compile from macOS to aarch64 (Raspberry Pi 5) ----

TARGET := aarch64-unknown-linux-gnu
RELEASE_DIR := target/$(TARGET)/release

build-precog:
	cross build --release --target $(TARGET) --package precog

build-report:
	cross build --release --target $(TARGET) --package report

build-all: build-precog build-report

clean-cross:
	cross clean --target $(TARGET)

# ---- Deploy: build, rsync binary, restart systemd unit ----

# Override these per Pi:
PRECOG_HOST ?= precog-01.local
PRECOG_USER ?= precog
REPORT_HOST ?= report.local
REPORT_USER ?= pi

deploy-precog: build-precog
	rsync -avz --progress $(RELEASE_DIR)/precog \
	    $(PRECOG_USER)@$(PRECOG_HOST):/usr/local/bin/precog.new
	ssh $(PRECOG_USER)@$(PRECOG_HOST) \
	    'sudo mv /usr/local/bin/precog.new /usr/local/bin/precog \
	     && sudo systemctl restart precog.service'

deploy-report: build-report
	rsync -avz --progress $(RELEASE_DIR)/report \
	    $(REPORT_USER)@$(REPORT_HOST):/usr/local/bin/report.new
	ssh $(REPORT_USER)@$(REPORT_HOST) \
	    'sudo mv /usr/local/bin/report.new /usr/local/bin/report \
	     && sudo systemctl restart report.service'
```

The `.new` + atomic-rename pattern avoids a partial-overwrite if the rsync drops mid-flight.

- [ ] **Step 3: Update / replace `logs-*` and `restart-*` targets** to keep the same UX:

```make
logs-precog:
	ssh $(PRECOG_USER)@$(PRECOG_HOST) 'journalctl -u precog.service -f'

logs-report:
	ssh $(REPORT_USER)@$(REPORT_HOST) 'journalctl -u report.service -f'

restart-precog:
	ssh $(PRECOG_USER)@$(PRECOG_HOST) 'sudo systemctl restart precog.service'

restart-report:
	ssh $(REPORT_USER)@$(REPORT_HOST) 'sudo systemctl restart report.service'
```

- [ ] **Step 4: Delete any old `deploy-*` recipes that ran `cargo build` over SSH**

Look for any line that SSHes into a Pi to run `cargo`. Remove all of them. The new flow never builds on a Pi.

- [ ] **Step 5: Add a `help` target listing the new shape**

```make
help:
	@echo "Build (cross-compile from macOS to aarch64):"
	@echo "  make build-precog | build-report | build-all"
	@echo "  make clean-cross"
	@echo ""
	@echo "Deploy (build + rsync binary + restart systemd):"
	@echo "  make deploy-precog PRECOG_HOST=...  PRECOG_USER=..."
	@echo "  make deploy-report REPORT_HOST=...  REPORT_USER=..."
	@echo ""
	@echo "Iterate:"
	@echo "  make logs-precog | logs-report"
	@echo "  make restart-precog | restart-report"
	@echo ""
	@echo "First-time provision (apt + systemd unit, run once per Pi):"
	@echo "  make install-precog | install-report"
```

- [ ] **Step 6: Smoke**

```bash
make build-all
make help
```

Both should succeed. No Pi needed for this step.

- [ ] **Step 7: Commit**

```bash
git add Makefile
git commit -m "build: Makefile retargets cross-compile + rsync (no on-Pi cargo)"
```

---

## Task 4: Update `install-*` targets to drop on-Pi rustup / cargo

**Files:**
- Modify: `report/install.sh`
- Modify: `precog/install.sh`

The Pis no longer need a Rust toolchain. They still need: apt-installed gstreamer plugins, the systemd unit file, a user account, and the config file template.

- [ ] **Step 1: Edit each `install.sh`** — remove any rustup install lines and `cargo` invocations. Keep the `apt-get install` block (gstreamer plugins, libcairo2, libevdev2, etc.) and the systemd-unit drop / enable.

Specifically, look for:
- `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` — DELETE
- Any `cargo install` or `cargo build` — DELETE
- The Rust toolchain `rust-toolchain.toml` SCP — DELETE

Keep:
- `apt-get install --assume-yes gstreamer1.0-plugins-{good,bad,ugly,libav} gstreamer1.0-tools`
- Cairo / evdev / udev runtime packages (no `-dev`, those are only needed in the cross sysroot)
- systemd unit drop
- User + directory creation

- [ ] **Step 2: Verify each install.sh remains idempotent** (re-running shouldn't fail).

- [ ] **Step 3: Smoke** (manual, deferred until first Pi is provisioned)

```bash
make install-precog PRECOG_HOST=precog-01.local
make deploy-precog  PRECOG_HOST=precog-01.local
```

- [ ] **Step 4: Commit**

```bash
git add precog/install.sh report/install.sh
git commit -m "deploy: install.sh no longer installs rust toolchain (binaries built on mac)"
```

---

## Task 5: CI — add cross check job

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Add a `cross-check` job** that runs `cross check --target aarch64-unknown-linux-gnu --workspace`. This catches sysroot drift (e.g., a new GStreamer dep that needs `libgstreamer-something-dev` not currently in `Cross.toml`).

GitHub Actions Ubuntu runners can run `cross` natively without colima (they have Docker installed by default).

```yaml
  cross-check:
    name: cross-check (aarch64-unknown-linux-gnu)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install cross
        run: cargo install cross --git https://github.com/cross-rs/cross
      - name: cross check
        run: cross check --target aarch64-unknown-linux-gnu --workspace --all-targets
```

`cross check` is faster than `cross build` (skips codegen) and catches the same dep / linkage issues. Optional: also run `cross build` in CI on PRs to catch the full link.

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add cross-check job for aarch64 sysroot drift"
```

---

## Task 6: README + docs update

**Files:**
- Modify: `README.md` (Build section + macOS dev block)
- Modify: `.docs/ROADMAP.md` (Design Notes — replace "rejected zigbuild" line)

- [ ] **Step 1: Rewrite README Build section**

Replace the "First-time install on a Pi" block with:

```markdown
## Build

All Rust compilation happens on the macOS dev machine. Pis only run the
deployed aarch64 binaries — no rustup, no cargo on the Pi side. This keeps
runtime RAM available for GStreamer (precog can run on a 1GB Pi 5; report on
2GB) and avoids on-Pi build latency.

**Prereqs on macOS (one-time):**

\`\`\`bash
brew install colima docker
colima start --cpu 4 --memory 8 --disk 60
cargo install cross --git https://github.com/cross-rs/cross
\`\`\`

**First-time provision a Pi (apt deps + systemd unit, run once):**

\`\`\`bash
make install-precog PRECOG_HOST=precog-01.local
make install-report REPORT_HOST=report.local
\`\`\`

**Build + deploy (iterate freely):**

\`\`\`bash
make deploy-precog PRECOG_HOST=precog-01.local  # cross build + rsync + systemctl restart
make deploy-report REPORT_HOST=report.local
\`\`\`

**Iterate:**

\`\`\`bash
make logs-precog | logs-report        # tail journalctl
make restart-precog | restart-report  # restart only, no rebuild
\`\`\`

\`make help\` lists every target.
```

- [ ] **Step 2: Update `.docs/ROADMAP.md` Design Notes**

Replace:

```markdown
- **Build:** Makefile-driven SSH iteration on Pi. No cross-compile (rejected zigbuild — mandleROT-style on-target build is fast enough).
```

with:

```markdown
- **Build:** Cross-compile from macOS via `cross` + colima (Docker). Binaries rsynced to Pi; no rustup or cargo on the Pi side. Frees ~1.5 GB of RAM on each Pi at idle, enables 1GB precog SKU.
```

- [ ] **Step 3: Commit**

```bash
git add README.md .docs/ROADMAP.md
git commit -m "docs: README + ROADMAP for cross-compile-from-macOS workflow"
```

Note: `.docs/ROADMAP.md` is in `.gitignore`. The edit is for the user's local notes only; the commit will only contain the README change. Update the file by-hand.

---

## Task 7: BOM revision in README

**Files:**
- Modify: `README.md` (Hardware section)

- [ ] **Step 1: Update the BOM**

Replace:

```markdown
- Raspberry Pi 5 8GB (REPORT) + accessories — ~$130
- Raspberry Pi 5 4GB (PRECOG encoder) + accessories — ~$100
```

with:

```markdown
- Raspberry Pi 5 2GB (REPORT, with HW H.264 decode) — $55 (4GB at $70 if staying on software decode)
- Raspberry Pi 5 1GB (PRECOG encoder, each) — $45
- Active cooler per Pi 5 — $5 (mandatory for sustained x264 software encode)
```

Add a note:

```markdown
**Memory sizing rationale:** all Rust compilation runs on the macOS dev machine
via `cross` (see Build section); Pis receive pre-built aarch64 binaries.
Runtime memory budgets:
- precog @ 1080p30: ~400–500 MB resident (GStreamer + x264 state + capture
  buffers) — fits 1GB Pi 5 with ~500 MB headroom.
- report @ 4 sources: ~600 MB with HW decode (`v4l2slh264dec`), ~1.1 GB with
  software decode (`avdec_h264`). 2GB Pi 5 fits HW-decode case; 4GB needed for
  software-decode case or >4 sources.

LPDDR4 prices are volatile in 2026; the 1GB Pi 5 ($45) was added by the Foundation
specifically as a budget point during the spike. Going 1GB precog instead of
4GB saves $25/unit at current prices — meaningful at scale.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: BOM revised for cross-compile workflow + LPDDR4 pricing"
```

---

## Task 8: Smoke + first-Pi deploy (deferred to hardware in-hand)

Manual milestone checklist. No code in this task.

- [ ] M-CC1: First successful `cross build --release --workspace` on macOS, both binaries strip-ok.
- [ ] M-CC2: First `make deploy-precog` to a real Pi 5 1GB succeeds, binary runs, systemd reports active.
- [ ] M-CC3: `tcpdump -i eth0 -nn host 239.42.1.99` from a laptop on the same switch shows RTP packets from the deployed precog within 5 s of `make deploy-precog`.
- [ ] M-CC4: `journalctl -u precog.service -f` shows the ball-tx thread emitting every 2 s.
- [ ] M-CC5: First `make deploy-report` to a Pi 5 2GB, HW decode enabled, 1 precog input rendering to HDMI program output.
- [ ] M-CC6: Two-precog rig: both visible in REPORT multiview, switch latency ≤ 200 ms via keyboard.
- [ ] M-CC7: 2-hour soak on the 2GB report: no OOM, no journald ENOMEM, rtpjitterbuffer underrun count stable.

If M-CC5 or M-CC7 fails due to RAM pressure on the 2GB report, bump to 4GB. Don't pre-commit to 2GB without the soak result.

---

## Risks + mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| Docker / colima setup friction on mac | Medium | Document exact `brew install` + `colima start` commands in README. Provide a `make doctor` target later that checks `docker info`. |
| GStreamer 1.22 ABI drift if Pi OS upgrades to Trixie (1.24) | Low (Bookworm is current stable) | Single-line `Cross.toml` change to bump image to `:trixie`. |
| `libgstreamer-plugins-base1.0-dev:arm64` apt install times out in cross container | Low | If hit, build a custom Dockerfile from `cross-rs/aarch64-unknown-linux-gnu` and bake the deps in; reference it via `Cross.toml`'s `image = ...`. One-shot fix. |
| HW decode (`v4l2slh264dec`) not actually faster on Pi 5 2GB | Low — Pi 5 has dedicated H.264 stateless decoder | Fall back to `avdec_h264` + 4GB report. $15 BOM bump. |
| Binary size growth makes rsync slow over WiFi | Very Low | Stripped < 18 MB; gigabit LAN is target anyway. |
| `cross` upstream breaks against future Rust toolchain | Low–Medium | Pin `cross` version in CI; project's `rust-toolchain.toml` already pins rustc. |

---

## Post-merge follow-ups

After this plan lands and the first cross-built deploy succeeds:

- [ ] **HW decode in REPORT.** Add a `target_os = "linux"` cfg branch in `report/src/pipeline.rs` that swaps `avdec_h264` for `v4l2slh264dec` on Pi 5. Cuts decode memory ~75% per stream (lets 2GB report handle more sources). Separate plan doc — small enough to be a single-task.
- [ ] **HW encode for low-CPU precog variants.** Pi 4 had `v4l2h264enc`; Pi 5 lost it. Not worth a Pi 4 cfg branch unless someone wants to deploy on existing Pi 4 hardware.
- [ ] **`make doctor`** — checks `colima status`, `docker info`, `cross --version`, prints helpful errors if any are missing.
- [ ] **Reproducible builds** — pin the cross image SHA in `Cross.toml` once we've validated a known-good baseline. Currently uses `:main` which can drift.
