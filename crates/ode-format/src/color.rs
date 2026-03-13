use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

/// Color representation supporting multiple color spaces.
/// Internal format is structured for performance; MCP tools handle
/// CSS color string parsing (e.g., "#3b82f6", "oklch(0.6 0.2 250)").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "space", rename_all = "lowercase")]
pub enum Color {
    Srgb {
        r: f32,
        g: f32,
        b: f32,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    #[serde(rename = "display-p3")]
    DisplayP3 {
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
        #[serde(default = "default_alpha")]
        a: f32,
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
        a_axis: f32,
        b_axis: f32,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    Icc {
        profile: String,
        channels: Vec<f32>,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    Spot {
        name: String,
        fallback_rgb: [f32; 3],
        #[serde(default = "default_alpha")]
        a: f32,
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

    pub fn alpha(&self) -> f32 {
        match self {
            Self::Srgb { a, .. }
            | Self::DisplayP3 { a, .. }
            | Self::Cmyk { a, .. }
            | Self::Oklch { a, .. }
            | Self::Lab { a, .. }
            | Self::Icc { a, .. }
            | Self::Spot { a, .. } => *a,
        }
    }

    pub fn with_alpha(&self, new_alpha: f32) -> Self {
        match self {
            Self::Srgb { r, g, b, .. } => Self::Srgb { r: *r, g: *g, b: *b, a: new_alpha },
            Self::DisplayP3 { r, g, b, .. } => Self::DisplayP3 { r: *r, g: *g, b: *b, a: new_alpha },
            Self::Cmyk { c, m, y, k, .. } => Self::Cmyk { c: *c, m: *m, y: *y, k: *k, a: new_alpha },
            Self::Oklch { l, c, h, .. } => Self::Oklch { l: *l, c: *c, h: *h, a: new_alpha },
            Self::Lab { l, a_axis, b_axis, .. } => Self::Lab { l: *l, a_axis: *a_axis, b_axis: *b_axis, a: new_alpha },
            Self::Icc { profile, channels, .. } => Self::Icc { profile: profile.clone(), channels: channels.clone(), a: new_alpha },
            Self::Spot { name, fallback_rgb, .. } => Self::Spot { name: name.clone(), fallback_rgb: *fallback_rgb, a: new_alpha },
        }
    }

    pub fn to_rgba_u8(&self) -> [u8; 4] {
        match self {
            Self::Srgb { r, g, b, a } | Self::DisplayP3 { r, g, b, a } => [
                (r.clamp(0.0, 1.0) * 255.0) as u8,
                (g.clamp(0.0, 1.0) * 255.0) as u8,
                (b.clamp(0.0, 1.0) * 255.0) as u8,
                (a.clamp(0.0, 1.0) * 255.0) as u8,
            ],
            Self::Spot { fallback_rgb, a, .. } => [
                (fallback_rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                (fallback_rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                (fallback_rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
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

    #[test]
    fn display_p3_roundtrip() {
        let color = Color::DisplayP3 { r: 1.0, g: 0.5, b: 0.0, a: 1.0 };
        let json = serde_json::to_string(&color).unwrap();
        let parsed: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(color, parsed);
    }

    #[test]
    fn cmyk_has_alpha() {
        let color = Color::Cmyk { c: 1.0, m: 0.0, y: 0.0, k: 0.0, a: 0.5 };
        assert!((color.alpha() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn spot_color_roundtrip() {
        let color = Color::Spot {
            name: "Pantone 2728 C".to_string(),
            fallback_rgb: [0.0, 0.318, 0.729],
            a: 1.0,
        };
        let json = serde_json::to_string(&color).unwrap();
        let parsed: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(color, parsed);
    }

    #[test]
    fn alpha_consistent_across_all_variants() {
        let variants: Vec<Color> = vec![
            Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 0.5 },
            Color::DisplayP3 { r: 1.0, g: 0.0, b: 0.0, a: 0.5 },
            Color::Cmyk { c: 1.0, m: 0.0, y: 0.0, k: 0.0, a: 0.5 },
            Color::Oklch { l: 0.7, c: 0.15, h: 30.0, a: 0.5 },
            Color::Lab { l: 50.0, a_axis: 20.0, b_axis: -10.0, a: 0.5 },
            Color::Icc { profile: "sRGB".to_string(), channels: vec![1.0, 0.0, 0.0], a: 0.5 },
            Color::Spot { name: "Gold".to_string(), fallback_rgb: [0.8, 0.7, 0.2], a: 0.5 },
        ];
        for color in &variants {
            assert!((color.alpha() - 0.5).abs() < f32::EPSILON, "Failed for {:?}", color);
        }
    }

    #[test]
    fn with_alpha_preserves_color() {
        let color = Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
        let transparent = color.with_alpha(0.3);
        assert!((transparent.alpha() - 0.3).abs() < f32::EPSILON);
        if let Color::Srgb { r, g, b, .. } = transparent {
            assert!((r - 1.0).abs() < f32::EPSILON);
            assert!((g - 0.0).abs() < f32::EPSILON);
            assert!((b - 0.0).abs() < f32::EPSILON);
        }
    }
}
