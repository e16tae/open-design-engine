//! Preset shape path generators for `ode add vector --shape <preset>`.

use crate::node::{PathSegment, VectorPath};

/// Bezier approximation constant for a 90° arc (kappa).
const KAPPA: f32 = 0.552_284_75;

/// Generate a rectangle path.
///
/// Produces 4 segments: MoveTo(0,0), LineTo(w,0), LineTo(w,h), LineTo(0,h). closed=true.
pub fn rect(width: f32, height: f32) -> VectorPath {
    VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: width, y: 0.0 },
            PathSegment::LineTo { x: width, y: height },
            PathSegment::LineTo { x: 0.0, y: height },
        ],
        closed: true,
    }
}

/// Generate a rounded rectangle path.
///
/// `radii` is [top-left, top-right, bottom-right, bottom-left].
/// If all radii are zero, delegates to [`rect`].
/// Each radius is clamped to `min(width/2, height/2)`.
/// Corners are approximated with cubic Bézier curves using kappa = 0.552284_75.
/// Traces clockwise starting from the top-left corner's right endpoint.
pub fn rounded_rect(width: f32, height: f32, radii: [f32; 4]) -> VectorPath {
    if radii.iter().all(|&r| r == 0.0) {
        return rect(width, height);
    }

    let max_r = (width / 2.0).min(height / 2.0);
    let [tl, tr, br, bl] = radii.map(|r| r.min(max_r).max(0.0));

    let k = KAPPA;
    let mut segs = Vec::with_capacity(9);

    // Start at top edge, after top-left corner
    segs.push(PathSegment::MoveTo { x: tl, y: 0.0 });

    // Top edge → top-right corner
    segs.push(PathSegment::LineTo { x: width - tr, y: 0.0 });
    if tr > 0.0 {
        segs.push(PathSegment::CurveTo {
            x1: width - tr + tr * k,
            y1: 0.0,
            x2: width,
            y2: tr - tr * k,
            x: width,
            y: tr,
        });
    }

    // Right edge → bottom-right corner
    segs.push(PathSegment::LineTo { x: width, y: height - br });
    if br > 0.0 {
        segs.push(PathSegment::CurveTo {
            x1: width,
            y1: height - br + br * k,
            x2: width - br + br * k,
            y2: height,
            x: width - br,
            y: height,
        });
    }

    // Bottom edge → bottom-left corner
    segs.push(PathSegment::LineTo { x: bl, y: height });
    if bl > 0.0 {
        segs.push(PathSegment::CurveTo {
            x1: bl - bl * k,
            y1: height,
            x2: 0.0,
            y2: height - bl + bl * k,
            x: 0.0,
            y: height - bl,
        });
    }

    // Left edge → top-left corner
    segs.push(PathSegment::LineTo { x: 0.0, y: tl });
    if tl > 0.0 {
        segs.push(PathSegment::CurveTo {
            x1: 0.0,
            y1: tl - tl * k,
            x2: tl - tl * k,
            y2: 0.0,
            x: tl,
            y: 0.0,
        });
    }

    VectorPath {
        segments: segs,
        closed: true,
    }
}

/// Generate an ellipse path approximated with 4 cubic Bézier curves.
///
/// Center is at `(rx, ry)`. Produces 5 segments: 1 MoveTo + 4 CurveTo. closed=true.
pub fn ellipse(width: f32, height: f32) -> VectorPath {
    let rx = width / 2.0;
    let ry = height / 2.0;
    let cx = rx;
    let cy = ry;
    let kx = rx * KAPPA;
    let ky = ry * KAPPA;

    VectorPath {
        segments: vec![
            // Start at rightmost point
            PathSegment::MoveTo { x: cx + rx, y: cy },
            // Top-right quadrant
            PathSegment::CurveTo {
                x1: cx + rx,
                y1: cy - ky,
                x2: cx + kx,
                y2: cy - ry,
                x: cx,
                y: cy - ry,
            },
            // Top-left quadrant
            PathSegment::CurveTo {
                x1: cx - kx,
                y1: cy - ry,
                x2: cx - rx,
                y2: cy - ky,
                x: cx - rx,
                y: cy,
            },
            // Bottom-left quadrant
            PathSegment::CurveTo {
                x1: cx - rx,
                y1: cy + ky,
                x2: cx - kx,
                y2: cy + ry,
                x: cx,
                y: cy + ry,
            },
            // Bottom-right quadrant
            PathSegment::CurveTo {
                x1: cx + kx,
                y1: cy + ry,
                x2: cx + rx,
                y2: cy + ky,
                x: cx + rx,
                y: cy,
            },
        ],
        closed: true,
    }
}

/// Generate a horizontal line.
///
/// Produces 2 segments: MoveTo(0,0), LineTo(w,0). closed=false.
pub fn line(width: f32) -> VectorPath {
    VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: width, y: 0.0 },
        ],
        closed: false,
    }
}

/// Generate a 5-pointed star inscribed in a circle of given diameter.
///
/// Uses a golden-ratio inner radius (`r_inner = r_outer * 0.381966`).
/// Starts from the top point (angle -PI/2), traces clockwise.
/// Produces 10 segments: 1 MoveTo + 9 LineTo. closed=true.
pub fn star(diameter: f32) -> VectorPath {
    let r_outer = diameter / 2.0;
    let r_inner = r_outer * 0.381_966;
    let cx = r_outer;
    let cy = r_outer;

    let mut segs = Vec::with_capacity(10);
    let start_angle = -std::f32::consts::FRAC_PI_2;
    let step = std::f32::consts::PI / 5.0; // 36 degrees per vertex

    for i in 0..10 {
        let angle = start_angle + i as f32 * step;
        let r = if i % 2 == 0 { r_outer } else { r_inner };
        let x = cx + r * angle.cos();
        let y = cy + r * angle.sin();
        if i == 0 {
            segs.push(PathSegment::MoveTo { x, y });
        } else {
            segs.push(PathSegment::LineTo { x, y });
        }
    }

    VectorPath {
        segments: segs,
        closed: true,
    }
}

/// Generate a regular polygon with N sides inscribed in a circle of given diameter.
///
/// `sides` is clamped to a minimum of 3.
/// Starts from the top vertex (angle -PI/2), traces clockwise.
/// Produces N segments: 1 MoveTo + (N-1) LineTo. closed=true.
pub fn polygon(sides: u32, diameter: f32) -> VectorPath {
    let sides = sides.max(3);
    let r = diameter / 2.0;
    let cx = r;
    let cy = r;

    let mut segs = Vec::with_capacity(sides as usize);
    let start_angle = -std::f32::consts::FRAC_PI_2;
    let step = 2.0 * std::f32::consts::PI / sides as f32;

    for i in 0..sides {
        let angle = start_angle + i as f32 * step;
        let x = cx + r * angle.cos();
        let y = cy + r * angle.sin();
        if i == 0 {
            segs.push(PathSegment::MoveTo { x, y });
        } else {
            segs.push(PathSegment::LineTo { x, y });
        }
    }

    VectorPath {
        segments: segs,
        closed: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_produces_4_lines_closed() {
        let path = rect(100.0, 50.0);
        assert!(path.closed);
        assert_eq!(path.segments.len(), 4);
        assert!(matches!(path.segments[0], PathSegment::MoveTo { x, y } if x == 0.0 && y == 0.0));
    }

    #[test]
    fn ellipse_produces_curves() {
        let path = ellipse(100.0, 80.0);
        assert!(path.closed);
        let curve_count = path
            .segments
            .iter()
            .filter(|s| matches!(s, PathSegment::CurveTo { .. }))
            .count();
        assert_eq!(curve_count, 4);
    }

    #[test]
    fn line_is_not_closed() {
        let path = line(200.0);
        assert!(!path.closed);
        assert_eq!(path.segments.len(), 2);
    }

    #[test]
    fn star_has_10_points() {
        let path = star(100.0);
        assert!(path.closed);
        assert_eq!(path.segments.len(), 10);
    }

    #[test]
    fn polygon_hexagon() {
        let path = polygon(6, 100.0);
        assert!(path.closed);
        assert_eq!(path.segments.len(), 6);
    }

    #[test]
    fn rounded_rect_with_zero_radii_equals_rect() {
        let r = rounded_rect(100.0, 50.0, [0.0; 4]);
        let plain = rect(100.0, 50.0);
        assert_eq!(r.segments.len(), plain.segments.len());
    }

    #[test]
    fn rounded_rect_with_radii_has_curves() {
        let path = rounded_rect(100.0, 50.0, [8.0, 8.0, 8.0, 8.0]);
        let curve_count = path
            .segments
            .iter()
            .filter(|s| matches!(s, PathSegment::CurveTo { .. }))
            .count();
        assert_eq!(curve_count, 4);
    }
}
