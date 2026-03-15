//! Figma style conversion: Paint, Effect, BlendMode, Stroke.
//!
//! Converts Figma REST API style types into ODE format equivalents.

use std::collections::HashMap;

use ode_format::color::Color;
use ode_format::style::{
    BlendMode, CollectionId, DashPattern, Effect, Fill, GradientStop, ImageFillMode, ImageSource,
    Paint, Point, Stroke, StrokeCap, StrokeJoin, StrokePosition, StyleValue, TokenId, TokenRef,
};

use super::types::{FigmaColor, FigmaColorStop, FigmaEffect, FigmaPaint, FigmaVector};
use crate::error::ImportWarning;

/// Map from Figma variable ID to ODE (CollectionId, TokenId).
pub type VariableMap = HashMap<String, (CollectionId, TokenId)>;

// ─── Color ──────────────────────────────────────────────────────────────────

/// Convert a Figma RGBA color to an ODE sRGB color.
pub fn convert_color(c: &FigmaColor) -> Color {
    Color::Srgb {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

// ─── BlendMode ──────────────────────────────────────────────────────────────

/// Convert a Figma blend mode string to an ODE `BlendMode`.
///
/// Figma modes that have no direct ODE equivalent (e.g. `LINEAR_BURN`,
/// `LINEAR_DODGE`) fall back to `Normal` with a warning.
pub fn convert_blend_mode(s: &str, warnings: &mut Vec<ImportWarning>) -> BlendMode {
    match s {
        "PASS_THROUGH" | "NORMAL" => BlendMode::Normal,
        "MULTIPLY" => BlendMode::Multiply,
        "SCREEN" => BlendMode::Screen,
        "OVERLAY" => BlendMode::Overlay,
        "DARKEN" => BlendMode::Darken,
        "LIGHTEN" => BlendMode::Lighten,
        "COLOR_DODGE" => BlendMode::ColorDodge,
        "COLOR_BURN" => BlendMode::ColorBurn,
        "HARD_LIGHT" => BlendMode::HardLight,
        "SOFT_LIGHT" => BlendMode::SoftLight,
        "DIFFERENCE" => BlendMode::Difference,
        "EXCLUSION" => BlendMode::Exclusion,
        "HUE" => BlendMode::Hue,
        "SATURATION" => BlendMode::Saturation,
        "COLOR" => BlendMode::Color,
        "LUMINOSITY" => BlendMode::Luminosity,
        "LINEAR_BURN" | "LINEAR_DODGE" => {
            warnings.push(ImportWarning {
                node_id: String::new(),
                node_name: String::new(),
                message: format!("Unsupported blend mode '{s}', falling back to Normal"),
            });
            BlendMode::Normal
        }
        other => {
            warnings.push(ImportWarning {
                node_id: String::new(),
                node_name: String::new(),
                message: format!("Unknown blend mode '{other}', falling back to Normal"),
            });
            BlendMode::Normal
        }
    }
}

// ─── Bound Variable Helpers ─────────────────────────────────────────────────

/// Try to create a `StyleValue::Bound` for a color property.
///
/// Looks up `property_key` (e.g. "color") in the paint/effect's `bound_variables`.
/// If found and mappable through `variable_map`, returns `StyleValue::Bound`;
/// otherwise returns `StyleValue::Raw`.
fn bind_color(
    raw: Color,
    property_key: &str,
    bound_variables: Option<&HashMap<String, super::types::FigmaVariableAlias>>,
    variable_map: &VariableMap,
) -> StyleValue<Color> {
    if let Some(bv) = bound_variables {
        if let Some(alias) = bv.get(property_key) {
            if let Some(&(coll_id, tok_id)) = variable_map.get(&alias.id) {
                return StyleValue::Bound {
                    token: TokenRef {
                        collection_id: coll_id,
                        token_id: tok_id,
                    },
                    resolved: raw,
                };
            }
        }
    }
    StyleValue::Raw(raw)
}

/// Try to create a `StyleValue::Bound` for an f32 property.
fn bind_f32(
    raw: f32,
    property_key: &str,
    bound_variables: Option<&HashMap<String, super::types::FigmaVariableAlias>>,
    variable_map: &VariableMap,
) -> StyleValue<f32> {
    if let Some(bv) = bound_variables {
        if let Some(alias) = bv.get(property_key) {
            if let Some(&(coll_id, tok_id)) = variable_map.get(&alias.id) {
                return StyleValue::Bound {
                    token: TokenRef {
                        collection_id: coll_id,
                        token_id: tok_id,
                    },
                    resolved: raw,
                };
            }
        }
    }
    StyleValue::Raw(raw)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn convert_gradient_stops(
    stops: &[FigmaColorStop],
    variable_map: &VariableMap,
) -> Vec<GradientStop> {
    stops
        .iter()
        .map(|s| {
            let raw = convert_color(&s.color);
            let color = bind_color(raw, "color", s.bound_variables.as_ref(), variable_map);
            GradientStop {
                position: s.position,
                color,
            }
        })
        .collect()
}

fn figma_vec_to_point(v: &FigmaVector) -> Point {
    Point {
        x: v.x as f32,
        y: v.y as f32,
    }
}

// ─── Fill ───────────────────────────────────────────────────────────────────

/// Convert a Figma paint into an ODE `Fill`.
///
/// Returns `None` for invisible paints or unsupported paint types (VIDEO,
/// PATTERN, EMOJI) — the latter also emits a warning.
pub fn convert_fill(
    paint: &FigmaPaint,
    variable_map: &VariableMap,
    warnings: &mut Vec<ImportWarning>,
) -> Option<Fill> {
    // Skip invisible paints.
    if paint.visible == Some(false) {
        return None;
    }

    let ode_paint = convert_paint(paint, variable_map, warnings)?;

    let raw_opacity = paint.opacity.unwrap_or(1.0);
    let opacity = bind_f32(
        raw_opacity,
        "opacity",
        paint.bound_variables.as_ref(),
        variable_map,
    );
    let blend_mode = paint
        .blend_mode
        .as_deref()
        .map(|bm| convert_blend_mode(bm, warnings))
        .unwrap_or(BlendMode::Normal);

    Some(Fill {
        paint: ode_paint,
        opacity,
        blend_mode,
        visible: true,
    })
}

/// Inner paint conversion shared by fills and strokes.
fn convert_paint(
    paint: &FigmaPaint,
    variable_map: &VariableMap,
    warnings: &mut Vec<ImportWarning>,
) -> Option<Paint> {
    match paint.paint_type.as_str() {
        "SOLID" => {
            let fc = paint.color.as_ref()?;
            let raw = convert_color(fc);
            let color = bind_color(raw, "color", paint.bound_variables.as_ref(), variable_map);
            Some(Paint::Solid { color })
        }
        "GRADIENT_LINEAR" => {
            let stops = paint.gradient_stops.as_deref().unwrap_or_default();
            let handles = paint
                .gradient_handle_positions
                .as_deref()
                .unwrap_or_default();
            let start = handles
                .first()
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 0.0, y: 0.5 });
            let end = handles
                .get(1)
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 1.0, y: 0.5 });
            Some(Paint::LinearGradient {
                stops: convert_gradient_stops(stops, variable_map),
                start,
                end,
            })
        }
        "GRADIENT_RADIAL" => {
            let stops = paint.gradient_stops.as_deref().unwrap_or_default();
            let handles = paint
                .gradient_handle_positions
                .as_deref()
                .unwrap_or_default();
            let center = handles
                .first()
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 0.5, y: 0.5 });
            let h1 = handles
                .get(1)
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 1.0, y: 0.5 });
            let h2 = handles
                .get(2)
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 0.5, y: 1.0 });
            let radius = Point {
                x: ((h1.x - center.x).powi(2) + (h1.y - center.y).powi(2)).sqrt(),
                y: ((h2.x - center.x).powi(2) + (h2.y - center.y).powi(2)).sqrt(),
            };
            Some(Paint::RadialGradient {
                stops: convert_gradient_stops(stops, variable_map),
                center,
                radius,
            })
        }
        "GRADIENT_ANGULAR" => {
            let stops = paint.gradient_stops.as_deref().unwrap_or_default();
            let handles = paint
                .gradient_handle_positions
                .as_deref()
                .unwrap_or_default();
            let center = handles
                .first()
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 0.5, y: 0.5 });
            let h1 = handles
                .get(1)
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 1.0, y: 0.5 });
            let angle = (h1.y - center.y).atan2(h1.x - center.x).to_degrees();
            Some(Paint::AngularGradient {
                stops: convert_gradient_stops(stops, variable_map),
                center,
                angle,
            })
        }
        "GRADIENT_DIAMOND" => {
            let stops = paint.gradient_stops.as_deref().unwrap_or_default();
            let handles = paint
                .gradient_handle_positions
                .as_deref()
                .unwrap_or_default();
            let center = handles
                .first()
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 0.5, y: 0.5 });
            let h1 = handles
                .get(1)
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 1.0, y: 0.5 });
            let h2 = handles
                .get(2)
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 0.5, y: 1.0 });
            let radius = Point {
                x: ((h1.x - center.x).powi(2) + (h1.y - center.y).powi(2)).sqrt(),
                y: ((h2.x - center.x).powi(2) + (h2.y - center.y).powi(2)).sqrt(),
            };
            Some(Paint::DiamondGradient {
                stops: convert_gradient_stops(stops, variable_map),
                center,
                radius,
            })
        }
        "IMAGE" => {
            let path = paint.image_ref.clone().unwrap_or_default();
            let mode = paint
                .scale_mode
                .as_deref()
                .map(convert_image_fill_mode)
                .unwrap_or(ImageFillMode::Fill);
            Some(Paint::ImageFill {
                source: ImageSource::Linked { path },
                mode,
            })
        }
        "VIDEO" | "PATTERN" | "EMOJI" => {
            warnings.push(ImportWarning {
                node_id: String::new(),
                node_name: String::new(),
                message: format!("Unsupported paint type '{}', skipping", paint.paint_type),
            });
            None
        }
        other => {
            warnings.push(ImportWarning {
                node_id: String::new(),
                node_name: String::new(),
                message: format!("Unknown paint type '{other}', skipping"),
            });
            None
        }
    }
}

// ─── Stroke ─────────────────────────────────────────────────────────────────

/// Convert a Figma paint plus stroke attributes into an ODE `Stroke`.
///
/// Returns `None` when the paint is invisible or the paint type is
/// unsupported.
#[allow(clippy::too_many_arguments)]
pub fn convert_stroke(
    paint: &FigmaPaint,
    weight: f32,
    align: Option<&str>,
    cap: Option<&str>,
    join: Option<&str>,
    miter_angle: Option<f32>,
    dashes: Option<&[f32]>,
    variable_map: &VariableMap,
    warnings: &mut Vec<ImportWarning>,
) -> Option<Stroke> {
    if paint.visible == Some(false) {
        return None;
    }

    let ode_paint = convert_paint(paint, variable_map, warnings)?;

    let position = match align {
        Some("INSIDE") => StrokePosition::Inside,
        Some("OUTSIDE") => StrokePosition::Outside,
        _ => StrokePosition::Center,
    };

    let stroke_cap = match cap {
        Some("NONE") => StrokeCap::Butt,
        Some("ROUND") => StrokeCap::Round,
        Some("SQUARE") => StrokeCap::Square,
        Some(arrow)
            if arrow.contains("ARROW")
                || arrow.starts_with("LINE_ARROW")
                || arrow.starts_with("TRIANGLE_ARROW") =>
        {
            warnings.push(ImportWarning {
                node_id: String::new(),
                node_name: String::new(),
                message: format!("Arrow cap '{arrow}' not supported, falling back to Butt"),
            });
            StrokeCap::Butt
        }
        _ => StrokeCap::Butt,
    };

    let stroke_join = match join {
        Some("BEVEL") => StrokeJoin::Bevel,
        Some("ROUND") => StrokeJoin::Round,
        _ => StrokeJoin::Miter,
    };

    let miter_limit = miter_angle.map(convert_miter).unwrap_or(4.0);

    let dash = dashes.and_then(|d| {
        if d.is_empty() {
            None
        } else {
            Some(DashPattern {
                segments: d.to_vec(),
                offset: 0.0,
            })
        }
    });

    let raw_opacity = paint.opacity.unwrap_or(1.0);
    let opacity = bind_f32(
        raw_opacity,
        "opacity",
        paint.bound_variables.as_ref(),
        variable_map,
    );
    let blend_mode = paint
        .blend_mode
        .as_deref()
        .map(|bm| convert_blend_mode(bm, warnings))
        .unwrap_or(BlendMode::Normal);

    Some(Stroke {
        paint: ode_paint,
        width: StyleValue::Raw(weight),
        position,
        cap: stroke_cap,
        join: stroke_join,
        miter_limit,
        dash,
        opacity,
        blend_mode,
        visible: true,
    })
}

// ─── Effect ─────────────────────────────────────────────────────────────────

/// Convert a Figma effect into an ODE `Effect`.
///
/// Returns `None` for invisible effects or unsupported effect types.
pub fn convert_effect(
    effect: &FigmaEffect,
    variable_map: &VariableMap,
    warnings: &mut Vec<ImportWarning>,
) -> Option<Effect> {
    if effect.visible == Some(false) {
        return None;
    }

    let bv = effect.bound_variables.as_ref();

    match effect.effect_type.as_str() {
        "DROP_SHADOW" => {
            let raw_color = effect
                .color
                .as_ref()
                .map(convert_color)
                .unwrap_or_else(Color::black);
            let offset = effect
                .offset
                .as_ref()
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 0.0, y: 0.0 });
            let raw_blur = effect.radius.unwrap_or(0.0);
            let raw_spread = effect.spread.unwrap_or(0.0);
            Some(Effect::DropShadow {
                color: bind_color(raw_color, "color", bv, variable_map),
                offset,
                blur: bind_f32(raw_blur, "radius", bv, variable_map),
                spread: bind_f32(raw_spread, "spread", bv, variable_map),
            })
        }
        "INNER_SHADOW" => {
            let raw_color = effect
                .color
                .as_ref()
                .map(convert_color)
                .unwrap_or_else(Color::black);
            let offset = effect
                .offset
                .as_ref()
                .map(figma_vec_to_point)
                .unwrap_or(Point { x: 0.0, y: 0.0 });
            let raw_blur = effect.radius.unwrap_or(0.0);
            let raw_spread = effect.spread.unwrap_or(0.0);
            Some(Effect::InnerShadow {
                color: bind_color(raw_color, "color", bv, variable_map),
                offset,
                blur: bind_f32(raw_blur, "radius", bv, variable_map),
                spread: bind_f32(raw_spread, "spread", bv, variable_map),
            })
        }
        "LAYER_BLUR" => {
            let raw_radius = effect.radius.unwrap_or(0.0);
            Some(Effect::LayerBlur {
                radius: bind_f32(raw_radius, "radius", bv, variable_map),
            })
        }
        "BACKGROUND_BLUR" => {
            let raw_radius = effect.radius.unwrap_or(0.0);
            Some(Effect::BackgroundBlur {
                radius: bind_f32(raw_radius, "radius", bv, variable_map),
            })
        }
        "TEXTURE" | "NOISE" => {
            warnings.push(ImportWarning {
                node_id: String::new(),
                node_name: String::new(),
                message: format!("Unsupported effect type '{}', skipping", effect.effect_type),
            });
            None
        }
        other => {
            warnings.push(ImportWarning {
                node_id: String::new(),
                node_name: String::new(),
                message: format!("Unknown effect type '{other}', skipping"),
            });
            None
        }
    }
}

// ─── Miter ──────────────────────────────────────────────────────────────────

/// Convert a Figma miter angle (degrees) to an ODE miter limit.
///
/// Formula: `1 / sin(angle/2)`. If sin is near zero, returns 100.0.
pub fn convert_miter(miter_angle_deg: f32) -> f32 {
    let half_rad = (miter_angle_deg / 2.0).to_radians();
    let sin_val = half_rad.sin();
    if sin_val.abs() < 1e-6 {
        100.0
    } else {
        (1.0 / sin_val).abs()
    }
}

// ─── ImageFillMode ──────────────────────────────────────────────────────────

/// Convert a Figma `scaleMode` string to an ODE `ImageFillMode`.
///
/// `STRETCH` maps to `Fill` (lossy). Unknown modes default to `Fill`.
pub fn convert_image_fill_mode(s: &str) -> ImageFillMode {
    match s {
        "FILL" => ImageFillMode::Fill,
        "FIT" => ImageFillMode::Fit,
        "TILE" => ImageFillMode::Tile,
        "CROP" => ImageFillMode::Crop,
        "STRETCH" => ImageFillMode::Fill, // lossy mapping
        _ => ImageFillMode::Fill,
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_warnings() -> Vec<ImportWarning> {
        Vec::new()
    }

    fn empty_var_map() -> VariableMap {
        HashMap::new()
    }

    // ── convert_color ───────────────────────────────────────────────────

    #[test]
    fn test_convert_color() {
        let fc = FigmaColor {
            r: 0.2,
            g: 0.4,
            b: 0.6,
            a: 0.8,
        };
        let c = convert_color(&fc);
        match c {
            Color::Srgb { r, g, b, a } => {
                assert!((r - 0.2).abs() < f32::EPSILON);
                assert!((g - 0.4).abs() < f32::EPSILON);
                assert!((b - 0.6).abs() < f32::EPSILON);
                assert!((a - 0.8).abs() < f32::EPSILON);
            }
            _ => panic!("Expected Srgb"),
        }
    }

    // ── convert_blend_mode ──────────────────────────────────────────────

    #[test]
    fn test_blend_mode_normal() {
        let mut w = empty_warnings();
        assert_eq!(convert_blend_mode("NORMAL", &mut w), BlendMode::Normal);
        assert!(w.is_empty());
    }

    #[test]
    fn test_blend_mode_multiply() {
        let mut w = empty_warnings();
        assert_eq!(convert_blend_mode("MULTIPLY", &mut w), BlendMode::Multiply);
        assert!(w.is_empty());
    }

    #[test]
    fn test_blend_mode_pass_through() {
        let mut w = empty_warnings();
        assert_eq!(
            convert_blend_mode("PASS_THROUGH", &mut w),
            BlendMode::Normal
        );
        assert!(w.is_empty());
    }

    #[test]
    fn test_blend_mode_linear_burn_warning() {
        let mut w = empty_warnings();
        let bm = convert_blend_mode("LINEAR_BURN", &mut w);
        assert_eq!(bm, BlendMode::Normal);
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("LINEAR_BURN"));
    }

    #[test]
    fn test_blend_mode_unknown_warning() {
        let mut w = empty_warnings();
        let bm = convert_blend_mode("SOMETHING_NEW", &mut w);
        assert_eq!(bm, BlendMode::Normal);
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("SOMETHING_NEW"));
    }

    // ── convert_fill ────────────────────────────────────────────────────

    #[test]
    fn test_fill_solid() {
        let mut w = empty_warnings();
        let paint = FigmaPaint {
            paint_type: "SOLID".to_string(),
            color: Some(FigmaColor {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            opacity: Some(0.5),
            ..Default::default()
        };
        let fill = convert_fill(&paint, &empty_var_map(), &mut w).expect("should produce a fill");
        assert!(w.is_empty());
        match &fill.paint {
            Paint::Solid { color } => {
                let c = color.value();
                assert_eq!(
                    c,
                    Color::Srgb {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0
                    }
                );
            }
            _ => panic!("Expected Solid"),
        }
        assert!((fill.opacity.value() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_fill_linear_gradient() {
        let mut w = empty_warnings();
        let paint = FigmaPaint {
            paint_type: "GRADIENT_LINEAR".to_string(),
            gradient_handle_positions: Some(vec![
                FigmaVector { x: 0.0, y: 0.5 },
                FigmaVector { x: 1.0, y: 0.5 },
                FigmaVector { x: 0.0, y: 1.0 },
            ]),
            gradient_stops: Some(vec![
                FigmaColorStop {
                    position: 0.0,
                    color: FigmaColor {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    },
                    bound_variables: None,
                },
                FigmaColorStop {
                    position: 1.0,
                    color: FigmaColor {
                        r: 0.0,
                        g: 0.0,
                        b: 1.0,
                        a: 1.0,
                    },
                    bound_variables: None,
                },
            ]),
            ..Default::default()
        };
        let fill = convert_fill(&paint, &empty_var_map(), &mut w).expect("should produce a fill");
        assert!(w.is_empty());
        match &fill.paint {
            Paint::LinearGradient { stops, start, end } => {
                assert_eq!(stops.len(), 2);
                assert!((start.x - 0.0).abs() < f32::EPSILON);
                assert!((start.y - 0.5).abs() < f32::EPSILON);
                assert!((end.x - 1.0).abs() < f32::EPSILON);
                assert!((end.y - 0.5).abs() < f32::EPSILON);
            }
            _ => panic!("Expected LinearGradient"),
        }
    }

    #[test]
    fn test_fill_unsupported_video() {
        let mut w = empty_warnings();
        let paint = FigmaPaint {
            paint_type: "VIDEO".to_string(),
            ..Default::default()
        };
        let fill = convert_fill(&paint, &empty_var_map(), &mut w);
        assert!(fill.is_none());
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("VIDEO"));
    }

    #[test]
    fn test_fill_invisible_returns_none() {
        let mut w = empty_warnings();
        let paint = FigmaPaint {
            paint_type: "SOLID".to_string(),
            visible: Some(false),
            color: Some(FigmaColor {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            ..Default::default()
        };
        assert!(convert_fill(&paint, &empty_var_map(), &mut w).is_none());
        assert!(w.is_empty());
    }

    // ── convert_effect ──────────────────────────────────────────────────

    #[test]
    fn test_effect_drop_shadow() {
        let mut w = empty_warnings();
        let effect = FigmaEffect {
            effect_type: "DROP_SHADOW".to_string(),
            color: Some(FigmaColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.25,
            }),
            offset: Some(FigmaVector { x: 0.0, y: 4.0 }),
            radius: Some(8.0),
            spread: Some(0.0),
            visible: Some(true),
            ..Default::default()
        };
        let eff =
            convert_effect(&effect, &empty_var_map(), &mut w).expect("should produce an effect");
        assert!(w.is_empty());
        match &eff {
            Effect::DropShadow {
                color,
                offset,
                blur,
                spread,
            } => {
                assert_eq!(
                    color.value(),
                    Color::Srgb {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.25
                    }
                );
                assert!((offset.y - 4.0).abs() < f32::EPSILON);
                assert!((blur.value() - 8.0).abs() < f32::EPSILON);
                assert!((spread.value() - 0.0).abs() < f32::EPSILON);
            }
            _ => panic!("Expected DropShadow"),
        }
    }

    #[test]
    fn test_effect_layer_blur() {
        let mut w = empty_warnings();
        let effect = FigmaEffect {
            effect_type: "LAYER_BLUR".to_string(),
            radius: Some(10.0),
            visible: Some(true),
            ..Default::default()
        };
        let eff =
            convert_effect(&effect, &empty_var_map(), &mut w).expect("should produce an effect");
        assert!(w.is_empty());
        match &eff {
            Effect::LayerBlur { radius } => {
                assert!((radius.value() - 10.0).abs() < f32::EPSILON);
            }
            _ => panic!("Expected LayerBlur"),
        }
    }

    #[test]
    fn test_effect_invisible_returns_none() {
        let mut w = empty_warnings();
        let effect = FigmaEffect {
            effect_type: "DROP_SHADOW".to_string(),
            visible: Some(false),
            ..Default::default()
        };
        assert!(convert_effect(&effect, &empty_var_map(), &mut w).is_none());
        assert!(w.is_empty());
    }

    #[test]
    fn test_effect_unsupported_noise() {
        let mut w = empty_warnings();
        let effect = FigmaEffect {
            effect_type: "NOISE".to_string(),
            ..Default::default()
        };
        assert!(convert_effect(&effect, &empty_var_map(), &mut w).is_none());
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("NOISE"));
    }

    // ── convert_miter ───────────────────────────────────────────────────

    #[test]
    fn test_convert_miter_90_deg() {
        let ml = convert_miter(90.0);
        // 1/sin(45°) = 1/0.7071 ≈ 1.4142
        assert!((ml - std::f32::consts::SQRT_2).abs() < 0.001);
    }

    #[test]
    fn test_convert_miter_zero_deg() {
        let ml = convert_miter(0.0);
        assert!((ml - 100.0).abs() < f32::EPSILON);
    }

    // ── convert_image_fill_mode ─────────────────────────────────────────

    #[test]
    fn test_image_fill_mode_fill() {
        assert_eq!(convert_image_fill_mode("FILL"), ImageFillMode::Fill);
    }

    #[test]
    fn test_image_fill_mode_fit() {
        assert_eq!(convert_image_fill_mode("FIT"), ImageFillMode::Fit);
    }

    #[test]
    fn test_image_fill_mode_tile() {
        assert_eq!(convert_image_fill_mode("TILE"), ImageFillMode::Tile);
    }

    #[test]
    fn test_image_fill_mode_stretch_maps_to_fill() {
        assert_eq!(convert_image_fill_mode("STRETCH"), ImageFillMode::Fill);
    }

    #[test]
    fn test_image_fill_mode_unknown_maps_to_fill() {
        assert_eq!(convert_image_fill_mode("UNKNOWN"), ImageFillMode::Fill);
    }

    // ── convert_stroke ──────────────────────────────────────────────────

    #[test]
    fn test_stroke_with_inside_round_bevel() {
        let mut w = empty_warnings();
        let paint = FigmaPaint {
            paint_type: "SOLID".to_string(),
            color: Some(FigmaColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            opacity: Some(1.0),
            ..Default::default()
        };
        let stroke = convert_stroke(
            &paint,
            2.0,
            Some("INSIDE"),
            Some("ROUND"),
            Some("BEVEL"),
            None,
            Some(&[5.0, 3.0]),
            &empty_var_map(),
            &mut w,
        )
        .expect("should produce a stroke");
        assert!(w.is_empty());
        assert_eq!(stroke.position, StrokePosition::Inside);
        assert_eq!(stroke.cap, StrokeCap::Round);
        assert_eq!(stroke.join, StrokeJoin::Bevel);
        assert!((stroke.width.value() - 2.0).abs() < f32::EPSILON);
        assert!((stroke.miter_limit - 4.0).abs() < f32::EPSILON); // default
        let dash = stroke.dash.as_ref().expect("dash should be set");
        assert_eq!(dash.segments, vec![5.0, 3.0]);
    }

    #[test]
    fn test_stroke_invisible_returns_none() {
        let mut w = empty_warnings();
        let paint = FigmaPaint {
            paint_type: "SOLID".to_string(),
            visible: Some(false),
            color: Some(FigmaColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            ..Default::default()
        };
        assert!(
            convert_stroke(
                &paint,
                1.0,
                None,
                None,
                None,
                None,
                None,
                &empty_var_map(),
                &mut w
            )
            .is_none()
        );
    }

    #[test]
    fn test_stroke_arrow_cap_warning() {
        let mut w = empty_warnings();
        let paint = FigmaPaint {
            paint_type: "SOLID".to_string(),
            color: Some(FigmaColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            ..Default::default()
        };
        let stroke = convert_stroke(
            &paint,
            1.0,
            None,
            Some("TRIANGLE_ARROW"),
            None,
            None,
            None,
            &empty_var_map(),
            &mut w,
        )
        .expect("should produce a stroke");
        assert_eq!(stroke.cap, StrokeCap::Butt);
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("TRIANGLE_ARROW"));
    }

    // ── boundVariables → StyleValue::Bound ────────────────────────────

    #[test]
    fn bound_variables_produce_style_value_bound() {
        use super::super::types::FigmaVariableAlias;

        let mut w = empty_warnings();
        let paint = FigmaPaint {
            paint_type: "SOLID".to_string(),
            color: Some(FigmaColor {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            bound_variables: Some(HashMap::from([(
                "color".to_string(),
                FigmaVariableAlias {
                    alias_type: "VARIABLE_ALIAS".to_string(),
                    id: "V:1".to_string(),
                },
            )])),
            ..Default::default()
        };
        let variable_map: VariableMap = HashMap::from([("V:1".to_string(), (0u32, 0u32))]);
        let fill = convert_fill(&paint, &variable_map, &mut w).unwrap();
        assert!(w.is_empty());
        match &fill.paint {
            Paint::Solid { color } => {
                assert!(color.is_bound(), "Color should be bound to a token");
            }
            _ => panic!("Expected Solid paint"),
        }
    }

    #[test]
    fn unmatched_bound_variable_stays_raw() {
        use super::super::types::FigmaVariableAlias;

        let mut w = empty_warnings();
        let paint = FigmaPaint {
            paint_type: "SOLID".to_string(),
            color: Some(FigmaColor {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            bound_variables: Some(HashMap::from([(
                "color".to_string(),
                FigmaVariableAlias {
                    alias_type: "VARIABLE_ALIAS".to_string(),
                    id: "V:999".to_string(),
                },
            )])),
            ..Default::default()
        };
        // variable_map doesn't contain V:999
        let variable_map: VariableMap = HashMap::new();
        let fill = convert_fill(&paint, &variable_map, &mut w).unwrap();
        match &fill.paint {
            Paint::Solid { color } => {
                assert!(
                    !color.is_bound(),
                    "Color should remain raw when variable not found"
                );
            }
            _ => panic!("Expected Solid paint"),
        }
    }

    #[test]
    fn bound_effect_color_produces_bound() {
        use super::super::types::FigmaVariableAlias;

        let mut w = empty_warnings();
        let effect = FigmaEffect {
            effect_type: "DROP_SHADOW".to_string(),
            color: Some(FigmaColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.25,
            }),
            offset: Some(FigmaVector { x: 0.0, y: 4.0 }),
            radius: Some(8.0),
            spread: Some(0.0),
            visible: Some(true),
            bound_variables: Some(HashMap::from([(
                "color".to_string(),
                FigmaVariableAlias {
                    alias_type: "VARIABLE_ALIAS".to_string(),
                    id: "V:2".to_string(),
                },
            )])),
            ..Default::default()
        };
        let variable_map: VariableMap = HashMap::from([("V:2".to_string(), (0u32, 1u32))]);
        let eff = convert_effect(&effect, &variable_map, &mut w).unwrap();
        match &eff {
            Effect::DropShadow { color, .. } => {
                assert!(color.is_bound(), "Effect color should be bound");
            }
            _ => panic!("Expected DropShadow"),
        }
    }
}
