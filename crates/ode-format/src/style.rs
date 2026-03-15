use crate::color::Color;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ─── Token ID Types (shared with tokens module) ───
pub type CollectionId = u32;
pub type TokenId = u32;

// ─── Token Reference ───
/// Reference to a design token. Used by both StyleValue::Bound and TokenResolve::Alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct TokenRef {
    pub collection_id: CollectionId,
    pub token_id: TokenId,
}

// ─── StyleValue<T> ───
/// A value that is either a raw value or bound to a design token.
/// Raw: bare value (e.g., `1.0`). Bound: `{"token":{...},"resolved":...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum StyleValue<T> {
    Bound { token: TokenRef, resolved: T },
    Raw(T),
}

impl<T: Clone> StyleValue<T> {
    pub fn value(&self) -> T {
        match self {
            Self::Raw(v) => v.clone(),
            Self::Bound { resolved, .. } => resolved.clone(),
        }
    }
    pub fn is_bound(&self) -> bool {
        matches!(self, Self::Bound { .. })
    }
}

// ─── Geometry ───
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// ─── BlendMode ───
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}
impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
}

// ─── Paint ───
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Paint {
    Solid {
        color: StyleValue<Color>,
    },
    LinearGradient {
        stops: Vec<GradientStop>,
        start: Point,
        end: Point,
    },
    RadialGradient {
        stops: Vec<GradientStop>,
        center: Point,
        radius: Point,
    },
    AngularGradient {
        stops: Vec<GradientStop>,
        center: Point,
        angle: f32,
    },
    DiamondGradient {
        stops: Vec<GradientStop>,
        center: Point,
        radius: Point,
    },
    MeshGradient(Box<MeshGradientData>),
    ImageFill {
        source: ImageSource,
        mode: ImageFillMode,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GradientStop {
    pub position: f32,
    pub color: StyleValue<Color>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeshGradientData {
    pub rows: u32,
    pub columns: u32,
    pub points: Vec<MeshPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MeshPoint {
    pub position: Point,
    pub color: StyleValue<Color>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ImageSource {
    Embedded { data: Vec<u8> },
    Linked { path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ImageFillMode {
    Fill,
    Fit,
    Crop,
    Tile,
}

// ─── Fill ───
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Fill {
    pub paint: Paint,
    pub opacity: StyleValue<f32>,
    pub blend_mode: BlendMode,
    pub visible: bool,
}

// ─── Stroke ───
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stroke {
    pub paint: Paint,
    pub width: StyleValue<f32>,
    pub position: StrokePosition,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f32,
    pub dash: Option<DashPattern>,
    pub opacity: StyleValue<f32>,
    pub blend_mode: BlendMode,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StrokePosition {
    Inside,
    Outside,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StrokeCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StrokeJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DashPattern {
    pub segments: Vec<f32>,
    pub offset: f32,
}

// ─── Effect ───
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Effect {
    DropShadow {
        color: StyleValue<Color>,
        offset: Point,
        blur: StyleValue<f32>,
        spread: StyleValue<f32>,
    },
    InnerShadow {
        color: StyleValue<Color>,
        offset: Point,
        blur: StyleValue<f32>,
        spread: StyleValue<f32>,
    },
    LayerBlur {
        radius: StyleValue<f32>,
    },
    BackgroundBlur {
        radius: StyleValue<f32>,
    },
}

// ─── Composable Property Structs ───
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct VisualProps {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<Fill>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strokes: Vec<Stroke>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn style_value_raw() {
        let val: StyleValue<f32> = StyleValue::Raw(1.0);
        assert!((val.value() - 1.0).abs() < f32::EPSILON);
        assert!(!val.is_bound());
    }

    #[test]
    fn style_value_bound() {
        let val: StyleValue<f32> = StyleValue::Bound {
            token: TokenRef {
                collection_id: 0,
                token_id: 0,
            },
            resolved: 42.0,
        };
        assert!((val.value() - 42.0).abs() < f32::EPSILON);
        assert!(val.is_bound());
    }

    #[test]
    fn style_value_roundtrip() {
        let val: StyleValue<f32> = StyleValue::Raw(3.14);
        let json = serde_json::to_string(&val).unwrap();
        let parsed: StyleValue<f32> = serde_json::from_str(&json).unwrap();
        assert_eq!(val, parsed);
    }

    #[test]
    fn fill_with_solid_paint() {
        let fill = Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::black()),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        };
        let json = serde_json::to_string(&fill).unwrap();
        let parsed: Fill = serde_json::from_str(&json).unwrap();
        assert_eq!(fill, parsed);
    }

    #[test]
    fn linear_gradient_roundtrip() {
        let paint = Paint::LinearGradient {
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: StyleValue::Raw(Color::black()),
                },
                GradientStop {
                    position: 1.0,
                    color: StyleValue::Raw(Color::white()),
                },
            ],
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 1.0, y: 1.0 },
        };
        let json = serde_json::to_string(&paint).unwrap();
        let parsed: Paint = serde_json::from_str(&json).unwrap();
        assert_eq!(paint, parsed);
    }

    #[test]
    fn stroke_roundtrip() {
        let stroke = Stroke {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::black()),
            },
            width: StyleValue::Raw(2.0),
            position: StrokePosition::Center,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            miter_limit: 4.0,
            dash: Some(DashPattern {
                segments: vec![5.0, 3.0],
                offset: 0.0,
            }),
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        };
        let json = serde_json::to_string(&stroke).unwrap();
        let parsed: Stroke = serde_json::from_str(&json).unwrap();
        assert_eq!(stroke, parsed);
    }

    #[test]
    fn drop_shadow_roundtrip() {
        let effect = Effect::DropShadow {
            color: StyleValue::Raw(Color::Srgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.25,
            }),
            offset: Point { x: 0.0, y: 4.0 },
            blur: StyleValue::Raw(8.0),
            spread: StyleValue::Raw(0.0),
        };
        let json = serde_json::to_string(&effect).unwrap();
        let parsed: Effect = serde_json::from_str(&json).unwrap();
        assert_eq!(effect, parsed);
    }

    #[test]
    fn visual_props_default_is_empty() {
        let vp = VisualProps::default();
        assert!(vp.fills.is_empty());
        assert!(vp.strokes.is_empty());
        assert!(vp.effects.is_empty());
    }
}
