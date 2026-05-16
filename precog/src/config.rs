//! TOML config parsing for precog.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PrecogConfig {
    /// NDI display name, e.g. "PRECOG-02-CCTV-DOOR".
    pub ndi_name: String,
    /// V4L2 device path, e.g. "/dev/video0".
    pub device: String,
    /// Pixel format string, e.g. "UYVY" or "YUYV".
    pub format: String,
    pub width: u32,
    pub height: u32,
    /// "30/1" for NTSC, "25/1" for PAL.
    pub framerate: String,
    /// Some installs of gst-plugin-rs need `ndisinkcombiner`; others go straight to `ndisink`.
    /// Default true (with combiner).
    #[serde(default = "default_combiner")]
    pub use_combiner: bool,
}

fn default_combiner() -> bool {
    true
}

impl PrecogConfig {
    pub fn from_toml(raw: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(raw)
    }
}
