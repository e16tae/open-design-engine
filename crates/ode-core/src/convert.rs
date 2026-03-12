use ode_format::document::Document;
use ode_format::node::{Node, NodeId, NodeKind, FillRule as OdeFillRule};
use ode_format::style::{Paint, Effect};
use crate::error::ConvertError;
use crate::scene::*;
use crate::path;

impl Scene {
    /// Convert a Document into a Scene.
    pub fn from_document(doc: &Document) -> Result<Self, ConvertError> {
        if doc.canvas.is_empty() {
            return Err(ConvertError::NoCanvasRoots);
        }

        // Determine scene size from first canvas root
        let first_root = doc.canvas[0];
        let (width, height) = get_frame_size(&doc.nodes[first_root]);

        let mut commands = Vec::new();
        let identity = tiny_skia::Transform::identity();

        for &root_id in &doc.canvas {
            convert_node(doc, root_id, identity, &mut commands);
        }

        Ok(Scene { width, height, commands })
    }
}

fn get_frame_size(node: &Node) -> (f32, f32) {
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
) {
    let node = &doc.nodes[node_id];

    // Accumulate transform
    let node_transform = path::transform_to_skia(&node.transform);
    let current_transform = parent_transform.post_concat(node_transform);

    // Get clip path for frames (clipping to frame bounds)
    let clip = get_clip_path(node);

    // PushLayer
    commands.push(RenderCommand::PushLayer {
        opacity: node.opacity,
        blend_mode: node.blend_mode,
        clip,
        transform: current_transform,
    });

    // Visual content (fills, strokes, effects)
    if let Some(visual) = node.kind.visual() {
        let node_path = get_node_path(doc, node);

        // Effects that render BEHIND content (DropShadow)
        for effect in &visual.effects {
            if let Effect::DropShadow { color, offset, blur, spread } = effect {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::DropShadow {
                        color: color.value(),
                        offset_x: offset.x,
                        offset_y: offset.y,
                        blur_radius: blur.value(),
                        spread: spread.value(),
                    },
                });
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
                    commands.push(RenderCommand::ApplyEffect {
                        effect: ResolvedEffect::InnerShadow {
                            color: color.value(),
                            offset_x: offset.x,
                            offset_y: offset.y,
                            blur_radius: blur.value(),
                            spread: spread.value(),
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
                Effect::DropShadow { .. } => {} // Already handled above
            }
        }
    }

    // Recurse into children
    if let Some(children) = node.kind.children() {
        for &child_id in children {
            convert_node(doc, child_id, current_transform, commands);
        }
    }

    // PopLayer
    commands.push(RenderCommand::PopLayer);
}

fn get_clip_path(node: &Node) -> Option<kurbo::BezPath> {
    if let NodeKind::Frame(ref data) = node.kind {
        if data.width > 0.0 && data.height > 0.0 {
            return Some(path::rounded_rect_path(data.width, data.height, data.corner_radius));
        }
    }
    None
}

fn get_node_path(doc: &Document, node: &Node) -> Option<kurbo::BezPath> {
    match &node.kind {
        NodeKind::Frame(data) => {
            if data.width > 0.0 && data.height > 0.0 {
                Some(path::rounded_rect_path(data.width, data.height, data.corner_radius))
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
                    if let Some(child_path) = get_node_path(doc, child) {
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

    #[test]
    fn simple_frame_produces_commands() {
        let doc = make_simple_doc();
        let scene = Scene::from_document(&doc).unwrap();
        assert!((scene.width - 100.0).abs() < f32::EPSILON);
        assert!((scene.height - 80.0).abs() < f32::EPSILON);
        // Should have: PushLayer, FillPath (red fill), PopLayer
        assert!(scene.commands.len() >= 3, "Expected at least 3 commands, got {}", scene.commands.len());
    }

    #[test]
    fn empty_canvas_is_error() {
        let doc = Document::new("Empty");
        let result = Scene::from_document(&doc);
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
        let scene = Scene::from_document(&doc).unwrap();
        let fill_count = scene.commands.iter().filter(|c| matches!(c, RenderCommand::FillPath { .. })).count();
        assert!(fill_count <= 1, "Group should not produce FillPath");
    }
}
