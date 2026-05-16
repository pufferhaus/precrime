# REPORT (Switcher) Implementation Plan — Rust

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build REPORT, a headless Rust binary that runs on a Pi 5, discovers N NDI sources named `PRECOG-NN-*` on the LAN, outputs a single operator-chosen source as program video on HDMI-A-1, and outputs a multiview preview with tally overlay on HDMI-A-2. Switching is driven by a USB keyboard for Phase 1 (MEZZANINE is a future USB HID device — no software change required).

**Architecture:** A single Rust binary, member of a cargo workspace at the repo root. The binary owns two independent GStreamer pipelines, each rendering directly to a DRM/KMS connector via `kmssink` (no X11, no Wayland, no desktop env). NDI ingest uses the `ndisrc` element from `gstreamer1.0-plugins-rs` (apt-installed). NDI source *discovery* is done via a small FFI module around `libndi`'s `NDIlib_find_*` API, since `ndisrc` itself is for stream consumption, not enumeration. Keyboard input uses the `evdev` crate. Tally overlay is drawn in a `cairo` callback attached to the multiview compositor. The whole thing is wrapped in `systemd` with auto-restart.

**Tech Stack:** Raspberry Pi OS 12 Lite (64-bit), GStreamer 1.22+, `gstreamer1.0-plugins-rs`, NewTek NDI SDK runtime, Rust stable (1.75+), `gstreamer` crate, `evdev`, `cairo-rs`, `serde` + `toml`, `tracing` + `tracing-journald`, `anyhow`, `thiserror`, `systemd`.

**Milestones covered:** M5, M6, M7, M8 from system spec.

**Depends on:** Network Brain plan complete (M1). At least one PRECOG plan complete and a source live on the LAN, for end-to-end tests.

---

### Task 1: Pi flash and headless bring-up

(Identical to the original plan; reproduced compactly.)

**Files:**
- Create: `report/runbook.md`

- [ ] **Step 1: Flash Pi OS Lite 64-bit to a 64GB microSD via Raspberry Pi Imager**

  Advanced settings: hostname `report`, SSH enabled, username `cody`, strong password (password manager), WiFi skipped (REPORT is wired), locale set.

- [ ] **Step 2: Insert microSD, attach active cooler, wire REPORT to the Flint 2 via Cat6, power on**

- [ ] **Step 3: SSH from laptop**

  ```bash
  ssh cody@report.local
  ```

- [ ] **Step 4: Set static DHCP reservation for REPORT at `.10`**

  ```bash
  ssh root@192.168.50.1
  uci add dhcp host
  uci set dhcp.@host[-1].name='report'
  uci set dhcp.@host[-1].mac='<REPORT_ETHERNET_MAC>'
  uci set dhcp.@host[-1].ip='192.168.50.10'
  uci commit dhcp
  /etc/init.d/dnsmasq restart
  ```

  Reboot REPORT to pick up the new IP.

- [ ] **Step 5: Append to `network/runbook.md` DHCP reservation table**

  Add the `report | <mac> | 192.168.50.10` row.

- [ ] **Step 6: Update the system**

  ```bash
  ssh cody@192.168.50.10
  sudo apt update && sudo apt full-upgrade -y
  sudo reboot
  ```

- [ ] **Step 7: Create `report/runbook.md`**

  ```markdown
  # REPORT — Switcher Runbook

  ## Identity
  - Hostname: `report`
  - Wired IP: `192.168.50.10`
  - Hardware: Pi 5 8GB + active cooler + dual HDMI

  ## Setup history
  - 2026-05-16: Initial provision per report-switcher plan (Rust)
  ```

- [ ] **Step 8: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add network/runbook.md report/runbook.md
  git commit -m "report: initial Pi 5 runbook, static IP .10"
  ```

---

### Task 2: Install Rust toolchain, GStreamer, libndi on REPORT

**Files:**
- Create: `report/install.sh`

- [ ] **Step 1: Install apt packages on REPORT**

  ```bash
  ssh cody@192.168.50.10
  sudo apt install -y \
      build-essential pkg-config curl \
      libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
      gstreamer1.0-tools \
      gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad \
      gstreamer1.0-plugins-ugly \
      gstreamer1.0-plugins-rs \
      libcairo2-dev \
      libudev-dev \
      v4l-utils
  ```

- [ ] **Step 2: Install rustup + stable Rust on REPORT**

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  source "$HOME/.cargo/env"
  rustc --version
  ```

  Expected: rustc 1.75 or newer.

- [ ] **Step 3: Install the NDI SDK runtime libndi.so**

  Download from `https://ndi.video/sdk/` (free, requires email). On the Pi:
  ```bash
  cd ~
  curl -L -o ndi-sdk.tar.gz "<URL_FROM_NDI_DOWNLOAD_PAGE>"
  tar xzf ndi-sdk.tar.gz
  cd "NDI SDK for Linux/lib/aarch64-rpi4-linux-gnueabi"
  sudo cp libndi.so* /usr/local/lib/
  sudo ldconfig
  ```

- [ ] **Step 4: Verify ndisrc element loads**

  ```bash
  gst-inspect-1.0 ndisrc | head -10
  ```

  Expected: element description prints.

- [ ] **Step 5: Identify DRM connector IDs for HDMI-A-1 and HDMI-A-2**

  ```bash
  for f in /sys/class/drm/card*-HDMI-A-*/connector_id; do
      echo "$(dirname $f) -> $(cat $f)"
  done
  ```

  Record the two IDs. Plug a known display into HDMI-A-1 first, rerun, identify which ID is which.

- [ ] **Step 6: Write `report/install.sh`**

  Reproducible installer for the apt + rustup steps. NDI SDK is documented separately because the URL changes per version.

  ```bash
  #!/bin/sh
  set -e
  echo "Installing GStreamer, build deps, and Rust toolchain..."
  sudo apt update
  sudo apt install -y \
      build-essential pkg-config curl \
      libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
      gstreamer1.0-tools \
      gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad \
      gstreamer1.0-plugins-ugly \
      gstreamer1.0-plugins-rs \
      libcairo2-dev \
      libudev-dev \
      v4l-utils
  if ! command -v rustc >/dev/null 2>&1; then
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  fi
  echo ""
  echo "Next: manually install NDI SDK runtime libndi.so per the runbook."
  echo "Then run: gst-inspect-1.0 ndisrc"
  ```

  ```bash
  chmod +x report/install.sh
  ```

- [ ] **Step 7: Append connector IDs to runbook**

  ```markdown
  ## DRM Connectors
  - HDMI-A-1 (program out): connector_id = <ID_1>
  - HDMI-A-2 (multiview):   connector_id = <ID_2>
  ```

- [ ] **Step 8: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/install.sh report/runbook.md
  git commit -m "report: install script + DRM connector IDs"
  ```

---

### Task 3: Cargo workspace + report crate scaffolding

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `report/Cargo.toml`
- Create: `report/src/main.rs`
- Create: `.gitignore`

- [ ] **Step 1: Create the workspace `Cargo.toml` at repo root**

  ```toml
  [workspace]
  resolver = "2"
  members = ["report", "precog"]

  [workspace.package]
  edition = "2021"
  rust-version = "1.75"
  license = "MIT OR Apache-2.0"
  authors = ["Cody <byrnes.cody@gmail.com>"]

  [workspace.lints.rust]
  unsafe_op_in_unsafe_fn = "deny"

  [workspace.lints.clippy]
  pedantic = { level = "warn", priority = -1 }
  module_name_repetitions = "allow"
  ```

- [ ] **Step 2: Create `.gitignore`**

  ```gitignore
  /target
  Cargo.lock
  *.swp
  .DS_Store
  ```

  Note: we **do** want `Cargo.lock` committed for binaries. Override:

  Actually, edit `.gitignore` to be:
  ```gitignore
  /target
  *.swp
  .DS_Store
  ```

  (Cargo.lock IS committed for binary crates — controls deploy reproducibility.)

- [ ] **Step 3: Create `report/Cargo.toml`**

  ```toml
  [package]
  name = "report"
  version = "0.1.0"
  edition.workspace = true
  rust-version.workspace = true
  license.workspace = true
  authors.workspace = true
  description = "PRECRIME REPORT: headless NDI switcher daemon"

  [dependencies]
  anyhow = "1"
  thiserror = "1"
  serde = { version = "1", features = ["derive"] }
  toml = "0.8"
  tracing = "0.1"
  tracing-subscriber = { version = "0.3", features = ["env-filter"] }
  tracing-journald = "0.3"
  gstreamer = "0.23"
  gstreamer-video = "0.23"
  cairo-rs = { version = "0.20", features = ["use_glib"] }
  evdev = "0.12"
  parking_lot = "0.12"

  [dev-dependencies]
  pretty_assertions = "1"

  [lints]
  workspace = true
  ```

- [ ] **Step 4: Create the minimal `report/src/main.rs`**

  ```rust
  //! REPORT — PRECRIME switcher daemon entry point.

  use anyhow::Result;

  fn main() -> Result<()> {
      tracing_subscriber::fmt().with_env_filter("info").init();
      tracing::info!("REPORT starting");
      Ok(())
  }
  ```

- [ ] **Step 5: Verify the workspace builds on macOS**

  From the repo root:
  ```bash
  cargo check -p report
  ```

  Expected: clean check, all dependencies resolved. (GStreamer dev headers are required on macOS for the `gstreamer` crate to compile — install via `brew install gstreamer` if missing.)

  **If GStreamer is not installed on macOS:** the user may skip this step and rely on Pi-side build. The plan favors macOS-side builds for fast iteration, but Pi-side build is the fallback.

- [ ] **Step 6: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add Cargo.toml .gitignore report/Cargo.toml report/src/main.rs
  git commit -m "report: cargo workspace + report crate scaffolding"
  ```

---

### Task 4: Config parsing module (TDD)

**Files:**
- Create: `report/src/config.rs`
- Modify: `report/src/main.rs` (add `mod config;`)
- Create: `report/tests/config.rs`

- [ ] **Step 1: Write the failing integration test**

  Create `report/tests/config.rs`:

  ```rust
  use report::config::ReportConfig;

  #[test]
  fn parses_minimal_config() {
      let raw = r#"
          program_connector_id = 32
          preview_connector_id = 34
          keyboard_device = "/dev/input/event0"
      "#;
      let cfg = ReportConfig::from_toml(raw).expect("parse");
      assert_eq!(cfg.program_connector_id, 32);
      assert_eq!(cfg.preview_connector_id, 34);
      assert_eq!(cfg.keyboard_device, "/dev/input/event0");
      assert!(cfg.source_slot_overrides.is_empty());
  }

  #[test]
  fn parses_source_slot_overrides() {
      let raw = r#"
          program_connector_id = 32
          preview_connector_id = 34
          keyboard_device = "/dev/input/event0"

          [source_slot_overrides]
          "PRECOG-01-IPHONE-STAGE" = 1
          "PRECOG-02-CCTV-DOOR" = 2
      "#;
      let cfg = ReportConfig::from_toml(raw).expect("parse");
      assert_eq!(cfg.source_slot_overrides.get("PRECOG-01-IPHONE-STAGE"), Some(&1));
      assert_eq!(cfg.source_slot_overrides.get("PRECOG-02-CCTV-DOOR"), Some(&2));
  }

  #[test]
  fn missing_required_field_errors() {
      let raw = r#"
          preview_connector_id = 34
          keyboard_device = "/dev/input/event0"
      "#;
      assert!(ReportConfig::from_toml(raw).is_err());
  }
  ```

  Reaching `report::config::ReportConfig` from the integration test requires `report` to be a library crate too. Add a `lib.rs` in the next step.

- [ ] **Step 2: Convert report to a lib+bin crate**

  Create `report/src/lib.rs`:

  ```rust
  //! REPORT library — exposes modules for integration testing.
  pub mod config;
  ```

  Update `report/Cargo.toml` to add an explicit `[lib]` section (Cargo will infer if both `src/lib.rs` and `src/main.rs` exist; no change needed unless you want to rename):

  No edit needed — Cargo auto-detects.

- [ ] **Step 3: Run the test, watch it fail**

  ```bash
  cargo test -p report --test config
  ```

  Expected: compile error — `report::config::ReportConfig` doesn't exist.

- [ ] **Step 4: Implement `report/src/config.rs`**

  ```rust
  //! TOML config parsing for REPORT.

  use serde::Deserialize;
  use std::collections::HashMap;
  use thiserror::Error;

  #[derive(Debug, Error)]
  pub enum ConfigError {
      #[error("toml parse: {0}")]
      Toml(#[from] toml::de::Error),
  }

  #[derive(Debug, Deserialize)]
  pub struct ReportConfig {
      pub program_connector_id: u32,
      pub preview_connector_id: u32,
      pub keyboard_device: String,
      #[serde(default)]
      pub source_slot_overrides: HashMap<String, u8>,
  }

  impl ReportConfig {
      pub fn from_toml(raw: &str) -> Result<Self, ConfigError> {
          Ok(toml::from_str(raw)?)
      }
  }
  ```

- [ ] **Step 5: Update `report/src/main.rs` to use the library**

  Replace `report/src/main.rs` with:

  ```rust
  //! REPORT — PRECRIME switcher daemon entry point.

  use anyhow::{Context, Result};
  use report::config::ReportConfig;
  use std::env;
  use std::fs;

  fn main() -> Result<()> {
      tracing_subscriber::fmt().with_env_filter("info").init();

      let config_path =
          env::var("REPORT_CONFIG").unwrap_or_else(|_| "/etc/precrime/report.conf".into());
      let raw = fs::read_to_string(&config_path)
          .with_context(|| format!("reading config from {config_path}"))?;
      let cfg = ReportConfig::from_toml(&raw)
          .with_context(|| format!("parsing config from {config_path}"))?;

      tracing::info!(?cfg, "REPORT starting");
      Ok(())
  }
  ```

- [ ] **Step 6: Run the test, watch it pass**

  ```bash
  cargo test -p report --test config
  ```

  Expected: 3 passed.

- [ ] **Step 7: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/src/lib.rs report/src/config.rs report/src/main.rs report/tests/config.rs
  git commit -m "report: TOML config parser (TDD)"
  ```

---

### Task 5: Source-slot mapping (TDD)

**Files:**
- Create: `report/src/mapping.rs`
- Modify: `report/src/lib.rs` (re-export module)
- Create: `report/tests/mapping.rs`

- [ ] **Step 1: Write the failing test**

  Create `report/tests/mapping.rs`:

  ```rust
  use report::mapping::{assign_slots, MAX_SLOTS};
  use std::collections::HashMap;

  #[test]
  fn alphabetical_order_assigns_slots() {
      let sources = vec![
          "PRECOG-02-CCTV-DOOR".to_string(),
          "PRECOG-01-IPHONE-STAGE".to_string(),
      ];
      let mapping = assign_slots(&sources, &HashMap::new());
      assert_eq!(mapping.get("PRECOG-01-IPHONE-STAGE"), Some(&1));
      assert_eq!(mapping.get("PRECOG-02-CCTV-DOOR"), Some(&2));
  }

  #[test]
  fn overrides_pin_specific_sources() {
      let sources = vec![
          "PRECOG-A".to_string(),
          "PRECOG-B".to_string(),
          "PRECOG-C".to_string(),
      ];
      let mut overrides = HashMap::new();
      overrides.insert("PRECOG-C".to_string(), 1u8);
      let mapping = assign_slots(&sources, &overrides);
      assert_eq!(mapping.get("PRECOG-C"), Some(&1));
      assert_eq!(mapping.get("PRECOG-A"), Some(&2));
      assert_eq!(mapping.get("PRECOG-B"), Some(&3));
  }

  #[test]
  fn extras_beyond_max_slots_dropped() {
      let sources: Vec<String> = (1..=11).map(|i| format!("PRECOG-{i:02}")).collect();
      let mapping = assign_slots(&sources, &HashMap::new());
      assert_eq!(mapping.len(), usize::from(MAX_SLOTS));
      assert_eq!(mapping.get("PRECOG-01"), Some(&1));
      assert_eq!(mapping.get("PRECOG-09"), Some(&9));
      assert!(mapping.get("PRECOG-10").is_none());
  }

  #[test]
  fn empty_input_returns_empty_mapping() {
      assert!(assign_slots(&[], &HashMap::new()).is_empty());
  }
  ```

- [ ] **Step 2: Run, watch it fail**

  ```bash
  cargo test -p report --test mapping
  ```

  Expected: compile error — `mapping` module doesn't exist.

- [ ] **Step 3: Implement `report/src/mapping.rs`**

  ```rust
  //! Map discovered NDI source names to keyboard slots 1..=9.

  use std::collections::{BTreeSet, HashMap};

  pub const MAX_SLOTS: u8 = 9;

  /// Return `{source_name -> slot}`. Overrides pin names to specific slots;
  /// the remainder fill the lowest free slots alphabetically.
  ///
  /// Sources beyond the available slots (`MAX_SLOTS`) are dropped.
  pub fn assign_slots(
      sources: &[String],
      overrides: &HashMap<String, u8>,
  ) -> HashMap<String, u8> {
      let mut mapping = HashMap::new();
      let mut taken = BTreeSet::new();

      // Apply pinned overrides first.
      for (name, slot) in overrides {
          if !sources.iter().any(|s| s == name) {
              continue;
          }
          if *slot < 1 || *slot > MAX_SLOTS {
              continue;
          }
          if taken.insert(*slot) {
              mapping.insert(name.clone(), *slot);
          }
      }

      // Fill remaining sources alphabetically into the lowest free slots.
      let mut remaining: Vec<&String> = sources
          .iter()
          .filter(|s| !mapping.contains_key(*s))
          .collect();
      remaining.sort();

      let mut free_slot: u8 = 1;
      for name in remaining {
          while free_slot <= MAX_SLOTS && taken.contains(&free_slot) {
              free_slot += 1;
          }
          if free_slot > MAX_SLOTS {
              break;
          }
          mapping.insert(name.clone(), free_slot);
          taken.insert(free_slot);
          free_slot += 1;
      }

      mapping
  }
  ```

- [ ] **Step 4: Add the module to `report/src/lib.rs`**

  ```rust
  //! REPORT library — exposes modules for integration testing.
  pub mod config;
  pub mod mapping;
  ```

- [ ] **Step 5: Run, watch it pass**

  ```bash
  cargo test -p report --test mapping
  ```

  Expected: 4 passed.

- [ ] **Step 6: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/src/lib.rs report/src/mapping.rs report/tests/mapping.rs
  git commit -m "report: source-slot mapping (TDD)"
  ```

---

### Task 6: NDI source name filter helpers (TDD)

**Files:**
- Create: `report/src/naming.rs`
- Modify: `report/src/lib.rs`
- Create: `report/tests/naming.rs`

- [ ] **Step 1: Write the failing test**

  Create `report/tests/naming.rs`:

  ```rust
  use report::naming::{display_name, is_precog_source};

  #[test]
  fn accepts_precog_named_sources() {
      assert!(is_precog_source("PRECOG-01-IPHONE-STAGE (Channel 1)"));
      assert!(is_precog_source("PRECOG-02-CCTV-DOOR"));
  }

  #[test]
  fn rejects_non_precog_sources() {
      assert!(!is_precog_source("REPORT (Internal)"));
      assert!(!is_precog_source("Random Studio Source"));
      assert!(!is_precog_source(""));
  }

  #[test]
  fn strips_channel_suffix() {
      assert_eq!(
          display_name("PRECOG-01-IPHONE-STAGE (Channel 1)"),
          "PRECOG-01-IPHONE-STAGE"
      );
      assert_eq!(display_name("PRECOG-02-CCTV-DOOR"), "PRECOG-02-CCTV-DOOR");
  }
  ```

- [ ] **Step 2: Run, watch it fail**

  ```bash
  cargo test -p report --test naming
  ```

  Expected: compile error.

- [ ] **Step 3: Implement `report/src/naming.rs`**

  ```rust
  //! PRECOG NDI source-name helpers.

  pub fn is_precog_source(name: &str) -> bool {
      name.starts_with("PRECOG-")
  }

  /// Strip the trailing " (Channel N)" suffix that NDI Find returns.
  pub fn display_name(ndi_name: &str) -> &str {
      ndi_name.find(" (").map_or(ndi_name, |i| &ndi_name[..i])
  }
  ```

- [ ] **Step 4: Add the module to `report/src/lib.rs`**

  ```rust
  pub mod config;
  pub mod mapping;
  pub mod naming;
  ```

- [ ] **Step 5: Run, watch it pass**

  ```bash
  cargo test -p report --test naming
  ```

  Expected: 3 passed.

- [ ] **Step 6: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/src/lib.rs report/src/naming.rs report/tests/naming.rs
  git commit -m "report: NDI source name filter helpers (TDD)"
  ```

---

### Task 7: libndi FFI for source discovery

NDI Find is not exposed by `gstreamer1.0-plugins-rs`'s `ndisrc` element. We wrap `NDIlib_find_*` directly via `extern "C"`. Kept small and safe via a single `Discovery` type that owns the find handle.

**Files:**
- Create: `report/src/ndi_find.rs`
- Modify: `report/src/lib.rs`
- Modify: `report/Cargo.toml` (add `libc` dependency)

- [ ] **Step 1: Add `libc` to `report/Cargo.toml`**

  Append under `[dependencies]`:
  ```toml
  libc = "0.2"
  ```

- [ ] **Step 2: Create `report/src/ndi_find.rs`**

  ```rust
  //! Minimal FFI wrapper around libndi's NDIlib_find_* API for source discovery.
  //!
  //! We don't wrap the full NDI SDK — `gst-plugin-rs`'s `ndisrc` handles streaming.
  //! This module only enumerates sources visible on the LAN.

  use anyhow::{anyhow, Result};
  use std::ffi::CStr;
  use std::os::raw::{c_char, c_void};
  use std::time::Duration;

  type NdiFindInstance = *mut c_void;

  #[repr(C)]
  struct NdiSource {
      p_ndi_name: *const c_char,
      p_url_address: *const c_char,
  }

  // Linked dynamically — libndi.so must be in the library path at runtime.
  extern "C" {
      fn NDIlib_initialize() -> bool;
      fn NDIlib_find_create_v2(p_create: *const c_void) -> NdiFindInstance;
      fn NDIlib_find_destroy(p_instance: NdiFindInstance);
      fn NDIlib_find_wait_for_sources(p_instance: NdiFindInstance, timeout_ms: u32) -> bool;
      fn NDIlib_find_get_current_sources(
          p_instance: NdiFindInstance,
          p_no_sources: *mut u32,
      ) -> *const NdiSource;
  }

  pub struct Discovery {
      handle: NdiFindInstance,
  }

  // SAFETY: NDI Find instances are thread-safe per the SDK docs; we only use them
  // from a single discovery thread anyway.
  unsafe impl Send for Discovery {}

  impl Discovery {
      pub fn new() -> Result<Self> {
          // SAFETY: NDIlib_initialize is the first call into the NDI library;
          // safe to invoke once at process start.
          let ok = unsafe { NDIlib_initialize() };
          if !ok {
              return Err(anyhow!("NDIlib_initialize failed (no system CPU support?)"));
          }
          // SAFETY: passing null for default settings is documented in the NDI SDK.
          let handle = unsafe { NDIlib_find_create_v2(std::ptr::null()) };
          if handle.is_null() {
              return Err(anyhow!("NDIlib_find_create_v2 returned null"));
          }
          Ok(Self { handle })
      }

      /// Block up to `timeout` waiting for the source list to change. Returns the
      /// current source names afterward (regardless of whether they changed).
      pub fn poll(&self, timeout: Duration) -> Vec<String> {
          let ms: u32 = timeout.as_millis().min(u32::MAX as u128) as u32;
          // SAFETY: handle is non-null (constructed in `new`), timeout is a primitive.
          unsafe {
              NDIlib_find_wait_for_sources(self.handle, ms);
          }
          self.current_sources()
      }

      fn current_sources(&self) -> Vec<String> {
          let mut count: u32 = 0;
          // SAFETY: handle is non-null; out-pointer points to local `count`.
          let ptr = unsafe { NDIlib_find_get_current_sources(self.handle, &mut count) };
          if ptr.is_null() || count == 0 {
              return Vec::new();
          }
          let mut out = Vec::with_capacity(count as usize);
          for i in 0..count as usize {
              // SAFETY: NDI guarantees the array is valid for `count` entries
              // until the next call into the find API.
              let src = unsafe { &*ptr.add(i) };
              if src.p_ndi_name.is_null() {
                  continue;
              }
              // SAFETY: NDI provides null-terminated UTF-8.
              let cstr = unsafe { CStr::from_ptr(src.p_ndi_name) };
              if let Ok(s) = cstr.to_str() {
                  out.push(s.to_owned());
              }
          }
          out
      }
  }

  impl Drop for Discovery {
      fn drop(&mut self) {
          if !self.handle.is_null() {
              // SAFETY: handle was created by NDIlib_find_create_v2 and not freed yet.
              unsafe { NDIlib_find_destroy(self.handle) };
          }
      }
  }
  ```

- [ ] **Step 3: Add `ndi_find` to `report/src/lib.rs`**

  ```rust
  pub mod config;
  pub mod mapping;
  pub mod naming;
  pub mod ndi_find;
  ```

- [ ] **Step 4: Verify it compiles on the Pi (not macOS — libndi.so won't link on host)**

  On the Pi:
  ```bash
  cd ~/precrime    # after rsync; see Task 11 deploy steps
  cargo check -p report
  ```

  Expected: clean check. **If the link complains about `libndi.so`, ensure `/usr/local/lib` is in the dynamic linker's path** (already done via `ldconfig` in Task 2).

  We don't unit-test this module — it needs a live NDI environment. Verified at M5/M6.

- [ ] **Step 5: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/Cargo.toml report/src/lib.rs report/src/ndi_find.rs
  git commit -m "report: libndi FFI for source discovery"
  ```

---

### Task 8: GStreamer pipeline builders (program + preview + tally)

**Files:**
- Create: `report/src/pipeline.rs`
- Modify: `report/src/lib.rs`

- [ ] **Step 1: Implement `report/src/pipeline.rs`**

  ```rust
  //! GStreamer pipeline builders for REPORT.
  //!
  //! Two independent pipelines, each rendering directly to a DRM/KMS connector:
  //! - `program`: input-selector over N ndisrc inputs → kmssink (HDMI-A-1)
  //! - `preview`: compositor (2x2 or 3x3 grid) → cairooverlay tally → kmssink (HDMI-A-2)

  use anyhow::{Context, Result};
  use gstreamer::prelude::*;
  use gstreamer::{Element, Pipeline};
  use std::sync::Arc;

  pub struct ProgramPipeline {
      pub pipeline: Pipeline,
      pub selector: Element,
  }

  /// Build the program-out pipeline. Returns the pipeline and the `input-selector`
  /// element handle (used to switch sources at runtime).
  pub fn build_program(source_names: &[String], connector_id: u32) -> Result<ProgramPipeline> {
      let mut parts = String::from("input-selector name=sel");
      for (i, name) in source_names.iter().enumerate() {
          let escaped = escape_ndi_name(name);
          parts.push_str(&format!(
              r#" ndisrc ndi-name="{escaped}" ! ndisrcdemux name=d{i} d{i}.video ! queue max-size-buffers=4 leaky=downstream ! videoconvert ! sel.sink_{i}"#
          ));
      }
      parts.push_str(&format!(
          " sel. ! videoconvert ! kmssink connector-id={connector_id}"
      ));

      let pipeline = gstreamer::parse::launch(&parts)
          .context("parse program pipeline")?
          .downcast::<Pipeline>()
          .map_err(|_| anyhow::anyhow!("downcast to Pipeline"))?;
      let selector = pipeline
          .by_name("sel")
          .context("input-selector element missing")?;
      Ok(ProgramPipeline { pipeline, selector })
  }

  /// Switch the active input on the program pipeline's selector.
  pub fn select_slot(selector: &Element, slot_index: usize) -> Result<()> {
      let pad_name = format!("sink_{slot_index}");
      let pad = selector
          .static_pad(&pad_name)
          .with_context(|| format!("no pad {pad_name}"))?;
      selector.set_property("active-pad", &pad);
      Ok(())
  }

  /// Grid dimensions for N sources: (cols, rows).
  fn grid_for(n: usize) -> (usize, usize) {
      match n {
          0 | 1 => (1, 1),
          2..=4 => (2, 2),
          _ => (3, 3),
      }
  }

  pub struct PreviewPipeline {
      pub pipeline: Pipeline,
  }

  /// `tally_callback` receives the current "active slot" 1..=N (or None) every
  /// time the compositor draws a frame, and should set a red rectangle on the
  /// active tile via cairo.
  pub fn build_preview(
      source_names: &[String],
      connector_id: u32,
      get_active_slot: Arc<dyn Fn() -> Option<u8> + Send + Sync>,
  ) -> Result<PreviewPipeline> {
      if source_names.is_empty() {
          let s = format!(
              "videotestsrc pattern=black is-live=true ! video/x-raw,width=1920,height=1080 ! videoconvert ! kmssink connector-id={connector_id}"
          );
          let pipeline = gstreamer::parse::launch(&s)?
              .downcast::<Pipeline>()
              .map_err(|_| anyhow::anyhow!("downcast"))?;
          return Ok(PreviewPipeline { pipeline });
      }

      let n = source_names.len();
      let (cols, rows) = grid_for(n);
      let tile_w: u32 = 1920 / cols as u32;
      let tile_h: u32 = 1080 / rows as u32;

      let mut s = String::from("compositor name=mix background=black");
      for (i, _name) in source_names.iter().enumerate() {
          let col = (i % cols) as u32;
          let row = (i / cols) as u32;
          let x = col * tile_w;
          let y = row * tile_h;
          s.push_str(&format!(
              " sink_{i}::xpos={x} sink_{i}::ypos={y} sink_{i}::width={tile_w} sink_{i}::height={tile_h}"
          ));
      }
      for (i, name) in source_names.iter().enumerate() {
          let escaped = escape_ndi_name(name);
          s.push_str(&format!(
              r#" ndisrc ndi-name="{escaped}" ! ndisrcdemux name=pd{i} pd{i}.video ! queue max-size-buffers=4 leaky=downstream ! videoconvert ! videoscale ! video/x-raw,width={tile_w},height={tile_h} ! mix.sink_{i}"#
          ));
      }
      s.push_str(&format!(
          " mix. ! videoconvert ! cairooverlay name=tally ! videoconvert ! kmssink connector-id={connector_id}"
      ));

      let pipeline = gstreamer::parse::launch(&s)
          .context("parse preview pipeline")?
          .downcast::<Pipeline>()
          .map_err(|_| anyhow::anyhow!("downcast to Pipeline"))?;

      let overlay = pipeline
          .by_name("tally")
          .context("cairooverlay 'tally' missing")?;

      // Cache grid info for the draw callback.
      let cb = get_active_slot.clone();
      overlay.connect("draw", true, move |args| {
          // args: (overlay: Element, ctx: cairo::Context, timestamp: u64, duration: u64)
          let ctx = args[1]
              .get::<cairo::Context>()
              .expect("cairo context arg");
          if let Some(slot) = cb() {
              if (1..=n as u8).contains(&slot) {
                  let idx = (slot - 1) as u32;
                  let col = idx % cols as u32;
                  let row = idx / cols as u32;
                  let x = (col * tile_w) as f64;
                  let y = (row * tile_h) as f64;
                  ctx.set_source_rgba(1.0, 0.0, 0.0, 1.0);
                  ctx.set_line_width(8.0);
                  ctx.rectangle(x + 4.0, y + 4.0, (tile_w as f64) - 8.0, (tile_h as f64) - 8.0);
                  let _ = ctx.stroke();
              }
          }
          None
      });

      Ok(PreviewPipeline { pipeline })
  }

  fn escape_ndi_name(name: &str) -> String {
      // Replace any double-quote in source name with empty — NDI names never
      // contain quotes in our convention, but defend in depth.
      name.replace('"', "")
  }
  ```

- [ ] **Step 2: Add `pipeline` to `report/src/lib.rs`**

  ```rust
  pub mod config;
  pub mod mapping;
  pub mod naming;
  pub mod ndi_find;
  pub mod pipeline;
  ```

- [ ] **Step 3: Compile-check on the Pi**

  ```bash
  cargo check -p report
  ```

  Expected: clean check. (Compile-check on macOS may fail if `gstreamer1.0-plugins-rs` isn't installed — fine to defer to Pi-side check.)

- [ ] **Step 4: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/src/lib.rs report/src/pipeline.rs
  git commit -m "report: GStreamer pipeline builders (program + preview + tally)"
  ```

---

### Task 9: Keyboard input via evdev

**Files:**
- Create: `report/src/input.rs`
- Modify: `report/src/lib.rs`

- [ ] **Step 1: Implement `report/src/input.rs`**

  ```rust
  //! USB keyboard input via evdev. Yields slot numbers 1..=9 on key press.

  use anyhow::{Context, Result};
  use evdev::{Device, EventType, KeyCode};
  use std::path::Path;
  use std::sync::mpsc::Sender;

  pub fn run_keyboard_loop(
      device_path: impl AsRef<Path>,
      tx: Sender<u8>,
  ) -> Result<()> {
      let path = device_path.as_ref();
      let mut device = Device::open(path)
          .with_context(|| format!("opening evdev device {}", path.display()))?;

      loop {
          let events = device
              .fetch_events()
              .with_context(|| format!("fetch_events on {}", path.display()))?;
          for ev in events {
              if ev.event_type() != EventType::KEY {
                  continue;
              }
              if ev.value() != 1 {
                  // 1 = key down; 0 = up; 2 = repeat
                  continue;
              }
              if let Some(slot) = key_to_slot(KeyCode(ev.code())) {
                  let _ = tx.send(slot);
              }
          }
      }
  }

  fn key_to_slot(code: KeyCode) -> Option<u8> {
      match code {
          KeyCode::KEY_1 => Some(1),
          KeyCode::KEY_2 => Some(2),
          KeyCode::KEY_3 => Some(3),
          KeyCode::KEY_4 => Some(4),
          KeyCode::KEY_5 => Some(5),
          KeyCode::KEY_6 => Some(6),
          KeyCode::KEY_7 => Some(7),
          KeyCode::KEY_8 => Some(8),
          KeyCode::KEY_9 => Some(9),
          _ => None,
      }
  }
  ```

- [ ] **Step 2: Add `input` to `report/src/lib.rs`**

  ```rust
  pub mod config;
  pub mod input;
  pub mod mapping;
  pub mod naming;
  pub mod ndi_find;
  pub mod pipeline;
  ```

- [ ] **Step 3: Compile-check**

  ```bash
  cargo check -p report
  ```

- [ ] **Step 4: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/src/lib.rs report/src/input.rs
  git commit -m "report: evdev keyboard input loop"
  ```

---

### Task 10: Daemon orchestration

**Files:**
- Create: `report/src/daemon.rs`
- Modify: `report/src/lib.rs`
- Modify: `report/src/main.rs`

- [ ] **Step 1: Implement `report/src/daemon.rs`**

  ```rust
  //! REPORT daemon: owns pipelines, handles source-set changes and keypresses.

  use crate::config::ReportConfig;
  use crate::mapping::assign_slots;
  use crate::naming::{display_name, is_precog_source};
  use crate::ndi_find::Discovery;
  use crate::pipeline::{build_preview, build_program, select_slot, PreviewPipeline, ProgramPipeline};
  use anyhow::Result;
  use gstreamer::prelude::*;
  use parking_lot::Mutex;
  use std::collections::BTreeSet;
  use std::sync::mpsc::{channel, Receiver};
  use std::sync::Arc;
  use std::time::Duration;
  use tracing::{info, warn};

  pub struct Daemon {
      cfg: ReportConfig,
      state: Arc<Mutex<DaemonState>>,
  }

  struct DaemonState {
      sources_in_order: Vec<String>,
      active_slot: Option<u8>,
      program: Option<ProgramPipeline>,
      preview: Option<PreviewPipeline>,
  }

  impl Daemon {
      pub fn new(cfg: ReportConfig) -> Self {
          Self {
              cfg,
              state: Arc::new(Mutex::new(DaemonState {
                  sources_in_order: Vec::new(),
                  active_slot: None,
                  program: None,
                  preview: None,
              })),
          }
      }

      /// Run the daemon. Blocks the calling thread; sets up discovery + keyboard threads
      /// and a GLib mainloop for GStreamer.
      pub fn run(self) -> Result<()> {
          gstreamer::init()?;

          // Channels: discovery and keyboard each push into separate receivers.
          let (src_tx, src_rx) = channel::<Vec<String>>();
          let (key_tx, key_rx) = channel::<u8>();

          let discovery = Discovery::new()?;
          let _disc_handle = std::thread::Builder::new()
              .name("report-discovery".into())
              .spawn(move || {
                  let mut last = BTreeSet::<String>::new();
                  loop {
                      let raw = discovery.poll(Duration::from_secs(2));
                      let names: BTreeSet<String> = raw
                          .iter()
                          .filter(|n| is_precog_source(n))
                          .map(|n| display_name(n).to_owned())
                          .collect();
                      if names != last {
                          last = names.clone();
                          let ordered: Vec<String> = names.into_iter().collect();
                          if src_tx.send(ordered).is_err() {
                              break;
                          }
                      }
                  }
              })?;

          let kbd_device = self.cfg.keyboard_device.clone();
          let _kbd_handle = std::thread::Builder::new()
              .name("report-keyboard".into())
              .spawn(move || {
                  if let Err(e) = crate::input::run_keyboard_loop(&kbd_device, key_tx) {
                      warn!(error = ?e, "keyboard loop exited");
                  }
              })?;

          self.event_loop(src_rx, key_rx)
      }

      fn event_loop(&self, src_rx: Receiver<Vec<String>>, key_rx: Receiver<u8>) -> Result<()> {
          loop {
              // Select-like behavior: try both channels with small timeout.
              if let Ok(names) = src_rx.recv_timeout(Duration::from_millis(50)) {
                  self.on_sources_changed(&names)?;
              }
              while let Ok(slot) = key_rx.try_recv() {
                  self.handle_keypress(slot)?;
              }
          }
      }

      fn on_sources_changed(&self, raw_sources: &[String]) -> Result<()> {
          let mapping = assign_slots(raw_sources, &self.cfg.source_slot_overrides);
          // Build slot-ordered list.
          let max = mapping.values().copied().max().unwrap_or(0);
          let mut ordered: Vec<Option<String>> = vec![None; max as usize];
          for (name, slot) in &mapping {
              ordered[(*slot - 1) as usize] = Some(name.clone());
          }
          let new_sources: Vec<String> = ordered.into_iter().flatten().collect();

          let mut st = self.state.lock();
          if new_sources == st.sources_in_order {
              return Ok(());
          }
          info!(new = ?new_sources, "sources changed");

          if let Some(p) = st.program.take() {
              let _ = p.pipeline.set_state(gstreamer::State::Null);
          }
          if let Some(p) = st.preview.take() {
              let _ = p.pipeline.set_state(gstreamer::State::Null);
          }
          st.sources_in_order = new_sources.clone();

          // Build new preview with tally callback closing over state.
          let state_for_tally = self.state.clone();
          let preview = build_preview(
              &new_sources,
              self.cfg.preview_connector_id,
              Arc::new(move || state_for_tally.lock().active_slot),
          )?;
          preview.pipeline.set_state(gstreamer::State::Playing)?;
          st.preview = Some(preview);

          if new_sources.is_empty() {
              st.active_slot = None;
              st.program = None;
              return Ok(());
          }

          let program = build_program(&new_sources, self.cfg.program_connector_id)?;
          program.pipeline.set_state(gstreamer::State::Playing)?;
          // Default-cut to slot 1 on source set change.
          let _ = select_slot(&program.selector, 0);
          st.active_slot = Some(1);
          st.program = Some(program);
          Ok(())
      }

      fn handle_keypress(&self, slot: u8) -> Result<()> {
          let st = self.state.lock();
          let Some(program) = st.program.as_ref() else {
              return Ok(());
          };
          if (slot as usize) > st.sources_in_order.len() || slot == 0 {
              return Ok(());
          }
          info!(
              slot,
              source = %st.sources_in_order[(slot - 1) as usize],
              "cut"
          );
          drop(st);
          // Take a fresh lock to mutate active_slot.
          {
              let mut st = self.state.lock();
              st.active_slot = Some(slot);
          }
          let st = self.state.lock();
          if let Some(program) = st.program.as_ref() {
              select_slot(&program.selector, (slot - 1) as usize)?;
          }
          Ok(())
      }
  }
  ```

- [ ] **Step 2: Add `daemon` to `report/src/lib.rs`**

  ```rust
  pub mod config;
  pub mod daemon;
  pub mod input;
  pub mod mapping;
  pub mod naming;
  pub mod ndi_find;
  pub mod pipeline;
  ```

- [ ] **Step 3: Update `report/src/main.rs`**

  ```rust
  //! REPORT — PRECRIME switcher daemon entry point.

  use anyhow::{Context, Result};
  use report::config::ReportConfig;
  use report::daemon::Daemon;
  use std::env;
  use std::fs;

  fn main() -> Result<()> {
      init_tracing();

      let config_path =
          env::var("REPORT_CONFIG").unwrap_or_else(|_| "/etc/precrime/report.conf".into());
      let raw = fs::read_to_string(&config_path)
          .with_context(|| format!("reading config from {config_path}"))?;
      let cfg = ReportConfig::from_toml(&raw)
          .with_context(|| format!("parsing config from {config_path}"))?;

      tracing::info!(?cfg, "REPORT starting");
      Daemon::new(cfg).run()
  }

  fn init_tracing() {
      use tracing_subscriber::{fmt, EnvFilter};
      let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
      // Try journald first; fall back to stdout for local dev.
      match tracing_journald::layer() {
          Ok(layer) => {
              use tracing_subscriber::prelude::*;
              tracing_subscriber::registry()
                  .with(env_filter)
                  .with(layer)
                  .init();
          }
          Err(_) => {
              fmt().with_env_filter(env_filter).init();
          }
      }
  }
  ```

- [ ] **Step 4: Compile-check on the Pi**

  ```bash
  cargo check -p report
  ```

- [ ] **Step 5: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/src/lib.rs report/src/daemon.rs report/src/main.rs
  git commit -m "report: daemon orchestration + journald-aware main"
  ```

---

### Task 11: M5 — single source → HDMI smoke test

**Files:**
- Create: `report/report.conf.example`
- Modify: `report/runbook.md`

- [ ] **Step 1: Create `report/report.conf.example`**

  ```toml
  # /etc/precrime/report.conf — copy and edit per deployment.

  program_connector_id = 32   # set per Task 2 Step 5 output
  preview_connector_id = 34
  keyboard_device = "/dev/input/event0"

  # Optional: pin specific PRECOG names to specific number keys.
  # Sources not listed here fall into the remaining slots alphabetically.
  # [source_slot_overrides]
  # "PRECOG-01-IPHONE-STAGE" = 1
  # "PRECOG-02-CCTV-DOOR" = 2
  ```

- [ ] **Step 2: Build the binary on the Pi**

  ```bash
  ssh cody@192.168.50.10
  cd ~
  # Assume the repo was rsynced into ~/precrime — see deploy command below.
  rsync -av --delete --exclude target/ /Users/cody/Dev/precrime/ cody@192.168.50.10:/home/cody/precrime/
  ssh cody@192.168.50.10 'cd ~/precrime && cargo build --release -p report'
  ```

  Expected: clean release build, binary at `~/precrime/target/release/report`. First build takes ~15-30 min on Pi 5; incremental builds <1 min.

- [ ] **Step 3: Install the config**

  ```bash
  ssh cody@192.168.50.10 '
      sudo mkdir -p /etc/precrime &&
      sudo cp ~/precrime/report/report.conf.example /etc/precrime/report.conf
  '
  ```

  Edit `/etc/precrime/report.conf` with your actual connector IDs (from Task 2 Step 5) and keyboard device path. Find the USB keyboard:
  ```bash
  ls /dev/input/by-id/ | grep -i kbd
  ```

- [ ] **Step 4: Run the binary manually**

  Ensure at least one PRECOG is live on the LAN (iPhone or CCTV Pi). Then:

  ```bash
  ssh cody@192.168.50.10 '
      sudo RUST_LOG=info ~/precrime/target/release/report
  '
  ```

  Expected:
  - Logs show "REPORT starting" and "sources changed: [...PRECOG-...]" within ~5 seconds.
  - HDMI-A-1 (if plugged in) shows the lowest-slot PRECOG as program out.
  - HDMI-A-2 (if plugged in) shows the multiview with red tally on slot 1.
  - Number keys on the USB keyboard switch program out.

  **M5, M6, M7, M8 all pass together with this run** if everything works. The Rust binary integrates the program, preview, tally, and switching logic in a single coherent daemon — once the pipeline compiles and `ndisrc` finds sources, all four milestones light up at once.

- [ ] **Step 5: Append the M5–M8 verification to runbook**

  Append to `report/runbook.md`:

  ```markdown
  ## M5-M8 verification — YYYY-MM-DD
  - Rust REPORT daemon built and run on Pi 5
  - At least one PRECOG discovered via libndi NDI Find
  - Program out on HDMI-A-1 live
  - Multiview on HDMI-A-2 live with grid + red tally on active slot
  - Number keys 1..N switch sources cleanly
  - Switch latency observed: <ballpark, e.g., ~80ms>
  ```

- [ ] **Step 6: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/report.conf.example report/runbook.md
  git commit -m "report: M5-M8 verified, full daemon working on Pi"
  ```

---

### Task 12: systemd service + boot survival (M10)

**Files:**
- Create: `report/report.service`
- Modify: `report/runbook.md`

- [ ] **Step 1: Create `report/report.service`**

  ```ini
  [Unit]
  Description=PRECRIME REPORT switcher daemon
  After=network-online.target
  Wants=network-online.target

  [Service]
  Type=simple
  ExecStart=/usr/local/bin/report
  Environment=REPORT_CONFIG=/etc/precrime/report.conf
  Environment=RUST_LOG=info
  Restart=on-failure
  RestartSec=3
  StandardOutput=journal
  StandardError=journal
  User=root

  [Install]
  WantedBy=multi-user.target
  ```

- [ ] **Step 2: Deploy binary + service to the Pi**

  ```bash
  ssh cody@192.168.50.10 '
      sudo cp ~/precrime/target/release/report /usr/local/bin/report &&
      sudo cp ~/precrime/report/report.service /etc/systemd/system/report.service &&
      sudo systemctl daemon-reload &&
      sudo systemctl enable report.service &&
      sudo systemctl start report.service
  '
  ```

- [ ] **Step 3: Check status**

  ```bash
  ssh cody@192.168.50.10 'sudo systemctl status report.service'
  ```

  Expected: `active (running)`. If failed:
  ```bash
  ssh cody@192.168.50.10 'sudo journalctl -u report.service -n 80 --no-pager'
  ```

- [ ] **Step 4: Reboot and verify M10**

  ```bash
  ssh cody@192.168.50.10 'sudo reboot'
  ```

  Wait ~45 seconds. Both HDMIs come live without intervention. New PRECOGs joining the LAN are picked up by the running daemon.

- [ ] **Step 5: Append M10 to runbook**

  ```markdown
  ## M10 verification — YYYY-MM-DD
  - report.service enabled and autostarts at boot
  - Cold-boot → both HDMI outputs live within: <observed seconds>
  - Survives PRECOG disconnect/reconnect cleanly
  ```

- [ ] **Step 6: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/report.service report/runbook.md
  git commit -m "report: systemd service + M10 boot-survival verified"
  ```

---

### Task 13: Integration test with two real PRECOGs (M9)

- [ ] **Step 1: Bring up the full Phase 1 system**

  Router + iPhone PRECOG + CCTV Pi PRECOG + REPORT + USB keyboard + two displays on HDMI-A-1/A-2.

- [ ] **Step 2: Operator experience check**

  - Multiview shows both sources side-by-side; red tally on whichever is live.
  - Press `1`: program cuts to PRECOG-01 (iPhone).
  - Press `2`: program cuts to PRECOG-02 (CCTV).
  - Live motion appears on HDMI-A-1 with <300ms latency.

- [ ] **Step 3: Stress — disconnect/reconnect a PRECOG**

  - Power off PRECOG-02. Within ~5s the daemon logs "sources changed", the multiview rebuilds without that tile, program auto-cuts to slot 1 during the rebuild (~1-2s glitch).
  - Power PRECOG-02 back on. ~30s later the multiview regrows; press `2` to cut to it again.

- [ ] **Step 4: Append M9 to runbook**

  ```markdown
  ## M9 verification — YYYY-MM-DD
  - Two PRECOGs + REPORT + router running together
  - Keyboard switching cuts cleanly between sources
  - Source disconnect/reconnect: daemon rebuilds, auto-cuts to slot 1, ~1-2s glitch on rebuild
  - Deferred: crossfade-on-loss + speaker beep on involuntary cuts (system spec §3 Failure Modes)
  ```

- [ ] **Step 5: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/runbook.md
  git commit -m "report: M9 integration verified, full two-PRECOG run"
  ```

---

### Task 14: Show-day runbook (M11)

- [ ] **Step 1: Append show-day procedures to `report/runbook.md`**

  ```markdown
  ## Show-day startup order

  1. Power on router (Flint 2). Wait ~30s for WiFi.
  2. Power on REPORT (wired Pi 5). Wait ~45s for `report.service` to come up.
  3. Power on PRECOGs:
     - For iPhone: launch NDI HX Camera, tap Start, place on mount.
     - For CCTV Pi: connect EasyCap + CCTV cam, plug in PD power, wait ~45s.
  4. Confirm multiview on HDMI-A-2 shows all expected PRECOGs with red tally on slot 1.
  5. Plug HDMI-A-1 into your stream encoder / projector.
  6. Switch sources via number keys on the REPORT USB keyboard.

  ## Show-day tear-down

  1. Stop NDI HX Camera on iPhone, unplug.
  2. `ssh cody@precog-NN-... 'sudo shutdown -h now'`, wait for green LED to stop.
  3. `ssh cody@192.168.50.10 'sudo shutdown -h now'`. Wait for green LED to stop.
  4. Power off router.
  5. Coil and pack cables.

  ## If things go wrong mid-show

  - **HDMI-A-1 goes black:** `ssh cody@192.168.50.10 'sudo systemctl restart report.service'` from phone.
  - **A PRECOG drops:** the tile vanishes; switch to a different source with the keyboard. Debug post-show.
  - **Both HDMIs go black but daemon is running:** `sudo systemctl restart report.service`.
  - **Router locks up:** power-cycle the Flint 2. Cams + REPORT reconnect on their own within ~30s.
  - **`journalctl -u report.service -f`** is your friend.
  ```

- [ ] **Step 2: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/runbook.md
  git commit -m "report: M11 show-day runbook, Phase 1 complete"
  ```

---

## File Structure Summary

```
Cargo.toml                          # workspace
.gitignore
report/
├── Cargo.toml
├── src/
│   ├── lib.rs                      # module roster
│   ├── main.rs                     # entry point
│   ├── config.rs                   # TOML config (TDD)
│   ├── mapping.rs                  # source → slot assignment (TDD)
│   ├── naming.rs                   # PRECOG- filter + display_name (TDD)
│   ├── ndi_find.rs                 # libndi FFI for discovery
│   ├── pipeline.rs                 # GStreamer pipeline builders
│   ├── input.rs                    # evdev keyboard loop
│   └── daemon.rs                   # orchestration
├── tests/
│   ├── config.rs
│   ├── mapping.rs
│   └── naming.rs
├── install.sh                      # apt + rustup installer (run on Pi)
├── report.conf.example             # /etc/precrime/report.conf template
├── report.service                  # systemd unit
└── runbook.md                      # operator runbook
```

`cargo test -p report` runs the pure-Rust tests on macOS or Pi. The GStreamer-bound modules (`pipeline`, `input`, `daemon`, `ndi_find`) are verified at M5-M11 on the Pi.

## Done means

- All milestones M5 through M11 verified per runbook
- `report.service` runs on boot, survives reboot, handles cam disconnect/reconnect
- Two-PRECOG integration test passes
- Show-day procedure documented
- Phase 1 of PRECRIME is complete
