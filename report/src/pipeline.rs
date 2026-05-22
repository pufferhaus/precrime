//! GStreamer pipeline builders for REPORT.
//!
//! Two independent pipelines, each rendering directly to a DRM/KMS connector:
//! - `program`: input-selector over N udpsrc RTP inputs → kmssink (HDMI-A-1)
//! - `preview`: compositor (2x2 or 3x3 grid) → cairooverlay tally → kmssink (HDMI-A-2)

use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer::{Element, Pipeline};
use std::collections::HashMap;
use std::sync::Arc;
use temple::HwStats;

/// Probe the GStreamer registry once and return the best available H.264 decoder.
/// Prefers v4l2slh264dec (Pi 5 stateless HW, zero-copy DMA-BUF) when present;
/// falls back to avdec_h264 (libavcodec SW) on hardware without the V4L2 codec.
fn h264_decoder() -> &'static str {
    static DECODER: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    *DECODER.get_or_init(|| {
        #[cfg(target_os = "linux")]
        if gstreamer::ElementFactory::find("v4l2slh264dec").is_some() {
            tracing::info!("H.264 decoder: v4l2slh264dec (hardware)");
            return "v4l2slh264dec";
        }
        tracing::info!("H.264 decoder: avdec_h264 (software)");
        "avdec_h264"
    })
}

/// Transport mode for a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    /// UDP multicast: join the given group address.
    Multicast { group: String },
    /// UDP unicast: bind 0.0.0.0 on the port, receive from any sender.
    Unicast,
}

/// One discovered source as seen by the pipeline builders. Cloned from the
/// `temple::Ball` payload at the moment pipelines are rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub transport: Transport,
    pub port: u16,
    pub payload_type: u8,
    pub clock_rate: u32,
    pub encoding_name: String, // "H264"
    /// Sender IP address for ack packets. None for pure-multicast sources.
    pub host: Option<String>,
}

pub struct ProgramPipeline {
    pub pipeline: Pipeline,
    pub selector: Element,
}

/// Construct the program-out gst-launch pipeline string. Pure function for testing.
pub fn program_pipeline_string(sources: &[Source], connector_id: u32) -> String {
    let mut parts = String::from("input-selector name=sel");
    for (i, s) in sources.iter().enumerate() {
        let udpsrc = match &s.transport {
            Transport::Multicast { group } => format!(
                "udpsrc address={group} port={port} auto-multicast=true \
                 caps=\"application/x-rtp,media=video,clock-rate={cr},encoding-name={enc},payload={pt}\"",
                port = s.port,
                cr = s.clock_rate,
                enc = s.encoding_name,
                pt = s.payload_type,
            ),
            Transport::Unicast => format!(
                "udpsrc port={port} \
                 caps=\"application/x-rtp,media=video,clock-rate={cr},encoding-name={enc},payload={pt}\"",
                port = s.port,
                cr = s.clock_rate,
                enc = s.encoding_name,
                pt = s.payload_type,
            ),
        };
        parts.push_str(&format!(
            " {udpsrc} ! \
             rtpjitterbuffer latency=20 ! \
             rtph264depay ! h264parse ! {decoder} ! \
             queue max-size-buffers=4 leaky=downstream ! \
             videoconvert ! sel.sink_{i}",
            decoder = h264_decoder(),
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

pub(crate) fn format_hw_line(hw: &HwStats) -> String {
    let temp = if hw.cpu_temp_mc == 0 {
        "--".to_string()
    } else {
        format!("{:.1}°C", hw.cpu_temp_mc as f64 / 1000.0)
    };
    let load = if hw.cpu_load_pct == 0 && hw.mem_total_mb == 0 {
        "--".to_string()
    } else {
        format!("CPU {}%", hw.cpu_load_pct)
    };
    let mem = if hw.mem_total_mb == 0 {
        "--".to_string()
    } else {
        format!("RAM {}/{}M", hw.mem_used_mb, hw.mem_total_mb)
    };
    let rssi = hw
        .wifi_rssi_dbm
        .map(|r| format!("  {r}dBm"))
        .unwrap_or_default();
    format!("{temp}  {load}  {mem}{rssi}")
}

fn draw_tile_stats(ctx: &cairo::Context, x: f64, y: f64, tile_h: f64, name: &str, hw: &HwStats) {
    let line2 = format_hw_line(hw);
    let text_y = y + tile_h - 8.0;
    let line_h = 22.0_f64;

    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.65);
    ctx.rectangle(x + 4.0, text_y - line_h * 2.0 - 4.0, 500.0, line_h * 2.0 + 8.0);
    let _ = ctx.fill();

    ctx.select_font_face("Monospace", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(14.0);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 1.0);
    ctx.move_to(x + 8.0, text_y - line_h);
    let _ = ctx.show_text(name);
    ctx.move_to(x + 8.0, text_y);
    let _ = ctx.show_text(&line2);
}

fn draw_self_stats(ctx: &cairo::Context, hw: &HwStats) {
    let temp = hw.cpu_temp_mc as f64 / 1000.0;
    let text = format!(
        "REPORT  {:.1}°C  CPU {}%  RAM {}/{}M",
        temp, hw.cpu_load_pct, hw.mem_used_mb, hw.mem_total_mb
    );

    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.65);
    ctx.rectangle(1920.0 - 430.0, 8.0, 422.0, 28.0);
    let _ = ctx.fill();

    ctx.select_font_face("Monospace", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    ctx.set_font_size(14.0);
    ctx.set_source_rgba(1.0, 1.0, 1.0, 1.0);
    ctx.move_to(1920.0 - 426.0, 28.0);
    let _ = ctx.show_text(&text);
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
        let udpsrc = match &src.transport {
            Transport::Multicast { group } => format!(
                "udpsrc address={group} port={port} auto-multicast=true \
                 caps=\"application/x-rtp,media=video,clock-rate={cr},encoding-name={enc},payload={pt}\"",
                port = src.port,
                cr = src.clock_rate,
                enc = src.encoding_name,
                pt = src.payload_type,
            ),
            Transport::Unicast => format!(
                "udpsrc port={port} \
                 caps=\"application/x-rtp,media=video,clock-rate={cr},encoding-name={enc},payload={pt}\"",
                port = src.port,
                cr = src.clock_rate,
                enc = src.encoding_name,
                pt = src.payload_type,
            ),
        };
        s.push_str(&format!(
            " {udpsrc} ! \
             rtpjitterbuffer latency=20 ! \
             rtph264depay ! h264parse ! {decoder} ! \
             queue max-size-buffers=4 leaky=downstream ! \
             videoconvert ! videoscale ! video/x-raw,width={tile_w},height={tile_h} ! mix.sink_{i}",
            decoder = h264_decoder(),
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
    get_hw_stats: Arc<dyn Fn() -> HashMap<String, HwStats> + Send + Sync>,
    get_self_hw: Arc<dyn Fn() -> Option<HwStats> + Send + Sync>,
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
    let hw_cb = get_hw_stats.clone();
    let self_hw_cb = get_self_hw.clone();
    let source_names: Vec<String> = sources.iter().map(|s| s.name.clone()).collect();
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

        // Tally border (existing behaviour)
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

        // PRECOG hw stats per tile
        let stats_snapshot = hw_cb();
        for (i, name) in source_names.iter().enumerate() {
            if let Some(hw) = stats_snapshot.get(name) {
                let col = (i % cols) as u32;
                let row = (i / cols) as u32;
                let x = (col * tile_w) as f64;
                let y = (row * tile_h) as f64;
                draw_tile_stats(&ctx, x, y, tile_h as f64, name, hw);
            }
        }

        // REPORT self stats — top-right corner
        if let Some(hw) = self_hw_cb() {
            draw_self_stats(&ctx, &hw);
        }

        None
    });

    Ok(PreviewPipeline { pipeline })
}

#[cfg(test)]
mod tests {
    use super::{format_hw_line, h264_decoder, preview_pipeline_string, program_pipeline_string, Source, Transport};
    use temple::HwStats;

    fn s(name: &str, mcast: &str, port: u16) -> Source {
        Source {
            name: name.into(),
            transport: Transport::Multicast {
                group: mcast.into(),
            },
            port,
            payload_type: 96,
            clock_rate: 90000,
            encoding_name: "H264".into(),
            host: None,
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
        let _ = gstreamer::init();
        assert!(p.contains(&format!("rtph264depay ! h264parse ! {}", h264_decoder())));
        assert!(p.contains("sel.sink_0"));
        assert!(p.ends_with("kmssink connector-id=32"));
        assert!(!p.contains("ndisrc"));
    }

    #[test]
    fn decoder_is_known_element() {
        let _ = gstreamer::init();
        let d = h264_decoder();
        assert!(d == "v4l2slh264dec" || d == "avdec_h264");
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

    #[test]
    fn format_hw_line_formats_temp_and_load() {
        let hw = HwStats {
            cpu_temp_mc: 42300,
            cpu_load_pct: 67,
            mem_used_mb: 280,
            mem_total_mb: 480,
            wifi_rssi_dbm: Some(-54),
        };
        let line = format_hw_line(&hw);
        assert!(line.contains("42.3"), "temp: {line}");
        assert!(line.contains("67%"), "load: {line}");
        assert!(line.contains("280/480M"), "mem: {line}");
        assert!(line.contains("-54dBm"), "rssi: {line}");
    }

    #[test]
    fn format_hw_line_omits_rssi_when_none() {
        let hw = HwStats { cpu_temp_mc: 50000, cpu_load_pct: 10, mem_used_mb: 100, mem_total_mb: 1000, wifi_rssi_dbm: None };
        let line = format_hw_line(&hw);
        assert!(!line.contains("dBm"), "no rssi expected: {line}");
    }

    #[test]
    fn format_hw_line_shows_dashes_for_zero_stats() {
        let hw = HwStats { cpu_temp_mc: 0, cpu_load_pct: 0, mem_used_mb: 0, mem_total_mb: 0, wifi_rssi_dbm: None };
        let line = format_hw_line(&hw);
        assert!(line.contains("--"), "should show dashes for unavailable stats: {line}");
    }
}
