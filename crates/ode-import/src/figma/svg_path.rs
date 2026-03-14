//! SVG path data parser.
//!
//! Parses SVG path strings (from Figma's `fillGeometry` / `strokeGeometry`)
//! into ODE's `VectorPath` representation.

use crate::error::ImportError;
use crate::figma::types::FigmaPath;
use ode_format::node::{FillRule, PathSegment, VectorPath};

// ─── Tokenizer ───────────────────────────────────────────────────────────────

/// A single token from an SVG path string: either a command letter or a number.
#[derive(Debug, Clone)]
enum Token {
    Command(char),
    Number(f64),
}

/// Tokenizes an SVG path string into a sequence of commands and numbers.
///
/// Handles:
/// - Whitespace and comma separators
/// - Numbers with optional sign, decimal point, and exponent
/// - Command letters as single-char tokens
/// - Adjacent numbers separated only by a sign character (e.g. `10-20`)
fn tokenize(input: &str) -> Result<Vec<Token>, ImportError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Skip whitespace and commas
        if ch.is_ascii_whitespace() || ch == ',' {
            i += 1;
            continue;
        }

        // Command letter
        if ch.is_ascii_alphabetic() {
            tokens.push(Token::Command(ch));
            i += 1;
            continue;
        }

        // Number: optional sign, digits, optional decimal, optional exponent
        if ch == '+' || ch == '-' || ch == '.' || ch.is_ascii_digit() {
            let start = i;
            // Sign
            if ch == '+' || ch == '-' {
                i += 1;
            }
            // Integer part
            while i < len && chars[i].is_ascii_digit() {
                i += 1;
            }
            // Decimal part
            if i < len && chars[i] == '.' {
                i += 1;
                while i < len && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            // Exponent
            if i < len && (chars[i] == 'e' || chars[i] == 'E') {
                i += 1;
                if i < len && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
                while i < len && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let s: String = chars[start..i].iter().collect();
            let num: f64 = s.parse().map_err(|_| {
                ImportError::PathParse(format!("invalid number: {}", s))
            })?;
            tokens.push(Token::Number(num));
            continue;
        }

        return Err(ImportError::PathParse(format!(
            "unexpected character '{}' at position {}",
            ch, i
        )));
    }

    Ok(tokens)
}

// ─── Parser ──────────────────────────────────────────────────────────────────

/// Parses an SVG path data string into an ODE `VectorPath`.
///
/// Supports all SVG path commands (both absolute and relative):
/// M/m, L/l, H/h, V/v, C/c, S/s, Q/q, T/t, A/a, Z/z.
///
/// Relative commands are converted to absolute coordinates. Smooth curves
/// (S, T) reflect the previous control point. Arcs (A) are approximated
/// with cubic bezier segments.
pub fn parse_svg_path(input: &str) -> Result<VectorPath, ImportError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(VectorPath {
            segments: vec![],
            closed: false,
        });
    }

    let tokens = tokenize(input)?;
    let mut segments: Vec<PathSegment> = Vec::new();
    let mut closed = false;

    // Current point
    let mut cx: f64 = 0.0;
    let mut cy: f64 = 0.0;

    // Start of current sub-path (for Z command)
    let mut sub_start_x: f64 = 0.0;
    let mut sub_start_y: f64 = 0.0;

    // Previous control points for smooth curves
    let mut last_cubic_cp_x: f64 = 0.0;
    let mut last_cubic_cp_y: f64 = 0.0;
    let mut last_quad_cp_x: f64 = 0.0;
    let mut last_quad_cp_y: f64 = 0.0;

    // Track what the previous command was (for S/T reflection logic)
    let mut prev_cmd: Option<char> = None;

    let mut pos = 0;
    let num_tokens = tokens.len();

    // Helper: read one number from tokens
    let read_num = |pos: &mut usize| -> Result<f64, ImportError> {
        if *pos >= num_tokens {
            return Err(ImportError::PathParse(
                "unexpected end of path data".to_string(),
            ));
        }
        match &tokens[*pos] {
            Token::Number(n) => {
                *pos += 1;
                Ok(*n)
            }
            Token::Command(c) => Err(ImportError::PathParse(format!(
                "expected number, got command '{}'",
                c
            ))),
        }
    };

    // Helper: read a flag (0 or 1) from tokens
    let read_flag = |pos: &mut usize| -> Result<bool, ImportError> {
        let n = read_num(pos)?;
        Ok(n != 0.0)
    };

    // Check if there are more numbers to consume for implicit repetition
    let has_more_numbers = |pos: usize| -> bool {
        pos < num_tokens && matches!(&tokens[pos], Token::Number(_))
    };

    while pos < num_tokens {
        // Read the next command
        let cmd = match &tokens[pos] {
            Token::Command(c) => {
                pos += 1;
                *c
            }
            Token::Number(_) => {
                // Implicit command: after M -> L, after m -> l, otherwise repeat previous
                match prev_cmd {
                    Some('M') => 'L',
                    Some('m') => 'l',
                    Some(c) => c,
                    None => {
                        return Err(ImportError::PathParse(
                            "path data must start with a command".to_string(),
                        ));
                    }
                }
            }
        };

        let is_relative = cmd.is_ascii_lowercase();
        let abs_cmd = cmd.to_ascii_uppercase();

        match abs_cmd {
            'M' => {
                loop {
                    let mut x = read_num(&mut pos)?;
                    let mut y = read_num(&mut pos)?;
                    if is_relative {
                        x += cx;
                        y += cy;
                    }
                    cx = x;
                    cy = y;
                    sub_start_x = x;
                    sub_start_y = y;
                    last_cubic_cp_x = cx;
                    last_cubic_cp_y = cy;
                    last_quad_cp_x = cx;
                    last_quad_cp_y = cy;
                    segments.push(PathSegment::MoveTo {
                        x: cx as f32,
                        y: cy as f32,
                    });
                    prev_cmd = Some(cmd);
                    // Subsequent coordinates after M are treated as L (or l)
                    if has_more_numbers(pos) {
                        // Switch to implicit LineTo for remaining coordinate pairs
                        break;
                    } else {
                        break;
                    }
                }
                // Handle implicit LineTo after MoveTo
                let implicit_line_relative = is_relative;
                while has_more_numbers(pos) {
                    let mut x = read_num(&mut pos)?;
                    let mut y = read_num(&mut pos)?;
                    if implicit_line_relative {
                        x += cx;
                        y += cy;
                    }
                    cx = x;
                    cy = y;
                    last_cubic_cp_x = cx;
                    last_cubic_cp_y = cy;
                    last_quad_cp_x = cx;
                    last_quad_cp_y = cy;
                    segments.push(PathSegment::LineTo {
                        x: cx as f32,
                        y: cy as f32,
                    });
                    prev_cmd = if implicit_line_relative {
                        Some('l')
                    } else {
                        Some('L')
                    };
                }
            }

            'L' => {
                loop {
                    let mut x = read_num(&mut pos)?;
                    let mut y = read_num(&mut pos)?;
                    if is_relative {
                        x += cx;
                        y += cy;
                    }
                    cx = x;
                    cy = y;
                    last_cubic_cp_x = cx;
                    last_cubic_cp_y = cy;
                    last_quad_cp_x = cx;
                    last_quad_cp_y = cy;
                    segments.push(PathSegment::LineTo {
                        x: cx as f32,
                        y: cy as f32,
                    });
                    prev_cmd = Some(cmd);
                    if !has_more_numbers(pos) {
                        break;
                    }
                }
            }

            'H' => {
                loop {
                    let mut x = read_num(&mut pos)?;
                    if is_relative {
                        x += cx;
                    }
                    cx = x;
                    last_cubic_cp_x = cx;
                    last_cubic_cp_y = cy;
                    last_quad_cp_x = cx;
                    last_quad_cp_y = cy;
                    segments.push(PathSegment::LineTo {
                        x: cx as f32,
                        y: cy as f32,
                    });
                    prev_cmd = Some(cmd);
                    if !has_more_numbers(pos) {
                        break;
                    }
                }
            }

            'V' => {
                loop {
                    let mut y = read_num(&mut pos)?;
                    if is_relative {
                        y += cy;
                    }
                    cy = y;
                    last_cubic_cp_x = cx;
                    last_cubic_cp_y = cy;
                    last_quad_cp_x = cx;
                    last_quad_cp_y = cy;
                    segments.push(PathSegment::LineTo {
                        x: cx as f32,
                        y: cy as f32,
                    });
                    prev_cmd = Some(cmd);
                    if !has_more_numbers(pos) {
                        break;
                    }
                }
            }

            'C' => {
                loop {
                    let mut x1 = read_num(&mut pos)?;
                    let mut y1 = read_num(&mut pos)?;
                    let mut x2 = read_num(&mut pos)?;
                    let mut y2 = read_num(&mut pos)?;
                    let mut x = read_num(&mut pos)?;
                    let mut y = read_num(&mut pos)?;
                    if is_relative {
                        x1 += cx;
                        y1 += cy;
                        x2 += cx;
                        y2 += cy;
                        x += cx;
                        y += cy;
                    }
                    segments.push(PathSegment::CurveTo {
                        x1: x1 as f32,
                        y1: y1 as f32,
                        x2: x2 as f32,
                        y2: y2 as f32,
                        x: x as f32,
                        y: y as f32,
                    });
                    last_cubic_cp_x = x2;
                    last_cubic_cp_y = y2;
                    last_quad_cp_x = x;
                    last_quad_cp_y = y;
                    cx = x;
                    cy = y;
                    prev_cmd = Some(cmd);
                    if !has_more_numbers(pos) {
                        break;
                    }
                }
            }

            'S' => {
                loop {
                    // Reflect previous cubic control point
                    let (rx, ry) = match prev_cmd {
                        Some('C') | Some('c') | Some('S') | Some('s') => {
                            (2.0 * cx - last_cubic_cp_x, 2.0 * cy - last_cubic_cp_y)
                        }
                        _ => (cx, cy),
                    };
                    let mut x2 = read_num(&mut pos)?;
                    let mut y2 = read_num(&mut pos)?;
                    let mut x = read_num(&mut pos)?;
                    let mut y = read_num(&mut pos)?;
                    if is_relative {
                        x2 += cx;
                        y2 += cy;
                        x += cx;
                        y += cy;
                    }
                    segments.push(PathSegment::CurveTo {
                        x1: rx as f32,
                        y1: ry as f32,
                        x2: x2 as f32,
                        y2: y2 as f32,
                        x: x as f32,
                        y: y as f32,
                    });
                    last_cubic_cp_x = x2;
                    last_cubic_cp_y = y2;
                    last_quad_cp_x = x;
                    last_quad_cp_y = y;
                    cx = x;
                    cy = y;
                    prev_cmd = Some(cmd);
                    if !has_more_numbers(pos) {
                        break;
                    }
                }
            }

            'Q' => {
                loop {
                    let mut x1 = read_num(&mut pos)?;
                    let mut y1 = read_num(&mut pos)?;
                    let mut x = read_num(&mut pos)?;
                    let mut y = read_num(&mut pos)?;
                    if is_relative {
                        x1 += cx;
                        y1 += cy;
                        x += cx;
                        y += cy;
                    }
                    segments.push(PathSegment::QuadTo {
                        x1: x1 as f32,
                        y1: y1 as f32,
                        x: x as f32,
                        y: y as f32,
                    });
                    last_quad_cp_x = x1;
                    last_quad_cp_y = y1;
                    last_cubic_cp_x = x;
                    last_cubic_cp_y = y;
                    cx = x;
                    cy = y;
                    prev_cmd = Some(cmd);
                    if !has_more_numbers(pos) {
                        break;
                    }
                }
            }

            'T' => {
                loop {
                    // Reflect previous quad control point
                    let (rx, ry) = match prev_cmd {
                        Some('Q') | Some('q') | Some('T') | Some('t') => {
                            (2.0 * cx - last_quad_cp_x, 2.0 * cy - last_quad_cp_y)
                        }
                        _ => (cx, cy),
                    };
                    let mut x = read_num(&mut pos)?;
                    let mut y = read_num(&mut pos)?;
                    if is_relative {
                        x += cx;
                        y += cy;
                    }
                    segments.push(PathSegment::QuadTo {
                        x1: rx as f32,
                        y1: ry as f32,
                        x: x as f32,
                        y: y as f32,
                    });
                    last_quad_cp_x = rx;
                    last_quad_cp_y = ry;
                    last_cubic_cp_x = x;
                    last_cubic_cp_y = y;
                    cx = x;
                    cy = y;
                    prev_cmd = Some(cmd);
                    if !has_more_numbers(pos) {
                        break;
                    }
                }
            }

            'A' => {
                loop {
                    let rx_val = read_num(&mut pos)?;
                    let ry_val = read_num(&mut pos)?;
                    let x_rotation = read_num(&mut pos)?;
                    let large_arc = read_flag(&mut pos)?;
                    let sweep = read_flag(&mut pos)?;
                    let mut x = read_num(&mut pos)?;
                    let mut y = read_num(&mut pos)?;
                    if is_relative {
                        x += cx;
                        y += cy;
                    }
                    arc_to_cubics(
                        &mut segments,
                        cx,
                        cy,
                        rx_val,
                        ry_val,
                        x_rotation,
                        large_arc,
                        sweep,
                        x,
                        y,
                    );
                    last_cubic_cp_x = x;
                    last_cubic_cp_y = y;
                    last_quad_cp_x = x;
                    last_quad_cp_y = y;
                    cx = x;
                    cy = y;
                    prev_cmd = Some(cmd);
                    if !has_more_numbers(pos) {
                        break;
                    }
                }
            }

            'Z' => {
                segments.push(PathSegment::Close);
                closed = true;
                cx = sub_start_x;
                cy = sub_start_y;
                last_cubic_cp_x = cx;
                last_cubic_cp_y = cy;
                last_quad_cp_x = cx;
                last_quad_cp_y = cy;
                prev_cmd = Some(cmd);
            }

            _ => {
                return Err(ImportError::PathParse(format!(
                    "unknown SVG path command '{}'",
                    cmd
                )));
            }
        }
    }

    Ok(VectorPath { segments, closed })
}

// ─── Arc to Cubic Bezier ─────────────────────────────────────────────────────

/// Converts an SVG arc (endpoint parameterization) to one or more cubic bezier
/// segments appended to `segments`.
///
/// Implements the standard SVG arc conversion algorithm:
/// 1. Convert endpoint parameterization to center parameterization
/// 2. Split into segments of at most 90 degrees
/// 3. Approximate each segment with a cubic bezier
fn arc_to_cubics(
    segments: &mut Vec<PathSegment>,
    x1: f64,
    y1: f64,
    mut rx: f64,
    mut ry: f64,
    x_rotation_deg: f64,
    large_arc: bool,
    sweep: bool,
    x2: f64,
    y2: f64,
) {
    // Degenerate cases: if endpoints are the same, emit nothing
    if (x1 - x2).abs() < 1e-10 && (y1 - y2).abs() < 1e-10 {
        return;
    }

    // Degenerate case: if either radius is zero, treat as line
    if rx.abs() < 1e-10 || ry.abs() < 1e-10 {
        segments.push(PathSegment::LineTo {
            x: x2 as f32,
            y: y2 as f32,
        });
        return;
    }

    rx = rx.abs();
    ry = ry.abs();

    let phi = x_rotation_deg.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();

    // Step 1: Compute (x1', y1') — translated/rotated midpoint
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    // Step 2: Scale radii if necessary
    let x1p_sq = x1p * x1p;
    let y1p_sq = y1p * y1p;
    let mut rx_sq = rx * rx;
    let mut ry_sq = ry * ry;

    let lambda = x1p_sq / rx_sq + y1p_sq / ry_sq;
    if lambda > 1.0 {
        let sqrt_lambda = lambda.sqrt();
        rx *= sqrt_lambda;
        ry *= sqrt_lambda;
        rx_sq = rx * rx;
        ry_sq = ry * ry;
    }

    // Step 3: Compute center point (cx', cy')
    let num = (rx_sq * ry_sq - rx_sq * y1p_sq - ry_sq * x1p_sq)
        .max(0.0);
    let den = rx_sq * y1p_sq + ry_sq * x1p_sq;

    let sq = if den.abs() < 1e-10 {
        0.0
    } else {
        (num / den).sqrt()
    };

    let sign = if large_arc == sweep { -1.0 } else { 1.0 };

    let cxp = sign * sq * (rx * y1p / ry);
    let cyp = sign * sq * -(ry * x1p / rx);

    // Step 4: Compute center point (cx, cy) in original coordinates
    let mx = (x1 + x2) / 2.0;
    let my = (y1 + y2) / 2.0;
    let _cx = cos_phi * cxp - sin_phi * cyp + mx;
    let _cy = sin_phi * cxp + cos_phi * cyp + my;

    // Step 5: Compute theta1 and dtheta
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;

    let theta1 = vec_angle(1.0, 0.0, ux, uy);
    let mut dtheta = vec_angle(ux, uy, vx, vy);

    if !sweep && dtheta > 0.0 {
        dtheta -= std::f64::consts::TAU;
    }
    if sweep && dtheta < 0.0 {
        dtheta += std::f64::consts::TAU;
    }

    // Step 6: Split arc into segments of max 90 degrees and emit cubics
    let n_segs = (dtheta.abs() / std::f64::consts::FRAC_PI_2).ceil() as usize;
    let n_segs = n_segs.max(1);
    let d = dtheta / n_segs as f64;

    let alpha = (d / 2.0).sin()
        * ((4.0 + 3.0 * (d / 2.0).tan().powi(2)).sqrt() - 1.0)
        / 3.0;

    let mut t = theta1;
    let mut px = x1;
    let mut py = y1;

    for _ in 0..n_segs {
        let t_next = t + d;

        // End point of this segment on the unit ellipse
        let cos_t_next = t_next.cos();
        let sin_t_next = t_next.sin();

        // End point in original coordinates
        let ex = cos_phi * rx * cos_t_next - sin_phi * ry * sin_t_next + _cx;
        let ey = sin_phi * rx * cos_t_next + cos_phi * ry * sin_t_next + _cy;

        // Tangent at start
        let cos_t = t.cos();
        let sin_t = t.sin();
        let dx1 = -rx * sin_t;
        let dy1 = ry * cos_t;
        let tdx1 = cos_phi * dx1 - sin_phi * dy1;
        let tdy1 = sin_phi * dx1 + cos_phi * dy1;

        // Tangent at end
        let dx2 = -rx * sin_t_next;
        let dy2 = ry * cos_t_next;
        let tdx2 = cos_phi * dx2 - sin_phi * dy2;
        let tdy2 = sin_phi * dx2 + cos_phi * dy2;

        // Control points
        let cp1x = px + alpha * tdx1;
        let cp1y = py + alpha * tdy1;
        let cp2x = ex - alpha * tdx2;
        let cp2y = ey - alpha * tdy2;

        segments.push(PathSegment::CurveTo {
            x1: cp1x as f32,
            y1: cp1y as f32,
            x2: cp2x as f32,
            y2: cp2y as f32,
            x: ex as f32,
            y: ey as f32,
        });

        px = ex;
        py = ey;
        t = t_next;
    }
}

/// Computes the angle in radians between vectors (ux, uy) and (vx, vy).
fn vec_angle(ux: f64, uy: f64, vx: f64, vy: f64) -> f64 {
    let dot = ux * vx + uy * vy;
    let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
    let mut cos_val = dot / len;
    // Clamp for numerical stability
    cos_val = cos_val.clamp(-1.0, 1.0);
    let angle = cos_val.acos();
    if ux * vy - uy * vx < 0.0 {
        -angle
    } else {
        angle
    }
}

// ─── Merge Figma Paths ───────────────────────────────────────────────────────

/// Merges multiple `FigmaPath` entries into a single `VectorPath` with a
/// combined `FillRule`.
///
/// Each sub-path is parsed individually and its segments are concatenated.
/// The fill rule is taken from the first path's `winding_rule`: `"EVENODD"`
/// maps to `FillRule::EvenOdd`, everything else (including `None`) maps to
/// `FillRule::NonZero`.
pub fn merge_figma_paths(paths: &[FigmaPath]) -> Result<(VectorPath, FillRule), ImportError> {
    let mut all_segments: Vec<PathSegment> = Vec::new();
    let mut any_closed = false;

    let fill_rule = paths
        .first()
        .and_then(|p| p.winding_rule.as_deref())
        .map(|wr| {
            if wr == "EVENODD" {
                FillRule::EvenOdd
            } else {
                FillRule::NonZero
            }
        })
        .unwrap_or(FillRule::NonZero);

    for fp in paths {
        let vp = parse_svg_path(&fp.path)?;
        if vp.closed {
            any_closed = true;
        }
        all_segments.extend(vp.segments);
    }

    Ok((
        VectorPath {
            segments: all_segments,
            closed: any_closed,
        },
        fill_rule,
    ))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::node::PathSegment;

    #[test]
    fn parse_simple_rect() {
        let path = parse_svg_path("M 0 0 L 100 0 L 100 100 L 0 100 Z").unwrap();
        assert_eq!(path.segments.len(), 5);
        assert!(
            matches!(path.segments[0], PathSegment::MoveTo { x, y } if x == 0.0 && y == 0.0)
        );
        assert!(matches!(path.segments[4], PathSegment::Close));
        assert!(path.closed);
    }

    #[test]
    fn parse_relative_commands() {
        let path = parse_svg_path("M 10 10 l 50 0 l 0 50 z").unwrap();
        assert_eq!(path.segments.len(), 4);
        assert!(
            matches!(path.segments[1], PathSegment::LineTo { x, y } if (x - 60.0).abs() < 0.001 && (y - 10.0).abs() < 0.001)
        );
    }

    #[test]
    fn parse_cubic_bezier() {
        let path = parse_svg_path("M 0 0 C 10 20 30 40 50 60").unwrap();
        assert_eq!(path.segments.len(), 2);
        assert!(matches!(path.segments[1], PathSegment::CurveTo { .. }));
    }

    #[test]
    fn parse_h_v_commands() {
        let path = parse_svg_path("M 0 0 H 100 V 50").unwrap();
        assert_eq!(path.segments.len(), 3);
        assert!(
            matches!(path.segments[1], PathSegment::LineTo { x, y } if x == 100.0 && y == 0.0)
        );
        assert!(
            matches!(path.segments[2], PathSegment::LineTo { x, y } if x == 100.0 && y == 50.0)
        );
    }

    #[test]
    fn parse_quadratic() {
        let path = parse_svg_path("M 0 0 Q 50 100 100 0").unwrap();
        assert_eq!(path.segments.len(), 2);
        assert!(matches!(path.segments[1], PathSegment::QuadTo { .. }));
    }

    #[test]
    fn parse_smooth_cubic() {
        let path =
            parse_svg_path("M 0 0 C 10 20 30 40 50 50 S 80 60 100 50").unwrap();
        assert_eq!(path.segments.len(), 3); // M, C, C (S becomes C with reflected cp)
        assert!(matches!(path.segments[2], PathSegment::CurveTo { .. }));
    }

    #[test]
    fn parse_smooth_quad() {
        let path =
            parse_svg_path("M 0 0 Q 50 100 100 0 T 200 0").unwrap();
        assert_eq!(path.segments.len(), 3); // M, Q, Q (T becomes Q with reflected cp)
        assert!(matches!(path.segments[2], PathSegment::QuadTo { .. }));
    }

    #[test]
    fn parse_arc_command() {
        let path = parse_svg_path("M 0 50 A 50 50 0 0 1 100 50").unwrap();
        assert!(path.segments.len() >= 2);
        assert!(matches!(path.segments[0], PathSegment::MoveTo { .. }));
        // Arc should be converted to CurveTo segments
        assert!(matches!(path.segments[1], PathSegment::CurveTo { .. }));
    }

    #[test]
    fn parse_no_close_means_not_closed() {
        let path = parse_svg_path("M 0 0 L 100 100").unwrap();
        assert!(!path.closed);
    }

    #[test]
    fn parse_implicit_lineto_after_moveto() {
        // After M, subsequent coordinate pairs are implicit L commands
        let path = parse_svg_path("M 0 0 100 0 100 100").unwrap();
        assert_eq!(path.segments.len(), 3); // M, L, L
    }

    #[test]
    fn merge_multiple_paths() {
        use crate::figma::types::FigmaPath;
        let paths = vec![
            FigmaPath {
                path: "M 0 0 L 10 10 Z".into(),
                winding_rule: Some("EVENODD".into()),
                overridden_fields: None,
            },
            FigmaPath {
                path: "M 20 20 L 30 30 Z".into(),
                winding_rule: None,
                overridden_fields: None,
            },
        ];
        let (vp, rule) = merge_figma_paths(&paths).unwrap();
        assert_eq!(vp.segments.len(), 6); // M L Z M L Z
        assert_eq!(rule, ode_format::node::FillRule::EvenOdd);
    }

    #[test]
    fn parse_comma_separated_coords() {
        let path = parse_svg_path("M0,0 L100,0 L100,100Z").unwrap();
        assert_eq!(path.segments.len(), 4);
        assert!(path.closed);
    }

    #[test]
    fn parse_empty_path() {
        let path = parse_svg_path("").unwrap();
        assert!(path.segments.is_empty());
        assert!(!path.closed);
    }
}
