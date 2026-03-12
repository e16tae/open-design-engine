use kurbo::{BezPath, PathEl, RoundedRect, RoundedRectRadii, Rect, Shape};
use ode_format::node::{VectorPath, PathSegment};

/// Convert serializable VectorPath to kurbo BezPath for rendering.
pub fn to_bezpath(path: &VectorPath) -> BezPath {
    let mut bp = BezPath::new();
    for seg in &path.segments {
        match *seg {
            PathSegment::MoveTo { x, y } => bp.move_to((x as f64, y as f64)),
            PathSegment::LineTo { x, y } => bp.line_to((x as f64, y as f64)),
            PathSegment::QuadTo { x1, y1, x, y } =>
                bp.quad_to((x1 as f64, y1 as f64), (x as f64, y as f64)),
            PathSegment::CurveTo { x1, y1, x2, y2, x, y } =>
                bp.curve_to((x1 as f64, y1 as f64), (x2 as f64, y2 as f64), (x as f64, y as f64)),
            PathSegment::Close => bp.close_path(),
        }
    }
    // If path is marked closed but doesn't end with Close segment, close it
    if path.closed && !path.segments.last().is_some_and(|s| matches!(s, PathSegment::Close)) {
        bp.close_path();
    }
    bp
}

/// Convert kurbo BezPath back to serializable VectorPath.
pub fn from_bezpath(bp: &BezPath) -> VectorPath {
    let mut segments = Vec::new();
    let mut closed = false;
    for el in bp.elements() {
        match *el {
            PathEl::MoveTo(p) => segments.push(PathSegment::MoveTo { x: p.x as f32, y: p.y as f32 }),
            PathEl::LineTo(p) => segments.push(PathSegment::LineTo { x: p.x as f32, y: p.y as f32 }),
            PathEl::QuadTo(p1, p2) => segments.push(PathSegment::QuadTo {
                x1: p1.x as f32, y1: p1.y as f32, x: p2.x as f32, y: p2.y as f32,
            }),
            PathEl::CurveTo(p1, p2, p3) => segments.push(PathSegment::CurveTo {
                x1: p1.x as f32, y1: p1.y as f32,
                x2: p2.x as f32, y2: p2.y as f32,
                x: p3.x as f32, y: p3.y as f32,
            }),
            PathEl::ClosePath => {
                segments.push(PathSegment::Close);
                closed = true;
            }
        }
    }
    VectorPath { segments, closed }
}

/// Generate a rounded rectangle path.
pub fn rounded_rect_path(width: f32, height: f32, radii: [f32; 4]) -> BezPath {
    let rect = Rect::new(0.0, 0.0, width as f64, height as f64);
    let rr = RoundedRect::from_rect(
        rect,
        RoundedRectRadii::new(
            radii[0] as f64, radii[1] as f64,
            radii[2] as f64, radii[3] as f64,
        ),
    );
    rr.to_path(0.1)
}

/// Convert kurbo BezPath to tiny_skia Path.
pub fn bezpath_to_skia(bp: &BezPath) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for el in bp.elements() {
        match *el {
            PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(p1, p2) => pb.quad_to(
                p1.x as f32, p1.y as f32,
                p2.x as f32, p2.y as f32,
            ),
            PathEl::CurveTo(p1, p2, p3) => pb.cubic_to(
                p1.x as f32, p1.y as f32,
                p2.x as f32, p2.y as f32,
                p3.x as f32, p3.y as f32,
            ),
            PathEl::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

/// Convert ode-format Transform to tiny_skia Transform.
pub fn transform_to_skia(t: &ode_format::node::Transform) -> tiny_skia::Transform {
    tiny_skia::Transform::from_row(t.a, t.b, t.c, t.d, t.tx, t.ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectorpath_to_bezpath_line() {
        let vp = VectorPath {
            segments: vec![
                PathSegment::MoveTo { x: 0.0, y: 0.0 },
                PathSegment::LineTo { x: 100.0, y: 0.0 },
                PathSegment::LineTo { x: 100.0, y: 100.0 },
                PathSegment::Close,
            ],
            closed: true,
        };
        let bp = to_bezpath(&vp);
        // Should have 4 elements: MoveTo, LineTo, LineTo, ClosePath
        assert_eq!(bp.elements().len(), 4);
    }

    #[test]
    fn bezpath_roundtrip() {
        let vp = VectorPath {
            segments: vec![
                PathSegment::MoveTo { x: 10.0, y: 20.0 },
                PathSegment::CurveTo { x1: 30.0, y1: 40.0, x2: 50.0, y2: 60.0, x: 70.0, y: 80.0 },
                PathSegment::Close,
            ],
            closed: true,
        };
        let bp = to_bezpath(&vp);
        let vp2 = from_bezpath(&bp);
        assert_eq!(vp.closed, vp2.closed);
        assert_eq!(vp.segments.len(), vp2.segments.len());
    }

    #[test]
    fn rounded_rect_sharp_corners() {
        let bp = rounded_rect_path(100.0, 50.0, [0.0; 4]);
        // kurbo RoundedRect with zero radii produces 9 elements (uses curve segments
        // even for zero-radius corners): MoveTo + 4x (LineTo + CurveTo) + ClosePath
        let elems: Vec<_> = bp.elements().iter().collect();
        assert_eq!(elems.len(), 9);
    }

    #[test]
    fn rounded_rect_with_radii() {
        let bp = rounded_rect_path(100.0, 50.0, [10.0, 10.0, 10.0, 10.0]);
        // Should have curves at corners
        let has_curve = bp.elements().iter().any(|el| matches!(el, kurbo::PathEl::CurveTo(..)));
        assert!(has_curve, "Rounded rect should have curves");
    }

    #[test]
    fn bezpath_to_skia_simple() {
        let mut bp = BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((100.0, 0.0));
        bp.line_to((100.0, 100.0));
        bp.close_path();
        let skia_path = bezpath_to_skia(&bp);
        assert!(skia_path.is_some(), "Should produce a valid tiny-skia path");
    }
}
