//! Spacing scale checker.
//!
//! Validates that item_spacing values in auto-layout frames adhere to a base
//! multiple (e.g. 8px grid), within a configurable tolerance.

use crate::checker::{CheckContext, Checker};
use crate::result::CheckerIssue;
use crate::traverse;
use ode_format::node::NodeKind;
use std::collections::HashMap;

pub struct SpacingScaleChecker;

impl Checker for SpacingScaleChecker {
    fn name(&self) -> &'static str {
        "spacing_scale"
    }

    fn check(&self, ctx: &CheckContext) -> Vec<CheckerIssue> {
        let base = ctx.params["base"].as_f64().unwrap_or(8.0) as f32;
        let tolerance = ctx.params["tolerance"].as_f64().unwrap_or(0.5) as f32;

        let mut issues = Vec::new();

        for (node_id, node) in ctx.doc.nodes.iter() {
            if !node.visible {
                continue;
            }

            let kind_name = traverse::node_kind_name(&node.kind);
            if !ctx.applies_to.node_kinds.is_empty()
                && !ctx.applies_to.node_kinds.iter().any(|k| k == kind_name)
            {
                continue;
            }

            // Only check Frame nodes with layout.
            if let NodeKind::Frame(data) = &node.kind {
                if let Some(ref layout) = data.container.layout {
                    let spacing = layout.item_spacing;

                    // Skip zero spacing — no issue.
                    if spacing.abs() < f32::EPSILON {
                        continue;
                    }

                    if !is_on_scale(spacing, base, tolerance) {
                        let path = traverse::node_path(ctx.doc, node_id);
                        let mut template_vars = HashMap::new();
                        template_vars.insert("actual".to_string(), format!("{spacing}"));
                        template_vars.insert("base".to_string(), format!("{base}"));
                        issues.push(CheckerIssue {
                            path,
                            template_vars,
                        });
                    }
                }
            }
        }

        issues
    }
}

/// Check whether `value` is on a scale defined by `base` within `tolerance`.
///
/// Returns `true` if the remainder of `value / base` is within `tolerance`
/// of zero (or of `base` itself, for values near the next multiple).
fn is_on_scale(value: f32, base: f32, tolerance: f32) -> bool {
    if base.abs() < f32::EPSILON {
        return true; // Avoid division by zero.
    }
    let remainder = value.abs() % base;
    remainder <= tolerance || (base - remainder) <= tolerance
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::CheckContext;
    use crate::rule::AppliesTo;
    use crate::traverse::build_parent_map;
    use ode_format::node::{
        LayoutConfig, LayoutDirection, LayoutPadding, LayoutWrap, Node, NodeKind,
        PrimaryAxisAlign, CounterAxisAlign,
    };
    use ode_format::Document;

    fn make_frame_with_spacing(name: &str, spacing: f32) -> Node {
        let mut frame = Node::new_frame(name, 200.0, 200.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.container.layout = Some(LayoutConfig {
                direction: LayoutDirection::default(),
                primary_axis_align: PrimaryAxisAlign::default(),
                counter_axis_align: CounterAxisAlign::default(),
                padding: LayoutPadding::default(),
                item_spacing: spacing,
                wrap: LayoutWrap::default(),
            });
        }
        frame
    }

    #[test]
    fn on_scale_values() {
        assert!(is_on_scale(8.0, 8.0, 0.5));
        assert!(is_on_scale(16.0, 8.0, 0.5));
        assert!(is_on_scale(24.0, 8.0, 0.5));
    }

    #[test]
    fn off_scale_values() {
        assert!(!is_on_scale(10.0, 8.0, 0.5));
        assert!(!is_on_scale(13.0, 8.0, 0.5));
    }

    #[test]
    fn negative_values_use_absolute() {
        assert!(is_on_scale(-8.0, 8.0, 0.5));
        assert!(is_on_scale(-16.0, 8.0, 0.5));
        assert!(!is_on_scale(-10.0, 8.0, 0.5));
    }

    #[test]
    fn spacing_on_scale_passes() {
        let mut doc = Document::new("Test");
        let frame = make_frame_with_spacing("Grid", 16.0);
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({ "base": 8.0, "tolerance": 0.5 });
        let applies_to = AppliesTo {
            node_kinds: vec!["frame".to_string()],
            contexts: vec![],
        };

        let ctx = CheckContext {
            doc: &doc,
            parent_map: &parent_map,
            params: &params,
            applies_to: &applies_to,
        };

        let checker = SpacingScaleChecker;
        let issues = checker.check(&ctx);
        assert!(issues.is_empty(), "16px spacing should be on 8px grid");
    }

    #[test]
    fn spacing_off_scale_fails() {
        let mut doc = Document::new("Test");
        let frame = make_frame_with_spacing("Messy", 10.0);
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({ "base": 8.0, "tolerance": 0.5 });
        let applies_to = AppliesTo {
            node_kinds: vec!["frame".to_string()],
            contexts: vec![],
        };

        let ctx = CheckContext {
            doc: &doc,
            parent_map: &parent_map,
            params: &params,
            applies_to: &applies_to,
        };

        let checker = SpacingScaleChecker;
        let issues = checker.check(&ctx);
        assert_eq!(issues.len(), 1, "10px spacing should fail 8px grid");
        assert_eq!(issues[0].template_vars["actual"], "10");
        assert_eq!(issues[0].template_vars["base"], "8");
    }

    #[test]
    fn frame_without_layout_is_skipped() {
        let mut doc = Document::new("Test");
        // Frame without layout (no auto-layout).
        let frame = Node::new_frame("Static", 200.0, 200.0);
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({ "base": 8.0 });
        let applies_to = AppliesTo {
            node_kinds: vec!["frame".to_string()],
            contexts: vec![],
        };

        let ctx = CheckContext {
            doc: &doc,
            parent_map: &parent_map,
            params: &params,
            applies_to: &applies_to,
        };

        let checker = SpacingScaleChecker;
        let issues = checker.check(&ctx);
        assert!(issues.is_empty(), "Frame without layout should be skipped");
    }
}
