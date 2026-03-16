//! Figma layout conversion: auto layout config, sizing, constraints, transform.

use ode_format::node::{
    ConstraintAxis, Constraints, CounterAxisAlign, LayoutConfig, LayoutDirection, LayoutPadding,
    LayoutSizing, LayoutWrap, PrimaryAxisAlign, SizingMode, Transform,
};

use super::types::FigmaLayoutConstraint;
use crate::error::ImportWarning;

// ─── convert_layout_config ───────────────────────────────────────────────────

/// Convert Figma auto-layout properties into an ODE `LayoutConfig`.
///
/// Returns `None` if `layout_mode` is `None`, `"NONE"`, or `"GRID"` (with a
/// warning for GRID).
#[allow(clippy::too_many_arguments)]
pub fn convert_layout_config(
    layout_mode: Option<&str>,
    primary_align: Option<&str>,
    counter_align: Option<&str>,
    pad_top: Option<f32>,
    pad_right: Option<f32>,
    pad_bottom: Option<f32>,
    pad_left: Option<f32>,
    item_spacing: Option<f32>,
    wrap: Option<&str>,
    warnings: &mut Vec<ImportWarning>,
) -> Option<LayoutConfig> {
    let mode = layout_mode?;

    match mode {
        "NONE" => return None,
        "GRID" => {
            warnings.push(ImportWarning {
                node_id: String::new(),
                node_name: String::new(),
                message: "Grid layout mode is not supported, skipping layout".to_string(),
            });
            return None;
        }
        _ => {}
    }

    let direction = match mode {
        "VERTICAL" => LayoutDirection::Vertical,
        _ => LayoutDirection::Horizontal,
    };

    let primary_axis_align = match primary_align {
        Some("CENTER") => PrimaryAxisAlign::Center,
        Some("MAX") => PrimaryAxisAlign::End,
        Some("SPACE_BETWEEN") => PrimaryAxisAlign::SpaceBetween,
        _ => PrimaryAxisAlign::Start,
    };

    let counter_axis_align = match counter_align {
        Some("CENTER") => CounterAxisAlign::Center,
        Some("MAX") => CounterAxisAlign::End,
        Some("BASELINE") => CounterAxisAlign::Baseline,
        Some("STRETCH") => CounterAxisAlign::Stretch,
        _ => CounterAxisAlign::Start,
    };

    let padding = LayoutPadding {
        top: pad_top.unwrap_or(0.0),
        right: pad_right.unwrap_or(0.0),
        bottom: pad_bottom.unwrap_or(0.0),
        left: pad_left.unwrap_or(0.0),
    };

    let layout_wrap = match wrap {
        Some("WRAP") => LayoutWrap::Wrap,
        _ => LayoutWrap::NoWrap,
    };

    Some(LayoutConfig {
        direction,
        primary_axis_align,
        counter_axis_align,
        padding,
        item_spacing: item_spacing.unwrap_or(0.0),
        wrap: layout_wrap,
    })
}

// ─── convert_layout_sizing ───────────────────────────────────────────────────

/// Convert Figma per-axis sizing and alignment into an ODE `LayoutSizing`.
pub fn convert_layout_sizing(
    h_sizing: Option<&str>,
    v_sizing: Option<&str>,
    layout_align: Option<&str>,
    min_w: Option<f32>,
    max_w: Option<f32>,
    min_h: Option<f32>,
    max_h: Option<f32>,
) -> LayoutSizing {
    let width = match h_sizing {
        Some("HUG") => SizingMode::Hug,
        Some("FILL") => SizingMode::Fill,
        _ => SizingMode::Fixed,
    };

    let height = match v_sizing {
        Some("HUG") => SizingMode::Hug,
        Some("FILL") => SizingMode::Fill,
        _ => SizingMode::Fixed,
    };

    let align_self = match layout_align {
        Some("STRETCH") => Some(CounterAxisAlign::Stretch),
        _ => None,
    };

    LayoutSizing {
        width,
        height,
        align_self,
        min_width: min_w,
        max_width: max_w,
        min_height: min_h,
        max_height: max_h,
    }
}

// ─── convert_constraints ─────────────────────────────────────────────────────

/// Convert a Figma `LayoutConstraint` to ODE `Constraints`.
pub fn convert_constraints(c: &FigmaLayoutConstraint) -> Constraints {
    Constraints {
        horizontal: convert_constraint_axis(&c.horizontal, true),
        vertical: convert_constraint_axis(&c.vertical, false),
    }
}

/// Map a single Figma constraint string to an ODE `ConstraintAxis`.
fn convert_constraint_axis(s: &str, _is_horizontal: bool) -> ConstraintAxis {
    match s {
        "LEFT" | "TOP" => ConstraintAxis::Start,
        "RIGHT" | "BOTTOM" => ConstraintAxis::End,
        "CENTER" => ConstraintAxis::Center,
        "LEFT_RIGHT" | "TOP_BOTTOM" => ConstraintAxis::StartEnd,
        "SCALE" => ConstraintAxis::Scale,
        _ => ConstraintAxis::Start,
    }
}

// ─── convert_transform ───────────────────────────────────────────────────────

/// Convert a Figma 2x3 affine transform matrix to an ODE `Transform`.
///
/// Figma format: `[[a, c, tx], [b, d, ty]]`
/// ODE format:   `Transform { a, b, c, d, tx, ty }`
pub fn convert_transform(ft: &[[f64; 3]; 2]) -> Transform {
    Transform {
        a: ft[0][0] as f32,
        b: ft[1][0] as f32,
        c: ft[0][1] as f32,
        d: ft[1][1] as f32,
        tx: ft[0][2] as f32,
        ty: ft[1][2] as f32,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_warnings() -> Vec<ImportWarning> {
        Vec::new()
    }

    #[test]
    fn test_layout_config_horizontal_with_padding_and_spacing() {
        let mut w = empty_warnings();
        let config = convert_layout_config(
            Some("HORIZONTAL"),
            Some("SPACE_BETWEEN"),
            Some("CENTER"),
            Some(8.0),
            Some(16.0),
            Some(8.0),
            Some(16.0),
            Some(12.0),
            Some("WRAP"),
            &mut w,
        );
        assert!(w.is_empty());
        let config = config.expect("should produce a LayoutConfig");
        assert_eq!(config.direction, LayoutDirection::Horizontal);
        assert_eq!(config.primary_axis_align, PrimaryAxisAlign::SpaceBetween);
        assert_eq!(config.counter_axis_align, CounterAxisAlign::Center);
        assert!((config.padding.top - 8.0).abs() < f32::EPSILON);
        assert!((config.padding.right - 16.0).abs() < f32::EPSILON);
        assert!((config.padding.bottom - 8.0).abs() < f32::EPSILON);
        assert!((config.padding.left - 16.0).abs() < f32::EPSILON);
        assert!((config.item_spacing - 12.0).abs() < f32::EPSILON);
        assert_eq!(config.wrap, LayoutWrap::Wrap);
    }

    #[test]
    fn test_layout_config_returns_none_for_none_mode() {
        let mut w = empty_warnings();
        let config = convert_layout_config(
            Some("NONE"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &mut w,
        );
        assert!(config.is_none());
        assert!(w.is_empty());
    }

    #[test]
    fn test_layout_config_returns_none_for_missing_mode() {
        let mut w = empty_warnings();
        let config =
            convert_layout_config(None, None, None, None, None, None, None, None, None, &mut w);
        assert!(config.is_none());
        assert!(w.is_empty());
    }

    #[test]
    fn test_layout_config_warns_for_grid() {
        let mut w = empty_warnings();
        let config = convert_layout_config(
            Some("GRID"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            &mut w,
        );
        assert!(config.is_none());
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("Grid"));
    }

    #[test]
    fn test_layout_config_vertical() {
        let mut w = empty_warnings();
        let config = convert_layout_config(
            Some("VERTICAL"),
            Some("MIN"),
            Some("STRETCH"),
            None,
            None,
            None,
            None,
            Some(4.0),
            None,
            &mut w,
        )
        .unwrap();
        assert_eq!(config.direction, LayoutDirection::Vertical);
        assert_eq!(config.primary_axis_align, PrimaryAxisAlign::Start);
        assert_eq!(config.counter_axis_align, CounterAxisAlign::Stretch);
        assert!((config.item_spacing - 4.0).abs() < f32::EPSILON);
        assert_eq!(config.wrap, LayoutWrap::NoWrap);
    }

    #[test]
    fn test_layout_sizing_hug_fill_stretch() {
        let sizing = convert_layout_sizing(
            Some("HUG"),
            Some("FILL"),
            Some("STRETCH"),
            Some(50.0),
            Some(200.0),
            None,
            None,
        );
        assert_eq!(sizing.width, SizingMode::Hug);
        assert_eq!(sizing.height, SizingMode::Fill);
        assert_eq!(sizing.align_self, Some(CounterAxisAlign::Stretch));
        assert_eq!(sizing.min_width, Some(50.0));
        assert_eq!(sizing.max_width, Some(200.0));
        assert!(sizing.min_height.is_none());
        assert!(sizing.max_height.is_none());
    }

    #[test]
    fn test_layout_sizing_defaults() {
        let sizing = convert_layout_sizing(None, None, None, None, None, None, None);
        assert_eq!(sizing.width, SizingMode::Fixed);
        assert_eq!(sizing.height, SizingMode::Fixed);
        assert!(sizing.align_self.is_none());
    }

    #[test]
    fn test_convert_constraints_mapping() {
        let c = FigmaLayoutConstraint {
            vertical: "TOP_BOTTOM".to_string(),
            horizontal: "CENTER".to_string(),
        };
        let constraints = convert_constraints(&c);
        assert_eq!(constraints.horizontal, ConstraintAxis::Center);
        assert_eq!(constraints.vertical, ConstraintAxis::StartEnd);
    }

    #[test]
    fn test_convert_constraints_scale() {
        let c = FigmaLayoutConstraint {
            vertical: "SCALE".to_string(),
            horizontal: "SCALE".to_string(),
        };
        let constraints = convert_constraints(&c);
        assert_eq!(constraints.horizontal, ConstraintAxis::Scale);
        assert_eq!(constraints.vertical, ConstraintAxis::Scale);
    }

    #[test]
    fn test_convert_constraints_fixed_edges() {
        let c = FigmaLayoutConstraint {
            vertical: "TOP".to_string(),
            horizontal: "LEFT".to_string(),
        };
        let constraints = convert_constraints(&c);
        assert_eq!(constraints.horizontal, ConstraintAxis::Start);
        assert_eq!(constraints.vertical, ConstraintAxis::Start);

        let c2 = FigmaLayoutConstraint {
            vertical: "BOTTOM".to_string(),
            horizontal: "RIGHT".to_string(),
        };
        let constraints2 = convert_constraints(&c2);
        assert_eq!(constraints2.horizontal, ConstraintAxis::End);
        assert_eq!(constraints2.vertical, ConstraintAxis::End);
    }

    #[test]
    fn test_convert_transform_matrix() {
        let ft: [[f64; 3]; 2] = [[1.0, 0.0, 100.0], [0.0, 1.0, 200.0]];
        let t = convert_transform(&ft);
        assert!((t.a - 1.0).abs() < f32::EPSILON);
        assert!((t.b - 0.0).abs() < f32::EPSILON);
        assert!((t.c - 0.0).abs() < f32::EPSILON);
        assert!((t.d - 1.0).abs() < f32::EPSILON);
        assert!((t.tx - 100.0).abs() < f32::EPSILON);
        assert!((t.ty - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_convert_transform_rotation() {
        // 90-degree rotation: a=0, b=1, c=-1, d=0
        let ft: [[f64; 3]; 2] = [[0.0, -1.0, 50.0], [1.0, 0.0, 75.0]];
        let t = convert_transform(&ft);
        assert!((t.a - 0.0).abs() < f32::EPSILON);
        assert!((t.b - 1.0).abs() < f32::EPSILON);
        assert!((t.c - (-1.0)).abs() < f32::EPSILON);
        assert!((t.d - 0.0).abs() < f32::EPSILON);
        assert!((t.tx - 50.0).abs() < f32::EPSILON);
        assert!((t.ty - 75.0).abs() < f32::EPSILON);
    }
}
