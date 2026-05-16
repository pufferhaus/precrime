//! Title page model and TOML parser.

use serde::Deserialize;
use std::fmt;
use thiserror::Error;

pub const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Debug, Error)]
pub enum PageError {
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("unsupported schema version {0} (this binary supports {SUPPORTED_SCHEMA})")]
    UnsupportedSchema(u32),
    #[error("invalid color literal {0:?} (expected #RRGGBB or #RRGGBBAA)")]
    InvalidColor(String),
}

/// A title page: a canvas size plus a list of layers rendered top-down.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub schema: u32,
    pub name: String,
    pub canvas: (u32, u32),
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Layer {
    Text(TextLayer),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLayer {
    pub text: String,
    pub size: f64,
    pub color: Rgba,
    pub position: Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}{:02X}", self.0, self.1, self.2, self.3)
    }
}

/// Parse a page TOML string into a `Page`. Returns `PageError::UnsupportedSchema`
/// if the file's schema doesn't match this binary.
pub fn parse_page(raw: &str) -> Result<Page, PageError> {
    let parsed: TomlPage = toml::from_str(raw)?;
    if parsed.schema != SUPPORTED_SCHEMA {
        return Err(PageError::UnsupportedSchema(parsed.schema));
    }
    let layers: Result<Vec<Layer>, PageError> =
        parsed.layer.into_iter().map(layer_from_toml).collect();
    Ok(Page {
        schema: parsed.schema,
        name: parsed.name,
        canvas: (parsed.render.canvas.w, parsed.render.canvas.h),
        layers: layers?,
    })
}

// ─── private TOML shape ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TomlPage {
    schema: u32,
    name: String,
    render: TomlRender,
    #[serde(default)]
    layer: Vec<TomlLayer>,
}

#[derive(Deserialize)]
struct TomlRender {
    canvas: TomlCanvas,
}

#[derive(Deserialize)]
struct TomlCanvas {
    w: u32,
    h: u32,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TomlLayer {
    Text {
        text: String,
        size: f64,
        color: String,
        position: TomlPos,
    },
}

#[derive(Deserialize)]
struct TomlPos {
    x: i32,
    y: i32,
}

fn layer_from_toml(t: TomlLayer) -> Result<Layer, PageError> {
    match t {
        TomlLayer::Text { text, size, color, position } => Ok(Layer::Text(TextLayer {
            text,
            size,
            color: parse_color(&color)?,
            position: Position { x: position.x, y: position.y },
        })),
    }
}

fn parse_color(s: &str) -> Result<Rgba, PageError> {
    let s = s.strip_prefix('#').ok_or_else(|| PageError::InvalidColor(s.into()))?;
    let (r, g, b, a) = match s.len() {
        6 => (
            u8::from_str_radix(&s[0..2], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            u8::from_str_radix(&s[2..4], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            u8::from_str_radix(&s[4..6], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            0xFF,
        ),
        8 => (
            u8::from_str_radix(&s[0..2], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            u8::from_str_radix(&s[2..4], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            u8::from_str_radix(&s[4..6], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
            u8::from_str_radix(&s[6..8], 16).map_err(|_| PageError::InvalidColor(s.into()))?,
        ),
        _ => return Err(PageError::InvalidColor(s.into())),
    };
    Ok(Rgba(r, g, b, a))
}
