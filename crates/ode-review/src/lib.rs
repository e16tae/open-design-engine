//! ODE design rules engine — validate designs against knowledge-based rules.
//!
//! This crate provides the `review_document()` entry point that evaluates a set
//! of JSON-defined rules against an `ode_format::Document`, producing a
//! [`result::ReviewResult`] with issues, summaries, and skipped-rule tracking.

pub mod checker;
pub mod checkers;
pub mod context;
pub mod result;
pub mod rule;
pub mod traverse;

// Re-exports for convenience.
pub use checker::CheckerRegistry;
pub use result::{CheckerIssue, ReviewIssue, ReviewResult, ReviewSummary};
pub use rule::{load_rules_from_dir, load_rules_from_paths, Rule};

use ode_format::Document;

/// Evaluate a set of rules against a document, returning a structured review result.
///
/// - `doc`: the design document to review.
/// - `rules`: the set of rules to evaluate.
/// - `context`: if `Some`, forces a single context label; if `None`, contexts are
///   auto-detected from the document's views via [`context::detect_context`].
/// - `registry`: the checker registry that maps checker names to implementations.
pub fn review_document(
    doc: &Document,
    rules: &[Rule],
    context: Option<&str>,
    registry: &CheckerRegistry,
) -> ReviewResult {
    let contexts = match context {
        Some(c) => vec![c.to_string()],
        None => context::detect_context(doc),
    };

    let parent_map = traverse::build_parent_map(doc);
    let mut issues = Vec::new();
    let mut rules_run: usize = 0;
    let mut passed: usize = 0;
    let mut skipped_rules = Vec::new();

    for rule in rules {
        if !rule.applies_to_any_context(&contexts) {
            continue;
        }

        match registry.run(&rule.checker, &rule.params, doc, &parent_map, &rule.applies_to) {
            Ok(checker_issues) => {
                rules_run += 1;
                if checker_issues.is_empty() {
                    passed += 1;
                } else {
                    for ci in checker_issues {
                        issues.push(result::ReviewIssue {
                            severity: rule.severity.clone(),
                            code: rule.id.clone(),
                            layer: rule.layer.clone(),
                            path: ci.path,
                            message: rule.render_message(&ci.template_vars),
                            suggestion: rule.render_suggestion(&ci.template_vars),
                        });
                    }
                }
            }
            Err(_) => {
                skipped_rules.push(rule.id.clone());
            }
        }
    }

    let total = rules_run;
    let errors = issues.iter().filter(|i| i.severity == "error").count();
    let warnings = issues.iter().filter(|i| i.severity == "warning").count();
    let infos = issues.iter().filter(|i| i.severity == "info").count();

    ReviewResult {
        contexts,
        summary: ReviewSummary {
            errors,
            warnings,
            infos,
            passed,
            total,
        },
        issues,
        skipped_rules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::color::Color;
    use ode_format::node::{Node, NodeKind};
    use ode_format::style::{BlendMode, Fill, Paint, StyleValue};

    #[test]
    fn review_catches_low_contrast_text() {
        // Build a doc: white frame containing light gray (#CCCCCC) text.
        let mut doc = Document::new("ContrastTest");

        let mut text = Node::new_text("LightText", "Hello");
        if let NodeKind::Text(ref mut data) = text.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(Color::from_hex("#CCCCCC").unwrap()),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let text_id = doc.nodes.insert(text);

        let mut frame = Node::new_frame("Background", 400.0, 400.0);
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

        // Build a contrast-ratio rule targeting text nodes in "web" context.
        let rule: Rule = serde_json::from_value(serde_json::json!({
            "id": "contrast-min",
            "layer": "visual",
            "severity": "error",
            "checker": "contrast_ratio",
            "params": { "min_ratio": 4.5 },
            "applies_to": {
                "node_kinds": ["text"],
                "contexts": ["web"]
            },
            "message": "Contrast ratio {actual} is below minimum {min_ratio}:1",
            "suggestion": "Increase contrast to at least {min_ratio}:1"
        }))
        .unwrap();

        let registry = checkers::default_registry();
        let result = review_document(&doc, &[rule], Some("web"), &registry);

        assert_eq!(result.summary.errors, 1, "Should find 1 contrast error");
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].code, "contrast-min");
        assert!(
            result.issues[0].message.contains(":1"),
            "Message should contain ':1' ratio suffix, got: {}",
            result.issues[0].message
        );
        assert_eq!(result.summary.passed, 0);
        assert_eq!(result.summary.total, 1);
    }

    #[test]
    fn unknown_checker_is_skipped() {
        let doc = Document::new("Empty");

        let rule: Rule = serde_json::from_value(serde_json::json!({
            "id": "bogus-rule",
            "layer": "visual",
            "severity": "error",
            "checker": "does_not_exist",
            "message": "should never appear"
        }))
        .unwrap();

        let registry = checkers::default_registry();
        let result = review_document(&doc, &[rule], None, &registry);

        assert!(
            result.skipped_rules.contains(&"bogus-rule".to_string()),
            "Unknown checker should cause the rule to be skipped"
        );
        assert_eq!(result.summary.errors, 0);
        assert_eq!(result.summary.warnings, 0);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn context_filtering_excludes_non_matching_rules() {
        let doc = Document::new("ContextTest");

        // Rule that only applies in "print" context.
        let rule: Rule = serde_json::from_value(serde_json::json!({
            "id": "print-only",
            "layer": "visual",
            "severity": "warning",
            "checker": "contrast_ratio",
            "params": { "min_ratio": 4.5 },
            "applies_to": {
                "node_kinds": ["text"],
                "contexts": ["print"]
            },
            "message": "Print contrast issue"
        }))
        .unwrap();

        let registry = checkers::default_registry();
        // Force "web" context — the "print"-only rule should be skipped entirely.
        let result = review_document(&doc, &[rule], Some("web"), &registry);

        assert_eq!(result.summary.total, 0, "Rule should not run in web context");
        assert!(result.issues.is_empty());
        assert!(result.skipped_rules.is_empty(), "Context mismatch is not a skip, it is a filter");
    }

    #[test]
    fn passing_rule_increments_passed_count() {
        // Black text on white background should pass contrast check.
        let mut doc = Document::new("PassTest");

        let mut text = Node::new_text("DarkText", "Hello");
        if let NodeKind::Text(ref mut data) = text.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid {
                    color: StyleValue::Raw(Color::black()),
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let text_id = doc.nodes.insert(text);

        let mut frame = Node::new_frame("Background", 400.0, 400.0);
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

        let rule: Rule = serde_json::from_value(serde_json::json!({
            "id": "contrast-min",
            "layer": "visual",
            "severity": "error",
            "checker": "contrast_ratio",
            "params": { "min_ratio": 4.5 },
            "applies_to": {
                "node_kinds": ["text"],
                "contexts": ["web"]
            },
            "message": "Contrast ratio {actual} is below minimum {min_ratio}:1"
        }))
        .unwrap();

        let registry = checkers::default_registry();
        let result = review_document(&doc, &[rule], Some("web"), &registry);

        assert_eq!(result.summary.passed, 1, "Black on white should pass");
        assert_eq!(result.summary.errors, 0);
        assert_eq!(result.summary.total, 1);
        assert!(result.issues.is_empty());
    }
}
