//! Ball wire format. JSON over UDP. Versioned envelope.

use serde::{Deserialize, Serialize};

/// Versioned envelope. Internal tag `v` lets receivers reject unknown versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "v")]
pub enum Ball {
    #[serde(rename = "1")]
    V1(BallV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BallV1 {
    /// Source name, e.g. "PRECOG-01-IPHONE-STAGE".
    pub name: String,
    /// Sender host IP (informational; not used for joining the multicast group).
    pub host: String,
    pub rtp: RtpInfo,
    pub video: VideoInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RtpInfo {
    /// IPv4 multicast group for the stream, e.g. "239.42.1.1".
    pub mcast: String,
    pub port: u16,
    /// RTP payload type (96 for dynamic H.264).
    pub pt: u8,
    /// RTP clock rate in Hz (90000 for H.264).
    pub clock_rate: u32,
    /// "H264" — kept as a string so future codecs slot in without enum bump.
    pub encoding_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    /// e.g. "30/1"
    pub framerate: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HwStats {
    pub cpu_temp_mc: u32,
    pub cpu_load_pct: u8,
    pub mem_used_mb: u32,
    pub mem_total_mb: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_rssi_dbm: Option<i16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThermalState {
    Nominal,
    Fair,
    Serious,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessStats {
    pub battery_pct: u8,
    pub charging: bool,
    pub thermal: ThermalState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessStatsPacket {
    pub name: String,
    pub stats: WitnessStats,
}

impl Ball {
    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn name(&self) -> &str {
        match self {
            Ball::V1(b) => &b.name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Ball {
        Ball::V1(BallV1 {
            name: "PRECOG-01-IPHONE-STAGE".into(),
            host: "10.0.0.11".into(),
            rtp: RtpInfo {
                mcast: "239.42.1.1".into(),
                port: 5000,
                pt: 96,
                clock_rate: 90000,
                encoding_name: "H264".into(),
            },
            video: VideoInfo {
                width: 1920,
                height: 1080,
                framerate: "30/1".into(),
            },
        })
    }

    #[test]
    fn round_trips_through_json() {
        let b = sample();
        let bytes = b.to_json().unwrap();
        let parsed = Ball::from_json(&bytes).unwrap();
        assert_eq!(b, parsed);
    }

    #[test]
    fn name_accessor_returns_v1_name() {
        assert_eq!(sample().name(), "PRECOG-01-IPHONE-STAGE");
    }

    #[test]
    fn json_includes_version_tag() {
        let bytes = sample().to_json().unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains(r#""v":"1""#), "missing v tag, got {s}");
    }

    #[test]
    fn unknown_version_fails_to_parse() {
        // Structurally-complete V1 payload, only the version tag is bogus.
        // Tests that serde's tag dispatch (not missing-field errors) rejects unknown versions.
        let raw = br#"{"v":"99","name":"PRECOG-01","host":"10.0.0.11","rtp":{"mcast":"239.42.1.1","port":5000,"pt":96,"clock_rate":90000,"encoding_name":"H264"},"video":{"width":1920,"height":1080,"framerate":"30/1"}}"#;
        assert!(Ball::from_json(raw).is_err());
    }

    #[test]
    fn payload_under_one_mtu() {
        let bytes = sample().to_json().unwrap();
        // Typical Ethernet MTU 1500 minus IPv4/UDP headers ≈ 1472 bytes safe; 600 leaves ample
        // headroom and catches if the ball schema ever bloats unexpectedly.
        assert!(bytes.len() < 600, "ball grew: {} bytes", bytes.len());
    }

    #[test]
    fn hw_stats_round_trips_json() {
        let hw = HwStats {
            cpu_temp_mc: 42300,
            cpu_load_pct: 67,
            mem_used_mb: 280,
            mem_total_mb: 480,
            wifi_rssi_dbm: Some(-54),
        };
        let json = serde_json::to_string(&hw).unwrap();
        let parsed: HwStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.cpu_temp_mc, 42300);
        assert_eq!(parsed.wifi_rssi_dbm, Some(-54));
    }

    #[test]
    fn hw_stats_omits_rssi_when_none() {
        let hw = HwStats {
            cpu_temp_mc: 50000,
            cpu_load_pct: 10,
            mem_used_mb: 100,
            mem_total_mb: 1000,
            wifi_rssi_dbm: None,
        };
        let json = serde_json::to_string(&hw).unwrap();
        assert!(!json.contains("wifi_rssi_dbm"), "should be omitted when None, got {json}");
    }

    #[test]
    fn witness_stats_round_trips_json() {
        let pkt = WitnessStatsPacket {
            name: "WITNESS-STAGE".into(),
            stats: WitnessStats {
                battery_pct: 84,
                charging: false,
                thermal: ThermalState::Nominal,
            },
        };
        let json = serde_json::to_string(&pkt).unwrap();
        let parsed: WitnessStatsPacket = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "WITNESS-STAGE");
        assert_eq!(parsed.stats.battery_pct, 84);
        assert!(!parsed.stats.charging);
    }

    #[test]
    fn thermal_state_serialises_as_pascal_case() {
        let s = serde_json::to_string(&ThermalState::Serious).unwrap();
        assert_eq!(s, r#""Serious""#);
    }
}
