/// Apply Gaussian blur to a pixmap using 3-pass box blur approximation.
/// Each pass is O(n) regardless of radius (separable horizontal + vertical).
pub fn gaussian_blur(pixmap: &mut tiny_skia::Pixmap, radius: f32) {
    if radius <= 0.0 {
        return;
    }
    let w = pixmap.width() as usize;
    let h = pixmap.height() as usize;
    if w == 0 || h == 0 {
        return;
    }

    // Box blur radius for 3-pass approximation of Gaussian
    let boxes = boxes_for_gauss(radius, 3);

    let mut src = extract_channels(pixmap);
    let mut dst = vec![vec![0.0f32; w * h]; 4];

    for &box_r in &boxes {
        box_blur_h(&src, &mut dst, w, h, box_r);
        box_blur_v(&dst, &mut src, w, h, box_r);
    }

    write_channels(pixmap, &src);
}

fn boxes_for_gauss(sigma: f32, n: usize) -> Vec<f32> {
    let w_ideal = ((12.0 * sigma * sigma / n as f32) + 1.0).sqrt();
    let mut wl = w_ideal.floor();
    if wl as i32 % 2 == 0 {
        wl -= 1.0;
    }
    if wl < 1.0 {
        wl = 1.0;
    }
    let wu = wl + 2.0;
    let m_ideal =
        (12.0 * sigma * sigma - n as f32 * wl * wl - 4.0 * n as f32 * wl - 3.0 * n as f32)
            / (-4.0 * wl - 4.0);
    let m = m_ideal.round() as usize;
    (0..n).map(|i| if i < m { wl } else { wu }).collect()
}

fn extract_channels(pixmap: &tiny_skia::Pixmap) -> Vec<Vec<f32>> {
    let pixels = pixmap.pixels();
    let len = pixels.len();
    let mut channels = vec![vec![0.0f32; len]; 4];
    for (i, px) in pixels.iter().enumerate() {
        let a = px.alpha() as f32;
        if a > 0.0 {
            channels[0][i] = px.red() as f32 * 255.0 / a;
            channels[1][i] = px.green() as f32 * 255.0 / a;
            channels[2][i] = px.blue() as f32 * 255.0 / a;
        }
        channels[3][i] = a;
    }
    channels
}

fn write_channels(pixmap: &mut tiny_skia::Pixmap, channels: &[Vec<f32>]) {
    let pixels = pixmap.pixels_mut();
    for (i, px) in pixels.iter_mut().enumerate() {
        let a = channels[3][i].clamp(0.0, 255.0) as u8;
        let r = channels[0][i].clamp(0.0, 255.0) as u8;
        let g = channels[1][i].clamp(0.0, 255.0) as u8;
        let b = channels[2][i].clamp(0.0, 255.0) as u8;
        // Channels store un-premultiplied RGB; re-premultiply before storing.
        *px = tiny_skia::ColorU8::from_rgba(r, g, b, a).premultiply();
    }
}

fn box_blur_h(src: &[Vec<f32>], dst: &mut [Vec<f32>], w: usize, h: usize, r: f32) {
    let r = (r as usize) / 2;
    if r == 0 {
        for c in 0..4 {
            dst[c].copy_from_slice(&src[c]);
        }
        return;
    }
    let iarr = 1.0 / (2 * r + 1) as f32;
    for c in 0..4 {
        for y in 0..h {
            let row = y * w;
            let mut val = src[c][row] * (r + 1) as f32;
            for i in 0..r {
                val += src[c][row + i.min(w - 1)];
            }
            for _ in 0..r {
                val += src[c][row];
            }

            let mut ri = r;
            for (li, x) in (0..w).enumerate() {
                dst[c][row + x] = val * iarr;
                let right = (ri + 1).min(w - 1);
                let left = if li > 0 { li - 1 } else { 0 };
                val += src[c][row + right] - src[c][row + left];
                ri += 1;
            }
        }
    }
}

fn box_blur_v(src: &[Vec<f32>], dst: &mut [Vec<f32>], w: usize, h: usize, r: f32) {
    let r = (r as usize) / 2;
    if r == 0 {
        for c in 0..4 {
            dst[c].copy_from_slice(&src[c]);
        }
        return;
    }
    let iarr = 1.0 / (2 * r + 1) as f32;
    for c in 0..4 {
        for x in 0..w {
            let mut val = src[c][x] * (r + 1) as f32;
            for i in 0..r {
                val += src[c][i.min(h - 1) * w + x];
            }
            for _ in 0..r {
                val += src[c][x];
            }

            let mut ri = r;
            for (li, y) in (0..h).enumerate() {
                dst[c][y * w + x] = val * iarr;
                let bottom = ((ri + 1).min(h - 1)) * w + x;
                let top = (if li > 0 { li - 1 } else { 0 }) * w + x;
                val += src[c][bottom] - src[c][top];
                ri += 1;
            }
        }
    }
}

/// Scale a path outward (positive spread) or inward (negative spread).
/// Uses scale-around-center as an approximation of true path offsetting.
fn apply_spread(path: &tiny_skia::Path, spread: f32) -> Option<tiny_skia::Path> {
    if spread.abs() < f32::EPSILON {
        return None;
    }
    let bounds = path.bounds();
    let cx = (bounds.left() + bounds.right()) / 2.0;
    let cy = (bounds.top() + bounds.bottom()) / 2.0;
    let w = bounds.width();
    let h = bounds.height();
    if w < f32::EPSILON || h < f32::EPSILON {
        return None;
    }
    let sx = ((w + 2.0 * spread) / w).max(f32::EPSILON);
    let sy = ((h + 2.0 * spread) / h).max(f32::EPSILON);
    let t1 = tiny_skia::Transform::from_translate(-cx, -cy);
    let s = tiny_skia::Transform::from_scale(sx, sy);
    let t2 = tiny_skia::Transform::from_translate(cx, cy);
    let transform = t1.post_concat(s).post_concat(t2);
    path.clone().transform(transform)
}

/// Render a drop shadow effect. Returns a pixmap with the shadow to composite UNDER content.
#[allow(clippy::too_many_arguments)]
pub fn render_drop_shadow(
    content_path: &tiny_skia::Path,
    color: &ode_format::color::Color,
    offset_x: f32,
    offset_y: f32,
    blur_radius: f32,
    spread: f32,
    width: u32,
    height: u32,
) -> Option<tiny_skia::Pixmap> {
    let spread_path = apply_spread(content_path, spread);
    let path = spread_path.as_ref().unwrap_or(content_path);
    let mut shadow = tiny_skia::Pixmap::new(width, height)?;
    let mut paint = tiny_skia::Paint::default();
    paint.shader = tiny_skia::Shader::SolidColor(crate::paint::color_to_skia(color));
    paint.anti_alias = true;
    let transform = tiny_skia::Transform::from_translate(offset_x, offset_y);
    shadow.fill_path(path, &paint, tiny_skia::FillRule::Winding, transform, None);
    if blur_radius > 0.0 {
        gaussian_blur(&mut shadow, blur_radius);
    }
    Some(shadow)
}

/// Render an inner shadow effect. Returns a pixmap to composite OVER content.
/// Spread contracts the cutout shape (making the shadow thicker around edges).
#[allow(clippy::too_many_arguments)]
pub fn render_inner_shadow(
    content_path: &tiny_skia::Path,
    color: &ode_format::color::Color,
    offset_x: f32,
    offset_y: f32,
    blur_radius: f32,
    spread: f32,
    width: u32,
    height: u32,
) -> Option<tiny_skia::Pixmap> {
    let cutout_path = apply_spread(content_path, -spread);
    let cutout = cutout_path.as_ref().unwrap_or(content_path);
    let mut shadow = tiny_skia::Pixmap::new(width, height)?;
    shadow.fill(crate::paint::color_to_skia(color));
    let cutout_paint = tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::TRANSPARENT),
        blend_mode: tiny_skia::BlendMode::Source,
        anti_alias: true,
        ..Default::default()
    };
    let transform = tiny_skia::Transform::from_translate(offset_x, offset_y);
    shadow.fill_path(
        cutout,
        &cutout_paint,
        tiny_skia::FillRule::Winding,
        transform,
        None,
    );
    if blur_radius > 0.0 {
        gaussian_blur(&mut shadow, blur_radius);
    }
    if let Some(mut mask) = tiny_skia::Mask::new(width, height) {
        mask.fill_path(
            content_path,
            tiny_skia::FillRule::Winding,
            true,
            tiny_skia::Transform::identity(),
        );
        let mut clipped = tiny_skia::Pixmap::new(width, height)?;
        let paint = tiny_skia::PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: tiny_skia::FilterQuality::Nearest,
        };
        clipped.draw_pixmap(
            0,
            0,
            shadow.as_ref(),
            &paint,
            tiny_skia::Transform::identity(),
            Some(&mask),
        );
        return Some(clipped);
    }
    Some(shadow)
}

/// Apply layer blur: blur the given pixmap in-place.
pub fn apply_layer_blur(pixmap: &mut tiny_skia::Pixmap, radius: f32) {
    gaussian_blur(pixmap, radius);
}

/// Apply background blur: blur a region of the background pixmap.
pub fn render_background_blur(
    background: &tiny_skia::Pixmap,
    content_path: &tiny_skia::Path,
    radius: f32,
    width: u32,
    height: u32,
) -> Option<tiny_skia::Pixmap> {
    let mut blurred = background.clone();
    gaussian_blur(&mut blurred, radius);
    if let Some(mut mask) = tiny_skia::Mask::new(width, height) {
        mask.fill_path(
            content_path,
            tiny_skia::FillRule::Winding,
            true,
            tiny_skia::Transform::identity(),
        );
        let mut result = tiny_skia::Pixmap::new(width, height)?;
        let paint = tiny_skia::PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: tiny_skia::FilterQuality::Nearest,
        };
        result.draw_pixmap(
            0,
            0,
            blurred.as_ref(),
            &paint,
            tiny_skia::Transform::identity(),
            Some(&mask),
        );
        return Some(result);
    }
    Some(blurred)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_blur_single_pass_averages() {
        let mut pixmap = tiny_skia::Pixmap::new(5, 1).unwrap();
        pixmap.pixels_mut()[2] = tiny_skia::ColorU8::from_rgba(255, 255, 255, 255).premultiply();
        gaussian_blur(&mut pixmap, 1.0);
        let center = pixmap.pixel(2, 0).unwrap();
        let left = pixmap.pixel(1, 0).unwrap();
        assert!(center.alpha() > 0);
        assert!(left.alpha() > 0, "Blur should spread to neighbors");
    }

    #[test]
    fn gaussian_blur_zero_radius_is_noop() {
        let mut pixmap = tiny_skia::Pixmap::new(10, 10).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(128, 64, 32, 255));
        let original = pixmap.pixel(5, 5).unwrap();
        gaussian_blur(&mut pixmap, 0.0);
        let after = pixmap.pixel(5, 5).unwrap();
        assert_eq!(original, after);
    }
}
