use ode_format::color::Color;
use ode_format::document::Document;
use ode_format::node::{NodeId, NodeKind};
use ode_format::style::Paint;
use std::collections::HashMap;

/// Maps each child `NodeId` to its parent `NodeId`.
pub type ParentMap = HashMap<NodeId, NodeId>;

/// Build a map from child node IDs to their parent node IDs.
///
/// Iterates every node in the document; for each node that has children,
/// maps `child -> parent`.
pub fn build_parent_map(doc: &Document) -> ParentMap {
    let mut map = ParentMap::new();
    for (parent_id, node) in doc.nodes.iter() {
        if let Some(children) = node.kind.children() {
            for &child_id in children {
                map.insert(child_id, parent_id);
            }
        }
    }
    map
}

/// Walk ancestors of `node_id` to find the nearest solid fill color.
///
/// Checks the node itself first, then walks up via the parent map.
/// Returns the first solid fill color found, or white if none.
pub fn find_background_color(doc: &Document, node_id: NodeId, parent_map: &ParentMap) -> Color {
    let mut current = Some(node_id);
    while let Some(id) = current {
        let node = &doc.nodes[id];
        if let Some(visual) = node.kind.visual() {
            for fill in &visual.fills {
                if fill.visible {
                    if let Paint::Solid { ref color } = fill.paint {
                        return color.value();
                    }
                }
            }
        }
        current = parent_map.get(&id).copied();
    }
    Color::white()
}

/// Return the wire-format name for a `NodeKind`.
pub fn node_kind_name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Frame(_) => "frame",
        NodeKind::Group(_) => "group",
        NodeKind::Vector(_) => "vector",
        NodeKind::BooleanOp(_) => "boolean-op",
        NodeKind::Text(_) => "text",
        NodeKind::Image(_) => "image",
        NodeKind::Instance(_) => "instance",
    }
}

/// Return a stable path string for a node: `"node[{stable_id}]"`.
pub fn node_path(doc: &Document, target_id: NodeId) -> String {
    let node = &doc.nodes[target_id];
    format!("node[{}]", node.stable_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::node::Node;
    use ode_format::style::{BlendMode, Fill, StyleValue};

    #[test]
    fn build_parent_map_from_doc() {
        let mut doc = Document::new("Test");

        // Create a frame (root) with a text child.
        let mut frame = Node::new_frame("Root", 100.0, 100.0);
        let text = Node::new_text("Title", "Hello");

        let text_id = doc.nodes.insert(text);

        // Add text as child of frame.
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.container.children.push(text_id);
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let parent_map = build_parent_map(&doc);

        // Root frame has no parent.
        assert!(!parent_map.contains_key(&frame_id));
        // Text child maps to the frame.
        assert_eq!(parent_map.get(&text_id), Some(&frame_id));
    }

    #[test]
    fn find_background_color_from_parent() {
        let mut doc = Document::new("Test");

        let mut frame = Node::new_frame("Card", 200.0, 200.0);
        let text = Node::new_text("Label", "Hello");

        let text_id = doc.nodes.insert(text);

        // Give the frame a solid fill of #336699.
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(Color::from_hex("#336699").unwrap()),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
            data.container.children.push(text_id);
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let parent_map = build_parent_map(&doc);
        let bg = find_background_color(&doc, text_id, &parent_map);
        let rgba = bg.to_rgba_u8();

        assert_eq!(rgba[0], 0x33);
        assert_eq!(rgba[1], 0x66);
        assert_eq!(rgba[2], 0x99);
    }

    #[test]
    fn no_parent_fill_defaults_to_white() {
        let mut doc = Document::new("Test");
        let text = Node::new_text("Orphan", "Alone");
        let text_id = doc.nodes.insert(text);
        doc.canvas.push(text_id);

        let parent_map = build_parent_map(&doc);
        let bg = find_background_color(&doc, text_id, &parent_map);
        let rgba = bg.to_rgba_u8();

        assert_eq!(rgba, [255, 255, 255, 255]);
    }

    #[test]
    fn node_kind_names_match_wire_format() {
        let frame = Node::new_frame("F", 10.0, 10.0);
        assert_eq!(node_kind_name(&frame.kind), "frame");

        let text = Node::new_text("T", "content");
        assert_eq!(node_kind_name(&text.kind), "text");
    }

    #[test]
    fn node_path_includes_stable_id() {
        let mut doc = Document::new("Test");
        let text = Node::new_text("Title", "Hello");
        let text_id = doc.nodes.insert(text);

        let path = node_path(&doc, text_id);
        let stable_id = &doc.nodes[text_id].stable_id;
        assert_eq!(path, format!("node[{stable_id}]"));
    }
}
