use std::collections::HashMap;
use std::fs;

use ode_format::node::NodeKind;
use ode_format::Document;
use ode_import::figma::convert::FigmaConverter;
use ode_import::figma::types::FigmaFileResponse;

#[test]
fn round_trip_simple_frame() {
    let json = fs::read_to_string("tests/fixtures/simple_frame.json").unwrap();
    let file: FigmaFileResponse = serde_json::from_str(&json).unwrap();
    let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();

    // Document -> JSON string
    let ode_json = serde_json::to_string_pretty(&result.document).unwrap();

    // JSON string -> Document (round-trip)
    let doc2: Document = serde_json::from_str(&ode_json).unwrap();

    assert_eq!(result.document.name, doc2.name);
    assert_eq!(result.document.canvas.len(), doc2.canvas.len());
    assert_eq!(result.document.nodes.len(), doc2.nodes.len());
}

#[test]
fn unsupported_nodes_produce_warnings() {
    let json = fs::read_to_string("tests/fixtures/unsupported_nodes.json").unwrap();
    let file: FigmaFileResponse = serde_json::from_str(&json).unwrap();
    let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
    // Should have warnings for: STICKY skipped, VIDEO paint, NOISE effect, LINEAR_BURN blend mode
    assert!(
        result.warnings.len() >= 3,
        "Expected at least 3 warnings, got {}: {:?}",
        result.warnings.len(),
        result.warnings
    );
}

#[test]
fn single_image_fill_frame_becomes_image_node() {
    let json = r#"{
        "name": "Image Test",
        "document": {"id": "0:0", "name": "Doc", "type": "DOCUMENT", "children": [
            {"id": "0:1", "name": "Page", "type": "CANVAS", "children": [
                {"id": "1:1", "name": "Photo", "type": "RECTANGLE",
                 "fills": [{"type": "IMAGE", "imageRef": "img123", "scaleMode": "FILL"}],
                 "strokes": [], "effects": [], "blendMode": "NORMAL",
                 "size": {"x": 200, "y": 200},
                 "relativeTransform": [[1,0,0],[0,1,0]]}
            ]}
        ]},
        "components": {}, "componentSets": {}, "schemaVersion": 0, "styles": {}
    }"#;
    let file: FigmaFileResponse = serde_json::from_str(json).unwrap();
    let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
    let has_image = result
        .document
        .nodes
        .iter()
        .any(|(_, n)| matches!(&n.kind, NodeKind::Image(_)));
    assert!(has_image, "Expected at least one Image node");
}
