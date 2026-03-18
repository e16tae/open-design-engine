use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::document::{Document, Version, View, ViewId, ViewKind, WorkingColorSpace};
use crate::node::{
    BooleanOpData, BooleanOperation, ComponentDef, Constraints, FrameData, GroupData, ImageData,
    InstanceData, LayoutConfig, LayoutSizing, Node, NodeId, NodeKind, NodeTree, Override,
    SizingMode, StableId, TextData, Transform, VectorData,
};
use crate::style::{BlendMode, ImageSource, VisualProps};
use crate::tokens::DesignTokens;
use crate::typography::{TextRun, TextSizingMode, TextStyle};

// ─── Error ───

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("unknown stable_id reference: {0}")]
    UnknownReference(String),
}

// ─── Default helpers ───

fn default_opacity() -> f32 {
    1.0
}

fn default_visible() -> bool {
    true
}

fn default_clips_content() -> bool {
    true
}

fn default_text_size() -> f32 {
    100.0
}

// ─── Wire Types ───

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentWire {
    pub format_version: Version,
    pub name: String,
    pub nodes: Vec<NodeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canvas: Vec<String>,
    #[serde(default)]
    pub tokens: DesignTokens,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<ViewWire>,
    #[serde(default)]
    pub working_color_space: WorkingColorSpace,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeWire {
    pub stable_id: StableId,
    pub name: String,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub blend_mode: BlendMode,
    #[serde(default = "default_visible")]
    pub visible: bool,
    pub constraints: Option<Constraints>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_sizing: Option<LayoutSizing>,
    #[serde(flatten)]
    pub kind: NodeKindWire,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NodeKindWire {
    Frame(FrameDataWire),
    Group(GroupDataWire),
    Vector(Box<VectorData>),
    BooleanOp(BooleanOpDataWire),
    Text(TextDataWire),
    Image(ImageDataWire),
    Instance(InstanceDataWire),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ContainerPropsWire {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    pub layout: Option<LayoutConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FrameDataWire {
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub width_sizing: SizingMode,
    #[serde(default)]
    pub height_sizing: SizingMode,
    #[serde(default)]
    pub corner_radius: [f32; 4],
    #[serde(default = "default_clips_content")]
    pub clips_content: bool,
    #[serde(default)]
    pub visual: VisualProps,
    #[serde(default)]
    pub container: ContainerPropsWire,
    pub component_def: Option<ComponentDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GroupDataWire {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BooleanOpDataWire {
    #[serde(default)]
    pub visual: VisualProps,
    pub op: BooleanOperation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TextDataWire {
    #[serde(default)]
    pub visual: VisualProps,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<TextRun>,
    #[serde(default)]
    pub default_style: TextStyle,
    #[serde(default = "default_text_size")]
    pub width: f32,
    #[serde(default = "default_text_size")]
    pub height: f32,
    #[serde(default)]
    pub sizing_mode: TextSizingMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImageDataWire {
    #[serde(default)]
    pub visual: VisualProps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ImageSource>,
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InstanceDataWire {
    #[serde(default)]
    pub container: ContainerPropsWire,
    pub source_component: StableId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<Override>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ViewWire {
    pub id: ViewId,
    pub name: String,
    #[serde(flatten)]
    pub kind: ViewKindWire,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ViewKindWire {
    Print { pages: Vec<String> },
    Web { root: String },
    Presentation { slides: Vec<String> },
    Export { targets: Vec<serde_json::Value> },
}

// ─── Document → Wire conversion ───

impl DocumentWire {
    pub fn from_document(doc: &Document) -> Self {
        // Build NodeId → StableId lookup
        let id_to_stable: HashMap<NodeId, &str> = doc
            .nodes
            .iter()
            .map(|(nid, node)| (nid, node.stable_id.as_str()))
            .collect();

        let resolve = |nid: &NodeId| -> String {
            id_to_stable
                .get(nid)
                .map(|s| s.to_string())
                .unwrap_or_default()
        };

        let nodes: Vec<NodeWire> = doc
            .nodes
            .iter()
            .map(|(_, node)| node_to_wire(node, &resolve))
            .collect();

        let canvas: Vec<String> = doc.canvas.iter().map(&resolve).collect();

        let views: Vec<ViewWire> = doc
            .views
            .iter()
            .map(|v| view_to_wire(v, &resolve))
            .collect();

        DocumentWire {
            format_version: doc.format_version.clone(),
            name: doc.name.clone(),
            nodes,
            canvas,
            tokens: doc.tokens.clone(),
            views,
            working_color_space: doc.working_color_space,
        }
    }

    pub fn into_document(self) -> Result<Document, WireError> {
        let mut tree = NodeTree::new();

        // Pass 1: Insert all nodes with placeholder kinds, build stable_id → NodeId mapping
        let mut stable_to_id: HashMap<String, NodeId> = HashMap::new();

        for nw in &self.nodes {
            let placeholder = Node {
                id: NodeId::default(),
                stable_id: nw.stable_id.clone(),
                name: nw.name.clone(),
                transform: nw.transform,
                opacity: nw.opacity,
                blend_mode: nw.blend_mode,
                visible: nw.visible,
                constraints: nw.constraints,
                layout_sizing: nw.layout_sizing.clone(),
                // Placeholder kind — will be overwritten in pass 2
                kind: NodeKind::Group(Box::new(GroupData {
                    children: Vec::new(),
                })),
            };
            let nid = tree.insert(placeholder);
            stable_to_id.insert(nw.stable_id.clone(), nid);
        }

        let resolve = |s: &str| -> Result<NodeId, WireError> {
            stable_to_id
                .get(s)
                .copied()
                .ok_or_else(|| WireError::UnknownReference(s.to_string()))
        };

        // Pass 2: Set correct NodeKind with resolved references
        for nw in &self.nodes {
            let nid = stable_to_id[&nw.stable_id];
            let kind = wire_kind_to_runtime(&nw.kind, &resolve)?;
            tree[nid].kind = kind;
        }

        // Resolve canvas
        let canvas: Vec<NodeId> = self
            .canvas
            .iter()
            .map(|s| resolve(s))
            .collect::<Result<Vec<_>, _>>()?;

        // Resolve views
        let views: Vec<View> = self
            .views
            .iter()
            .map(|vw| wire_view_to_runtime(vw, &resolve))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Document {
            format_version: self.format_version,
            name: self.name,
            nodes: tree,
            canvas,
            tokens: self.tokens,
            views,
            working_color_space: self.working_color_space,
        })
    }
}

// ─── Wire mutation helpers ───

impl DocumentWire {
    /// Check if a node kind can have children.
    pub fn is_container(kind: &NodeKindWire) -> bool {
        matches!(
            kind,
            NodeKindWire::Frame(_)
                | NodeKindWire::Group(_)
                | NodeKindWire::BooleanOp(_)
                | NodeKindWire::Instance(_)
        )
    }

    /// Get mutable reference to a node kind's children list.
    pub fn children_of_kind_mut(kind: &mut NodeKindWire) -> Option<&mut Vec<String>> {
        match kind {
            NodeKindWire::Frame(d) => Some(&mut d.container.children),
            NodeKindWire::Group(d) => Some(&mut d.children),
            NodeKindWire::BooleanOp(d) => Some(&mut d.children),
            NodeKindWire::Instance(d) => Some(&mut d.container.children),
            _ => None,
        }
    }

    /// Find a node by stable_id.
    pub fn find_node(&self, stable_id: &str) -> Option<&NodeWire> {
        self.nodes.iter().find(|n| n.stable_id == stable_id)
    }

    /// Find a node by stable_id (mutable).
    pub fn find_node_mut(&mut self, stable_id: &str) -> Option<&mut NodeWire> {
        self.nodes.iter_mut().find(|n| n.stable_id == stable_id)
    }

    /// Find the stable_id of the parent that has child_id in its children.
    pub fn find_parent(&self, child_id: &str) -> Option<String> {
        for node in &self.nodes {
            let children = match &node.kind {
                NodeKindWire::Frame(d) => Some(d.container.children.as_slice()),
                NodeKindWire::Group(d) => Some(d.children.as_slice()),
                NodeKindWire::BooleanOp(d) => Some(d.children.as_slice()),
                NodeKindWire::Instance(d) => Some(d.container.children.as_slice()),
                _ => None,
            };
            if let Some(children) = children {
                if children.iter().any(|c| c == child_id) {
                    return Some(node.stable_id.clone());
                }
            }
        }
        None
    }

    /// Remove child_id from whichever parent's children list contains it.
    pub fn remove_child_from_parent(&mut self, child_id: &str) {
        for node in &mut self.nodes {
            let children = match &mut node.kind {
                NodeKindWire::Frame(d) => Some(&mut d.container.children),
                NodeKindWire::Group(d) => Some(&mut d.children),
                NodeKindWire::BooleanOp(d) => Some(&mut d.children),
                NodeKindWire::Instance(d) => Some(&mut d.container.children),
                _ => None,
            };
            if let Some(children) = children {
                if let Some(pos) = children.iter().position(|c| c == child_id) {
                    children.remove(pos);
                    return;
                }
            }
        }
    }

    /// Collect all descendant stable_ids recursively.
    pub fn collect_descendants(&self, stable_id: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut queue = std::collections::VecDeque::new();

        // Seed the queue with direct children of stable_id
        if let Some(node) = self.find_node(stable_id) {
            let children = match &node.kind {
                NodeKindWire::Frame(d) => Some(d.container.children.as_slice()),
                NodeKindWire::Group(d) => Some(d.children.as_slice()),
                NodeKindWire::BooleanOp(d) => Some(d.children.as_slice()),
                NodeKindWire::Instance(d) => Some(d.container.children.as_slice()),
                _ => None,
            };
            if let Some(children) = children {
                for c in children {
                    queue.push_back(c.clone());
                }
            }
        }

        while let Some(current) = queue.pop_front() {
            result.push(current.clone());
            if let Some(node) = self.find_node(&current) {
                let children = match &node.kind {
                    NodeKindWire::Frame(d) => Some(d.container.children.as_slice()),
                    NodeKindWire::Group(d) => Some(d.children.as_slice()),
                    NodeKindWire::BooleanOp(d) => Some(d.children.as_slice()),
                    NodeKindWire::Instance(d) => Some(d.container.children.as_slice()),
                    _ => None,
                };
                if let Some(children) = children {
                    for c in children {
                        queue.push_back(c.clone());
                    }
                }
            }
        }

        result
    }

    /// Get mutable reference to a node kind's VisualProps (if it has one).
    pub fn visual_props_mut(kind: &mut NodeKindWire) -> Option<&mut VisualProps> {
        match kind {
            NodeKindWire::Frame(d) => Some(&mut d.visual),
            NodeKindWire::Vector(d) => Some(&mut d.visual),
            NodeKindWire::BooleanOp(d) => Some(&mut d.visual),
            NodeKindWire::Text(d) => Some(&mut d.visual),
            NodeKindWire::Image(d) => Some(&mut d.visual),
            _ => None,
        }
    }
}

// ─── Node conversion helpers ───

fn node_to_wire(node: &Node, resolve: &dyn Fn(&NodeId) -> String) -> NodeWire {
    let kind = match &node.kind {
        NodeKind::Frame(d) => NodeKindWire::Frame(FrameDataWire {
            width: d.width,
            height: d.height,
            width_sizing: d.width_sizing,
            height_sizing: d.height_sizing,
            corner_radius: d.corner_radius,
            clips_content: d.clips_content,
            visual: d.visual.clone(),
            container: ContainerPropsWire {
                children: d.container.children.iter().map(resolve).collect(),
                layout: d.container.layout.clone(),
            },
            component_def: d.component_def.clone(),
        }),
        NodeKind::Group(d) => NodeKindWire::Group(GroupDataWire {
            children: d.children.iter().map(resolve).collect(),
        }),
        NodeKind::Vector(d) => NodeKindWire::Vector(d.clone()),
        NodeKind::BooleanOp(d) => NodeKindWire::BooleanOp(BooleanOpDataWire {
            visual: d.visual.clone(),
            op: d.op,
            children: d.children.iter().map(resolve).collect(),
        }),
        NodeKind::Text(d) => NodeKindWire::Text(TextDataWire {
            visual: d.visual.clone(),
            content: d.content.clone(),
            runs: d.runs.clone(),
            default_style: d.default_style.clone(),
            width: d.width,
            height: d.height,
            sizing_mode: d.sizing_mode,
        }),
        NodeKind::Image(d) => NodeKindWire::Image(ImageDataWire {
            visual: d.visual.clone(),
            source: d.source.clone(),
            width: d.width,
            height: d.height,
        }),
        NodeKind::Instance(d) => NodeKindWire::Instance(InstanceDataWire {
            container: ContainerPropsWire {
                children: d.container.children.iter().map(resolve).collect(),
                layout: d.container.layout.clone(),
            },
            source_component: d.source_component.clone(),
            width: d.width,
            height: d.height,
            overrides: d.overrides.clone(),
        }),
    };

    NodeWire {
        stable_id: node.stable_id.clone(),
        name: node.name.clone(),
        transform: node.transform,
        opacity: node.opacity,
        blend_mode: node.blend_mode,
        visible: node.visible,
        constraints: node.constraints,
        layout_sizing: node.layout_sizing.clone(),
        kind,
    }
}

fn wire_kind_to_runtime(
    kind: &NodeKindWire,
    resolve: &dyn Fn(&str) -> Result<NodeId, WireError>,
) -> Result<NodeKind, WireError> {
    let result = match kind {
        NodeKindWire::Frame(d) => {
            let children = d
                .container
                .children
                .iter()
                .map(|s| resolve(s))
                .collect::<Result<Vec<_>, _>>()?;
            NodeKind::Frame(Box::new(FrameData {
                width: d.width,
                height: d.height,
                width_sizing: d.width_sizing,
                height_sizing: d.height_sizing,
                corner_radius: d.corner_radius,
                clips_content: d.clips_content,
                visual: d.visual.clone(),
                container: crate::node::ContainerProps {
                    children,
                    layout: d.container.layout.clone(),
                },
                component_def: d.component_def.clone(),
            }))
        }
        NodeKindWire::Group(d) => {
            let children = d
                .children
                .iter()
                .map(|s| resolve(s))
                .collect::<Result<Vec<_>, _>>()?;
            NodeKind::Group(Box::new(GroupData { children }))
        }
        NodeKindWire::Vector(d) => NodeKind::Vector(d.clone()),
        NodeKindWire::BooleanOp(d) => {
            let children = d
                .children
                .iter()
                .map(|s| resolve(s))
                .collect::<Result<Vec<_>, _>>()?;
            NodeKind::BooleanOp(Box::new(BooleanOpData {
                visual: d.visual.clone(),
                op: d.op,
                children,
            }))
        }
        NodeKindWire::Text(d) => NodeKind::Text(Box::new(TextData {
            visual: d.visual.clone(),
            content: d.content.clone(),
            runs: d.runs.clone(),
            default_style: d.default_style.clone(),
            width: d.width,
            height: d.height,
            sizing_mode: d.sizing_mode,
        })),
        NodeKindWire::Image(d) => NodeKind::Image(Box::new(ImageData {
            visual: d.visual.clone(),
            source: d.source.clone(),
            width: d.width,
            height: d.height,
        })),
        NodeKindWire::Instance(d) => {
            let children = d
                .container
                .children
                .iter()
                .map(|s| resolve(s))
                .collect::<Result<Vec<_>, _>>()?;
            NodeKind::Instance(Box::new(InstanceData {
                container: crate::node::ContainerProps {
                    children,
                    layout: d.container.layout.clone(),
                },
                source_component: d.source_component.clone(),
                width: d.width,
                height: d.height,
                overrides: d.overrides.clone(),
            }))
        }
    };
    Ok(result)
}

// ─── View conversion helpers ───

fn view_to_wire(view: &View, resolve: &dyn Fn(&NodeId) -> String) -> ViewWire {
    let kind = match &view.kind {
        ViewKind::Print { pages } => ViewKindWire::Print {
            pages: pages.iter().map(resolve).collect(),
        },
        ViewKind::Web { root } => ViewKindWire::Web {
            root: resolve(root),
        },
        ViewKind::Presentation { slides } => ViewKindWire::Presentation {
            slides: slides.iter().map(resolve).collect(),
        },
        ViewKind::Export { targets } => ViewKindWire::Export {
            targets: targets.clone(),
        },
    };

    ViewWire {
        id: view.id,
        name: view.name.clone(),
        kind,
    }
}

fn wire_view_to_runtime(
    vw: &ViewWire,
    resolve: &dyn Fn(&str) -> Result<NodeId, WireError>,
) -> Result<View, WireError> {
    let kind = match &vw.kind {
        ViewKindWire::Print { pages } => {
            let pages = pages
                .iter()
                .map(|s| resolve(s))
                .collect::<Result<Vec<_>, _>>()?;
            ViewKind::Print { pages }
        }
        ViewKindWire::Web { root } => ViewKind::Web {
            root: resolve(root)?,
        },
        ViewKindWire::Presentation { slides } => {
            let slides = slides
                .iter()
                .map(|s| resolve(s))
                .collect::<Result<Vec<_>, _>>()?;
            ViewKind::Presentation { slides }
        }
        ViewKindWire::Export { targets } => ViewKind::Export {
            targets: targets.clone(),
        },
    };

    Ok(View {
        id: vw.id,
        name: vw.name.clone(),
        kind,
    })
}

// ─── Custom Serialize/Deserialize for Document ───

impl Serialize for Document {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let wire = DocumentWire::from_document(self);
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = DocumentWire::deserialize(deserializer)?;
        wire.into_document().map_err(serde::de::Error::custom)
    }
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Version, ViewId};
    use crate::node::Node;

    #[test]
    fn document_wire_roundtrip() {
        let wire = DocumentWire {
            format_version: Version(0, 2, 0),
            name: "Wire Test".to_string(),
            nodes: vec![
                NodeWire {
                    stable_id: "frame-1".to_string(),
                    name: "Parent Frame".to_string(),
                    transform: Transform::default(),
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    visible: true,
                    constraints: None,
                    layout_sizing: None,
                    kind: NodeKindWire::Frame(FrameDataWire {
                        width: 200.0,
                        height: 100.0,
                        width_sizing: SizingMode::Fixed,
                        height_sizing: SizingMode::Fixed,
                        corner_radius: [0.0; 4],
                        clips_content: true,
                        visual: VisualProps::default(),
                        container: ContainerPropsWire {
                            children: vec!["text-1".to_string()],
                            layout: None,
                        },
                        component_def: None,
                    }),
                },
                NodeWire {
                    stable_id: "text-1".to_string(),
                    name: "Child Text".to_string(),
                    transform: Transform::default(),
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    visible: true,
                    constraints: None,
                    layout_sizing: None,
                    kind: NodeKindWire::Text(TextDataWire {
                        visual: VisualProps::default(),
                        content: "Hello".to_string(),
                        runs: Vec::new(),
                        default_style: TextStyle::default(),
                        width: 100.0,
                        height: 100.0,
                        sizing_mode: TextSizingMode::Fixed,
                    }),
                },
            ],
            canvas: vec!["frame-1".to_string()],
            tokens: DesignTokens::new(),
            views: vec![],
            working_color_space: WorkingColorSpace::default(),
        };

        let json = serde_json::to_string_pretty(&wire).unwrap();
        let parsed: DocumentWire = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "Wire Test");
        assert_eq!(parsed.nodes.len(), 2);
        assert_eq!(parsed.canvas, vec!["frame-1"]);
        if let NodeKindWire::Frame(ref d) = parsed.nodes[0].kind {
            assert_eq!(d.container.children, vec!["text-1"]);
        } else {
            panic!("Expected Frame wire");
        }
    }

    #[test]
    fn view_kind_wire_roundtrip() {
        let view = ViewWire {
            id: ViewId(1),
            name: "Main Page".to_string(),
            kind: ViewKindWire::Web {
                root: "page-root".to_string(),
            },
        };

        let json = serde_json::to_string(&view).unwrap();
        let parsed: ViewWire = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "Main Page");
        if let ViewKindWire::Web { root } = &parsed.kind {
            assert_eq!(root, "page-root");
        } else {
            panic!("Expected Web view kind");
        }
    }

    #[test]
    fn document_to_wire_and_back() {
        // Build a runtime Document with parent-child
        let mut doc = Document::new("Roundtrip");

        let frame = Node::new_frame("Artboard", 400.0, 300.0);
        let text = Node::new_text("Label", "Hello, Wire!");

        let frame_id = doc.nodes.insert(frame.clone());
        let text_id = doc.nodes.insert(text);

        // Set known stable_ids for deterministic checks
        doc.nodes[frame_id].stable_id = "frame-abc".to_string();
        doc.nodes[text_id].stable_id = "text-xyz".to_string();

        // Add text as child of frame
        if let NodeKind::Frame(ref mut data) = doc.nodes[frame_id].kind {
            data.container.children.push(text_id);
        }

        doc.canvas.push(frame_id);

        // Serialize to JSON via custom Serialize (Document → DocumentWire → JSON)
        let json = serde_json::to_string_pretty(&doc).unwrap();

        // Verify wire format has string IDs
        assert!(json.contains("frame-abc"));
        assert!(json.contains("text-xyz"));

        // Deserialize back (JSON → DocumentWire → Document)
        let parsed: Document = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "Roundtrip");
        assert_eq!(parsed.nodes.len(), 2);
        assert_eq!(parsed.canvas.len(), 1);

        // Verify parent-child survived
        let parsed_frame_id = parsed.canvas[0];
        assert_eq!(parsed.nodes[parsed_frame_id].stable_id, "frame-abc");
        if let NodeKind::Frame(ref data) = parsed.nodes[parsed_frame_id].kind {
            assert_eq!(data.container.children.len(), 1);
            let child_id = data.container.children[0];
            assert_eq!(parsed.nodes[child_id].stable_id, "text-xyz");
            assert_eq!(parsed.nodes[child_id].name, "Label");
        } else {
            panic!("Expected Frame node");
        }
    }

    #[test]
    fn document_wire_with_views() {
        let mut doc = Document::new("View Test");

        let frame = Node::new_frame("Page", 800.0, 600.0);
        let frame_id = doc.nodes.insert(frame);
        doc.nodes[frame_id].stable_id = "page-root".to_string();
        doc.canvas.push(frame_id);

        doc.views.push(View {
            id: ViewId(0),
            name: "Web View".to_string(),
            kind: ViewKind::Web { root: frame_id },
        });

        // Roundtrip through JSON
        let json = serde_json::to_string_pretty(&doc).unwrap();
        assert!(json.contains("page-root"));

        let parsed: Document = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.views.len(), 1);
        assert_eq!(parsed.views[0].name, "Web View");
        if let ViewKind::Web { root } = &parsed.views[0].kind {
            assert_eq!(parsed.nodes[*root].stable_id, "page-root");
        } else {
            panic!("Expected Web view kind");
        }
    }

    // ─── Shared helper for mutation helper tests ───

    fn make_test_wire() -> DocumentWire {
        DocumentWire {
            format_version: Version(0, 2, 0),
            name: "Test".to_string(),
            nodes: vec![
                NodeWire {
                    stable_id: "frame-1".to_string(),
                    name: "Parent Frame".to_string(),
                    transform: Transform::default(),
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    visible: true,
                    constraints: None,
                    layout_sizing: None,
                    kind: NodeKindWire::Frame(FrameDataWire {
                        width: 200.0,
                        height: 100.0,
                        width_sizing: SizingMode::Fixed,
                        height_sizing: SizingMode::Fixed,
                        corner_radius: [0.0; 4],
                        clips_content: true,
                        visual: VisualProps::default(),
                        container: ContainerPropsWire {
                            children: vec!["text-1".to_string()],
                            layout: None,
                        },
                        component_def: None,
                    }),
                },
                NodeWire {
                    stable_id: "text-1".to_string(),
                    name: "Child Text".to_string(),
                    transform: Transform::default(),
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    visible: true,
                    constraints: None,
                    layout_sizing: None,
                    kind: NodeKindWire::Text(TextDataWire {
                        visual: VisualProps::default(),
                        content: "Hello".to_string(),
                        runs: Vec::new(),
                        default_style: TextStyle::default(),
                        width: 100.0,
                        height: 100.0,
                        sizing_mode: TextSizingMode::Fixed,
                    }),
                },
            ],
            canvas: vec!["frame-1".to_string()],
            tokens: DesignTokens::new(),
            views: vec![],
            working_color_space: WorkingColorSpace::default(),
        }
    }

    #[test]
    fn is_container_returns_true_for_frame_group_booleanop_instance() {
        use crate::node::BooleanOperation;

        let frame_kind = NodeKindWire::Frame(FrameDataWire {
            width: 0.0,
            height: 0.0,
            width_sizing: SizingMode::Fixed,
            height_sizing: SizingMode::Fixed,
            corner_radius: [0.0; 4],
            clips_content: true,
            visual: VisualProps::default(),
            container: ContainerPropsWire { children: vec![], layout: None },
            component_def: None,
        });
        let group_kind = NodeKindWire::Group(GroupDataWire { children: vec![] });
        let bool_kind = NodeKindWire::BooleanOp(BooleanOpDataWire {
            visual: VisualProps::default(),
            op: BooleanOperation::Union,
            children: vec![],
        });
        let instance_kind = NodeKindWire::Instance(InstanceDataWire {
            container: ContainerPropsWire { children: vec![], layout: None },
            source_component: "comp-1".to_string(),
            width: None,
            height: None,
            overrides: vec![],
        });
        let text_kind = NodeKindWire::Text(TextDataWire {
            visual: VisualProps::default(),
            content: "".to_string(),
            runs: vec![],
            default_style: TextStyle::default(),
            width: 100.0,
            height: 100.0,
            sizing_mode: TextSizingMode::Fixed,
        });

        assert!(DocumentWire::is_container(&frame_kind));
        assert!(DocumentWire::is_container(&group_kind));
        assert!(DocumentWire::is_container(&bool_kind));
        assert!(DocumentWire::is_container(&instance_kind));
        assert!(!DocumentWire::is_container(&text_kind));
    }

    #[test]
    fn find_node_by_stable_id() {
        let wire = make_test_wire();

        let found = wire.find_node("frame-1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Parent Frame");

        let found_text = wire.find_node("text-1");
        assert!(found_text.is_some());
        assert_eq!(found_text.unwrap().name, "Child Text");

        let not_found = wire.find_node("nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn find_node_mut_modifies_in_place() {
        let mut wire = make_test_wire();

        {
            let node = wire.find_node_mut("text-1").unwrap();
            node.name = "Modified Text".to_string();
        }

        let node = wire.find_node("text-1").unwrap();
        assert_eq!(node.name, "Modified Text");
    }

    #[test]
    fn find_parent_returns_parent_stable_id() {
        let wire = make_test_wire();

        let parent = wire.find_parent("text-1");
        assert_eq!(parent, Some("frame-1".to_string()));

        // Root node has no parent
        let root_parent = wire.find_parent("frame-1");
        assert!(root_parent.is_none());

        // Non-existent node has no parent
        let missing_parent = wire.find_parent("ghost");
        assert!(missing_parent.is_none());
    }

    #[test]
    fn remove_child_from_parent_updates_children() {
        let mut wire = make_test_wire();

        // Verify text-1 is a child of frame-1 before removal
        if let NodeKindWire::Frame(ref d) = wire.find_node("frame-1").unwrap().kind {
            assert!(d.container.children.contains(&"text-1".to_string()));
        }

        wire.remove_child_from_parent("text-1");

        // Verify children vec is now empty
        if let NodeKindWire::Frame(ref d) = wire.find_node("frame-1").unwrap().kind {
            assert!(d.container.children.is_empty());
        } else {
            panic!("Expected Frame");
        }
    }

    #[test]
    fn collect_descendants_returns_all_nested() {
        // Build: frame-1 → group-1 → text-1
        let mut wire = DocumentWire {
            format_version: Version(0, 2, 0),
            name: "Nested".to_string(),
            nodes: vec![
                NodeWire {
                    stable_id: "frame-1".to_string(),
                    name: "Frame".to_string(),
                    transform: Transform::default(),
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    visible: true,
                    constraints: None,
                    layout_sizing: None,
                    kind: NodeKindWire::Frame(FrameDataWire {
                        width: 200.0,
                        height: 100.0,
                        width_sizing: SizingMode::Fixed,
                        height_sizing: SizingMode::Fixed,
                        corner_radius: [0.0; 4],
                        clips_content: true,
                        visual: VisualProps::default(),
                        container: ContainerPropsWire {
                            children: vec!["group-1".to_string()],
                            layout: None,
                        },
                        component_def: None,
                    }),
                },
                NodeWire {
                    stable_id: "group-1".to_string(),
                    name: "Group".to_string(),
                    transform: Transform::default(),
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    visible: true,
                    constraints: None,
                    layout_sizing: None,
                    kind: NodeKindWire::Group(GroupDataWire {
                        children: vec!["text-1".to_string()],
                    }),
                },
                NodeWire {
                    stable_id: "text-1".to_string(),
                    name: "Text".to_string(),
                    transform: Transform::default(),
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    visible: true,
                    constraints: None,
                    layout_sizing: None,
                    kind: NodeKindWire::Text(TextDataWire {
                        visual: VisualProps::default(),
                        content: "Hi".to_string(),
                        runs: vec![],
                        default_style: TextStyle::default(),
                        width: 100.0,
                        height: 100.0,
                        sizing_mode: TextSizingMode::Fixed,
                    }),
                },
            ],
            canvas: vec!["frame-1".to_string()],
            tokens: DesignTokens::new(),
            views: vec![],
            working_color_space: WorkingColorSpace::default(),
        };

        let descendants = wire.collect_descendants("frame-1");
        assert_eq!(descendants.len(), 2);
        assert!(descendants.contains(&"group-1".to_string()));
        assert!(descendants.contains(&"text-1".to_string()));
    }

    #[test]
    fn collect_descendants_of_leaf_is_empty() {
        let wire = make_test_wire();
        let descendants = wire.collect_descendants("text-1");
        assert!(descendants.is_empty());
    }

    #[test]
    fn visual_props_mut_works_for_frame_and_text() {
        use crate::style::{Fill, Paint, BlendMode as StyleBlendMode};
        use crate::style::StyleValue;
        use crate::color::Color;

        let mut frame_kind = NodeKindWire::Frame(FrameDataWire {
            width: 0.0,
            height: 0.0,
            width_sizing: SizingMode::Fixed,
            height_sizing: SizingMode::Fixed,
            corner_radius: [0.0; 4],
            clips_content: true,
            visual: VisualProps::default(),
            container: ContainerPropsWire { children: vec![], layout: None },
            component_def: None,
        });

        // Frame returns Some and we can mutate fills
        {
            let vp = DocumentWire::visual_props_mut(&mut frame_kind).unwrap();
            vp.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::black()) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: StyleBlendMode::Normal,
                visible: true,
            });
        }
        if let NodeKindWire::Frame(ref d) = frame_kind {
            assert_eq!(d.visual.fills.len(), 1);
        } else {
            panic!("Expected Frame");
        }

        let mut text_kind = NodeKindWire::Text(TextDataWire {
            visual: VisualProps::default(),
            content: "".to_string(),
            runs: vec![],
            default_style: TextStyle::default(),
            width: 100.0,
            height: 100.0,
            sizing_mode: TextSizingMode::Fixed,
        });

        // Text returns Some
        assert!(DocumentWire::visual_props_mut(&mut text_kind).is_some());

        // Group has no visual props
        let mut group_kind = NodeKindWire::Group(GroupDataWire { children: vec![] });
        assert!(DocumentWire::visual_props_mut(&mut group_kind).is_none());

        // Instance has no visual props
        let mut instance_kind = NodeKindWire::Instance(InstanceDataWire {
            container: ContainerPropsWire { children: vec![], layout: None },
            source_component: "comp-1".to_string(),
            width: None,
            height: None,
            overrides: vec![],
        });
        assert!(DocumentWire::visual_props_mut(&mut instance_kind).is_none());
    }

    #[test]
    fn unknown_reference_returns_error() {
        let wire = DocumentWire {
            format_version: Version(0, 2, 0),
            name: "Bad Ref".to_string(),
            nodes: vec![NodeWire {
                stable_id: "node-1".to_string(),
                name: "Frame".to_string(),
                transform: Transform::default(),
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                visible: true,
                constraints: None,
                layout_sizing: None,
                kind: NodeKindWire::Frame(FrameDataWire {
                    width: 100.0,
                    height: 100.0,
                    width_sizing: SizingMode::Fixed,
                    height_sizing: SizingMode::Fixed,
                    corner_radius: [0.0; 4],
                    clips_content: true,
                    visual: VisualProps::default(),
                    container: ContainerPropsWire {
                        children: vec!["nonexistent".to_string()],
                        layout: None,
                    },
                    component_def: None,
                }),
            }],
            canvas: vec!["node-1".to_string()],
            tokens: DesignTokens::new(),
            views: vec![],
            working_color_space: WorkingColorSpace::default(),
        };

        let result = wire.into_document();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }
}
