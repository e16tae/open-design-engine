//! Minimum property value checker.
//!
//! Validates that a specified property (width, height, font_size) on matching
//! nodes meets a minimum threshold.

use crate::checker::{CheckContext, Checker};
use crate::result::CheckerIssue;
use crate::traverse;
use ode_format::node::NodeKind;
use std::collections::HashMap;

pub struct MinValueChecker;

impl Checker for MinValueChecker {
    fn name(&self) -> &'static str {
        "min_value"
    }

    fn check(&self, ctx: &CheckContext) -> Vec<CheckerIssue> {
        let property = ctx.params["property"]
            .as_str()
            .unwrap_or("width");
        let min = ctx.params["min"].as_f64().unwrap_or(0.0) as f32;

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

            let actual = match property {
                "width" => match &node.kind {
                    NodeKind::Frame(data) => Some(data.width),
                    NodeKind::Image(data) => Some(data.width),
                    _ => None,
                },
                "height" => match &node.kind {
                    NodeKind::Frame(data) => Some(data.height),
                    NodeKind::Image(data) => Some(data.height),
                    _ => None,
                },
                "font_size" => match &node.kind {
                    NodeKind::Text(data) => Some(data.default_style.font_size.value()),
                    _ => None,
                },
                _ => None,
            };

            if let Some(value) = actual {
                if value < min {
                    let path = traverse::node_path(ctx.doc, node_id);
                    let mut template_vars = HashMap::new();
                    template_vars.insert("actual".to_string(), format!("{value}"));
                    template_vars.insert("min".to_string(), format!("{min}"));
                    template_vars.insert("property".to_string(), property.to_string());
                    issues.push(CheckerIssue {
                        path,
                        template_vars,
                    });
                }
            }
        }

        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::CheckContext;
    use crate::rule::AppliesTo;
    use crate::traverse::build_parent_map;
    use ode_format::Document;
    use ode_format::node::Node;

    #[test]
    fn frame_passes_min_width() {
        let mut doc = Document::new("Test");
        let frame = Node::new_frame("Button", 48.0, 48.0);
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({ "property": "width", "min": 44.0 });
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

        let checker = MinValueChecker;
        let issues = checker.check(&ctx);
        assert!(issues.is_empty(), "48x48 should pass min 44");
    }

    #[test]
    fn frame_fails_min_width() {
        let mut doc = Document::new("Test");
        let frame = Node::new_frame("SmallButton", 30.0, 30.0);
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({ "property": "width", "min": 44.0 });
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

        let checker = MinValueChecker;
        let issues = checker.check(&ctx);
        assert_eq!(issues.len(), 1, "30x30 should fail min 44");
        assert_eq!(issues[0].template_vars["actual"], "30");
        assert_eq!(issues[0].template_vars["min"], "44");
    }

    #[test]
    fn font_size_check() {
        let mut doc = Document::new("Test");
        let text = Node::new_text("Small", "Tiny text");
        // Default font_size is 16.0 (from TextStyle::default)
        let text_id = doc.nodes.insert(text);
        doc.canvas.push(text_id);

        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({ "property": "font_size", "min": 12.0 });
        let applies_to = AppliesTo {
            node_kinds: vec!["text".to_string()],
            contexts: vec![],
        };

        let ctx = CheckContext {
            doc: &doc,
            parent_map: &parent_map,
            params: &params,
            applies_to: &applies_to,
        };

        let checker = MinValueChecker;
        let issues = checker.check(&ctx);
        assert!(issues.is_empty(), "Default 16px should pass min 12");
    }
}
