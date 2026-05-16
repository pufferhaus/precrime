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
/// Construct the program-out gst-launch pipeline string. Pure function for testing.
pub fn program_pipeline_string(source_names: &[String], connector_id: u32) -> String {
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
    parts
}

pub fn build_program(source_names: &[String], connector_id: u32) -> Result<ProgramPipeline> {
    let parts = program_pipeline_string(source_names, connector_id);
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
/// Construct the multiview gst-launch pipeline string. Pure function for testing.
/// Returns the pipeline string. For zero sources, returns a black-test-pattern fallback.
pub fn preview_pipeline_string(source_names: &[String], connector_id: u32) -> String {
    if source_names.is_empty() {
        return format!(
            "videotestsrc pattern=black is-live=true ! video/x-raw,width=1920,height=1080 ! videoconvert ! kmssink connector-id={connector_id}"
        );
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
    s
}

pub fn build_preview(
    source_names: &[String],
    connector_id: u32,
    get_active_slot: Arc<dyn Fn() -> Option<u8> + Send + Sync>,
) -> Result<PreviewPipeline> {
    let s = preview_pipeline_string(source_names, connector_id);
    if source_names.is_empty() {
        let pipeline = gstreamer::parse::launch(&s)?
            .downcast::<Pipeline>()
            .map_err(|_| anyhow::anyhow!("downcast"))?;
        return Ok(PreviewPipeline { pipeline });
    }

    let n = source_names.len();
    let (cols, rows) = grid_for(n);
    let tile_w: u32 = 1920 / cols as u32;
    let tile_h: u32 = 1080 / rows as u32;

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

/// Escape an NDI source name for embedding inside a double-quoted
/// gst-parse string. Backslash must be escaped first so the subsequent
/// quote-escape doesn't get re-consumed by the parser.
fn escape_ndi_name(name: &str) -> String {
    name.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::{escape_ndi_name, preview_pipeline_string, program_pipeline_string};

    #[test]
    fn escapes_backslash_before_quote() {
        assert_eq!(escape_ndi_name(r"CAM\1"), r"CAM\\1");
        assert_eq!(escape_ndi_name(r#"CAM"X"#), r#"CAM\"X"#);
        assert_eq!(escape_ndi_name(r#"CAM\"X"#), r#"CAM\\\"X"#);
    }

    #[test]
    fn ordinary_names_unchanged() {
        assert_eq!(
            escape_ndi_name("PRECOG-01-IPHONE-STAGE"),
            "PRECOG-01-IPHONE-STAGE"
        );
    }

    #[test]
    fn program_pipeline_zero_sources() {
        let s = program_pipeline_string(&[], 32);
        assert_eq!(
            s,
            "input-selector name=sel sel. ! videoconvert ! kmssink connector-id=32"
        );
    }

    #[test]
    fn program_pipeline_one_source() {
        let s = program_pipeline_string(&["PRECOG-01-IPHONE-STAGE".to_string()], 32);
        assert_eq!(
            s,
            r#"input-selector name=sel ndisrc ndi-name="PRECOG-01-IPHONE-STAGE" ! ndisrcdemux name=d0 d0.video ! queue max-size-buffers=4 leaky=downstream ! videoconvert ! sel.sink_0 sel. ! videoconvert ! kmssink connector-id=32"#
        );
    }

    #[test]
    fn program_pipeline_two_sources() {
        let s = program_pipeline_string(
            &[
                "PRECOG-01-IPHONE-STAGE".to_string(),
                "PRECOG-02-CCTV-DOOR".to_string(),
            ],
            32,
        );
        assert!(s.starts_with("input-selector name=sel"));
        assert!(s.contains(r#"ndisrc ndi-name="PRECOG-01-IPHONE-STAGE""#));
        assert!(s.contains(r#"ndisrc ndi-name="PRECOG-02-CCTV-DOOR""#));
        assert!(s.contains("sel.sink_0"));
        assert!(s.contains("sel.sink_1"));
        assert!(s.ends_with("kmssink connector-id=32"));
    }

    #[test]
    fn preview_pipeline_zero_sources_is_black_test_pattern() {
        let s = preview_pipeline_string(&[], 34);
        assert!(s.starts_with("videotestsrc pattern=black is-live=true"));
        assert!(s.contains("width=1920,height=1080"));
        assert!(s.ends_with("kmssink connector-id=34"));
        assert!(!s.contains("compositor"));
        assert!(!s.contains("ndisrc"));
    }

    #[test]
    fn preview_pipeline_one_source_uses_1x1_grid() {
        let s = preview_pipeline_string(&["A".to_string()], 34);
        assert!(s.starts_with("compositor name=mix background=black"));
        assert!(s.contains("sink_0::xpos=0"));
        assert!(s.contains("sink_0::ypos=0"));
        assert!(s.contains("sink_0::width=1920"));
        assert!(s.contains("sink_0::height=1080"));
    }

    #[test]
    fn preview_pipeline_four_sources_uses_2x2_grid() {
        let sources: Vec<String> = (0..4).map(|i| format!("S{i}")).collect();
        let s = preview_pipeline_string(&sources, 34);
        assert!(s.contains("sink_0::xpos=0 sink_0::ypos=0 sink_0::width=960 sink_0::height=540"));
        assert!(s.contains("sink_1::xpos=960 sink_1::ypos=0 sink_1::width=960 sink_1::height=540"));
        assert!(
            s.contains("sink_2::xpos=0 sink_2::ypos=540 sink_2::width=960 sink_2::height=540")
        );
        assert!(s.contains(
            "sink_3::xpos=960 sink_3::ypos=540 sink_3::width=960 sink_3::height=540"
        ));
    }

    #[test]
    fn preview_pipeline_nine_sources_uses_3x3_grid() {
        let sources: Vec<String> = (0..9).map(|i| format!("S{i}")).collect();
        let s = preview_pipeline_string(&sources, 34);
        // tile is 640x360 (1920/3 by 1080/3)
        assert!(s.contains("sink_0::xpos=0 sink_0::ypos=0 sink_0::width=640 sink_0::height=360"));
        assert!(s.contains("sink_4::xpos=640 sink_4::ypos=360 sink_4::width=640 sink_4::height=360"));
        assert!(s.contains(
            "sink_8::xpos=1280 sink_8::ypos=720 sink_8::width=640 sink_8::height=360"
        ));
    }

    #[test]
    fn preview_pipeline_ends_with_cairo_overlay_and_kmssink() {
        let s = preview_pipeline_string(&["A".to_string()], 34);
        assert!(s.ends_with("mix. ! videoconvert ! cairooverlay name=tally ! videoconvert ! kmssink connector-id=34"));
    }
}
