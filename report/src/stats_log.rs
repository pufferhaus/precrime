//! Periodic stats logger — emits hardware and witness stats at regular intervals.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use parking_lot::Mutex;
use temple::{HwStats, WitnessStats};
use tracing::info;

pub fn spawn_stats_logger(
    hw_stats: Arc<Mutex<HashMap<String, HwStats>>>,
    witness_stats: Arc<Mutex<HashMap<String, WitnessStats>>>,
    self_hw: Arc<Mutex<Option<HwStats>>>,
    shutdown: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("report-stats-log".into())
        .spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(30));
                log_all(&hw_stats, &witness_stats, &self_hw);
            }
        })?;
    Ok(())
}

pub(crate) fn log_all(
    hw_stats: &Mutex<HashMap<String, HwStats>>,
    witness_stats: &Mutex<HashMap<String, WitnessStats>>,
    self_hw: &Mutex<Option<HwStats>>,
) {
    for (name, hw) in hw_stats.lock().iter() {
        info!(
            source = %name,
            temp_c = hw.cpu_temp_mc as f64 / 1000.0,
            cpu_pct = hw.cpu_load_pct,
            mem_used_mb = hw.mem_used_mb,
            mem_total_mb = hw.mem_total_mb,
            rssi_dbm = ?hw.wifi_rssi_dbm,
            "precog hw stats"
        );
    }

    for (name, ws) in witness_stats.lock().iter() {
        info!(
            source = %name,
            battery_pct = ws.battery_pct,
            charging = ws.charging,
            thermal = ?ws.thermal,
            "witness stats"
        );
    }

    if let Some(hw) = self_hw.lock().as_ref() {
        info!(
            source = "REPORT-SELF",
            temp_c = hw.cpu_temp_mc as f64 / 1000.0,
            cpu_pct = hw.cpu_load_pct,
            mem_used_mb = hw.mem_used_mb,
            mem_total_mb = hw.mem_total_mb,
            rssi_dbm = ?hw.wifi_rssi_dbm,
            "report self hw stats"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use temple::ThermalState;

    #[test]
    fn log_all_does_not_panic_with_empty_maps() {
        let hw: Arc<Mutex<HashMap<String, HwStats>>> = Arc::new(Mutex::new(HashMap::new()));
        let ws: Arc<Mutex<HashMap<String, WitnessStats>>> = Arc::new(Mutex::new(HashMap::new()));
        let self_hw: Arc<Mutex<Option<HwStats>>> = Arc::new(Mutex::new(None));
        log_all(&hw, &ws, &self_hw);
    }

    #[test]
    fn log_all_does_not_panic_with_populated_data() {
        let hw: Arc<Mutex<HashMap<String, HwStats>>> = Arc::new(Mutex::new({
            let mut m = HashMap::new();
            m.insert("PRECOG-01".into(), HwStats {
                cpu_temp_mc: 42300,
                cpu_load_pct: 67,
                mem_used_mb: 280,
                mem_total_mb: 480,
                wifi_rssi_dbm: Some(-54),
            });
            m
        }));
        let ws: Arc<Mutex<HashMap<String, WitnessStats>>> = Arc::new(Mutex::new({
            let mut m = HashMap::new();
            m.insert("WITNESS-STAGE".into(), WitnessStats {
                battery_pct: 84,
                charging: false,
                thermal: ThermalState::Nominal,
            });
            m
        }));
        let self_hw: Arc<Mutex<Option<HwStats>>> = Arc::new(Mutex::new(Some(HwStats {
            cpu_temp_mc: 51000,
            cpu_load_pct: 23,
            mem_used_mb: 340,
            mem_total_mb: 2000,
            wifi_rssi_dbm: Some(-61),
        })));
        log_all(&hw, &ws, &self_hw);
    }
}
