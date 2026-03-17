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
pub use result::{CheckerIssue, ReviewIssue, ReviewResult, ReviewSummary};
pub use rule::{load_rules_from_dir, load_rules_from_paths, Rule};
