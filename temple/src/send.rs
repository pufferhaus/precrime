//! UDP multicast ball sender. Owns a socket configured for IPv4 multicast
//! TX with TTL=1 (admin-scoped, never leaves the LAN segment).

use crate::ball::Ball;
use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddrV4};

pub struct Sender {
    socket: std::net::UdpSocket,
    dest: SocketAddrV4,
}

impl Sender {
    /// Bind an ephemeral UDP socket and configure it to send to `group:port`
    /// with multicast TTL=1.
    pub fn new(group: Ipv4Addr, port: u16) -> Result<Self> {
        anyhow::ensure!(
            group.is_multicast(),
            "group {group} is not a multicast address (224.0.0.0/4)"
        );
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("create UDP socket")?;
        socket.set_multicast_ttl_v4(1).context("set mcast TTL")?;
        socket
            .set_multicast_loop_v4(true)
            .context("enable mcast loopback")?;
        socket
            .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())
            .context("bind ephemeral port")?;
        let std_socket: std::net::UdpSocket = socket.into();
        Ok(Self {
            socket: std_socket,
            dest: SocketAddrV4::new(group, port),
        })
    }

    pub fn send(&self, ball: &Ball) -> Result<()> {
        let bytes = ball.to_json().context("serialize ball")?;
        self.socket
            .send_to(&bytes, self.dest)
            .context("send ball datagram")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ball::{BallV1, RtpInfo, VideoInfo};

    #[test]
    fn sender_constructs_with_valid_group() {
        let s = Sender::new(Ipv4Addr::new(239, 42, 0, 1), 9999);
        assert!(s.is_ok());
    }

    #[test]
    fn send_does_not_error_when_no_listener() {
        let s = Sender::new(Ipv4Addr::new(239, 42, 0, 1), 9999).unwrap();
        let b = Ball::V1(BallV1 {
            name: "PRECOG-99-TEST".into(),
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
        });
        assert!(s.send(&b).is_ok());
    }

    #[test]
    fn sender_rejects_non_multicast_group() {
        let s = Sender::new(Ipv4Addr::new(192, 168, 1, 1), 9999);
        assert!(s.is_err(), "expected non-multicast address to be rejected");
    }
}
