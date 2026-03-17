use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// A single design rule loaded from a JSON file.
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    /// Unique rule identifier, e.g. `"contrast-min"`.
    pub id: String,
    /// Which design layer this rule checks (e.g. `"visual"`, `"layout"`, `"typography"`).
    pub layer: String,
    /// `"error"` or `"warning"`.
    pub severity: String,
    /// Name of the checker function to invoke (e.g. `"contrast"`, `"touch-target"`).
    pub checker: String,
    /// Arbitrary parameters passed to the checker.
    #[serde(default)]
    pub params: serde_json::Value,
    /// Filter: which node kinds and contexts this rule applies to.
    #[serde(default)]
    pub applies_to: AppliesTo,
    /// Human-readable message template. May contain `{key}` placeholders.
    pub message: String,
    /// Optional fix suggestion template.
    #[serde(default)]
    pub suggestion: Option<String>,
    /// Links to external references (docs, WCAG, etc.).
    #[serde(default)]
    pub references: Vec<String>,
}

/// Filter describing which nodes and contexts a rule applies to.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AppliesTo {
    /// If empty, the rule applies to all node kinds.
    #[serde(default)]
    pub node_kinds: Vec<String>,
    /// If empty, the rule applies in any context.
    #[serde(default)]
    pub contexts: Vec<String>,
}

impl Rule {
    /// Returns `true` if this rule applies to the given context.
    ///
    /// An empty `contexts` list means "applies everywhere".
    pub fn applies_to_any_context(&self, active_contexts: &[String]) -> bool {
        if self.applies_to.contexts.is_empty() {
            return true;
        }
        active_contexts
            .iter()
            .any(|ctx| self.applies_to.contexts.contains(ctx))
    }

    /// Returns `true` if this rule applies to the given node kind tag.
    ///
    /// An empty `node_kinds` list means "applies to all kinds".
    pub fn applies_to_node_kind(&self, kind_tag: &str) -> bool {
        if self.applies_to.node_kinds.is_empty() {
            return true;
        }
        self.applies_to.node_kinds.iter().any(|k| k == kind_tag)
    }

    /// Render the `message` template, replacing `{key}` with values from `vars`.
    pub fn render_message(&self, vars: &HashMap<String, String>) -> String {
        render_template(&self.message, vars)
    }

    /// Render the `suggestion` template if present.
    pub fn render_suggestion(&self, vars: &HashMap<String, String>) -> Option<String> {
        self.suggestion.as_ref().map(|tpl| render_template(tpl, vars))
    }
}

/// Replace every `{key}` in `template` with the corresponding value from `vars`.
///
/// Unresolvable placeholders (keys not found in `vars`) are kept as-is.
pub fn render_template(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{key}}}"), value);
    }
    result
}

/// Load all rules from `.json` files in the given directory.
///
/// Each file must contain a JSON array of [`Rule`] objects.
/// Files that fail to parse are silently skipped.
pub fn load_rules_from_dir(dir: &Path) -> std::io::Result<Vec<Rule>> {
    let mut rules = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let contents = std::fs::read_to_string(&path)?;
            if let Ok(mut file_rules) = serde_json::from_str::<Vec<Rule>>(&contents) {
                rules.append(&mut file_rules);
            }
        }
    }
    Ok(rules)
}

/// Load rules from specific file paths, resolved relative to `base`.
///
/// Each file must contain a JSON array of [`Rule`] objects.
/// Rejects paths that are absolute or contain `..` to prevent path traversal.
pub fn load_rules_from_paths(base: &Path, paths: &[&str]) -> std::io::Result<Vec<Rule>> {
    let mut rules = Vec::new();
    for rel in paths {
        // Reject absolute paths and traversals
        if Path::new(rel).is_absolute() || rel.contains("..") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("rule path escapes knowledge dir: {rel}"),
            ));
        }
        let path = base.join(rel);
        let contents = std::fs::read_to_string(&path)?;
        let mut file_rules: Vec<Rule> = serde_json::from_str(&contents)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        rules.append(&mut file_rules);
    }
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_rule_from_json() {
        let json = r#"{
            "id": "contrast-min",
            "layer": "visual",
            "severity": "error",
            "checker": "contrast",
            "params": { "min_ratio": 4.5 },
            "applies_to": {
                "node_kinds": ["text"],
                "contexts": ["mobile", "web"]
            },
            "message": "Contrast ratio {actual} is below minimum {min_ratio}",
            "suggestion": "Increase contrast to at least {min_ratio}:1",
            "references": ["https://www.w3.org/WAI/WCAG21/Understanding/contrast-minimum.html"]
        }"#;

        let rule: Rule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.id, "contrast-min");
        assert_eq!(rule.layer, "visual");
        assert_eq!(rule.severity, "error");
        assert_eq!(rule.checker, "contrast");
        assert_eq!(rule.params["min_ratio"], 4.5);
        assert_eq!(rule.applies_to.node_kinds, vec!["text"]);
        assert_eq!(rule.applies_to.contexts, vec!["mobile", "web"]);
        assert_eq!(rule.references.len(), 1);

        // Context filtering
        let mobile = vec!["mobile".to_string()];
        let print = vec!["print".to_string()];
        assert!(rule.applies_to_any_context(&mobile));
        assert!(!rule.applies_to_any_context(&print));

        // Node kind filtering
        assert!(rule.applies_to_node_kind("text"));
        assert!(!rule.applies_to_node_kind("frame"));
    }

    #[test]
    fn render_message_replaces_template_vars() {
        let json = r#"{
            "id": "contrast-min",
            "layer": "visual",
            "severity": "error",
            "checker": "contrast",
            "message": "Contrast ratio {actual} is below minimum {min_ratio}",
            "suggestion": "Increase contrast to at least {min_ratio}:1"
        }"#;

        let rule: Rule = serde_json::from_str(json).unwrap();
        let mut vars = HashMap::new();
        vars.insert("actual".to_string(), "2.1".to_string());
        vars.insert("min_ratio".to_string(), "4.5".to_string());

        assert_eq!(
            rule.render_message(&vars),
            "Contrast ratio 2.1 is below minimum 4.5"
        );
        assert_eq!(
            rule.render_suggestion(&vars).unwrap(),
            "Increase contrast to at least 4.5:1"
        );
    }

    #[test]
    fn unresolvable_template_vars_kept_as_is() {
        let json = r#"{
            "id": "test",
            "layer": "visual",
            "severity": "warning",
            "checker": "noop",
            "message": "Value is {actual} but expected {unknown}"
        }"#;

        let rule: Rule = serde_json::from_str(json).unwrap();
        let mut vars = HashMap::new();
        vars.insert("actual".to_string(), "42".to_string());

        assert_eq!(
            rule.render_message(&vars),
            "Value is 42 but expected {unknown}"
        );
    }

    #[test]
    fn default_applies_to_matches_everything() {
        let json = r#"{
            "id": "universal",
            "layer": "layout",
            "severity": "warning",
            "checker": "noop",
            "message": "applies everywhere"
        }"#;

        let rule: Rule = serde_json::from_str(json).unwrap();
        assert!(rule.applies_to.node_kinds.is_empty());
        assert!(rule.applies_to.contexts.is_empty());
        assert!(rule.applies_to_any_context(&["mobile".to_string()]));
        assert!(rule.applies_to_any_context(&[]));
        assert!(rule.applies_to_node_kind("frame"));
        assert!(rule.applies_to_node_kind("text"));
    }

    #[test]
    fn load_rules_from_directory() {
        let dir = tempfile::tempdir().unwrap();
        let rule_json = r#"[
            {
                "id": "test-rule",
                "layer": "visual",
                "severity": "warning",
                "checker": "noop",
                "message": "test message"
            }
        ]"#;

        let file_path = dir.path().join("test-rules.json");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(rule_json.as_bytes()).unwrap();

        let rules = load_rules_from_dir(dir.path()).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "test-rule");
        assert_eq!(rules[0].layer, "visual");
        assert_eq!(rules[0].severity, "warning");
    }

    #[test]
    fn load_rules_from_specific_paths() {
        let dir = tempfile::tempdir().unwrap();
        let rule_json = r#"[
            {
                "id": "path-rule",
                "layer": "layout",
                "severity": "error",
                "checker": "noop",
                "message": "loaded by path"
            }
        ]"#;

        let sub = dir.path().join("rules");
        std::fs::create_dir_all(&sub).unwrap();
        let file_path = sub.join("a.json");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(rule_json.as_bytes()).unwrap();

        let rules = load_rules_from_paths(dir.path(), &["rules/a.json"]).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "path-rule");
    }

    #[test]
    fn render_suggestion_none_when_absent() {
        let json = r#"{
            "id": "no-suggestion",
            "layer": "visual",
            "severity": "warning",
            "checker": "noop",
            "message": "something"
        }"#;

        let rule: Rule = serde_json::from_str(json).unwrap();
        assert!(rule.render_suggestion(&HashMap::new()).is_none());
    }
}
