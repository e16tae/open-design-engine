use std::path::Path;

use krilla::blend::BlendMode as KrillaBlendMode;
use krilla::color::rgb::Color as KrillaRgbColor;
use krilla::document::Document;
use krilla::geom::{PathBuilder, Size as KrillaSize, Transform as KrillaTransform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{
    self, Fill as KrillaFill, LinearGradient, Paint, Pattern, RadialGradient, SpreadMethod, Stop,
    Stroke as KrillaStroke, SweepGradient,
};
use krilla::surface::Surface;
use kurbo::{BezPath, PathEl};
use ode_core::scene::{
    RenderCommand, ResolvedGradientStop, ResolvedPaint, Scene, StrokeStyle,
};
use ode_format::color::Color;
use ode_format::node::FillRule;
use ode_format::style::{BlendMode, StrokeCap, StrokeJoin, StrokePosition};

use crate::error::ExportError;

pub struct PdfExporter;

impl PdfExporter {
    pub fn export(scene: &Scene, path: &Path) -> Result<(), ExportError> {
        let bytes = Self::export_bytes(scene)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    pub fn export_bytes(scene: &Scene) -> Result<Vec<u8>, ExportError> {
        let mut doc = Document::new();

        if scene.width <= 0.0 || scene.height <= 0.0 {
            return Err(ExportError::PdfGenerationFailed(format!(
                "invalid page size: {}x{}",
                scene.width, scene.height
            )));
        }
        let page_settings = PageSettings::new(scene.width, scene.height);

        let mut page = doc.start_page_with(page_settings);
        let mut surface = page.surface();
        let mut layer_stack: Vec<LayerState> = Vec::new();

        for cmd in &scene.commands {
            process_command(
                &mut surface,
                cmd,
                &mut layer_stack,
                scene.width,
                scene.height,
            )?;
        }

        surface.finish();
        page.finish();

        doc.finish()
            .map_err(|e| ExportError::PdfGenerationFailed(format!("{:?}", e)))
    }
}

// ─── Layer State ───

struct LayerState {
    push_count: u32,
}

// ─── Command Processing ───

fn process_command(
    surface: &mut Surface,
    cmd: &RenderCommand,
    layer_stack: &mut Vec<LayerState>,
    scene_w: f32,
    scene_h: f32,
) -> Result<(), ExportError> {
    match cmd {
        RenderCommand::PushLayer {
            opacity,
            blend_mode,
            clip,
            transform,
        } => {
            let mut push_count: u32 = 0;

            if let Some(clip_path) = clip {
                if let Some(krilla_clip) = bezpath_to_krilla(clip_path) {
                    surface.push_transform(&transform_to_krilla(transform));
                    surface.push_clip_path(&krilla_clip, &paint::FillRule::NonZero);
                    push_count += 2;
                }
            }

            if *blend_mode != BlendMode::Normal {
                surface.push_blend_mode(blend_mode_to_krilla(blend_mode));
                push_count += 1;
            }

            if (*opacity - 1.0).abs() > f32::EPSILON {
                let nf = NormalizedF32::new(opacity.clamp(0.0, 1.0))
                    .unwrap_or(NormalizedF32::ONE);
                surface.push_opacity(nf);
                push_count += 1;
            }

            surface.push_isolated();
            push_count += 1;

            layer_stack.push(LayerState { push_count });
        }
        RenderCommand::PopLayer => {
            if let Some(state) = layer_stack.pop() {
                for _ in 0..state.push_count {
                    surface.pop();
                }
            }
        }
        RenderCommand::FillPath {
            path,
            paint,
            fill_rule,
            transform,
        } => {
            let krilla_path = match bezpath_to_krilla(path) {
                Some(p) => p,
                None => return Ok(()),
            };

            let (resolved_paint, alpha) =
                resolve_paint_with_surface(surface, paint, scene_w, scene_h)?;
            let nf_alpha =
                NormalizedF32::new(alpha.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE);

            surface.push_transform(&transform_to_krilla(transform));
            surface.set_fill(Some(KrillaFill {
                paint: resolved_paint,
                opacity: nf_alpha,
                rule: fill_rule_to_krilla(fill_rule),
            }));
            surface.set_stroke(None);
            surface.draw_path(&krilla_path);
            surface.pop(); // transform
        }
        RenderCommand::StrokePath {
            path,
            paint,
            stroke,
            transform,
        } => {
            draw_stroke(surface, path, paint, stroke, transform, scene_w, scene_h)?;
        }
        RenderCommand::ApplyEffect { .. } => {
            // PDF has no native filter effects (shadows, blurs).
            // Silently skip, matching SVG exporter's approach for unsupported features.
        }
    }
    Ok(())
}

// ─── Stroke Drawing ───

fn draw_stroke(
    surface: &mut Surface,
    path: &BezPath,
    paint: &ResolvedPaint,
    stroke: &StrokeStyle,
    transform: &tiny_skia::Transform,
    scene_w: f32,
    scene_h: f32,
) -> Result<(), ExportError> {
    let krilla_path = match bezpath_to_krilla(path) {
        Some(p) => p,
        None => return Ok(()),
    };

    let (resolved_paint, alpha) = resolve_paint_with_surface(surface, paint, scene_w, scene_h)?;
    let nf_alpha = NormalizedF32::new(alpha.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE);

    let dash = stroke.dash.as_ref().map(|d| krilla::paint::StrokeDash {
        array: d.segments.clone(),
        offset: d.offset,
    });

    surface.push_transform(&transform_to_krilla(transform));

    match stroke.position {
        StrokePosition::Center => {
            surface.set_fill(None);
            surface.set_stroke(Some(KrillaStroke {
                paint: resolved_paint,
                width: stroke.width,
                miter_limit: stroke.miter_limit,
                line_cap: stroke_cap_to_krilla(&stroke.cap),
                line_join: stroke_join_to_krilla(&stroke.join),
                opacity: nf_alpha,
                dash,
            }));
            surface.draw_path(&krilla_path);
        }
        StrokePosition::Inside => {
            surface.push_clip_path(&krilla_path, &paint::FillRule::NonZero);
            surface.set_fill(None);
            surface.set_stroke(Some(KrillaStroke {
                paint: resolved_paint,
                width: stroke.width * 2.0,
                miter_limit: stroke.miter_limit,
                line_cap: stroke_cap_to_krilla(&stroke.cap),
                line_join: stroke_join_to_krilla(&stroke.join),
                opacity: nf_alpha,
                dash,
            }));
            surface.draw_path(&krilla_path);
            surface.pop(); // clip
        }
        StrokePosition::Outside => {
            let mut clip_builder = PathBuilder::new();
            clip_builder.move_to(0.0, 0.0);
            clip_builder.line_to(scene_w, 0.0);
            clip_builder.line_to(scene_w, scene_h);
            clip_builder.line_to(0.0, scene_h);
            clip_builder.close();
            // Add the original path for even-odd subtraction
            for el in path.elements() {
                match el {
                    PathEl::MoveTo(p) => clip_builder.move_to(p.x as f32, p.y as f32),
                    PathEl::LineTo(p) => clip_builder.line_to(p.x as f32, p.y as f32),
                    PathEl::QuadTo(p1, p2) => clip_builder.quad_to(
                        p1.x as f32,
                        p1.y as f32,
                        p2.x as f32,
                        p2.y as f32,
                    ),
                    PathEl::CurveTo(p1, p2, p3) => clip_builder.cubic_to(
                        p1.x as f32,
                        p1.y as f32,
                        p2.x as f32,
                        p2.y as f32,
                        p3.x as f32,
                        p3.y as f32,
                    ),
                    PathEl::ClosePath => clip_builder.close(),
                }
            }
            if let Some(clip_path) = clip_builder.finish() {
                surface.push_clip_path(&clip_path, &paint::FillRule::EvenOdd);
                surface.set_fill(None);
                surface.set_stroke(Some(KrillaStroke {
                    paint: resolved_paint,
                    width: stroke.width * 2.0,
                    miter_limit: stroke.miter_limit,
                    line_cap: stroke_cap_to_krilla(&stroke.cap),
                    line_join: stroke_join_to_krilla(&stroke.join),
                    opacity: nf_alpha,
                    dash,
                }));
                surface.draw_path(&krilla_path);
                surface.pop(); // clip
            }
        }
    }

    surface.pop(); // transform
    Ok(())
}

// ─── Paint Resolution ───

fn resolve_paint_with_surface(
    surface: &mut Surface,
    paint: &ResolvedPaint,
    scene_w: f32,
    scene_h: f32,
) -> Result<(Paint, f32), ExportError> {
    match paint {
        ResolvedPaint::Solid(color) => {
            let (krilla_color, alpha) = color_to_krilla(color);
            Ok((krilla_color.into(), alpha))
        }
        ResolvedPaint::LinearGradient { stops, start, end } => {
            let gradient = LinearGradient {
                x1: start.x as f32,
                y1: start.y as f32,
                x2: end.x as f32,
                y2: end.y as f32,
                transform: KrillaTransform::identity(),
                spread_method: SpreadMethod::Pad,
                stops: convert_stops(stops),
                anti_alias: true,
            };
            Ok((gradient.into(), 1.0))
        }
        ResolvedPaint::RadialGradient {
            stops,
            center,
            radius,
        } => {
            let cx = center.x as f32;
            let cy = center.y as f32;
            let rx = radius.x as f32;
            let ry = radius.y as f32;

            // For elliptical gradients, use the larger radius as cr and apply a scale transform
            let (cr, transform) = if (rx - ry).abs() > f32::EPSILON && rx > 0.0 {
                let scale_y = ry / rx;
                // Scale around center: translate(cx,cy) * scale(1, sy) * translate(-cx,-cy)
                // Combined affine: [1, 0, 0, sy, cx*(1-1), cy*(1-sy)]
                //                = [1, 0, 0, sy, 0, cy*(1-sy)]
                let transform = KrillaTransform::from_row(
                    1.0,
                    0.0,
                    0.0,
                    scale_y,
                    0.0,
                    cy * (1.0 - scale_y),
                );
                (rx, transform)
            } else {
                (rx, KrillaTransform::identity())
            };

            let gradient = RadialGradient {
                fx: cx,
                fy: cy,
                fr: 0.0,
                cx,
                cy,
                cr,
                transform,
                spread_method: SpreadMethod::Pad,
                stops: convert_stops(stops),
                anti_alias: true,
            };
            Ok((gradient.into(), 1.0))
        }
        ResolvedPaint::AngularGradient {
            stops,
            center,
            angle,
        } => {
            let gradient = SweepGradient {
                cx: center.x as f32,
                cy: center.y as f32,
                start_angle: *angle,
                end_angle: angle + 360.0,
                transform: KrillaTransform::identity(),
                spread_method: SpreadMethod::Pad,
                stops: convert_stops(stops),
                anti_alias: true,
            };
            Ok((gradient.into(), 1.0))
        }
        ResolvedPaint::DiamondGradient {
            stops,
            center,
            radius,
        } => {
            // No native diamond gradient in PDF — rasterize to image pattern
            rasterize_gradient_to_pattern(surface, scene_w, scene_h, |w, h| {
                ode_core::paint::generate_diamond_gradient_pixmap(w, h, stops, *center, *radius)
            })
        }
    }
}

/// Rasterize a gradient to a pixmap and create a PDF pattern paint.
fn rasterize_gradient_to_pattern(
    surface: &mut Surface,
    scene_w: f32,
    scene_h: f32,
    generate: impl FnOnce(u32, u32) -> Option<tiny_skia::Pixmap>,
) -> Result<(Paint, f32), ExportError> {
    let w = (scene_w.ceil() as u32).max(1);
    let h = (scene_h.ceil() as u32).max(1);
    let pixmap = generate(w, h).ok_or_else(|| {
        ExportError::PdfGenerationFailed("failed to generate gradient pixmap".into())
    })?;

    // Unpremultiply tiny_skia's premultiplied RGBA for Image::from_rgba8
    let pixmap_data = pixmap.data();
    let mut rgba_data = Vec::with_capacity(pixmap_data.len());
    for chunk in pixmap_data.chunks_exact(4) {
        let a = chunk[3] as f32 / 255.0;
        if a > 0.0 {
            rgba_data.push((chunk[0] as f32 / a).min(255.0) as u8);
            rgba_data.push((chunk[1] as f32 / a).min(255.0) as u8);
            rgba_data.push((chunk[2] as f32 / a).min(255.0) as u8);
            rgba_data.push(chunk[3]);
        } else {
            rgba_data.extend_from_slice(&[0, 0, 0, 0]);
        }
    }

    let image = Image::from_rgba8(rgba_data, w, h);
    let size = KrillaSize::from_wh(scene_w, scene_h).ok_or_else(|| {
        ExportError::PdfGenerationFailed(format!("invalid size: {}x{}", scene_w, scene_h))
    })?;

    // Create pattern: use surface's stream_builder to draw the image into a sub-surface
    let mut sb = surface.stream_builder();
    let mut sub = sb.surface();
    sub.draw_image(image, size);
    sub.finish();
    let stream = sb.finish();

    let pattern = Pattern {
        stream,
        transform: KrillaTransform::identity(),
        width: scene_w,
        height: scene_h,
    };
    Ok((pattern.into(), 1.0))
}

// ─── Conversion Helpers ───

fn bezpath_to_krilla(bp: &BezPath) -> Option<krilla::geom::Path> {
    let mut pb = PathBuilder::new();
    for el in bp.elements() {
        match el {
            PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(p1, p2) => {
                pb.quad_to(p1.x as f32, p1.y as f32, p2.x as f32, p2.y as f32)
            }
            PathEl::CurveTo(p1, p2, p3) => pb.cubic_to(
                p1.x as f32,
                p1.y as f32,
                p2.x as f32,
                p2.y as f32,
                p3.x as f32,
                p3.y as f32,
            ),
            PathEl::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

fn color_to_krilla(color: &Color) -> (KrillaRgbColor, f32) {
    let [r, g, b, a] = color.to_rgba_u8();
    (KrillaRgbColor::new(r, g, b), a as f32 / 255.0)
}

fn convert_stops(stops: &[ResolvedGradientStop]) -> Vec<Stop> {
    stops
        .iter()
        .map(|s| {
            let (color, alpha) = color_to_krilla(&s.color);
            Stop {
                offset: NormalizedF32::new(s.position.clamp(0.0, 1.0))
                    .unwrap_or(NormalizedF32::ZERO),
                color: color.into(),
                opacity: NormalizedF32::new(alpha.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE),
            }
        })
        .collect()
}

fn transform_to_krilla(t: &tiny_skia::Transform) -> KrillaTransform {
    KrillaTransform::from_row(t.sx, t.ky, t.kx, t.sy, t.tx, t.ty)
}

fn blend_mode_to_krilla(mode: &BlendMode) -> KrillaBlendMode {
    match mode {
        BlendMode::Normal => KrillaBlendMode::Normal,
        BlendMode::Multiply => KrillaBlendMode::Multiply,
        BlendMode::Screen => KrillaBlendMode::Screen,
        BlendMode::Overlay => KrillaBlendMode::Overlay,
        BlendMode::Darken => KrillaBlendMode::Darken,
        BlendMode::Lighten => KrillaBlendMode::Lighten,
        BlendMode::ColorDodge => KrillaBlendMode::ColorDodge,
        BlendMode::ColorBurn => KrillaBlendMode::ColorBurn,
        BlendMode::HardLight => KrillaBlendMode::HardLight,
        BlendMode::SoftLight => KrillaBlendMode::SoftLight,
        BlendMode::Difference => KrillaBlendMode::Difference,
        BlendMode::Exclusion => KrillaBlendMode::Exclusion,
        BlendMode::Hue => KrillaBlendMode::Hue,
        BlendMode::Saturation => KrillaBlendMode::Saturation,
        BlendMode::Color => KrillaBlendMode::Color,
        BlendMode::Luminosity => KrillaBlendMode::Luminosity,
    }
}

fn fill_rule_to_krilla(rule: &FillRule) -> paint::FillRule {
    match rule {
        FillRule::NonZero => paint::FillRule::NonZero,
        FillRule::EvenOdd => paint::FillRule::EvenOdd,
    }
}

fn stroke_cap_to_krilla(cap: &StrokeCap) -> paint::LineCap {
    match cap {
        StrokeCap::Butt => paint::LineCap::Butt,
        StrokeCap::Round => paint::LineCap::Round,
        StrokeCap::Square => paint::LineCap::Square,
    }
}

fn stroke_join_to_krilla(join: &StrokeJoin) -> paint::LineJoin {
    match join {
        StrokeJoin::Miter => paint::LineJoin::Miter,
        StrokeJoin::Round => paint::LineJoin::Round,
        StrokeJoin::Bevel => paint::LineJoin::Bevel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ode_core::scene::{RenderCommand, ResolvedGradientStop, ResolvedPaint, Scene, StrokeStyle};
    use ode_format::color::Color;
    use ode_format::node::FillRule;
    use ode_format::style::{BlendMode, StrokeCap, StrokeJoin, StrokePosition};

    fn make_rect_path(w: f32, h: f32) -> BezPath {
        let mut bp = BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((w as f64, 0.0));
        bp.line_to((w as f64, h as f64));
        bp.line_to((0.0, h as f64));
        bp.close_path();
        bp
    }

    #[test]
    fn export_bytes_solid_fill() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(100.0, 100.0),
                paint: ResolvedPaint::Solid(Color::Srgb {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert!(bytes.len() > 10);
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_bytes_linear_gradient() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(100.0, 100.0),
                paint: ResolvedPaint::LinearGradient {
                    stops: vec![
                        ResolvedGradientStop {
                            position: 0.0,
                            color: Color::black(),
                        },
                        ResolvedGradientStop {
                            position: 1.0,
                            color: Color::white(),
                        },
                    ],
                    start: kurbo::Point::new(0.0, 0.0),
                    end: kurbo::Point::new(100.0, 0.0),
                },
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_bytes_radial_gradient() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(100.0, 100.0),
                paint: ResolvedPaint::RadialGradient {
                    stops: vec![
                        ResolvedGradientStop {
                            position: 0.0,
                            color: Color::white(),
                        },
                        ResolvedGradientStop {
                            position: 1.0,
                            color: Color::black(),
                        },
                    ],
                    center: kurbo::Point::new(50.0, 50.0),
                    radius: kurbo::Point::new(50.0, 50.0),
                },
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_bytes_angular_gradient() {
        let scene = Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(50.0, 50.0),
                paint: ResolvedPaint::AngularGradient {
                    stops: vec![
                        ResolvedGradientStop {
                            position: 0.0,
                            color: Color::black(),
                        },
                        ResolvedGradientStop {
                            position: 1.0,
                            color: Color::white(),
                        },
                    ],
                    center: kurbo::Point::new(25.0, 25.0),
                    angle: 0.0,
                },
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_bytes_diamond_gradient_fallback() {
        let scene = Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(50.0, 50.0),
                paint: ResolvedPaint::DiamondGradient {
                    stops: vec![
                        ResolvedGradientStop {
                            position: 0.0,
                            color: Color::white(),
                        },
                        ResolvedGradientStop {
                            position: 1.0,
                            color: Color::black(),
                        },
                    ],
                    center: kurbo::Point::new(25.0, 25.0),
                    radius: kurbo::Point::new(25.0, 25.0),
                },
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_bytes_opacity_layer() {
        let scene = Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 0.5,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::FillPath {
                    path: make_rect_path(50.0, 50.0),
                    paint: ResolvedPaint::Solid(Color::black()),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_bytes_blend_mode() {
        let scene = Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Multiply,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::FillPath {
                    path: make_rect_path(50.0, 50.0),
                    paint: ResolvedPaint::Solid(Color::black()),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_bytes_inside_stroke() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![RenderCommand::StrokePath {
                path: make_rect_path(100.0, 100.0),
                paint: ResolvedPaint::Solid(Color::black()),
                stroke: StrokeStyle {
                    width: 4.0,
                    position: StrokePosition::Inside,
                    cap: StrokeCap::Butt,
                    join: StrokeJoin::Miter,
                    miter_limit: 4.0,
                    dash: None,
                },
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_bytes_outside_stroke() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![RenderCommand::StrokePath {
                path: make_rect_path(100.0, 100.0),
                paint: ResolvedPaint::Solid(Color::black()),
                stroke: StrokeStyle {
                    width: 4.0,
                    position: StrokePosition::Outside,
                    cap: StrokeCap::Butt,
                    join: StrokeJoin::Miter,
                    miter_limit: 4.0,
                    dash: None,
                },
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_bytes_nested_layers() {
        let scene = Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 0.8,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PushLayer {
                    opacity: 0.5,
                    blend_mode: BlendMode::Screen,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::FillPath {
                    path: make_rect_path(50.0, 50.0),
                    paint: ResolvedPaint::Solid(Color::black()),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
                RenderCommand::PopLayer,
            ],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_bytes_clip_path() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: Some(make_rect_path(50.0, 50.0)),
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::FillPath {
                    path: make_rect_path(100.0, 100.0),
                    paint: ResolvedPaint::Solid(Color::Srgb {
                        r: 0.0,
                        g: 0.0,
                        b: 1.0,
                        a: 1.0,
                    }),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_to_file() {
        let scene = Scene {
            width: 10.0,
            height: 10.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(10.0, 10.0),
                paint: ResolvedPaint::Solid(Color::black()),
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let path = std::env::temp_dir().join("ode_test_pdf_export.pdf");
        PdfExporter::export(&scene, &path).unwrap();
        let content = std::fs::read(&path).unwrap();
        assert_eq!(&content[..5], b"%PDF-");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn export_empty_scene() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![],
        };
        let bytes = PdfExporter::export_bytes(&scene).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn export_invalid_size_returns_error() {
        let scene = Scene {
            width: 0.0,
            height: 100.0,
            commands: vec![],
        };
        let result = PdfExporter::export_bytes(&scene);
        assert!(result.is_err());
    }
}
