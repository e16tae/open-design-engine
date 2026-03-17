//! WCAG contrast ratio checker.
//!
//! Verifies that foreground text/shape colors have sufficient contrast
//! against their background, per WCAG 2.x guidelines.

use crate::checker::{CheckContext, Checker};
use crate::result::CheckerIssue;
use crate::traverse;
use ode_format::color::Color;
use ode_format::style::Paint;
use std::collections::HashMap;

pub struct ContrastRatioChecker;

impl Checker for ContrastRatioChecker {
    fn name(&self) -> &'static str {
        "contrast_ratio"
    }

    fn check(&self, ctx: &CheckContext) -> Vec<CheckerIssue> {
        let min_ratio = ctx.params["min_ratio"].as_f64().unwrap_or(4.5) as f32;

        let mut issues = Vec::new();

        for (node_id, node) in ctx.doc.nodes.iter() {
            // Skip invisible nodes.
            if !node.visible {
                continue;
            }

            // Filter by applies_to node kinds.
            let kind_name = traverse::node_kind_name(&node.kind);
            if !ctx.applies_to.node_kinds.is_empty()
                && !ctx.applies_to.node_kinds.iter().any(|k| k == kind_name)
            {
                continue;
            }

            // Extract foreground color from the node's first visible solid fill.
            let fg_color = match node.kind.visual() {
                Some(visual) => {
                    let mut found = None;
                    for fill in &visual.fills {
                        if fill.visible {
                            if let Paint::Solid { ref color } = fill.paint {
                                found = Some(color.value());
                                break;
                            }
                        }
                    }
                    match found {
                        Some(c) => c,
                        None => continue, // No solid fill to check.
                    }
                }
                None => continue,
            };

            // Find background color by walking ancestors (start from parent, not the
            // node itself, since the node's own fill is the foreground).
            let bg_start = ctx.parent_map.get(&node_id).copied().unwrap_or(node_id);
            let bg_color = traverse::find_background_color(ctx.doc, bg_start, ctx.parent_map);

            let ratio = contrast_ratio(&fg_color, &bg_color);

            if ratio < min_ratio {
                let path = traverse::node_path(ctx.doc, node_id);
                let mut template_vars = HashMap::new();
                template_vars.insert("actual".to_string(), format!("{ratio:.2}"));
                template_vars.insert("min_ratio".to_string(), format!("{min_ratio}"));
                issues.push(CheckerIssue {
                    path,
                    template_vars,
                });
            }
        }

        issues
    }
}

/// Linearize a single sRGB channel value (0.0–1.0) to linear light.
fn linearize(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Calculate relative luminance per WCAG 2.x from a Color.
fn relative_luminance(color: &Color) -> f32 {
    let rgba = color.to_rgba_u8();
    let r = linearize(rgba[0] as f32 / 255.0);
    let g = linearize(rgba[1] as f32 / 255.0);
    let b = linearize(rgba[2] as f32 / 255.0);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Calculate WCAG 2.x contrast ratio between two colors.
fn contrast_ratio(fg: &Color, bg: &Color) -> f32 {
    let lum_fg = relative_luminance(fg);
    let lum_bg = relative_luminance(bg);
    let lighter = lum_fg.max(lum_bg);
    let darker = lum_fg.min(lum_bg);
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::CheckContext;
    use crate::rule::AppliesTo;
    use crate::traverse::build_parent_map;
    use ode_format::node::{Node, NodeKind};
    use ode_format::style::{BlendMode, Fill, StyleValue};
    use ode_format::Document;

    /// Helper: create a text node with a solid fill color, inside a white-background frame.
    fn make_doc_with_text_color(hex: &str) -> (Document, AppliesTo) {
        let mut doc = Document::new("Test");

        let mut text = Node::new_text("Label", "Hello");
        if let NodeKind::Text(ref mut data) = text.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(Color::from_hex(hex).unwrap()),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let text_id = doc.nodes.insert(text);

        // Parent frame with white background.
        let mut frame = Node::new_frame("Card", 200.0, 200.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(Color::white()),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
            data.container.children.push(text_id);
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let applies_to = AppliesTo {
            node_kinds: vec!["text".to_string()],
            contexts: vec![],
        };

        (doc, applies_to)
    }

    #[test]
    fn high_contrast_passes() {
        // Black text (#000000) on white background -> ratio ~21:1
        let (doc, applies_to) = make_doc_with_text_color("#000000");
        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({ "min_ratio": 4.5 });

        let ctx = CheckContext {
            doc: &doc,
            parent_map: &parent_map,
            params: &params,
            applies_to: &applies_to,
        };

        let checker = ContrastRatioChecker;
        let issues = checker.check(&ctx);
        assert!(issues.is_empty(), "Black on white should pass 4.5:1");
    }

    #[test]
    fn low_contrast_fails() {
        // Light gray text (#CCCCCC) on white background -> ratio ~1.6:1
        let (doc, applies_to) = make_doc_with_text_color("#CCCCCC");
        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({ "min_ratio": 4.5 });

        let ctx = CheckContext {
            doc: &doc,
            parent_map: &parent_map,
            params: &params,
            applies_to: &applies_to,
        };

        let checker = ContrastRatioChecker;
        let issues = checker.check(&ctx);
        assert_eq!(issues.len(), 1, "Light gray on white should fail 4.5:1");
        assert!(
            issues[0].template_vars.contains_key("actual"),
            "Issue should contain 'actual' template var"
        );
    }

    #[test]
    fn linearize_thresholds() {
        // Below threshold
        assert!((linearize(0.0) - 0.0).abs() < 1e-6);
        // Above threshold
        assert!((linearize(1.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn contrast_ratio_black_on_white() {
        let ratio = contrast_ratio(&Color::black(), &Color::white());
        assert!(ratio > 20.0, "Black on white should be ~21:1, got {ratio}");
    }

    #[test]
    fn contrast_ratio_white_on_white() {
        let ratio = contrast_ratio(&Color::white(), &Color::white());
        assert!(
            (ratio - 1.0).abs() < 0.01,
            "Same colors should be 1:1, got {ratio}"
        );
    }
}
