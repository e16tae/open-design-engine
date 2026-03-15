use crate::blend;
use crate::effects;
use crate::error::RenderError;
use crate::paint;
use crate::path;
use crate::scene::*;

/// Stateless renderer: converts a Scene into a Pixmap.
pub struct Renderer;

struct LayerEntry {
    pixmap: tiny_skia::Pixmap,
    mask: Option<tiny_skia::Mask>,
    paint: tiny_skia::PixmapPaint,
}

impl Renderer {
    pub fn render(scene: &Scene) -> Result<tiny_skia::Pixmap, RenderError> {
        let w = scene.width.ceil() as u32;
        let h = scene.height.ceil() as u32;
        if w == 0 || h == 0 {
            return Err(RenderError::PixmapCreationFailed {
                width: w,
                height: h,
            });
        }
        let root = tiny_skia::Pixmap::new(w, h).ok_or(RenderError::PixmapCreationFailed {
            width: w,
            height: h,
        })?;

        let mut stack: Vec<LayerEntry> = vec![LayerEntry {
            pixmap: root,
            mask: None,
            paint: tiny_skia::PixmapPaint::default(),
        }];

        for cmd in &scene.commands {
            match cmd {
                RenderCommand::PushLayer {
                    opacity,
                    blend_mode,
                    clip,
                    transform,
                } => {
                    let layer_pixmap =
                        tiny_skia::Pixmap::new(w, h).ok_or(RenderError::PixmapCreationFailed {
                            width: w,
                            height: h,
                        })?;
                    let mask = clip.as_ref().and_then(|clip_path| {
                        let mut m = tiny_skia::Mask::new(w, h)?;
                        if let Some(skia_path) = path::bezpath_to_skia(clip_path) {
                            m.fill_path(&skia_path, tiny_skia::FillRule::Winding, true, *transform);
                        }
                        Some(m)
                    });
                    let paint = tiny_skia::PixmapPaint {
                        opacity: *opacity,
                        blend_mode: blend::to_skia_blend(*blend_mode),
                        quality: tiny_skia::FilterQuality::Nearest,
                    };
                    stack.push(LayerEntry {
                        pixmap: layer_pixmap,
                        mask,
                        paint,
                    });
                }
                RenderCommand::PopLayer => {
                    if stack.len() <= 1 {
                        continue;
                    }
                    let entry = stack.pop().unwrap();
                    let parent = stack.last_mut().unwrap();
                    parent.pixmap.draw_pixmap(
                        0,
                        0,
                        entry.pixmap.as_ref(),
                        &entry.paint,
                        tiny_skia::Transform::identity(),
                        entry.mask.as_ref(),
                    );
                }
                RenderCommand::FillPath {
                    path: bp,
                    paint: resolved_paint,
                    fill_rule,
                    transform,
                } => {
                    let current = stack.last_mut().unwrap();
                    if let Some(skia_path) = path::bezpath_to_skia(bp) {
                        let skia_fill_rule = blend::to_skia_fill_rule(*fill_rule);
                        paint::fill_with_paint(
                            &mut current.pixmap,
                            &skia_path,
                            resolved_paint,
                            skia_fill_rule,
                            *transform,
                            None,
                        );
                    }
                }
                RenderCommand::StrokePath {
                    path: bp,
                    paint: resolved_paint,
                    stroke,
                    transform,
                } => {
                    let current = stack.last_mut().unwrap();
                    if let Some(skia_path) = path::bezpath_to_skia(bp) {
                        let skia_stroke = to_skia_stroke(stroke);
                        match stroke.position {
                            ode_format::style::StrokePosition::Center => {
                                paint::stroke_with_paint(
                                    &mut current.pixmap,
                                    &skia_path,
                                    resolved_paint,
                                    &skia_stroke,
                                    *transform,
                                    None,
                                );
                            }
                            ode_format::style::StrokePosition::Inside => {
                                let mut wide_stroke = skia_stroke.clone();
                                wide_stroke.width *= 2.0;
                                let mask = build_fill_mask(&skia_path, w, h);
                                paint::stroke_with_paint(
                                    &mut current.pixmap,
                                    &skia_path,
                                    resolved_paint,
                                    &wide_stroke,
                                    *transform,
                                    mask.as_ref(),
                                );
                            }
                            ode_format::style::StrokePosition::Outside => {
                                let mut wide_stroke = skia_stroke.clone();
                                wide_stroke.width *= 2.0;
                                let mask = build_inverted_fill_mask(&skia_path, w, h);
                                paint::stroke_with_paint(
                                    &mut current.pixmap,
                                    &skia_path,
                                    resolved_paint,
                                    &wide_stroke,
                                    *transform,
                                    mask.as_ref(),
                                );
                            }
                        }
                    }
                }
                RenderCommand::DrawImage {
                    data,
                    width,
                    height,
                    transform,
                } => {
                    let current = stack.last_mut().unwrap();
                    if let Ok(dyn_img) = image::load_from_memory(data) {
                        let rgba = dyn_img.to_rgba8();
                        let (img_w, img_h) = (rgba.width(), rgba.height());
                        if let Some(src_pixmap) =
                            tiny_skia::PixmapRef::from_bytes(rgba.as_raw(), img_w, img_h)
                        {
                            // Scale from decoded size to display size
                            let sx = width / img_w as f32;
                            let sy = height / img_h as f32;
                            let scale = tiny_skia::Transform::from_scale(sx, sy);
                            let combined = transform.post_concat(scale);
                            let img_paint = tiny_skia::PixmapPaint {
                                opacity: 1.0,
                                blend_mode: tiny_skia::BlendMode::SourceOver,
                                quality: tiny_skia::FilterQuality::Bilinear,
                            };
                            current
                                .pixmap
                                .draw_pixmap(0, 0, src_pixmap, &img_paint, combined, None);
                        }
                    }
                }
                RenderCommand::ApplyEffect { effect } => match effect {
                    ResolvedEffect::DropShadow {
                        color,
                        offset_x,
                        offset_y,
                        blur_radius,
                        spread,
                        shape,
                    } => {
                        if let Some(skia_shape) = path::bezpath_to_skia(shape) {
                            if let Some(shadow) = effects::render_drop_shadow(
                                &skia_shape,
                                color,
                                *offset_x,
                                *offset_y,
                                *blur_radius,
                                *spread,
                                w,
                                h,
                            ) {
                                let current = stack.last_mut().unwrap();
                                let content = current.pixmap.clone();
                                current.pixmap.fill(tiny_skia::Color::TRANSPARENT);
                                let paint = tiny_skia::PixmapPaint {
                                    opacity: 1.0,
                                    blend_mode: tiny_skia::BlendMode::SourceOver,
                                    quality: tiny_skia::FilterQuality::Nearest,
                                };
                                current.pixmap.draw_pixmap(
                                    0,
                                    0,
                                    shadow.as_ref(),
                                    &paint,
                                    tiny_skia::Transform::identity(),
                                    None,
                                );
                                current.pixmap.draw_pixmap(
                                    0,
                                    0,
                                    content.as_ref(),
                                    &paint,
                                    tiny_skia::Transform::identity(),
                                    None,
                                );
                            }
                        }
                    }
                    ResolvedEffect::InnerShadow {
                        color,
                        offset_x,
                        offset_y,
                        blur_radius,
                        spread,
                        shape,
                    } => {
                        if let Some(skia_shape) = path::bezpath_to_skia(shape) {
                            if let Some(shadow) = effects::render_inner_shadow(
                                &skia_shape,
                                color,
                                *offset_x,
                                *offset_y,
                                *blur_radius,
                                *spread,
                                w,
                                h,
                            ) {
                                let current = stack.last_mut().unwrap();
                                let paint = tiny_skia::PixmapPaint {
                                    opacity: 1.0,
                                    blend_mode: tiny_skia::BlendMode::SourceOver,
                                    quality: tiny_skia::FilterQuality::Nearest,
                                };
                                current.pixmap.draw_pixmap(
                                    0,
                                    0,
                                    shadow.as_ref(),
                                    &paint,
                                    tiny_skia::Transform::identity(),
                                    None,
                                );
                            }
                        }
                    }
                    ResolvedEffect::LayerBlur { radius } => {
                        let current = stack.last_mut().unwrap();
                        effects::apply_layer_blur(&mut current.pixmap, *radius);
                    }
                    ResolvedEffect::BackgroundBlur { radius } => {
                        if stack.len() >= 2 {
                            let parent_idx = stack.len() - 2;
                            let rect_path = tiny_skia::PathBuilder::from_rect(
                                tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap(),
                            );
                            let blurred_bg = effects::render_background_blur(
                                &stack[parent_idx].pixmap,
                                &rect_path,
                                *radius,
                                w,
                                h,
                            );
                            if let Some(blurred_bg) = blurred_bg {
                                let current = stack.last_mut().unwrap();
                                let paint = tiny_skia::PixmapPaint {
                                    opacity: 1.0,
                                    blend_mode: tiny_skia::BlendMode::DestinationOver,
                                    quality: tiny_skia::FilterQuality::Nearest,
                                };
                                current.pixmap.draw_pixmap(
                                    0,
                                    0,
                                    blurred_bg.as_ref(),
                                    &paint,
                                    tiny_skia::Transform::identity(),
                                    None,
                                );
                            }
                        }
                    }
                },
            }
        }

        // Return the root pixmap
        if let Some(entry) = stack.into_iter().next() {
            Ok(entry.pixmap)
        } else {
            Err(RenderError::EmptyScene)
        }
    }
}

fn to_skia_stroke(style: &StrokeStyle) -> tiny_skia::Stroke {
    let mut stroke = tiny_skia::Stroke {
        width: style.width,
        line_cap: match style.cap {
            ode_format::style::StrokeCap::Butt => tiny_skia::LineCap::Butt,
            ode_format::style::StrokeCap::Round => tiny_skia::LineCap::Round,
            ode_format::style::StrokeCap::Square => tiny_skia::LineCap::Square,
        },
        line_join: match style.join {
            ode_format::style::StrokeJoin::Miter => tiny_skia::LineJoin::Miter,
            ode_format::style::StrokeJoin::Round => tiny_skia::LineJoin::Round,
            ode_format::style::StrokeJoin::Bevel => tiny_skia::LineJoin::Bevel,
        },
        miter_limit: style.miter_limit,
        dash: None,
    };
    if let Some(ref dash) = style.dash {
        stroke.dash = tiny_skia::StrokeDash::new(dash.segments.clone(), dash.offset);
    }
    stroke
}

fn build_fill_mask(path: &tiny_skia::Path, w: u32, h: u32) -> Option<tiny_skia::Mask> {
    let mut mask = tiny_skia::Mask::new(w, h)?;
    mask.fill_path(
        path,
        tiny_skia::FillRule::Winding,
        true,
        tiny_skia::Transform::identity(),
    );
    Some(mask)
}

fn build_inverted_fill_mask(path: &tiny_skia::Path, w: u32, h: u32) -> Option<tiny_skia::Mask> {
    let mut mask = tiny_skia::Mask::new(w, h)?;
    mask.fill_path(
        path,
        tiny_skia::FillRule::Winding,
        true,
        tiny_skia::Transform::identity(),
    );
    for byte in mask.data_mut() {
        *byte = 255 - *byte;
    }
    Some(mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::color::Color;
    use ode_format::node::FillRule;
    use ode_format::style::BlendMode;

    fn red_rect_scene() -> Scene {
        let mut bp = kurbo::BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((50.0, 0.0));
        bp.line_to((50.0, 50.0));
        bp.line_to((0.0, 50.0));
        bp.close_path();

        Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::FillPath {
                    path: bp,
                    paint: ResolvedPaint::Solid(Color::Srgb {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        }
    }

    #[test]
    fn render_red_rectangle() {
        let scene = red_rect_scene();
        let pixmap = Renderer::render(&scene).unwrap();
        assert_eq!(pixmap.width(), 50);
        assert_eq!(pixmap.height(), 50);
        let center = pixmap.pixel(25, 25).unwrap();
        assert_eq!(center.red(), 255);
        assert_eq!(center.green(), 0);
        assert_eq!(center.blue(), 0);
        assert_eq!(center.alpha(), 255);
    }

    #[test]
    fn render_with_opacity() {
        let mut bp = kurbo::BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((50.0, 0.0));
        bp.line_to((50.0, 50.0));
        bp.line_to((0.0, 50.0));
        bp.close_path();

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
                    path: bp,
                    paint: ResolvedPaint::Solid(Color::Srgb {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        let pixmap = Renderer::render(&scene).unwrap();
        let center = pixmap.pixel(25, 25).unwrap();
        assert!(
            center.alpha() > 100 && center.alpha() < 160,
            "Expected ~128 alpha, got {}",
            center.alpha()
        );
    }

    #[test]
    fn empty_scene_error() {
        let scene = Scene {
            width: 0.0,
            height: 0.0,
            commands: vec![],
        };
        assert!(Renderer::render(&scene).is_err());
    }
}
