//! TOML config parsing for precog.

use serde::Deserialize;
use std::net::Ipv4Addr;

#[derive(Debug, Deserialize)]
pub struct PrecogConfig {
    /// Source display name, e.g. "PRECOG-02-CCTV-DOOR".
    pub source_name: String,
    /// V4L2 device path (Linux) or AVFoundation device index (macOS dev).
    pub device: String,
    /// Pixel format string, e.g. "UYVY" or "YUYV".
    pub format: String,
    pub width: u32,
    pub height: u32,
    /// "30/1" for NTSC, "25/1" for PAL.
    pub framerate: String,

    /// Per-source RTP multicast group, e.g. "239.42.1.1".
    pub rtp_mcast: Ipv4Addr,
    /// Per-source RTP UDP port, e.g. 5000.
    pub rtp_port: u16,
    /// Target H.264 bitrate in kbps. 4000 ≈ 1080p30 broadcast quality.
    #[serde(default = "default_bitrate_kbps")]
    pub bitrate_kbps: u32,

    /// Discovery multicast group. Default: 239.42.0.1.
    #[serde(default = "default_temple_group")]
    pub temple_group: Ipv4Addr,
    #[serde(default = "default_temple_port")]
    pub temple_port: u16,

    /// Sender host IP advertised in balls (informational only).
    #[serde(default = "default_host")]
    pub host: String,
}

fn default_bitrate_kbps() -> u32 { 4000 }
fn default_temple_group() -> Ipv4Addr { "239.42.0.1".parse().unwrap() }
fn default_temple_port() -> u16 { 9999 }
fn default_host() -> String { "0.0.0.0".into() }

impl PrecogConfig {
    pub fn from_toml(raw: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimum_config() {
        let raw = r#"
source_name = "PRECOG-01-IPHONE-STAGE"
device = "/dev/video0"
format = "UYVY"
width = 1920
height = 1080
framerate = "30/1"
rtp_mcast = "239.42.1.1"
rtp_port = 5000
"#;
        let c = PrecogConfig::from_toml(raw).unwrap();
        assert_eq!(c.source_name, "PRECOG-01-IPHONE-STAGE");
        assert_eq!(c.rtp_port, 5000);
        assert_eq!(c.bitrate_kbps, 4000);
        assert_eq!(c.temple_port, 9999);
    }

    #[test]
    fn rejects_non_ipv4_mcast() {
        let raw = r#"
source_name = "X"
device = "0"
format = "UYVY"
width = 1
height = 1
framerate = "30/1"
rtp_mcast = "not-an-ip"
rtp_port = 5000
"#;
        assert!(PrecogConfig::from_toml(raw).is_err());
    }
}
