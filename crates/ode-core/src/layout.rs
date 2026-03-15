use std::collections::HashMap;

use ode_format::document::Document;
use ode_format::node::{
    CounterAxisAlign, FrameData, LayoutConfig, LayoutDirection, LayoutWrap, Node, NodeId, NodeKind,
    PrimaryAxisAlign, SizingMode,
};
use taffy::prelude::*;

/// Computed position and size for a node placed by auto layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Maps NodeIds to their computed layout rectangles.
pub type LayoutMap = HashMap<NodeId, LayoutRect>;

/// Compute auto layout for the entire document.
///
/// Walks the node tree depth-first, computing layout for each auto-layout
/// container subtree using taffy. Returns a map of NodeId → LayoutRect
/// for all nodes that participate in auto layout.
pub fn compute_layout<'a>(
    doc: &'a Document,
    stable_id_index: &HashMap<&'a str, NodeId>,
) -> LayoutMap {
    let mut result = LayoutMap::new();
    for &root_id in &doc.canvas {
        walk_for_layout(doc, root_id, &mut result, stable_id_index);
    }
    result
}

/// Depth-first walk: compute children first (bottom-up), then this node.
fn walk_for_layout(
    doc: &Document,
    node_id: NodeId,
    result: &mut LayoutMap,
    stable_id_index: &HashMap<&str, NodeId>,
) {
    let node = &doc.nodes[node_id];

    // Recurse into children first (bottom-up for nested auto layout)
    if let Some(children) = node.kind.children() {
        for &child_id in children {
            walk_for_layout(doc, child_id, result, stable_id_index);
        }
    }

    // If this node is an auto-layout container, compute its subtree.
    // If layout computation fails (e.g., taffy error), skip this node gracefully.
    if let Some(config) = get_layout_config(node) {
        compute_subtree_layout(doc, node_id, config, result, stable_id_index);
    }
}

/// Extract LayoutConfig from a node if it's a container with auto layout enabled.
fn get_layout_config(node: &Node) -> Option<&LayoutConfig> {
    match &node.kind {
        NodeKind::Frame(data) => data.container.layout.as_ref(),
        NodeKind::Instance(data) => data.container.layout.as_ref(),
        _ => None,
    }
}

/// Get the intrinsic (declared) size of a node.
/// For frames, uses width/height. For instances, resolves from source component.
/// For nested auto-layout containers, uses the already-computed LayoutRect if available.
fn get_intrinsic_size(
    node: &Node,
    node_id: NodeId,
    result: &LayoutMap,
    doc: &Document,
    stable_id_index: &HashMap<&str, NodeId>,
) -> (f32, f32) {
    // If this node already has a computed layout (nested auto-layout container),
    // use that as its intrinsic size.
    if let Some(rect) = result.get(&node_id) {
        return (rect.width, rect.height);
    }

    match &node.kind {
        NodeKind::Frame(data) => (data.width, data.height),
        NodeKind::Text(data) => (data.width, data.height),
        NodeKind::Instance(data) => {
            // Use instance's own size overrides, falling back to component's size
            let comp_size = stable_id_index
                .get(data.source_component.as_str())
                .and_then(|&comp_id| {
                    if let NodeKind::Frame(ref fd) = doc.nodes[comp_id].kind {
                        Some((fd.width, fd.height))
                    } else {
                        None
                    }
                })
                .unwrap_or((0.0, 0.0));
            (
                data.width.unwrap_or(comp_size.0),
                data.height.unwrap_or(comp_size.1),
            )
        }
        // For other node types, use a default size
        _ => (0.0, 0.0),
    }
}

/// Build a taffy tree for a single auto-layout subtree and compute positions.
/// Returns `None` if any taffy operation fails, allowing the caller to skip gracefully.
fn compute_subtree_layout(
    doc: &Document,
    container_id: NodeId,
    config: &LayoutConfig,
    result: &mut LayoutMap,
    stable_id_index: &HashMap<&str, NodeId>,
) -> Option<()> {
    let container_node = &doc.nodes[container_id];
    let children = match container_node.kind.children() {
        Some(c) => c,
        None => return Some(()),
    };

    let mut taffy: TaffyTree<()> = TaffyTree::new();

    // Build taffy child nodes
    let mut taffy_children = Vec::new();
    let mut child_id_map: Vec<NodeId> = Vec::new(); // taffy index → ODE NodeId

    for &child_id in children {
        let child_node = &doc.nodes[child_id];
        let (intrinsic_w, intrinsic_h) =
            get_intrinsic_size(child_node, child_id, result, doc, stable_id_index);
        let already_laid_out = result.contains_key(&child_id);
        let child_style = build_child_style(
            child_node,
            intrinsic_w,
            intrinsic_h,
            config,
            already_laid_out,
        );

        let taffy_child = taffy.new_leaf(child_style).ok()?;
        taffy_children.push(taffy_child);
        child_id_map.push(child_id);
    }

    // Build container style
    let container_style = build_container_style(container_node, config);

    // Create container node
    let taffy_root = taffy
        .new_with_children(container_style, &taffy_children)
        .ok()?;

    // Determine available space for the container
    let available_size = get_container_available_size(container_node, container_id, result);

    // Compute layout
    taffy.compute_layout(taffy_root, available_size).ok()?;

    // Extract results: update container size if Hug
    let root_layout = taffy.layout(taffy_root).ok()?;
    let container_rect = LayoutRect {
        x: 0.0, // Container position is determined by its parent (or its own transform)
        y: 0.0,
        width: root_layout.size.width,
        height: root_layout.size.height,
    };

    // Only store the container's rect if its size changed (Hug mode)
    if let Some(frame_data) = get_frame_data(container_node) {
        if frame_data.width_sizing != SizingMode::Fixed
            || frame_data.height_sizing != SizingMode::Fixed
        {
            result.insert(container_id, container_rect);
        }
    }

    // Extract child positions
    for (i, &taffy_child) in taffy_children.iter().enumerate() {
        let child_layout = taffy.layout(taffy_child).ok()?;
        let ode_child_id = child_id_map[i];

        // If the child already has a LayoutRect (nested auto-layout container),
        // update only its position (x, y), keeping its computed size.
        if let Some(existing) = result.get(&ode_child_id) {
            result.insert(
                ode_child_id,
                LayoutRect {
                    x: child_layout.location.x,
                    y: child_layout.location.y,
                    width: existing.width,
                    height: existing.height,
                },
            );
        } else {
            result.insert(
                ode_child_id,
                LayoutRect {
                    x: child_layout.location.x,
                    y: child_layout.location.y,
                    width: child_layout.size.width,
                    height: child_layout.size.height,
                },
            );
        }
    }

    Some(())
}

fn get_frame_data(node: &Node) -> Option<&FrameData> {
    match &node.kind {
        NodeKind::Frame(data) => Some(data),
        _ => None,
    }
}

/// Build taffy Style for a container node.
fn build_container_style(node: &Node, config: &LayoutConfig) -> Style {
    let flex_direction = match config.direction {
        LayoutDirection::Horizontal => FlexDirection::Row,
        LayoutDirection::Vertical => FlexDirection::Column,
    };

    let justify_content = match config.primary_axis_align {
        PrimaryAxisAlign::Start => Some(JustifyContent::FlexStart),
        PrimaryAxisAlign::Center => Some(JustifyContent::Center),
        PrimaryAxisAlign::End => Some(JustifyContent::FlexEnd),
        PrimaryAxisAlign::SpaceBetween => Some(JustifyContent::SpaceBetween),
    };

    let align_items = match config.counter_axis_align {
        CounterAxisAlign::Start => Some(AlignItems::FlexStart),
        CounterAxisAlign::Center => Some(AlignItems::Center),
        CounterAxisAlign::End => Some(AlignItems::FlexEnd),
        CounterAxisAlign::Stretch => Some(AlignItems::Stretch),
        CounterAxisAlign::Baseline => Some(AlignItems::Baseline),
    };

    let flex_wrap = match config.wrap {
        LayoutWrap::NoWrap => FlexWrap::NoWrap,
        LayoutWrap::Wrap => FlexWrap::Wrap,
    };

    let padding = Rect {
        top: LengthPercentage::Length(config.padding.top),
        right: LengthPercentage::Length(config.padding.right),
        bottom: LengthPercentage::Length(config.padding.bottom),
        left: LengthPercentage::Length(config.padding.left),
    };

    let gap = Size {
        width: LengthPercentage::Length(config.item_spacing),
        height: LengthPercentage::Length(config.item_spacing),
    };

    // Determine container size dimensions
    let (width, height) = if let Some(frame_data) = get_frame_data(node) {
        let w = match frame_data.width_sizing {
            SizingMode::Fixed => Dimension::Length(frame_data.width),
            SizingMode::Hug => Dimension::Auto,
            SizingMode::Fill => Dimension::Auto, // Fill at container level = auto
        };
        let h = match frame_data.height_sizing {
            SizingMode::Fixed => Dimension::Length(frame_data.height),
            SizingMode::Hug => Dimension::Auto,
            SizingMode::Fill => Dimension::Auto,
        };
        (w, h)
    } else {
        (Dimension::Auto, Dimension::Auto)
    };

    Style {
        display: Display::Flex,
        flex_direction,
        justify_content,
        align_items,
        flex_wrap,
        padding,
        gap,
        size: Size { width, height },
        ..Style::DEFAULT
    }
}

/// Build taffy Style for a child node within an auto-layout container.
///
/// If `already_laid_out` is true, this child is a nested auto-layout container
/// whose size has already been computed — use its intrinsic size as fixed.
fn build_child_style(
    child_node: &Node,
    intrinsic_w: f32,
    intrinsic_h: f32,
    parent_config: &LayoutConfig,
    already_laid_out: bool,
) -> Style {
    // Determine sizing from layout_sizing if present, otherwise use Fixed with intrinsic size
    let (width_mode, height_mode, align_self, min_w, max_w, min_h, max_h) =
        if let Some(sizing) = &child_node.layout_sizing {
            (
                sizing.width,
                sizing.height,
                sizing.align_self,
                sizing.min_width,
                sizing.max_width,
                sizing.min_height,
                sizing.max_height,
            )
        } else {
            // For Frame children, check their own sizing modes
            if let Some(frame_data) = get_frame_data(child_node) {
                (
                    frame_data.width_sizing,
                    frame_data.height_sizing,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
            } else {
                (
                    SizingMode::Fixed,
                    SizingMode::Fixed,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
            }
        };

    // If this child's size was already computed by a nested auto-layout pass,
    // use fixed dimensions so the parent respects the resolved size.
    let width = if already_laid_out && !matches!(width_mode, SizingMode::Fill) {
        Dimension::Length(intrinsic_w)
    } else {
        match width_mode {
            SizingMode::Fixed => Dimension::Length(intrinsic_w),
            SizingMode::Hug => Dimension::Auto,
            SizingMode::Fill => Dimension::Auto,
        }
    };

    let height = if already_laid_out && !matches!(height_mode, SizingMode::Fill) {
        Dimension::Length(intrinsic_h)
    } else {
        match height_mode {
            SizingMode::Fixed => Dimension::Length(intrinsic_h),
            SizingMode::Hug => Dimension::Auto,
            SizingMode::Fill => Dimension::Auto,
        }
    };

    // Figma: Fill children grow to fill available space
    // Determine which axis is the main axis
    let is_horizontal = matches!(parent_config.direction, LayoutDirection::Horizontal);
    let flex_grow = if is_horizontal {
        if matches!(width_mode, SizingMode::Fill) {
            1.0
        } else {
            0.0
        }
    } else if matches!(height_mode, SizingMode::Fill) {
        1.0
    } else {
        0.0
    };

    // Figma default: children don't shrink
    let flex_shrink = 0.0;

    // Cross-axis Fill → Stretch (Figma behavior: Fill on the cross axis
    // stretches the child to match the container's cross dimension)
    let cross_axis_fill = if is_horizontal {
        matches!(height_mode, SizingMode::Fill)
    } else {
        matches!(width_mode, SizingMode::Fill)
    };

    let taffy_align_self = if let Some(a) = align_self {
        // Explicit align_self override takes priority
        Some(match a {
            CounterAxisAlign::Start => AlignSelf::FlexStart,
            CounterAxisAlign::Center => AlignSelf::Center,
            CounterAxisAlign::End => AlignSelf::FlexEnd,
            CounterAxisAlign::Stretch => AlignSelf::Stretch,
            CounterAxisAlign::Baseline => AlignSelf::Baseline,
        })
    } else if cross_axis_fill {
        Some(AlignSelf::Stretch)
    } else {
        None
    };

    let min_size = Size {
        width: min_w.map_or(Dimension::Auto, Dimension::Length),
        height: min_h.map_or(Dimension::Auto, Dimension::Length),
    };

    let max_size = Size {
        width: max_w.map_or(Dimension::Auto, Dimension::Length),
        height: max_h.map_or(Dimension::Auto, Dimension::Length),
    };

    Style {
        size: Size { width, height },
        min_size,
        max_size,
        flex_grow,
        flex_shrink,
        align_self: taffy_align_self,
        ..Style::DEFAULT
    }
}

/// Determine the available space for layout computation.
fn get_container_available_size(
    node: &Node,
    _node_id: NodeId,
    _result: &LayoutMap,
) -> Size<AvailableSpace> {
    if let Some(frame_data) = get_frame_data(node) {
        let w = match frame_data.width_sizing {
            SizingMode::Fixed => AvailableSpace::Definite(frame_data.width),
            _ => AvailableSpace::MinContent,
        };
        let h = match frame_data.height_sizing {
            SizingMode::Fixed => AvailableSpace::Definite(frame_data.height),
            _ => AvailableSpace::MinContent,
        };
        Size {
            width: w,
            height: h,
        }
    } else {
        Size {
            width: AvailableSpace::MinContent,
            height: AvailableSpace::MinContent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::document::Document;
    use ode_format::node::{
        CounterAxisAlign, LayoutConfig, LayoutDirection, LayoutPadding, LayoutSizing, LayoutWrap,
        Node, NodeKind, PrimaryAxisAlign, SizingMode,
    };

    /// Helper: create a frame with auto layout enabled.
    fn make_auto_layout_frame(name: &str, width: f32, height: f32, config: LayoutConfig) -> Node {
        let mut frame = Node::new_frame(name, width, height);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.container.layout = Some(config);
        }
        frame
    }

    /// Helper: build stable_id index and compute layout.
    fn test_compute_layout(doc: &Document) -> LayoutMap {
        let index: HashMap<&str, NodeId> = doc
            .nodes
            .iter()
            .map(|(nid, node)| (node.stable_id.as_str(), nid))
            .collect();
        compute_layout(doc, &index)
    }

    fn default_config() -> LayoutConfig {
        LayoutConfig {
            direction: LayoutDirection::Horizontal,
            primary_axis_align: PrimaryAxisAlign::Start,
            counter_axis_align: CounterAxisAlign::Start,
            padding: LayoutPadding::default(),
            item_spacing: 0.0,
            wrap: LayoutWrap::NoWrap,
        }
    }

    #[test]
    fn horizontal_layout_three_children() {
        let mut doc = Document::new("Test");

        let config = default_config();
        let mut parent = make_auto_layout_frame("Parent", 300.0, 100.0, config);

        let child1 = Node::new_frame("C1", 50.0, 40.0);
        let child2 = Node::new_frame("C2", 80.0, 40.0);
        let child3 = Node::new_frame("C3", 60.0, 40.0);

        let c1_id = doc.nodes.insert(child1);
        let c2_id = doc.nodes.insert(child2);
        let c3_id = doc.nodes.insert(child3);

        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c1_id, c2_id, c3_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);

        // Children should be placed left-to-right
        let r1 = layout.get(&c1_id).expect("C1 should have layout");
        let r2 = layout.get(&c2_id).expect("C2 should have layout");
        let r3 = layout.get(&c3_id).expect("C3 should have layout");

        assert!((r1.x - 0.0).abs() < 0.1, "C1.x = {}", r1.x);
        assert!((r2.x - 50.0).abs() < 0.1, "C2.x = {}", r2.x);
        assert!((r3.x - 130.0).abs() < 0.1, "C3.x = {}", r3.x);

        // All at y=0
        assert!((r1.y - 0.0).abs() < 0.1);
        assert!((r2.y - 0.0).abs() < 0.1);
        assert!((r3.y - 0.0).abs() < 0.1);
    }

    #[test]
    fn vertical_layout_with_gap_and_padding() {
        let mut doc = Document::new("Test");

        let config = LayoutConfig {
            direction: LayoutDirection::Vertical,
            primary_axis_align: PrimaryAxisAlign::Start,
            counter_axis_align: CounterAxisAlign::Start,
            padding: LayoutPadding {
                top: 10.0,
                right: 10.0,
                bottom: 10.0,
                left: 10.0,
            },
            item_spacing: 8.0,
            wrap: LayoutWrap::NoWrap,
        };
        let mut parent = make_auto_layout_frame("Parent", 200.0, 200.0, config);

        let child1 = Node::new_frame("C1", 50.0, 30.0);
        let child2 = Node::new_frame("C2", 50.0, 40.0);

        let c1_id = doc.nodes.insert(child1);
        let c2_id = doc.nodes.insert(child2);

        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c1_id, c2_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);

        let r1 = layout.get(&c1_id).unwrap();
        let r2 = layout.get(&c2_id).unwrap();

        // padding-left = 10
        assert!((r1.x - 10.0).abs() < 0.1, "C1.x = {}", r1.x);
        // padding-top = 10
        assert!((r1.y - 10.0).abs() < 0.1, "C1.y = {}", r1.y);
        // C2.y = padding-top + C1.height + gap = 10 + 30 + 8 = 48
        assert!((r2.y - 48.0).abs() < 0.1, "C2.y = {}", r2.y);
    }

    #[test]
    fn hug_container_shrinks_to_children() {
        let mut doc = Document::new("Test");

        let config = default_config();
        let mut parent = make_auto_layout_frame("Parent", 500.0, 500.0, config);
        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.width_sizing = SizingMode::Hug;
            data.height_sizing = SizingMode::Hug;
        }

        let child = Node::new_frame("C1", 80.0, 60.0);
        let c_id = doc.nodes.insert(child);

        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);

        // Container should shrink to child size
        let parent_rect = layout
            .get(&parent_id)
            .expect("Parent should have layout (hug)");
        assert!(
            (parent_rect.width - 80.0).abs() < 0.1,
            "w = {}",
            parent_rect.width
        );
        assert!(
            (parent_rect.height - 60.0).abs() < 0.1,
            "h = {}",
            parent_rect.height
        );
    }

    #[test]
    fn fill_child_expands() {
        let mut doc = Document::new("Test");

        let config = default_config();
        let mut parent = make_auto_layout_frame("Parent", 300.0, 100.0, config);

        let child1 = Node::new_frame("C1", 50.0, 40.0);
        let mut child2 = Node::new_frame("C2", 50.0, 40.0);
        // C2 fills remaining width
        child2.layout_sizing = Some(LayoutSizing {
            width: SizingMode::Fill,
            height: SizingMode::Fixed,
            align_self: None,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
        });

        let c1_id = doc.nodes.insert(child1);
        let c2_id = doc.nodes.insert(child2);

        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c1_id, c2_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);

        let r1 = layout.get(&c1_id).unwrap();
        let r2 = layout.get(&c2_id).unwrap();

        // C1 = 50px, C2 fills remaining = 250px
        assert!((r1.width - 50.0).abs() < 0.1, "C1.w = {}", r1.width);
        assert!((r2.width - 250.0).abs() < 0.1, "C2.w = {}", r2.width);
    }

    #[test]
    fn nested_auto_layout() {
        let mut doc = Document::new("Test");

        // Inner auto-layout: vertical, hug
        let inner_config = LayoutConfig {
            direction: LayoutDirection::Vertical,
            item_spacing: 4.0,
            ..default_config()
        };
        let mut inner = make_auto_layout_frame("Inner", 100.0, 100.0, inner_config);
        if let NodeKind::Frame(ref mut data) = inner.kind {
            data.width_sizing = SizingMode::Hug;
            data.height_sizing = SizingMode::Hug;
        }

        let ic1 = Node::new_frame("IC1", 60.0, 20.0);
        let ic2 = Node::new_frame("IC2", 60.0, 20.0);
        let ic1_id = doc.nodes.insert(ic1);
        let ic2_id = doc.nodes.insert(ic2);

        if let NodeKind::Frame(ref mut data) = inner.kind {
            data.container.children = vec![ic1_id, ic2_id];
        }
        let inner_id = doc.nodes.insert(inner);

        // Outer auto-layout: horizontal
        let outer_config = default_config();
        let mut outer = make_auto_layout_frame("Outer", 400.0, 200.0, outer_config);
        let sibling = Node::new_frame("Sibling", 100.0, 50.0);
        let sibling_id = doc.nodes.insert(sibling);

        if let NodeKind::Frame(ref mut data) = outer.kind {
            data.container.children = vec![inner_id, sibling_id];
        }
        let outer_id = doc.nodes.insert(outer);
        doc.canvas.push(outer_id);

        let layout = test_compute_layout(&doc);

        // Inner container should be 60x44 (hug: w=60, h=20+4+20=44)
        let inner_rect = layout.get(&inner_id).unwrap();
        assert!(
            (inner_rect.width - 60.0).abs() < 0.1,
            "inner.w = {}",
            inner_rect.width
        );
        assert!(
            (inner_rect.height - 44.0).abs() < 0.1,
            "inner.h = {}",
            inner_rect.height
        );

        // Sibling should be placed after inner
        let sibling_rect = layout.get(&sibling_id).unwrap();
        assert!(
            (sibling_rect.x - 60.0).abs() < 0.1,
            "sibling.x = {}",
            sibling_rect.x
        );
    }

    #[test]
    fn empty_container_no_crash() {
        let mut doc = Document::new("Test");

        let config = default_config();
        let parent = make_auto_layout_frame("Parent", 200.0, 100.0, config);
        // No children
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);
        // No children means no layout entries (container is Fixed so not stored either)
        assert!(layout.is_empty());
    }

    #[test]
    fn no_auto_layout_returns_empty_map() {
        let mut doc = Document::new("Test");

        let frame = Node::new_frame("Plain", 200.0, 100.0);
        let fid = doc.nodes.insert(frame);
        doc.canvas.push(fid);

        let layout = test_compute_layout(&doc);
        assert!(layout.is_empty());
    }

    #[test]
    fn center_alignment() {
        let mut doc = Document::new("Test");

        let config = LayoutConfig {
            direction: LayoutDirection::Horizontal,
            primary_axis_align: PrimaryAxisAlign::Center,
            counter_axis_align: CounterAxisAlign::Center,
            padding: LayoutPadding::default(),
            item_spacing: 0.0,
            wrap: LayoutWrap::NoWrap,
        };
        let mut parent = make_auto_layout_frame("Parent", 200.0, 100.0, config);

        let child = Node::new_frame("C1", 50.0, 30.0);
        let c_id = doc.nodes.insert(child);

        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);
        let r = layout.get(&c_id).unwrap();

        // Centered horizontally: (200 - 50) / 2 = 75
        assert!((r.x - 75.0).abs() < 0.1, "x = {}", r.x);
        // Centered vertically: (100 - 30) / 2 = 35
        assert!((r.y - 35.0).abs() < 0.1, "y = {}", r.y);
    }

    #[test]
    fn space_between_alignment() {
        let mut doc = Document::new("Test");

        let config = LayoutConfig {
            direction: LayoutDirection::Horizontal,
            primary_axis_align: PrimaryAxisAlign::SpaceBetween,
            counter_axis_align: CounterAxisAlign::Start,
            padding: LayoutPadding::default(),
            item_spacing: 0.0,
            wrap: LayoutWrap::NoWrap,
        };
        let mut parent = make_auto_layout_frame("Parent", 200.0, 100.0, config);

        let child1 = Node::new_frame("C1", 40.0, 30.0);
        let child2 = Node::new_frame("C2", 40.0, 30.0);
        let c1_id = doc.nodes.insert(child1);
        let c2_id = doc.nodes.insert(child2);

        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c1_id, c2_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);

        let r1 = layout.get(&c1_id).unwrap();
        let r2 = layout.get(&c2_id).unwrap();

        // C1 at start, C2 at end: 200 - 40 = 160
        assert!((r1.x - 0.0).abs() < 0.1, "C1.x = {}", r1.x);
        assert!((r2.x - 160.0).abs() < 0.1, "C2.x = {}", r2.x);
    }

    #[test]
    fn cross_axis_fill_stretches() {
        let mut doc = Document::new("Test");

        // Horizontal layout, 300x100
        let config = default_config();
        let mut parent = make_auto_layout_frame("Parent", 300.0, 100.0, config);

        // Child: width=Fixed(50), height=Fill → should stretch to container height (100)
        let mut child = Node::new_frame("C1", 50.0, 30.0);
        child.layout_sizing = Some(LayoutSizing {
            width: SizingMode::Fixed,
            height: SizingMode::Fill,
            align_self: None,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
        });
        let c_id = doc.nodes.insert(child);

        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);
        let r = layout.get(&c_id).unwrap();

        assert!((r.width - 50.0).abs() < 0.1, "w = {}", r.width);
        // Cross-axis Fill → stretches to container height
        assert!((r.height - 100.0).abs() < 0.1, "h = {}", r.height);
    }

    #[test]
    fn cross_axis_fill_vertical() {
        let mut doc = Document::new("Test");

        // Vertical layout, 200x400
        let config = LayoutConfig {
            direction: LayoutDirection::Vertical,
            ..default_config()
        };
        let mut parent = make_auto_layout_frame("Parent", 200.0, 400.0, config);

        // Child: width=Fill (cross axis in vertical), height=Fixed(60)
        let mut child = Node::new_frame("C1", 40.0, 60.0);
        child.layout_sizing = Some(LayoutSizing {
            width: SizingMode::Fill,
            height: SizingMode::Fixed,
            align_self: None,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
        });
        let c_id = doc.nodes.insert(child);

        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);
        let r = layout.get(&c_id).unwrap();

        // Cross-axis Fill → stretches to container width
        assert!((r.width - 200.0).abs() < 0.1, "w = {}", r.width);
        assert!((r.height - 60.0).abs() < 0.1, "h = {}", r.height);
    }

    #[test]
    fn instance_inherits_component_frame_size() {
        use ode_format::node::ComponentDef;

        let mut doc = Document::new("Instance Layout Test");

        // Create a component frame 80x40
        let mut comp = Node::new_frame("ButtonComp", 80.0, 40.0);
        let comp_stable = comp.stable_id.clone();
        if let NodeKind::Frame(ref mut data) = comp.kind {
            data.component_def = Some(ComponentDef {
                name: "Button".to_string(),
                description: "".to_string(),
            });
        }
        doc.nodes.insert(comp);

        // Create an instance (no size override → should inherit 80x40)
        let instance = Node::new_instance("ButtonInst", comp_stable);
        let inst_id = doc.nodes.insert(instance);

        // Parent auto-layout container
        let config = default_config();
        let mut parent = make_auto_layout_frame("Container", 300.0, 100.0, config);
        let fixed_child = Node::new_frame("Fixed", 50.0, 50.0);
        let fixed_id = doc.nodes.insert(fixed_child);

        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![fixed_id, inst_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);

        // Instance should be placed after the fixed child at x=50
        let inst_rect = layout.get(&inst_id).unwrap();
        assert!((inst_rect.x - 50.0).abs() < 0.1, "inst.x = {}", inst_rect.x);
        assert!(
            (inst_rect.width - 80.0).abs() < 0.1,
            "inst.w = {}",
            inst_rect.width
        );
        assert!(
            (inst_rect.height - 40.0).abs() < 0.1,
            "inst.h = {}",
            inst_rect.height
        );
    }

    #[test]
    fn instance_with_size_override_in_auto_layout() {
        use ode_format::node::ComponentDef;

        let mut doc = Document::new("Instance Size Override Test");

        // Component 80x40
        let mut comp = Node::new_frame("Comp", 80.0, 40.0);
        let comp_stable = comp.stable_id.clone();
        if let NodeKind::Frame(ref mut data) = comp.kind {
            data.component_def = Some(ComponentDef {
                name: "Comp".to_string(),
                description: "".to_string(),
            });
        }
        doc.nodes.insert(comp);

        // Instance with size override: 120x60
        let mut instance = Node::new_instance("Inst", comp_stable);
        if let NodeKind::Instance(ref mut data) = instance.kind {
            data.width = Some(120.0);
            data.height = Some(60.0);
        }
        let inst_id = doc.nodes.insert(instance);

        let config = default_config();
        let mut parent = make_auto_layout_frame("Container", 300.0, 100.0, config);
        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![inst_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let layout = test_compute_layout(&doc);
        let inst_rect = layout.get(&inst_id).unwrap();
        assert!(
            (inst_rect.width - 120.0).abs() < 0.1,
            "inst.w = {}",
            inst_rect.width
        );
        assert!(
            (inst_rect.height - 60.0).abs() < 0.1,
            "inst.h = {}",
            inst_rect.height
        );
    }
}
