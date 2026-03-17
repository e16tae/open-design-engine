use crate::result::CheckerIssue;
use crate::rule::AppliesTo;
use crate::traverse::ParentMap;
use ode_format::Document;
use std::collections::HashMap;

/// Context passed to every checker invocation.
pub struct CheckContext<'a> {
    pub doc: &'a Document,
    pub parent_map: &'a ParentMap,
    pub params: &'a serde_json::Value,
    pub applies_to: &'a AppliesTo,
}

/// Trait that all checkers must implement.
pub trait Checker: Send + Sync {
    /// Returns the wire-format name of this checker (e.g. `"contrast_ratio"`).
    fn name(&self) -> &'static str;

    /// Run the check and return any issues found.
    fn check(&self, ctx: &CheckContext) -> Vec<CheckerIssue>;
}

/// Registry that maps checker names to implementations.
pub struct CheckerRegistry {
    checkers: HashMap<String, Box<dyn Checker>>,
}

impl CheckerRegistry {
    pub fn new() -> Self {
        Self {
            checkers: HashMap::new(),
        }
    }

    /// Register a checker. Uses `checker.name()` as the lookup key.
    pub fn register(&mut self, checker: Box<dyn Checker>) {
        let name = checker.name().to_string();
        self.checkers.insert(name, checker);
    }

    /// Run a named checker with the given parameters.
    ///
    /// Returns `Err` if the checker name is not found in the registry.
    pub fn run(
        &self,
        name: &str,
        params: &serde_json::Value,
        doc: &Document,
        parent_map: &ParentMap,
        applies_to: &AppliesTo,
    ) -> Result<Vec<CheckerIssue>, String> {
        let checker = self
            .checkers
            .get(name)
            .ok_or_else(|| format!("unknown checker: {name}"))?;
        let ctx = CheckContext {
            doc,
            parent_map,
            params,
            applies_to,
        };
        Ok(checker.check(&ctx))
    }
}

impl Default for CheckerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::AppliesTo;
    use crate::traverse::build_parent_map;

    struct NoopChecker;
    impl Checker for NoopChecker {
        fn name(&self) -> &'static str {
            "noop"
        }
        fn check(&self, _ctx: &CheckContext) -> Vec<CheckerIssue> {
            vec![]
        }
    }

    #[test]
    fn register_and_run_checker() {
        let mut registry = CheckerRegistry::new();
        registry.register(Box::new(NoopChecker));

        let doc = Document::new("Test");
        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({});
        let applies_to = AppliesTo::default();

        let issues = registry
            .run("noop", &params, &doc, &parent_map, &applies_to)
            .unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn unknown_checker_returns_error() {
        let registry = CheckerRegistry::new();
        let doc = Document::new("Test");
        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({});
        let applies_to = AppliesTo::default();

        let result = registry.run("nonexistent", &params, &doc, &parent_map, &applies_to);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown checker"));
    }
}
