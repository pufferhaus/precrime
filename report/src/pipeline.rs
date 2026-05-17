//! GStreamer pipeline builders for REPORT.
//!
//! Two independent pipelines, each rendering directly to a DRM/KMS connector:
//! - `program`: input-selector over N udpsrc RTP inputs → kmssink (HDMI-A-1)
//! - `preview`: compositor (2x2 or 3x3 grid) → cairooverlay tally → kmssink (HDMI-A-2)

use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer::{Element, Pipeline};
use std::sync::Arc;

/// One discovered source as seen by the pipeline builders. Cloned from the
/// `temple::Ball` payload at the moment pipelines are rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub mcast: String, // IPv4 multicast group, e.g. "239.42.1.1"
    pub port: u16,
    pub payload_type: u8,
    pub clock_rate: u32,
    pub encoding_name: String, // "H264"
}

pub struct ProgramPipeline {
    pub pipeline: Pipeline,
    pub selector: Element,
}

/// Construct the program-out gst-launch pipeline string. Pure function for testing.
pub fn program_pipeline_string(sources: &[Source], connector_id: u32) -> String {
    let mut parts = String::from("input-selector name=sel");
    for (i, s) in sources.iter().enumerate() {
        parts.push_str(&format!(
            " udpsrc address={mcast} port={port} auto-multicast=true \
             caps=\"application/x-rtp,media=video,clock-rate={cr},encoding-name={enc},payload={pt}\" ! \
             rtpjitterbuffer latency=20 ! \
             rtph264depay ! h264parse ! avdec_h264 ! \
             queue max-size-buffers=4 leaky=downstream ! \
             videoconvert ! sel.sink_{i}",
            mcast = s.mcast,
            port = s.port,
            cr = s.clock_rate,
            enc = s.encoding_name,
            pt = s.payload_type,
        ));
    }
    parts.push_str(&format!(
        " sel. ! videoconvert ! kmssink connector-id={connector_id}"
    ));
    parts
}

pub fn build_program(sources: &[Source], connector_id: u32) -> Result<ProgramPipeline> {
    let parts = program_pipeline_string(sources, connector_id);
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
pub fn preview_pipeline_string(sources: &[Source], connector_id: u32) -> String {
    if sources.is_empty() {
        return format!(
            "videotestsrc pattern=black is-live=true ! video/x-raw,width=1920,height=1080 ! videoconvert ! kmssink connector-id={connector_id}"
        );
    }

    let n = sources.len();
    let (cols, rows) = grid_for(n);
    let tile_w: u32 = 1920 / cols as u32;
    let tile_h: u32 = 1080 / rows as u32;

    let mut s = String::from("compositor name=mix background=black");
    for (i, _) in sources.iter().enumerate() {
        let col = (i % cols) as u32;
        let row = (i / cols) as u32;
        s.push_str(&format!(
            " sink_{i}::xpos={x} sink_{i}::ypos={y} sink_{i}::width={tile_w} sink_{i}::height={tile_h}",
            x = col * tile_w,
            y = row * tile_h,
        ));
    }
    for (i, src) in sources.iter().enumerate() {
        s.push_str(&format!(
            " udpsrc address={mcast} port={port} auto-multicast=true \
             caps=\"application/x-rtp,media=video,clock-rate={cr},encoding-name={enc},payload={pt}\" ! \
             rtpjitterbuffer latency=20 ! \
             rtph264depay ! h264parse ! avdec_h264 ! \
             queue max-size-buffers=4 leaky=downstream ! \
             videoconvert ! videoscale ! video/x-raw,width={tile_w},height={tile_h} ! mix.sink_{i}",
            mcast = src.mcast,
            port = src.port,
            cr = src.clock_rate,
            enc = src.encoding_name,
            pt = src.payload_type,
        ));
    }
    s.push_str(&format!(
        " mix. ! videoconvert ! cairooverlay name=tally ! videoconvert ! kmssink connector-id={connector_id}"
    ));
    s
}

pub fn build_preview(
    sources: &[Source],
    connector_id: u32,
    get_active_slot: Arc<dyn Fn() -> Option<u8> + Send + Sync>,
) -> Result<PreviewPipeline> {
    let s = preview_pipeline_string(sources, connector_id);
    if sources.is_empty() {
        let pipeline = gstreamer::parse::launch(&s)?
            .downcast::<Pipeline>()
            .map_err(|_| anyhow::anyhow!("downcast"))?;
        return Ok(PreviewPipeline { pipeline });
    }

    let n = sources.len();
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
                ctx.rectangle(
                    x + 4.0,
                    y + 4.0,
                    (tile_w as f64) - 8.0,
                    (tile_h as f64) - 8.0,
                );
                let _ = ctx.stroke();
            }
        }
        None
    });

    Ok(PreviewPipeline { pipeline })
}

#[cfg(test)]
mod tests {
    use super::{preview_pipeline_string, program_pipeline_string, Source};

    fn s(name: &str, mcast: &str, port: u16) -> Source {
        Source {
            name: name.into(),
            mcast: mcast.into(),
            port,
            payload_type: 96,
            clock_rate: 90000,
            encoding_name: "H264".into(),
        }
    }

    #[test]
    fn program_pipeline_zero_sources() {
        let p = program_pipeline_string(&[], 32);
        assert_eq!(
            p,
            "input-selector name=sel sel. ! videoconvert ! kmssink connector-id=32"
        );
    }

    #[test]
    fn program_pipeline_one_source_uses_rtp_chain() {
        let p = program_pipeline_string(&[s("A", "239.42.1.1", 5000)], 32);
        assert!(p.contains("udpsrc address=239.42.1.1 port=5000 auto-multicast=true"));
        assert!(p.contains("rtpjitterbuffer latency=20"));
        assert!(p.contains("rtph264depay ! h264parse ! avdec_h264"));
        assert!(p.contains("sel.sink_0"));
        assert!(p.ends_with("kmssink connector-id=32"));
        assert!(!p.contains("ndisrc"));
    }

    #[test]
    fn program_pipeline_two_sources_use_distinct_groups() {
        let p = program_pipeline_string(
            &[s("A", "239.42.1.1", 5000), s("B", "239.42.1.2", 5000)],
            32,
        );
        assert!(p.contains("address=239.42.1.1"));
        assert!(p.contains("address=239.42.1.2"));
        assert!(p.contains("sel.sink_0"));
        assert!(p.contains("sel.sink_1"));
    }

    #[test]
    fn preview_pipeline_zero_sources_is_black_test_pattern() {
        let p = preview_pipeline_string(&[], 34);
        assert!(p.starts_with("videotestsrc pattern=black is-live=true"));
        assert!(!p.contains("compositor"));
    }

    #[test]
    fn preview_pipeline_four_sources_uses_2x2_grid() {
        let sources = vec![
            s("A", "239.42.1.1", 5000),
            s("B", "239.42.1.2", 5000),
            s("C", "239.42.1.3", 5000),
            s("D", "239.42.1.4", 5000),
        ];
        let p = preview_pipeline_string(&sources, 34);
        assert!(p.contains("sink_0::xpos=0 sink_0::ypos=0 sink_0::width=960 sink_0::height=540"));
        assert!(
            p.contains("sink_3::xpos=960 sink_3::ypos=540 sink_3::width=960 sink_3::height=540")
        );
        assert!(p.ends_with(
            "mix. ! videoconvert ! cairooverlay name=tally ! videoconvert ! kmssink connector-id=34"
        ));
    }
}
