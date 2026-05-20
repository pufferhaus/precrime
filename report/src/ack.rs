//! UDP acknowledgment sender: notifies all active sources that REPORT is alive.

use crate::pipeline::Source;
use crate::registration::RegisteredSources;
use anyhow::Result;
use parking_lot::Mutex;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::warn;

/// Spawn a background thread that sends UDP ack packets every 2 seconds to all
/// known source hosts (both registered unicast sources and temple multicast
/// sources that provided a sender IP).
pub fn spawn_ack_sender(
    registered: Arc<StdMutex<RegisteredSources>>,
    temple_snapshot: Arc<Mutex<Vec<Source>>>,
    report_name: String,
    ack_port: u16,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    std::thread::Builder::new()
        .name("report-ack-tx".into())
        .spawn(move || {
            let socket = match UdpSocket::bind("0.0.0.0:0") {
                Ok(s) => s,
                Err(e) => {
                    warn!(error = ?e, "ack sender: failed to bind UDP socket; thread exiting");
                    return;
                }
            };

            while !shutdown.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(2));

                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let payload = match serde_json::to_vec(&serde_json::json!({
                    "v": "1",
                    "report": report_name,
                    "ts": ts,
                })) {
                    Ok(b) => b,
                    Err(e) => {
                        warn!(error = ?e, "ack sender: failed to serialize payload");
                        continue;
                    }
                };

                // Collect registered (name, ip) pairs — name needed for refresh.
                let registered_targets: Vec<(String, String)> = registered
                    .lock()
                    .map(|reg| {
                        reg.sources
                            .iter()
                            .map(|(n, s)| (n.clone(), s.host_ip.clone()))
                            .collect()
                    })
                    .unwrap_or_default();

                // Temple sources: ip only (temple handles its own eviction).
                let temple_targets: Vec<String> = {
                    let snap = temple_snapshot.lock();
                    snap.iter()
                        .filter_map(|s| s.host.clone())
                        .collect()
                };

                for (name, ip) in &registered_targets {
                    let addr = format!("{ip}:{ack_port}");
                    match socket.send_to(&payload, &addr) {
                        Ok(_) => {
                            // Refresh last_seen so actively-acked sources aren't evicted.
                            if let Ok(mut reg) = registered.lock() {
                                reg.touch(name);
                            }
                        }
                        Err(e) => warn!(dest = %addr, error = ?e, "ack send failed"),
                    }
                }

                for ip in &temple_targets {
                    let addr = format!("{ip}:{ack_port}");
                    if let Err(e) = socket.send_to(&payload, &addr) {
                        warn!(dest = %addr, error = ?e, "ack send failed");
                    }
                }
            }
        })?;

    Ok(())
}
