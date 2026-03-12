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
            } else {
                eprintln!("[ode-core] LinearGradient creation failed ({} stops, start={:?}, end={:?}). Falling back to first stop color.", stops.len(), start, end);
                if let Some(first) = stops.first() {
                    let mut p = tiny_skia::Paint::default();
                    p.shader = tiny_skia::Shader::SolidColor(color_to_skia(&first.color));
                    p.anti_alias = true;
                    pixmap.fill_path(path, &p, fill_rule, transform, mask);
                }
            }
        }
        ResolvedPaint::RadialGradient { stops, center, radius } => {
            if let Some(shader) = make_radial_gradient(stops, *center, *radius) {
                let mut p = tiny_skia::Paint::default();
                p.shader = shader;
                p.anti_alias = true;
                pixmap.fill_path(path, &p, fill_rule, transform, mask);
            } else {
                eprintln!("[ode-core] RadialGradient creation failed ({} stops, center={:?}, radius={:?}). Falling back to first stop color.", stops.len(), center, radius);
                if let Some(first) = stops.first() {
                    let mut p = tiny_skia::Paint::default();
                    p.shader = tiny_skia::Shader::SolidColor(color_to_skia(&first.color));
                    p.anti_alias = true;
                    pixmap.fill_path(path, &p, fill_rule, transform, mask);
                }
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
    match paint {
        ResolvedPaint::Solid(color) => {
            let mut p = tiny_skia::Paint::default();
            p.shader = tiny_skia::Shader::SolidColor(color_to_skia(color));
            p.anti_alias = true;
            pixmap.stroke_path(path, &p, stroke, transform, mask);
        }
        ResolvedPaint::LinearGradient { stops, start, end } => {
            let mut p = tiny_skia::Paint::default();
            if let Some(shader) = make_linear_gradient(stops, *start, *end) {
                p.shader = shader;
            } else {
                eprintln!("[ode-core] LinearGradient creation failed for stroke ({} stops). Using first stop color.", stops.len());
                let fallback = stops.first().map(|s| color_to_skia(&s.color)).unwrap_or(tiny_skia::Color::BLACK);
                p.shader = tiny_skia::Shader::SolidColor(fallback);
            }
            p.anti_alias = true;
            pixmap.stroke_path(path, &p, stroke, transform, mask);
        }
        ResolvedPaint::RadialGradient { stops, center, radius } => {
            let mut p = tiny_skia::Paint::default();
            if let Some(shader) = make_radial_gradient(stops, *center, *radius) {
                p.shader = shader;
            } else {
                eprintln!("[ode-core] RadialGradient creation failed for stroke ({} stops). Using first stop color.", stops.len());
                let fallback = stops.first().map(|s| color_to_skia(&s.color)).unwrap_or(tiny_skia::Color::BLACK);
                p.shader = tiny_skia::Shader::SolidColor(fallback);
            }
            p.anti_alias = true;
            pixmap.stroke_path(path, &p, stroke, transform, mask);
        }
        ResolvedPaint::AngularGradient { stops, center, angle } => {
            let w = pixmap.width();
            let h = pixmap.height();
            if let Some(grad_pm) = generate_angular_gradient_pixmap(w, h, stops, *center, *angle) {
                stroke_with_gradient_pixmap(pixmap, &grad_pm, path, stroke, transform, mask);
            }
        }
        ResolvedPaint::DiamondGradient { stops, center, radius } => {
            let w = pixmap.width();
            let h = pixmap.height();
            if let Some(grad_pm) = generate_diamond_gradient_pixmap(w, h, stops, *center, *radius) {
                stroke_with_gradient_pixmap(pixmap, &grad_pm, path, stroke, transform, mask);
            }
        }
    }
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

/// Generate an angular gradient pixmap (unclipped, full canvas).
fn generate_angular_gradient_pixmap(
    w: u32,
    h: u32,
    stops: &[ResolvedGradientStop],
    center: kurbo::Point,
    angle: f32,
) -> Option<tiny_skia::Pixmap> {
    let mut grad_pixmap = tiny_skia::Pixmap::new(w, h)?;
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
    Some(grad_pixmap)
}

/// Generate a diamond gradient pixmap (unclipped, full canvas).
fn generate_diamond_gradient_pixmap(
    w: u32,
    h: u32,
    stops: &[ResolvedGradientStop],
    center: kurbo::Point,
    radius: kurbo::Point,
) -> Option<tiny_skia::Pixmap> {
    let mut grad_pixmap = tiny_skia::Pixmap::new(w, h)?;
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
    Some(grad_pixmap)
}

/// Fill a path with an angular gradient using a pre-generated pixmap + clip mask.
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
    if let Some(grad_pixmap) = generate_angular_gradient_pixmap(w, h, stops, center, angle) {
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

/// Fill a path with a diamond gradient using a pre-generated pixmap + clip mask.
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
    if let Some(grad_pixmap) = generate_diamond_gradient_pixmap(w, h, stops, center, radius) {
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

/// Stroke a path using a pre-generated gradient pixmap.
/// Renders the gradient to a temp surface, strokes the path to another,
/// then uses DestinationIn compositing to mask the gradient with the stroke shape.
fn stroke_with_gradient_pixmap(
    pixmap: &mut tiny_skia::Pixmap,
    grad_pixmap: &tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    stroke: &tiny_skia::Stroke,
    transform: tiny_skia::Transform,
    mask: Option<&tiny_skia::Mask>,
) {
    let w = pixmap.width();
    let h = pixmap.height();
    // Stroke path with solid white to get the stroke shape
    let Some(mut stroke_pm) = tiny_skia::Pixmap::new(w, h) else { return };
    let mut white = tiny_skia::Paint::default();
    white.shader = tiny_skia::Shader::SolidColor(tiny_skia::Color::WHITE);
    white.anti_alias = true;
    stroke_pm.stroke_path(path, &white, stroke, transform, None);
    // Mask gradient with stroke shape: keep gradient only where stroke exists
    let mut masked = grad_pixmap.clone();
    let mask_paint = tiny_skia::PixmapPaint {
        blend_mode: tiny_skia::BlendMode::DestinationIn,
        opacity: 1.0,
        quality: tiny_skia::FilterQuality::Nearest,
    };
    masked.draw_pixmap(0, 0, stroke_pm.as_ref(), &mask_paint, tiny_skia::Transform::identity(), None);
    // Composite onto target
    let paint = tiny_skia::PixmapPaint {
        blend_mode: tiny_skia::BlendMode::SourceOver,
        opacity: 1.0,
        quality: tiny_skia::FilterQuality::Nearest,
    };
    pixmap.draw_pixmap(0, 0, masked.as_ref(), &paint, tiny_skia::Transform::identity(), mask);
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
