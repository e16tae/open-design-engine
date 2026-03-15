use ode_format::color::Color;
use ode_format::node::TextData;
use ode_format::style::{BlendMode, Fill, Paint, StyleValue, VisualProps};
use ode_format::typography::*;
use ode_text::FontDatabase;

fn system_font_db() -> FontDatabase {
    FontDatabase::new_system()
}

#[test]
fn process_empty_text() {
    let db = system_font_db();
    let data = TextData {
        visual: VisualProps::default(),
        content: String::new(),
        runs: Vec::new(),
        default_style: TextStyle::default(),
        width: 100.0,
        height: 100.0,
        sizing_mode: TextSizingMode::Fixed,
    };

    let result = ode_text::process_text(&data, &db).unwrap();
    assert!(result.glyphs.is_empty());
    assert_eq!(result.computed_width, 0.0);
    assert_eq!(result.computed_height, 0.0);
}

#[test]
fn process_simple_text_with_system_font() {
    let db = system_font_db();
    if db.is_empty() {
        eprintln!("Skipping test: no system fonts available");
        return;
    }

    let data = TextData {
        visual: VisualProps::default(),
        content: "Hello".to_string(),
        runs: Vec::new(),
        default_style: TextStyle::default(),
        width: 200.0,
        height: 50.0,
        sizing_mode: TextSizingMode::Fixed,
    };

    let result = ode_text::process_text(&data, &db).unwrap();

    // "Hello" has 5 characters — should produce at least some glyphs
    // (exact count depends on shaping; ligatures may reduce it)
    assert!(
        !result.glyphs.is_empty(),
        "Expected glyph outlines for 'Hello'"
    );
    assert!(
        result.computed_height > 0.0,
        "Computed height should be positive"
    );
}

#[test]
fn process_multiline_text() {
    let db = system_font_db();
    if db.is_empty() {
        eprintln!("Skipping test: no system fonts available");
        return;
    }

    let data = TextData {
        visual: VisualProps::default(),
        content: "Line 1\nLine 2\nLine 3".to_string(),
        runs: Vec::new(),
        default_style: TextStyle::default(),
        width: 200.0,
        height: 200.0,
        sizing_mode: TextSizingMode::Fixed,
    };

    let result = ode_text::process_text(&data, &db).unwrap();
    assert!(!result.glyphs.is_empty());
    // Multiline should produce a taller result than single line
    assert!(
        result.computed_height > 20.0,
        "Multi-line text should have substantial height"
    );
}

#[test]
fn process_text_with_underline_decoration() {
    let db = system_font_db();
    if db.is_empty() {
        eprintln!("Skipping test: no system fonts available");
        return;
    }

    let mut style = TextStyle::default();
    style.decoration = TextDecoration::Underline;

    let data = TextData {
        visual: VisualProps::default(),
        content: "Underlined".to_string(),
        runs: Vec::new(),
        default_style: style,
        width: 200.0,
        height: 50.0,
        sizing_mode: TextSizingMode::Fixed,
    };

    let result = ode_text::process_text(&data, &db).unwrap();
    assert!(!result.glyphs.is_empty());
    assert!(
        !result.decorations.is_empty(),
        "Underlined text should produce decoration rects"
    );
}

#[test]
fn process_text_auto_width_mode() {
    let db = system_font_db();
    if db.is_empty() {
        eprintln!("Skipping test: no system fonts available");
        return;
    }

    let data = TextData {
        visual: VisualProps::default(),
        content: "A very long text that would normally wrap".to_string(),
        runs: Vec::new(),
        default_style: TextStyle::default(),
        width: 50.0, // narrow, but AutoWidth should ignore this
        height: 50.0,
        sizing_mode: TextSizingMode::AutoWidth,
    };

    let result = ode_text::process_text(&data, &db).unwrap();
    // AutoWidth: text should not wrap (except at \n), so computed_width > box width
    assert!(
        result.computed_width > 50.0,
        "AutoWidth should allow text wider than container"
    );
}

#[test]
fn text_data_new_fields_roundtrip() {
    let data = TextData {
        visual: VisualProps::default(),
        content: "Styled text".to_string(),
        runs: vec![TextRun {
            start: 0,
            end: 6,
            style: TextRunStyle {
                font_weight: Some(StyleValue::Raw(700)),
                fills: Some(vec![Fill {
                    paint: Paint::Solid {
                        color: StyleValue::Raw(Color::Srgb {
                            r: 1.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                    },
                    opacity: StyleValue::Raw(1.0),
                    blend_mode: BlendMode::Normal,
                    visible: true,
                }]),
                ..Default::default()
            },
        }],
        default_style: TextStyle {
            font_size: StyleValue::Raw(24.0),
            text_align: TextAlign::Center,
            ..Default::default()
        },
        width: 300.0,
        height: 100.0,
        sizing_mode: TextSizingMode::AutoHeight,
    };

    // Serialize
    let json = serde_json::to_string_pretty(&data).unwrap();

    // Verify key fields are present in JSON
    assert!(json.contains("Styled text"));
    assert!(json.contains("auto-height"));

    // Deserialize
    let parsed: TextData = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.content, "Styled text");
    assert_eq!(parsed.sizing_mode, TextSizingMode::AutoHeight);
    assert_eq!(parsed.width, 300.0);
    assert_eq!(parsed.height, 100.0);
    assert_eq!(parsed.runs.len(), 1);
    assert_eq!(parsed.runs[0].start, 0);
    assert_eq!(parsed.runs[0].end, 6);
    assert!(parsed.runs[0].style.font_weight.is_some());
}

#[test]
fn backward_compat_text_data_without_new_fields() {
    // Simulate old JSON that only has visual + content
    let json = r#"{"visual":{},"content":"Legacy text"}"#;
    let parsed: TextData = serde_json::from_str(json).unwrap();

    assert_eq!(parsed.content, "Legacy text");
    assert!(parsed.runs.is_empty());
    assert_eq!(parsed.width, 100.0); // default
    assert_eq!(parsed.height, 100.0); // default
    assert_eq!(parsed.sizing_mode, TextSizingMode::Fixed); // default
    assert_eq!(parsed.default_style.font_family.value(), "Inter"); // default
}

#[test]
fn font_db_add_and_find() {
    let mut db = FontDatabase::new();
    assert!(db.is_empty());

    // Try to find before adding — should return None
    assert!(db.find_font("Arial", 400).is_none());

    // Load a system font manually if available
    let font_paths = [
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/SFNS.ttf",
        "/Library/Fonts/Arial.ttf",
    ];

    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            if db.add_font(data) {
                break;
            }
        }
    }

    if !db.is_empty() {
        // Should now be able to find some font
        // Using generic fallback since we don't know exact family name
        assert!(db.font_count() > 0);
    }
}
