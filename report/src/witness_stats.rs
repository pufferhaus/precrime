//! WITNESS hardware stats UDP listener.

use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::Arc;
use parking_lot::Mutex;
use temple::{ThermalState, WitnessStats, WitnessStatsPacket};
use tracing::{info, warn};

pub fn spawn_witness_stats_receiver(
    stats_port: u16,
    witness_stats: Arc<Mutex<HashMap<String, WitnessStats>>>,
) -> anyhow::Result<()> {
    let sock = UdpSocket::bind(("0.0.0.0", stats_port))?;
    info!(port = stats_port, "witness stats receiver listening");

    std::thread::Builder::new()
        .name("report-witness-stats-rx".into())
        .spawn(move || {
            let mut buf = [0u8; 512];
            loop {
                match sock.recv_from(&mut buf) {
                    Ok((n, _addr)) => match serde_json::from_slice::<WitnessStatsPacket>(&buf[..n]) {
                        Ok(pkt) => {
                            witness_stats
                                .lock()
                                .insert(pkt.name, pkt.stats);
                        }
                        Err(e) => {
                            warn!(error = ?e, "malformed witness stats packet");
                        }
                    },
                    Err(e) => {
                        warn!(error = ?e, "witness stats recv error");
                        break;
                    }
                }
            }
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn witness_stats_packet_deserialises() {
        let json = r#"{"name":"WITNESS-STAGE","stats":{"battery_pct":84,"charging":false,"thermal":"Nominal"}}"#;
        let pkt: WitnessStatsPacket = serde_json::from_str(json).unwrap();
        assert_eq!(pkt.name, "WITNESS-STAGE");
        assert_eq!(pkt.stats.battery_pct, 84);
        assert!(!pkt.stats.charging);
        assert_eq!(pkt.stats.thermal, ThermalState::Nominal);
    }
}
