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

                // Collect all destination IPs before sending to minimize lock time.
                let mut targets: Vec<String> = Vec::new();

                {
                    if let Ok(reg) = registered.lock() {
                        for src in reg.sources.values() {
                            targets.push(src.host_ip.clone());
                        }
                    }
                }

                {
                    let snap = temple_snapshot.lock();
                    for src in snap.iter() {
                        if let Some(ref ip) = src.host {
                            targets.push(ip.clone());
                        }
                    }
                }

                for ip in &targets {
                    let addr = format!("{ip}:{ack_port}");
                    if let Err(e) = socket.send_to(&payload, &addr) {
                        warn!(dest = %addr, error = ?e, "ack send failed");
                    }
                }
            }
        })?;

    Ok(())
}
