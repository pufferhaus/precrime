//! REPORT daemon: owns pipelines, handles source-set changes and keypresses.

use crate::config::ReportConfig;
use crate::mapping::assign_slots;
use crate::naming::{display_name, is_precog_source};
use crate::ndi_find::Discovery;
use crate::pipeline::{build_preview, build_program, select_slot, PreviewPipeline, ProgramPipeline};
use anyhow::Result;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

pub struct Daemon {
    cfg: ReportConfig,
    state: Arc<Mutex<DaemonState>>,
}

struct DaemonState {
    sources_in_order: Vec<String>,
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

    /// Run the daemon. Blocks the calling thread; sets up discovery + keyboard
    /// + per-pipeline bus-watch threads. Returns Ok(()) on clean shutdown
    /// (SIGTERM/SIGINT).
    pub fn run(self) -> Result<()> {
        gstreamer::init()?;

        // Shutdown flag flipped by SIGTERM/SIGINT handlers.
        let shutdown = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(signal_hook::consts::SIGTERM, shutdown.clone())?;
        signal_hook::flag::register(signal_hook::consts::SIGINT, shutdown.clone())?;

        let (src_tx, src_rx) = channel::<Vec<String>>();
        let (key_tx, key_rx) = channel::<u8>();
        let (bus_tx, bus_rx) = channel::<BusEvent>();

        let discovery = Discovery::new()?;
        std::thread::Builder::new()
            .name("report-discovery".into())
            .spawn(move || {
                let mut last = BTreeSet::<String>::new();
                loop {
                    let raw = discovery.poll(Duration::from_secs(2));
                    let names: BTreeSet<String> = raw
                        .iter()
                        .filter(|n| is_precog_source(n))
                        .map(|n| display_name(n).to_owned())
                        .collect();
                    if names != last {
                        last = names.clone();
                        let ordered: Vec<String> = names.into_iter().collect();
                        if src_tx.send(ordered).is_err() {
                            break;
                        }
                    }
                }
            })?;

        let kbd_device = self.cfg.keyboard_device.clone();
        std::thread::Builder::new()
            .name("report-keyboard".into())
            .spawn(move || {
                if let Err(e) = crate::input::run_keyboard_loop(&kbd_device, key_tx) {
                    warn!(error = ?e, "keyboard loop exited");
                }
            })?;

        self.event_loop(src_rx, key_rx, bus_rx, bus_tx, shutdown)
    }

    fn event_loop(
        &self,
        src_rx: Receiver<Vec<String>>,
        key_rx: Receiver<u8>,
        bus_rx: Receiver<BusEvent>,
        bus_tx: Sender<BusEvent>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<()> {
        while !shutdown.load(Ordering::Relaxed) {
            if let Ok(names) = src_rx.recv_timeout(Duration::from_millis(50)) {
                self.on_sources_changed(&names, &bus_tx)?;
            }
            while let Ok(slot) = key_rx.try_recv() {
                self.handle_keypress(slot)?;
            }
            while let Ok(event) = bus_rx.try_recv() {
                match event {
                    BusEvent::Error { which, msg } => {
                        error!(pipeline = which, %msg, "pipeline error from bus");
                    }
                    BusEvent::Eos { which } => {
                        warn!(pipeline = which, "pipeline EOS from bus");
                    }
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

    fn on_sources_changed(&self, raw_sources: &[String], bus_tx: &Sender<BusEvent>) -> Result<()> {
        let mapping = assign_slots(raw_sources, &self.cfg.source_slot_overrides);
        let max = mapping.values().copied().max().unwrap_or(0);
        let mut ordered: Vec<Option<String>> = vec![None; max as usize];
        for (name, slot) in &mapping {
            ordered[(*slot - 1) as usize] = Some(name.clone());
        }
        let new_sources: Vec<String> = ordered.into_iter().flatten().collect();

        // Phase 1: take old pipelines out under the lock, set active_slot,
        // then RELEASE the lock before destroying old pipelines or building new ones.
        // set_state(Null) blocks until streaming threads exit; the cairo tally
        // callback also locks state. Holding the lock here would deadlock.
        let (old_program, old_preview) = {
            let mut st = self.state.lock();
            if new_sources == st.sources_in_order {
                return Ok(());
            }
            info!(new = ?new_sources, "sources changed");
            st.sources_in_order = new_sources.clone();
            if new_sources.is_empty() {
                st.active_slot = None;
            } else {
                st.active_slot = Some(1);
            }
            (st.program.take(), st.preview.take())
        };

        // Phase 2: destroy old pipelines outside the lock. Bus watch threads
        // exit cleanly when the pipeline goes NULL.
        if let Some(p) = old_program {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }
        if let Some(p) = old_preview {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }

        // Phase 3: build new pipelines.
        let state_for_tally = self.state.clone();
        let preview = build_preview(
            &new_sources,
            self.cfg.preview_connector_id,
            Arc::new(move || state_for_tally.lock().active_slot),
        )?;
        spawn_bus_watch("preview", &preview.pipeline, bus_tx.clone())?;
        preview.pipeline.set_state(gstreamer::State::Playing)?;

        if new_sources.is_empty() {
            let mut st = self.state.lock();
            st.preview = Some(preview);
            return Ok(());
        }

        let program = build_program(&new_sources, self.cfg.program_connector_id)?;
        spawn_bus_watch("program", &program.pipeline, bus_tx.clone())?;
        program.pipeline.set_state(gstreamer::State::Playing)?;
        let _ = select_slot(&program.selector, 0);

        let mut st = self.state.lock();
        st.preview = Some(preview);
        st.program = Some(program);
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
        info!(
            slot,
            source = %st.sources_in_order[(slot - 1) as usize],
            "cut"
        );
        st.active_slot = Some(slot);
        if let Some(program) = st.program.as_ref() {
            select_slot(&program.selector, (slot - 1) as usize)?;
        }
        Ok(())
    }
}

/// Spawn a thread that drains the pipeline's bus and forwards Error/EOS to the
/// daemon's event loop. Thread exits when the pipeline is destroyed (bus iter
/// terminates on Null state).
fn spawn_bus_watch(
    which: &'static str,
    pipeline: &gstreamer::Pipeline,
    tx: Sender<BusEvent>,
) -> Result<()> {
    let bus = pipeline.bus().ok_or_else(|| anyhow::anyhow!("no bus on pipeline"))?;
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
