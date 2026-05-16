//! Render a `Page` into an owned ARGB32 RGBA buffer.
//!
//! Returns [`RenderedPage`] — bytes + dimensions — rather than a raw
//! `cairo::ImageSurface` because `ImageSurface` is `!Send + !Sync` and the
//! daemon shares the rendered output with the GStreamer streaming thread.
//! The element reconstructs an `ImageSurface` from these bytes in-place via
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

/// Render the page and return its pixels as an owned, shareable buffer.
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
    Ok(RenderedPage {
        rgba: Arc::new(rgba),
        width: w,
        height: h,
        stride,
    })
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
