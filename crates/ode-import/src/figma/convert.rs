//! Main Figma-to-ODE conversion: two-pass DFS traversal.
//!
//! **Pre-pass**: assigns stable IDs and builds Figma ID → StableId mappings.
//! **Main pass**: recursively converts every Figma node into an ODE `Node`.

use std::collections::HashMap;

use ode_format::document::{Document, Version, WorkingColorSpace};
use ode_format::node::{
    BooleanOpData, BooleanOperation, ComponentDef, ContainerProps, FillRule, FrameData, GroupData,
    ImageData, InstanceData, Node, NodeId, NodeKind, NodeTree, PathSegment, SizingMode, StableId,
    TextData, VectorData, VectorPath,
};
use ode_format::style::{BlendMode, VisualProps};
use ode_format::tokens::DesignTokens;
use ode_format::typography::TextSizingMode;

use super::convert_layout::{
    convert_constraints, convert_layout_config, convert_layout_sizing, convert_transform,
};
use super::convert_style::{convert_blend_mode, convert_effect, convert_fill, convert_stroke};
use super::convert_text::{convert_sizing_mode, convert_text_runs, convert_text_style};
use super::convert_tokens::convert_all_variables;
use super::svg_path::merge_figma_paths;
use super::types::{FigmaFileResponse, FigmaNode, FigmaVariablesResponse};
use crate::error::{ImportError, ImportWarning};

// ─── Public Types ────────────────────────────────────────────────────────────

/// Result of converting a Figma file into an ODE document.
#[derive(Debug)]
pub struct ImportResult {
    pub document: Document,
    pub warnings: Vec<ImportWarning>,
}

/// Main converter: stateless entry point via `FigmaConverter::convert`.
pub struct FigmaConverter;

impl FigmaConverter {
    /// Convert a Figma file response into an ODE `Document`.
    ///
    /// # Arguments
    /// - `file` — the deserialized `GET /v1/files/:key` response.
    /// - `variables` — optional `GET /v1/files/:key/variables/local` response.
    /// - `images` — pre-fetched image data keyed by Figma image ref.
    pub fn convert(
        file: FigmaFileResponse,
        variables: Option<FigmaVariablesResponse>,
        images: HashMap<String, Vec<u8>>,
    ) -> Result<ImportResult, ImportError> {
        let mut ctx = ConvertContext::new(&file, images);

        // ── Pre-pass: assign stable IDs ──────────────────────────────────
        ctx.pre_pass(&file.document);

        // ── Variables pass ───────────────────────────────────────────────
        let tokens = if let Some(ref vars) = variables {
            let (design_tokens, _variable_map) = convert_all_variables(&vars.meta);
            design_tokens
        } else {
            DesignTokens::new()
        };

        // ── Main DFS pass ───────────────────────────────────────────────
        let mut nodes = NodeTree::new();
        let mut canvas: Vec<NodeId> = Vec::new();

        // file.document is the DOCUMENT node; its children are CANVAS pages.
        if let Some(pages) = &file.document.children {
            for page in pages {
                // Each CANVAS page's children become root frames on the canvas.
                if let Some(frames) = &page.children {
                    for frame_node in frames {
                        if let Some(node) = ctx.convert_node(frame_node, &mut nodes, None) {
                            let id = nodes.insert(node);
                            canvas.push(id);
                        }
                    }
                }
            }
        }

        let document = Document {
            format_version: Version(0, 2, 0),
            name: file.name.clone(),
            nodes,
            canvas,
            tokens,
            views: Vec::new(),
            working_color_space: WorkingColorSpace::Srgb,
        };

        Ok(ImportResult {
            document,
            warnings: ctx.warnings,
        })
    }
}

// ─── Internal Context ────────────────────────────────────────────────────────

/// Mutable state carried through the conversion.
struct ConvertContext<'a> {
    /// Figma ID → ODE StableId.
    node_id_map: HashMap<String, StableId>,
    /// Figma component ID → ODE StableId (for instance resolution).
    component_map: HashMap<String, StableId>,
    /// Top-level component metadata from the file response.
    components_meta: &'a HashMap<String, super::types::FigmaComponentMeta>,
    /// Pre-fetched rasterized image data (unused in this pass but reserved).
    #[allow(dead_code)]
    images: HashMap<String, Vec<u8>>,
    /// Accumulated warnings.
    warnings: Vec<ImportWarning>,
}

impl<'a> ConvertContext<'a> {
    fn new(file: &'a FigmaFileResponse, images: HashMap<String, Vec<u8>>) -> Self {
        Self {
            node_id_map: HashMap::new(),
            component_map: HashMap::new(),
            components_meta: &file.components,
            images,
            warnings: Vec::new(),
        }
    }

    // ── Pre-pass ─────────────────────────────────────────────────────────

    /// DFS through all Figma nodes, assigning stable IDs and recording
    /// component nodes for later instance resolution.
    fn pre_pass(&mut self, node: &FigmaNode) {
        let stable_id = nanoid::nanoid!();
        self.node_id_map.insert(node.id.clone(), stable_id.clone());

        if node.node_type == "COMPONENT" {
            self.component_map.insert(node.id.clone(), stable_id);
        }

        if let Some(children) = &node.children {
            for child in children {
                self.pre_pass(child);
            }
        }
    }

    // ── Main DFS ─────────────────────────────────────────────────────────

    /// Convert a single Figma node into an ODE `Node`.
    ///
    /// Returns `None` for skipped node types (SLICE, FigJam, unknown).
    /// `parent` is the parent Figma node, used to decide whether to emit
    /// layout sizing information.
    fn convert_node(
        &mut self,
        fnode: &FigmaNode,
        nodes: &mut NodeTree,
        parent: Option<&FigmaNode>,
    ) -> Option<Node> {
        let node_type = fnode.node_type.as_str();

        // ── Skip types ───────────────────────────────────────────────
        match node_type {
            "SLICE" => return None,
            "STICKY" | "SHAPE_WITH_TEXT" | "CONNECTOR" | "STAMP" | "CODE_BLOCK" | "WIDGET"
            | "EMBED" | "LINK_UNFURL" | "MEDIA" => {
                self.warnings.push(ImportWarning {
                    node_id: fnode.id.clone(),
                    node_name: fnode.name.clone(),
                    message: format!("FigJam node type '{node_type}' is not supported, skipping"),
                });
                return None;
            }
            "DOCUMENT" | "CANVAS" => {
                // These are handled by the top-level loop, not recursively.
                return None;
            }
            _ => {}
        }

        // ── Mask warning ─────────────────────────────────────────────
        if fnode.is_mask == Some(true) {
            self.warnings.push(ImportWarning {
                node_id: fnode.id.clone(),
                node_name: fnode.name.clone(),
                message: "Mask nodes are not supported in ODE; mask flag ignored".to_string(),
            });
        }

        // ── Common fields ────────────────────────────────────────────
        let stable_id = self
            .node_id_map
            .get(&fnode.id)
            .cloned()
            .unwrap_or_else(|| nanoid::nanoid!());

        let name = fnode.name.clone();

        let transform = fnode
            .relative_transform
            .as_ref()
            .map(convert_transform)
            .unwrap_or_default();

        let opacity = fnode.opacity.unwrap_or(1.0);

        let blend_mode = fnode
            .blend_mode
            .as_deref()
            .map(|bm| convert_blend_mode(bm, &mut self.warnings))
            .unwrap_or(BlendMode::Normal);

        let visible = fnode.visible.unwrap_or(true);

        let constraints = fnode.constraints.as_ref().map(convert_constraints);

        // Layout sizing: only set if the parent has auto-layout.
        let layout_sizing = if parent_has_auto_layout(parent) {
            Some(convert_layout_sizing(
                fnode.layout_sizing_horizontal.as_deref(),
                fnode.layout_sizing_vertical.as_deref(),
                fnode.layout_align.as_deref(),
                fnode.min_width,
                fnode.max_width,
                fnode.min_height,
                fnode.max_height,
            ))
        } else {
            None
        };

        // ── Dispatch by node type ────────────────────────────────────
        let kind = match node_type {
            "FRAME" | "SECTION" | "COMPONENT_SET" | "TABLE" | "TABLE_CELL" => {
                // Check for image node promotion.
                if let Some(image_kind) = self.try_promote_to_image(fnode) {
                    image_kind
                } else {
                    self.convert_frame(fnode, nodes, None)
                }
            }
            "COMPONENT" => {
                let comp_def = self.build_component_def(fnode);
                self.convert_frame(fnode, nodes, Some(comp_def))
            }
            "GROUP" => {
                let children = self.convert_children(fnode, nodes);
                NodeKind::Group(Box::new(GroupData { children }))
            }
            "VECTOR" | "RECTANGLE" | "ELLIPSE" | "LINE" | "STAR" | "REGULAR_POLYGON" => {
                // Check for image node promotion (RECTANGLE with image fill).
                if node_type == "RECTANGLE" {
                    if let Some(image_kind) = self.try_promote_to_image(fnode) {
                        image_kind
                    } else {
                        self.convert_vector(fnode)
                    }
                } else {
                    self.convert_vector(fnode)
                }
            }
            "BOOLEAN_OPERATION" => self.convert_boolean_op(fnode, nodes),
            "TEXT" => self.convert_text(fnode),
            "INSTANCE" => self.convert_instance(fnode, nodes),
            _ => {
                self.warnings.push(ImportWarning {
                    node_id: fnode.id.clone(),
                    node_name: fnode.name.clone(),
                    message: format!("Unknown node type '{node_type}', skipping"),
                });
                return None;
            }
        };

        Some(Node {
            id: NodeId::default(),
            stable_id,
            name,
            transform,
            opacity,
            blend_mode,
            visible,
            constraints,
            layout_sizing,
            kind,
        })
    }

    // ── Frame ────────────────────────────────────────────────────────────

    fn convert_frame(
        &mut self,
        fnode: &FigmaNode,
        nodes: &mut NodeTree,
        component_def: Option<ComponentDef>,
    ) -> NodeKind {
        let (width, height) = node_size(fnode);
        let corner_radius = node_corner_radius(fnode);
        let visual = self.convert_visual_props(fnode);
        let children = self.convert_children(fnode, nodes);
        let clips_content = fnode.clips_content.unwrap_or(true);

        let layout = convert_layout_config(
            fnode.layout_mode.as_deref(),
            fnode.primary_axis_align_items.as_deref(),
            fnode.counter_axis_align_items.as_deref(),
            fnode.padding_top,
            fnode.padding_right,
            fnode.padding_bottom,
            fnode.padding_left,
            fnode.item_spacing,
            fnode.layout_wrap.as_deref(),
            &mut self.warnings,
        );

        // Derive sizing modes from Figma layout sizing fields.
        let width_sizing = match fnode.layout_sizing_horizontal.as_deref() {
            Some("HUG") => SizingMode::Hug,
            Some("FILL") => SizingMode::Fill,
            _ => SizingMode::Fixed,
        };
        let height_sizing = match fnode.layout_sizing_vertical.as_deref() {
            Some("HUG") => SizingMode::Hug,
            Some("FILL") => SizingMode::Fill,
            _ => SizingMode::Fixed,
        };

        NodeKind::Frame(Box::new(FrameData {
            width,
            height,
            width_sizing,
            height_sizing,
            corner_radius,
            clips_content,
            visual,
            container: ContainerProps { children, layout },
            component_def,
        }))
    }

    // ── Vector / Shape ───────────────────────────────────────────────────

    fn convert_vector(&mut self, fnode: &FigmaNode) -> NodeKind {
        let visual = self.convert_visual_props(fnode);
        let (path, fill_rule) = self.extract_path(fnode);

        NodeKind::Vector(Box::new(VectorData {
            visual,
            path,
            fill_rule,
        }))
    }

    fn extract_path(&mut self, fnode: &FigmaNode) -> (VectorPath, FillRule) {
        // Prefer fillGeometry SVG paths.
        if let Some(ref geom) = fnode.fill_geometry {
            if !geom.is_empty() {
                match merge_figma_paths(geom) {
                    Ok((path, rule)) => return (path, rule),
                    Err(e) => {
                        self.warnings.push(ImportWarning {
                            node_id: fnode.id.clone(),
                            node_name: fnode.name.clone(),
                            message: format!("Failed to parse SVG path: {e}"),
                        });
                    }
                }
            }
        }

        // Fallback: synthesize geometry from node type and size.
        let (w, h) = node_size(fnode);
        match fnode.node_type.as_str() {
            "RECTANGLE" => {
                let r = node_corner_radius(fnode);
                (make_rect_path(w, h, &r), FillRule::NonZero)
            }
            "ELLIPSE" => (make_ellipse_path(w, h), FillRule::NonZero),
            "LINE" => (make_line_path(w), FillRule::NonZero),
            _ => (VectorPath::default(), FillRule::NonZero),
        }
    }

    // ── Boolean Operation ────────────────────────────────────────────────

    fn convert_boolean_op(&mut self, fnode: &FigmaNode, nodes: &mut NodeTree) -> NodeKind {
        let visual = self.convert_visual_props(fnode);
        let op = match fnode.boolean_operation.as_deref() {
            Some("UNION") => BooleanOperation::Union,
            Some("INTERSECT") => BooleanOperation::Intersect,
            Some("SUBTRACT") => BooleanOperation::Subtract,
            Some("EXCLUDE") => BooleanOperation::Exclude,
            other => {
                self.warnings.push(ImportWarning {
                    node_id: fnode.id.clone(),
                    node_name: fnode.name.clone(),
                    message: format!(
                        "Unknown boolean operation '{}', defaulting to Union",
                        other.unwrap_or("(none)")
                    ),
                });
                BooleanOperation::Union
            }
        };
        let children = self.convert_children(fnode, nodes);

        NodeKind::BooleanOp(Box::new(BooleanOpData {
            visual,
            op,
            children,
        }))
    }

    // ── Text ─────────────────────────────────────────────────────────────

    fn convert_text(&mut self, fnode: &FigmaNode) -> NodeKind {
        let visual = self.convert_visual_props(fnode);
        let content = fnode.characters.clone().unwrap_or_default();
        let (width, height) = node_size(fnode);

        let default_style = fnode
            .style
            .as_ref()
            .map(convert_text_style)
            .unwrap_or_default();

        let runs = match (
            &fnode.character_style_overrides,
            &fnode.style_override_table,
        ) {
            (Some(overrides), Some(table)) => convert_text_runs(&content, overrides, table),
            _ => Vec::new(),
        };

        let sizing_mode = fnode
            .style
            .as_ref()
            .and_then(|s| s.text_auto_resize.as_deref())
            .map(|s| convert_sizing_mode(Some(s)))
            .unwrap_or(TextSizingMode::Fixed);

        NodeKind::Text(Box::new(TextData {
            visual,
            content,
            runs,
            default_style,
            width,
            height,
            sizing_mode,
        }))
    }

    // ── Instance ─────────────────────────────────────────────────────────

    fn convert_instance(&mut self, fnode: &FigmaNode, nodes: &mut NodeTree) -> NodeKind {
        let component_id = fnode.component_id.as_deref().unwrap_or("");

        if let Some(source_stable_id) = self.component_map.get(component_id).cloned() {
            let children = self.convert_children(fnode, nodes);
            let (width, height) = node_size(fnode);

            let layout = convert_layout_config(
                fnode.layout_mode.as_deref(),
                fnode.primary_axis_align_items.as_deref(),
                fnode.counter_axis_align_items.as_deref(),
                fnode.padding_top,
                fnode.padding_right,
                fnode.padding_bottom,
                fnode.padding_left,
                fnode.item_spacing,
                fnode.layout_wrap.as_deref(),
                &mut self.warnings,
            );

            NodeKind::Instance(Box::new(InstanceData {
                container: ContainerProps { children, layout },
                source_component: source_stable_id,
                width: Some(width),
                height: Some(height),
                overrides: Vec::new(),
            }))
        } else {
            // Component not found — warn and convert as Frame.
            self.warnings.push(ImportWarning {
                node_id: fnode.id.clone(),
                node_name: fnode.name.clone(),
                message: format!(
                    "Instance references unknown component '{component_id}', converting as Frame"
                ),
            });
            self.convert_frame(fnode, nodes, None)
        }
    }

    // ── Image Promotion ──────────────────────────────────────────────────

    /// Check if a node should be promoted to an Image node.
    ///
    /// Promotion happens when:
    /// - fills has exactly 1 entry of type IMAGE
    /// - no strokes
    /// - no effects
    /// - no children
    fn try_promote_to_image(&mut self, fnode: &FigmaNode) -> Option<NodeKind> {
        let fills = fnode.fills.as_ref()?;
        if fills.len() != 1 || fills[0].paint_type != "IMAGE" {
            return None;
        }

        let has_strokes = fnode.strokes.as_ref().is_some_and(|s| !s.is_empty());
        let has_effects = fnode.effects.as_ref().is_some_and(|e| !e.is_empty());
        let has_children = fnode.children.as_ref().is_some_and(|c| !c.is_empty());

        if has_strokes || has_effects || has_children {
            return None;
        }

        let visual = self.convert_visual_props(fnode);
        Some(NodeKind::Image(Box::new(ImageData { visual })))
    }

    // ── Visual Props ─────────────────────────────────────────────────────

    fn convert_visual_props(&mut self, fnode: &FigmaNode) -> VisualProps {
        let fills = fnode.fills.as_ref().map_or(Vec::new(), |fills| {
            fills
                .iter()
                .filter_map(|f| convert_fill(f, &mut self.warnings))
                .collect()
        });
        let strokes = fnode.strokes.as_ref().map_or(Vec::new(), |strokes| {
            strokes
                .iter()
                .filter_map(|s| {
                    convert_stroke(
                        s,
                        fnode.stroke_weight.unwrap_or(1.0),
                        fnode.stroke_align.as_deref(),
                        fnode.stroke_cap.as_deref(),
                        fnode.stroke_join.as_deref(),
                        fnode.stroke_miter_angle,
                        fnode.stroke_dashes.as_deref(),
                        &mut self.warnings,
                    )
                })
                .collect()
        });
        let effects = fnode.effects.as_ref().map_or(Vec::new(), |effects| {
            effects
                .iter()
                .filter_map(|e| convert_effect(e, &mut self.warnings))
                .collect()
        });

        VisualProps {
            fills,
            strokes,
            effects,
        }
    }

    // ── Children ─────────────────────────────────────────────────────────

    fn convert_children(&mut self, fnode: &FigmaNode, nodes: &mut NodeTree) -> Vec<NodeId> {
        fnode.children.as_ref().map_or(Vec::new(), |children| {
            children
                .iter()
                .filter_map(|child| {
                    self.convert_node(child, nodes, Some(fnode))
                        .map(|n| nodes.insert(n))
                })
                .collect()
        })
    }

    // ── Component Def ────────────────────────────────────────────────────

    fn build_component_def(&self, fnode: &FigmaNode) -> ComponentDef {
        if let Some(meta) = self.components_meta.get(&fnode.id) {
            ComponentDef {
                name: meta.name.clone(),
                description: meta.description.clone(),
            }
        } else {
            ComponentDef {
                name: fnode.name.clone(),
                description: String::new(),
            }
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extract (width, height) from a Figma node's `size` field.
fn node_size(fnode: &FigmaNode) -> (f32, f32) {
    fnode
        .size
        .as_ref()
        .map(|s| (s.x as f32, s.y as f32))
        .unwrap_or((0.0, 0.0))
}

/// Extract the corner radius array from a Figma node.
///
/// Prefers `rectangleCornerRadii` (per-corner) over `cornerRadius` (uniform).
fn node_corner_radius(fnode: &FigmaNode) -> [f32; 4] {
    if let Some(radii) = fnode.rectangle_corner_radii {
        radii
    } else if let Some(r) = fnode.corner_radius {
        [r, r, r, r]
    } else {
        [0.0; 4]
    }
}

/// Check whether a parent node has auto-layout enabled.
fn parent_has_auto_layout(parent: Option<&FigmaNode>) -> bool {
    parent
        .and_then(|p| p.layout_mode.as_deref())
        .is_some_and(|m| m != "NONE")
}

/// Generate a rectangle path from width, height, and optional corner radii.
fn make_rect_path(w: f32, h: f32, _radii: &[f32; 4]) -> VectorPath {
    // Simple rectangle (corner radii are stored on FrameData, not in the path).
    VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: w, y: 0.0 },
            PathSegment::LineTo { x: w, y: h },
            PathSegment::LineTo { x: 0.0, y: h },
            PathSegment::Close,
        ],
        closed: true,
    }
}

/// Generate an ellipse path (approximated with 4 cubic beziers).
fn make_ellipse_path(w: f32, h: f32) -> VectorPath {
    let rx = w / 2.0;
    let ry = h / 2.0;
    // Kappa constant for circular arc approximation.
    let k: f32 = 0.552_284_8;
    let kx = rx * k;
    let ky = ry * k;

    VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: rx, y: 0.0 },
            PathSegment::CurveTo {
                x1: rx + kx,
                y1: 0.0,
                x2: w,
                y2: ry - ky,
                x: w,
                y: ry,
            },
            PathSegment::CurveTo {
                x1: w,
                y1: ry + ky,
                x2: rx + kx,
                y2: h,
                x: rx,
                y: h,
            },
            PathSegment::CurveTo {
                x1: rx - kx,
                y1: h,
                x2: 0.0,
                y2: ry + ky,
                x: 0.0,
                y: ry,
            },
            PathSegment::CurveTo {
                x1: 0.0,
                y1: ry - ky,
                x2: rx - kx,
                y2: 0.0,
                x: rx,
                y: 0.0,
            },
            PathSegment::Close,
        ],
        closed: true,
    }
}

/// Generate a horizontal line path.
fn make_line_path(w: f32) -> VectorPath {
    VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: w, y: 0.0 },
        ],
        closed: false,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figma::types::*;

    /// Helper to build a minimal FigmaFileResponse.
    fn make_file(name: &str, pages: Vec<FigmaNode>) -> FigmaFileResponse {
        FigmaFileResponse {
            name: name.to_string(),
            document: FigmaNode {
                id: "0:0".to_string(),
                name: "Document".to_string(),
                node_type: "DOCUMENT".to_string(),
                children: Some(vec![FigmaNode {
                    id: "0:1".to_string(),
                    name: "Page 1".to_string(),
                    node_type: "CANVAS".to_string(),
                    children: Some(pages),
                    ..Default::default()
                }]),
                ..Default::default()
            },
            components: HashMap::new(),
            component_sets: HashMap::new(),
            schema_version: 0,
            styles: HashMap::new(),
        }
    }

    #[test]
    fn convert_empty_file() {
        let file = make_file("Empty", vec![]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        assert_eq!(result.document.name, "Empty");
        assert!(result.document.canvas.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn convert_single_frame() {
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Frame 1".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 200.0, y: 100.0 }),
            children: Some(vec![]),
            ..Default::default()
        };
        let file = make_file("Test", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        assert_eq!(result.document.canvas.len(), 1);
        let root_id = result.document.canvas[0];
        let root = &result.document.nodes[root_id];
        assert_eq!(root.name, "Frame 1");
        assert!(matches!(root.kind, NodeKind::Frame(_)));
        if let NodeKind::Frame(ref data) = root.kind {
            assert!((data.width - 200.0).abs() < f32::EPSILON);
            assert!((data.height - 100.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn convert_frame_with_text_and_vector() {
        let text_node = FigmaNode {
            id: "2:1".to_string(),
            name: "Title".to_string(),
            node_type: "TEXT".to_string(),
            characters: Some("Hello".to_string()),
            size: Some(FigmaVector { x: 100.0, y: 24.0 }),
            style: Some(FigmaTypeStyle {
                font_family: Some("Inter".to_string()),
                font_size: Some(16.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let vector_node = FigmaNode {
            id: "2:2".to_string(),
            name: "Star".to_string(),
            node_type: "VECTOR".to_string(),
            size: Some(FigmaVector { x: 50.0, y: 50.0 }),
            fill_geometry: Some(vec![FigmaPath {
                path: "M 0 0 L 50 0 L 50 50 L 0 50 Z".to_string(),
                winding_rule: Some("NONZERO".to_string()),
                overridden_fields: None,
            }]),
            ..Default::default()
        };
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Card".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 300.0, y: 200.0 }),
            children: Some(vec![text_node, vector_node]),
            ..Default::default()
        };

        let file = make_file("Test File", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();

        assert_eq!(result.document.name, "Test File");
        assert_eq!(result.document.canvas.len(), 1);

        // Frame + 2 children = 3 nodes total
        assert_eq!(result.document.nodes.len(), 3);

        let frame_id = result.document.canvas[0];
        let frame_node = &result.document.nodes[frame_id];
        assert_eq!(frame_node.name, "Card");
        if let NodeKind::Frame(ref data) = frame_node.kind {
            assert_eq!(data.container.children.len(), 2);

            let text_id = data.container.children[0];
            let text_n = &result.document.nodes[text_id];
            assert_eq!(text_n.name, "Title");
            assert!(matches!(text_n.kind, NodeKind::Text(_)));

            let vec_id = data.container.children[1];
            let vec_n = &result.document.nodes[vec_id];
            assert_eq!(vec_n.name, "Star");
            assert!(matches!(vec_n.kind, NodeKind::Vector(_)));
        } else {
            panic!("Expected Frame");
        }

        assert!(result.warnings.is_empty());
    }

    #[test]
    fn convert_slice_is_skipped() {
        let slice = FigmaNode {
            id: "3:1".to_string(),
            name: "Slice".to_string(),
            node_type: "SLICE".to_string(),
            ..Default::default()
        };
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Frame".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 100.0, y: 100.0 }),
            children: Some(vec![slice]),
            ..Default::default()
        };
        let file = make_file("Test", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        // Frame only, slice skipped.
        assert_eq!(result.document.nodes.len(), 1);
    }

    #[test]
    fn convert_unknown_type_warns_and_skips() {
        let unknown = FigmaNode {
            id: "3:1".to_string(),
            name: "Unknown".to_string(),
            node_type: "FUTURE_TYPE".to_string(),
            ..Default::default()
        };
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Frame".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 100.0, y: 100.0 }),
            children: Some(vec![unknown]),
            ..Default::default()
        };
        let file = make_file("Test", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        assert_eq!(result.document.nodes.len(), 1);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("FUTURE_TYPE"));
    }

    #[test]
    fn convert_group_with_children() {
        let child = FigmaNode {
            id: "3:1".to_string(),
            name: "Rect".to_string(),
            node_type: "RECTANGLE".to_string(),
            size: Some(FigmaVector { x: 50.0, y: 50.0 }),
            ..Default::default()
        };
        let group = FigmaNode {
            id: "2:1".to_string(),
            name: "Group".to_string(),
            node_type: "GROUP".to_string(),
            children: Some(vec![child]),
            ..Default::default()
        };
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Frame".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 100.0, y: 100.0 }),
            children: Some(vec![group]),
            ..Default::default()
        };
        let file = make_file("Test", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        // Frame + Group + Rect = 3
        assert_eq!(result.document.nodes.len(), 3);
    }

    #[test]
    fn convert_component_and_instance() {
        let component = FigmaNode {
            id: "10:1".to_string(),
            name: "Button".to_string(),
            node_type: "COMPONENT".to_string(),
            size: Some(FigmaVector { x: 120.0, y: 40.0 }),
            children: Some(vec![]),
            ..Default::default()
        };
        let instance = FigmaNode {
            id: "20:1".to_string(),
            name: "Button Instance".to_string(),
            node_type: "INSTANCE".to_string(),
            component_id: Some("10:1".to_string()),
            size: Some(FigmaVector { x: 120.0, y: 40.0 }),
            children: Some(vec![]),
            ..Default::default()
        };

        let mut file = make_file("Test", vec![component, instance]);
        file.components.insert(
            "10:1".to_string(),
            FigmaComponentMeta {
                key: "abc".to_string(),
                name: "Button".to_string(),
                description: "A button component".to_string(),
                component_set_id: None,
                documentation_links: None,
            },
        );

        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        assert_eq!(result.document.canvas.len(), 2);

        // First is a component (Frame with component_def).
        let comp_id = result.document.canvas[0];
        let comp_node = &result.document.nodes[comp_id];
        assert!(matches!(comp_node.kind, NodeKind::Frame(_)));
        if let NodeKind::Frame(ref data) = comp_node.kind {
            assert!(data.component_def.is_some());
            let def = data.component_def.as_ref().unwrap();
            assert_eq!(def.name, "Button");
            assert_eq!(def.description, "A button component");
        }

        // Second is an instance pointing to the component.
        let inst_id = result.document.canvas[1];
        let inst_node = &result.document.nodes[inst_id];
        assert!(matches!(inst_node.kind, NodeKind::Instance(_)));
        if let NodeKind::Instance(ref data) = inst_node.kind {
            assert_eq!(data.source_component, comp_node.stable_id);
        }

        assert!(result.warnings.is_empty());
    }

    #[test]
    fn instance_with_unknown_component_becomes_frame() {
        let instance = FigmaNode {
            id: "20:1".to_string(),
            name: "Orphan Instance".to_string(),
            node_type: "INSTANCE".to_string(),
            component_id: Some("999:999".to_string()),
            size: Some(FigmaVector { x: 100.0, y: 50.0 }),
            children: Some(vec![]),
            ..Default::default()
        };
        let file = make_file("Test", vec![instance]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        let id = result.document.canvas[0];
        let node = &result.document.nodes[id];
        assert!(matches!(node.kind, NodeKind::Frame(_)));
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("999:999"));
    }

    #[test]
    fn convert_boolean_operation() {
        let child_a = FigmaNode {
            id: "3:1".to_string(),
            name: "A".to_string(),
            node_type: "RECTANGLE".to_string(),
            size: Some(FigmaVector { x: 50.0, y: 50.0 }),
            ..Default::default()
        };
        let child_b = FigmaNode {
            id: "3:2".to_string(),
            name: "B".to_string(),
            node_type: "ELLIPSE".to_string(),
            size: Some(FigmaVector { x: 50.0, y: 50.0 }),
            ..Default::default()
        };
        let bool_op = FigmaNode {
            id: "2:1".to_string(),
            name: "Union".to_string(),
            node_type: "BOOLEAN_OPERATION".to_string(),
            boolean_operation: Some("UNION".to_string()),
            children: Some(vec![child_a, child_b]),
            ..Default::default()
        };
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Frame".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 100.0, y: 100.0 }),
            children: Some(vec![bool_op]),
            ..Default::default()
        };
        let file = make_file("Test", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        // Frame + BooleanOp + 2 children = 4
        assert_eq!(result.document.nodes.len(), 4);
    }

    #[test]
    fn convert_mask_node_warns() {
        let masked = FigmaNode {
            id: "2:1".to_string(),
            name: "MaskRect".to_string(),
            node_type: "RECTANGLE".to_string(),
            is_mask: Some(true),
            size: Some(FigmaVector { x: 50.0, y: 50.0 }),
            ..Default::default()
        };
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Frame".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 100.0, y: 100.0 }),
            children: Some(vec![masked]),
            ..Default::default()
        };
        let file = make_file("Test", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("Mask"));
    }

    #[test]
    fn image_promotion_for_rectangle_with_image_fill() {
        let paint = FigmaPaint {
            paint_type: "IMAGE".to_string(),
            image_ref: Some("img_abc".to_string()),
            ..Default::default()
        };
        let rect = FigmaNode {
            id: "2:1".to_string(),
            name: "Photo".to_string(),
            node_type: "RECTANGLE".to_string(),
            size: Some(FigmaVector { x: 200.0, y: 150.0 }),
            fills: Some(vec![paint]),
            strokes: Some(vec![]),
            effects: Some(vec![]),
            children: None,
            ..Default::default()
        };
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Frame".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 400.0, y: 300.0 }),
            children: Some(vec![rect]),
            ..Default::default()
        };
        let file = make_file("Test", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        let frame_id = result.document.canvas[0];
        let frame_node = &result.document.nodes[frame_id];
        if let NodeKind::Frame(ref data) = frame_node.kind {
            let img_id = data.container.children[0];
            let img_node = &result.document.nodes[img_id];
            assert!(matches!(img_node.kind, NodeKind::Image(_)));
        } else {
            panic!("Expected Frame");
        }
    }

    #[test]
    fn convert_transform_applied() {
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Rotated".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 100.0, y: 100.0 }),
            relative_transform: Some([[0.0, -1.0, 50.0], [1.0, 0.0, 75.0]]),
            children: Some(vec![]),
            ..Default::default()
        };
        let file = make_file("Test", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        let id = result.document.canvas[0];
        let node = &result.document.nodes[id];
        assert!((node.transform.a - 0.0).abs() < f32::EPSILON);
        assert!((node.transform.b - 1.0).abs() < f32::EPSILON);
        assert!((node.transform.c - (-1.0)).abs() < f32::EPSILON);
        assert!((node.transform.tx - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn convert_visible_false() {
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Hidden".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 100.0, y: 100.0 }),
            visible: Some(false),
            children: Some(vec![]),
            ..Default::default()
        };
        let file = make_file("Test", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        let id = result.document.canvas[0];
        assert!(!result.document.nodes[id].visible);
    }

    #[test]
    fn ellipse_path_generation() {
        let path = make_ellipse_path(100.0, 50.0);
        assert!(path.closed);
        // MoveTo + 4 CurveTo + Close = 6 segments
        assert_eq!(path.segments.len(), 6);
    }

    #[test]
    fn line_path_generation() {
        let path = make_line_path(80.0);
        assert!(!path.closed);
        assert_eq!(path.segments.len(), 2);
        assert!(matches!(
            path.segments[1],
            PathSegment::LineTo { x, .. } if (x - 80.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn figma_sticky_node_warns_and_skips() {
        let sticky = FigmaNode {
            id: "5:1".to_string(),
            name: "Note".to_string(),
            node_type: "STICKY".to_string(),
            ..Default::default()
        };
        let frame = FigmaNode {
            id: "1:1".to_string(),
            name: "Frame".to_string(),
            node_type: "FRAME".to_string(),
            size: Some(FigmaVector { x: 100.0, y: 100.0 }),
            children: Some(vec![sticky]),
            ..Default::default()
        };
        let file = make_file("Test", vec![frame]);
        let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
        assert_eq!(result.document.nodes.len(), 1); // only Frame
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("STICKY"));
    }
}
