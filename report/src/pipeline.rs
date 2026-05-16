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

/// `get_active_slot` is invoked from the cairo draw callback on every frame and
/// should return the 1-based slot of the currently-program source, or None if
/// no source is selected.
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

    let cb = get_active_slot.clone();
    overlay.connect("draw", true, move |args| {
        // args: [element, cairo_t_ptr_as_boxed, timestamp_u64, duration_u64]
        // cairo::Context does not implement glib::value::FromValue; extract via raw pointer.
        // SAFETY: GStreamer's cairooverlay passes a valid cairo_t* as a boxed glib value.
        let ctx = unsafe {
            let ptr = gstreamer::glib::gobject_ffi::g_value_get_boxed(
                gstreamer::glib::translate::ToGlibPtr::to_glib_none(&args[1]).0,
            ) as *mut cairo::ffi::cairo_t;
            cairo::Context::from_raw_borrow(ptr)
        };
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
    name.replace('"', "")
}
