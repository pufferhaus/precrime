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
}

fn default_temple_group() -> Ipv4Addr {
    "239.42.0.1".parse().unwrap()
}
fn default_temple_port() -> u16 {
    9999
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
}
