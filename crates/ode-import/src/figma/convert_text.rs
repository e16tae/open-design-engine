//! Figma text style conversion: TypeStyle → TextStyle, TextRun, TextSizingMode.

use std::collections::HashMap;

use ode_format::style::StyleValue;
use ode_format::typography::{
    LineHeight, OpenTypeFeature, TextAlign, TextDecoration, TextRun, TextRunStyle, TextSizingMode,
    TextStyle, TextTransform, VerticalAlign,
};

use super::convert_style::convert_fill;
use super::types::FigmaTypeStyle;

// ─── convert_text_style ──────────────────────────────────────────────────────

/// Convert a Figma `TypeStyle` into an ODE `TextStyle`.
pub fn convert_text_style(fts: &FigmaTypeStyle) -> TextStyle {
    let font_family = StyleValue::Raw(
        fts.font_family
            .clone()
            .unwrap_or_else(|| "Inter".to_string()),
    );

    let font_weight = StyleValue::Raw(fts.font_weight.unwrap_or(400.0).round() as u16);

    let font_size = StyleValue::Raw(fts.font_size.unwrap_or(16.0));

    let text_align = match fts.text_align_horizontal.as_deref() {
        Some("CENTER") => TextAlign::Center,
        Some("RIGHT") => TextAlign::Right,
        Some("JUSTIFIED") => TextAlign::Justify,
        _ => TextAlign::Left,
    };

    let vertical_align = match fts.text_align_vertical.as_deref() {
        Some("CENTER") => VerticalAlign::Middle,
        Some("BOTTOM") => VerticalAlign::Bottom,
        _ => VerticalAlign::Top,
    };

    let letter_spacing = StyleValue::Raw(fts.letter_spacing.unwrap_or(0.0));

    let line_height = match fts.line_height_unit.as_deref() {
        Some("PIXELS") => LineHeight::Fixed {
            value: StyleValue::Raw(fts.line_height_px.unwrap_or(0.0)),
        },
        Some("FONT_SIZE_%") => LineHeight::Percent {
            value: StyleValue::Raw(fts.line_height_percent_font_size.unwrap_or(100.0) / 100.0),
        },
        _ => LineHeight::Auto,
    };

    let decoration = match fts.text_decoration.as_deref() {
        Some("UNDERLINE") => TextDecoration::Underline,
        Some("STRIKETHROUGH") => TextDecoration::Strikethrough,
        _ => TextDecoration::None,
    };

    let transform = match fts.text_case.as_deref() {
        Some("UPPER") => TextTransform::Uppercase,
        Some("LOWER") => TextTransform::Lowercase,
        Some("TITLE") => TextTransform::Capitalize,
        _ => TextTransform::None,
    };

    let paragraph_spacing = StyleValue::Raw(fts.paragraph_spacing.unwrap_or(0.0));

    let opentype_features = fts
        .opentype_flags
        .as_ref()
        .map(convert_opentype_flags)
        .unwrap_or_default();

    TextStyle {
        font_family,
        font_weight,
        font_size,
        line_height,
        letter_spacing,
        paragraph_spacing,
        text_align,
        vertical_align,
        decoration,
        transform,
        opentype_features,
        variable_axes: Vec::new(),
    }
}

// ─── convert_text_runs ───────────────────────────────────────────────────────

/// Convert Figma character style overrides into ODE `TextRun` spans.
///
/// `content` is the UTF-8 text string.
/// `overrides` is a per-UTF-16-code-unit array of style-table indices.
/// `table` maps string keys ("1", "2", ...) to `FigmaTypeStyle` overrides.
///
/// Style index 0 means "use the default style" — no `TextRun` is created for
/// index-0 spans. Only non-zero indices that exist in `table` produce runs.
pub fn convert_text_runs(
    content: &str,
    overrides: &[usize],
    table: &HashMap<String, FigmaTypeStyle>,
) -> Vec<TextRun> {
    if overrides.is_empty() {
        return Vec::new();
    }

    // Group consecutive same-index code-units into (style_index, utf16_start, utf16_end).
    let groups = group_consecutive(overrides);

    let mut runs = Vec::new();
    for (style_idx, utf16_start, utf16_end) in groups {
        if style_idx == 0 {
            continue;
        }
        let key = style_idx.to_string();
        if let Some(fts) = table.get(&key) {
            let byte_start = utf16_index_to_byte_offset(content, utf16_start);
            let byte_end = utf16_index_to_byte_offset(content, utf16_end);
            let style = figma_type_style_to_run_style(fts);
            runs.push(TextRun {
                start: byte_start,
                end: byte_end,
                style,
            });
        }
    }

    runs
}

// ─── convert_sizing_mode ─────────────────────────────────────────────────────

/// Convert a Figma `textAutoResize` string to an ODE `TextSizingMode`.
pub fn convert_sizing_mode(s: Option<&str>) -> TextSizingMode {
    match s {
        Some("HEIGHT") => TextSizingMode::AutoHeight,
        Some("WIDTH_AND_HEIGHT") => TextSizingMode::AutoWidth,
        _ => TextSizingMode::Fixed,
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Convert a UTF-16 code-unit index to a UTF-8 byte offset within `content`.
///
/// Walks through `content` character by character, counting UTF-16 code units
/// per char (1 for BMP, 2 for non-BMP). Returns the byte offset once the
/// UTF-16 count reaches `utf16_idx`.
fn utf16_index_to_byte_offset(content: &str, utf16_idx: usize) -> usize {
    let mut utf16_count: usize = 0;
    for (byte_offset, ch) in content.char_indices() {
        if utf16_count >= utf16_idx {
            return byte_offset;
        }
        utf16_count += ch.len_utf16();
    }
    // If utf16_idx >= total UTF-16 length, return byte length of the string.
    content.len()
}

/// Group consecutive identical values into (value, start, end) tuples.
fn group_consecutive(overrides: &[usize]) -> Vec<(usize, usize, usize)> {
    if overrides.is_empty() {
        return Vec::new();
    }
    let mut groups = Vec::new();
    let mut current_idx = overrides[0];
    let mut start: usize = 0;

    for (i, &idx) in overrides.iter().enumerate().skip(1) {
        if idx != current_idx {
            groups.push((current_idx, start, i));
            current_idx = idx;
            start = i;
        }
    }
    groups.push((current_idx, start, overrides.len()));
    groups
}

/// Convert OpenType flags from a Figma HashMap to a Vec of ODE features.
fn convert_opentype_flags(flags: &HashMap<String, u32>) -> Vec<OpenTypeFeature> {
    flags
        .iter()
        .filter_map(|(key, &value)| {
            let bytes = key.as_bytes();
            if bytes.len() == 4 {
                Some(OpenTypeFeature {
                    tag: [bytes[0], bytes[1], bytes[2], bytes[3]],
                    enabled: value > 0,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Convert a `FigmaTypeStyle` to a `TextRunStyle` (all fields are Optional).
fn figma_type_style_to_run_style(fts: &FigmaTypeStyle) -> TextRunStyle {
    let mut warnings = Vec::new();

    let fills = fts.fills.as_ref().map(|paints| {
        paints
            .iter()
            .filter_map(|p| convert_fill(p, &mut warnings))
            .collect()
    });

    TextRunStyle {
        font_family: fts.font_family.as_ref().map(|f| StyleValue::Raw(f.clone())),
        font_weight: fts.font_weight.map(|w| StyleValue::Raw(w.round() as u16)),
        font_size: fts.font_size.map(StyleValue::Raw),
        line_height: fts.line_height_unit.as_deref().map(|unit| match unit {
            "PIXELS" => LineHeight::Fixed {
                value: StyleValue::Raw(fts.line_height_px.unwrap_or(0.0)),
            },
            "FONT_SIZE_%" => LineHeight::Percent {
                value: StyleValue::Raw(fts.line_height_percent_font_size.unwrap_or(100.0) / 100.0),
            },
            _ => LineHeight::Auto,
        }),
        letter_spacing: fts.letter_spacing.map(StyleValue::Raw),
        decoration: fts.text_decoration.as_deref().map(|d| match d {
            "UNDERLINE" => TextDecoration::Underline,
            "STRIKETHROUGH" => TextDecoration::Strikethrough,
            _ => TextDecoration::None,
        }),
        transform: fts.text_case.as_deref().map(|c| match c {
            "UPPER" => TextTransform::Uppercase,
            "LOWER" => TextTransform::Lowercase,
            "TITLE" => TextTransform::Capitalize,
            _ => TextTransform::None,
        }),
        opentype_features: fts.opentype_flags.as_ref().map(convert_opentype_flags),
        variable_axes: None,
        fills,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_text_style_all_fields() {
        let mut ot = HashMap::new();
        ot.insert("liga".to_string(), 1u32);
        ot.insert("kern".to_string(), 0u32);

        let fts = FigmaTypeStyle {
            font_family: Some("Roboto".into()),
            font_weight: Some(700.4),
            font_size: Some(24.0),
            text_align_horizontal: Some("CENTER".into()),
            text_align_vertical: Some("BOTTOM".into()),
            letter_spacing: Some(1.5),
            line_height_px: Some(32.0),
            line_height_percent_font_size: None,
            line_height_unit: Some("PIXELS".into()),
            text_decoration: Some("UNDERLINE".into()),
            text_case: Some("UPPER".into()),
            paragraph_spacing: Some(8.0),
            opentype_flags: Some(ot),
            ..Default::default()
        };

        let ts = convert_text_style(&fts);
        assert_eq!(ts.font_family.value(), "Roboto");
        assert_eq!(ts.font_weight.value(), 700);
        assert!((ts.font_size.value() - 24.0).abs() < f32::EPSILON);
        assert_eq!(ts.text_align, TextAlign::Center);
        assert_eq!(ts.vertical_align, VerticalAlign::Bottom);
        assert!((ts.letter_spacing.value() - 1.5).abs() < f32::EPSILON);
        assert_eq!(
            ts.line_height,
            LineHeight::Fixed {
                value: StyleValue::Raw(32.0)
            }
        );
        assert_eq!(ts.decoration, TextDecoration::Underline);
        assert_eq!(ts.transform, TextTransform::Uppercase);
        assert!((ts.paragraph_spacing.value() - 8.0).abs() < f32::EPSILON);
        assert_eq!(ts.opentype_features.len(), 2);
    }

    #[test]
    fn test_convert_text_runs_ascii() {
        // "Hello World" — 5 chars default, 6 chars override #1
        let content = "Hello World";
        let overrides = vec![0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1];
        let mut table = HashMap::new();
        table.insert(
            "1".to_string(),
            FigmaTypeStyle {
                font_weight: Some(700.0),
                ..Default::default()
            },
        );

        let runs = convert_text_runs(content, &overrides, &table);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].start, 5); // byte offset of ' '
        assert_eq!(runs[0].end, 11); // byte length of "Hello World"
        assert_eq!(runs[0].style.font_weight, Some(StyleValue::Raw(700)));
    }

    #[test]
    fn test_convert_text_runs_emoji_utf16() {
        // "A😀B" — 'A' = 1 UTF-16 unit, '😀' = 2 UTF-16 units, 'B' = 1 UTF-16 unit
        // Total UTF-16 units: 4
        // overrides: [0, 1, 1, 0] (indices 1-2 are the emoji, override #1)
        let content = "A\u{1F600}B";
        let overrides = vec![0, 1, 1, 0];
        let mut table = HashMap::new();
        table.insert(
            "1".to_string(),
            FigmaTypeStyle {
                font_size: Some(32.0),
                ..Default::default()
            },
        );

        let runs = convert_text_runs(content, &overrides, &table);
        assert_eq!(runs.len(), 1);
        // 'A' is 1 byte, '😀' is 4 bytes in UTF-8
        assert_eq!(runs[0].start, 1); // after 'A'
        assert_eq!(runs[0].end, 5); // after 'A' + '😀' (1+4)
        assert_eq!(runs[0].style.font_size, Some(StyleValue::Raw(32.0)));
    }

    #[test]
    fn test_convert_sizing_mode_all_variants() {
        assert_eq!(convert_sizing_mode(None), TextSizingMode::Fixed);
        assert_eq!(convert_sizing_mode(Some("NONE")), TextSizingMode::Fixed);
        assert_eq!(
            convert_sizing_mode(Some("HEIGHT")),
            TextSizingMode::AutoHeight
        );
        assert_eq!(
            convert_sizing_mode(Some("WIDTH_AND_HEIGHT")),
            TextSizingMode::AutoWidth
        );
    }

    #[test]
    fn test_convert_text_style_line_height_pixels() {
        let fts = FigmaTypeStyle {
            line_height_unit: Some("PIXELS".into()),
            line_height_px: Some(28.0),
            ..Default::default()
        };
        let ts = convert_text_style(&fts);
        assert_eq!(
            ts.line_height,
            LineHeight::Fixed {
                value: StyleValue::Raw(28.0)
            }
        );
    }

    #[test]
    fn test_convert_text_style_line_height_percent() {
        let fts = FigmaTypeStyle {
            line_height_unit: Some("FONT_SIZE_%".into()),
            line_height_percent_font_size: Some(150.0),
            ..Default::default()
        };
        let ts = convert_text_style(&fts);
        assert_eq!(
            ts.line_height,
            LineHeight::Percent {
                value: StyleValue::Raw(1.5)
            }
        );
    }

    #[test]
    fn test_convert_text_style_line_height_auto() {
        let fts = FigmaTypeStyle {
            line_height_unit: Some("INTRINSIC_%".into()),
            ..Default::default()
        };
        let ts = convert_text_style(&fts);
        assert_eq!(ts.line_height, LineHeight::Auto);
    }

    #[test]
    fn test_convert_text_runs_default_only_no_runs() {
        let content = "Hello";
        let overrides = vec![0, 0, 0, 0, 0];
        let table: HashMap<String, FigmaTypeStyle> = HashMap::new();

        let runs = convert_text_runs(content, &overrides, &table);
        assert!(runs.is_empty());
    }

    #[test]
    fn test_convert_text_runs_empty_overrides() {
        let content = "Hello";
        let overrides: Vec<usize> = Vec::new();
        let table: HashMap<String, FigmaTypeStyle> = HashMap::new();

        let runs = convert_text_runs(content, &overrides, &table);
        assert!(runs.is_empty());
    }

    #[test]
    fn test_utf16_index_to_byte_offset_basic() {
        let content = "Hello";
        assert_eq!(utf16_index_to_byte_offset(content, 0), 0);
        assert_eq!(utf16_index_to_byte_offset(content, 3), 3);
        assert_eq!(utf16_index_to_byte_offset(content, 5), 5);
    }

    #[test]
    fn test_utf16_index_to_byte_offset_emoji() {
        // '😀' is U+1F600 — 2 UTF-16 code units, 4 UTF-8 bytes
        let content = "A\u{1F600}B";
        assert_eq!(utf16_index_to_byte_offset(content, 0), 0); // start
        assert_eq!(utf16_index_to_byte_offset(content, 1), 1); // after 'A'
        assert_eq!(utf16_index_to_byte_offset(content, 3), 5); // after emoji
        assert_eq!(utf16_index_to_byte_offset(content, 4), 6); // after 'B'
    }

    #[test]
    fn test_opentype_flags_conversion() {
        let mut flags = HashMap::new();
        flags.insert("liga".to_string(), 1u32);
        flags.insert("kern".to_string(), 0u32);

        let features = convert_opentype_flags(&flags);
        assert_eq!(features.len(), 2);

        let liga = features.iter().find(|f| &f.tag == b"liga").unwrap();
        assert!(liga.enabled);

        let kern = features.iter().find(|f| &f.tag == b"kern").unwrap();
        assert!(!kern.enabled);
    }
}
