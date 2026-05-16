# REPORT (Switcher) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build REPORT, a headless Pi 5 NDI switcher that ingests N discovered PRECOG sources from the LAN, outputs a single chosen source as program video on HDMI-A-1, and outputs a multiview preview (with tally overlay) on HDMI-A-2. Source selection is driven by a USB keyboard for Phase 1 (MEZZANINE hardware controller comes in Phase 2 with no software changes — it appears as a USB HID keyboard).

**Architecture:** Two independent GStreamer pipelines, each rendering directly to a DRM/KMS connector via `kmssink` (no X11, no Wayland, no desktop env). A single Python daemon owns both pipelines, runs an NDI discovery loop using `libndi` via `gst-plugin-rs`'s `ndisrc`, maintains a deterministic source-name → keyboard slot mapping, and listens for keypresses via `python-evdev`. The daemon is wrapped in `systemd` with auto-restart.

**Tech Stack:** Raspberry Pi OS 12 Lite (64-bit), GStreamer 1.22+, `gstreamer1.0-plugins-rs` (provides `ndisrc` + `ndisinkcombiner`), NewTek NDI SDK runtime, Python 3.11+, `pygobject` (Python GStreamer bindings), `python-evdev`, `pytest` for unit tests, `systemd`.

**Milestones covered:** M5, M6, M7, M8 from system spec.

**Depends on:** Network Brain plan complete (M1). At least one PRECOG plan complete and a source live on the LAN, for end-to-end tests.

---

### Task 1: Flash Pi OS Lite and headless bring-up

**Files:**
- Create: `report/runbook.md`

- [ ] **Step 1: Use Raspberry Pi Imager to flash Pi OS Lite 64-bit to a 64GB microSD**

  Advanced settings:
  - Hostname: `report`
  - SSH: enabled with password auth
  - Username: `cody`, strong password (record in password manager)
  - WiFi: skip — REPORT is wired
  - Locale set per your region

- [ ] **Step 2: Insert microSD, attach active cooler, plug REPORT into the router via Cat6 to a LAN port**

  Power on.

- [ ] **Step 3: SSH from your laptop**

  ```bash
  ssh cody@report.local
  ```

  If `.local` fails, find the IP in the router's client list at `http://192.168.50.1`.

- [ ] **Step 4: Set a static DHCP reservation for REPORT at `.10`**

  ```bash
  ssh root@192.168.50.1
  uci add dhcp host
  uci set dhcp.@host[-1].name='report'
  uci set dhcp.@host[-1].mac='<REPORT_ETHERNET_MAC>'   # get from `ip link show eth0` on REPORT
  uci set dhcp.@host[-1].ip='192.168.50.10'
  uci commit dhcp
  /etc/init.d/dnsmasq restart
  ```

  Reboot REPORT to pick up the new IP:
  ```bash
  ssh cody@report.local 'sudo reboot'
  ```

- [ ] **Step 5: Append to `network/runbook.md`'s DHCP reservation table**

  Add a row:
  ```
  | report | <mac> | 192.168.50.10 |
  ```

- [ ] **Step 6: Update the system**

  ```bash
  ssh cody@192.168.50.10
  sudo apt update && sudo apt full-upgrade -y
  sudo reboot
  ```

- [ ] **Step 7: Create the REPORT runbook stub**

  Create `report/runbook.md`:

  ```markdown
  # REPORT — Switcher Runbook

  ## Identity
  - Hostname: `report`
  - Wired IP: `192.168.50.10`
  - Hardware: Pi 5 8GB + active cooler + dual HDMI

  ## Setup history
  - 2026-05-16: Initial provision per report-switcher plan
  ```

- [ ] **Step 8: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add network/runbook.md report/runbook.md
  git commit -m "report: initial Pi 5 runbook, static IP .10"
  ```

---

### Task 2: Install dependencies on REPORT

**Files:**
- Create: `report/install.sh`

- [ ] **Step 1: Install GStreamer, libndi prerequisites, Python tooling**

  ```bash
  ssh cody@192.168.50.10
  sudo apt install -y \
      gstreamer1.0-tools \
      gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad \
      gstreamer1.0-plugins-ugly \
      gstreamer1.0-plugins-rs \
      python3-gi \
      python3-gi-cairo \
      python3-cairo \
      python3-evdev \
      python3-pytest \
      python3-venv \
      v4l-utils \
      curl
  ```

- [ ] **Step 2: Install the NDI SDK runtime (same as PRECOG Kit A Task 2 Step 2)**

  Download the NDI SDK for Linux ARM (`aarch64`) from `https://ndi.video/sdk/`, copy `libndi.so*` into `/usr/local/lib/`, run `sudo ldconfig`.

- [ ] **Step 3: Verify `ndisrc` and `kmssink` elements load**

  ```bash
  gst-inspect-1.0 ndisrc | head -20
  gst-inspect-1.0 kmssink | head -20
  ```

  Expected: both inspect cleanly. **If `ndisrc` errors, libndi did not load — re-run Step 2 and check `ldconfig -p | grep libndi`.**

- [ ] **Step 4: Identify the DRM connector IDs for HDMI-A-1 and HDMI-A-2**

  ```bash
  for f in /sys/class/drm/card*-HDMI-A-*/status; do
      echo "$f -> $(cat $f)"
  done
  ```

  Expected: two lines, one per HDMI port. If a port has nothing plugged in, status will be `disconnected`. Plug a known HDMI display into each port one at a time and rerun to identify which port maps to which connector ID.

  Then read the connector IDs:
  ```bash
  for f in /sys/class/drm/card*-HDMI-A-*/connector_id; do
      echo "$(dirname $f) -> $(cat $f)"
  done
  ```

  Record the two numeric IDs.

- [ ] **Step 5: Create `report/install.sh` for reproducibility**

  ```bash
  #!/bin/sh
  set -e
  echo "Installing GStreamer + Rust plugins + Python tooling..."
  sudo apt update
  sudo apt install -y \
      gstreamer1.0-tools \
      gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good \
      gstreamer1.0-plugins-bad \
      gstreamer1.0-plugins-ugly \
      gstreamer1.0-plugins-rs \
      python3-gi \
      python3-gi-cairo \
      python3-cairo \
      python3-evdev \
      python3-pytest \
      python3-venv \
      v4l-utils \
      curl
  echo ""
  echo "Next: manually install NDI SDK runtime libndi.so per the runbook."
  echo "Then run: gst-inspect-1.0 ndisrc"
  ```

  ```bash
  chmod +x report/install.sh
  ```

- [ ] **Step 6: Append connector IDs to the runbook**

  Append to `report/runbook.md`:
  ```markdown
  ## DRM Connectors
  - HDMI-A-1 (program out): connector_id = <ID_1>
  - HDMI-A-2 (multiview):   connector_id = <ID_2>
  ```

- [ ] **Step 7: Commit**

  ```bash
  git add report/install.sh report/runbook.md
  git commit -m "report: install script + DRM connector IDs identified"
  ```

---

### Task 3: Project structure and config parser (TDD)

The config parser is the only piece of REPORT cleanly testable as a pure function. Building it first establishes the project's Python skeleton.

**Files:**
- Create: `report/pyproject.toml`
- Create: `report/report/__init__.py`
- Create: `report/report/config.py`
- Create: `report/tests/__init__.py`
- Create: `report/tests/test_config.py`

- [ ] **Step 1: Create `report/pyproject.toml`**

  ```toml
  [project]
  name = "report"
  version = "0.1.0"
  description = "PRECRIME REPORT: headless NDI switcher daemon"
  requires-python = ">=3.11"

  [tool.pytest.ini_options]
  testpaths = ["tests"]
  ```

- [ ] **Step 2: Create empty `report/report/__init__.py` and `report/tests/__init__.py`**

  ```bash
  mkdir -p report/report report/tests
  touch report/report/__init__.py report/tests/__init__.py
  ```

- [ ] **Step 3: Write the failing test for config parsing**

  Create `report/tests/test_config.py`:

  ```python
  from report.config import ReportConfig, parse_config


  def test_parse_minimal_config():
      raw = """
      program_connector_id = 32
      preview_connector_id = 34
      keyboard_device = "/dev/input/event0"
      """
      cfg = parse_config(raw)
      assert isinstance(cfg, ReportConfig)
      assert cfg.program_connector_id == 32
      assert cfg.preview_connector_id == 34
      assert cfg.keyboard_device == "/dev/input/event0"


  def test_parse_with_source_overrides():
      raw = """
      program_connector_id = 32
      preview_connector_id = 34
      keyboard_device = "/dev/input/event0"

      [source_slot_overrides]
      "PRECOG-01-IPHONE-STAGE" = 1
      "PRECOG-02-CCTV-DOOR" = 2
      """
      cfg = parse_config(raw)
      assert cfg.source_slot_overrides == {
          "PRECOG-01-IPHONE-STAGE": 1,
          "PRECOG-02-CCTV-DOOR": 2,
      }


  def test_missing_required_field_raises():
      raw = """
      preview_connector_id = 34
      keyboard_device = "/dev/input/event0"
      """
      import pytest
      with pytest.raises(KeyError):
          parse_config(raw)
  ```

- [ ] **Step 4: Run the tests, watch them fail**

  ```bash
  cd report
  python -m pytest tests/test_config.py -v
  ```

  Expected: ImportError, `report.config` doesn't exist yet.

- [ ] **Step 5: Implement `report/report/config.py`**

  ```python
  """Config file parsing for REPORT. TOML format."""

  from dataclasses import dataclass, field
  import tomllib


  @dataclass
  class ReportConfig:
      program_connector_id: int
      preview_connector_id: int
      keyboard_device: str
      source_slot_overrides: dict[str, int] = field(default_factory=dict)


  def parse_config(raw: str) -> ReportConfig:
      data = tomllib.loads(raw)
      return ReportConfig(
          program_connector_id=data["program_connector_id"],
          preview_connector_id=data["preview_connector_id"],
          keyboard_device=data["keyboard_device"],
          source_slot_overrides=data.get("source_slot_overrides", {}),
      )
  ```

- [ ] **Step 6: Run the tests again, watch them pass**

  ```bash
  python -m pytest tests/test_config.py -v
  ```

  Expected: 3 passed.

- [ ] **Step 7: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/pyproject.toml report/report/ report/tests/
  git commit -m "report: project skeleton + config parser (TDD)"
  ```

---

### Task 4: Source-slot mapping function (TDD)

A pure function that takes a list of discovered NDI source names and returns a stable name→slot mapping (slots numbered 1..N). Used by the daemon to bind sources to keyboard digits deterministically.

**Files:**
- Create: `report/report/mapping.py`
- Create: `report/tests/test_mapping.py`

- [ ] **Step 1: Write the failing test**

  Create `report/tests/test_mapping.py`:

  ```python
  from report.mapping import assign_slots


  def test_alphabetical_order_assigns_slots():
      sources = ["PRECOG-02-CCTV-DOOR", "PRECOG-01-IPHONE-STAGE"]
      mapping = assign_slots(sources, overrides={})
      assert mapping == {"PRECOG-01-IPHONE-STAGE": 1, "PRECOG-02-CCTV-DOOR": 2}


  def test_overrides_pin_specific_sources():
      sources = ["PRECOG-A", "PRECOG-B", "PRECOG-C"]
      overrides = {"PRECOG-C": 1}
      mapping = assign_slots(sources, overrides=overrides)
      assert mapping["PRECOG-C"] == 1
      # Remaining sources fill the next free slots alphabetically.
      assert mapping["PRECOG-A"] == 2
      assert mapping["PRECOG-B"] == 3


  def test_more_than_nine_sources_drops_extras():
      sources = [f"PRECOG-{i:02d}" for i in range(1, 12)]   # 11 sources
      mapping = assign_slots(sources, overrides={})
      # Only the first 9 fit on number keys 1..9.
      assert len(mapping) == 9
      assert mapping["PRECOG-01"] == 1
      assert mapping["PRECOG-09"] == 9
      assert "PRECOG-10" not in mapping


  def test_empty_input_returns_empty_mapping():
      assert assign_slots([], overrides={}) == {}
  ```

- [ ] **Step 2: Run, watch it fail**

  ```bash
  cd report
  python -m pytest tests/test_mapping.py -v
  ```

  Expected: ImportError.

- [ ] **Step 3: Implement `report/report/mapping.py`**

  ```python
  """Map discovered NDI source names to keyboard slots 1..9."""

  MAX_SLOTS = 9


  def assign_slots(sources: list[str], overrides: dict[str, int]) -> dict[str, int]:
      """Return {source_name: slot_number}. Overrides pin names to slots; the rest fill alphabetically."""
      mapping: dict[str, int] = {}
      taken_slots: set[int] = set()

      # Apply pinned overrides first.
      for name, slot in overrides.items():
          if name in sources and 1 <= slot <= MAX_SLOTS and slot not in taken_slots:
              mapping[name] = slot
              taken_slots.add(slot)

      # Fill remaining sources alphabetically into the lowest free slots.
      remaining = sorted(s for s in sources if s not in mapping)
      free_slots = (s for s in range(1, MAX_SLOTS + 1) if s not in taken_slots)
      for name, slot in zip(remaining, free_slots):
          mapping[name] = slot

      return mapping
  ```

- [ ] **Step 4: Run, watch it pass**

  ```bash
  python -m pytest tests/test_mapping.py -v
  ```

  Expected: 4 passed.

- [ ] **Step 5: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/report/mapping.py report/tests/test_mapping.py
  git commit -m "report: source-slot mapping (TDD)"
  ```

---

### Task 5: M5 — single NDI source → single HDMI out

This is the first system-level milestone. No daemon yet, no config, no keyboard — just prove the receiving end of the pipeline works.

**Files:**
- Modify: `report/runbook.md`

- [ ] **Step 1: Make sure at least one PRECOG is live on the network**

  Either the iPhone PRECOG (Kit B) or the CCTV PRECOG (Kit A). Verify with NDI Studio Monitor from your laptop.

- [ ] **Step 2: On REPORT, with HDMI-A-1 plugged into a display, run a one-shot pipeline**

  ```bash
  ssh cody@192.168.50.10
  sudo gst-launch-1.0 -v \
      ndisrc ndi-name="PRECOG-01-IPHONE-STAGE" \
      ! ndisrcdemux name=d \
      d.video ! queue ! videoconvert \
      ! kmssink connector-id=<HDMI-A-1_CONNECTOR_ID>
  ```

  Substitute your actual NDI source name and connector ID. `sudo` because `kmssink` needs DRM permission outside an X session; we'll fix this with a service user later.

  Expected: the connected HDMI display shows the live iPhone (or CCTV) feed within ~5 seconds.

  **If you see "Could not open device":** the Pi already has a console session holding the framebuffer. Switch to a different VT or boot with `console=tty3` to free `/dev/dri/card0`. The systemd version in Task 9 will handle this cleanly.

  **If you see colored noise or a black screen with audio:** the demuxer may not be pulling the video pad correctly. Try the simpler form:
  ```bash
  sudo gst-launch-1.0 -v \
      ndisrc ndi-name="..." ! ndisrcdemux name=d \
      d.video ! videoconvert ! kmssink connector-id=<ID>
  ```

- [ ] **Step 3: Stop the pipeline (Ctrl-C) and append M5 to the runbook**

  Append to `report/runbook.md`:

  ```markdown
  ## M5 verification — YYYY-MM-DD
  - Single NDI source rendered to HDMI-A-1 via headless GStreamer + kmssink
  - Source used: <name>
  - Latency observed: <ballpark, e.g., ~120ms>
  - Notes: <any quirks observed>
  ```

- [ ] **Step 4: Commit**

  ```bash
  git add report/runbook.md
  git commit -m "report: M5 verified, single NDI → HDMI via kmssink"
  ```

---

### Task 6: M6 — multi-source input-selector with keyboard switching

This is where REPORT becomes a switcher. Introduces the Python daemon.

**Files:**
- Create: `report/report/pipeline.py`
- Create: `report/report/input.py`
- Create: `report/report/__main__.py`
- Modify: `report/runbook.md`

- [ ] **Step 1: Make sure two PRECOGs are live on the network**

  iPhone PRECOG + CCTV PRECOG (or two iPhones, two test patterns, etc).

- [ ] **Step 2: Create `report/report/pipeline.py` with the program pipeline builder**

  ```python
  """GStreamer pipeline construction for REPORT."""

  import gi
  gi.require_version("Gst", "1.0")
  from gi.repository import Gst


  def build_program_pipeline(source_names: list[str], connector_id: int) -> tuple[Gst.Pipeline, Gst.Element]:
      """Build the program-out pipeline. Returns (pipeline, input_selector_element)."""
      pieces = ["input-selector name=sel"]
      for i, name in enumerate(source_names):
          # Each source: ndisrc -> ndisrcdemux -> queue -> sel.sink_i
          pieces.append(
              f'ndisrc ndi-name="{name}" ! ndisrcdemux name=d{i} '
              f'd{i}.video ! queue max-size-buffers=4 leaky=downstream ! videoconvert ! sel.sink_{i}'
          )
      pieces.append(
          f"sel. ! videoconvert ! kmssink connector-id={connector_id}"
      )
      pipeline_str = " ".join(pieces)
      pipeline = Gst.parse_launch(pipeline_str)
      selector = pipeline.get_by_name("sel")
      return pipeline, selector


  def select_slot(selector: Gst.Element, slot_index: int) -> None:
      """Switch the input-selector to the given sink pad index (0-based)."""
      pad = selector.get_static_pad(f"sink_{slot_index}")
      if pad is None:
          return
      selector.set_property("active-pad", pad)
  ```

- [ ] **Step 3: Create `report/report/input.py` for keyboard event handling**

  ```python
  """USB keyboard event reading via evdev."""

  import evdev
  from collections.abc import Iterator


  KEY_TO_SLOT = {
      evdev.ecodes.KEY_1: 1,
      evdev.ecodes.KEY_2: 2,
      evdev.ecodes.KEY_3: 3,
      evdev.ecodes.KEY_4: 4,
      evdev.ecodes.KEY_5: 5,
      evdev.ecodes.KEY_6: 6,
      evdev.ecodes.KEY_7: 7,
      evdev.ecodes.KEY_8: 8,
      evdev.ecodes.KEY_9: 9,
  }


  def slot_keypresses(device_path: str) -> Iterator[int]:
      """Yield slot numbers (1..9) for each number-key press on the device."""
      device = evdev.InputDevice(device_path)
      for event in device.read_loop():
          if event.type != evdev.ecodes.EV_KEY:
              continue
          if event.value != 1:           # 1 = key down, 2 = repeat, 0 = key up
              continue
          slot = KEY_TO_SLOT.get(event.code)
          if slot is not None:
              yield slot
  ```

- [ ] **Step 4: Create `report/report/__main__.py` as the entry point**

  ```python
  """REPORT daemon entry point. Phase 1: static source list from config or env."""

  import os
  import sys
  import threading
  import gi
  gi.require_version("Gst", "1.0")
  from gi.repository import Gst, GLib

  from report.config import parse_config
  from report.pipeline import build_program_pipeline, select_slot
  from report.input import slot_keypresses


  def main() -> int:
      Gst.init(None)

      config_path = os.environ.get("REPORT_CONFIG", "/etc/precrime/report.conf")
      with open(config_path) as f:
          cfg = parse_config(f.read())

      # Phase 1: source list comes from env var REPORT_SOURCES (comma-separated).
      # Phase 1.5 (Task 7) replaces this with NDI discovery.
      sources_env = os.environ.get("REPORT_SOURCES", "")
      sources = [s.strip() for s in sources_env.split(",") if s.strip()]
      if not sources:
          print("ERROR: REPORT_SOURCES env var is empty.", file=sys.stderr)
          return 2

      pipeline, selector = build_program_pipeline(sources, cfg.program_connector_id)
      pipeline.set_state(Gst.State.PLAYING)

      # Keyboard input thread.
      def keypress_thread():
          for slot in slot_keypresses(cfg.keyboard_device):
              if 1 <= slot <= len(sources):
                  print(f"[REPORT] switching to slot {slot} = {sources[slot - 1]}")
                  select_slot(selector, slot - 1)

      t = threading.Thread(target=keypress_thread, daemon=True)
      t.start()

      loop = GLib.MainLoop()
      try:
          loop.run()
      except KeyboardInterrupt:
          pass
      finally:
          pipeline.set_state(Gst.State.NULL)
      return 0


  if __name__ == "__main__":
      sys.exit(main())
  ```

- [ ] **Step 5: Deploy and run on REPORT**

  ```bash
  rsync -av --delete report/ cody@192.168.50.10:/home/cody/report/
  ssh cody@192.168.50.10
  sudo mkdir -p /etc/precrime
  cat <<EOF | sudo tee /etc/precrime/report.conf
  program_connector_id = <YOUR_HDMI_A_1_ID>
  preview_connector_id = <YOUR_HDMI_A_2_ID>
  keyboard_device = "/dev/input/event0"
  EOF
  ```

  Find the USB keyboard's evdev device:
  ```bash
  ls /dev/input/by-id/ | grep -i kbd
  ```
  Resolve the symlink to the actual `eventN` path and update `/etc/precrime/report.conf` to match.

- [ ] **Step 6: Run the daemon manually**

  ```bash
  cd ~/report
  sudo REPORT_SOURCES="PRECOG-01-IPHONE-STAGE,PRECOG-02-CCTV-DOOR" python3 -m report
  ```

  Expected: HDMI-A-1 shows the first source. Press `1` on the USB keyboard → it switches to PRECOG-01. Press `2` → switches to PRECOG-02. **M6 milestone passes.**

- [ ] **Step 7: Append M6 to the runbook**

  Append to `report/runbook.md`:

  ```markdown
  ## M6 verification — YYYY-MM-DD
  - Two NDI sources fed into input-selector
  - Number keys 1 and 2 switch between sources
  - Switch latency observed: <ballpark, e.g., ~80ms>
  ```

- [ ] **Step 8: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/report/pipeline.py report/report/input.py report/report/__main__.py report/runbook.md
  git commit -m "report: M6, multi-source input-selector + USB keyboard switching"
  ```

---

### Task 7: NDI auto-discovery (replace env var with live source list)

**Files:**
- Create: `report/report/discovery.py`
- Create: `report/tests/test_discovery.py` (interface-level test only)
- Modify: `report/report/__main__.py`

- [ ] **Step 1: Write an interface test for the discovery module**

  We can't unit-test libndi find in isolation, but we can test the discovery loop's name-filter logic.

  Create `report/tests/test_discovery.py`:

  ```python
  from report.discovery import is_precog_source


  def test_precog_named_source_accepted():
      assert is_precog_source("PRECOG-01-IPHONE-STAGE (Channel 1)")
      assert is_precog_source("PRECOG-02-CCTV-DOOR")


  def test_non_precog_source_rejected():
      assert not is_precog_source("REPORT (Internal)")
      assert not is_precog_source("Random Studio Source")
      assert not is_precog_source("")


  def test_extracts_display_name():
      from report.discovery import display_name
      assert display_name("PRECOG-01-IPHONE-STAGE (Channel 1)") == "PRECOG-01-IPHONE-STAGE"
      assert display_name("PRECOG-02-CCTV-DOOR") == "PRECOG-02-CCTV-DOOR"
  ```

- [ ] **Step 2: Run, watch it fail**

  ```bash
  cd report && python -m pytest tests/test_discovery.py -v
  ```

  Expected: ImportError.

- [ ] **Step 3: Create `report/report/discovery.py`**

  ```python
  """NDI source discovery via gst-plugin-rs's ndi-find utility, or libndi via ctypes.

  Phase 1 implementation uses NDI's native Find API through libndi via ctypes for portability.
  """

  import ctypes
  import threading
  import time
  from collections.abc import Callable


  def is_precog_source(name: str) -> bool:
      return name.startswith("PRECOG-")


  def display_name(ndi_name: str) -> str:
      """Strip the trailing ' (Channel N)' that NDI Find includes."""
      idx = ndi_name.find(" (")
      return ndi_name[:idx] if idx > 0 else ndi_name


  class NDIDiscovery:
      """Wraps NDIlib_find_* via ctypes. Polls every 2s and calls on_change with current set of source names."""

      def __init__(self, on_change: Callable[[list[str]], None]):
          self._on_change = on_change
          self._stop = threading.Event()
          self._thread: threading.Thread | None = None
          self._lib = ctypes.CDLL("libndi.so")
          self._setup_lib_signatures()
          self._known: set[str] = set()

      def _setup_lib_signatures(self):
          # NDIlib_initialize
          self._lib.NDIlib_initialize.restype = ctypes.c_bool
          # NDIlib_find_create_v2(NDIlib_find_create_t* p_create)
          self._lib.NDIlib_find_create_v2.restype = ctypes.c_void_p
          self._lib.NDIlib_find_create_v2.argtypes = [ctypes.c_void_p]
          # NDIlib_find_get_current_sources(NDIlib_find_instance_t, uint32_t* p_no_sources)
          self._lib.NDIlib_find_get_current_sources.restype = ctypes.c_void_p
          self._lib.NDIlib_find_get_current_sources.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
          # NDIlib_find_wait_for_sources(NDIlib_find_instance_t, uint32_t timeout_in_ms)
          self._lib.NDIlib_find_wait_for_sources.restype = ctypes.c_bool
          self._lib.NDIlib_find_wait_for_sources.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
          # NDIlib_find_destroy
          self._lib.NDIlib_find_destroy.argtypes = [ctypes.c_void_p]

      def start(self):
          if not self._lib.NDIlib_initialize():
              raise RuntimeError("NDIlib_initialize failed")
          self._find = self._lib.NDIlib_find_create_v2(None)
          if not self._find:
              raise RuntimeError("NDIlib_find_create_v2 returned NULL")
          self._thread = threading.Thread(target=self._loop, daemon=True)
          self._thread.start()

      def stop(self):
          self._stop.set()
          if self._thread:
              self._thread.join(timeout=5)
          if hasattr(self, "_find") and self._find:
              self._lib.NDIlib_find_destroy(self._find)

      def _loop(self):
          while not self._stop.is_set():
              # Wait up to 2s for changes.
              self._lib.NDIlib_find_wait_for_sources(self._find, 2000)
              names = self._read_sources()
              precog_names = sorted({display_name(n) for n in names if is_precog_source(n)})
              current = set(precog_names)
              if current != self._known:
                  self._known = current
                  self._on_change(precog_names)

      def _read_sources(self) -> list[str]:
          class NDIlib_source_t(ctypes.Structure):
              _fields_ = [
                  ("p_ndi_name", ctypes.c_char_p),
                  ("p_url_address", ctypes.c_char_p),
              ]

          count = ctypes.c_uint32(0)
          src_ptr = self._lib.NDIlib_find_get_current_sources(self._find, ctypes.byref(count))
          if not src_ptr or count.value == 0:
              return []
          array_type = NDIlib_source_t * count.value
          sources = ctypes.cast(src_ptr, ctypes.POINTER(array_type))[0]
          return [s.p_ndi_name.decode("utf-8") for s in sources if s.p_ndi_name]
  ```

- [ ] **Step 4: Run the unit tests, watch them pass**

  ```bash
  python -m pytest tests/test_discovery.py -v
  ```

  Expected: 4 passed (`is_precog_source` and `display_name` are pure functions; ctypes binding is not tested in unit tests).

- [ ] **Step 5: Update `report/report/__main__.py` to use discovery**

  Replace the entire file with:

  ```python
  """REPORT daemon entry point with live NDI discovery."""

  import os
  import sys
  import threading
  import gi
  gi.require_version("Gst", "1.0")
  from gi.repository import Gst, GLib

  from report.config import parse_config
  from report.discovery import NDIDiscovery
  from report.mapping import assign_slots
  from report.pipeline import build_program_pipeline, select_slot
  from report.input import slot_keypresses


  class Report:
      def __init__(self, cfg):
          self.cfg = cfg
          self.pipeline = None
          self.selector = None
          self.sources_in_order: list[str] = []
          self.lock = threading.Lock()

      def on_sources_changed(self, names: list[str]):
          mapping = assign_slots(names, self.cfg.source_slot_overrides)
          # Build the list of sources in slot order (slot index = list index + 1).
          ordered = [None] * max(mapping.values(), default=0)
          for name, slot in mapping.items():
              ordered[slot - 1] = name
          new_sources = [n for n in ordered if n]
          with self.lock:
              if new_sources == self.sources_in_order:
                  return
              print(f"[REPORT] sources changed: {new_sources}")
              if self.pipeline is not None:
                  self.pipeline.set_state(Gst.State.NULL)
              self.sources_in_order = new_sources
              if not new_sources:
                  self.pipeline = None
                  self.selector = None
                  return
              self.pipeline, self.selector = build_program_pipeline(
                  new_sources, self.cfg.program_connector_id
              )
              self.pipeline.set_state(Gst.State.PLAYING)

      def handle_keypress(self, slot: int):
          with self.lock:
              if self.selector is None:
                  return
              if 1 <= slot <= len(self.sources_in_order):
                  print(f"[REPORT] cut to slot {slot} = {self.sources_in_order[slot - 1]}")
                  select_slot(self.selector, slot - 1)


  def main() -> int:
      Gst.init(None)

      config_path = os.environ.get("REPORT_CONFIG", "/etc/precrime/report.conf")
      with open(config_path) as f:
          cfg = parse_config(f.read())

      report = Report(cfg)
      discovery = NDIDiscovery(on_change=report.on_sources_changed)
      discovery.start()

      def keypress_thread():
          for slot in slot_keypresses(cfg.keyboard_device):
              report.handle_keypress(slot)

      threading.Thread(target=keypress_thread, daemon=True).start()

      loop = GLib.MainLoop()
      try:
          loop.run()
      except KeyboardInterrupt:
          pass
      finally:
          discovery.stop()
          if report.pipeline:
              report.pipeline.set_state(Gst.State.NULL)
      return 0


  if __name__ == "__main__":
      sys.exit(main())
  ```

- [ ] **Step 6: Deploy and run, verify auto-discovery**

  ```bash
  rsync -av --delete report/ cody@192.168.50.10:/home/cody/report/
  ssh cody@192.168.50.10
  cd ~/report
  sudo python3 -m report
  ```

  With at least one PRECOG live: console should print `[REPORT] sources changed: ['PRECOG-...']` within a few seconds. Number keys switch between any discovered sources.

  Bring another PRECOG online: a new "sources changed" line should print within a few seconds; pressing the new slot number should cut to it.

- [ ] **Step 7: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/report/discovery.py report/report/__main__.py report/tests/test_discovery.py
  git commit -m "report: NDI auto-discovery loop, dynamic slot mapping"
  ```

---

### Task 8: M7 — multiview preview on HDMI-A-2

**Files:**
- Modify: `report/report/pipeline.py`
- Modify: `report/report/__main__.py`
- Modify: `report/runbook.md`

- [ ] **Step 1: Add a multiview builder to `report/report/pipeline.py`**

  Append to `report/report/pipeline.py`:

  ```python
  def build_preview_pipeline(source_names: list[str], connector_id: int) -> Gst.Pipeline:
      """Build the multiview pipeline. Returns the pipeline (no element handle needed — purely display)."""
      n = len(source_names)
      if n == 0:
          # Black placeholder when no sources.
          pipeline_str = (
              f'videotestsrc pattern=black is-live=true ! video/x-raw,width=1920,height=1080 '
              f'! videoconvert ! kmssink connector-id={connector_id}'
          )
          return Gst.parse_launch(pipeline_str)

      # Choose grid: 1=full, 2-4=2x2, 5-9=3x3.
      if n == 1:
          grid_w, grid_h = 1, 1
      elif n <= 4:
          grid_w, grid_h = 2, 2
      else:
          grid_w, grid_h = 3, 3

      tile_w = 1920 // grid_w
      tile_h = 1080 // grid_h

      compositor_parts = [f"compositor name=mix background=black"]
      for i in range(n):
          col = i % grid_w
          row = i // grid_w
          x = col * tile_w
          y = row * tile_h
          compositor_parts.append(
              f"sink_{i}::xpos={x} sink_{i}::ypos={y} sink_{i}::width={tile_w} sink_{i}::height={tile_h}"
          )

      compositor_str = " ".join(compositor_parts)

      source_parts = []
      for i, name in enumerate(source_names):
          source_parts.append(
              f'ndisrc ndi-name="{name}" ! ndisrcdemux name=pd{i} '
              f'pd{i}.video ! queue max-size-buffers=4 leaky=downstream '
              f'! videoconvert ! videoscale ! video/x-raw,width={tile_w},height={tile_h} ! mix.sink_{i}'
          )

      pipeline_str = (
          f"{compositor_str} {' '.join(source_parts)} "
          f"mix. ! videoconvert ! kmssink connector-id={connector_id}"
      )
      return Gst.parse_launch(pipeline_str)
  ```

- [ ] **Step 2: Modify `Report` in `__main__.py` to manage the preview pipeline alongside the program pipeline**

  In `report/report/__main__.py`, replace the `Report` class with:

  ```python
  class Report:
      def __init__(self, cfg):
          self.cfg = cfg
          self.program_pipeline = None
          self.preview_pipeline = None
          self.selector = None
          self.sources_in_order: list[str] = []
          self.lock = threading.Lock()

      def on_sources_changed(self, names: list[str]):
          mapping = assign_slots(names, self.cfg.source_slot_overrides)
          ordered = [None] * max(mapping.values(), default=0)
          for name, slot in mapping.items():
              ordered[slot - 1] = name
          new_sources = [n for n in ordered if n]
          with self.lock:
              if new_sources == self.sources_in_order:
                  return
              print(f"[REPORT] sources changed: {new_sources}")
              if self.program_pipeline is not None:
                  self.program_pipeline.set_state(Gst.State.NULL)
              if self.preview_pipeline is not None:
                  self.preview_pipeline.set_state(Gst.State.NULL)
              self.sources_in_order = new_sources

              from report.pipeline import build_preview_pipeline
              self.preview_pipeline = build_preview_pipeline(new_sources, self.cfg.preview_connector_id)
              self.preview_pipeline.set_state(Gst.State.PLAYING)

              if not new_sources:
                  self.program_pipeline = None
                  self.selector = None
                  return

              self.program_pipeline, self.selector = build_program_pipeline(
                  new_sources, self.cfg.program_connector_id
              )
              self.program_pipeline.set_state(Gst.State.PLAYING)

      def handle_keypress(self, slot: int):
          with self.lock:
              if self.selector is None:
                  return
              if 1 <= slot <= len(self.sources_in_order):
                  print(f"[REPORT] cut to slot {slot} = {self.sources_in_order[slot - 1]}")
                  select_slot(self.selector, slot - 1)
  ```

  Update the `finally:` block in `main()` to stop both pipelines:

  ```python
      finally:
          discovery.stop()
          if report.program_pipeline:
              report.program_pipeline.set_state(Gst.State.NULL)
          if report.preview_pipeline:
              report.preview_pipeline.set_state(Gst.State.NULL)
  ```

- [ ] **Step 3: Plug a second display into HDMI-A-2, deploy, and run**

  ```bash
  rsync -av --delete report/ cody@192.168.50.10:/home/cody/report/
  ssh cody@192.168.50.10
  cd ~/report && sudo python3 -m report
  ```

  Expected:
  - HDMI-A-1 still shows the program out (single source, switchable with keyboard).
  - HDMI-A-2 shows a multiview grid of all live PRECOG sources at once.

  Bring more PRECOGs online: multiview grid re-tiles automatically. **M7 milestone passes.**

- [ ] **Step 4: Append M7 to runbook**

  Append to `report/runbook.md`:

  ```markdown
  ## M7 verification — YYYY-MM-DD
  - Multiview pipeline rendering on HDMI-A-2 alongside program out on HDMI-A-1
  - Tested with <N> sources, grid size <2x2 | 3x3> as expected
  ```

- [ ] **Step 5: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/report/pipeline.py report/report/__main__.py report/runbook.md
  git commit -m "report: M7, multiview preview pipeline on HDMI-A-2"
  ```

---

### Task 9: M8 — tally overlay on multiview

**Files:**
- Modify: `report/report/pipeline.py`
- Modify: `report/report/__main__.py`
- Modify: `report/runbook.md`

- [ ] **Step 1: Add tally drawing to the preview pipeline using `cairooverlay`**

  Replace the contents of `build_preview_pipeline` in `report/report/pipeline.py` with a version that exposes a tally callback. Append/replace:

  ```python
  import cairo


  def build_preview_pipeline(
      source_names: list[str],
      connector_id: int,
      get_active_slot: "Callable[[], int | None]",
  ) -> Gst.Pipeline:
      """Build multiview with a cairo overlay that draws a red border on the active tile.

      get_active_slot is a callable returning the 1-based slot of the currently-program source,
      or None if no source is selected.
      """
      n = len(source_names)
      if n == 0:
          pipeline_str = (
              f'videotestsrc pattern=black is-live=true ! video/x-raw,width=1920,height=1080 '
              f'! videoconvert ! kmssink connector-id={connector_id}'
          )
          return Gst.parse_launch(pipeline_str)

      if n == 1:
          grid_w, grid_h = 1, 1
      elif n <= 4:
          grid_w, grid_h = 2, 2
      else:
          grid_w, grid_h = 3, 3

      tile_w = 1920 // grid_w
      tile_h = 1080 // grid_h

      compositor_parts = [f"compositor name=mix background=black"]
      for i in range(n):
          col = i % grid_w
          row = i // grid_w
          x = col * tile_w
          y = row * tile_h
          compositor_parts.append(
              f"sink_{i}::xpos={x} sink_{i}::ypos={y} sink_{i}::width={tile_w} sink_{i}::height={tile_h}"
          )

      compositor_str = " ".join(compositor_parts)

      source_parts = []
      for i, name in enumerate(source_names):
          source_parts.append(
              f'ndisrc ndi-name="{name}" ! ndisrcdemux name=pd{i} '
              f'pd{i}.video ! queue max-size-buffers=4 leaky=downstream '
              f'! videoconvert ! videoscale ! video/x-raw,width={tile_w},height={tile_h} ! mix.sink_{i}'
          )

      pipeline_str = (
          f"{compositor_str} {' '.join(source_parts)} "
          f"mix. ! videoconvert ! cairooverlay name=tally ! videoconvert ! kmssink connector-id={connector_id}"
      )
      pipeline = Gst.parse_launch(pipeline_str)

      overlay = pipeline.get_by_name("tally")

      def on_draw(_overlay, ctx, _timestamp, _duration):
          slot = get_active_slot()
          if slot is None or slot < 1 or slot > n:
              return
          idx = slot - 1
          col = idx % grid_w
          row = idx // grid_w
          x = col * tile_w
          y = row * tile_h
          ctx.set_source_rgba(1.0, 0.0, 0.0, 1.0)
          ctx.set_line_width(8.0)
          ctx.rectangle(x + 4, y + 4, tile_w - 8, tile_h - 8)
          ctx.stroke()

      overlay.connect("draw", on_draw)

      return pipeline
  ```

- [ ] **Step 2: Update `Report` to track the active slot and pass `get_active_slot` to the preview builder**

  In `report/report/__main__.py`, modify `Report` to add an `active_slot` attribute, expose it via callback, and update on each keypress:

  ```python
  class Report:
      def __init__(self, cfg):
          self.cfg = cfg
          self.program_pipeline = None
          self.preview_pipeline = None
          self.selector = None
          self.sources_in_order: list[str] = []
          self.active_slot: int | None = None
          self.lock = threading.Lock()

      def _get_active_slot(self) -> int | None:
          return self.active_slot

      def on_sources_changed(self, names: list[str]):
          mapping = assign_slots(names, self.cfg.source_slot_overrides)
          ordered = [None] * max(mapping.values(), default=0)
          for name, slot in mapping.items():
              ordered[slot - 1] = name
          new_sources = [n for n in ordered if n]
          with self.lock:
              if new_sources == self.sources_in_order:
                  return
              print(f"[REPORT] sources changed: {new_sources}")
              if self.program_pipeline is not None:
                  self.program_pipeline.set_state(Gst.State.NULL)
              if self.preview_pipeline is not None:
                  self.preview_pipeline.set_state(Gst.State.NULL)
              self.sources_in_order = new_sources

              from report.pipeline import build_preview_pipeline
              self.preview_pipeline = build_preview_pipeline(
                  new_sources, self.cfg.preview_connector_id, self._get_active_slot
              )
              self.preview_pipeline.set_state(Gst.State.PLAYING)

              if not new_sources:
                  self.program_pipeline = None
                  self.selector = None
                  self.active_slot = None
                  return

              self.program_pipeline, self.selector = build_program_pipeline(
                  new_sources, self.cfg.program_connector_id
              )
              self.program_pipeline.set_state(Gst.State.PLAYING)
              # Default-cut to slot 1 on source list refresh.
              self.active_slot = 1
              select_slot(self.selector, 0)

      def handle_keypress(self, slot: int):
          with self.lock:
              if self.selector is None:
                  return
              if 1 <= slot <= len(self.sources_in_order):
                  print(f"[REPORT] cut to slot {slot} = {self.sources_in_order[slot - 1]}")
                  self.active_slot = slot
                  select_slot(self.selector, slot - 1)
  ```

- [ ] **Step 3: Deploy and run, verify tally moves with keypresses**

  ```bash
  rsync -av --delete report/ cody@192.168.50.10:/home/cody/report/
  ssh cody@192.168.50.10 'cd ~/report && sudo python3 -m report'
  ```

  Expected: multiview on HDMI-A-2 now has a red border around whichever tile corresponds to the live program source. Press number keys and confirm the red border moves to match. **M8 milestone passes.**

- [ ] **Step 4: Append M8 to runbook**

  Append to `report/runbook.md`:

  ```markdown
  ## M8 verification — YYYY-MM-DD
  - Tally overlay rendered via cairooverlay on the multiview pipeline
  - Red border tracks the active program source as switched by keyboard
  ```

- [ ] **Step 5: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/report/pipeline.py report/report/__main__.py report/runbook.md
  git commit -m "report: M8, tally overlay tracks active source"
  ```

---

### Task 10: Systemd service + boot-survival (M10)

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
  WorkingDirectory=/opt/report
  ExecStart=/usr/bin/python3 -m report
  Environment=REPORT_CONFIG=/etc/precrime/report.conf
  Restart=on-failure
  RestartSec=3
  StandardOutput=journal
  StandardError=journal
  User=root

  [Install]
  WantedBy=multi-user.target
  ```

  Note: User=root because `kmssink` and `evdev` access need DRM and input device permissions. A non-root user with `video` and `input` group membership would work but adds setup steps; defer that hardening to a future iteration.

- [ ] **Step 2: Deploy and install on REPORT**

  ```bash
  rsync -av --delete report/ cody@192.168.50.10:/home/cody/report/
  ssh cody@192.168.50.10 '
      sudo mkdir -p /opt/report &&
      sudo cp -r /home/cody/report/report /opt/report/ &&
      sudo cp /home/cody/report/report.service /etc/systemd/system/report.service &&
      sudo systemctl daemon-reload &&
      sudo systemctl enable report.service &&
      sudo systemctl start report.service
  '
  ```

- [ ] **Step 3: Verify it started**

  ```bash
  ssh cody@192.168.50.10 'sudo systemctl status report.service'
  ```

  Expected: `active (running)`. HDMI-A-1 and HDMI-A-2 outputs should be live.

  If failed:
  ```bash
  ssh cody@192.168.50.10 'sudo journalctl -u report.service -n 80 --no-pager'
  ```

- [ ] **Step 4: Reboot and verify M10 (boot survival)**

  ```bash
  ssh cody@192.168.50.10 'sudo reboot'
  ```

  Wait ~60 seconds. Check both HDMI outputs come alive without intervention; new PRECOGs joining the LAN are picked up by the running daemon.

- [ ] **Step 5: Append M10 to runbook**

  Append to `report/runbook.md`:

  ```markdown
  ## M10 verification — YYYY-MM-DD
  - report.service enabled, autostarts at boot
  - Cold-boot → both HDMI outputs live within: <observed seconds>
  - Survives cam disconnect / reconnect cleanly
  ```

- [ ] **Step 6: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/report.service report/runbook.md
  git commit -m "report: systemd service + M10 boot-survival verified"
  ```

---

### Task 11: Integration test with two real PRECOGs (M9)

**Files:**
- Modify: `report/runbook.md`

- [ ] **Step 1: Bring up the full Phase 1 system**

  - Router (Flint 2) powered and configured.
  - PRECOG-01-IPHONE-STAGE (iPhone) running NDI HX Camera.
  - PRECOG-02-CCTV-DOOR (Pi 5 with CCTV cam) running `precog.service`.
  - REPORT (Pi 5 wired to router) running `report.service`.
  - HDMI-A-1 plugged into a display or HDMI capture device for stream.
  - HDMI-A-2 plugged into a second display as operator preview.
  - USB keyboard plugged into REPORT.

- [ ] **Step 2: Verify the full operator experience**

  - Multiview on HDMI-A-2 shows both sources side-by-side with a red tally on whichever is live.
  - Press `1`: program out cuts to iPhone, tally moves.
  - Press `2`: program out cuts to CCTV, tally moves.
  - Move the iPhone, see motion on HDMI-A-1 with <300ms latency.

- [ ] **Step 3: Stress test — disconnect and reconnect a PRECOG**

  - Power off PRECOG-02 (CCTV Pi). Within ~5s the discovery loop drops it from the list and `on_sources_changed` fires. The pipeline rebuilds with the remaining sources; the CCTV tile disappears from multiview; if CCTV was the live program, REPORT auto-cuts to slot 1 (the lowest live source) during the rebuild. Expect a brief (~1-2s) black/glitch on HDMI-A-1 during pipeline tear-down + re-launch — this is acceptable for Phase 1; cleaner crossfade-on-loss is deferred.
  - Power CCTV Pi back on. ~30s later it reappears on the network, the pipeline rebuilds again, the multiview regrows the tile. Active slot resets to slot 1 each time the source set changes — operator will need to press the desired number key again if they wanted a different cam live.

- [ ] **Step 4: Append M9 to runbook**

  Append to `report/runbook.md`:

  ```markdown
  ## M9 verification — YYYY-MM-DD
  - Two PRECOGs + REPORT + router running together
  - Keyboard switching cuts cleanly between sources
  - Source disconnect/reconnect handled without daemon crash
  - Source-loss behavior: pipeline rebuilds, active slot resets to 1, brief glitch (~1-2s)
  - Deferred: crossfade-on-loss + keyboard speaker beep on involuntary cuts (system spec §3 Failure Modes)
  ```

- [ ] **Step 5: Commit**

  ```bash
  cd /Users/cody/Dev/precrime
  git add report/runbook.md
  git commit -m "report: M9 integration verified, two-PRECOG full system test"
  ```

---

### Task 12: Show-day runbook (M11)

**Files:**
- Modify: `report/runbook.md`

- [ ] **Step 1: Append the show-day procedure**

  Append to `report/runbook.md`:

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
  2. Power off CCTV PRECOG Pis: `ssh cody@precog-NN-... 'sudo shutdown -h now'`. Wait for green LED to stop.
  3. Power off REPORT: `ssh cody@192.168.50.10 'sudo shutdown -h now'`. Wait for green LED to stop.
  4. Power off router.
  5. Coil and pack cables.

  ## If things go wrong mid-show

  - **HDMI-A-1 goes black:** check `sudo systemctl status report.service` from a phone SSH'd to REPORT. Restart with `sudo systemctl restart report.service`.
  - **A PRECOG drops:** that tile vanishes from multiview; switch to a different source with the keyboard. After the show, debug.
  - **Both HDMIs go black but daemon is running:** kmssink may have lost the connector. `sudo systemctl restart report.service`.
  - **Router locks up:** power-cycle the Flint 2. Cams + REPORT will reconnect on their own within ~30s.
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
report/
├── install.sh                  # Reproducible package install
├── pyproject.toml              # Python project metadata
├── report.service              # systemd unit
├── runbook.md                  # Operator runbook (setup, verification, show-day, troubleshooting)
├── report/
│   ├── __init__.py
│   ├── __main__.py             # Daemon entry point; owns Report class
│   ├── config.py               # TOML config parser
│   ├── discovery.py            # NDI auto-discovery via libndi ctypes
│   ├── input.py                # USB keyboard via evdev
│   ├── mapping.py              # Source-name → slot mapping
│   └── pipeline.py             # GStreamer pipeline builders
└── tests/
    ├── __init__.py
    ├── test_config.py
    ├── test_discovery.py        # name-filter and display-name pure functions
    └── test_mapping.py
```

Total Python is ~250 lines. Tests cover the pure-function pieces (config, mapping, name filtering). System-level behavior is verified via observable HDMI output + journalctl logs at M5–M11.

## Done means

- All milestones M5 through M11 verified per the runbook
- `report.service` runs on boot, survives reboot, handles cam disconnect/reconnect
- Two-PRECOG integration test passes
- Show-day procedure documented
- Phase 1 of PRECRIME is complete
