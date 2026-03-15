use std::ops::{Index, IndexMut};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};

use crate::style::{BlendMode, Effect, Fill, Stroke, VisualProps};
use crate::typography::{TextRun, TextSizingMode, TextStyle};

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
#[derive(Debug, Default, Clone)]
pub struct NodeTree(SlotMap<NodeId, Node>);

impl NodeTree {
    pub fn new() -> Self {
        Self(SlotMap::with_key())
    }

    pub fn insert(&mut self, node: Node) -> NodeId {
        self.0.insert(node)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.0.iter()
    }

    /// Find a node by its stable_id (linear scan).
    /// For hot paths, build a `HashMap<&str, NodeId>` index instead.
    pub fn find_by_stable_id(&self, stable_id: &str) -> Option<NodeId> {
        self.0
            .iter()
            .find(|(_, n)| n.stable_id == stable_id)
            .map(|(id, _)| id)
    }
}

impl PartialEq for NodeTree {
    fn eq(&self, other: &Self) -> bool {
        if self.0.len() != other.0.len() {
            return false;
        }
        // Compare by collecting and sorting stable_ids and node content.
        let mut a: Vec<_> = self.0.values().map(|n| (&n.stable_id, n)).collect();
        let mut b: Vec<_> = other.0.values().map(|n| (&n.stable_id, n)).collect();
        a.sort_by_key(|(id, _)| *id);
        b.sort_by_key(|(id, _)| *id);
        a.iter()
            .zip(b.iter())
            .all(|((ia, na), (ib, nb))| ia == ib && na == nb)
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
}

// ─── Constraints ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintAxis {
    Fixed,
    Scale,
    Stretch,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

/// Auto layout configuration for a container (Flexbox-based).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct LayoutConfig {
    #[serde(default)]
    pub direction: LayoutDirection,
    #[serde(default)]
    pub primary_axis_align: PrimaryAxisAlign,
    #[serde(default)]
    pub counter_axis_align: CounterAxisAlign,
    #[serde(default)]
    pub padding: LayoutPadding,
    #[serde(default)]
    pub item_spacing: f32,
    #[serde(default)]
    pub wrap: LayoutWrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutDirection {
    Horizontal,
    Vertical,
}

impl Default for LayoutDirection {
    fn default() -> Self {
        Self::Horizontal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PrimaryAxisAlign {
    Start,
    Center,
    End,
    SpaceBetween,
}

impl Default for PrimaryAxisAlign {
    fn default() -> Self {
        Self::Start
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CounterAxisAlign {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
}

impl Default for CounterAxisAlign {
    fn default() -> Self {
        Self::Start
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayoutPadding {
    #[serde(default)]
    pub top: f32,
    #[serde(default)]
    pub right: f32,
    #[serde(default)]
    pub bottom: f32,
    #[serde(default)]
    pub left: f32,
}

impl Default for LayoutPadding {
    fn default() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutWrap {
    NoWrap,
    Wrap,
}

impl Default for LayoutWrap {
    fn default() -> Self {
        Self::NoWrap
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SizingMode {
    Fixed,
    Hug,
    Fill,
}

impl Default for SizingMode {
    fn default() -> Self {
        Self::Fixed
    }
}

/// Per-child layout sizing overrides within an auto-layout container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct LayoutSizing {
    #[serde(default)]
    pub width: SizingMode,
    #[serde(default)]
    pub height: SizingMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align_self: Option<CounterAxisAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<f32>,
}

// ─── BooleanOperation ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BooleanOperation {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

// ─── VectorPath ───

/// Serializable path representation.
/// Conversion to/from kurbo::BezPath lives in ode-core::path.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VectorPath {
    pub segments: Vec<PathSegment>,
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PathSegment {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    QuadTo {
        x1: f32,
        y1: f32,
        x: f32,
        y: f32,
    },
    CurveTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x: f32,
        y: f32,
    },
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

impl Default for FillRule {
    fn default() -> Self {
        Self::NonZero
    }
}

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
    pub container: ContainerProps,
    pub component_def: Option<ComponentDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VectorData {
    #[serde(default)]
    pub visual: VisualProps,
    #[serde(default)]
    pub path: VectorPath,
    #[serde(default)]
    pub fill_rule: FillRule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BooleanOpData {
    #[serde(default)]
    pub visual: VisualProps,
    pub op: BooleanOperation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeId>,
}

fn default_clips_content() -> bool {
    true
}

fn default_text_size() -> f32 {
    100.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextData {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImageData {
    #[serde(default)]
    pub visual: VisualProps,
}

/// Target a node within the component tree by its stable_id.
/// Since stable_ids are globally unique, a single StableId suffices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Override {
    Fills {
        target: StableId,
        fills: Vec<Fill>,
    },
    Strokes {
        target: StableId,
        strokes: Vec<Stroke>,
    },
    Effects {
        target: StableId,
        effects: Vec<Effect>,
    },
    Opacity {
        target: StableId,
        opacity: f32,
    },
    BlendMode {
        target: StableId,
        blend_mode: BlendMode,
    },
    Visible {
        target: StableId,
        visible: bool,
    },
    Size {
        target: StableId,
        width: Option<f32>,
        height: Option<f32>,
    },
    TextContent {
        target: StableId,
        content: String,
    },
}

impl Override {
    /// Returns the stable_id of the node this override targets.
    pub fn target(&self) -> &str {
        match self {
            Self::Fills { target, .. }
            | Self::Strokes { target, .. }
            | Self::Effects { target, .. }
            | Self::Opacity { target, .. }
            | Self::BlendMode { target, .. }
            | Self::Visible { target, .. }
            | Self::Size { target, .. }
            | Self::TextContent { target, .. } => target,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceData {
    #[serde(default)]
    pub container: ContainerProps,
    pub source_component: StableId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<Override>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    #[serde(default = "default_visible")]
    pub visible: bool,
    pub constraints: Option<Constraints>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_sizing: Option<LayoutSizing>,
    pub kind: NodeKind,
}

fn default_opacity() -> f32 {
    1.0
}
fn default_visible() -> bool {
    true
}

// Note: `impl Default for BlendMode` is in style.rs (where BlendMode is defined).

impl Node {
    pub fn new_frame(name: &str, width: f32, height: f32) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            constraints: None,
            layout_sizing: None,
            kind: NodeKind::Frame(Box::new(FrameData {
                width,
                height,
                width_sizing: SizingMode::Fixed,
                height_sizing: SizingMode::Fixed,
                corner_radius: [0.0; 4],
                clips_content: true,
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
            visible: true,
            constraints: None,
            layout_sizing: None,
            kind: NodeKind::Group(Box::new(GroupData {
                children: Vec::new(),
            })),
        }
    }

    pub fn new_vector(name: &str, path: VectorPath) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            visible: true,
            constraints: None,
            layout_sizing: None,
            kind: NodeKind::Vector(Box::new(VectorData {
                visual: VisualProps::default(),
                path,
                fill_rule: FillRule::default(),
            })),
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
            visible: true,
            constraints: None,
            layout_sizing: None,
            kind: NodeKind::Text(Box::new(TextData {
                visual: VisualProps::default(),
                content: content.to_string(),
                runs: Vec::new(),
                default_style: TextStyle::default(),
                width: 100.0,
                height: 100.0,
                sizing_mode: TextSizingMode::Fixed,
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
            visible: true,
            constraints: None,
            layout_sizing: None,
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
            visible: true,
            constraints: None,
            layout_sizing: None,
            kind: NodeKind::Image(Box::new(ImageData {
                visual: VisualProps::default(),
            })),
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
            visible: true,
            constraints: None,
            layout_sizing: None,
            kind: NodeKind::Instance(Box::new(InstanceData {
                container: ContainerProps::default(),
                source_component,
                width: None,
                height: None,
                overrides: Vec::new(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::style::{BlendMode, Fill, Paint, StyleValue};

    #[test]
    fn create_frame_node() {
        let mut tree = NodeTree::new();
        let node = Node::new_frame("Header", 100.0, 100.0);
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
        let node = Node::new_frame("Card", 200.0, 150.0);
        assert!(node.kind.visual().is_some());
    }

    #[test]
    fn group_has_no_visual_props() {
        let node = Node::new_group("Group");
        assert!(node.kind.visual().is_none());
    }

    #[test]
    fn frame_has_children() {
        let node = Node::new_frame("Parent", 100.0, 100.0);
        assert!(node.kind.children().is_some());
        assert!(node.kind.children().unwrap().is_empty());
    }

    #[test]
    fn vector_has_no_children() {
        let node = Node::new_vector("Path", VectorPath::default());
        assert!(node.kind.children().is_none());
    }

    #[test]
    fn stable_ids_are_unique() {
        let a = Node::new_frame("A", 100.0, 100.0);
        let b = Node::new_frame("B", 100.0, 100.0);
        assert_ne!(a.stable_id, b.stable_id);
    }

    #[test]
    fn node_kind_visual_accessor() {
        let mut node = Node::new_frame("Colored", 100.0, 100.0);
        if let NodeKind::Frame(ref mut data) = node.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(Color::black()),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let visual = node.kind.visual().unwrap();
        assert_eq!(visual.fills.len(), 1);
    }

    #[test]
    fn vectorpath_serde_roundtrip() {
        let path = VectorPath {
            segments: vec![
                PathSegment::MoveTo { x: 0.0, y: 0.0 },
                PathSegment::LineTo { x: 100.0, y: 0.0 },
                PathSegment::CurveTo {
                    x1: 100.0,
                    y1: 50.0,
                    x2: 50.0,
                    y2: 100.0,
                    x: 0.0,
                    y: 100.0,
                },
                PathSegment::Close,
            ],
            closed: true,
        };
        let json = serde_json::to_string(&path).unwrap();
        let parsed: VectorPath = serde_json::from_str(&json).unwrap();
        assert_eq!(path, parsed);
    }

    #[test]
    fn fillrule_default_is_nonzero() {
        assert_eq!(FillRule::default(), FillRule::NonZero);
    }

    #[test]
    fn frame_data_has_size_and_corner_radius() {
        let node = Node::new_frame("Card", 200.0, 100.0);
        if let NodeKind::Frame(ref data) = node.kind {
            assert!((data.width - 200.0).abs() < f32::EPSILON);
            assert!((data.height - 100.0).abs() < f32::EPSILON);
            assert_eq!(data.corner_radius, [0.0; 4]);
        } else {
            panic!("Expected Frame node");
        }
    }

    #[test]
    fn vector_data_has_path_and_fill_rule() {
        let path = VectorPath {
            segments: vec![
                PathSegment::MoveTo { x: 0.0, y: 0.0 },
                PathSegment::LineTo { x: 50.0, y: 50.0 },
            ],
            closed: false,
        };
        let node = Node::new_vector("Line", path.clone());
        if let NodeKind::Vector(ref data) = node.kind {
            assert_eq!(data.path, path);
            assert_eq!(data.fill_rule, FillRule::NonZero);
        } else {
            panic!("Expected Vector node");
        }
    }

    #[test]
    fn frame_data_backward_compat_no_size() {
        let json = r#"{"type":"frame","visual":{},"container":{},"component_def":null}"#;
        let kind: NodeKind = serde_json::from_str(json).unwrap();
        if let NodeKind::Frame(data) = kind {
            assert!((data.width - 0.0).abs() < f32::EPSILON);
            assert!((data.height - 0.0).abs() < f32::EPSILON);
            assert_eq!(data.corner_radius, [0.0; 4]);
            // New sizing fields default to Fixed
            assert_eq!(data.width_sizing, SizingMode::Fixed);
            assert_eq!(data.height_sizing, SizingMode::Fixed);
        } else {
            panic!("Expected Frame");
        }
    }

    #[test]
    fn layout_config_serde_roundtrip() {
        let config = LayoutConfig {
            direction: LayoutDirection::Vertical,
            primary_axis_align: PrimaryAxisAlign::SpaceBetween,
            counter_axis_align: CounterAxisAlign::Stretch,
            padding: LayoutPadding {
                top: 8.0,
                right: 16.0,
                bottom: 8.0,
                left: 16.0,
            },
            item_spacing: 12.0,
            wrap: LayoutWrap::Wrap,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: LayoutConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn layout_config_defaults() {
        let json = "{}";
        let config: LayoutConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.direction, LayoutDirection::Horizontal);
        assert_eq!(config.primary_axis_align, PrimaryAxisAlign::Start);
        assert_eq!(config.counter_axis_align, CounterAxisAlign::Start);
        assert!((config.item_spacing - 0.0).abs() < f32::EPSILON);
        assert_eq!(config.wrap, LayoutWrap::NoWrap);
    }

    #[test]
    fn layout_sizing_serde_roundtrip() {
        let sizing = LayoutSizing {
            width: SizingMode::Fill,
            height: SizingMode::Hug,
            align_self: Some(CounterAxisAlign::Center),
            min_width: Some(50.0),
            max_width: Some(200.0),
            min_height: None,
            max_height: None,
        };
        let json = serde_json::to_string(&sizing).unwrap();
        let parsed: LayoutSizing = serde_json::from_str(&json).unwrap();
        assert_eq!(sizing, parsed);
    }

    #[test]
    fn sizing_mode_defaults_to_fixed() {
        let json = "{}";
        let sizing: LayoutSizing = serde_json::from_str(json).unwrap();
        assert_eq!(sizing.width, SizingMode::Fixed);
        assert_eq!(sizing.height, SizingMode::Fixed);
        assert!(sizing.align_self.is_none());
    }

    #[test]
    fn existing_json_without_layout_fields_deserializes() {
        // Simulate pre-layout JSON (no layout_sizing, no width_sizing/height_sizing)
        let json = r#"{
            "type": "frame",
            "width": 100, "height": 50,
            "visual": {}, "container": {}, "component_def": null
        }"#;
        let kind: NodeKind = serde_json::from_str(json).unwrap();
        if let NodeKind::Frame(data) = kind {
            assert_eq!(data.width_sizing, SizingMode::Fixed);
            assert_eq!(data.height_sizing, SizingMode::Fixed);
        } else {
            panic!("Expected Frame");
        }
    }

    // ─── Override Tests ───

    #[test]
    fn override_fills_serde_roundtrip() {
        let overrides = vec![
            Override::Fills {
                target: "node-1".to_string(),
                fills: vec![Fill {
                    paint: Paint::Solid {
                        color: StyleValue::Raw(Color::Srgb {
                            r: 0.0,
                            g: 0.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                    },
                    opacity: StyleValue::Raw(1.0),
                    blend_mode: BlendMode::Normal,
                    visible: true,
                }],
            },
            Override::Strokes {
                target: "node-2".to_string(),
                strokes: vec![],
            },
            Override::Effects {
                target: "node-1".to_string(),
                effects: vec![],
            },
            Override::Opacity {
                target: "node-3".to_string(),
                opacity: 0.5,
            },
            Override::BlendMode {
                target: "node-3".to_string(),
                blend_mode: BlendMode::Multiply,
            },
            Override::Visible {
                target: "node-4".to_string(),
                visible: false,
            },
            Override::Size {
                target: "node-1".to_string(),
                width: Some(200.0),
                height: None,
            },
            Override::TextContent {
                target: "text-1".to_string(),
                content: "Overridden text".to_string(),
            },
        ];
        let json = serde_json::to_string(&overrides).unwrap();
        let parsed: Vec<Override> = serde_json::from_str(&json).unwrap();
        assert_eq!(overrides, parsed);
    }

    #[test]
    fn override_target_accessor() {
        let ov = Override::Fills {
            target: "my-node".to_string(),
            fills: vec![],
        };
        assert_eq!(ov.target(), "my-node");

        let ov2 = Override::TextContent {
            target: "text-abc".to_string(),
            content: "hello".to_string(),
        };
        assert_eq!(ov2.target(), "text-abc");
    }

    #[test]
    fn instance_data_with_typed_overrides_roundtrip() {
        let inst = InstanceData {
            container: ContainerProps::default(),
            source_component: "comp-1".to_string(),
            width: Some(150.0),
            height: None,
            overrides: vec![Override::Visible {
                target: "child-1".to_string(),
                visible: false,
            }],
        };
        let json = serde_json::to_string(&inst).unwrap();
        let parsed: InstanceData = serde_json::from_str(&json).unwrap();
        assert_eq!(inst, parsed);
    }

    #[test]
    fn instance_data_empty_overrides_roundtrip() {
        let inst = InstanceData {
            container: ContainerProps::default(),
            source_component: "comp-1".to_string(),
            width: None,
            height: None,
            overrides: vec![],
        };
        let json = serde_json::to_string(&inst).unwrap();
        assert!(!json.contains("overrides")); // skip_serializing_if = "Vec::is_empty"
        let parsed: InstanceData = serde_json::from_str(&json).unwrap();
        assert_eq!(inst, parsed);
    }

    #[test]
    fn find_by_stable_id_returns_correct_node() {
        let mut tree = NodeTree::new();
        let mut node = Node::new_frame("Target", 50.0, 50.0);
        node.stable_id = "find-me".to_string();
        let expected_id = tree.insert(node);

        let other = Node::new_frame("Other", 100.0, 100.0);
        tree.insert(other);

        assert_eq!(tree.find_by_stable_id("find-me"), Some(expected_id));
        assert_eq!(tree.find_by_stable_id("nonexistent"), None);
    }
}
