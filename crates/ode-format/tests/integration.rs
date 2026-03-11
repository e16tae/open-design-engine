use ode_format::color::Color;
use ode_format::document::{Document, View, ViewId, ViewKind};
use ode_format::node::Node;
use ode_format::style::*;

/// End-to-end test: create a document with nodes, styles, tokens, and views,
/// serialize to JSON, deserialize, and verify equality.
#[test]
fn full_document_roundtrip() {
    let mut doc = Document::new("Integration Test");

    // Create a frame with a fill
    let mut frame = Node::new_frame("Card");
    if let ode_format::node::NodeKind::Frame(ref mut data) = frame.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::from_hex("#3b82f6").unwrap()),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
        data.visual.strokes.push(Stroke {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::black()),
            },
            width: StyleValue::Raw(1.0),
            position: StrokePosition::Inside,
            cap: StrokeCap::Butt,
            join: StrokeJoin::Miter,
            miter_limit: 4.0,
            dash: None,
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }

    // Create a text node
    let text = Node::new_text("Title", "Hello, ODE!");

    // Insert nodes and build tree
    let frame_id = doc.nodes.insert(frame);
    let text_id = doc.nodes.insert(text);

    // Add text as child of frame
    if let ode_format::node::NodeKind::Frame(ref mut data) = doc.nodes[frame_id].kind {
        data.container.children.push(text_id);
    }

    // Register frame as canvas root
    doc.canvas.push(frame_id);

    // Add an export view
    doc.views.push(View {
        id: ViewId(0),
        name: "PNG Export".to_string(),
        kind: ViewKind::Export { targets: vec![] },
    });

    // Serialize
    let json = serde_json::to_string_pretty(&doc).unwrap();

    // Verify JSON is not empty and contains key fields
    assert!(json.contains("Integration Test"));
    assert!(json.contains("Card"));
    assert!(json.contains("Hello, ODE!"));
    assert!(json.contains("3b82f6") || json.contains("0.231"));

    // Deserialize
    let parsed: Document = serde_json::from_str(&json).unwrap();

    // Verify document structure
    assert_eq!(parsed.name, "Integration Test");
    assert_eq!(parsed.canvas.len(), 1);
    assert_eq!(parsed.views.len(), 1);
    assert_eq!(parsed.format_version, ode_format::document::Version(0, 1, 0));

    // Verify parent-child relationship survived roundtrip
    let parsed_frame_id = parsed.canvas[0];
    if let ode_format::node::NodeKind::Frame(ref data) = parsed.nodes[parsed_frame_id].kind {
        assert_eq!(data.container.children.len(), 1, "Frame should have 1 child after roundtrip");
    } else {
        panic!("Expected Frame node");
    }

    // Verify working color space roundtrip
    assert_eq!(parsed.working_color_space, doc.working_color_space);
}

#[test]
fn style_value_bound_with_token_roundtrip() {
    use ode_format::style::TokenRef;
    use ode_format::tokens::TokenValue;

    let mut doc = Document::new("Bound Token Test");

    // Create token
    let col = doc.tokens.add_collection("Colors", vec!["Light"]);
    let tok_id = doc.tokens.add_token(col, "primary", TokenValue::Color(
        Color::from_hex("#3b82f6").unwrap()
    ));

    // Create frame with a token-bound fill
    let mut frame = Node::new_frame("Card");
    if let ode_format::node::NodeKind::Frame(ref mut data) = frame.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Bound {
                    token: TokenRef { collection_id: col, token_id: tok_id },
                    resolved: Color::from_hex("#3b82f6").unwrap(),
                },
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let frame_id = doc.nodes.insert(frame);
    doc.canvas.push(frame_id);

    // Roundtrip
    let json = serde_json::to_string_pretty(&doc).unwrap();
    let parsed: Document = serde_json::from_str(&json).unwrap();

    // Verify bound style value survived
    let parsed_frame_id = parsed.canvas[0];
    if let ode_format::node::NodeKind::Frame(ref data) = parsed.nodes[parsed_frame_id].kind {
        assert_eq!(data.visual.fills.len(), 1);
        if let Paint::Solid { ref color } = data.visual.fills[0].paint {
            assert!(color.is_bound(), "Fill color should still be bound to token after roundtrip");
        } else {
            panic!("Expected Solid paint");
        }
    } else {
        panic!("Expected Frame node");
    }
}

#[test]
fn document_with_tokens() {
    use ode_format::tokens::TokenValue;

    let mut doc = Document::new("Token Test");

    // Add color tokens
    let col = doc.tokens.add_collection("Colors", vec!["Light", "Dark"]);
    doc.tokens.add_token(col, "primary", TokenValue::Color(
        Color::from_hex("#3b82f6").unwrap()
    ));

    // Resolve token
    let tok_id = doc.tokens.collections[0].tokens[0].id;
    let resolved = doc.tokens.resolve(col, tok_id).unwrap();
    assert!(matches!(resolved, TokenValue::Color(_)));

    // Serialize
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("primary"));

    // Deserialize and verify token survived
    let parsed: Document = serde_json::from_str(&json).unwrap();
    let parsed_resolved = parsed.tokens.resolve(col, tok_id).unwrap();
    assert!(matches!(parsed_resolved, TokenValue::Color(_)));
}
