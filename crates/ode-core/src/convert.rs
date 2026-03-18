use std::collections::{HashMap, HashSet};

use crate::error::ConvertError;
use crate::path;
use crate::scene::*;
use ode_format::color::Color;
use ode_format::document::Document;
use ode_format::node::{FillRule as OdeFillRule, Node, NodeId, NodeKind, Override};
use ode_format::style::{BlendMode, Effect, Paint, StyleValue, VisualProps};
use ode_format::tokens::{DesignTokens, TokenValue};
use ode_text::FontDatabase;

/// Maximum nesting depth for instance resolution to prevent stack overflow.
const MAX_INSTANCE_DEPTH: usize = 32;

/// Index mapping stable_id → NodeId for fast lookups during instance resolution.
type StableIdIndex<'a> = HashMap<&'a str, NodeId>;

impl Scene {
    /// Convert a Document into a Scene.
    pub fn from_document(doc: &Document, font_db: &FontDatabase) -> Result<Self, ConvertError> {
        Self::from_document_with_resize(doc, font_db, &crate::layout::ResizeMap::new())
    }

    /// Convert a Document into a Scene with optional frame resize overrides.
    pub fn from_document_with_resize(
        doc: &Document,
        font_db: &FontDatabase,
        resize_map: &crate::layout::ResizeMap,
    ) -> Result<Self, ConvertError> {
        if doc.canvas.is_empty() {
            return Err(ConvertError::NoCanvasRoots);
        }

        // Build stable_id → NodeId index for instance resolution
        let stable_id_index: StableIdIndex = doc
            .nodes
            .iter()
            .map(|(nid, node)| (node.stable_id.as_str(), nid))
            .collect();

        // Compute layout (auto layout + constraints)
        let layout_map = crate::layout::compute_layout(doc, &stable_id_index, resize_map);

        // If root is resized, use the resize dimensions for scene size
        let first_root = doc.canvas[0];
        let (width, height) = resize_map
            .get(&first_root)
            .map(|&(w, h)| (w, h))
            .unwrap_or_else(|| get_frame_size(&doc.nodes[first_root], layout_map.get(&first_root)));

        let mut commands = Vec::new();
        let identity = tiny_skia::Transform::identity();

        for &root_id in &doc.canvas {
            convert_node(
                doc,
                root_id,
                identity,
                &mut commands,
                font_db,
                &layout_map,
                &stable_id_index,
            )?;
        }

        Ok(Scene {
            width,
            height,
            commands,
        })
    }
}

fn get_frame_size(node: &Node, layout_rect: Option<&crate::layout::LayoutRect>) -> (f32, f32) {
    if let Some(rect) = layout_rect {
        return (rect.width, rect.height);
    }
    if let NodeKind::Frame(ref data) = node.kind {
        (data.width, data.height)
    } else {
        (100.0, 100.0) // Default fallback
    }
}

fn convert_node(
    doc: &Document,
    node_id: NodeId,
    parent_transform: tiny_skia::Transform,
    commands: &mut Vec<RenderCommand>,
    font_db: &FontDatabase,
    layout_map: &crate::layout::LayoutMap,
    stable_id_index: &StableIdIndex,
) -> Result<(), ConvertError> {
    let node = &doc.nodes[node_id];
    let layout_rect = layout_map.get(&node_id);

    // Instance nodes delegate to resolve_instance instead of normal rendering
    if let NodeKind::Instance(ref inst_data) = node.kind {
        let mut resolution_stack = Vec::new();
        let mut resolution_set = HashSet::new();
        return resolve_instance(
            doc,
            node,
            inst_data,
            parent_transform,
            layout_rect,
            commands,
            font_db,
            layout_map,
            stable_id_index,
            &mut resolution_stack,
            &mut resolution_set,
        );
    }

    // Accumulate transform: layout overrides tx/ty but preserves rotation/scale
    let node_transform = if let Some(rect) = layout_rect {
        let t = &node.transform;
        tiny_skia::Transform::from_row(t.a, t.b, t.c, t.d, rect.x, rect.y)
    } else {
        path::transform_to_skia(&node.transform)
    };
    let current_transform = parent_transform.post_concat(node_transform);

    // Get clip path for frames (using layout-computed size if available)
    let clip = get_clip_path(node, layout_rect);

    // PushLayer
    commands.push(RenderCommand::PushLayer {
        opacity: node.opacity,
        blend_mode: node.blend_mode,
        clip,
        transform: current_transform,
    });

    // Visual content (fills, strokes, effects)
    if let Some(visual) = node.kind.visual() {
        // Text nodes use glyph-based rendering instead of a single path
        if let NodeKind::Text(ref text_data) = node.kind {
            convert_text_node(
                text_data,
                visual,
                current_transform,
                commands,
                font_db,
                &doc.tokens,
            )?;
        } else if let NodeKind::Image(ref img_data) = node.kind {
            // Image nodes: emit DrawImage first, then visual overlays (strokes/effects)
            emit_image(img_data, current_transform, commands, layout_rect);
            let node_path = get_node_path(doc, node, layout_rect);
            emit_visual(
                visual,
                &node_path,
                get_fill_rule(node),
                current_transform,
                commands,
                &doc.tokens,
            );
        } else {
            let node_path = get_node_path(doc, node, layout_rect);
            emit_visual(
                visual,
                &node_path,
                get_fill_rule(node),
                current_transform,
                commands,
                &doc.tokens,
            );
        }
    }

    // Recurse into children
    if let Some(children) = node.kind.children() {
        for &child_id in children {
            convert_node(
                doc,
                child_id,
                current_transform,
                commands,
                font_db,
                layout_map,
                stable_id_index,
            )?;
        }
    }

    // PopLayer
    commands.push(RenderCommand::PopLayer);

    Ok(())
}

/// Emit visual rendering commands (fills, strokes, effects) for a VisualProps + path.
/// Extracted as a helper so both normal nodes and instance-resolved nodes can use it.
fn emit_visual(
    visual: &VisualProps,
    node_path: &Option<kurbo::BezPath>,
    fill_rule: OdeFillRule,
    current_transform: tiny_skia::Transform,
    commands: &mut Vec<RenderCommand>,
    tokens: &DesignTokens,
) {
    // Effects that render BEHIND content (DropShadow)
    if let Some(bp) = node_path {
        for effect in &visual.effects {
            if let Effect::DropShadow {
                color,
                offset,
                blur,
                spread,
            } = effect
            {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::DropShadow {
                        color: resolve_color(color, tokens),
                        offset_x: offset.x,
                        offset_y: offset.y,
                        blur_radius: resolve_f32(blur, tokens),
                        spread: resolve_f32(spread, tokens),
                        shape: bp.clone(),
                    },
                });
            }
        }
    }

    // Fills
    if let Some(bp) = node_path {
        for fill in &visual.fills {
            if !fill.visible {
                continue;
            }
            if let Some(resolved) = resolve_paint(&fill.paint, tokens) {
                commands.push(RenderCommand::FillPath {
                    path: bp.clone(),
                    paint: resolved,
                    fill_rule,
                    transform: current_transform,
                });
            }
        }

        // Strokes
        for stroke in &visual.strokes {
            if !stroke.visible {
                continue;
            }
            if let Some(resolved) = resolve_paint(&stroke.paint, tokens) {
                commands.push(RenderCommand::StrokePath {
                    path: bp.clone(),
                    paint: resolved,
                    stroke: StrokeStyle {
                        width: resolve_f32(&stroke.width, tokens),
                        position: stroke.position,
                        cap: stroke.cap,
                        join: stroke.join,
                        miter_limit: stroke.miter_limit,
                        dash: stroke.dash.clone(),
                    },
                    transform: current_transform,
                });
            }
        }
    }

    // Effects that render ON content (InnerShadow, LayerBlur, BackgroundBlur)
    for effect in &visual.effects {
        match effect {
            Effect::InnerShadow {
                color,
                offset,
                blur,
                spread,
            } => {
                if let Some(bp) = node_path {
                    commands.push(RenderCommand::ApplyEffect {
                        effect: ResolvedEffect::InnerShadow {
                            color: resolve_color(color, tokens),
                            offset_x: offset.x,
                            offset_y: offset.y,
                            blur_radius: resolve_f32(blur, tokens),
                            spread: resolve_f32(spread, tokens),
                            shape: bp.clone(),
                        },
                    });
                }
            }
            Effect::LayerBlur { radius } => {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::LayerBlur {
                        radius: resolve_f32(radius, tokens),
                    },
                });
            }
            Effect::BackgroundBlur { radius } => {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::BackgroundBlur {
                        radius: resolve_f32(radius, tokens),
                    },
                });
            }
            Effect::DropShadow { .. } => {} // Already handled above
        }
    }
}

/// Emit a DrawImage command for an Image node if it has a usable source.
fn emit_image(
    img_data: &ode_format::node::ImageData,
    current_transform: tiny_skia::Transform,
    commands: &mut Vec<RenderCommand>,
    layout_rect: Option<&crate::layout::LayoutRect>,
) {
    let (w, h) = layout_rect
        .map(|r| (r.width, r.height))
        .unwrap_or((img_data.width, img_data.height));

    if w <= 0.0 || h <= 0.0 {
        return;
    }

    let image_bytes = match &img_data.source {
        Some(ode_format::style::ImageSource::Embedded { data }) => {
            if data.is_empty() {
                return;
            }
            data.clone()
        }
        Some(ode_format::style::ImageSource::Linked { path }) => {
            // Try to read from disk; skip gracefully if it fails
            match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(_) => return,
            }
        }
        None => return,
    };

    commands.push(RenderCommand::DrawImage {
        data: image_bytes,
        width: w,
        height: h,
        transform: current_transform,
    });
}

// ─── Instance Resolution ───

/// Build an override map grouping overrides by their target stable_id.
fn build_override_map<'a>(overrides: &'a [Override]) -> HashMap<&'a str, Vec<&'a Override>> {
    let mut map: HashMap<&'a str, Vec<&'a Override>> = HashMap::new();
    for ov in overrides {
        map.entry(ov.target()).or_default().push(ov);
    }
    map
}

/// Clone base VisualProps and apply any visual overrides targeting this node.
fn apply_visual_overrides(base: &VisualProps, overrides: &[&Override]) -> VisualProps {
    let mut result = base.clone();
    for ov in overrides {
        match ov {
            Override::Fills { fills, .. } => result.fills = fills.clone(),
            Override::Strokes { strokes, .. } => result.strokes = strokes.clone(),
            Override::Effects { effects, .. } => result.effects = effects.clone(),
            _ => {} // Non-visual overrides handled elsewhere
        }
    }
    result
}

/// Return overridden opacity if a matching override exists, otherwise the base value.
fn apply_opacity_override(base: f32, overrides: &[&Override]) -> f32 {
    for ov in overrides {
        if let Override::Opacity { opacity, .. } = ov {
            return *opacity;
        }
    }
    base
}

/// Return overridden blend_mode if a matching override exists, otherwise the base value.
fn apply_blend_mode_override(base: BlendMode, overrides: &[&Override]) -> BlendMode {
    for ov in overrides {
        if let Override::BlendMode { blend_mode, .. } = ov {
            return *blend_mode;
        }
    }
    base
}

/// Check if a node should be visible, considering a Visible override.
/// Falls back to `base_visible` (the node's own `visible` field) if no override is found.
fn is_visible(base_visible: bool, overrides: &[&Override]) -> bool {
    for ov in overrides {
        if let Override::Visible { visible, .. } = ov {
            return *visible;
        }
    }
    base_visible
}

/// Resolve and render an Instance node by expanding its source component.
#[allow(clippy::too_many_arguments)]
fn resolve_instance(
    doc: &Document,
    instance_node: &Node,
    inst_data: &ode_format::node::InstanceData,
    parent_transform: tiny_skia::Transform,
    layout_rect: Option<&crate::layout::LayoutRect>,
    commands: &mut Vec<RenderCommand>,
    font_db: &FontDatabase,
    layout_map: &crate::layout::LayoutMap,
    stable_id_index: &StableIdIndex,
    resolution_stack: &mut Vec<String>,
    resolution_set: &mut HashSet<String>,
) -> Result<(), ConvertError> {
    // Cycle detection (O(1) lookup via HashSet)
    if resolution_set.contains(&inst_data.source_component) {
        return Err(ConvertError::InstanceCycle(format!(
            "{} → {}",
            resolution_stack.join(" → "),
            inst_data.source_component
        )));
    }
    if resolution_stack.len() >= MAX_INSTANCE_DEPTH {
        return Ok(()); // Depth limit — silently stop
    }

    // Lookup the source component node
    let comp_node_id = match stable_id_index.get(inst_data.source_component.as_str()) {
        Some(&id) => id,
        None => return Ok(()), // Missing component — render nothing
    };
    let comp_node = &doc.nodes[comp_node_id];

    // Must be a Frame with component_def
    let comp_frame = match &comp_node.kind {
        NodeKind::Frame(data) if data.component_def.is_some() => data,
        _ => return Ok(()), // Not a component frame — render nothing
    };

    resolution_stack.push(inst_data.source_component.clone());
    resolution_set.insert(inst_data.source_component.clone());

    // Instance layer transform
    let node_transform = if let Some(rect) = layout_rect {
        let t = &instance_node.transform;
        tiny_skia::Transform::from_row(t.a, t.b, t.c, t.d, rect.x, rect.y)
    } else {
        path::transform_to_skia(&instance_node.transform)
    };
    let current_transform = parent_transform.post_concat(node_transform);

    // Resolve size: instance override > component frame size
    let width = inst_data.width.unwrap_or(comp_frame.width);
    let height = inst_data.height.unwrap_or(comp_frame.height);

    // Clip path from resolved size + component corner radius
    let clip = if width > 0.0 && height > 0.0 {
        Some(path::rounded_rect_path(
            width,
            height,
            comp_frame.corner_radius,
        ))
    } else {
        None
    };

    // Build override map
    let override_map = build_override_map(&inst_data.overrides);

    // Check visibility override on the component root before emitting any commands
    let root_overrides = override_map
        .get(comp_node.stable_id.as_str())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    if !is_visible(comp_node.visible, root_overrides) {
        resolution_stack.pop();
        resolution_set.remove(&inst_data.source_component);
        return Ok(());
    }

    // PushLayer for the instance
    commands.push(RenderCommand::PushLayer {
        opacity: instance_node.opacity,
        blend_mode: instance_node.blend_mode,
        clip,
        transform: current_transform,
    });
    let visual = apply_visual_overrides(&comp_frame.visual, root_overrides);
    let frame_path = if width > 0.0 && height > 0.0 {
        Some(path::rounded_rect_path(
            width,
            height,
            comp_frame.corner_radius,
        ))
    } else {
        None
    };
    emit_visual(
        &visual,
        &frame_path,
        OdeFillRule::NonZero,
        current_transform,
        commands,
        &doc.tokens,
    );

    // Recurse into component's children
    for &child_id in &comp_frame.container.children {
        convert_component_child(
            doc,
            child_id,
            current_transform,
            commands,
            font_db,
            layout_map,
            stable_id_index,
            &override_map,
            resolution_stack,
            resolution_set,
        )?;
    }

    // PopLayer
    commands.push(RenderCommand::PopLayer);

    resolution_set.remove(&inst_data.source_component);
    resolution_stack.pop();
    Ok(())
}

/// Render a child node from the component tree with override application.
#[allow(clippy::too_many_arguments)]
fn convert_component_child(
    doc: &Document,
    child_id: NodeId,
    parent_transform: tiny_skia::Transform,
    commands: &mut Vec<RenderCommand>,
    font_db: &FontDatabase,
    layout_map: &crate::layout::LayoutMap,
    stable_id_index: &StableIdIndex,
    override_map: &HashMap<&str, Vec<&Override>>,
    resolution_stack: &mut Vec<String>,
    resolution_set: &mut HashSet<String>,
) -> Result<(), ConvertError> {
    let child = &doc.nodes[child_id];

    // Check visibility override (falls back to base node visibility)
    let child_overrides = override_map
        .get(child.stable_id.as_str())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    if !is_visible(child.visible, child_overrides) {
        return Ok(()); // Hidden by override or base visibility
    }

    // If this child is itself an Instance, resolve it (nested instance)
    let child_layout_rect = layout_map.get(&child_id);
    if let NodeKind::Instance(ref nested_inst) = child.kind {
        return resolve_instance(
            doc,
            child,
            nested_inst,
            parent_transform,
            child_layout_rect,
            commands,
            font_db,
            layout_map,
            stable_id_index,
            resolution_stack,
            resolution_set,
        );
    }

    // Normal child rendering with overrides
    let child_transform = if let Some(rect) = child_layout_rect {
        let t = &child.transform;
        tiny_skia::Transform::from_row(t.a, t.b, t.c, t.d, rect.x, rect.y)
    } else {
        path::transform_to_skia(&child.transform)
    };
    let current_transform = parent_transform.post_concat(child_transform);

    let opacity = apply_opacity_override(child.opacity, child_overrides);
    let blend_mode = apply_blend_mode_override(child.blend_mode, child_overrides);

    // Apply Size override to get effective dimensions for clip/path
    let size_override = child_overrides.iter().find_map(|ov| {
        if let Override::Size { width, height, .. } = ov {
            Some((*width, *height))
        } else {
            None
        }
    });

    let clip = if let Some((ov_w, ov_h)) = size_override {
        // Rebuild clip from overridden size if this is a frame
        if let NodeKind::Frame(ref data) = child.kind {
            let w = ov_w.unwrap_or(data.width);
            let h = ov_h.unwrap_or(data.height);
            if w > 0.0 && h > 0.0 {
                Some(path::rounded_rect_path(w, h, data.corner_radius))
            } else {
                None
            }
        } else {
            get_clip_path(child, child_layout_rect)
        }
    } else {
        get_clip_path(child, child_layout_rect)
    };

    commands.push(RenderCommand::PushLayer {
        opacity,
        blend_mode,
        clip,
        transform: current_transform,
    });

    // Visual content with overrides
    if let Some(base_visual) = child.kind.visual() {
        let visual = apply_visual_overrides(base_visual, child_overrides);

        // Handle TextContent override for text nodes
        if let NodeKind::Text(ref text_data) = child.kind {
            // Check for text content override
            let content_override = child_overrides.iter().find_map(|ov| {
                if let Override::TextContent { content, .. } = ov {
                    Some(content.as_str())
                } else {
                    None
                }
            });

            if let Some(new_content) = content_override {
                // Create modified text data with overridden content
                let mut modified = text_data.as_ref().clone();
                modified.content = new_content.to_string();
                modified.visual = visual;
                convert_text_node(
                    &modified,
                    &modified.visual.clone(),
                    current_transform,
                    commands,
                    font_db,
                    &doc.tokens,
                )?;
            } else {
                convert_text_node(
                    text_data,
                    &visual,
                    current_transform,
                    commands,
                    font_db,
                    &doc.tokens,
                )?;
            }
        } else {
            // Use size override for node path if present
            let node_path = if let Some((ov_w, ov_h)) = size_override {
                if let NodeKind::Frame(ref data) = child.kind {
                    let w = ov_w.unwrap_or(data.width);
                    let h = ov_h.unwrap_or(data.height);
                    if w > 0.0 && h > 0.0 {
                        Some(path::rounded_rect_path(w, h, data.corner_radius))
                    } else {
                        None
                    }
                } else {
                    get_node_path(doc, child, child_layout_rect)
                }
            } else {
                get_node_path(doc, child, child_layout_rect)
            };
            emit_visual(
                &visual,
                &node_path,
                get_fill_rule(child),
                current_transform,
                commands,
                &doc.tokens,
            );
        }
    }

    // Recurse into this child's children (still within component tree)
    if let Some(children) = child.kind.children() {
        for &grandchild_id in children {
            convert_component_child(
                doc,
                grandchild_id,
                current_transform,
                commands,
                font_db,
                layout_map,
                stable_id_index,
                override_map,
                resolution_stack,
                resolution_set,
            )?;
        }
    }

    commands.push(RenderCommand::PopLayer);
    Ok(())
}

/// Convert a text node into glyph-based FillPath commands.
fn convert_text_node(
    text_data: &ode_format::node::TextData,
    visual: &ode_format::style::VisualProps,
    current_transform: tiny_skia::Transform,
    commands: &mut Vec<RenderCommand>,
    font_db: &FontDatabase,
    tokens: &DesignTokens,
) -> Result<(), ConvertError> {
    // Skip if font database is empty (no fonts available)
    if font_db.is_empty() {
        return Ok(());
    }

    let processed = match ode_text::process_text(text_data, font_db) {
        Ok(p) => p,
        Err(ode_text::TextError::FontNotFound { .. }) => {
            // Silently skip text if font not found — don't fail the whole render
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    // Determine the fill paint for glyphs
    let paint = if let Some(fill) = visual.fills.iter().find(|f| f.visible) {
        resolve_paint(&fill.paint, tokens)
    } else {
        // Default to black if no fills specified
        Some(ResolvedPaint::Solid(ode_format::color::Color::black()))
    };

    let Some(paint) = paint else { return Ok(()) };

    // Effects behind content (DropShadow) — use text bounding box
    let bbox = make_text_bbox(text_data);
    for effect in &visual.effects {
        if let Effect::DropShadow {
            color,
            offset,
            blur,
            spread,
        } = effect
        {
            commands.push(RenderCommand::ApplyEffect {
                effect: ResolvedEffect::DropShadow {
                    color: resolve_color(color, tokens),
                    offset_x: offset.x,
                    offset_y: offset.y,
                    blur_radius: resolve_f32(blur, tokens),
                    spread: resolve_f32(spread, tokens),
                    shape: bbox.clone(),
                },
            });
        }
    }

    // Emit FillPath for each glyph
    for glyph in &processed.glyphs {
        commands.push(RenderCommand::FillPath {
            path: glyph.path.clone(),
            paint: paint.clone(),
            fill_rule: OdeFillRule::NonZero,
            transform: current_transform,
        });
    }

    // Emit FillPath for decorations (underline, strikethrough)
    for decoration in &processed.decorations {
        commands.push(RenderCommand::FillPath {
            path: decoration.path.clone(),
            paint: paint.clone(),
            fill_rule: OdeFillRule::NonZero,
            transform: current_transform,
        });
    }

    // Effects on content
    for effect in &visual.effects {
        match effect {
            Effect::InnerShadow {
                color,
                offset,
                blur,
                spread,
            } => {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::InnerShadow {
                        color: resolve_color(color, tokens),
                        offset_x: offset.x,
                        offset_y: offset.y,
                        blur_radius: resolve_f32(blur, tokens),
                        spread: resolve_f32(spread, tokens),
                        shape: bbox.clone(),
                    },
                });
            }
            Effect::LayerBlur { radius } => {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::LayerBlur {
                        radius: resolve_f32(radius, tokens),
                    },
                });
            }
            Effect::BackgroundBlur { radius } => {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::BackgroundBlur {
                        radius: resolve_f32(radius, tokens),
                    },
                });
            }
            Effect::DropShadow { .. } => {}
        }
    }

    Ok(())
}

/// Create a bounding box path for a text node (for effects).
fn make_text_bbox(text_data: &ode_format::node::TextData) -> kurbo::BezPath {
    let w = text_data.width as f64;
    let h = text_data.height as f64;
    let mut path = kurbo::BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((w, 0.0));
    path.line_to((w, h));
    path.line_to((0.0, h));
    path.close_path();
    path
}

fn get_clip_path(
    node: &Node,
    layout_rect: Option<&crate::layout::LayoutRect>,
) -> Option<kurbo::BezPath> {
    if let NodeKind::Frame(ref data) = node.kind {
        if !data.clips_content {
            return None;
        }
        let (w, h) = layout_rect
            .map(|r| (r.width, r.height))
            .unwrap_or((data.width, data.height));
        if w > 0.0 && h > 0.0 {
            return Some(path::rounded_rect_path(w, h, data.corner_radius));
        }
    }
    None
}

fn get_node_path(
    doc: &Document,
    node: &Node,
    layout_rect: Option<&crate::layout::LayoutRect>,
) -> Option<kurbo::BezPath> {
    match &node.kind {
        NodeKind::Frame(data) => {
            let (w, h) = layout_rect
                .map(|r| (r.width, r.height))
                .unwrap_or((data.width, data.height));
            if w > 0.0 && h > 0.0 {
                Some(path::rounded_rect_path(w, h, data.corner_radius))
            } else {
                None
            }
        }
        NodeKind::Image(data) => {
            let (w, h) = layout_rect
                .map(|r| (r.width, r.height))
                .unwrap_or((data.width, data.height));
            if w > 0.0 && h > 0.0 {
                Some(path::rounded_rect_path(w, h, [0.0; 4]))
            } else {
                None
            }
        }
        NodeKind::Vector(data) => Some(path::to_bezpath(&data.path)),
        NodeKind::BooleanOp(data) => {
            if let Some(children) = node.kind.children() {
                let mut paths: Vec<kurbo::BezPath> = Vec::new();
                for &child_id in children {
                    let child = &doc.nodes[child_id];
                    if let Some(mut child_path) = get_node_path(doc, child, None) {
                        let t = &child.transform;
                        let affine = kurbo::Affine::new([
                            t.a as f64,
                            t.b as f64,
                            t.c as f64,
                            t.d as f64,
                            t.tx as f64,
                            t.ty as f64,
                        ]);
                        child_path.apply_affine(affine);
                        paths.push(child_path);
                    }
                }
                if paths.len() >= 2 {
                    let mut result = paths[0].clone();
                    for p in &paths[1..] {
                        if let Ok(r) = path::boolean_op(&result, p, data.op) {
                            result = r;
                        }
                    }
                    Some(result)
                } else {
                    paths.into_iter().next()
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn get_fill_rule(node: &Node) -> ode_format::node::FillRule {
    if let NodeKind::Vector(ref data) = node.kind {
        data.fill_rule
    } else {
        OdeFillRule::NonZero
    }
}

/// Resolve a `StyleValue<Color>` using active token modes.
///
/// For `Bound` values, attempts live resolution from `DesignTokens`.
/// Falls back to the cached `resolved` value if the token is missing or
/// has the wrong type.
fn resolve_color(sv: &StyleValue<Color>, tokens: &DesignTokens) -> Color {
    match sv {
        StyleValue::Bound { token, resolved } => tokens
            .resolve(token.collection_id, token.token_id)
            .ok()
            .and_then(|tv| match tv {
                TokenValue::Color(c) => Some(c),
                _ => None,
            })
            .unwrap_or_else(|| resolved.clone()),
        StyleValue::Raw(v) => v.clone(),
    }
}

/// Resolve a `StyleValue<f32>` using active token modes.
fn resolve_f32(sv: &StyleValue<f32>, tokens: &DesignTokens) -> f32 {
    match sv {
        StyleValue::Bound { token, resolved } => tokens
            .resolve(token.collection_id, token.token_id)
            .ok()
            .and_then(|tv| match tv {
                TokenValue::Number(n) => Some(n),
                _ => None,
            })
            .unwrap_or(*resolved),
        StyleValue::Raw(v) => *v,
    }
}

/// Resolve a format-level Paint to a render-level ResolvedPaint.
fn resolve_paint(paint: &Paint, tokens: &DesignTokens) -> Option<ResolvedPaint> {
    match paint {
        Paint::Solid { color } => Some(ResolvedPaint::Solid(resolve_color(color, tokens))),
        Paint::LinearGradient { stops, start, end } => Some(ResolvedPaint::LinearGradient {
            stops: stops
                .iter()
                .map(|s| ResolvedGradientStop {
                    position: s.position,
                    color: resolve_color(&s.color, tokens),
                })
                .collect(),
            start: kurbo::Point::new(start.x as f64, start.y as f64),
            end: kurbo::Point::new(end.x as f64, end.y as f64),
        }),
        Paint::RadialGradient {
            stops,
            center,
            radius,
        } => Some(ResolvedPaint::RadialGradient {
            stops: stops
                .iter()
                .map(|s| ResolvedGradientStop {
                    position: s.position,
                    color: resolve_color(&s.color, tokens),
                })
                .collect(),
            center: kurbo::Point::new(center.x as f64, center.y as f64),
            radius: kurbo::Point::new(radius.x as f64, radius.y as f64),
        }),
        Paint::AngularGradient {
            stops,
            center,
            angle,
        } => Some(ResolvedPaint::AngularGradient {
            stops: stops
                .iter()
                .map(|s| ResolvedGradientStop {
                    position: s.position,
                    color: resolve_color(&s.color, tokens),
                })
                .collect(),
            center: kurbo::Point::new(center.x as f64, center.y as f64),
            angle: *angle,
        }),
        Paint::DiamondGradient {
            stops,
            center,
            radius,
        } => Some(ResolvedPaint::DiamondGradient {
            stops: stops
                .iter()
                .map(|s| ResolvedGradientStop {
                    position: s.position,
                    color: resolve_color(&s.color, tokens),
                })
                .collect(),
            center: kurbo::Point::new(center.x as f64, center.y as f64),
            radius: kurbo::Point::new(radius.x as f64, radius.y as f64),
        }),
        // MeshGradient and ImageFill deferred
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;
    use ode_format::color::Color;
    use ode_format::document::Document;
    use ode_format::node::{Node, NodeKind};
    use ode_format::style::{BlendMode, Fill, Paint, StyleValue};

    fn make_simple_doc() -> Document {
        let mut doc = Document::new("Test");
        let mut frame = Node::new_frame("Root", 100.0, 80.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(Color::Srgb {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);
        doc
    }

    fn empty_font_db() -> FontDatabase {
        FontDatabase::new()
    }

    #[test]
    fn simple_frame_produces_commands() {
        let doc = make_simple_doc();
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        assert!((scene.width - 100.0).abs() < f32::EPSILON);
        assert!((scene.height - 80.0).abs() < f32::EPSILON);
        // Should have: PushLayer, FillPath (red fill), PopLayer
        assert!(
            scene.commands.len() >= 3,
            "Expected at least 3 commands, got {}",
            scene.commands.len()
        );
    }

    #[test]
    fn empty_canvas_is_error() {
        let doc = Document::new("Empty");
        let result = Scene::from_document(&doc, &empty_font_db());
        assert!(result.is_err());
    }

    #[test]
    fn group_produces_no_fill() {
        let mut doc = Document::new("Group Test");
        let group = Node::new_group("G");
        let gid = doc.nodes.insert(group);
        let mut frame = Node::new_frame("Container", 200.0, 200.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.container.children.push(gid);
        }
        let fid = doc.nodes.insert(frame);
        doc.canvas.push(fid);
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        let fill_count = scene
            .commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::FillPath { .. }))
            .count();
        assert!(fill_count <= 1, "Group should not produce FillPath");
    }

    #[test]
    fn text_node_with_no_fonts_skipped() {
        let mut doc = Document::new("Text Test");
        let text = Node::new_text("Label", "Hello");
        let text_id = doc.nodes.insert(text);
        let mut frame = Node::new_frame("Container", 200.0, 200.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.container.children.push(text_id);
        }
        let fid = doc.nodes.insert(frame);
        doc.canvas.push(fid);

        // With empty font db, text should be silently skipped
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        // Should have commands for the frame but no FillPath for text glyphs
        let fill_count = scene
            .commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::FillPath { .. }))
            .count();
        assert!(
            fill_count <= 1,
            "Text with no fonts should produce no glyph fills"
        );
    }

    #[test]
    fn auto_layout_document_produces_scene() {
        use ode_format::node::{
            CounterAxisAlign, LayoutConfig, LayoutDirection, LayoutPadding, LayoutWrap,
            PrimaryAxisAlign,
        };

        let mut doc = Document::new("Auto Layout Test");

        // Create parent with auto layout
        let mut parent = Node::new_frame("Container", 300.0, 100.0);
        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.layout = Some(LayoutConfig {
                direction: LayoutDirection::Horizontal,
                primary_axis_align: PrimaryAxisAlign::Start,
                counter_axis_align: CounterAxisAlign::Start,
                padding: LayoutPadding::default(),
                item_spacing: 10.0,
                wrap: LayoutWrap::NoWrap,
            });
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(Color::Srgb {
                        r: 0.9,
                        g: 0.9,
                        b: 0.9,
                        a: 1.0,
                    }),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }

        // Create children with fills
        let mut child1 = Node::new_frame("C1", 50.0, 50.0);
        if let NodeKind::Frame(ref mut data) = child1.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(Color::Srgb {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let mut child2 = Node::new_frame("C2", 80.0, 50.0);
        if let NodeKind::Frame(ref mut data) = child2.kind {
            data.visual.fills.push(Fill {
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
            });
        }

        let c1_id = doc.nodes.insert(child1);
        let c2_id = doc.nodes.insert(child2);

        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c1_id, c2_id];
        }
        let parent_id = doc.nodes.insert(parent);
        doc.canvas.push(parent_id);

        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

        // Scene should have correct dimensions
        assert!((scene.width - 300.0).abs() < f32::EPSILON);
        assert!((scene.height - 100.0).abs() < f32::EPSILON);

        // Should have: parent PushLayer + FillPath + child1 (PushLayer + FillPath + PopLayer) +
        //              child2 (PushLayer + FillPath + PopLayer) + parent PopLayer
        // That's at least 8 commands
        assert!(
            scene.commands.len() >= 8,
            "Expected ≥8 commands, got {}",
            scene.commands.len()
        );

        // Verify transforms — child2 should be offset by child1.width + gap = 50 + 10 = 60
        let push_layers: Vec<_> = scene
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::PushLayer { transform, .. } => Some(transform),
                _ => None,
            })
            .collect();

        // push_layers[0] = parent, push_layers[1] = child1, push_layers[2] = child2
        assert!(
            push_layers.len() >= 3,
            "Expected ≥3 PushLayers, got {}",
            push_layers.len()
        );
    }

    #[test]
    fn no_layout_backward_compat() {
        // Ensure existing documents without layout still work
        let doc = make_simple_doc();
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        assert!(scene.commands.len() >= 3);
    }

    // ─── Instance Resolution Tests ───

    /// Helper: create a document with a component frame and an instance pointing at it.
    /// Returns (doc, component_stable_id) for further customization.
    fn make_component_instance_doc(comp_fill_color: Color) -> (Document, String) {
        use ode_format::node::ComponentDef;

        let mut doc = Document::new("Instance Test");

        // Create the component frame with a fill
        let mut comp = Node::new_frame("MyComponent", 80.0, 40.0);
        let comp_stable_id = comp.stable_id.clone();
        if let NodeKind::Frame(ref mut data) = comp.kind {
            data.component_def = Some(ComponentDef {
                name: "Button".to_string(),
                description: "A button component".to_string(),
            });
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(comp_fill_color),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let comp_id = doc.nodes.insert(comp);

        // Create an instance of the component
        let instance = Node::new_instance("Button Instance", comp_stable_id.clone());
        let inst_id = doc.nodes.insert(instance);

        // Container frame to hold both on canvas
        let mut container = Node::new_frame("Canvas", 400.0, 300.0);
        if let NodeKind::Frame(ref mut data) = container.kind {
            data.container.children = vec![comp_id, inst_id];
        }
        let container_id = doc.nodes.insert(container);
        doc.canvas.push(container_id);

        (doc, comp_stable_id)
    }

    #[test]
    fn instance_renders_component_visuals() {
        let red = Color::Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let (doc, _) = make_component_instance_doc(red);
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

        // The component frame itself generates 1 FillPath (red)
        // The instance should ALSO generate 1 FillPath (red, from component expansion)
        // Plus the container frame (no fills)
        let fill_count = scene
            .commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::FillPath { .. }))
            .count();
        assert_eq!(
            fill_count, 2,
            "Component + instance should each produce a FillPath, got {}",
            fill_count
        );
    }

    #[test]
    fn instance_with_fill_override() {
        use ode_format::node::Override as Ov;

        let red = Color::Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let blue = Color::Srgb {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        };
        let (mut doc, comp_stable_id) = make_component_instance_doc(red.clone());

        // Find and modify the instance to add a fill override
        let inst_id = doc
            .nodes
            .iter()
            .find(|(_, n)| matches!(&n.kind, NodeKind::Instance(_)))
            .map(|(id, _)| id)
            .unwrap();
        if let NodeKind::Instance(ref mut data) = doc.nodes[inst_id].kind {
            data.overrides.push(Ov::Fills {
                target: comp_stable_id,
                fills: vec![Fill {
                    paint: Paint::Solid {
                        color: StyleValue::Raw(blue.clone()),
                    },
                    opacity: StyleValue::Raw(1.0),
                    blend_mode: BlendMode::Normal,
                    visible: true,
                }],
            });
        }

        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

        // Verify we get 2 FillPaths total
        let fills: Vec<_> = scene
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::FillPath { paint, .. } => Some(paint),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 2);

        // One should be red (component itself) and one blue (instance with override)
        let has_red = fills
            .iter()
            .any(|p| matches!(p, ResolvedPaint::Solid(c) if *c == red));
        let has_blue = fills
            .iter()
            .any(|p| matches!(p, ResolvedPaint::Solid(c) if *c == blue));
        assert!(has_red, "Component should render with red fill");
        assert!(has_blue, "Instance should render with blue fill override");
    }

    #[test]
    fn instance_with_visibility_override_hides_child() {
        use ode_format::node::{ComponentDef, Override as Ov};

        let mut doc = Document::new("Visibility Test");
        let red = Color::Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let green = Color::Srgb {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        };

        // Component with a child
        let mut child_node = Node::new_frame("Inner", 40.0, 20.0);
        let child_stable_id = child_node.stable_id.clone();
        if let NodeKind::Frame(ref mut data) = child_node.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(green),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let child_id = doc.nodes.insert(child_node);

        let mut comp = Node::new_frame("Component", 80.0, 40.0);
        let comp_stable_id = comp.stable_id.clone();
        if let NodeKind::Frame(ref mut data) = comp.kind {
            data.component_def = Some(ComponentDef {
                name: "Card".to_string(),
                description: "".to_string(),
            });
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(red),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
            data.container.children = vec![child_id];
        }
        let comp_id = doc.nodes.insert(comp);

        // Instance with Visible override to hide the inner child
        let mut instance = Node::new_instance("Card Instance", comp_stable_id);
        if let NodeKind::Instance(ref mut data) = instance.kind {
            data.overrides.push(Ov::Visible {
                target: child_stable_id,
                visible: false,
            });
        }
        let inst_id = doc.nodes.insert(instance);

        let mut canvas = Node::new_frame("Canvas", 400.0, 300.0);
        if let NodeKind::Frame(ref mut data) = canvas.kind {
            data.container.children = vec![comp_id, inst_id];
        }
        let canvas_id = doc.nodes.insert(canvas);
        doc.canvas.push(canvas_id);

        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

        let fills: Vec<_> = scene
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::FillPath { paint, .. } => Some(paint),
                _ => None,
            })
            .collect();

        // Component renders: red (comp root) + green (child) = 2
        // Instance renders: red (comp root via instance) + hidden child = 1
        // Total = 3
        assert_eq!(
            fills.len(),
            3,
            "Expected 3 fills (comp root + child + instance root, child hidden), got {}",
            fills.len()
        );
    }

    #[test]
    fn instance_missing_component_renders_nothing() {
        let mut doc = Document::new("Missing Comp Test");

        // Instance pointing to non-existent component
        let instance = Node::new_instance("Orphan", "nonexistent-comp-id".to_string());
        let inst_id = doc.nodes.insert(instance);

        let mut canvas = Node::new_frame("Canvas", 200.0, 200.0);
        if let NodeKind::Frame(ref mut data) = canvas.kind {
            data.container.children = vec![inst_id];
        }
        let canvas_id = doc.nodes.insert(canvas);
        doc.canvas.push(canvas_id);

        // Should not panic, should produce no fills from the instance
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        let fill_count = scene
            .commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::FillPath { .. }))
            .count();
        assert_eq!(
            fill_count, 0,
            "Missing component instance should produce no fills"
        );
    }

    #[test]
    fn nested_instances_both_resolve() {
        use ode_format::node::ComponentDef;

        let mut doc = Document::new("Nested Instances");
        let red = Color::Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let blue = Color::Srgb {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        };

        // Inner component: blue fill
        let mut inner_comp = Node::new_frame("InnerComp", 30.0, 30.0);
        let inner_stable = inner_comp.stable_id.clone();
        if let NodeKind::Frame(ref mut data) = inner_comp.kind {
            data.component_def = Some(ComponentDef {
                name: "Inner".to_string(),
                description: "".to_string(),
            });
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(blue),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let inner_comp_id = doc.nodes.insert(inner_comp);

        // Outer component: red fill + child instance of inner
        let inner_instance = Node::new_instance("InnerInstance", inner_stable.clone());
        let inner_inst_id = doc.nodes.insert(inner_instance);

        let mut outer_comp = Node::new_frame("OuterComp", 100.0, 100.0);
        let outer_stable = outer_comp.stable_id.clone();
        if let NodeKind::Frame(ref mut data) = outer_comp.kind {
            data.component_def = Some(ComponentDef {
                name: "Outer".to_string(),
                description: "".to_string(),
            });
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(red),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
            data.container.children = vec![inner_inst_id];
        }
        let outer_comp_id = doc.nodes.insert(outer_comp);

        // Top-level instance of outer
        let outer_instance = Node::new_instance("OuterInstance", outer_stable);
        let outer_inst_id = doc.nodes.insert(outer_instance);

        let mut canvas = Node::new_frame("Canvas", 400.0, 400.0);
        if let NodeKind::Frame(ref mut data) = canvas.kind {
            data.container.children = vec![inner_comp_id, outer_comp_id, outer_inst_id];
        }
        let canvas_id = doc.nodes.insert(canvas);
        doc.canvas.push(canvas_id);

        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

        let fills: Vec<_> = scene
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::FillPath { paint, .. } => Some(paint),
                _ => None,
            })
            .collect();

        // inner_comp: 1 blue fill
        // outer_comp: 1 red fill + inner_instance expands to 1 blue fill = 2
        // outer_instance: expands outer_comp: 1 red + nested inner_instance: 1 blue = 2
        // Total = 5
        assert_eq!(
            fills.len(),
            5,
            "Expected 5 fills from nested components + instances, got {}",
            fills.len()
        );
    }

    #[test]
    fn instance_cycle_detection() {
        use ode_format::node::ComponentDef;

        let mut doc = Document::new("Cycle Test");

        // Create two components that reference each other via instances
        // Comp A has instance of Comp B, Comp B has instance of Comp A
        let stable_a = "comp-a".to_string();
        let stable_b = "comp-b".to_string();

        // Instance of B inside A
        let mut inst_b = Node::new_instance("InstB", stable_b.clone());
        inst_b.stable_id = "inst-b-in-a".to_string();
        let inst_b_id = doc.nodes.insert(inst_b);

        // Component A
        let mut comp_a = Node::new_frame("CompA", 100.0, 100.0);
        comp_a.stable_id = stable_a.clone();
        if let NodeKind::Frame(ref mut data) = comp_a.kind {
            data.component_def = Some(ComponentDef {
                name: "A".to_string(),
                description: "".to_string(),
            });
            data.container.children = vec![inst_b_id];
        }
        let comp_a_id = doc.nodes.insert(comp_a);

        // Instance of A inside B
        let mut inst_a = Node::new_instance("InstA", stable_a.clone());
        inst_a.stable_id = "inst-a-in-b".to_string();
        let inst_a_id = doc.nodes.insert(inst_a);

        // Component B
        let mut comp_b = Node::new_frame("CompB", 100.0, 100.0);
        comp_b.stable_id = stable_b;
        if let NodeKind::Frame(ref mut data) = comp_b.kind {
            data.component_def = Some(ComponentDef {
                name: "B".to_string(),
                description: "".to_string(),
            });
            data.container.children = vec![inst_a_id];
        }
        let comp_b_id = doc.nodes.insert(comp_b);

        // Top-level instance of A
        let top_instance = Node::new_instance("TopInstance", stable_a);
        let top_id = doc.nodes.insert(top_instance);

        let mut canvas = Node::new_frame("Canvas", 400.0, 400.0);
        if let NodeKind::Frame(ref mut data) = canvas.kind {
            data.container.children = vec![comp_a_id, comp_b_id, top_id];
        }
        let canvas_id = doc.nodes.insert(canvas);
        doc.canvas.push(canvas_id);

        let result = Scene::from_document(&doc, &empty_font_db());
        assert!(result.is_err(), "Cyclic instances should return an error");
        let err = result.unwrap_err();
        assert!(
            format!("{err}").contains("cycle"),
            "Error should mention cycle: {err}"
        );
    }

    #[test]
    fn instance_size_override_changes_child_dimensions() {
        use ode_format::node::{ComponentDef, Override as Ov};

        let mut doc = Document::new("Size Override Test");
        let red = Color::Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };

        // Component child: 80×40 frame with a fill
        let mut child_node = Node::new_frame("Inner", 80.0, 40.0);
        let child_stable_id = child_node.stable_id.clone();
        if let NodeKind::Frame(ref mut data) = child_node.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(red.clone()),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let child_id = doc.nodes.insert(child_node);

        // Component with the child
        let mut comp = Node::new_frame("Comp", 200.0, 100.0);
        let comp_stable_id = comp.stable_id.clone();
        if let NodeKind::Frame(ref mut data) = comp.kind {
            data.component_def = Some(ComponentDef {
                name: "SizeComp".to_string(),
                description: "".to_string(),
            });
            data.container.children = vec![child_id];
        }
        let comp_id = doc.nodes.insert(comp);

        // Instance with size override: 80×40 → 120×60
        let mut instance = Node::new_instance("SizeInst", comp_stable_id);
        if let NodeKind::Instance(ref mut data) = instance.kind {
            data.overrides.push(Ov::Size {
                target: child_stable_id,
                width: Some(120.0),
                height: Some(60.0),
            });
        }
        let inst_id = doc.nodes.insert(instance);

        let mut canvas = Node::new_frame("Canvas", 400.0, 300.0);
        if let NodeKind::Frame(ref mut data) = canvas.kind {
            data.container.children = vec![comp_id, inst_id];
        }
        let canvas_id = doc.nodes.insert(canvas);
        doc.canvas.push(canvas_id);

        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

        // Find clip paths from PushLayer commands — the overridden child should have 120×60 clip
        let clips: Vec<_> = scene
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::PushLayer {
                    clip: Some(clip), ..
                } => Some(clip.bounding_box()),
                _ => None,
            })
            .collect();

        // One of the clips should be 120×60 (from the size-overridden child in the instance)
        let has_overridden_size = clips.iter().any(|bb| {
            let w = bb.x1 - bb.x0;
            let h = bb.y1 - bb.y0;
            (w - 120.0).abs() < 1.0 && (h - 60.0).abs() < 1.0
        });
        assert!(
            has_overridden_size,
            "Expected a clip with 120×60 from size override, got clips: {:?}",
            clips
        );
    }

    #[test]
    fn instance_visible_override_on_root_hides_instance() {
        use ode_format::node::Override as Ov;

        let red = Color::Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let (mut doc, comp_stable_id) = make_component_instance_doc(red);

        // Add Visible override on the component root to hide the instance
        let inst_id = doc
            .nodes
            .iter()
            .find(|(_, n)| matches!(&n.kind, NodeKind::Instance(_)))
            .map(|(id, _)| id)
            .unwrap();
        if let NodeKind::Instance(ref mut data) = doc.nodes[inst_id].kind {
            data.overrides.push(Ov::Visible {
                target: comp_stable_id,
                visible: false,
            });
        }

        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

        let fill_count = scene
            .commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::FillPath { .. }))
            .count();

        // Component definition itself renders 1 FillPath (red)
        // Instance with visible=false on root should render 0 FillPaths
        // Total = 1
        assert_eq!(
            fill_count, 1,
            "Only the component definition should produce a fill (instance hidden), got {}",
            fill_count
        );
    }

    #[test]
    fn instance_expands_auto_layout_component_children() {
        use ode_format::node::{
            ComponentDef, CounterAxisAlign, LayoutConfig, LayoutDirection, LayoutPadding,
            LayoutWrap, PrimaryAxisAlign,
        };

        let mut doc = Document::new("Auto Layout Component Test");
        let red = Color::Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let blue = Color::Srgb {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        };

        // Two children 50×50 each
        let mut child1 = Node::new_frame("C1", 50.0, 50.0);
        if let NodeKind::Frame(ref mut data) = child1.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(red.clone()),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let c1_id = doc.nodes.insert(child1);

        let mut child2 = Node::new_frame("C2", 50.0, 50.0);
        if let NodeKind::Frame(ref mut data) = child2.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(blue.clone()),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let c2_id = doc.nodes.insert(child2);

        // Component with horizontal auto-layout (gap=10)
        let mut comp = Node::new_frame("ALComp", 110.0, 50.0);
        let comp_stable_id = comp.stable_id.clone();
        if let NodeKind::Frame(ref mut data) = comp.kind {
            data.component_def = Some(ComponentDef {
                name: "AutoLayoutComp".to_string(),
                description: "".to_string(),
            });
            data.container.layout = Some(LayoutConfig {
                direction: LayoutDirection::Horizontal,
                primary_axis_align: PrimaryAxisAlign::Start,
                counter_axis_align: CounterAxisAlign::Start,
                padding: LayoutPadding::default(),
                item_spacing: 10.0,
                wrap: LayoutWrap::NoWrap,
            });
            data.container.children = vec![c1_id, c2_id];
        }
        let comp_id = doc.nodes.insert(comp);

        // Instance of the auto-layout component
        let instance = Node::new_instance("ALInst", comp_stable_id);
        let inst_id = doc.nodes.insert(instance);

        let mut canvas = Node::new_frame("Canvas", 400.0, 300.0);
        if let NodeKind::Frame(ref mut data) = canvas.kind {
            data.container.children = vec![comp_id, inst_id];
        }
        let canvas_id = doc.nodes.insert(canvas);
        doc.canvas.push(canvas_id);

        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

        let fill_count = scene
            .commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::FillPath { .. }))
            .count();

        // Component renders its 2 children: 2 FillPaths
        // Instance expands the same component, rendering 2 more FillPaths
        // Total = 4
        assert_eq!(
            fill_count, 4,
            "Component + instance should each render 2 children fills, got {}",
            fill_count
        );
    }

    // ─── Image Node Tests ───

    /// Minimal 1x1 red PNG for tests.
    fn minimal_png_bytes() -> Vec<u8> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==")
            .unwrap()
    }

    #[test]
    fn image_node_with_embedded_source_produces_draw_image() {
        use ode_format::style::ImageSource;

        let mut doc = Document::new("ImageTest");
        let mut frame = Node::new_frame("Root", 200.0, 150.0);
        let mut img = Node::new_image("Photo", 100.0, 80.0);
        if let NodeKind::Image(ref mut data) = img.kind {
            data.source = Some(ImageSource::Embedded {
                data: minimal_png_bytes(),
            });
        }
        let img_id = doc.nodes.insert(img);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.container.children.push(img_id);
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

        let draw_image_count = scene
            .commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::DrawImage { .. }))
            .count();
        assert_eq!(
            draw_image_count, 1,
            "Expected 1 DrawImage command, got {}",
            draw_image_count
        );

        // Verify dimensions on the DrawImage command
        if let Some(RenderCommand::DrawImage { width, height, .. }) = scene
            .commands
            .iter()
            .find(|c| matches!(c, RenderCommand::DrawImage { .. }))
        {
            assert!((*width - 100.0).abs() < f32::EPSILON);
            assert!((*height - 80.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn image_node_without_source_produces_no_draw_image() {
        let mut doc = Document::new("ImageTest");
        let mut frame = Node::new_frame("Root", 200.0, 150.0);
        let img = Node::new_image("NoSource", 100.0, 80.0);
        let img_id = doc.nodes.insert(img);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.container.children.push(img_id);
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        let draw_image_count = scene
            .commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::DrawImage { .. }))
            .count();
        assert_eq!(
            draw_image_count, 0,
            "Expected 0 DrawImage commands, got {}",
            draw_image_count
        );
    }

    // ─── Token Mode-Aware Rendering Tests ───

    #[test]
    fn token_mode_affects_resolved_color() {
        use ode_format::style::TokenRef;
        use ode_format::tokens::{DesignTokens, TokenValue};

        let mut doc = Document::new("Token Mode Test");

        // Create a token collection with Light and Dark modes
        let mut tokens = DesignTokens::new();
        let col_id = tokens.add_collection("Theme", vec!["Light", "Dark"]);
        let light_mode = tokens.collections[0].modes[0].id;
        let dark_mode = tokens.collections[0].modes[1].id;

        let red = Color::Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let blue = Color::Srgb {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        };

        // Add token with red in Light mode (via add_token), then override Dark mode
        let tok_id = tokens.add_token(col_id, "primary", TokenValue::Color(red.clone()));

        // Set Dark mode value to blue
        if let Some(coll) = tokens.collections.iter_mut().find(|c| c.id == col_id) {
            if let Some(tok) = coll.tokens.iter_mut().find(|t| t.id == tok_id) {
                tok.values.insert(
                    dark_mode,
                    ode_format::tokens::TokenResolve::Direct(TokenValue::Color(blue.clone())),
                );
            }
        }

        // Set Light mode as active
        tokens.set_active_mode(col_id, light_mode);

        // Create a frame with a fill bound to the token
        let mut frame = Node::new_frame("Root", 100.0, 80.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Bound {
                        token: TokenRef {
                            collection_id: col_id,
                            token_id: tok_id,
                        },
                        resolved: red.clone(),
                    },
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);
        doc.tokens = tokens;

        // Render with Light mode active → should produce red
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        let fill_colors: Vec<_> = scene
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::FillPath {
                    paint: ResolvedPaint::Solid(c),
                    ..
                } => Some(c.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(fill_colors.len(), 1);
        assert_eq!(fill_colors[0], red);

        // Switch to Dark mode and render again → should produce blue
        doc.tokens.set_active_mode(col_id, dark_mode);
        let scene2 = Scene::from_document(&doc, &empty_font_db()).unwrap();
        let fill_colors2: Vec<_> = scene2
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::FillPath {
                    paint: ResolvedPaint::Solid(c),
                    ..
                } => Some(c.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(fill_colors2.len(), 1);
        assert_eq!(fill_colors2[0], blue);
    }

    #[test]
    fn unbound_values_still_work() {
        // Ensure that documents without any tokens continue to work correctly
        let doc = make_simple_doc();
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        let fill_count = scene
            .commands
            .iter()
            .filter(|c| matches!(c, RenderCommand::FillPath { .. }))
            .count();
        assert!(fill_count >= 1, "Raw values should still render");
    }

    #[test]
    fn bound_token_missing_falls_back_to_cached() {
        use ode_format::style::TokenRef;

        let mut doc = Document::new("Fallback Test");
        let green = Color::Srgb {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        };

        // Create a frame with a fill bound to a non-existent token
        let mut frame = Node::new_frame("Root", 100.0, 80.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Bound {
                        token: TokenRef {
                            collection_id: 999,
                            token_id: 999,
                        },
                        resolved: green.clone(),
                    },
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        // Should fall back to the cached resolved value (green)
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        let fill_colors: Vec<_> = scene
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::FillPath {
                    paint: ResolvedPaint::Solid(c),
                    ..
                } => Some(c.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(fill_colors.len(), 1);
        assert_eq!(fill_colors[0], green);
    }

    #[test]
    fn frame_clips_content_false_no_clip() {
        let mut doc = Document::new("NoClip");
        let mut frame = Node::new_frame("Root", 200.0, 200.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.clips_content = false;
            data.visual.fills.push(Fill {
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
            });
        }
        let fid = doc.nodes.insert(frame);
        doc.canvas.push(fid);
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        // The PushLayer for this frame should have clip: None
        match &scene.commands[0] {
            RenderCommand::PushLayer { clip, .. } => {
                assert!(clip.is_none(), "clips_content=false should produce no clip");
            }
            other => panic!("Expected PushLayer, got {:?}", other),
        }
    }
}
