//! UDP multicast ball receiver with last-seen tracking + TTL eviction.

use crate::ball::Ball;
use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};
use tracing::warn;

pub struct Receiver {
    socket: std::net::UdpSocket,
    sources: HashMap<String, (Ball, Instant)>,
    eviction: Duration,
}

impl Receiver {
    /// Bind to the temple group on all interfaces. `eviction` is the
    /// silence interval after which a source is dropped from the map.
    pub fn new(group: Ipv4Addr, port: u16, eviction: Duration) -> Result<Self> {
        anyhow::ensure!(
            group.is_multicast(),
            "group {group} is not a multicast address (224.0.0.0/4)"
        );
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("create UDP socket")?;
        socket.set_reuse_address(true).context("SO_REUSEADDR")?;
        #[cfg(unix)]
        socket.set_reuse_port(true).context("SO_REUSEPORT")?;
        socket
            .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())
            .context("bind to temple port")?;
        socket
            .join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)
            .context("join temple multicast group")?;
        let std_socket: std::net::UdpSocket = socket.into();
        Ok(Self {
            socket: std_socket,
            sources: HashMap::new(),
            eviction,
        })
    }

    /// Block up to `timeout` for one datagram, parse it, and update the source
    /// map. Returns true if the source set changed (added, removed, or fields
    /// changed). Always evicts expired entries before returning.
    pub fn poll(&mut self, timeout: Duration) -> Result<bool> {
        self.socket
            .set_read_timeout(Some(timeout))
            .context("set read timeout")?;
        let mut buf = [0u8; 2048];
        let mut changed = false;
        match self.socket.recv_from(&mut buf) {
            Ok((n, _addr)) => match Ball::from_json(&buf[..n]) {
                Ok(b) => {
                    let name = b.name().to_owned();
                    let now = Instant::now();
                    let updated = match self.sources.get(&name) {
                        Some((existing, _)) => existing != &b,
                        None => true,
                    };
                    self.sources.insert(name, (b, now));
                    if updated {
                        changed = true;
                    }
                }
                Err(e) => {
                    tracing::debug!(error = ?e, "discarded malformed ball");
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(anyhow::Error::from(e).context("recv_from")),
        }
        changed |= self.evict_expired();
        Ok(changed)
    }

    fn evict_expired(&mut self) -> bool {
        let now = Instant::now();
        let before = self.sources.len();
        self.sources
            .retain(|_, (_, ts)| now.duration_since(*ts) < self.eviction);
        self.sources.len() != before
    }

    /// Snapshot of currently-live sources.
    pub fn snapshot(&self) -> Vec<Ball> {
        self.sources.values().map(|(b, _)| b.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ball::{Ball, BallV1, RtpInfo, VideoInfo};
    use crate::send::Sender;

    fn b(name: &str) -> Ball {
        Ball::V1(BallV1 {
            name: name.into(),
            host: "127.0.0.1".into(),
            rtp: RtpInfo {
                mcast: "239.42.99.1".into(),
                port: 5000,
                pt: 96,
                clock_rate: 90000,
                encoding_name: "H264".into(),
            },
            video: VideoInfo {
                width: 1280,
                height: 720,
                framerate: "30/1".into(),
            },
        })
    }

    #[test]
    fn sender_to_receiver_loopback() {
        let group = Ipv4Addr::new(239, 42, 0, 200);
        let port = 19999;
        let mut rx = Receiver::new(group, port, Duration::from_secs(6)).unwrap();
        let tx = Sender::new(group, port).unwrap();

        tx.send(&b("PRECOG-01-X")).unwrap();
        let changed = rx.poll(Duration::from_millis(500)).unwrap();
        assert!(changed, "first ball should be a change");
        let snap = rx.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].name(), "PRECOG-01-X");

        tx.send(&b("PRECOG-01-X")).unwrap();
        let changed = rx.poll(Duration::from_millis(500)).unwrap();
        assert!(!changed);
    }

    #[test]
    fn eviction_drops_silent_sources() {
        let group = Ipv4Addr::new(239, 42, 0, 201);
        let port = 19998;
        let mut rx = Receiver::new(group, port, Duration::from_millis(100)).unwrap();
        let tx = Sender::new(group, port).unwrap();

        tx.send(&b("PRECOG-02-Y")).unwrap();
        rx.poll(Duration::from_millis(500)).unwrap();
        assert_eq!(rx.snapshot().len(), 1);

        std::thread::sleep(Duration::from_millis(200));
        let changed = rx.poll(Duration::from_millis(50)).unwrap();
        assert!(changed, "eviction should report change");
        assert_eq!(rx.snapshot().len(), 0);
    }

    #[test]
    fn receiver_rejects_non_multicast_group() {
        let r = Receiver::new(Ipv4Addr::new(192, 168, 1, 1), 20000, Duration::from_secs(6));
        assert!(r.is_err(), "expected non-multicast address to be rejected");
    }
}
