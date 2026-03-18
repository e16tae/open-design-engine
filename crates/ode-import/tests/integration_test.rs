use std::collections::HashMap;
use std::fs;

use ode_format::node::NodeKind;
use ode_import::figma::convert::FigmaConverter;
use ode_import::figma::types::FigmaFileResponse;

#[test]
fn convert_simple_frame() {
    let json = fs::read_to_string("tests/fixtures/simple_frame.json").unwrap();
    let file: FigmaFileResponse = serde_json::from_str(&json).unwrap();
    let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();

    assert_eq!(result.document.name, "Test File");
    assert_eq!(result.document.canvas.len(), 1);
    // Frame + Text + Vector = 3 nodes
    assert_eq!(result.document.nodes.len(), 3);
    assert!(result.warnings.is_empty());

    // Verify root frame.
    let frame_id = result.document.canvas[0];
    let frame = &result.document.nodes[frame_id];
    assert_eq!(frame.name, "Card");
    assert!(matches!(frame.kind, NodeKind::Frame(_)));

    if let NodeKind::Frame(ref data) = frame.kind {
        assert!((data.width - 300.0).abs() < f32::EPSILON);
        assert!((data.height - 200.0).abs() < f32::EPSILON);
        assert_eq!(data.container.children.len(), 2);

        // First child is Text.
        let text_id = data.container.children[0];
        let text_node = &result.document.nodes[text_id];
        assert_eq!(text_node.name, "Title");
        assert!(matches!(text_node.kind, NodeKind::Text(_)));
        if let NodeKind::Text(ref td) = text_node.kind {
            assert_eq!(td.content, "Hello World");
        }

        // Second child is Vector.
        let vec_id = data.container.children[1];
        let vec_node = &result.document.nodes[vec_id];
        assert_eq!(vec_node.name, "Icon");
        assert!(matches!(vec_node.kind, NodeKind::Vector(_)));
    } else {
        panic!("Expected Frame node kind");
    }
}

#[test]
fn import_mask_basic_sets_is_mask() {
    let json = fs::read_to_string("tests/fixtures/mask_basic.json").unwrap();
    let file: FigmaFileResponse = serde_json::from_str(&json).unwrap();
    let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();

    // No mask warnings (mask is supported now)
    let mask_warnings: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.to_lowercase().contains("mask"))
        .collect();
    assert!(
        mask_warnings.is_empty(),
        "No mask warnings expected: {:?}",
        mask_warnings
    );

    // Find the mask node
    let mask_node = result
        .document
        .nodes
        .iter()
        .find(|(_, n)| n.name == "MaskCircle")
        .map(|(_, n)| n)
        .expect("MaskCircle should exist");
    assert!(mask_node.is_mask, "MaskCircle should have is_mask=true");

    // The masked sibling should NOT have is_mask
    let sibling = result
        .document
        .nodes
        .iter()
        .find(|(_, n)| n.name == "MaskedRect")
        .map(|(_, n)| n)
        .expect("MaskedRect should exist");
    assert!(!sibling.is_mask, "MaskedRect should not be a mask");
}
