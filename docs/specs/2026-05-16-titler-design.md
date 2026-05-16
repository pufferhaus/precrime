# PRECRIME Titler — Design Spec

**Date:** 2026-05-16
**Status:** Approved (Phase 1.5 scope)
**Author:** Cody (with Claude)

The Titler is a character generator and lower-third compositor added to REPORT, slotting between PRECRIME Phase 1 (hardware bring-up) and Phase 2 (smart plugs, MEZZANINE). It mirrors the feature set of the Videonics TitleMaker series (1990s broadcast CG appliances): a library of title pages composed of text and image layers, applied as a burn-in overlay on the program output with selectable transitions including shaped and feathered wipes.

The Titler is a feature of REPORT, not a separate device. It reuses REPORT's existing Pi 5 8GB, dual HDMI outputs, USB keyboard, and storage. Net additional hardware cost: **$0**. The operator flips REPORT between **Producer mode** (existing multiview + camera switching) and **Titler mode** (title editor) via a single hotkey on the same keyboard.

This spec covers all six design areas: hardware, video pipeline, feature parity vs Videonics, UI / edit flow, on-disk storage, and build phasing. Implementation plan follows in a separate document under `docs/plans/`.

## 1. Hardware

The Titler adds no hardware. REPORT's existing BOM is reused:

| Existing REPORT part | Titler use |
|---|---|
| Pi 5 8GB + active cooler | Hosts both Producer and Titler subsystems |
| HDMI0 (program) | Composited program with title burned in |
| HDMI1 (multiview / editor) | Mode-dependent: multiview in Producer mode, title editor in Titler mode |
| USB keyboard | Mode-routed: show-ops keys always active, edit keys active only in Titler mode |
| REPORT's existing storage (microSD or NVMe per Phase 1 BOM) | Title library, fonts, transition masks, asset images |
| Power, enclosure, cables | Unchanged |

Hardware decisions previously considered and rejected:
- **Standalone titler box (Pi 5 + CSI HDMI bridge + own display).** ~$180–$240 added. Rejected because REPORT already has all the parts and the titler is naturally upstream-of-program inside REPORT's pipeline anyway. A separate box would require a second NDI round-trip or a third HDMI loop with no benefit.
- **DisplayLink USB-C third display for dedicated editor.** ~$40 dongle + a monitor. Rejected — operator already has HDMI1 and the mode-toggle UX is acceptable.
- **External hardware button box for titler.** Out of scope for v1; MEZZANINE controller (PRECRIME Phase 2) absorbs this if desired later.

## 2. Video Pipeline

### High-level

Titles are composited into REPORT's existing GStreamer program pipeline. A new custom GL element is inserted between `input-selector` and `kmssink` on Pipeline A (program out). Multiview pipeline (Pipeline B on HDMI1) is unchanged when in Producer mode; in Titler mode, Pipeline B is torn down (its `kmssink` releases the HDMI-A-2 connector) and the editor UI claims the connector via DRM directly. On mode flip back, Pipeline B is rebuilt from scratch.

```
Pipeline A (HDMI0, ALWAYS LIVE — program out):
  N×ndisrc → input-selector → titler-gl-overlay → kmssink (HDMI-A-1)
                                    ↑
                            title state:
                              - current page texture (RGBA)
                              - next page texture (for transitions)
                              - transition type, phase, feather
                              - on-air flag

Pipeline B / Editor on HDMI1 (mode-dependent):
  Producer mode: existing multiview compositor + tally → kmssink (HDMI-A-2)
  Titler mode:   DRM/Cairo editor surface direct to (HDMI-A-2)
```

### The titler-gl-overlay element

A custom GStreamer element living in the `report` crate. Responsibilities:

- Accepts program video frames on its sink pad (NV12 / RGBA from upstream `input-selector`)
- Holds title texture handles owned by `report::titler`
- Runs a single fragment shader per output frame that composites: `program × title_overlay(current_page, next_page, transition, phase, feather)`
- Outputs RGBA / NV12 to the downstream `kmssink`

Element registration: `gstreamer::Element::register(None, "titler-overlay", Rank::None, TitlerOverlay::static_type())` at daemon startup.

### Why an in-pipeline GL element rather than a separate KMS overlay plane

KMS overlay planes can do plane-alpha and clip-rectangle effects, but cannot implement diagonal wipes, shaped wipes, feathered edges, or shader-based dissolves. Since shaped + feathered wipes are explicit v1 scope (§3), a real shader compositor is needed. The cost is ~1 frame of added latency on the program path (capture and switching are unchanged; only the final composite gets one extra GPU pass).

### Title texture lifecycle

- Title pages are rendered to RGBA by Cairo when their TOML changes (initial load, after each edit), cached as GL textures keyed by page id.
- During a transition, two textures (outgoing and incoming) are uploaded to the shader; on transition completion, the outgoing texture is released.
- When no title is on-air, the shader short-circuits to passthrough (mix with zero alpha).

### Frame budget

REPORT's existing pipeline budget on Pi 5 must still be met. Added cost from the titler element:

- 1 fragment-shader pass per output frame, 1920×1080: ~1–2ms on VideoCore VII (measured during prototype, see T1).
- Texture upload only on edit, never per-frame.
- Total added latency: 1 frame (16ms at 60fps).

If multiview ever becomes the budget bottleneck, that's an existing PRECRIME concern unaffected by the titler.

## 3. Feature Parity vs Videonics TitleMaker

The Videonics TitleMaker series (TM-2000, TM-3000, TM-Pro, TM-4000) defines the target feature set. Modern conveniences (TTF fonts, antialiased glyphs, 24-bit color, unlimited pages) are accepted where they fall out for free; truly cheesy items (kaleidoscope wipes, etc.) are dropped.

### Text rendering (v1)

- TTF / OTF fonts via Pango (~10 broadcast-friendly fonts shipped under permissive licenses)
- Free pt size selection
- Bold / italic / underline (Pango markup)
- 24-bit RGB color, plus alpha
- Drop shadow with offset, blur, and color
- Outline with width and color
- Per-text-block background fill
- L / C / R horizontal alignment
- Antialiased rendering (free win over Videonics bitmap glyphs)

### Page / library (v1 except where marked)

- Multi-page library, unlimited size (Videonics capped at 32–100 pages)
- Named pages
- Page sequencer: prev / next / jump by index
- *Playlist / cue stack* — **v2**
- *Folders / multi-show projects* — **v2** (filesystem already supports it; only the UI is deferred)

### Transitions (all v1)

| Transition | Shader logic |
|---|---|
| Cut | `step(phase, 0.5)` between A and B |
| Fade in / out | `mix(video, title, alpha * phase)` |
| Dissolve A→B | `mix(A, B, phase)` |
| Horizontal wipe L→R | `step(uv.x, phase)` |
| Vertical wipe T→B | `step(uv.y, phase)` |
| Diagonal wipe | `step(uv.x + uv.y, phase * 2)` |
| Push L (B pushes A off) | sample A at `uv - phase`, B at `uv + (1-phase)` |
| Slide L (B slides over A) | similar to push, A stationary |
| Soft-edge / feathered wipe | `smoothstep(phase - edge, phase + edge, axis)` — applies to all wipes |
| Shaped wipe (iris, heart, star, diamond, clock, etc.) | mask texture lookup + threshold |

A single uber-shader handles all transitions via a `wipe_type` uniform with dynamic branching. VC7 handles this without stalls. Adding a new geometric transition = one branch in the shader; adding a new shaped transition = drop a PNG mask in `masks/` and reference it from TOML, no code change.

### Motion (v1 except where marked)

- Crawl (horizontal scroll, ticker-style on one line of text)
- Roll (vertical scroll, credits-style)
- Adjustable speed (px/s) via hotkey
- Pause / resume crawl or roll
- *Multi-line crawl / ticker* — **v2**

### Video I/O behavior (v1)

- Clean program passthrough when no title is on-air
- Background solid color shown when no NDI sources at all (existing REPORT behavior, unchanged)
- Bypass mode (titles off via show-ops hotkey)
- Image / logo (PNG) layers — bug feature
- *Animated bug (APNG / GIF)* — **v2**

### Control (v1 except where marked)

- USB keyboard editing (REPORT's existing keyboard, mode-routed)
- Live take / clear / next cue via show-ops hotkeys
- Speed control for crawl / roll
- *MIDI / OSC / network control* — **v2**
- *Hardware button box* — Phase 2 via MEZZANINE controller

### Out of scope (both v1 and v2 of titler)

- Switcher / multi-input video (this is REPORT's job; titler is one layer on top)
- Streaming output / NDI out from titler (REPORT's program HDMI is captured downstream)
- Audio mixing UI (HDMI audio passes through unmodified)
- Live data feeds (clock / score / weather)

## 4. UI and Edit Flow

### Mode model

REPORT runs a single `Mode` state machine:

```
Mode = Producer | Titler  (toggled by F12 on the USB keyboard)
```

- **Program output (HDMI0) is always live.** Mode only affects what's on the operator's screen (HDMI1) and which keyboard layer is active.
- Title state persists across mode flips — flipping to Titler doesn't reset the page library or the on-air title.
- Show-ops actions (take title, clear title, switch camera) work from either mode. Edit actions (typing, formatting) only work in Titler mode.

### Keyboard routing (two-layer)

| Key | Producer mode | Titler mode |
|---|---|---|
| `1`–`8` | switch program camera | switch program camera |
| `Enter` | (currently unused; future cue feature) | TAKE title ON-AIR with current transition |
| `Esc` / `Backspace` | — | CLEAR title (transition off-air) |
| `[` / `]` | — | previous / next library page |
| `Space` | (reserved) | (reserved — future cue advance) |
| `Ctrl+Enter` | — | take + auto-advance library cursor |
| `F12` | flip to Titler mode | flip to Producer mode |
| typing, arrows, `Tab`, `F2`–`F9`, `Ctrl+S`, `N`, `D`, `T` | ignored | editor input |
| `Ctrl+L` | — | toggle live-edit (changes push to program without transition) |
| `{` / `}` | — | adjust feather width |

Keyboard routing happens inside `report::input::dispatch`, which inspects current `Mode` and the keycode. Show-ops keys are matched before edit keys.

### HDMI1 layout in Titler mode

Rendered via Cairo directly to a DRM framebuffer (no Wayland, no GTK). Same rendering stack as REPORT's existing tally overlay. Hand-rolled widgets: list view, text input, focus state.

```
┌──────────────────────────────────────────────────────────────────────┐
│ TITLER  •  page 7/23: "lower_third_speaker"  •  unsaved              │
├────────────────────┬─────────────────────────────────────────────────┤
│  Library           │  Live composite preview                         │
│                    │  (program + title, downscaled, same shader      │
│  ► 7 lower_third…  │   path as HDMI0)                                │
│    8 sponsor_logo  │                                                 │
│    9 score_intro   │  [ ON-AIR ]                                     │
│   10 venue_card    │                                                 │
│    …               │                                                 │
│                    ├─────────────────────────────────────────────────┤
│  [↑↓] navigate     │  Page editor                                    │
│  [N]ew [D]elete    │  Line 1: │ DR. JANE SMITH                       │
│  [F2] rename       │  Line 2: │ Cognitive Neuroscientist             │
│                    │  Font: Inter Bold  •  Size: 64                  │
│                    │  Color: #FFFFFF  •  Shadow: on  •  Outline: off │
│                    │  Position: bottom-left, 80px margin             │
├────────────────────┴─────────────────────────────────────────────────┤
│ Transition: dissolve  •  duration: 0.5s  •  feather: 4px             │
├──────────────────────────────────────────────────────────────────────┤
│ PROGRAM TALLY: cam 3 LIVE   •   [Enter] take title  [Esc] clear      │
└──────────────────────────────────────────────────────────────────────┘
```

Persistent tally row at the bottom keeps the camera-switching context visible in Titler mode — operator never loses program awareness.

### Producer-mode title tally strip

In Producer mode, the multiview compositor reserves a single row at the bottom:

```
┌─────────────────────────────────────────────────┐
│ multiview grid: cams 1-8                        │
├─────────────────────────────────────────────────┤
│ TITLE: lower_third_speaker  ON-AIR  •  next:    │
│        sponsor_logo  •  trans: dissolve 0.5s    │
└─────────────────────────────────────────────────┘
```

Symmetric to Titler mode's persistent tally row: each mode shows a slim status indicator for the *other* mode's state.

### Editor field hotkeys

| Key | Action |
|---|---|
| `↑` / `↓` | navigate library list |
| `Tab` / `Shift+Tab` | move focus between library and editor fields |
| `Enter` (in field) | confirm field, move to next |
| `Esc` (in field) | revert field to saved value |
| `F2` | rename current page |
| `F5` | cycle font |
| `F6` | cycle preset color |
| `F7` | toggle shadow |
| `F8` | toggle outline |
| `F9` | cycle position preset (9 anchors: BL, B, BR, L, C, R, TL, T, TR) |
| `N` | new page (added below cursor) |
| `D` | delete current page (confirmation modal) |
| `T` | open transition picker (modal: list of types, duration, feather, mask) |
| `Ctrl+S` | save library to disk |

### Live editing of an on-air title

Default behavior: edits to a currently-on-air title are **staged** in memory. The status bar shows `staged changes — Enter to apply`. Pressing `Enter` triggers a take with the current transition, applying staged changes.

Optional **live-edit** mode (`Ctrl+L` toggles): edits push to program immediately with no transition. Useful for fixing a typo on a stuck title without taking it off-air.

### State persistence across mode flips

- Library + cursor position: preserved
- Editor field buffers: preserved (unsaved buffer survives flips; lost only on power-off without save)
- On-air title state: preserved (program output is independent of operator's mode)
- Transition settings: preserved

### Renderer choice: direct DRM + Cairo

The editor is implemented as a direct DRM modeset + Cairo rendering loop on HDMI1, with no display server (X / Wayland) and no widget toolkit. Rationale:

- Same stack already used by REPORT's tally overlay and multiview composition
- No additional system dependencies; minimal apt surface
- Fast boot, low memory
- Cairo + Pango can do all the drawing the editor needs

The cost is hand-rolled widgets (focus state, text input cursor, list view scrolling). The widget set is small enough (~5 widget types) that this is manageable, and isolating them in `report::titler::editor::widgets` keeps them testable.

## 5. Storage and Title File Format

### Layout

```
/etc/precrime/titler/
├── library/
│   ├── default/                          ← default library, always present
│   │   ├── 001-lower_third_speaker.toml
│   │   ├── 002-sponsor_logo.toml
│   │   ├── 003-score_intro.toml
│   │   └── assets/
│   │       ├── sponsor_logo.png
│   │       └── venue_map.png
│   └── show_2026_05_16/                  ← optional per-show library (v2 picks via UI)
│       └── ...
├── fonts/                                ← TTF/OTF scanned on boot, registered with Pango
│   ├── Inter-Bold.ttf
│   ├── Inter-Regular.ttf
│   └── ...
├── masks/                                ← grayscale PNG masks for shaped wipes
│   ├── circle_iris.png
│   ├── diamond.png
│   ├── star.png
│   └── ...
└── titler.conf                           ← default library, default transition, default font
```

### Why filesystem-as-library

- **Git-friendly:** operator can version-control a show
- **Drop-in assets:** copy a PNG into `assets/`, it appears in pickers on next refresh
- **Trivial backup:** rsync the directory
- **Crash-safe:** no DB to corrupt; partial writes are isolated per page

### Page file format (TOML, schema = 1)

```toml
schema = 1
name = "lower_third_speaker"

[render]
canvas = { w = 1920, h = 1080 }
position = "bottom-left"     # 9-anchor preset OR { x = 80, y = 920 }
margin = 80

[[layer]]
kind = "rect"
rect = { x = 0, y = 880, w = 800, h = 160 }
fill = "#000000CC"

[[layer]]
kind = "text"
text = "DR. JANE SMITH"
font = "Inter-Bold"
size = 64
color = "#FFFFFF"
position = { x = 30, y = 920 }
shadow = { offset = [2, 2], blur = 4, color = "#000000AA" }
outline = { width = 0, color = "#000000" }

[[layer]]
kind = "text"
text = "Cognitive Neuroscientist"
font = "Inter-Regular"
size = 32
color = "#FFFFFFCC"
position = { x = 30, y = 990 }

[[layer]]
kind = "image"
src = "assets/network_bug.png"  # relative to this page's library folder
position = { x = 1780, y = 40 }
opacity = 1.0

[transition]
type = "dissolve"            # cut | fade | dissolve | wipe_h | wipe_v | wipe_d | push_l | push_r | slide_l | slide_r | shaped
duration_s = 0.5
feather_px = 4
mask = "circle_iris"         # only used when type = "shaped"

[motion]                     # optional
type = "none"                # none | crawl | roll
speed_pps = 60
loop = true
```

### Why TOML

- Human-readable and -editable in any text editor (operator can prep titles outside the box)
- Comments allowed (notes about which talent each page is for)
- Strict typing
- `toml-rs` already in REPORT's dep graph (`report.conf` uses it)

### Filename convention

`NNN-slug.toml`. Numeric prefix gives explicit order (library is sorted by filename, not by page name). The slug is human-readable. Rename via editor F2 rewrites the file with a new slug and preserves the prefix; reordering pages bumps prefixes.

### Runtime library model

```rust
struct Library {
    root: PathBuf,
    pages: Vec<Page>,           // sorted by filename
    cursor: usize,
    dirty: HashSet<usize>,      // unsaved buffers, by page index
}

struct Page {
    path: PathBuf,
    name: String,
    layers: Vec<Layer>,
    transition: Transition,
    motion: Option<Motion>,
    cached_texture: Option<TextureHandle>,
}
```

Re-render of a page's RGBA texture happens:
- On load
- On any edit (debounced 100ms — typing doesn't thrash GL uploads)
- On font or asset change (filesystem watch via `notify` crate)

### Asset (PNG) handling

- `assets/` paths in TOML are resolved relative to the page's library folder
- Loaded lazily into Cairo `ImageSurface`, cached in-process
- Filesystem watch via `notify`; reload + re-render dependent pages when a file changes (operator can update a logo mid-show without daemon restart)
- Defensive: also rescan every 5s as a safety net, since `notify` can drop events under high I/O load on the Pi

### Font handling

- All `*.ttf` / `*.otf` under `fonts/` scanned on boot, registered with Pango via FontConfig
- Pango handles fallback, hinting, kerning
- Ship a small set of permissively-licensed defaults; operator drops their own as needed

### Schema versioning

Every page file has `schema = N`. Future migrations bump and run on load. `report::titler::migrate` lives next to the parser; tests cover round-trips for every supported schema version.

### Risks

- Deeply nested layer counts (>10 per page) make TOML unwieldy. Mitigate by keeping layer count modest in practice; revisit format (KDL, JSON) only if hit.
- `notify` event drops on Pi under heavy I/O — addressed by 5s rescan safety net.

## 6. Build Phasing

Six phases (T1–T6), each demonstrable on its own. Titler is opt-in via mode key, so REPORT stays usable for shows without titles even mid-implementation.

### T1 — Minimum titler (proves the path)
- `report::titler` module scaffold
- TOML page parser, single hardcoded page
- Cairo render → RGBA → GL texture upload
- Custom GStreamer GL element registered, inserted post `input-selector`, pre `kmssink`
- Composite static title over program; no transitions yet; no editor UI
- Title visibility toggled via daemon SIGHUP rereading config

**Demo:** title burned over live cam feed.

**De-risking:** prototype the custom GL element standalone with `videotestsrc` on x86 dev machine before integrating.

### T2 — Mode + library + cut / fade
- `Mode` state machine in `report::daemon` (Producer ↔ Titler via F12)
- Library loader: scan `library/default/*.toml`
- Direct DRM + Cairo editor surface on HDMI1 — list + selection only, no editing
- Show-ops keys: `Enter` (take), `Esc` (clear), `[`/`]` (cycle pages)
- Two transitions in shader: cut, fade

**Demo:** operator flips to Titler mode, selects a page, flips back, takes it on-air with fade.

### T3 — Edit, save, formatting
- Hand-rolled text input widget (focus, cursor, typing, navigation)
- New / delete / rename page (`N` / `D` / `F2`)
- Font cycle, color cycle, shadow / outline toggles, 9-anchor position presets
- `Ctrl+S` save, dirty-buffer tracking
- Confirmation modal for delete

**Demo:** operator builds a lower-third from scratch on the box.

### T4 — Full geometric transition matrix
- Uber-shader gains dissolve, wipes (H / V / D), push, slide
- Feather uniform implemented; `smoothstep` edge on all wipes
- Transition picker modal (`T`): list of types, duration slider, feather slider
- Per-page transition saved in TOML

**Demo:** every Videonics geometric transition selectable per page.

### T5 — Shaped wipes + motion
- Mask texture loader (PNG → GL texture, cached, hot-reloadable)
- `shaped` transition type with `mask` field active
- Ship 12 default masks under `masks/`: circle iris, diamond, heart, star, clock-sweep, vertical blinds, checkerboard, four-corner, plus-sign, ellipse, hex, gradient-noise
- Crawl + roll motion in shader (uv offset over time)
- Speed adjust hotkeys (`{` / `}`)

**Demo:** iris wipe, heart wipe, scrolling credits roll.

### T6 — Assets, polish, live-edit
- PNG image layers (logos / bug)
- `notify` filesystem watcher for `assets/`, hot reload
- Live-edit toggle (`Ctrl+L`)
- Staged-changes status indicator
- Title tally strip on Producer-mode multiview (cross-mode awareness)
- Documentation pass: update REPORT runbook with mode toggle, keymap, library layout

**Demo:** sponsor logo bug, mid-show typo fix without taking off-air.

### Out of scope for titler v1 (all defer to v2)

- Playlist / cue stack
- Animated bug (GIF / APNG)
- Multi-line crawl / ticker
- MIDI / OSC / network control
- Live data sources (clock, score, weather)
- Folders / multi-show libraries in UI (filesystem already supports them)
- Hardware button box (Phase 2 via MEZZANINE)

### Dependencies on PRECRIME state

| Phase | Blocked by |
|---|---|
| T1 development | None — prototype on x86 dev box with `videotestsrc` |
| T1 ship to Pi | PRECRIME Phase 1 M3–M11 (REPORT hardware bring-up, real program pipeline running on Pi) |
| T2–T6 development | Mostly x86; final tuning and integration testing on Pi |
| Editor UI | Existing Cairo / DRM stack from tally overlay (already in REPORT) |

### Where titler slots into the PRECRIME roadmap

- **PRECRIME Phase 1** — finish hardware bring-up, run first show with REPORT alone (no titles)
- **PRECRIME Phase 1.5 — Titler T1–T6** ← this spec
- **PRECRIME Phase 2** — smart plug bus, phone provisioning, MEZZANINE controller

Rationale: titler is self-contained inside REPORT, with no router / network / multi-device coupling. Slotting it before Phase 2 means the first real show with titles uses the same minimal rig, and Phase 2 hardware layers on top later.

### Test strategy per phase

Continuing PRECRIME's TDD pattern (pure-Rust modules unit-tested on x86, GStreamer / GL / Cairo on Pi):

- **Unit-testable on x86:** TOML parser, page model, library loader, mode state machine, transition matrix logic, schema migration, keyboard routing dispatch
- **Pi-only integration tests:** custom GL element pipeline, Cairo rendering output, evdev keyboard end-to-end, KMS modeset on HDMI1

## 7. Failure modes

| Failure | Behavior |
|---|---|
| Title TOML fails to parse | Page is marked `invalid` in library; editor shows error inline; program output unaffected |
| Asset PNG missing | Layer renders with magenta placeholder rectangle; warning in logs; program still outputs |
| Font missing | Pango falls back to its default sans; warning in logs |
| Shader compile failure on boot | Daemon refuses to start; systemd reports; surfaces on HDMI0 as "TITLER SHADER ERROR" with passthrough of program continuing |
| Editor surface fails to acquire HDMI1 in Titler mode | Mode flip rejected; status briefly shown on HDMI0; operator notified via keyboard speaker beep |
| `notify` watcher dies | 5s polling rescan keeps library / assets fresh; warning logged |
| Library directory missing | Daemon creates empty `library/default/`, ships with one example page |

## 8. Open questions for implementation

- Concrete GStreamer crate version compatible with the custom GL element pattern on the Pi 5's Mesa V3D — verify during T1 prototyping.
- Whether VideoCore VII supports enough simultaneous texture units for the uber-shader's worst case (program + page A + page B + mask + lookup tables for color cycling). Likely yes; verify during T4.
- Exact Pipeline B teardown / rebuild latency on mode flip — needs measurement during T2. If it exceeds ~500ms it will feel sluggish; mitigations include keeping Pipeline B paused rather than torn down (requires confirming `kmssink` releases the connector when paused), or pre-warming a fresh pipeline in the background before flipping.
