# Titler Implementation Plan

> **STATUS — PAUSED 2026-05-16.** Titler is not a core-launch feature. Parked on branch `titler-t1` until REPORT core dev finishes. Do not merge to main yet. Resume by checking out `titler-t1` and starting at **T1 Task 3** (custom GStreamer element). See `## Resume Checklist` below before picking up.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

## Resume Checklist

When picking this work back up:

1. **Branch:** `git checkout titler-t1` — all T1 work lives here, isolated from main.
2. **Re-baseline:** rebase onto current `main` and run `cargo build -p report && cargo test -p report` before touching anything. The REPORT crate will have moved.
3. **State of T1:**
   - ✅ Task 1 — module scaffold + TOML page parser (commit `84b0ac4`)
   - ✅ Task 2 — Cairo render → `RenderedPage` (commit `13e5de8` + return-type refactor commit)
   - ⏭ **Task 3 — `titleroverlay` GStreamer element** (next)
   - ⏳ Tasks 4–7 — pipeline integration, daemon wiring, SIGHUP reload, Pi smoke test
4. **Known carry-overs documented in the review:**
   - Spec drift: `position = "bottom-left"` 9-anchor preset is in spec §5 but not in `schema = 1`. Decide whether to add to T3's schema bump or annotate spec.
   - Cairo toy text on minimal Pi OS: Task 7 should `fc-match sans` before blaming the pipeline.
5. **Type invariants locked in:** `RenderedPage: Send + Sync` is verified by `rendered_page_is_send_and_sync` compile-trait test in `report/tests/titler_render.rs`. Don't regress this — Task 3's `TitleSlot` depends on it.

---

**Goal:** Add a Videonics-style character generator / titler to PRECRIME's REPORT switcher. The titler renders title pages (text + images + transitions) and composites them as a burn-in overlay on the program output, controlled via the operator's existing USB keyboard with a mode toggle.

**Architecture:** A new `titler` module inside the existing `report` crate. A custom GStreamer element (`titleroverlay`) is inserted into the program pipeline between `input-selector` and `kmssink`. Title pages are TOML files; their rendered RGBA textures live in the daemon and are handed to the custom element. T1 lands the element with a CPU-Cairo composite (no transitions); T4 swaps the internals to GL with a shader that implements the full transition matrix.

**Tech Stack:** Rust stable, GStreamer 1.22+ with `gstreamer-rs`, `gstreamer-base` (subclassing), `cairo-rs`, `serde` + `toml`, `tracing`, `signal-hook` (SIGHUP).

**Spec:** [`docs/specs/2026-05-16-titler-design.md`](../specs/2026-05-16-titler-design.md)

**This plan covers Phase T1 in full implementation detail. Phases T2–T6 are sketched at the end with files, decisions, and risks — each will get its own focused plan when work on it begins.**

**Depends on:** REPORT switcher plan complete through M7 (program pipeline running on Pi with real NDI sources).

---

## Phase T1 — Minimum Titler

**T1 demo:** A single hardcoded title page burns over the live cam feed when the daemon starts; sending the daemon `SIGHUP` reloads the page from disk.

T1 scope deliberately excludes:
- Transitions (cut only; title is either fully on or fully off; T2 adds cut + fade, T4 adds the full matrix)
- Editor UI (T2 adds the library + cursor; T3 adds editing)
- Mode toggle keyboard handling (T2 adds F12)
- Pango font handling (T1 uses cairo native text; T3 swaps in Pango)
- Image / rect layers (T6 adds images; T3 adds rects)
- Asset watcher, SIGHUP only

### File Structure for T1

| Path | Status | Responsibility |
|---|---|---|
| `report/src/titler/mod.rs` | create | titler module entry, re-exports |
| `report/src/titler/page.rs` | create | `Page` model, TOML parser, validation |
| `report/src/titler/render.rs` | create | `render_page` — `Page` → `RenderedPage` (owned ARGB32 bytes + dims, Send+Sync) |
| `report/src/titler/element.rs` | create | custom `BaseTransform` GStreamer element, registration |
| `report/src/lib.rs` | modify | `pub mod titler` |
| `report/src/config.rs` | modify | add `Option<TitlerConfig>` section |
| `report/src/pipeline.rs` | modify | extend `program_pipeline_string` with optional titler element |
| `report/src/daemon.rs` | modify | load page on startup, hand RGBA to element, install SIGHUP handler |
| `report/src/main.rs` | modify | call `titler::element::register` before pipeline init |
| `report/report.conf.example` | modify | add `[titler]` section example |
| `report/Cargo.toml` | modify | add `gstreamer-base`, `signal-hook` deps |
| `report/tests/titler_page.rs` | create | pure-Rust TOML parse tests |
| `report/tests/titler_render.rs` | create | pure-Rust Cairo render output tests (pixel sampling) |
| `precrime/docs/titler-library/default/001-test_page.toml` | create | shipped test page used for T1 demo |
| `precrime/docs/titler-library/README.md` | create | brief note on library layout |

### Task 1: Module scaffold + minimal page schema

**Files:**
- Create: `report/src/titler/mod.rs`
- Create: `report/src/titler/page.rs`
- Modify: `report/src/lib.rs` (add `pub mod titler;`)
- Create: `report/tests/titler_page.rs`

T1 page schema is intentionally tiny — just enough for a single text layer. T3 extends the schema for fonts, shadow, outline, rects; T5 adds masks; T6 adds images. The schema field gates future migrations.

- [ ] **Step 1: Add the `pub mod titler;` line to `report/src/lib.rs`**

Open `report/src/lib.rs` and add the line alphabetically:

```rust
//! REPORT library — exposes modules for integration testing.
pub mod config;
pub mod daemon;
pub mod input;
pub mod mapping;
pub mod naming;
pub mod ndi_find;
pub mod pipeline;
pub mod titler;
```

- [ ] **Step 2: Create empty `report/src/titler/mod.rs`**

Create with content:

```rust
//! Titler subsystem — character generator that burns title pages over the program out.
//!
//! T1 scope: load one TOML page, render to RGBA via Cairo, hand to the custom
//! `titleroverlay` GStreamer element which composites over program video.

pub mod page;
pub mod render;
pub mod element;
```

- [ ] **Step 3: Write failing test for minimal page parser**

Create `report/tests/titler_page.rs` with:

```rust
use pretty_assertions::assert_eq;
use report::titler::page::{parse_page, Layer, Page, Position, Rgba};

#[test]
fn parses_minimal_text_page() {
    let toml = r#"
schema = 1
name = "test_page"

[render]
canvas = { w = 1920, h = 1080 }

[[layer]]
kind = "text"
text = "TEST TITLE"
size = 64.0
color = "#FFFFFF"
position = { x = 100, y = 900 }
"#;

    let page = parse_page(toml).expect("parse");

    assert_eq!(page.schema, 1);
    assert_eq!(page.name, "test_page");
    assert_eq!(page.canvas, (1920, 1080));
    assert_eq!(page.layers.len(), 1);

    match &page.layers[0] {
        Layer::Text(t) => {
            assert_eq!(t.text, "TEST TITLE");
            assert_eq!(t.size, 64.0);
            assert_eq!(t.color, Rgba(0xFF, 0xFF, 0xFF, 0xFF));
            assert_eq!(t.position, Position { x: 100, y: 900 });
        }
    }
}

#[test]
fn rejects_wrong_schema_version() {
    let toml = r#"
schema = 99
name = "future_page"
[render]
canvas = { w = 1920, h = 1080 }
"#;
    let err = parse_page(toml).unwrap_err();
    assert!(format!("{err}").contains("schema"));
}

#[test]
fn rejects_missing_required_fields() {
    let toml = r#"
schema = 1
"#;
    parse_page(toml).unwrap_err();
}

#[test]
fn parses_color_with_alpha() {
    let toml = r#"
schema = 1
name = "p"
[render]
canvas = { w = 1920, h = 1080 }

[[layer]]
kind = "text"
text = "X"
size = 10.0
color = "#80402040"
position = { x = 0, y = 0 }
"#;
    let page = parse_page(toml).expect("parse");
    let Layer::Text(t) = &page.layers[0];
    assert_eq!(t.color, Rgba(0x80, 0x40, 0x20, 0x40));
}
```

- [ ] **Step 4: Run test, verify it fails to compile**

```bash
cd /Users/cody/Dev/precrime
cargo test -p report --test titler_page
```
Expected: compile error — `report::titler::page` doesn't exist.

- [ ] **Step 5: Implement `report/src/titler/page.rs`**

```rust
//! Title page model and TOML parser.

use serde::Deserialize;
use std::fmt;
use thiserror::Error;

pub const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Debug, Error)]
pub enum PageError {
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("unsupported schema version {0} (this binary supports {SUPPORTED_SCHEMA})")]
    UnsupportedSchema(u32),
    #[error("invalid color literal {0:?} (expected #RRGGBB or #RRGGBBAA)")]
    InvalidColor(String),
}

/// A title page: a canvas size plus a list of layers rendered top-down.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub schema: u32,
    pub name: String,
    pub canvas: (u32, u32),
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Layer {
    Text(TextLayer),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLayer {
    pub text: String,
    pub size: f64,
    pub color: Rgba,
    pub position: Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}{:02X}", self.0, self.1, self.2, self.3)
    }
}

/// Parse a page TOML string into a `Page`. Returns `PageError::UnsupportedSchema`
/// if the file's schema doesn't match this binary.
pub fn parse_page(raw: &str) -> Result<Page, PageError> {
    let parsed: TomlPage = toml::from_str(raw)?;
    if parsed.schema != SUPPORTED_SCHEMA {
        return Err(PageError::UnsupportedSchema(parsed.schema));
    }
    let layers: Result<Vec<Layer>, PageError> =
        parsed.layer.into_iter().map(layer_from_toml).collect();
    Ok(Page {
        schema: parsed.schema,
        name: parsed.name,
        canvas: (parsed.render.canvas.w, parsed.render.canvas.h),
        layers: layers?,
    })
}

// ─── private TOML shape ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TomlPage {
    schema: u32,
    name: String,
    render: TomlRender,
    #[serde(default)]
    layer: Vec<TomlLayer>,
}

#[derive(Deserialize)]
struct TomlRender {
    canvas: TomlCanvas,
}

#[derive(Deserialize)]
struct TomlCanvas {
    w: u32,
    h: u32,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TomlLayer {
    Text {
        text: String,
        size: f64,
        color: String,
        position: TomlPos,
    },
}

#[derive(Deserialize)]
struct TomlPos {
    x: i32,
    y: i32,
}

fn layer_from_toml(t: TomlLayer) -> Result<Layer, PageError> {
    match t {
        TomlLayer::Text { text, size, color, position } => Ok(Layer::Text(TextLayer {
            text,
            size,
            color: parse_color(&color)?,
            position: Position { x: position.x, y: position.y },
        })),
    }
}

fn parse_color(s: &str) -> Result<Rgba, PageError> {
    let s = s.strip_prefix('#').ok_or_else(|| PageError::InvalidColor(s.into()))?;
    let (r, g, b, a) = match s.len() {
        6 => (
            u8::from_str_radix(&s[0..2], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            u8::from_str_radix(&s[2..4], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            u8::from_str_radix(&s[4..6], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            0xFF,
        ),
        8 => (
            u8::from_str_radix(&s[0..2], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            u8::from_str_radix(&s[2..4], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            u8::from_str_radix(&s[4..6], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            u8::from_str_radix(&s[6..8], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
        ),
        _ => return Err(PageError::InvalidColor(s.into())),
    };
    Ok(Rgba(r, g, b, a))
}
```

- [ ] **Step 6: Run test, verify it passes**

```bash
cargo test -p report --test titler_page
```
Expected: 4 passed.

- [ ] **Step 7: Commit**

```bash
cd /Users/cody/Dev/precrime
git add report/src/lib.rs report/src/titler/mod.rs report/src/titler/page.rs report/tests/titler_page.rs
git commit -m "feat(titler): add page model + TOML parser (T1 task 1)"
```

### Task 2: Cairo render — page → RGBA buffer

**Files:**
- Create: `report/src/titler/render.rs`
- Create: `report/tests/titler_render.rs`

The renderer takes a `Page`, paints layers into a Cairo `ImageSurface`, and returns the pixels as an owned `RenderedPage { rgba: Arc<Vec<u8>>, width, height, stride }`. `ImageSurface` is `!Send + !Sync`, so the daemon — which shares the rendered output with the GStreamer streaming thread via `TitleSlot` — must hand around bytes, not a raw surface. The element reconstructs an `ImageSurface` from these bytes via `ImageSurface::create_for_data_unsafe` inside `transform_ip`. T1 uses Cairo native `show_text` — no Pango. T3 swaps in Pango.

- [ ] **Step 1: Write failing render test**

Create `report/tests/titler_render.rs`:

```rust
use pretty_assertions::assert_eq;
use report::titler::page::{parse_page, Rgba};
use report::titler::render::render_page;

#[test]
fn render_produces_canvas_sized_buffer() {
    let toml = r#"
schema = 1
name = "t"
[render]
canvas = { w = 320, h = 240 }
"#;
    let page = parse_page(toml).unwrap();
    let rendered = render_page(&page).expect("render");

    assert_eq!(rendered.width, 320);
    assert_eq!(rendered.height, 240);
    assert_eq!(rendered.rgba.len(), rendered.stride as usize * 240);
}

#[test]
fn render_with_no_layers_is_fully_transparent() {
    let toml = r#"
schema = 1
name = "t"
[render]
canvas = { w = 16, h = 16 }
"#;
    let page = parse_page(toml).unwrap();
    let rendered = render_page(&page).expect("render");

    // ARGB32 layout in Cairo: little-endian 4 bytes per pixel (B, G, R, A).
    assert!(rendered.rgba.iter().all(|b| *b == 0), "expected fully transparent buffer");
}

#[test]
fn text_layer_writes_non_transparent_pixels_near_position() {
    let toml = r#"
schema = 1
name = "t"
[render]
canvas = { w = 256, h = 64 }

[[layer]]
kind = "text"
text = "HELLO"
size = 24.0
color = "#FFFFFF"
position = { x = 10, y = 40 }
"#;
    let page = parse_page(toml).unwrap();
    let rendered = render_page(&page).expect("render");

    let stride = rendered.stride as usize;
    let data = &rendered.rgba;
    let mut any_nonzero_alpha = false;
    for y in 20..50 {
        for x in 10..80 {
            let offset = y * stride + x * 4;
            let a = data[offset + 3];
            if a > 0 { any_nonzero_alpha = true; break; }
        }
    }
    assert!(any_nonzero_alpha, "expected some text pixels in the sample region");
}

#[test]
fn text_color_round_trips_through_render() {
    // Render a magenta glyph and verify the most opaque sampled pixel decodes
    // back to a roughly-magenta color after un-premultiplying alpha.
    let toml = r#"
schema = 1
name = "t"
[render]
canvas = { w = 128, h = 32 }

[[layer]]
kind = "text"
text = "M"
size = 20.0
color = "#FF00FF"
position = { x = 30, y = 24 }
"#;
    let page = parse_page(toml).unwrap();
    let rendered = render_page(&page).expect("render");

    let stride = rendered.stride as usize;
    let data = &rendered.rgba;
    let mut best_alpha = 0u8;
    let mut best_rgba = Rgba(0, 0, 0, 0);
    for y in 0..32 {
        for x in 25..70 {
            let offset = y * stride + x * 4;
            let a = data[offset + 3];
            if a > best_alpha {
                best_alpha = a;
                let b = data[offset];
                let g = data[offset + 1];
                let r = data[offset + 2];
                let inv = 255.0 / a as f32;
                best_rgba = Rgba(
                    ((r as f32 * inv).min(255.0)) as u8,
                    ((g as f32 * inv).min(255.0)) as u8,
                    ((b as f32 * inv).min(255.0)) as u8,
                    a,
                );
            }
        }
    }
    assert!(best_alpha > 200, "expected an opaque pixel somewhere in the glyph");
    let Rgba(r, g, b, _) = best_rgba;
    assert!(r > 150, "expected high red, got {r}");
    assert!(g < 80, "expected low green, got {g}");
    assert!(b > 150, "expected high blue, got {b}");
}

#[test]
fn rendered_page_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<report::titler::render::RenderedPage>();
}
```

- [ ] **Step 2: Run test, verify compile failure**

```bash
cargo test -p report --test titler_render
```
Expected: compile error — `report::titler::render` not found.

- [ ] **Step 3: Implement `report/src/titler/render.rs`**

```rust
//! Render a `Page` into an owned ARGB32 RGBA buffer.
//!
//! Returns [`RenderedPage`] — bytes + dimensions — rather than a raw
//! `cairo::ImageSurface` because `ImageSurface` is `!Send + !Sync` and the
//! daemon shares the rendered output with the GStreamer streaming thread.
//! The element reconstructs an `ImageSurface` from these bytes via
//! `ImageSurface::create_for_data_unsafe` when compositing.
//!
//! T1 uses cairo native text. T3 will swap to Pango for proper font handling.

use crate::titler::page::{Layer, Page, Rgba, TextLayer};
use anyhow::{anyhow, Context, Result};
use cairo::{Format, ImageSurface};
use std::sync::Arc;

/// Owned, thread-safe render output. `rgba` is ARGB32 little-endian
/// (bytes B, G, R, A) with premultiplied alpha — Cairo's native layout.
#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub rgba: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub stride: i32,
}

pub fn render_page(page: &Page) -> Result<RenderedPage> {
    let (w, h) = page.canvas;
    let stride = Format::ARgb32
        .stride_for_width(w)
        .map_err(|e| anyhow!("cairo stride_for_width({w}): {e:?}"))?;
    let surface = ImageSurface::create(Format::ARgb32, w as i32, h as i32)
        .context("allocate cairo ARgb32 surface for page")?;
    {
        let ctx = cairo::Context::new(&surface).context("cairo context")?;
        for layer in &page.layers {
            draw_layer(&ctx, layer).context("draw layer")?;
        }
    }
    let rgba = surface
        .take_data()
        .map_err(|e| anyhow!("cairo take_data: {e:?}"))?
        .to_vec();
    Ok(RenderedPage { rgba: Arc::new(rgba), width: w, height: h, stride })
}

fn draw_layer(ctx: &cairo::Context, layer: &Layer) -> Result<()> {
    match layer {
        Layer::Text(t) => draw_text(ctx, t),
    }
}

fn draw_text(ctx: &cairo::Context, t: &TextLayer) -> Result<()> {
    let Rgba(r, g, b, a) = t.color;
    ctx.set_source_rgba(
        r as f64 / 255.0,
        g as f64 / 255.0,
        b as f64 / 255.0,
        a as f64 / 255.0,
    );
    ctx.set_font_size(t.size);
    // Cairo positions text from the baseline at (x, y). T3 swaps to Pango
    // which lets us choose the anchor (top-left etc.) more flexibly.
    ctx.move_to(t.position.x as f64, t.position.y as f64);
    ctx.show_text(&t.text).context("cairo show_text")?;
    Ok(())
}
```

- [ ] **Step 4: Run test, verify pass**

```bash
cargo test -p report --test titler_render
```
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add report/src/titler/render.rs report/tests/titler_render.rs
git commit -m "feat(titler): render page to Cairo ARGB32 buffer (T1 task 2)"
```

### Task 3: Custom GStreamer titler-overlay element

**Files:**
- Modify: `report/Cargo.toml` (add `gstreamer-base`)
- Create: `report/src/titler/element.rs`

This task introduces a custom GStreamer element subclassing `BaseTransform`. The element accepts video frames on its sink pad, composites a held RGBA title image over each frame in-place, and outputs the result. T1's compositing is CPU Cairo blend; T4 replaces the internals with GL.

**Reading reference for the engineer:** [`gstreamer-rs` book — "Element subclassing"](https://gstreamer.freedesktop.org/documentation/rust/stable/latest/docs/gstreamer/subclass/index.html) and the `videofilter` example in `gst-plugins-rs`.

- [ ] **Step 1: Add `gstreamer-base` and `signal-hook` deps to `report/Cargo.toml`**

In the `[dependencies]` block, add:

```toml
gstreamer-base = "0.23"
signal-hook = "0.3"
```

- [ ] **Step 2: Build to confirm dep resolves**

```bash
cargo check -p report
```
Expected: clean build, only existing warnings.

- [ ] **Step 3: Create `report/src/titler/element.rs` with the subclass skeleton**

```rust
//! Custom GStreamer element `titleroverlay`.
//!
//! Subclasses `BaseTransform`. Accepts BGRx / BGRA video and composites a
//! held RGBA title image over each frame in-place. The title image is owned
//! by the daemon and pushed to the element via `TitleSlot`; if no title is
//! set, the element acts as a passthrough.
//!
//! The slot holds [`RenderedPage`] bytes (Send + Sync) rather than a raw
//! `ImageSurface` (which is `!Send + !Sync`). On each frame the element wraps
//! both the held bytes and the video buffer in temporary `ImageSurface`s via
//! `create_for_data_unsafe`, then runs the Cairo blend.
//!
//! T1: CPU Cairo blend. T4: replace with GL via `glupload`/`gldownload` siblings.

use anyhow::Result;
use crate::titler::render::RenderedPage;
use glib::subclass::prelude::*;
use glib::Properties;
use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base::subclass::prelude::*;
use gst_video::VideoFormat;
use parking_lot::Mutex;
use std::sync::Arc;

/// Public handle: shared mutable slot for the current rendered title.
/// The daemon holds a clone of this and updates it via `set`; the element
/// reads from it on each frame.
#[derive(Clone, Default)]
pub struct TitleSlot {
    inner: Arc<Mutex<Option<RenderedPage>>>,
}

impl TitleSlot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the title image. `None` disables overlay (passthrough).
    pub fn set(&self, page: Option<RenderedPage>) {
        *self.inner.lock() = page;
    }

    fn get(&self) -> Option<RenderedPage> {
        self.inner.lock().clone()
    }
}

// ─── glib subclass plumbing ──────────────────────────────────────────────────

glib::wrapper! {
    pub struct TitlerOverlay(ObjectSubclass<imp::TitlerOverlay>)
        @extends gst_base::BaseTransform, gst::Element, gst::Object;
}

/// Register the element with the global GStreamer registry. Call once at startup
/// before constructing any pipeline that references `titleroverlay`.
pub fn register() -> Result<(), glib::BoolError> {
    gst::Element::register(
        None,
        "titleroverlay",
        gst::Rank::NONE,
        TitlerOverlay::static_type(),
    )
}

/// After a pipeline is built with `gst::parse::launch`, look up the element
/// by name and install the daemon's `TitleSlot` so the daemon can push updates.
pub fn install_slot(element: &gst::Element, slot: TitleSlot) -> Result<()> {
    let imp = element
        .downcast_ref::<TitlerOverlay>()
        .ok_or_else(|| anyhow::anyhow!("element is not a TitlerOverlay"))?
        .imp();
    *imp.slot.lock() = Some(slot);
    Ok(())
}

mod imp {
    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::TitlerOverlay)]
    pub struct TitlerOverlay {
        pub(super) slot: Mutex<Option<super::TitleSlot>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TitlerOverlay {
        const NAME: &'static str = "PrecrimeTitlerOverlay";
        type Type = super::TitlerOverlay;
        type ParentType = gst_base::BaseTransform;
    }

    #[glib::derived_properties]
    impl ObjectImpl for TitlerOverlay {}

    impl GstObjectImpl for TitlerOverlay {}

    impl ElementImpl for TitlerOverlay {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            use std::sync::OnceLock;
            static M: OnceLock<gst::subclass::ElementMetadata> = OnceLock::new();
            Some(M.get_or_init(|| {
                gst::subclass::ElementMetadata::new(
                    "PRECRIME Titler Overlay",
                    "Filter/Effect/Video",
                    "Composites a held RGBA title image over each video frame",
                    "PRECRIME",
                )
            }))
        }

        fn pad_templates() -> &'static [gst::PadTemplate] {
            use std::sync::OnceLock;
            static TEMPLATES: OnceLock<Vec<gst::PadTemplate>> = OnceLock::new();
            TEMPLATES.get_or_init(|| {
                let caps = gst::Caps::builder("video/x-raw")
                    .field("format", gst::List::new(["BGRA", "BGRx"]))
                    .field("width", gst::IntRange::new(1, i32::MAX))
                    .field("height", gst::IntRange::new(1, i32::MAX))
                    .field("framerate", gst::FractionRange::new(
                        gst::Fraction::new(0, 1), gst::Fraction::new(i32::MAX, 1)))
                    .build();
                vec![
                    gst::PadTemplate::new(
                        "sink",
                        gst::PadDirection::Sink,
                        gst::PadPresence::Always,
                        &caps,
                    ).unwrap(),
                    gst::PadTemplate::new(
                        "src",
                        gst::PadDirection::Src,
                        gst::PadPresence::Always,
                        &caps,
                    ).unwrap(),
                ]
            })
        }
    }

    impl BaseTransformImpl for TitlerOverlay {
        const MODE: gst_base::subclass::BaseTransformMode = gst_base::subclass::BaseTransformMode::AlwaysInPlace;
        const PASSTHROUGH_ON_SAME_CAPS: bool = false;
        const TRANSFORM_IP_ON_PASSTHROUGH: bool = true;

        fn transform_ip(&self, buf: &mut gst::BufferRef) -> Result<gst::FlowSuccess, gst::FlowError> {
            // Fast path: no title set → passthrough.
            let slot_opt = self.slot.lock().clone();
            let Some(slot) = slot_opt else { return Ok(gst::FlowSuccess::Ok); };
            let Some(title) = slot.get() else { return Ok(gst::FlowSuccess::Ok); };

            // Look up the input video info from the current caps. BaseTransform
            // re-negotiates caps before transform_ip; we need width/height/stride.
            let caps = self.obj().sink_pad().current_caps().ok_or(gst::FlowError::NotNegotiated)?;
            let info = gst_video::VideoInfo::from_caps(&caps).map_err(|_| gst::FlowError::NotNegotiated)?;
            if !matches!(info.format(), VideoFormat::Bgra | VideoFormat::Bgrx) {
                return Err(gst::FlowError::NotNegotiated);
            }

            let mut frame = gst_video::VideoFrameRef::from_buffer_ref_writable(buf, &info)
                .map_err(|_| gst::FlowError::Error)?;
            let stride = frame.plane_stride()[0] as usize;
            let width = info.width() as usize;
            let height = info.height() as usize;
            let data = frame.plane_data_mut(0).map_err(|_| gst::FlowError::Error)?;

            // Wrap both the title bytes and the video buffer as temporary Cairo
            // surfaces. Both are BGRA / pre-multiplied ARGB32 — Cairo treats
            // them identically. The title `Arc<Vec<u8>>` outlives the surface
            // borrow because we hold `title` for the entire scope.
            let title_surface = unsafe {
                cairo::ImageSurface::create_for_data_unsafe(
                    title.rgba.as_ptr() as *mut u8,
                    cairo::Format::ARgb32,
                    title.width as i32,
                    title.height as i32,
                    title.stride,
                ).map_err(|_| gst::FlowError::Error)?
            };
            let video_surface = unsafe {
                cairo::ImageSurface::create_for_data_unsafe(
                    data.as_mut_ptr(),
                    cairo::Format::ARgb32,
                    width as i32,
                    height as i32,
                    stride as i32,
                ).map_err(|_| gst::FlowError::Error)?
            };
            let ctx = cairo::Context::new(&video_surface).map_err(|_| gst::FlowError::Error)?;
            ctx.set_source_surface(&title_surface, 0.0, 0.0).map_err(|_| gst::FlowError::Error)?;
            ctx.paint().map_err(|_| gst::FlowError::Error)?;

            Ok(gst::FlowSuccess::Ok)
        }
    }
}
```

- [ ] **Step 4: Add x86-side smoke test for element registration**

Append to `report/tests/titler_render.rs`:

```rust
#[test]
fn titler_overlay_element_registers() {
    gst::init().expect("gst init");
    report::titler::element::register().expect("register");
    let elem = gst::ElementFactory::make("titleroverlay").build().expect("instantiate");
    assert_eq!(elem.factory().unwrap().name(), "titleroverlay");
}
```

Add the use line at the top:

```rust
use gst::prelude::*;
use gstreamer as gst;
```

- [ ] **Step 5: Run tests, verify pass on x86 dev box**

```bash
cargo test -p report --test titler_render
```
Expected: 5 passed (previous 4 + the new element registration test). If the test fails to link with GStreamer on macOS, gate it the same way `pipeline.rs` integration tests are gated — see existing `#[cfg]` patterns in `report/tests/`. The element body itself runs only on Pi.

- [ ] **Step 6: Commit**

```bash
git add report/Cargo.toml report/src/titler/element.rs report/tests/titler_render.rs
git commit -m "feat(titler): add titleroverlay GStreamer element (T1 task 3)"
```

### Task 4: Pipeline integration

**Files:**
- Modify: `report/src/pipeline.rs` (extend `program_pipeline_string`, take an optional flag)
- Modify: `report/src/main.rs` (call `titler::element::register` at startup, before pipeline construction)

The custom element is inserted into the program pipeline just before the final `videoconvert ! kmssink`. Output of `input-selector` enters the titler element via `videoconvert` (to coerce to BGRA), titler element runs in-place, then `kmssink`.

- [ ] **Step 1: Write failing test for the updated pipeline string**

Open `report/src/pipeline.rs` and append to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn program_pipeline_with_titler_inserts_element_before_kmssink() {
    let s = super::program_pipeline_string_with_titler(&["A".to_string()], 32, true);
    // Element must appear between videoconvert (post-selector) and kmssink.
    let conv_pos = s.find("sel. ! videoconvert").expect("post-selector videoconvert");
    let titler_pos = s.find("titleroverlay name=titler").expect("titler element");
    let kms_pos = s.find("kmssink connector-id=32").expect("kmssink");
    assert!(conv_pos < titler_pos, "titler must follow post-selector videoconvert");
    assert!(titler_pos < kms_pos, "titler must precede kmssink");
    // Must coerce to BGRA before titler element since titler caps require it.
    let coerce_pos = s.find("video/x-raw,format=BGRA").expect("BGRA coercion");
    assert!(coerce_pos < titler_pos, "BGRA coerce must precede titler");
}

#[test]
fn program_pipeline_without_titler_is_unchanged() {
    let s = super::program_pipeline_string_with_titler(&["A".to_string()], 32, false);
    assert!(!s.contains("titleroverlay"), "titler must be absent when disabled");
    let plain = super::program_pipeline_string(&["A".to_string()], 32);
    assert_eq!(s, plain, "disabled-titler form must match the original string");
}
```

- [ ] **Step 2: Run test, verify failure**

```bash
cargo test -p report --lib pipeline::tests
```
Expected: compile error — `program_pipeline_string_with_titler` doesn't exist.

- [ ] **Step 3: Implement the new function and refactor `program_pipeline_string` to call it**

In `report/src/pipeline.rs`, replace the existing `program_pipeline_string` with:

```rust
pub fn program_pipeline_string(source_names: &[String], connector_id: u32) -> String {
    program_pipeline_string_with_titler(source_names, connector_id, false)
}

/// Same as `program_pipeline_string`, but optionally inserts the `titleroverlay`
/// element after the `input-selector` and before `kmssink`. When `with_titler`
/// is true the element is named `titler` so the daemon can look it up.
pub fn program_pipeline_string_with_titler(
    source_names: &[String],
    connector_id: u32,
    with_titler: bool,
) -> String {
    let mut parts = String::from("input-selector name=sel");
    for (i, name) in source_names.iter().enumerate() {
        let escaped = escape_ndi_name(name);
        parts.push_str(&format!(
            r#" ndisrc ndi-name="{escaped}" ! ndisrcdemux name=d{i} d{i}.video ! queue max-size-buffers=4 leaky=downstream ! videoconvert ! sel.sink_{i}"#
        ));
    }
    if with_titler {
        parts.push_str(&format!(
            " sel. ! videoconvert ! video/x-raw,format=BGRA ! titleroverlay name=titler ! videoconvert ! kmssink connector-id={connector_id}"
        ));
    } else {
        parts.push_str(&format!(
            " sel. ! videoconvert ! kmssink connector-id={connector_id}"
        ));
    }
    parts
}
```

Also update `build_program` to accept a `with_titler` flag:

```rust
pub fn build_program(
    source_names: &[String],
    connector_id: u32,
    with_titler: bool,
) -> Result<ProgramPipeline> {
    let parts = program_pipeline_string_with_titler(source_names, connector_id, with_titler);
    let pipeline = gstreamer::parse::launch(&parts)
        .context("parse program pipeline")?
        .downcast::<Pipeline>()
        .map_err(|_| anyhow::anyhow!("downcast to Pipeline"))?;
    let selector = pipeline
        .by_name("sel")
        .context("input-selector element missing")?;
    Ok(ProgramPipeline { pipeline, selector })
}
```

- [ ] **Step 4: Find every call site of `build_program` and update with `false` (no titler) initially**

```bash
grep -n "build_program" report/src/
```

There is one caller in `report/src/daemon.rs` — update it now to pass `false`. The daemon-side wiring in Task 5 will flip it to `true` when a titler config is present.

```rust
// in report/src/daemon.rs, on_sources_changed:
let program = build_program(&new_sources, self.cfg.program_connector_id, false)?;
```

- [ ] **Step 5: Register the titler element in `main.rs` before any pipeline is built**

Open `report/src/main.rs`. Inside `main()`, after `gstreamer::init()` (or before constructing the `Daemon` — find the analogous line), add:

```rust
report::titler::element::register().context("register titleroverlay")?;
```

- [ ] **Step 6: Run all pipeline tests + build**

```bash
cargo test -p report --lib pipeline::tests
cargo build -p report
```
Expected: tests pass, build clean.

- [ ] **Step 7: Commit**

```bash
git add report/src/pipeline.rs report/src/daemon.rs report/src/main.rs
git commit -m "feat(titler): wire titleroverlay into program pipeline (T1 task 4)"
```

### Task 5: Daemon wiring — load page + push to element

**Files:**
- Modify: `report/src/config.rs` (add `Option<TitlerConfig>`)
- Modify: `report/src/daemon.rs` (load + render page on startup, install `TitleSlot` on element)
- Modify: `report/report.conf.example` (`[titler]` section)
- Create: `precrime/docs/titler-library/default/001-test_page.toml`
- Create: `precrime/docs/titler-library/README.md`

- [ ] **Step 1: Write failing config-parse test**

In `report/tests/config.rs`, append:

```rust
#[test]
fn parses_titler_config() {
    let raw = r#"
program_connector_id = 32
preview_connector_id = 34
keyboard_device = "/dev/input/event0"

[titler]
library_dir = "/etc/precrime/titler/library/default"
initial_page = "001-test_page"
"#;
    let cfg = ReportConfig::from_toml(raw).expect("parse");
    let t = cfg.titler.expect("titler section present");
    assert_eq!(t.library_dir, std::path::PathBuf::from("/etc/precrime/titler/library/default"));
    assert_eq!(t.initial_page, "001-test_page");
}

#[test]
fn titler_section_is_optional() {
    let raw = r#"
program_connector_id = 32
preview_connector_id = 34
keyboard_device = "/dev/input/event0"
"#;
    let cfg = ReportConfig::from_toml(raw).expect("parse");
    assert!(cfg.titler.is_none());
}
```

- [ ] **Step 2: Run test, verify failure**

```bash
cargo test -p report --test config
```
Expected: failure — `titler` field doesn't exist.

- [ ] **Step 3: Add `TitlerConfig` to `report/src/config.rs`**

```rust
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct ReportConfig {
    pub program_connector_id: u32,
    pub preview_connector_id: u32,
    pub keyboard_device: String,
    #[serde(default)]
    pub source_slot_overrides: HashMap<String, u8>,
    #[serde(default)]
    pub titler: Option<TitlerConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TitlerConfig {
    pub library_dir: PathBuf,
    pub initial_page: String,
}
```

- [ ] **Step 4: Run test, verify pass**

```bash
cargo test -p report --test config
```
Expected: pass.

- [ ] **Step 5: Wire titler into the daemon**

In `report/src/daemon.rs`:

Add to `DaemonState`:
```rust
struct DaemonState {
    sources_in_order: Vec<String>,
    active_slot: Option<u8>,
    program: Option<ProgramPipeline>,
    preview: Option<PreviewPipeline>,
    title_slot: Option<crate::titler::element::TitleSlot>,
}
```

Initialize it in `Daemon::new`:
```rust
state: Arc::new(Mutex::new(DaemonState {
    sources_in_order: Vec::new(),
    active_slot: None,
    program: None,
    preview: None,
    title_slot: None,
})),
```

Add a method that loads the initial page and stores its rendered surface into the title slot. Call it after `gstreamer::init()` in `run()`:

```rust
fn load_initial_title(&self) -> Result<Option<crate::titler::element::TitleSlot>> {
    use crate::titler::{element::TitleSlot, page::parse_page, render::render_page};
    let Some(t) = &self.cfg.titler else { return Ok(None); };
    let path = t.library_dir.join(format!("{}.toml", t.initial_page));
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read initial title page: {}", path.display()))?;
    let page = parse_page(&raw).context("parse initial title page")?;
    let rendered = render_page(&page).context("render initial title page")?;
    let slot = TitleSlot::new();
    slot.set(Some(rendered));
    info!(page = %t.initial_page, "loaded initial title page");
    Ok(Some(slot))
}
```

In `run()`, after `gstreamer::init()`:
```rust
let title_slot = self.load_initial_title()?;
self.state.lock().title_slot = title_slot;
```

In `on_sources_changed`, change the `build_program` call to pass the titler flag and install the slot on the element:

```rust
let with_titler = self.cfg.titler.is_some();
let program = build_program(&new_sources, self.cfg.program_connector_id, with_titler)?;
if with_titler {
    let titler_elem = program
        .pipeline
        .by_name("titler")
        .context("titler element missing from program pipeline")?;
    let slot = self.state.lock()
        .title_slot
        .as_ref()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("title_slot not loaded"))?;
    crate::titler::element::install_slot(&titler_elem, slot)?;
}
spawn_bus_watch("program", &program.pipeline, bus_tx.clone())?;
```

- [ ] **Step 6: Update `report.conf.example`**

Append to `report/report.conf.example`:
```toml
# Optional: enable the burn-in titler.
# Comment this block out to disable the titler entirely.
# [titler]
# library_dir = "/etc/precrime/titler/library/default"
# initial_page = "001-test_page"
```

- [ ] **Step 7: Create the shipped test page**

Create directory and file: `precrime/docs/titler-library/default/001-test_page.toml`

```toml
schema = 1
name = "test_page"

[render]
canvas = { w = 1920, h = 1080 }

[[layer]]
kind = "text"
text = "PRECRIME TITLER — T1"
size = 96.0
color = "#FFFFFFFF"
position = { x = 80, y = 1000 }
```

Create `precrime/docs/titler-library/README.md`:

```markdown
# Titler library

Title pages live here as TOML files. Each page is one file under
`default/` (or a per-show subdirectory). Filename convention:
`NNN-slug.toml`, e.g. `001-lower_third_speaker.toml`.

Deploy to the Pi at `/etc/precrime/titler/library/`.

Schema reference: `docs/specs/2026-05-16-titler-design.md` §5.
```

- [ ] **Step 8: Build + run unit tests**

```bash
cargo build -p report
cargo test -p report --lib
cargo test -p report --tests
```
Expected: clean build, all tests pass.

- [ ] **Step 9: Commit**

```bash
git add report/src/config.rs report/src/daemon.rs report/report.conf.example \
        report/tests/config.rs docs/titler-library/
git commit -m "feat(titler): load + push initial page from config (T1 task 5)"
```

### Task 6: SIGHUP reload

**Files:**
- Modify: `report/src/daemon.rs` (install SIGHUP handler, reload-and-push on signal)
- Modify: `report/src/main.rs` (already-wired `signal-hook` dep used here)

`SIGHUP` is the standard "reload your config" signal. T1 uses it as the only way to refresh the title page from disk; T6 will add a filesystem watcher for the same job.

- [ ] **Step 1: Add SIGHUP setup to `Daemon::run`**

In `report/src/daemon.rs`, near the top of `run()`, after `gstreamer::init()` and `load_initial_title`, install a signal channel:

```rust
use signal_hook::consts::SIGHUP;
use signal_hook::iterator::Signals;

let (hup_tx, hup_rx) = channel::<()>();
{
    let mut signals = Signals::new([SIGHUP]).context("install SIGHUP handler")?;
    std::thread::Builder::new()
        .name("report-sighup".into())
        .spawn(move || {
            for _ in signals.forever() {
                if hup_tx.send(()).is_err() {
                    break;
                }
            }
        })?;
}
```

Pass `hup_rx` into `event_loop` (update the signature and the call).

- [ ] **Step 2: Add the reload handler**

In `event_loop`, drain the hup channel each tick:

```rust
while let Ok(()) = hup_rx.try_recv() {
    if let Err(e) = self.reload_title() {
        warn!(error = ?e, "title reload failed");
    } else {
        info!("title reloaded on SIGHUP");
    }
}
```

Implement `reload_title`:

```rust
fn reload_title(&self) -> Result<()> {
    use crate::titler::{page::parse_page, render::render_page};
    let Some(t) = &self.cfg.titler else { return Ok(()); };
    let path = t.library_dir.join(format!("{}.toml", t.initial_page));
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read title page: {}", path.display()))?;
    let page = parse_page(&raw).context("parse title page")?;
    let rendered = render_page(&page).context("render title page")?;
    let slot = self.state.lock().title_slot.as_ref().cloned();
    if let Some(slot) = slot {
        slot.set(Some(rendered));
    }
    Ok(())
}
```

- [ ] **Step 3: Build + run all tests**

```bash
cargo build -p report
cargo test -p report
```
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add report/src/daemon.rs
git commit -m "feat(titler): SIGHUP reloads title page from disk (T1 task 6)"
```

### Task 7: End-to-end smoke test on Pi

**Files (no edits, this task is verification):**
- Inspect: `target/aarch64-unknown-linux-gnu/release/report` (built artifact)
- Verify: `journalctl -u report` for tracing output
- Verify: visual burn-in on HDMI-A-1

This task runs on the actual Pi (REPORT) connected to a monitor on HDMI-A-1 with at least one live PRECOG NDI source on the LAN.

- [ ] **Step 1: Cross-compile or build on the Pi**

If building on the Pi:
```bash
cd /home/cody/precrime
cargo build --release -p report
```

If cross-compiling from x86, follow the existing REPORT runbook for cross-compile setup (this plan doesn't change that flow).

- [ ] **Step 2: Deploy the test page to the Pi**

```bash
ssh report 'sudo mkdir -p /etc/precrime/titler/library/default'
scp precrime/docs/titler-library/default/001-test_page.toml \
    report:/tmp/001-test_page.toml
ssh report 'sudo mv /tmp/001-test_page.toml /etc/precrime/titler/library/default/'
```

- [ ] **Step 3: Update `/etc/precrime/report.conf` on the Pi to enable titler**

```bash
ssh report 'sudo nano /etc/precrime/report.conf'
```

Uncomment and set:
```toml
[titler]
library_dir = "/etc/precrime/titler/library/default"
initial_page = "001-test_page"
```

- [ ] **Step 4: Deploy the new `report` binary and restart**

```bash
scp target/aarch64-unknown-linux-gnu/release/report report:/tmp/report
ssh report 'sudo install -m755 /tmp/report /usr/local/bin/report && sudo systemctl restart report'
```

- [ ] **Step 5: Verify title is burned over HDMI-A-1 output**

Look at the monitor connected to HDMI-A-1. The text "PRECRIME TITLER — T1" should appear in white near the bottom-left of the program output, regardless of which camera is currently switched live.

- [ ] **Step 6: Verify SIGHUP reload**

Edit the title page on the Pi:
```bash
ssh report 'sudo sed -i "s/PRECRIME TITLER — T1/PRECRIME RELOADED/" /etc/precrime/titler/library/default/001-test_page.toml'
ssh report 'sudo systemctl kill --signal=SIGHUP report'
```

The on-screen text should change to "PRECRIME RELOADED" within ~1 second.

- [ ] **Step 7: Capture latency observation**

Use a stopwatch or a 60fps phone camera comparing source-monitor to program-monitor: estimate added latency. Note in the runbook. Expected: ≤1 frame added by titler.

- [ ] **Step 8: Update REPORT runbook with titler section**

Append a "Titler" section to `report/runbook.md` documenting:
- How to deploy a title page
- The SIGHUP reload procedure
- How to disable the titler (comment out `[titler]` and restart)

- [ ] **Step 9: Commit any runbook changes and tag the milestone**

```bash
git add report/runbook.md
git commit -m "docs(titler): T1 runbook section + smoke test procedure"
git tag titler-t1
```

---

## Phase T2 — Mode + library + cut/fade (sketch)

**T2 demo:** Operator presses F12 to flip HDMI-A-2 from multiview to title editor (list-only view). Selects a page via `↑`/`↓`. Presses F12 to flip back. Presses `Enter` to take title on-air with a fade transition; `Esc` to fade out.

### Files

| Path | Status | Responsibility |
|---|---|---|
| `report/src/titler/library.rs` | create | scan `library_dir/*.toml`, sort by filename, expose cursor |
| `report/src/titler/transition.rs` | create | transition state machine (cut, fade in, fade out — phase + time) |
| `report/src/titler/editor.rs` | create | DRM modeset on HDMI-A-2 + Cairo render of list view |
| `report/src/titler/mode.rs` | create | `Mode` enum, transition rules |
| `report/src/input.rs` | modify | two-layer key dispatch (show-ops always active; edit only when in Titler mode) |
| `report/src/daemon.rs` | modify | mode state, swap pipelines on F12, on-air flag in title slot |
| `report/src/titler/element.rs` | modify | element now reads `(image, alpha)` from slot, not just image |

### Key decisions from spec

- **Pipeline B (preview) is torn down on flip to Titler mode**; rebuild on flip back. T2 needs to measure rebuild latency (spec §8 open question).
- **Two-layer keyboard:** `report::input` adds a `Mode` parameter to its dispatch. `Enter`/`Esc`/`[`/`]`/`F12` are show-ops (always active); arrow keys are edit-only.
- **Title slot extended:** `TitleSlot` now holds `Option<(RenderedPage, f32 alpha)>`. Element samples alpha when blending (uniform scale factor on the Cairo paint operation).
- **Cut + fade only:** transition state machine produces `(image_a, image_b, phase, type)` — for T2, only `type=cut | fade_in | fade_out`. T4 adds wipes/dissolves.

### Risks

- DRM connector release/reclaim on mode flip: `kmssink`'s release behavior under `set_state(Null)` may be slow or leak the connector. Mitigation candidates: keep preview pipeline paused rather than torn down (test whether `kmssink` releases the modeset in PAUSED), or pre-create the editor surface and only `drmModeSetCrtc` on flip.
- Cairo direct DRM modeset code path is new in REPORT; existing `kmssink` use was always GStreamer-driven. The first 1–2 tasks in T2 should be a standalone Cairo+DRM hello-world that draws a single static frame to HDMI-A-2.

### Out of scope (defer to T3+)

- Editing fields, typing, formatting
- Save / load semantics
- Per-page transitions

---

## Phase T3 — Edit, save, formatting (sketch)

**T3 demo:** Operator creates a new lower-third from scratch on the Pi — types text, picks a font, picks color, positions it, saves, takes it on-air.

### Files

| Path | Status | Responsibility |
|---|---|---|
| `report/src/titler/render.rs` | modify | swap from cairo native text to Pango via `pangocairo` |
| `report/src/titler/page.rs` | modify | schema additions: font field, shadow, outline, rect layers, 9-anchor position |
| `report/src/titler/library.rs` | modify | save-to-disk path, dirty-buffer tracking |
| `report/src/titler/editor/widgets.rs` | create | hand-rolled text input, focus state, list scrolling |
| `report/src/titler/editor.rs` | modify | layout the page editor pane, route arrow/Tab keys to widgets |
| `report/src/titler/fonts.rs` | create | scan `fonts/` dir at startup, register with FontConfig |

### Key decisions

- **Pango replaces cairo native text** in the renderer at this phase. Page rendering output should be visually identical for ASCII-only text but improves accents, kerning, and antialiasing.
- **Hand-rolled widgets** per spec §4 — no GTK / egui. Widget primitives: `TextField`, `ListView`, `Button`, `Toggle`. Each is ~50–100 lines of Cairo + key dispatch.
- **Per-page schema extension** is backwards-compatible: T1's pages still parse with the new schema.

### Risks

- Pango FontConfig setup on Pi OS may need an explicit `fc-cache -fv` on first deploy.
- Hand-rolled text input is fiddly — IME / Unicode composition is out of scope for v1, ASCII only.

---

## Phase T4 — Full geometric transition matrix (sketch)

**T4 demo:** Operator picks dissolve / horizontal wipe / vertical wipe / diagonal wipe / push / slide on a per-page basis, with adjustable feather width. Every Videonics-era geometric transition selectable.

### Files

| Path | Status | Responsibility |
|---|---|---|
| `report/src/titler/element.rs` | major refactor | swap CPU Cairo blend for GL: `glupload`, fragment shader, `gldownload` siblings inside the element |
| `report/src/titler/element/shader.frag` | create | uber-shader: branches on `wipe_type` uniform, computes feathered transition between texture A and texture B |
| `report/src/titler/transition.rs` | modify | extend state machine: dissolve, wipe_h/v/d, push, slide |
| `report/src/titler/editor.rs` | modify | transition picker modal (open with `T`), duration + feather sliders |
| `report/src/titler/page.rs` | modify | `[transition]` schema gains all type variants |

### Key decisions

- **Custom element transitions to GL** here, not earlier. T1–T3 lived on CPU Cairo blend; T4 swaps the internals while keeping the element's external interface (caps, name, pad layout) stable. Anything outside `element.rs` should not need to change.
- **Uber-shader** with dynamic `wipe_type` uniform branching — Pi 5 VC7 handles this without stalls.
- **EGLImage** import from V4L2 DMA-BUF: prototype standalone before integrating. Spec §8 open question on texture-unit count gets answered here.

### Risks

- This is the highest-risk phase architecturally. Standalone EGLImage + shader prototype is required before touching the element. Plan for a 1–2 week spike.
- GStreamer GL elements have specific caps requirements (`memory:GLMemory` features). The pipeline string changes accordingly.

---

## Phase T5 — Shaped wipes + motion (sketch)

**T5 demo:** Iris wipe, heart wipe, star wipe. Scrolling credits roll. Horizontal ticker crawl.

### Files

| Path | Status | Responsibility |
|---|---|---|
| `report/src/titler/element/shader.frag` | modify | add mask-texture sampling branch for shaped wipes; add scroll-offset path for crawl/roll |
| `report/src/titler/masks.rs` | create | scan `masks/` dir, load PNGs as GL textures, cache by name |
| `report/src/titler/page.rs` | modify | `[transition].mask` and `[motion]` schema additions |
| `report/src/titler/transition.rs` | modify | shaped transition type; motion state (continuous scroll) |
| `report/src/titler/editor.rs` | modify | mask picker; speed adjust hotkeys (`{` / `}`) |
| `precrime/docs/titler-library/masks/` | create | ship 12 default mask PNGs |

### Key decisions

- **Mask textures are grayscale R8** for memory efficiency (256×256 × 1 byte = 64 KB each, 12 shipped = 768 KB total).
- **Shaped wipe shader path** is one extra `texture()` call and a `smoothstep` — minimal added cost.
- **Crawl / roll** are implemented as a uv-offset on the title texture sample, computed from `time * speed_pps / canvas_size`. Loop modulo by canvas width/height.

### Risks

- Mask PNG authoring: ship 12 stock masks at 256×256, ensure they're sharp enough to look intentional on a 1080p output.
- Motion text needs to be rendered at higher resolution than the visible area (so the off-screen part is ready to scroll in). Memory cost manageable at 1080p.

---

## Phase T6 — Assets, polish, live-edit (sketch)

**T6 demo:** Sponsor logo bug in the corner. Mid-show typo fix without taking off-air. Producer-mode multiview gains a title tally strip.

### Files

| Path | Status | Responsibility |
|---|---|---|
| `report/src/titler/page.rs` | modify | `image` layer kind with src + opacity |
| `report/src/titler/render.rs` | modify | load PNGs via Cairo `ImageSurface::from_png_read`, draw to page surface |
| `report/src/titler/assets.rs` | create | asset cache + `notify` filesystem watcher |
| `report/src/titler/library.rs` | modify | live-edit toggle in `TitleSlot` (skip transition, push immediately on edit) |
| `report/src/titler/editor.rs` | modify | staged-changes status indicator; `Ctrl+L` live-edit toggle |
| `report/src/pipeline.rs` | modify | multiview compositor gains bottom-row title tally strip |
| `report/runbook.md` | modify | document mode toggle, keymap, library layout |

### Key decisions

- **`notify` filesystem watcher** for `assets/` with 5s rescan safety net per spec §5.
- **Live-edit toggle is state on `TitleSlot`**: when on, the element bypasses the transition state machine and uses the latest image directly.
- **Multiview tally strip** reuses the existing `cairooverlay` callback in `pipeline.rs`. Add a second draw step below the existing tile-border rect.

### Risks

- `notify` event drops under Pi I/O load → 5s rescan task as defense in depth.
- Multiview cairo callback grows in complexity; consider extracting tally / title-strip rendering into its own module before adding to it.

---

## Self-Review Notes

**Spec coverage:**
- §1 hardware reuses REPORT — task list reflects no new hardware
- §2 pipeline integration — Tasks 3, 4 land the custom element + pipeline string
- §3 feature parity — T1 covers static-text-burn-in only; later T-phases cover the matrix; each row has a phase pointer
- §4 UI / edit flow — T2 / T3 sketches cover mode, two-layer keyboard, editor layout
- §5 storage / TOML — Task 1 implements parser; Tasks 5+ extend schema as features land
- §6 phasing — plan structure matches T1–T6
- §7 failure modes — T1 covers TOML parse error (returns to caller; daemon logs); shader / DRM failure modes covered in later phases
- §8 open questions — TOML-cache + DRM handoff carried forward into T2/T4

**Placeholder scan:** no TBD/TODO. Every T1 step has actual code or commands.

**Type consistency:**
- `RenderedPage` defined in Task 2 (render.rs), held by `TitleSlot` in Task 3 (element.rs), produced by daemon in Task 5/6 — names + types match.
- `TitleSlot` defined in Task 3 (element.rs), referenced in Task 5 (daemon.rs) — names match.
- `parse_page` / `render_page` / `register` / `install_slot` — all referenced names are defined in earlier tasks.
- `program_pipeline_string_with_titler` defined Task 4, no other callers.

**Why `RenderedPage` instead of `ImageSurface`:** cairo-rs's `ImageSurface` is `!Send + !Sync` (wraps `NonNull<cairo_surface_t>`). The slot is shared between the daemon thread and the GStreamer streaming thread, so it must hold a `Send + Sync` value. `RenderedPage` stores the rendered bytes in `Arc<Vec<u8>>`; the element wraps both the title bytes and the video buffer as temporary `ImageSurface`s via `create_for_data_unsafe` inside `transform_ip`. A `rendered_page_is_send_and_sync` compile-trait test in `report/tests/titler_render.rs` locks the invariant.

**Known gaps for the executor:**
- The existing `build_program` signature is changing (added bool arg). Task 4 Step 4 explicitly updates the one known caller in `daemon.rs`; if a workspace search reveals additional callers later, update them with `false` initially.
- macOS `cargo test` may not link the GStreamer element test; gate per existing repo conventions.
