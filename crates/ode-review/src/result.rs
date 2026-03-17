use serde::Serialize;
use std::collections::HashMap;

/// The full result of reviewing a document against a set of rules.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewResult {
    /// Active context labels detected for this document (e.g. "mobile", "print").
    pub contexts: Vec<String>,
    /// Aggregate counts.
    pub summary: ReviewSummary,
    /// Every issue found, in the order rules were evaluated.
    pub issues: Vec<ReviewIssue>,
    /// Rule IDs that could not be run because their checker was not found in the registry.
    pub skipped_rules: Vec<String>,
}

/// Aggregate pass/fail counts.
///
/// `total` is the number of rules evaluated (not the number of issues).
#[derive(Debug, Clone, Serialize)]
pub struct ReviewSummary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub passed: usize,
    pub total: usize,
}

/// A single issue emitted by a checker.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewIssue {
    /// `"error"` or `"warning"`.
    pub severity: String,
    /// The rule id that produced this issue (e.g. `"contrast-min"`).
    pub code: String,
    /// Which design layer was inspected (e.g. `"visual"`, `"layout"`).
    pub layer: String,
    /// Breadcrumb path to the offending node (e.g. `"Frame > Card > Title"`).
    pub path: String,
    /// Human-readable description with template variables resolved.
    pub message: String,
    /// Optional fix suggestion with template variables resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Raw issue produced by a checker before template rendering.
///
/// Checkers return this; `review_document` expands it into a [`ReviewIssue`]
/// using the rule's `message` / `suggestion` templates.
#[derive(Debug, Clone)]
pub struct CheckerIssue {
    /// Breadcrumb path to the offending node.
    pub path: String,
    /// Template variables to substitute into the rule's message/suggestion.
    /// e.g. `{ "actual": "2.1", "min_ratio": "4.5" }`.
    pub template_vars: HashMap<String, String>,
}
