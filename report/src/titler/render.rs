//! Render a `Page` into a Cairo `ImageSurface` (ARGB32 RGBA buffer).
//!
//! T1 uses cairo native text. T3 will swap to Pango for proper font handling.

use crate::titler::page::{Layer, Page, Rgba, TextLayer};
use anyhow::{Context, Result};
use cairo::{Format, ImageSurface};

/// Render the page to a freshly-allocated ARGB32 `ImageSurface`.
/// Surface dimensions match `page.canvas`. Caller owns the returned surface.
pub fn render_page(page: &Page) -> Result<ImageSurface> {
    let (w, h) = page.canvas;
    let surface = ImageSurface::create(Format::ARgb32, w as i32, h as i32)
        .context("allocate cairo ARgb32 surface for page")?;
    {
        let ctx = cairo::Context::new(&surface).context("cairo context")?;
        // Transparent default — nothing drawn yet.
        for layer in &page.layers {
            draw_layer(&ctx, layer).context("draw layer")?;
        }
    }
    Ok(surface)
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
