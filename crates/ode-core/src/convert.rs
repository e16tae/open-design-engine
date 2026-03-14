use ode_format::document::Document;
use ode_format::node::{Node, NodeId, NodeKind, FillRule as OdeFillRule};
use ode_format::style::{Paint, Effect};
use ode_text::FontDatabase;
use crate::error::ConvertError;
use crate::scene::*;
use crate::path;

impl Scene {
    /// Convert a Document into a Scene.
    ///
    /// The `font_db` is used to resolve fonts for text rendering.
    /// Pass `&FontDatabase::new()` if no text rendering is needed.
    pub fn from_document(doc: &Document, font_db: &FontDatabase) -> Result<Self, ConvertError> {
        if doc.canvas.is_empty() {
            return Err(ConvertError::NoCanvasRoots);
        }

        // Compute auto layout positions
        let layout_map = crate::layout::compute_layout(doc);

        // Determine scene size from first canvas root
        let first_root = doc.canvas[0];
        let (width, height) = get_frame_size(&doc.nodes[first_root], layout_map.get(&first_root));

        let mut commands = Vec::new();
        let identity = tiny_skia::Transform::identity();

        for &root_id in &doc.canvas {
            convert_node(doc, root_id, identity, &mut commands, font_db, &layout_map)?;
        }

        Ok(Scene { width, height, commands })
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
) -> Result<(), ConvertError> {
    let node = &doc.nodes[node_id];
    let layout_rect = layout_map.get(&node_id);

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
            convert_text_node(text_data, visual, current_transform, commands, font_db)?;
        } else {
            let node_path = get_node_path(doc, node, layout_rect);

            // Effects that render BEHIND content (DropShadow)
            if let Some(ref bp) = node_path {
                for effect in &visual.effects {
                    if let Effect::DropShadow { color, offset, blur, spread } = effect {
                        commands.push(RenderCommand::ApplyEffect {
                            effect: ResolvedEffect::DropShadow {
                                color: color.value(),
                                offset_x: offset.x,
                                offset_y: offset.y,
                                blur_radius: blur.value(),
                                spread: spread.value(),
                                shape: bp.clone(),
                            },
                        });
                    }
                }
            }

            // Fills
            if let Some(ref bp) = node_path {
                let fill_rule = get_fill_rule(node);
                for fill in &visual.fills {
                    if !fill.visible { continue; }
                    if let Some(resolved) = resolve_paint(&fill.paint) {
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
                    if !stroke.visible { continue; }
                    if let Some(resolved) = resolve_paint(&stroke.paint) {
                        commands.push(RenderCommand::StrokePath {
                            path: bp.clone(),
                            paint: resolved,
                            stroke: StrokeStyle {
                                width: stroke.width.value(),
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
                    Effect::InnerShadow { color, offset, blur, spread } => {
                        if let Some(ref bp) = node_path {
                            commands.push(RenderCommand::ApplyEffect {
                                effect: ResolvedEffect::InnerShadow {
                                    color: color.value(),
                                    offset_x: offset.x,
                                    offset_y: offset.y,
                                    blur_radius: blur.value(),
                                    spread: spread.value(),
                                    shape: bp.clone(),
                                },
                            });
                        }
                    }
                    Effect::LayerBlur { radius } => {
                        commands.push(RenderCommand::ApplyEffect {
                            effect: ResolvedEffect::LayerBlur { radius: radius.value() },
                        });
                    }
                    Effect::BackgroundBlur { radius } => {
                        commands.push(RenderCommand::ApplyEffect {
                            effect: ResolvedEffect::BackgroundBlur { radius: radius.value() },
                        });
                    }
                    Effect::DropShadow { .. } => {} // Already handled above
                }
            }
        }
    }

    // Recurse into children
    if let Some(children) = node.kind.children() {
        for &child_id in children {
            convert_node(doc, child_id, current_transform, commands, font_db, layout_map)?;
        }
    }

    // PopLayer
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
        resolve_paint(&fill.paint)
    } else {
        // Default to black if no fills specified
        Some(ResolvedPaint::Solid(ode_format::color::Color::black()))
    };

    let Some(paint) = paint else { return Ok(()) };

    // Effects behind content (DropShadow) — use text bounding box
    let bbox = make_text_bbox(text_data);
    for effect in &visual.effects {
        if let Effect::DropShadow { color, offset, blur, spread } = effect {
            commands.push(RenderCommand::ApplyEffect {
                effect: ResolvedEffect::DropShadow {
                    color: color.value(),
                    offset_x: offset.x,
                    offset_y: offset.y,
                    blur_radius: blur.value(),
                    spread: spread.value(),
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
            Effect::InnerShadow { color, offset, blur, spread } => {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::InnerShadow {
                        color: color.value(),
                        offset_x: offset.x,
                        offset_y: offset.y,
                        blur_radius: blur.value(),
                        spread: spread.value(),
                        shape: bbox.clone(),
                    },
                });
            }
            Effect::LayerBlur { radius } => {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::LayerBlur { radius: radius.value() },
                });
            }
            Effect::BackgroundBlur { radius } => {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::BackgroundBlur { radius: radius.value() },
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
        NodeKind::Vector(data) => {
            Some(path::to_bezpath(&data.path))
        }
        NodeKind::BooleanOp(data) => {
            if let Some(children) = node.kind.children() {
                let mut paths: Vec<kurbo::BezPath> = Vec::new();
                for &child_id in children {
                    let child = &doc.nodes[child_id];
                    if let Some(mut child_path) = get_node_path(doc, child, None) {
                        let t = &child.transform;
                        let affine = kurbo::Affine::new([
                            t.a as f64, t.b as f64,
                            t.c as f64, t.d as f64,
                            t.tx as f64, t.ty as f64,
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

/// Resolve a format-level Paint to a render-level ResolvedPaint.
fn resolve_paint(paint: &Paint) -> Option<ResolvedPaint> {
    match paint {
        Paint::Solid { color } => Some(ResolvedPaint::Solid(color.value())),
        Paint::LinearGradient { stops, start, end } => Some(ResolvedPaint::LinearGradient {
            stops: stops.iter().map(|s| ResolvedGradientStop {
                position: s.position,
                color: s.color.value(),
            }).collect(),
            start: kurbo::Point::new(start.x as f64, start.y as f64),
            end: kurbo::Point::new(end.x as f64, end.y as f64),
        }),
        Paint::RadialGradient { stops, center, radius } => Some(ResolvedPaint::RadialGradient {
            stops: stops.iter().map(|s| ResolvedGradientStop { position: s.position, color: s.color.value() }).collect(),
            center: kurbo::Point::new(center.x as f64, center.y as f64),
            radius: kurbo::Point::new(radius.x as f64, radius.y as f64),
        }),
        Paint::AngularGradient { stops, center, angle } => Some(ResolvedPaint::AngularGradient {
            stops: stops.iter().map(|s| ResolvedGradientStop { position: s.position, color: s.color.value() }).collect(),
            center: kurbo::Point::new(center.x as f64, center.y as f64),
            angle: *angle,
        }),
        Paint::DiamondGradient { stops, center, radius } => Some(ResolvedPaint::DiamondGradient {
            stops: stops.iter().map(|s| ResolvedGradientStop { position: s.position, color: s.color.value() }).collect(),
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
    use ode_format::document::Document;
    use ode_format::node::{Node, NodeKind};
    use ode_format::style::{Fill, BlendMode, Paint, StyleValue};
    use ode_format::color::Color;

    fn make_simple_doc() -> Document {
        let mut doc = Document::new("Test");
        let mut frame = Node::new_frame("Root", 100.0, 80.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }) },
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
        assert!(scene.commands.len() >= 3, "Expected at least 3 commands, got {}", scene.commands.len());
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
        let fill_count = scene.commands.iter().filter(|c| matches!(c, RenderCommand::FillPath { .. })).count();
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
        let fill_count = scene.commands.iter()
            .filter(|c| matches!(c, RenderCommand::FillPath { .. }))
            .count();
        assert!(fill_count <= 1, "Text with no fonts should produce no glyph fills");
    }

    #[test]
    fn auto_layout_document_produces_scene() {
        use ode_format::node::{LayoutConfig, LayoutDirection, LayoutPadding, LayoutWrap,
            PrimaryAxisAlign, CounterAxisAlign};

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
                paint: Paint::Solid { color: StyleValue::Raw(Color::Srgb { r: 0.9, g: 0.9, b: 0.9, a: 1.0 }) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }

        // Create children with fills
        let mut child1 = Node::new_frame("C1", 50.0, 50.0);
        if let NodeKind::Frame(ref mut data) = child1.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let mut child2 = Node::new_frame("C2", 80.0, 50.0);
        if let NodeKind::Frame(ref mut data) = child2.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::Srgb { r: 0.0, g: 0.0, b: 1.0, a: 1.0 }) },
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
        assert!(scene.commands.len() >= 8, "Expected ≥8 commands, got {}", scene.commands.len());

        // Verify transforms — child2 should be offset by child1.width + gap = 50 + 10 = 60
        let push_layers: Vec<_> = scene.commands.iter()
            .filter_map(|c| match c {
                RenderCommand::PushLayer { transform, .. } => Some(transform),
                _ => None,
            })
            .collect();

        // push_layers[0] = parent, push_layers[1] = child1, push_layers[2] = child2
        assert!(push_layers.len() >= 3, "Expected ≥3 PushLayers, got {}", push_layers.len());
    }

    #[test]
    fn no_layout_backward_compat() {
        // Ensure existing documents without layout still work
        let doc = make_simple_doc();
        let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
        assert!(scene.commands.len() >= 3);
    }
}
