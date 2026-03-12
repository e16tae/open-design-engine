use serde::{Deserialize, Serialize};
use crate::style::StyleValue;

/// Font family name. Type alias for spec alignment — allows future refinement.
pub type FontFamily = String;
/// Font weight (1–1000). Type alias for spec alignment.
pub type FontWeight = u16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font_family: StyleValue<FontFamily>,
    pub font_weight: StyleValue<FontWeight>,
    pub font_size: StyleValue<f32>,
    pub line_height: LineHeight,
    pub letter_spacing: StyleValue<f32>,
    pub paragraph_spacing: StyleValue<f32>,
    pub text_align: TextAlign,
    pub vertical_align: VerticalAlign,
    pub decoration: TextDecoration,
    pub transform: TextTransform,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opentype_features: Vec<OpenTypeFeature>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variable_axes: Vec<VariableFontAxis>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: StyleValue::Raw("Inter".into()),
            font_weight: StyleValue::Raw(400 as FontWeight),
            font_size: StyleValue::Raw(16.0),
            line_height: LineHeight::Auto,
            letter_spacing: StyleValue::Raw(0.0),
            paragraph_spacing: StyleValue::Raw(0.0),
            text_align: TextAlign::Left,
            vertical_align: VerticalAlign::Top,
            decoration: TextDecoration::None,
            transform: TextTransform::None,
            opentype_features: Vec::new(),
            variable_axes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LineHeight {
    Auto,
    Fixed { value: StyleValue<f32> },
    Percent { value: StyleValue<f32> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextAlign { Left, Center, Right, Justify }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerticalAlign { Top, Middle, Bottom }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextDecoration { None, Underline, Strikethrough, Both }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextTransform { None, Uppercase, Lowercase, Capitalize }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenTypeFeature {
    pub tag: [u8; 4],
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableFontAxis {
    pub tag: [u8; 4],
    pub value: StyleValue<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::StyleValue;

    #[test]
    fn text_style_roundtrip() {
        let style = TextStyle::default();
        let json = serde_json::to_string(&style).unwrap();
        let parsed: TextStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(style, parsed);
    }

    #[test]
    fn opentype_feature_tag() {
        let feat = OpenTypeFeature { tag: *b"liga", enabled: true };
        assert_eq!(&feat.tag, b"liga");
    }

    #[test]
    fn variable_axis_roundtrip() {
        let axis = VariableFontAxis {
            tag: *b"wght",
            value: StyleValue::Raw(700.0),
        };
        let json = serde_json::to_string(&axis).unwrap();
        let parsed: VariableFontAxis = serde_json::from_str(&json).unwrap();
        assert_eq!(axis, parsed);
    }
}
