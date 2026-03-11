use serde::{Deserialize, Serialize};

/// Color representation supporting multiple color spaces.
/// Internal format is structured for performance; MCP tools handle
/// CSS color string parsing (e.g., "#3b82f6", "oklch(0.6 0.2 250)").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "space", rename_all = "lowercase")]
pub enum Color {
    Srgb {
        r: f32,
        g: f32,
        b: f32,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    Cmyk {
        c: f32,
        m: f32,
        y: f32,
        k: f32,
    },
    Oklch {
        l: f32,
        c: f32,
        h: f32,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    Lab {
        l: f32,
        a: f32,
        b: f32,
        #[serde(default = "default_alpha")]
        alpha: f32,
    },
    Icc {
        profile: String,
        channels: Vec<f32>,
    },
}

fn default_alpha() -> f32 {
    1.0
}

impl Color {
    pub fn black() -> Self {
        Self::Srgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }
    }

    pub fn white() -> Self {
        Self::Srgb {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        }
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        let (r, g, b, a) = match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                (r, g, b, 255u8)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                (r, g, b, a)
            }
            _ => return None,
        };
        Some(Self::Srgb {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        })
    }

    pub fn to_rgba_u8(&self) -> [u8; 4] {
        match self {
            Self::Srgb { r, g, b, a } => [
                (r.clamp(0.0, 1.0) * 255.0) as u8,
                (g.clamp(0.0, 1.0) * 255.0) as u8,
                (b.clamp(0.0, 1.0) * 255.0) as u8,
                (a.clamp(0.0, 1.0) * 255.0) as u8,
            ],
            // TODO: color space conversion for non-sRGB
            _ => [0, 0, 0, 255],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color() {
        let color = Color::from_hex("#3b82f6").unwrap();
        if let Color::Srgb { r, g, b, a } = color {
            assert!((r - 0.231).abs() < 0.01);
            assert!((g - 0.510).abs() < 0.01);
            assert!((b - 0.965).abs() < 0.01);
            assert!((a - 1.0).abs() < f32::EPSILON);
        } else {
            panic!("Expected Srgb color");
        }
    }

    #[test]
    fn serialize_roundtrip() {
        let color = Color::Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let json = serde_json::to_string(&color).unwrap();
        let parsed: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(color, parsed);
    }
}
