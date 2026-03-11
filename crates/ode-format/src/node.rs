use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};
use slotmap::{new_key_type, SlotMap};

use crate::style::{VisualProps, BlendMode};

// ─── IDs ───

new_key_type! {
    /// Runtime arena key. Not stable across save/load.
    pub struct NodeId;
}

/// Stable, serialization-safe identifier (nanoid).
pub type StableId = String;

/// Arena-based node storage.
///
/// Wraps `SlotMap<NodeId, Node>` so that `NodeTree::new()` works (custom
/// slotmap key types require `with_key()` instead of `new()`).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NodeTree(SlotMap<NodeId, Node>);

impl NodeTree {
    pub fn new() -> Self {
        Self(SlotMap::with_key())
    }

    pub fn insert(&mut self, node: Node) -> NodeId {
        self.0.insert(node)
    }
}

impl Index<NodeId> for NodeTree {
    type Output = Node;
    fn index(&self, id: NodeId) -> &Node {
        &self.0[id]
    }
}

impl IndexMut<NodeId> for NodeTree {
    fn index_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.0[id]
    }
}

// ─── Transform ───

/// 2D affine transform matrix: [a, b, c, d, tx, ty]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub a: f32, pub b: f32, pub c: f32, pub d: f32, pub tx: f32, pub ty: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 }
    }
}

// ─── Constraints ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintAxis { Fixed, Scale, Stretch, Center }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraints {
    pub horizontal: ConstraintAxis,
    pub vertical: ConstraintAxis,
}

// ─── ContainerProps ───

/// **Serialization note:** Vec<NodeId> round-trips correctly via slotmap's
/// Serialize impl. For .ode file format (v0.2+), children will be serialized
/// as Vec<StableId> with a NodeId<->StableId mapping table built on load.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ContainerProps {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeId>,
    pub layout: Option<LayoutConfig>,
}

/// Placeholder for layout configuration (designed when taffy is integrated).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutConfig {
    _placeholder: (),
}

// ─── BooleanOperation ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BooleanOperation { Union, Subtract, Intersect, Exclude }

// ─── NodeKind ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NodeKind {
    Frame(Box<FrameData>),
    Group(Box<GroupData>),
    Vector(Box<VectorData>),
    BooleanOp(Box<BooleanOpData>),
    Text(Box<TextData>),
    Image(Box<ImageData>),
    Instance(Box<InstanceData>),
}

impl NodeKind {
    pub fn visual(&self) -> Option<&VisualProps> {
        match self {
            Self::Frame(d) => Some(&d.visual),
            Self::Vector(d) => Some(&d.visual),
            Self::BooleanOp(d) => Some(&d.visual),
            Self::Text(d) => Some(&d.visual),
            Self::Image(d) => Some(&d.visual),
            Self::Group(_) | Self::Instance(_) => None,
        }
    }

    pub fn visual_mut(&mut self) -> Option<&mut VisualProps> {
        match self {
            Self::Frame(d) => Some(&mut d.visual),
            Self::Vector(d) => Some(&mut d.visual),
            Self::BooleanOp(d) => Some(&mut d.visual),
            Self::Text(d) => Some(&mut d.visual),
            Self::Image(d) => Some(&mut d.visual),
            Self::Group(_) | Self::Instance(_) => None,
        }
    }

    pub fn children(&self) -> Option<&[NodeId]> {
        match self {
            Self::Frame(d) => Some(&d.container.children),
            Self::Instance(d) => Some(&d.container.children),
            Self::Group(d) => Some(&d.children),
            Self::BooleanOp(d) => Some(&d.children),
            Self::Vector(_) | Self::Text(_) | Self::Image(_) => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<NodeId>> {
        match self {
            Self::Frame(d) => Some(&mut d.container.children),
            Self::Instance(d) => Some(&mut d.container.children),
            Self::Group(d) => Some(&mut d.children),
            Self::BooleanOp(d) => Some(&mut d.children),
            Self::Vector(_) | Self::Text(_) | Self::Image(_) => None,
        }
    }
}

// ─── Kind-Specific Data ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameData {
    #[serde(default)]
    pub visual: VisualProps,
    #[serde(default)]
    pub container: ContainerProps,
    pub component_def: Option<ComponentDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorData {
    #[serde(default)]
    pub visual: VisualProps,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BooleanOpData {
    #[serde(default)]
    pub visual: VisualProps,
    pub op: BooleanOperation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextData {
    #[serde(default)]
    pub visual: VisualProps,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageData {
    #[serde(default)]
    pub visual: VisualProps,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceData {
    #[serde(default)]
    pub container: ContainerProps,
    pub source_component: StableId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentDef {
    pub name: String,
    pub description: String,
}

// ─── Node ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    #[serde(skip)]
    pub id: NodeId,
    pub stable_id: StableId,
    pub name: String,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub blend_mode: BlendMode,
    pub constraints: Option<Constraints>,
    pub kind: NodeKind,
}

fn default_opacity() -> f32 { 1.0 }

// Note: `impl Default for BlendMode` is in style.rs (where BlendMode is defined).

impl Node {
    pub fn new_frame(name: &str) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Frame(Box::new(FrameData {
                visual: VisualProps::default(),
                container: ContainerProps::default(),
                component_def: None,
            })),
        }
    }

    pub fn new_group(name: &str) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Group(Box::new(GroupData { children: Vec::new() })),
        }
    }

    pub fn new_vector(name: &str) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Vector(Box::new(VectorData { visual: VisualProps::default() })),
        }
    }

    pub fn new_text(name: &str, content: &str) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Text(Box::new(TextData {
                visual: VisualProps::default(),
                content: content.to_string(),
            })),
        }
    }

    pub fn new_boolean_op(name: &str, op: BooleanOperation) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::BooleanOp(Box::new(BooleanOpData {
                visual: VisualProps::default(),
                op,
                children: Vec::new(),
            })),
        }
    }

    pub fn new_image(name: &str) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Image(Box::new(ImageData { visual: VisualProps::default() })),
        }
    }

    pub fn new_instance(name: &str, source_component: StableId) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Instance(Box::new(InstanceData {
                container: ContainerProps::default(),
                source_component,
                overrides: Vec::new(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::style::{StyleValue, Paint, Fill, BlendMode};

    #[test]
    fn create_frame_node() {
        let mut tree = NodeTree::new();
        let node = Node::new_frame("Header");
        let id = tree.insert(node);
        assert_eq!(tree[id].name, "Header");
        assert!(matches!(tree[id].kind, NodeKind::Frame(_)));
    }

    #[test]
    fn create_group_node() {
        let mut tree = NodeTree::new();
        let node = Node::new_group("Icons");
        let id = tree.insert(node);
        assert!(matches!(tree[id].kind, NodeKind::Group(_)));
    }

    #[test]
    fn frame_has_visual_props() {
        let node = Node::new_frame("Card");
        assert!(node.kind.visual().is_some());
    }

    #[test]
    fn group_has_no_visual_props() {
        let node = Node::new_group("Group");
        assert!(node.kind.visual().is_none());
    }

    #[test]
    fn frame_has_children() {
        let node = Node::new_frame("Parent");
        assert!(node.kind.children().is_some());
        assert!(node.kind.children().unwrap().is_empty());
    }

    #[test]
    fn vector_has_no_children() {
        let node = Node::new_vector("Path");
        assert!(node.kind.children().is_none());
    }

    #[test]
    fn stable_ids_are_unique() {
        let a = Node::new_frame("A");
        let b = Node::new_frame("B");
        assert_ne!(a.stable_id, b.stable_id);
    }

    #[test]
    fn node_kind_visual_accessor() {
        let mut node = Node::new_frame("Colored");
        if let NodeKind::Frame(ref mut data) = node.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::black()) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let visual = node.kind.visual().unwrap();
        assert_eq!(visual.fills.len(), 1);
    }
}
