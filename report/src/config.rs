//! TOML config parsing for REPORT.

use serde::Deserialize;
use std::collections::HashMap;
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
}

impl ReportConfig {
    pub fn from_toml(raw: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(raw)?)
    }
}
