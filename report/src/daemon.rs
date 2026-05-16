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
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

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

    /// Run the daemon. Blocks the calling thread; sets up discovery + keyboard threads.
    pub fn run(self) -> Result<()> {
        gstreamer::init()?;

        let (src_tx, src_rx) = channel::<Vec<String>>();
        let (key_tx, key_rx) = channel::<u8>();

        let discovery = Discovery::new()?;
        let _disc_handle = std::thread::Builder::new()
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
        let _kbd_handle = std::thread::Builder::new()
            .name("report-keyboard".into())
            .spawn(move || {
                if let Err(e) = crate::input::run_keyboard_loop(&kbd_device, key_tx) {
                    warn!(error = ?e, "keyboard loop exited");
                }
            })?;

        self.event_loop(src_rx, key_rx)
    }

    fn event_loop(&self, src_rx: Receiver<Vec<String>>, key_rx: Receiver<u8>) -> Result<()> {
        loop {
            if let Ok(names) = src_rx.recv_timeout(Duration::from_millis(50)) {
                self.on_sources_changed(&names)?;
            }
            while let Ok(slot) = key_rx.try_recv() {
                self.handle_keypress(slot)?;
            }
        }
    }

    fn on_sources_changed(&self, raw_sources: &[String]) -> Result<()> {
        let mapping = assign_slots(raw_sources, &self.cfg.source_slot_overrides);
        let max = mapping.values().copied().max().unwrap_or(0);
        let mut ordered: Vec<Option<String>> = vec![None; max as usize];
        for (name, slot) in &mapping {
            ordered[(*slot - 1) as usize] = Some(name.clone());
        }
        let new_sources: Vec<String> = ordered.into_iter().flatten().collect();

        let mut st = self.state.lock();
        if new_sources == st.sources_in_order {
            return Ok(());
        }
        info!(new = ?new_sources, "sources changed");

        if let Some(p) = st.program.take() {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }
        if let Some(p) = st.preview.take() {
            let _ = p.pipeline.set_state(gstreamer::State::Null);
        }
        st.sources_in_order = new_sources.clone();

        let state_for_tally = self.state.clone();
        let preview = build_preview(
            &new_sources,
            self.cfg.preview_connector_id,
            Arc::new(move || state_for_tally.lock().active_slot),
        )?;
        preview.pipeline.set_state(gstreamer::State::Playing)?;
        st.preview = Some(preview);

        if new_sources.is_empty() {
            st.active_slot = None;
            st.program = None;
            return Ok(());
        }

        let program = build_program(&new_sources, self.cfg.program_connector_id)?;
        program.pipeline.set_state(gstreamer::State::Playing)?;
        let _ = select_slot(&program.selector, 0);
        st.active_slot = Some(1);
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
