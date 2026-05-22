//! TOML config parsing for REPORT.

use serde::Deserialize;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
}

#[derive(Debug, Deserialize)]
pub struct ReportConfig {
    pub program_connector_id: u32,
    pub preview_connector_id: u32,
    pub keyboard_device: String,
    #[serde(default)]
    pub source_slot_overrides: HashMap<String, u8>,

    /// Discovery multicast group. Default: 239.42.0.1.
    #[serde(default = "default_temple_group")]
    pub temple_group: Ipv4Addr,
    #[serde(default = "default_temple_port")]
    pub temple_port: u16,

    /// TCP port for dynamic source registration. Default: 4999.
    #[serde(default = "default_reg_port")]
    pub reg_port: u16,

    /// RTP port pool for unicast sources.
    #[serde(default = "default_rtp_port_min")]
    pub rtp_port_min: u16,
    #[serde(default = "default_rtp_port_max")]
    pub rtp_port_max: u16,

    /// UDP port that sources listen on for ack packets. Default: 9998.
    #[serde(default = "default_ack_port")]
    pub ack_port: u16,

    /// UDP port for WITNESS hardware stats push. Default: 4998.
    #[serde(default = "default_stats_port")]
    pub stats_port: u16,

    /// Identity string used in Bonjour advertisement and ack payload. Default: "REPORT-MAIN".
    #[serde(default = "default_report_name")]
    pub report_name: String,
}

fn default_temple_group() -> Ipv4Addr {
    "239.42.0.1".parse().unwrap()
}
fn default_temple_port() -> u16 {
    9999
}
fn default_reg_port() -> u16 {
    4999
}
fn default_rtp_port_min() -> u16 {
    5000
}
fn default_rtp_port_max() -> u16 {
    5099
}
fn default_ack_port() -> u16 {
    9998
}
fn default_stats_port() -> u16 {
    4998
}
fn default_report_name() -> String {
    "REPORT-MAIN".into()
}

impl ReportConfig {
    pub fn from_toml(raw: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(raw)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimum_config_with_defaults() {
        let raw = r#"
program_connector_id = 32
preview_connector_id = 34
keyboard_device = "/dev/input/event0"
"#;
        let c = ReportConfig::from_toml(raw).unwrap();
        assert_eq!(c.temple_port, 9999);
        assert_eq!(c.temple_group.to_string(), "239.42.0.1");
        assert_eq!(c.reg_port, 4999);
        assert_eq!(c.rtp_port_min, 5000);
        assert_eq!(c.rtp_port_max, 5099);
        assert_eq!(c.ack_port, 9998);
        assert_eq!(c.report_name, "REPORT-MAIN");
    }

    #[test]
    fn parses_explicit_discovery_overrides() {
        let raw = r#"
program_connector_id = 32
preview_connector_id = 34
keyboard_device = "/dev/input/event0"
temple_group = "239.42.0.99"
temple_port = 12345
"#;
        let c = ReportConfig::from_toml(raw).unwrap();
        assert_eq!(c.temple_group.to_string(), "239.42.0.99");
        assert_eq!(c.temple_port, 12345);
    }

    #[test]
    fn stats_port_defaults_to_4998() {
        let raw = r#"
program_connector_id = 32
preview_connector_id = 34
keyboard_device = "/dev/input/event0"
"#;
        let c = ReportConfig::from_toml(raw).unwrap();
        assert_eq!(c.stats_port, 4998);
    }

    #[test]
    fn stats_port_can_be_overridden() {
        let raw = r#"
program_connector_id = 32
preview_connector_id = 34
keyboard_device = "/dev/input/event0"
stats_port = 5555
"#;
        let c = ReportConfig::from_toml(raw).unwrap();
        assert_eq!(c.stats_port, 5555);
    }
}
