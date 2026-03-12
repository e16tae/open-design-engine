use serde::{Deserialize, Serialize};
use crate::node::{NodeId, NodeTree};
use crate::tokens::DesignTokens;

// ─── Version ───
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version(pub u32, pub u32, pub u32);

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

// ─── IDs ───
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewId(pub u32);

// ─── Working Color Space ───
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkingColorSpace { Srgb, DisplayP3, AdobeRgb, ProPhotoRgb }

impl Default for WorkingColorSpace {
    fn default() -> Self { Self::Srgb }
}

// ─── Canvas ───
pub type CanvasRoot = NodeId;

// ─── View ───
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub id: ViewId,
    pub name: String,
    pub kind: ViewKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ViewKind {
    Print { pages: Vec<NodeId> },
    Web { root: NodeId },
    Presentation { slides: Vec<NodeId> },
    Export { targets: Vec<serde_json::Value> },
}

// ─── Document ───
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub format_version: Version,
    pub name: String,
    pub nodes: NodeTree,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canvas: Vec<CanvasRoot>,
    #[serde(default)]
    pub tokens: DesignTokens,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<View>,
    #[serde(default)]
    pub working_color_space: WorkingColorSpace,
}

impl Document {
    pub fn new(name: &str) -> Self {
        Self {
            format_version: Version(0, 1, 0),
            name: name.to_string(),
            nodes: NodeTree::new(),
            canvas: Vec::new(),
            tokens: DesignTokens::new(),
            views: Vec::new(),
            working_color_space: WorkingColorSpace::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;

    #[test]
    fn create_empty_document() {
        let doc = Document::new("My Design");
        assert_eq!(doc.name, "My Design");
        assert_eq!(doc.format_version, Version(0, 1, 0));
        assert!(doc.canvas.is_empty());
        assert!(doc.views.is_empty());
    }

    #[test]
    fn add_frame_to_canvas() {
        let mut doc = Document::new("Test");
        let frame = Node::new_frame("Artboard 1");
        let id = doc.nodes.insert(frame);
        doc.canvas.push(id);
        assert_eq!(doc.canvas.len(), 1);
        assert_eq!(doc.nodes[id].name, "Artboard 1");
    }

    #[test]
    fn add_export_view() {
        let mut doc = Document::new("Test");
        let frame = Node::new_frame("Icon");
        let id = doc.nodes.insert(frame);
        doc.canvas.push(id);
        doc.views.push(View {
            id: ViewId(0),
            name: "PNG Export".to_string(),
            kind: ViewKind::Export { targets: vec![] },
        });
        assert_eq!(doc.views.len(), 1);
    }

    #[test]
    fn document_roundtrip_json() {
        let doc = Document::new("Roundtrip Test");
        let json = serde_json::to_string_pretty(&doc).unwrap();
        let parsed: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, parsed);
    }
}
