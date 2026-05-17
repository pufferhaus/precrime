//! Dynamic source registration: TCP server, port pool, registered source set.

use crate::pipeline::{Source, Transport};
use anyhow::Result;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

// ── Port pool ────────────────────────────────────────────────────────────────

/// Allocates UDP ports from a configured range for registered sources.
pub struct PortPool {
    min: u16,
    max: u16,
    /// port → source name
    assigned: HashMap<u16, String>,
}

impl PortPool {
    pub fn new(min: u16, max: u16) -> Self {
        Self {
            min,
            max,
            assigned: HashMap::new(),
        }
    }

    /// Return already-assigned port for `name`, or allocate the next free port.
    /// Returns `None` if the pool is exhausted.
    pub fn allocate(&mut self, name: &str) -> Option<u16> {
        // Re-use existing assignment.
        if let Some((&port, _)) = self.assigned.iter().find(|(_, n)| n.as_str() == name) {
            return Some(port);
        }
        // Find the first free port in range.
        for port in self.min..=self.max {
            if !self.assigned.contains_key(&port) {
                self.assigned.insert(port, name.to_owned());
                return Some(port);
            }
        }
        None
    }

    pub fn release(&mut self, name: &str) {
        self.assigned.retain(|_, n| n.as_str() != name);
    }

    pub fn port_for(&self, name: &str) -> Option<u16> {
        self.assigned
            .iter()
            .find(|(_, n)| n.as_str() == name)
            .map(|(&port, _)| port)
    }
}

// ── Registered source ────────────────────────────────────────────────────────

pub struct RegisteredSource {
    pub name: String,
    pub host_ip: String,
    pub port: u16,
    pub last_seen: Instant,
}

// ── RegisteredSources ────────────────────────────────────────────────────────

pub struct RegisteredSources {
    pub pool: PortPool,
    pub sources: HashMap<String, RegisteredSource>,
}

impl RegisteredSources {
    pub fn new(port_min: u16, port_max: u16) -> Self {
        Self {
            pool: PortPool::new(port_min, port_max),
            sources: HashMap::new(),
        }
    }

    /// Insert or update a registered source.
    pub fn upsert(&mut self, name: &str, host_ip: &str, port: u16) {
        self.sources.insert(
            name.to_owned(),
            RegisteredSource {
                name: name.to_owned(),
                host_ip: host_ip.to_owned(),
                port,
                last_seen: Instant::now(),
            },
        );
    }

    /// Refresh `last_seen` for a known source (keep-alive).
    pub fn touch(&mut self, name: &str) {
        if let Some(src) = self.sources.get_mut(name) {
            src.last_seen = Instant::now();
        }
    }

    /// Remove sources that have not been seen within `timeout`.
    /// Releases their port pool entries and returns the evicted names.
    pub fn evict_stale(&mut self, timeout: Duration) -> Vec<String> {
        let stale: Vec<String> = self
            .sources
            .iter()
            .filter(|(_, s)| s.last_seen.elapsed() > timeout)
            .map(|(name, _)| name.clone())
            .collect();
        for name in &stale {
            self.pool.release(name);
            self.sources.remove(name);
        }
        stale
    }

    /// Produce a `Vec<Source>` suitable for the pipeline builder.
    pub fn as_sources(
        &self,
        payload_type: u8,
        clock_rate: u32,
        encoding_name: &str,
    ) -> Vec<Source> {
        self.sources
            .values()
            .map(|s| Source {
                name: s.name.clone(),
                transport: Transport::Unicast,
                port: s.port,
                payload_type,
                clock_rate,
                encoding_name: encoding_name.to_owned(),
                host: Some(s.host_ip.clone()),
            })
            .collect()
    }
}

// ── Wire types ───────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct RegistrationRequest {
    #[allow(dead_code)]
    v: String,
    name: String,
    #[allow(dead_code)]
    resolution: String,
    #[allow(dead_code)]
    fps: u32,
    #[allow(dead_code)]
    bitrate_kbps: u32,
}

#[derive(serde::Serialize)]
struct RegistrationResponse {
    assigned_port: u16,
    report_name: String,
    ack_port: u16,
}

// ── Per-connection handler ───────────────────────────────────────────────────

fn handle_registration(
    stream: TcpStream,
    registered: Arc<Mutex<RegisteredSources>>,
    change_tx: Sender<()>,
    report_name: String,
    ack_port: u16,
) {
    let peer = match stream.peer_addr() {
        Ok(a) => a.ip().to_string(),
        Err(e) => {
            warn!(error = ?e, "could not get peer address; dropping connection");
            return;
        }
    };

    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .ok();

    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if let Err(e) = reader.read_line(&mut line) {
        warn!(peer = %peer, error = ?e, "failed to read registration line");
        return;
    }

    let req: RegistrationRequest = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            warn!(peer = %peer, error = ?e, "malformed registration JSON");
            return;
        }
    };

    let port = {
        let mut reg = registered.lock().expect("registration lock");
        match reg.pool.allocate(&req.name) {
            Some(p) => {
                reg.upsert(&req.name, &peer, p);
                p
            }
            None => {
                warn!(name = %req.name, "port pool exhausted; dropping registration");
                return;
            }
        }
    };

    info!(name = %req.name, peer = %peer, port, "source registered");

    let resp = RegistrationResponse {
        assigned_port: port,
        report_name,
        ack_port,
    };
    let mut resp_bytes = match serde_json::to_vec(&resp) {
        Ok(b) => b,
        Err(e) => {
            warn!(error = ?e, "failed to serialize registration response");
            return;
        }
    };
    resp_bytes.push(b'\n');

    let mut stream = stream;
    if let Err(e) = stream.write_all(&resp_bytes) {
        warn!(peer = %peer, error = ?e, "failed to send registration response");
    }

    // Signal event loop to re-merge sources.
    let _ = change_tx.send(());
}

// ── Server spawn ─────────────────────────────────────────────────────────────

pub fn spawn_registration_server(
    reg_port: u16,
    report_name: String,
    ack_port: u16,
    registered: Arc<Mutex<RegisteredSources>>,
    change_tx: Sender<()>,
) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", reg_port))?;
    info!(port = reg_port, "registration server listening");

    std::thread::Builder::new()
        .name("report-reg-server".into())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(s) => {
                        let registered = registered.clone();
                        let change_tx = change_tx.clone();
                        let report_name = report_name.clone();
                        std::thread::Builder::new()
                            .name("report-reg-conn".into())
                            .spawn(move || {
                                handle_registration(
                                    s,
                                    registered,
                                    change_tx,
                                    report_name,
                                    ack_port,
                                );
                            })
                            .ok();
                    }
                    Err(e) => {
                        warn!(error = ?e, "registration accept error");
                    }
                }
            }
        })?;

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_pool_allocates_from_range() {
        let mut pool = PortPool::new(5000, 5002);
        let p1 = pool.allocate("A").unwrap();
        let p2 = pool.allocate("B").unwrap();
        let p3 = pool.allocate("C").unwrap();
        assert!(p1 >= 5000 && p1 <= 5002);
        assert!(p2 >= 5000 && p2 <= 5002);
        assert!(p3 >= 5000 && p3 <= 5002);
        assert_ne!(p1, p2);
        assert_ne!(p2, p3);
        // Pool exhausted
        assert!(pool.allocate("D").is_none());
    }

    #[test]
    fn port_pool_reuses_for_same_name() {
        let mut pool = PortPool::new(5000, 5099);
        let p1 = pool.allocate("CAM-1").unwrap();
        let p2 = pool.allocate("CAM-1").unwrap();
        assert_eq!(p1, p2);
    }

    #[test]
    fn port_pool_release_frees_port() {
        let mut pool = PortPool::new(5000, 5000);
        pool.allocate("A").unwrap();
        assert!(pool.allocate("B").is_none());
        pool.release("A");
        assert!(pool.allocate("B").is_some());
    }

    #[test]
    fn registered_sources_upsert_and_evict() {
        let mut reg = RegisteredSources::new(5000, 5099);
        reg.upsert("CAM-1", "10.0.0.5", 5000);
        assert_eq!(reg.sources.len(), 1);

        // Force-stale by backfilling last_seen.
        reg.sources.get_mut("CAM-1").unwrap().last_seen =
            Instant::now() - Duration::from_secs(60);

        let evicted = reg.evict_stale(Duration::from_secs(30));
        assert_eq!(evicted, vec!["CAM-1".to_owned()]);
        assert!(reg.sources.is_empty());
        // Port should be freed — re-allocate succeeds.
        assert!(reg.pool.allocate("CAM-1").is_some());
    }

    #[test]
    fn as_sources_returns_unicast_sources() {
        let mut reg = RegisteredSources::new(5000, 5099);
        reg.upsert("CAM-1", "10.0.0.5", 5010);
        let sources = reg.as_sources(96, 90000, "H264");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "CAM-1");
        assert_eq!(sources[0].transport, Transport::Unicast);
        assert_eq!(sources[0].port, 5010);
        assert_eq!(sources[0].host, Some("10.0.0.5".into()));
    }
}
