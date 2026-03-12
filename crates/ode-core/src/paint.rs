use ode_format::color::Color;
use crate::scene::{ResolvedPaint, ResolvedGradientStop};

/// Convert ODE Color to tiny-skia Color.
pub fn color_to_skia(color: &Color) -> tiny_skia::Color {
    let [r, g, b, a] = color.to_rgba_u8();
    tiny_skia::Color::from_rgba8(r, g, b, a)
}

/// Fill a path with a ResolvedPaint. Handles all paint types including custom gradients.
pub fn fill_with_paint(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    paint: &ResolvedPaint,
    fill_rule: tiny_skia::FillRule,
    transform: tiny_skia::Transform,
    mask: Option<&tiny_skia::Mask>,
) {
    match paint {
        ResolvedPaint::Solid(color) => {
            let mut p = tiny_skia::Paint::default();
            p.shader = tiny_skia::Shader::SolidColor(color_to_skia(color));
            p.anti_alias = true;
            pixmap.fill_path(path, &p, fill_rule, transform, mask);
        }
        ResolvedPaint::LinearGradient { stops, start, end } => {
            if let Some(shader) = make_linear_gradient(stops, *start, *end) {
                let mut p = tiny_skia::Paint::default();
                p.shader = shader;
                p.anti_alias = true;
                pixmap.fill_path(path, &p, fill_rule, transform, mask);
            }
        }
        ResolvedPaint::RadialGradient { stops, center, radius } => {
            if let Some(shader) = make_radial_gradient(stops, *center, *radius) {
                let mut p = tiny_skia::Paint::default();
                p.shader = shader;
                p.anti_alias = true;
                pixmap.fill_path(path, &p, fill_rule, transform, mask);
            }
        }
        ResolvedPaint::AngularGradient { stops, center, angle } => {
            fill_angular_gradient(pixmap, path, stops, *center, *angle, fill_rule, transform, mask);
        }
        ResolvedPaint::DiamondGradient { stops, center, radius } => {
            fill_diamond_gradient(pixmap, path, stops, *center, *radius, fill_rule, transform, mask);
        }
    }
}

/// Stroke a path with a ResolvedPaint.
pub fn stroke_with_paint(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    paint: &ResolvedPaint,
    stroke: &tiny_skia::Stroke,
    transform: tiny_skia::Transform,
    mask: Option<&tiny_skia::Mask>,
) {
    let skia_paint = match paint {
        ResolvedPaint::Solid(color) => {
            let mut p = tiny_skia::Paint::default();
            p.shader = tiny_skia::Shader::SolidColor(color_to_skia(color));
            p.anti_alias = true;
            p
        }
        ResolvedPaint::LinearGradient { stops, start, end } => {
            let mut p = tiny_skia::Paint::default();
            if let Some(shader) = make_linear_gradient(stops, *start, *end) {
                p.shader = shader;
            }
            p.anti_alias = true;
            p
        }
        ResolvedPaint::RadialGradient { stops, center, radius } => {
            let mut p = tiny_skia::Paint::default();
            if let Some(shader) = make_radial_gradient(stops, *center, *radius) {
                p.shader = shader;
            }
            p.anti_alias = true;
            p
        }
        _ => {
            // For angular/diamond gradient strokes, fall back to first stop color
            let color = match paint {
                ResolvedPaint::AngularGradient { stops, .. }
                | ResolvedPaint::DiamondGradient { stops, .. } => {
                    stops.first().map(|s| color_to_skia(&s.color))
                        .unwrap_or(tiny_skia::Color::BLACK)
                }
                _ => tiny_skia::Color::BLACK,
            };
            let mut p = tiny_skia::Paint::default();
            p.shader = tiny_skia::Shader::SolidColor(color);
            p.anti_alias = true;
            p
        }
    };
    pixmap.stroke_path(path, &skia_paint, stroke, transform, mask);
}

fn gradient_stops_to_skia(stops: &[ResolvedGradientStop]) -> Vec<tiny_skia::GradientStop> {
    stops.iter().map(|s| {
        tiny_skia::GradientStop::new(s.position, color_to_skia(&s.color))
    }).collect()
}

fn make_linear_gradient(
    stops: &[ResolvedGradientStop],
    start: kurbo::Point,
    end: kurbo::Point,
) -> Option<tiny_skia::Shader<'static>> {
    tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(start.x as f32, start.y as f32),
        tiny_skia::Point::from_xy(end.x as f32, end.y as f32),
        gradient_stops_to_skia(stops),
        tiny_skia::SpreadMode::Pad,
        tiny_skia::Transform::identity(),
    )
}

fn make_radial_gradient(
    stops: &[ResolvedGradientStop],
    center: kurbo::Point,
    radius: kurbo::Point,
) -> Option<tiny_skia::Shader<'static>> {
    let rx = radius.x as f32;
    let ry = radius.y as f32;
    let transform = if (rx - ry).abs() > f32::EPSILON && rx > 0.0 {
        tiny_skia::Transform::from_scale(1.0, ry / rx)
    } else {
        tiny_skia::Transform::identity()
    };
    tiny_skia::RadialGradient::new(
        tiny_skia::Point::from_xy(center.x as f32, center.y as f32),
        tiny_skia::Point::from_xy(center.x as f32, center.y as f32),
        rx,
        gradient_stops_to_skia(stops),
        tiny_skia::SpreadMode::Pad,
        transform,
    )
}

/// Sample a gradient color at a given position (0.0 to 1.0) from sorted stops.
fn sample_gradient(stops: &[ResolvedGradientStop], t: f32) -> tiny_skia::Color {
    if stops.is_empty() { return tiny_skia::Color::TRANSPARENT; }
    if t <= stops[0].position { return color_to_skia(&stops[0].color); }
    if t >= stops[stops.len() - 1].position { return color_to_skia(&stops[stops.len() - 1].color); }
    for i in 0..stops.len() - 1 {
        if t >= stops[i].position && t <= stops[i + 1].position {
            let range = stops[i + 1].position - stops[i].position;
            let frac = if range > 0.0 { (t - stops[i].position) / range } else { 0.0 };
            let c0 = color_to_skia(&stops[i].color);
            let c1 = color_to_skia(&stops[i + 1].color);
            return tiny_skia::Color::from_rgba(
                c0.red() + (c1.red() - c0.red()) * frac,
                c0.green() + (c1.green() - c0.green()) * frac,
                c0.blue() + (c1.blue() - c0.blue()) * frac,
                c0.alpha() + (c1.alpha() - c0.alpha()) * frac,
            ).unwrap_or(tiny_skia::Color::BLACK);
        }
    }
    color_to_skia(&stops[stops.len() - 1].color)
}

/// Manual angular gradient: for each pixel compute angle from center, sample gradient.
fn fill_angular_gradient(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    stops: &[ResolvedGradientStop],
    center: kurbo::Point,
    angle: f32,
    fill_rule: tiny_skia::FillRule,
    transform: tiny_skia::Transform,
    _mask: Option<&tiny_skia::Mask>,
) {
    let w = pixmap.width();
    let h = pixmap.height();
    if let Some(mut grad_pixmap) = tiny_skia::Pixmap::new(w, h) {
        let cx = center.x as f32;
        let cy = center.y as f32;
        let angle_offset = angle.to_radians();
        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let mut a = dy.atan2(dx) - angle_offset;
                if a < 0.0 { a += std::f32::consts::TAU; }
                let t = a / std::f32::consts::TAU;
                let color = sample_gradient(stops, t);
                let pm = color.premultiply().to_color_u8();
                grad_pixmap.pixels_mut()[(y * w + x) as usize] = pm;
            }
        }
        if let Some(mut clip_mask) = tiny_skia::Mask::new(w, h) {
            clip_mask.fill_path(path, fill_rule, true, transform);
            let paint = tiny_skia::PixmapPaint {
                opacity: 1.0,
                blend_mode: tiny_skia::BlendMode::SourceOver,
                quality: tiny_skia::FilterQuality::Nearest,
            };
            pixmap.draw_pixmap(0, 0, grad_pixmap.as_ref(), &paint, tiny_skia::Transform::identity(), Some(&clip_mask));
        }
    }
}

/// Manual diamond gradient: Manhattan distance-based color sampling.
fn fill_diamond_gradient(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    stops: &[ResolvedGradientStop],
    center: kurbo::Point,
    radius: kurbo::Point,
    fill_rule: tiny_skia::FillRule,
    transform: tiny_skia::Transform,
    _mask: Option<&tiny_skia::Mask>,
) {
    let w = pixmap.width();
    let h = pixmap.height();
    if let Some(mut grad_pixmap) = tiny_skia::Pixmap::new(w, h) {
        let cx = center.x as f32;
        let cy = center.y as f32;
        let rx = radius.x as f32;
        let ry = radius.y as f32;
        for y in 0..h {
            for x in 0..w {
                let dx = ((x as f32 - cx) / rx).abs();
                let dy = ((y as f32 - cy) / ry).abs();
                let t = (dx + dy).min(1.0);
                let color = sample_gradient(stops, t);
                let pm = color.premultiply().to_color_u8();
                grad_pixmap.pixels_mut()[(y * w + x) as usize] = pm;
            }
        }
        if let Some(mut clip_mask) = tiny_skia::Mask::new(w, h) {
            clip_mask.fill_path(path, fill_rule, true, transform);
            let paint = tiny_skia::PixmapPaint {
                opacity: 1.0,
                blend_mode: tiny_skia::BlendMode::SourceOver,
                quality: tiny_skia::FilterQuality::Nearest,
            };
            pixmap.draw_pixmap(0, 0, grad_pixmap.as_ref(), &paint, tiny_skia::Transform::identity(), Some(&clip_mask));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::color::Color;

    #[test]
    fn color_to_skia_black() {
        let c = color_to_skia(&Color::black());
        assert!((c.red() - 0.0).abs() < f32::EPSILON);
        assert!((c.green() - 0.0).abs() < f32::EPSILON);
        assert!((c.blue() - 0.0).abs() < f32::EPSILON);
        assert!((c.alpha() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn color_to_skia_white() {
        let c = color_to_skia(&Color::white());
        assert!((c.red() - 1.0).abs() < f32::EPSILON);
        assert!((c.green() - 1.0).abs() < f32::EPSILON);
        assert!((c.blue() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn solid_paint_fills_pixel() {
        let mut pixmap = tiny_skia::Pixmap::new(10, 10).unwrap();
        let paint = ResolvedPaint::Solid(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 });
        let mut bp = kurbo::BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((10.0, 0.0));
        bp.line_to((10.0, 10.0));
        bp.line_to((0.0, 10.0));
        bp.close_path();
        let skia_path = crate::path::bezpath_to_skia(&bp).unwrap();
        fill_with_paint(
            &mut pixmap, &skia_path, &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(), None,
        );
        let pixel = pixmap.pixel(5, 5).unwrap();
        assert_eq!(pixel.red(), 255);
        assert_eq!(pixel.green(), 0);
        assert_eq!(pixel.blue(), 0);
    }

    #[test]
    fn linear_gradient_paint_fills() {
        let mut pixmap = tiny_skia::Pixmap::new(100, 10).unwrap();
        let paint = ResolvedPaint::LinearGradient {
            stops: vec![
                ResolvedGradientStop { position: 0.0, color: Color::black() },
                ResolvedGradientStop { position: 1.0, color: Color::white() },
            ],
            start: kurbo::Point::new(0.0, 0.0),
            end: kurbo::Point::new(100.0, 0.0),
        };
        let mut bp = kurbo::BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((100.0, 0.0));
        bp.line_to((100.0, 10.0));
        bp.line_to((0.0, 10.0));
        bp.close_path();
        let skia_path = crate::path::bezpath_to_skia(&bp).unwrap();
        fill_with_paint(
            &mut pixmap, &skia_path, &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(), None,
        );
        let left = pixmap.pixel(5, 5).unwrap();
        let right = pixmap.pixel(95, 5).unwrap();
        assert!(left.red() < 50, "Left should be dark, got {}", left.red());
        assert!(right.red() > 200, "Right should be light, got {}", right.red());
    }
}
