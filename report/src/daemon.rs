//! REPORT daemon: owns pipelines, handles source-set changes and keypresses.

use crate::config::ReportConfig;
use crate::mapping::assign_slots;
use crate::pipeline::{
    build_preview, build_program, select_slot, PreviewPipeline, ProgramPipeline, Source,
};
use crate::registration::RegisteredSources;
use anyhow::Result;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use temple::{Ball, Receiver as TempleReceiver, BALL_EVICTION_SECS};
use tracing::{error, info, warn};

pub struct Daemon {
    cfg: ReportConfig,
    state: Arc<Mutex<DaemonState>>,
}

struct DaemonState {
    sources_in_order: Vec<Source>,
    active_slot: Option<u8>,
    program: Option<ProgramPipeline>,
    preview: Option<PreviewPipeline>,
}

#[derive(Debug)]
enum BusEvent {
    Error { which: &'static str, msg: String },
    Eos { which: &'static str },
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

    pub fn run(mut self) -> Result<()> {
        gstreamer::init()?;

        let shutdown = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(signal_hook::consts::SIGTERM, shutdown.clone())?;
        signal_hook::flag::register(signal_hook::consts::SIGINT, shutdown.clone())?;

        // Shared source snapshots — written by their respective producer threads,
        // read in the event loop for merging.
        let temple_snapshot: Arc<Mutex<Vec<Source>>> = Arc::new(Mutex::new(Vec::new()));
        let registered: Arc<std::sync::Mutex<RegisteredSources>> = Arc::new(std::sync::Mutex::new(
            RegisteredSources::new(self.cfg.rtp_port_min, self.cfg.rtp_port_max),
        ));

        // Single unified signal channel: any producer sends () to trigger a merge.
        let (change_tx, change_rx) = channel::<()>();
        let (key_tx, key_rx) = channel::<u8>();
        let (bus_tx, bus_rx) = channel::<BusEvent>();

        // ── Temple-rx thread ─────────────────────────────────────────────────
        let group = self.cfg.temple_group;
        let port = self.cfg.temple_port;
        let eviction = Duration::from_secs(BALL_EVICTION_SECS);
        let temple_snap_tx = temple_snapshot.clone();
        let change_tx_temple = change_tx.clone();
        std::thread::Builder::new()
            .name("report-temple-rx".into())
            .spawn(move || {
                let mut rx = match TempleReceiver::new(group, port, eviction) {
                    Ok(r) => r,
                    Err(e) => {
                        error!(error = ?e, "temple receiver init failed; thread exiting");
                        return;
                    }
                };
                loop {
                    match rx.poll(Duration::from_secs(1)) {
                        Ok(true) => {
                            let sources = balls_to_sources(rx.snapshot());
                            *temple_snap_tx.lock() = sources;
                            if change_tx_temple.send(()).is_err() {
                                break;
                            }
                        }
                        Ok(false) => {}
                        Err(e) => {
                            warn!(error = ?e, "temple poll error");
                        }
                    }
                }
            })?;

        // ── Keyboard thread ───────────────────────────────────────────────────
        let kbd_device = self.cfg.keyboard_device.clone();
        std::thread::Builder::new()
            .name("report-keyboard".into())
            .spawn(move || {
                if let Err(e) = crate::input::run_keyboard_loop(&kbd_device, key_tx) {
                    warn!(error = ?e, "keyboard loop exited");
                }
            })?;

        // ── Registration server thread ────────────────────────────────────────
        crate::registration::spawn_registration_server(
            self.cfg.reg_port,
            self.cfg.report_name.clone(),
            self.cfg.ack_port,
            self.cfg.stats_port,
            registered.clone(),
            change_tx.clone(),
        )?;

        // ── Eviction thread ───────────────────────────────────────────────────
        {
            let registered_evict = registered.clone();
            let change_tx_evict = change_tx.clone();
            std::thread::Builder::new()
                .name("report-eviction".into())
                .spawn(move || loop {
                    std::thread::sleep(Duration::from_secs(5));
                    let evicted = registered_evict
                        .lock()
                        .expect("eviction lock")
                        .evict_stale(Duration::from_secs(30));
                    if !evicted.is_empty() {
                        for name in &evicted {
                            info!(source = %name, "evicted stale registered source");
                        }
                        if change_tx_evict.send(()).is_err() {
                            break;
                        }
                    }
                })?;
        }

        // ── Ack sender thread ─────────────────────────────────────────────────
        crate::ack::spawn_ack_sender(
            registered.clone(),
            temple_snapshot.clone(),
            self.cfg.report_name.clone(),
            self.cfg.ack_port,
            shutdown.clone(),
        )?;

        // ── Bonjour publisher watchdog ────────────────────────────────────────
        crate::bonjour::spawn_bonjour_publisher(
            &self.cfg.report_name,
            self.cfg.reg_port,
            shutdown.clone(),
        );

        self.event_loop(
            temple_snapshot,
            registered,
            change_rx,
            key_rx,
            bus_rx,
            bus_tx,
            shutdown,
        )
    }

    fn event_loop(
        &self,
        temple_snapshot: Arc<Mutex<Vec<Source>>>,
        registered: Arc<std::sync::Mutex<RegisteredSources>>,
        change_rx: Receiver<()>,
        key_rx: Receiver<u8>,
        bus_rx: Receiver<BusEvent>,
        bus_tx: Sender<BusEvent>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<()> {
        while !shutdown.load(Ordering::Relaxed) {
            if change_rx.recv_timeout(Duration::from_millis(50)).is_ok() {
                let sources = {
                    let temple = temple_snapshot.lock();
                    let reg = registered.lock().expect("registered lock");
                    merged_sources(&temple, &reg)
                };
                self.on_sources_changed(sources, &bus_tx)?;
            }
            while let Ok(slot) = key_rx.try_recv() {
                self.handle_keypress(slot)?;
            }
            let mut needs_rebuild = false;
            while let Ok(event) = bus_rx.try_recv() {
                match event {
                    BusEvent::Error { which, msg } => {
                        error!(pipeline = which, %msg, "pipeline error from bus");
                    }
                    BusEvent::Eos { which } => {
                        warn!(pipeline = which, "pipeline EOS from bus");
                    }
                }
                // Only rebuild if at least one pipeline is live — avoids a tight
                // loop when both pipelines fail (e.g. no display connected) and
                // their stale bus watchers keep firing after install_pipelines
                // already set both to None.
                let has_live_pipeline = {
                    let st = self.state.lock();
                    st.program.is_some() || st.preview.is_some()
                };
                if has_live_pipeline {
                    needs_rebuild = true;
                }
            }
            if needs_rebuild {
                if let Err(e) = self.force_rebuild(&bus_tx) {
                    error!(error = ?e, "rebuild after bus event failed");
                }
            }
        }

        info!("shutdown signal received — tearing down pipelines");
        let mut st = self.state.lock();
        if let Some(p) = st.program.take() {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }
        if let Some(p) = st.preview.take() {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }
        Ok(())
    }

    fn on_sources_changed(
        &self,
        raw_sources: Vec<Source>,
        bus_tx: &Sender<BusEvent>,
    ) -> Result<()> {
        let names: Vec<String> = raw_sources.iter().map(|s| s.name.clone()).collect();
        let mapping = assign_slots(&names, &self.cfg.source_slot_overrides);
        let max = mapping.values().copied().max().unwrap_or(0);
        let mut ordered: Vec<Option<Source>> = vec![None; max as usize];
        for src in &raw_sources {
            if let Some(&slot) = mapping.get(&src.name) {
                ordered[(slot - 1) as usize] = Some(src.clone());
            }
        }
        let new_sources: Vec<Source> = ordered.into_iter().flatten().collect();

        {
            let st = self.state.lock();
            if new_sources == st.sources_in_order {
                return Ok(());
            }
        }
        info!(new = ?new_sources.iter().map(|s| &s.name).collect::<Vec<_>>(), "sources changed");
        self.install_pipelines(&new_sources, bus_tx)
    }

    fn force_rebuild(&self, bus_tx: &Sender<BusEvent>) -> Result<()> {
        let sources = self.state.lock().sources_in_order.clone();
        info!(?sources, "forced rebuild after bus event");
        self.install_pipelines(&sources, bus_tx)
    }

    fn install_pipelines(&self, sources: &[Source], bus_tx: &Sender<BusEvent>) -> Result<()> {
        let (old_program, old_preview) = {
            let mut st = self.state.lock();
            st.sources_in_order = sources.to_vec();
            st.active_slot = if sources.is_empty() { None } else { Some(1) };
            (st.program.take(), st.preview.take())
        };

        if let Some(p) = old_program {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }
        if let Some(p) = old_preview {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }

        let state_for_tally = self.state.clone();
        let preview_result = build_preview(
            sources,
            self.cfg.preview_connector_id,
            Arc::new(move || state_for_tally.lock().active_slot),
        )
        .and_then(|p| {
            p.pipeline
                .set_state(gstreamer::State::Playing)
                .map_err(|e| anyhow::anyhow!("preview set_state: {e}"))?;
            spawn_bus_watch("preview", &p.pipeline, bus_tx.clone())?;
            Ok(p)
        });

        let preview = match preview_result {
            Ok(p) => Some(p),
            Err(e) => {
                warn!(error = ?e, "preview pipeline unavailable — no display?");
                None
            }
        };

        let program = if sources.is_empty() {
            None
        } else {
            let program_result = build_program(sources, self.cfg.program_connector_id)
                .and_then(|p| {
                    p.pipeline
                        .set_state(gstreamer::State::Playing)
                        .map_err(|e| anyhow::anyhow!("program set_state: {e}"))?;
                    spawn_bus_watch("program", &p.pipeline, bus_tx.clone())?;
                    let _ = select_slot(&p.selector, 0);
                    Ok(p)
                });
            match program_result {
                Ok(p) => Some(p),
                Err(e) => {
                    warn!(error = ?e, "program pipeline unavailable — no display?");
                    None
                }
            }
        };

        let mut st = self.state.lock();
        st.preview = preview;
        st.program = program;
        Ok(())
    }

    fn handle_keypress(&self, slot: u8) -> Result<()> {
        let mut st = self.state.lock();
        if st.program.is_none() {
            return Ok(());
        }
        if (slot as usize) > st.sources_in_order.len() || slot == 0 {
            return Ok(());
        }
        info!(slot, source = %st.sources_in_order[(slot - 1) as usize].name, "cut");
        st.active_slot = Some(slot);
        if let Some(program) = st.program.as_ref() {
            select_slot(&program.selector, (slot - 1) as usize)?;
        }
        Ok(())
    }
}

/// Merge temple (multicast) sources with registered (unicast) sources.
/// Registered source wins over temple source if both have the same name.
fn merged_sources(temple: &[Source], registered: &RegisteredSources) -> Vec<Source> {
    const DEFAULT_PT: u8 = 96;
    const DEFAULT_CLOCK_RATE: u32 = 90000;
    const DEFAULT_ENCODING: &str = "H264";

    let unicast = registered.as_sources(DEFAULT_PT, DEFAULT_CLOCK_RATE, DEFAULT_ENCODING);
    let unicast_names: std::collections::HashSet<&str> =
        unicast.iter().map(|s| s.name.as_str()).collect();

    let mut result: Vec<Source> = temple
        .iter()
        .filter(|s| !unicast_names.contains(s.name.as_str()))
        .cloned()
        .collect();
    result.extend(unicast);
    result
}

/// Convert balls into pipeline-ready Source records, keeping only PRECOG-named
/// entries (defense in depth — non-PRECOG balls should not reach this channel
/// in production).
fn balls_to_sources(balls: Vec<Ball>) -> Vec<Source> {
    balls
        .into_iter()
        .filter_map(|b| match b {
            Ball::V1(v) if v.name.starts_with("PRECOG-") => Some(Source {
                name: v.name,
                transport: crate::pipeline::Transport::Multicast { group: v.rtp.mcast },
                port: v.rtp.port,
                payload_type: v.rtp.pt,
                clock_rate: v.rtp.clock_rate,
                encoding_name: v.rtp.encoding_name,
                host: Some(v.host),
            }),
            _ => None,
        })
        .collect()
}

fn spawn_bus_watch(
    which: &'static str,
    pipeline: &gstreamer::Pipeline,
    tx: Sender<BusEvent>,
) -> Result<()> {
    let bus = pipeline
        .bus()
        .ok_or_else(|| anyhow::anyhow!("no bus on pipeline"))?;
    std::thread::Builder::new()
        .name(format!("report-bus-{which}"))
        .spawn(move || {
            for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
                use gstreamer::MessageView;
                match msg.view() {
                    MessageView::Error(e) => {
                        let _ = tx.send(BusEvent::Error {
                            which,
                            msg: e.error().to_string(),
                        });
                        break;
                    }
                    MessageView::Eos(..) => {
                        let _ = tx.send(BusEvent::Eos { which });
                        break;
                    }
                    _ => {}
                }
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use temple::{BallV1, RtpInfo, VideoInfo};

    fn ball(name: &str, mcast: &str) -> Ball {
        Ball::V1(BallV1 {
            name: name.into(),
            host: "10.0.0.1".into(),
            rtp: RtpInfo {
                mcast: mcast.into(),
                port: 5000,
                pt: 96,
                clock_rate: 90000,
                encoding_name: "H264".into(),
            },
            video: VideoInfo {
                width: 1920,
                height: 1080,
                framerate: "30/1".into(),
            },
            hw: None,
        })
    }

    #[test]
    fn balls_to_sources_keeps_precog_names() {
        use crate::pipeline::Transport;
        let sources = balls_to_sources(vec![
            ball("PRECOG-01-X", "239.42.1.1"),
            ball("OTHER-DEVICE", "239.42.1.2"),
        ]);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "PRECOG-01-X");
        assert_eq!(
            sources[0].transport,
            Transport::Multicast {
                group: "239.42.1.1".into()
            }
        );
        assert_eq!(sources[0].payload_type, 96);
        assert_eq!(sources[0].host, Some("10.0.0.1".into()));
    }

    #[test]
    fn balls_to_sources_empty_when_no_balls() {
        assert!(balls_to_sources(Vec::new()).is_empty());
    }
}
