# Design Knowledge Engine — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a design knowledge engine (rules + guides) so that AI agents can generate professional-quality designs and validate them against design standards via `ode guide` and `ode review`.

**Architecture:** 2-layer knowledge system — JSON rules checked by named Rust checker functions (`ode-review` crate), and Markdown guides served via `ode guide` CLI command. A `design-knowledge/` data directory at the project root holds all knowledge files, discovered at runtime by `ode-cli`.

**Tech Stack:** Rust (edition 2024), serde/serde_json, clap 4 (derive), ODE format types (`ode-format`), color math from `ode-format::Color::to_rgba_u8()`

**Spec:** `docs/superpowers/specs/2026-03-17-design-knowledge-engine-design.md`

---

## File Structure

### New crate: `crates/ode-review/`

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Crate manifest, depends on `ode-format` + `serde` + `serde_json` |
| `src/lib.rs` | Public API: `review_document()`, re-exports |
| `src/rule.rs` | Rule file parsing: `Rule`, `AppliesTo`, rule loader |
| `src/checker.rs` | Checker registry: `CheckerRegistry`, trait `Checker`, dispatch |
| `src/checkers/mod.rs` | Checker module root |
| `src/checkers/contrast_ratio.rs` | `ContrastRatioChecker` — WCAG contrast ratio |
| `src/checkers/min_value.rs` | `MinValueChecker` — minimum property value |
| `src/checkers/spacing_scale.rs` | `SpacingScaleChecker` — spacing base multiple |
| `src/result.rs` | `ReviewResult`, `ReviewIssue`, `ReviewSummary` — output types |
| `src/context.rs` | Context detection from document views |
| `src/traverse.rs` | Node tree traversal, ancestor lookup, background color resolution |

### New directory: `design-knowledge/`

| File | Responsibility |
|------|---------------|
| `index.json` | Layer → rules/guides mapping |
| `rules/accessibility/contrast-ratio.json` | WCAG AA text contrast rule |
| `rules/accessibility/touch-target-size.json` | Minimum touch target 44px |
| `rules/accessibility/font-size-minimum.json` | Minimum text font size |
| `rules/spatial-composition/minimum-spacing.json` | Minimum spacing between elements |
| `rules/spatial-composition/alignment-consistency.json` | Alignment grid adherence |
| `rules/spatial-composition/density-range.json` | Spacing density range check |
| `guides/accessibility.md` | Accessibility design guide |
| `guides/spatial-composition.md` | Spatial composition design guide |

### Modified files

| File | Change |
|------|--------|
| `Cargo.toml` (workspace root) | Add `crates/ode-review` to members |
| `crates/ode-cli/Cargo.toml` | Add `ode-review` dependency |
| `crates/ode-cli/src/main.rs` | Add `Guide` and `Review` subcommands |
| `crates/ode-cli/src/commands.rs` | Add `cmd_guide()` and `cmd_review()` |
| `crates/ode-cli/src/output.rs` | Add `ReviewResponse`, `ReviewIssue` structs |
| `crates/ode-cli/src/knowledge.rs` (new) | Knowledge path discovery logic |

---

## Chunk 1: ode-review crate foundation

### Task 1: Create ode-review crate skeleton

**Files:**
- Create: `crates/ode-review/Cargo.toml`
- Create: `crates/ode-review/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create crate directory**

Run: `mkdir -p crates/ode-review/src`

- [ ] **Step 2: Write Cargo.toml**

Create `crates/ode-review/Cargo.toml`:
```toml
[package]
name = "ode-review"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "ODE design rules engine — validate designs against knowledge-based rules"

[dependencies]
ode-format = { path = "../ode-format" }
serde = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
```

- [ ] **Step 3: Write initial lib.rs**

Create `crates/ode-review/src/lib.rs`:
```rust
pub mod checker;
pub mod checkers;
pub mod context;
pub mod result;
pub mod rule;
pub mod traverse;

use ode_format::Document;
use result::{ReviewResult, ReviewSummary};
use rule::Rule;
use checker::CheckerRegistry;
use context::detect_context;

/// Review a document against a set of design rules.
pub fn review_document(
    doc: &Document,
    rules: &[Rule],
    context: Option<&str>,
    registry: &CheckerRegistry,
) -> ReviewResult {
    let contexts = match context {
        Some(c) => vec![c.to_string()],
        None => detect_context(doc),
    };

    let parent_map = traverse::build_parent_map(doc);
    let mut issues = Vec::new();
    let mut passed = 0u32;
    let mut skipped_rules = Vec::new();

    for rule in rules {
        // Check if rule applies to any detected context
        if !rule.applies_to_any_context(&contexts) {
            continue;
        }

        match registry.run(&rule.checker, &rule.params, doc, &parent_map, &rule.applies_to) {
            Ok(checker_issues) => {
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

    let total = passed + issues.len() as u32;
    let errors = issues.iter().filter(|i| i.severity == "error").count() as u32;
    let warnings = issues.iter().filter(|i| i.severity == "warning").count() as u32;

    ReviewResult {
        contexts,
        summary: ReviewSummary { errors, warnings, passed, total },
        issues,
        skipped_rules,
    }
}
```

- [ ] **Step 4: Add to workspace**

In root `Cargo.toml`, add `"crates/ode-review"` to the `members` array.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p ode-review`
Expected: Compilation errors (modules not yet created) — that's fine, we'll build them next.

- [ ] **Step 6: Commit**

```bash
git add crates/ode-review/ Cargo.toml
git commit -m "feat(ode-review): create crate skeleton with review_document API"
```

---

### Task 2: Implement result types

**Files:**
- Create: `crates/ode-review/src/result.rs`

- [ ] **Step 1: Write result.rs**

```rust
use serde::Serialize;

/// Full review result returned by review_document().
#[derive(Debug, Serialize)]
pub struct ReviewResult {
    /// Detected or specified contexts (e.g., ["web"], ["web", "print"]).
    pub contexts: Vec<String>,
    pub summary: ReviewSummary,
    pub issues: Vec<ReviewIssue>,
    /// Rules that were skipped because their checker was not registered.
    pub skipped_rules: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ReviewSummary {
    pub errors: u32,
    pub warnings: u32,
    pub passed: u32,
    pub total: u32,
}

/// A single design issue found by a checker.
#[derive(Debug, Serialize)]
pub struct ReviewIssue {
    pub severity: String,
    /// Rule ID (e.g., "a11y-contrast-ratio-aa").
    pub code: String,
    /// Knowledge layer (e.g., "accessibility").
    pub layer: String,
    /// Node path (e.g., "nodes[3]").
    pub path: String,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Internal: raw issue from a checker before template rendering.
#[derive(Debug)]
pub struct CheckerIssue {
    pub path: String,
    pub template_vars: std::collections::HashMap<String, String>,
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p ode-review`
Expected: Still errors from other missing modules — result.rs itself should be fine.

- [ ] **Step 3: Commit**

```bash
git add crates/ode-review/src/result.rs
git commit -m "feat(ode-review): add ReviewResult and ReviewIssue types"
```

---

### Task 3: Implement rule parsing

**Files:**
- Create: `crates/ode-review/src/rule.rs`

- [ ] **Step 1: Write failing test**

At the bottom of `rule.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rule_from_json() {
        let json = r#"{
            "id": "a11y-contrast-ratio-aa",
            "layer": "accessibility",
            "severity": "error",
            "checker": "contrast_ratio",
            "params": {"min_ratio": 4.5},
            "applies_to": {
                "node_kinds": ["text"],
                "contexts": ["web"]
            },
            "message": "Contrast {actual}:1 below {min_ratio}:1",
            "suggestion": "Darken text or lighten background"
        }"#;

        let rule: Rule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.id, "a11y-contrast-ratio-aa");
        assert_eq!(rule.checker, "contrast_ratio");
        assert!(rule.applies_to_any_context(&["web".to_string()]));
        assert!(!rule.applies_to_any_context(&["print".to_string()]));
    }

    #[test]
    fn render_message_replaces_template_vars() {
        let rule = Rule {
            id: "test".into(),
            layer: "test".into(),
            severity: "error".into(),
            checker: "test".into(),
            params: serde_json::Value::Object(Default::default()),
            applies_to: AppliesTo { node_kinds: vec![], contexts: vec![] },
            message: "Ratio {actual}:1 below {min_ratio}:1".into(),
            suggestion: Some("Fix {actual}".into()),
            references: vec![],
        };

        let mut vars = std::collections::HashMap::new();
        vars.insert("actual".into(), "3.2".into());
        vars.insert("min_ratio".into(), "4.5".into());

        assert_eq!(rule.render_message(&vars), "Ratio 3.2:1 below 4.5:1");
        assert_eq!(rule.render_suggestion(&vars), Some("Fix 3.2".to_string()));
    }

    #[test]
    fn unresolvable_template_vars_kept_as_is() {
        let rule = Rule {
            id: "test".into(),
            layer: "test".into(),
            severity: "error".into(),
            checker: "test".into(),
            params: serde_json::Value::Object(Default::default()),
            applies_to: AppliesTo { node_kinds: vec![], contexts: vec![] },
            message: "Value {unknown} here".into(),
            suggestion: None,
            references: vec![],
        };

        let vars = std::collections::HashMap::new();
        assert_eq!(rule.render_message(&vars), "Value {unknown} here");
    }

    #[test]
    fn load_rules_from_directory() {
        // This test uses a temp directory with a rule file
        let dir = std::env::temp_dir().join("ode-review-test-rules");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("test-rule.json"),
            r#"{"id":"test","layer":"test","severity":"warning","checker":"min_value","params":{},"applies_to":{"node_kinds":[],"contexts":[]},"message":"test","suggestion":null}"#,
        ).unwrap();

        let rules = load_rules_from_dir(&dir).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "test");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Write rule.rs implementation**

```rust
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Rule {
    pub id: String,
    pub layer: String,
    pub severity: String,
    pub checker: String,
    pub params: serde_json::Value,
    pub applies_to: AppliesTo,
    pub message: String,
    pub suggestion: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AppliesTo {
    #[serde(default)]
    pub node_kinds: Vec<String>,
    #[serde(default)]
    pub contexts: Vec<String>,
}

impl Rule {
    /// Check if this rule applies to any of the given contexts.
    /// Empty contexts in the rule means it applies to all.
    pub fn applies_to_any_context(&self, contexts: &[String]) -> bool {
        if self.applies_to.contexts.is_empty() {
            return true;
        }
        contexts.iter().any(|c| self.applies_to.contexts.contains(c))
    }

    /// Render message template by replacing {key} with values.
    pub fn render_message(&self, vars: &HashMap<String, String>) -> String {
        render_template(&self.message, vars)
    }

    /// Render suggestion template. Returns None if suggestion is None.
    pub fn render_suggestion(&self, vars: &HashMap<String, String>) -> Option<String> {
        self.suggestion.as_ref().map(|s| render_template(s, vars))
    }

    /// Check if this rule applies to a given node kind.
    /// Empty node_kinds means it applies to all.
    pub fn applies_to_node_kind(&self, kind: &str) -> bool {
        if self.applies_to.node_kinds.is_empty() {
            return true;
        }
        self.applies_to.node_kinds.iter().any(|k| k == kind)
    }
}

fn render_template(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{key}}}"), value);
    }
    result
}

/// Load all .json rule files from a directory.
pub fn load_rules_from_dir(dir: &Path) -> Result<Vec<Rule>, String> {
    let mut rules = Vec::new();
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("failed to read rules directory {}: {e}", dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
            let rule: Rule = serde_json::from_str(&content)
                .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
            rules.push(rule);
        }
    }
    Ok(rules)
}

/// Load rules from multiple directories listed in index.json.
pub fn load_rules_from_paths(base: &Path, paths: &[String]) -> Result<Vec<Rule>, String> {
    let mut rules = Vec::new();
    for path_str in paths {
        let path = base.join(path_str);
        if path.is_file() {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
            let rule: Rule = serde_json::from_str(&content)
                .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
            rules.push(rule);
        }
    }
    Ok(rules)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p ode-review -- rule`
Expected: All 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ode-review/src/rule.rs
git commit -m "feat(ode-review): implement rule parsing with template rendering"
```

---

### Task 4: Implement context detection

**Files:**
- Create: `crates/ode-review/src/context.rs`

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::document::{Document, View, ViewId, ViewKind};

    #[test]
    fn no_views_defaults_to_web() {
        let doc = Document::new("Test");
        assert_eq!(detect_context(&doc), vec!["web".to_string()]);
    }

    #[test]
    fn export_view_falls_back_to_web() {
        let mut doc = Document::new("Test");
        doc.views.push(View {
            id: ViewId(1),
            name: "Export".into(),
            kind: ViewKind::Export { targets: vec![] },
        });
        // Export view doesn't map to a specific context, falls back to web
        let contexts = detect_context(&doc);
        assert!(contexts.contains(&"web".to_string()));
    }
}
```

- [ ] **Step 2: Write context.rs**

```rust
use ode_format::Document;
use ode_format::document::ViewKind;

/// Detect review contexts from document views.
/// Returns ["web"] as default if no views or only Export views.
pub fn detect_context(doc: &Document) -> Vec<String> {
    let mut contexts = Vec::new();

    for view in &doc.views {
        match &view.kind {
            ViewKind::Print { .. } => {
                if !contexts.contains(&"print".to_string()) {
                    contexts.push("print".to_string());
                }
            }
            ViewKind::Web { .. } => {
                if !contexts.contains(&"web".to_string()) {
                    contexts.push("web".to_string());
                }
            }
            ViewKind::Presentation { .. } => {
                if !contexts.contains(&"presentation".to_string()) {
                    contexts.push("presentation".to_string());
                }
            }
            ViewKind::Export { .. } => {
                // Export views don't imply a specific context
            }
        }
    }

    if contexts.is_empty() {
        contexts.push("web".to_string());
    }

    contexts
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p ode-review -- context`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/ode-review/src/context.rs
git commit -m "feat(ode-review): implement context detection from document views"
```

---

### Task 5: Implement node traversal and background color resolution

**Files:**
- Create: `crates/ode-review/src/traverse.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::document::Document;
    use ode_format::node::{Node, NodeId, NodeKind};
    use ode_format::style::{Fill, Paint, StyleValue, BlendMode};
    use ode_format::color::Color;

    fn make_doc_with_colored_frame() -> (Document, NodeId) {
        let mut doc = Document::new("Test");
        let mut frame = Node::new_frame("Root", 400.0, 300.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::from_hex("#336699").unwrap()) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);
        (doc, frame_id)
    }

    #[test]
    fn build_parent_map_from_doc() {
        let (doc, frame_id) = make_doc_with_colored_frame();
        let map = build_parent_map(&doc);
        // Root frame has no parent
        assert!(map.get(&frame_id).is_none());
    }

    #[test]
    fn find_background_color_from_parent() {
        let (mut doc, frame_id) = make_doc_with_colored_frame();

        // Add a text child inside the frame
        let text = Node::new_text("Label", "Hello");
        let text_id = doc.nodes.insert(text);
        if let NodeKind::Frame(ref mut data) = doc.nodes[frame_id].kind {
            data.container.children.push(text_id);
        }

        let parent_map = build_parent_map(&doc);
        let bg = find_background_color(&doc, text_id, &parent_map);
        // Should find the parent frame's fill color
        let [r, g, b, _] = bg.to_rgba_u8();
        assert_eq!(r, 0x33);
        assert_eq!(g, 0x66);
        assert_eq!(b, 0x99);
    }

    #[test]
    fn no_parent_fill_defaults_to_white() {
        let mut doc = Document::new("Test");
        let frame = Node::new_frame("Root", 400.0, 300.0);
        // No fill on frame
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let text = Node::new_text("Label", "Hello");
        let text_id = doc.nodes.insert(text);
        if let NodeKind::Frame(ref mut data) = doc.nodes[frame_id].kind {
            data.container.children.push(text_id);
        }

        let parent_map = build_parent_map(&doc);
        let bg = find_background_color(&doc, text_id, &parent_map);
        let [r, g, b, _] = bg.to_rgba_u8();
        assert_eq!((r, g, b), (255, 255, 255));
    }
}
```

- [ ] **Step 2: Write traverse.rs**

```rust
use ode_format::color::Color;
use ode_format::document::Document;
use ode_format::node::{NodeId, NodeKind};
use ode_format::style::Paint;
use std::collections::HashMap;

/// Map from child NodeId → parent NodeId.
pub type ParentMap = HashMap<NodeId, NodeId>;

/// Build a parent map for the entire document.
pub fn build_parent_map(doc: &Document) -> ParentMap {
    let mut map = ParentMap::new();
    for (node_id, node) in doc.nodes.iter() {
        if let Some(children) = node.kind.children() {
            for &child_id in children {
                map.insert(child_id, node_id);
            }
        }
    }
    map
}

/// Walk up ancestors to find the nearest solid fill color.
/// Returns white (#FFFFFF) if no ancestor has a solid fill.
pub fn find_background_color(doc: &Document, node_id: NodeId, parent_map: &ParentMap) -> Color {
    let mut current = parent_map.get(&node_id).copied();
    while let Some(ancestor_id) = current {
        let ancestor = &doc.nodes[ancestor_id];
        if let Some(visual) = ancestor.kind.visual() {
            for fill in &visual.fills {
                if !fill.visible {
                    continue;
                }
                if let Paint::Solid { ref color } = fill.paint {
                    return color.value().clone();
                }
            }
        }
        current = parent_map.get(&ancestor_id).copied();
    }
    Color::white()
}

/// Get the node kind name as a wire-format string.
pub fn node_kind_name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Frame(_) => "frame",
        NodeKind::Group(_) => "group",
        NodeKind::Vector(_) => "vector",
        NodeKind::BooleanOp(_) => "boolean-op",
        NodeKind::Text(_) => "text",
        NodeKind::Image(_) => "image",
        NodeKind::Instance(_) => "instance",
    }
}

/// Generate a human-readable path for a node using its stable_id.
pub fn node_path(doc: &Document, target_id: NodeId) -> String {
    let node = &doc.nodes[target_id];
    format!("node[{}]", node.stable_id)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p ode-review -- traverse`
Expected: All 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ode-review/src/traverse.rs
git commit -m "feat(ode-review): implement parent map and background color resolution"
```

---

### Task 6: Implement checker registry and trait

**Files:**
- Create: `crates/ode-review/src/checker.rs`
- Create: `crates/ode-review/src/checkers/mod.rs`

- [ ] **Step 1: Write checker.rs**

```rust
use crate::result::CheckerIssue;
use crate::rule::AppliesTo;
use crate::traverse::ParentMap;
use ode_format::Document;
use std::collections::HashMap;

/// Context passed to every checker function.
pub struct CheckContext<'a> {
    pub doc: &'a Document,
    pub parent_map: &'a ParentMap,
    pub params: &'a serde_json::Value,
    pub applies_to: &'a AppliesTo,
}

/// Trait for a named checker function.
pub trait Checker: Send + Sync {
    fn name(&self) -> &'static str;
    fn check(&self, ctx: &CheckContext) -> Vec<CheckerIssue>;
}

/// Registry of checker functions, keyed by name.
pub struct CheckerRegistry {
    checkers: HashMap<String, Box<dyn Checker>>,
}

impl CheckerRegistry {
    pub fn new() -> Self {
        Self { checkers: HashMap::new() }
    }

    pub fn register(&mut self, checker: Box<dyn Checker>) {
        let name = checker.name().to_string();
        self.checkers.insert(name, checker);
    }

    /// Run a named checker. Returns Err if checker not found.
    pub fn run(
        &self,
        name: &str,
        params: &serde_json::Value,
        doc: &Document,
        parent_map: &crate::traverse::ParentMap,
        applies_to: &AppliesTo,
    ) -> Result<Vec<CheckerIssue>, String> {
        let checker = self.checkers.get(name)
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
```

- [ ] **Step 2: Write checkers/mod.rs**

```rust
pub mod contrast_ratio;
pub mod min_value;
pub mod spacing_scale;

use crate::checker::CheckerRegistry;

/// Create a registry with all built-in checkers.
pub fn default_registry() -> CheckerRegistry {
    let mut registry = CheckerRegistry::new();
    registry.register(Box::new(contrast_ratio::ContrastRatioChecker));
    registry.register(Box::new(min_value::MinValueChecker));
    registry.register(Box::new(spacing_scale::SpacingScaleChecker));
    registry
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/ode-review/src/checker.rs crates/ode-review/src/checkers/mod.rs
git commit -m "feat(ode-review): add Checker trait and CheckerRegistry"
```

---

### Task 7: Implement contrast_ratio checker

**Files:**
- Create: `crates/ode-review/src/checkers/contrast_ratio.rs`

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::CheckContext;
    use crate::rule::AppliesTo;
    use crate::traverse::build_parent_map;
    use ode_format::color::Color;
    use ode_format::document::Document;
    use ode_format::node::{Node, NodeKind};
    use ode_format::style::{Fill, Paint, StyleValue, BlendMode};

    fn doc_with_text_on_bg(text_hex: &str, bg_hex: &str) -> Document {
        let mut doc = Document::new("Test");
        let mut frame = Node::new_frame("Root", 400.0, 300.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::from_hex(bg_hex).unwrap()) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let mut text = Node::new_text("Label", "Hello");
        if let NodeKind::Text(ref mut data) = text.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::from_hex(text_hex).unwrap()) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let text_id = doc.nodes.insert(text);
        if let NodeKind::Frame(ref mut data) = doc.nodes[frame_id].kind {
            data.container.children.push(text_id);
        }

        doc
    }

    #[test]
    fn high_contrast_passes() {
        let doc = doc_with_text_on_bg("#000000", "#FFFFFF");
        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({"min_ratio": 4.5});
        let applies_to = AppliesTo { node_kinds: vec!["text".into()], contexts: vec![] };
        let ctx = CheckContext { doc: &doc, parent_map: &parent_map, params: &params, applies_to: &applies_to };

        let issues = ContrastRatioChecker.check(&ctx);
        assert!(issues.is_empty());
    }

    #[test]
    fn low_contrast_fails() {
        // Light gray text on white background
        let doc = doc_with_text_on_bg("#CCCCCC", "#FFFFFF");
        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({"min_ratio": 4.5});
        let applies_to = AppliesTo { node_kinds: vec!["text".into()], contexts: vec![] };
        let ctx = CheckContext { doc: &doc, parent_map: &parent_map, params: &params, applies_to: &applies_to };

        let issues = ContrastRatioChecker.check(&ctx);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].template_vars.contains_key("actual"));
    }
}
```

- [ ] **Step 2: Write contrast_ratio.rs**

```rust
use crate::checker::{CheckContext, Checker};
use crate::result::CheckerIssue;
use crate::traverse::{find_background_color, node_kind_name, node_path};
use ode_format::color::Color;
use ode_format::style::Paint;
use std::collections::HashMap;

pub struct ContrastRatioChecker;

impl Checker for ContrastRatioChecker {
    fn name(&self) -> &'static str {
        "contrast_ratio"
    }

    fn check(&self, ctx: &CheckContext) -> Vec<CheckerIssue> {
        let min_ratio = ctx.params.get("min_ratio")
            .and_then(|v| v.as_f64())
            .unwrap_or(4.5);

        let mut issues = Vec::new();

        for (node_id, node) in ctx.doc.nodes.iter() {
            if !ctx.applies_to.node_kinds.is_empty()
                && !ctx.applies_to.node_kinds.iter().any(|k| k == node_kind_name(&node.kind))
            {
                continue;
            }

            if !node.visible {
                continue;
            }

            let fg_color = match extract_foreground_color(node) {
                Some(c) => c,
                None => continue,
            };

            let bg_color = find_background_color(ctx.doc, node_id, ctx.parent_map);
            let ratio = contrast_ratio(&fg_color, &bg_color);

            if ratio < min_ratio as f32 {
                let mut vars = HashMap::new();
                vars.insert("actual".to_string(), format!("{ratio:.1}"));
                vars.insert("min_ratio".to_string(), format!("{min_ratio}"));

                issues.push(CheckerIssue {
                    path: node_path(ctx.doc, node_id),
                    template_vars: vars,
                });
            }
        }

        issues
    }
}

fn extract_foreground_color(node: &ode_format::node::Node) -> Option<Color> {
    let visual = node.kind.visual()?;
    for fill in &visual.fills {
        if !fill.visible {
            continue;
        }
        if let Paint::Solid { ref color } = fill.paint {
            return Some(color.value().clone());
        }
    }
    None
}

/// Calculate WCAG 2.x contrast ratio between two colors.
/// Returns a value >= 1.0 (higher = more contrast).
fn contrast_ratio(fg: &Color, bg: &Color) -> f32 {
    let fg_lum = relative_luminance(fg);
    let bg_lum = relative_luminance(bg);
    let (lighter, darker) = if fg_lum > bg_lum {
        (fg_lum, bg_lum)
    } else {
        (bg_lum, fg_lum)
    };
    (lighter + 0.05) / (darker + 0.05)
}

/// Calculate relative luminance per WCAG 2.x formula.
/// Input: Color (converted to sRGB via to_rgba_u8).
fn relative_luminance(color: &Color) -> f32 {
    let [r, g, b, _] = color.to_rgba_u8();
    let r = linearize(r as f32 / 255.0);
    let g = linearize(g as f32 / 255.0);
    let b = linearize(b as f32 / 255.0);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn linearize(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p ode-review -- contrast_ratio`
Expected: Both tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ode-review/src/checkers/contrast_ratio.rs
git commit -m "feat(ode-review): implement WCAG contrast ratio checker"
```

---

### Task 8: Implement min_value and spacing_scale checkers

**Files:**
- Create: `crates/ode-review/src/checkers/min_value.rs`
- Create: `crates/ode-review/src/checkers/spacing_scale.rs`

- [ ] **Step 1: Write min_value.rs with tests**

```rust
use crate::checker::{CheckContext, Checker};
use crate::result::CheckerIssue;
use crate::traverse::{node_kind_name, node_path};
use std::collections::HashMap;

pub struct MinValueChecker;

impl Checker for MinValueChecker {
    fn name(&self) -> &'static str {
        "min_value"
    }

    fn check(&self, ctx: &CheckContext) -> Vec<CheckerIssue> {
        let property = ctx.params.get("property").and_then(|v| v.as_str()).unwrap_or("");
        let min = ctx.params.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

        let mut issues = Vec::new();

        for (node_id, node) in ctx.doc.nodes.iter() {
            if !ctx.applies_to.node_kinds.is_empty()
                && !ctx.applies_to.node_kinds.iter().any(|k| k == node_kind_name(&node.kind))
            {
                continue;
            }

            let actual = match get_property_value(node, property) {
                Some(v) => v,
                None => continue,
            };

            if actual < min {
                let mut vars = HashMap::new();
                vars.insert("actual".to_string(), format!("{actual}"));
                vars.insert("min".to_string(), format!("{min}"));
                vars.insert("property".to_string(), property.to_string());

                issues.push(CheckerIssue {
                    path: node_path(ctx.doc, node_id),
                    template_vars: vars,
                });
            }
        }

        issues
    }
}

fn get_property_value(node: &ode_format::node::Node, property: &str) -> Option<f32> {
    match property {
        "width" => match &node.kind {
            ode_format::node::NodeKind::Frame(data) => Some(data.width),
            ode_format::node::NodeKind::Image(data) => Some(data.width),
            _ => None,
        },
        "height" => match &node.kind {
            ode_format::node::NodeKind::Frame(data) => Some(data.height),
            ode_format::node::NodeKind::Image(data) => Some(data.height),
            _ => None,
        },
        "font_size" => match &node.kind {
            ode_format::node::NodeKind::Text(data) => Some(data.default_style.font_size.value()),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::CheckContext;
    use crate::rule::AppliesTo;
    use crate::traverse::build_parent_map;
    use ode_format::document::Document;
    use ode_format::node::Node;

    #[test]
    fn frame_width_above_min_passes() {
        let mut doc = Document::new("Test");
        let frame = Node::new_frame("Button", 48.0, 48.0);
        let fid = doc.nodes.insert(frame);
        doc.canvas.push(fid);

        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({"property": "width", "min": 44.0});
        let applies_to = AppliesTo { node_kinds: vec!["frame".into()], contexts: vec![] };
        let ctx = CheckContext { doc: &doc, parent_map: &parent_map, params: &params, applies_to: &applies_to };

        let issues = MinValueChecker.check(&ctx);
        assert!(issues.is_empty());
    }

    #[test]
    fn frame_width_below_min_fails() {
        let mut doc = Document::new("Test");
        let frame = Node::new_frame("Button", 30.0, 30.0);
        let fid = doc.nodes.insert(frame);
        doc.canvas.push(fid);

        let parent_map = build_parent_map(&doc);
        let params = serde_json::json!({"property": "width", "min": 44.0});
        let applies_to = AppliesTo { node_kinds: vec!["frame".into()], contexts: vec![] };
        let ctx = CheckContext { doc: &doc, parent_map: &parent_map, params: &params, applies_to: &applies_to };

        let issues = MinValueChecker.check(&ctx);
        assert_eq!(issues.len(), 1);
    }
}
```

- [ ] **Step 2: Write spacing_scale.rs with tests**

```rust
use crate::checker::{CheckContext, Checker};
use crate::result::CheckerIssue;
use crate::traverse::node_path;
use ode_format::node::NodeKind;
use std::collections::HashMap;

pub struct SpacingScaleChecker;

impl Checker for SpacingScaleChecker {
    fn name(&self) -> &'static str {
        "spacing_scale"
    }

    fn check(&self, ctx: &CheckContext) -> Vec<CheckerIssue> {
        let base = ctx.params.get("base").and_then(|v| v.as_f64()).unwrap_or(8.0) as f32;
        let tolerance = ctx.params.get("tolerance").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;

        let mut issues = Vec::new();

        for (node_id, node) in ctx.doc.nodes.iter() {
            if let NodeKind::Frame(data) = &node.kind {
                if let Some(ref layout) = data.container.layout {
                    let spacing = layout.item_spacing;
                    if spacing > 0.0 && !is_on_scale(spacing, base, tolerance) {
                        let nearest = (spacing / base).round() * base;
                        let mut vars = HashMap::new();
                        vars.insert("actual".to_string(), format!("{spacing}"));
                        vars.insert("base".to_string(), format!("{base}"));
                        vars.insert("nearest".to_string(), format!("{nearest}"));

                        issues.push(CheckerIssue {
                            path: node_path(ctx.doc, node_id),
                            template_vars: vars,
                        });
                    }
                }
            }
        }

        issues
    }
}

fn is_on_scale(value: f32, base: f32, tolerance: f32) -> bool {
    let remainder = value % base;
    remainder <= tolerance || (base - remainder) <= tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_scale_values() {
        assert!(is_on_scale(8.0, 8.0, 0.5));
        assert!(is_on_scale(16.0, 8.0, 0.5));
        assert!(is_on_scale(24.0, 8.0, 0.5));
        assert!(is_on_scale(0.0, 8.0, 0.5)); // zero is always on scale (not checked)
    }

    #[test]
    fn off_scale_values() {
        assert!(!is_on_scale(10.0, 8.0, 0.5));
        assert!(!is_on_scale(13.0, 8.0, 0.5));
    }
}
```

- [ ] **Step 3: Run all checker tests**

Run: `cargo test -p ode-review`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ode-review/src/checkers/
git commit -m "feat(ode-review): implement min_value and spacing_scale checkers"
```

---

### Task 9: Wire up lib.rs and verify full crate

**Files:**
- Modify: `crates/ode-review/src/lib.rs`

- [ ] **Step 1: Write integration test at crate level**

Add to bottom of `lib.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::color::Color;
    use ode_format::document::Document;
    use ode_format::node::{Node, NodeKind};
    use ode_format::style::{BlendMode, Fill, Paint, StyleValue};

    #[test]
    fn review_catches_low_contrast_text() {
        let mut doc = Document::new("Test");
        let mut frame = Node::new_frame("Root", 400.0, 300.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::white()) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        let mut text = Node::new_text("Label", "Low contrast");
        if let NodeKind::Text(ref mut data) = text.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::from_hex("#CCCCCC").unwrap()) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let text_id = doc.nodes.insert(text);
        if let NodeKind::Frame(ref mut data) = doc.nodes[frame_id].kind {
            data.container.children.push(text_id);
        }

        let rules_json = r#"[{
            "id": "a11y-contrast",
            "layer": "accessibility",
            "severity": "error",
            "checker": "contrast_ratio",
            "params": {"min_ratio": 4.5},
            "applies_to": {"node_kinds": ["text"], "contexts": ["web"]},
            "message": "Contrast {actual}:1 below {min_ratio}:1",
            "suggestion": "Increase contrast"
        }]"#;
        let rules: Vec<rule::Rule> = serde_json::from_str(rules_json).unwrap();
        let registry = checkers::default_registry();

        let result = review_document(&doc, &rules, Some("web"), &registry);
        assert_eq!(result.summary.errors, 1);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].code, "a11y-contrast");
        assert!(result.issues[0].message.contains(":1"));
    }

    #[test]
    fn unknown_checker_is_skipped() {
        let doc = Document::new("Test");
        let rules_json = r#"[{
            "id": "unknown-rule",
            "layer": "test",
            "severity": "error",
            "checker": "does_not_exist",
            "params": {},
            "applies_to": {"node_kinds": [], "contexts": []},
            "message": "test",
            "suggestion": null
        }]"#;
        let rules: Vec<rule::Rule> = serde_json::from_str(rules_json).unwrap();
        let registry = checkers::default_registry();

        let result = review_document(&doc, &rules, None, &registry);
        assert_eq!(result.skipped_rules, vec!["unknown-rule"]);
        assert_eq!(result.summary.errors, 0);
    }
}
```

- [ ] **Step 2: Run full crate tests**

Run: `cargo test -p ode-review`
Expected: All tests pass (rule tests, context tests, traverse tests, checker tests, integration tests).

- [ ] **Step 3: Commit**

```bash
git add crates/ode-review/src/lib.rs
git commit -m "feat(ode-review): wire up full review pipeline with integration tests"
```

---

## Chunk 2: CLI integration and design-knowledge content

### Task 10: Add knowledge path discovery to ode-cli

**Files:**
- Create: `crates/ode-cli/src/knowledge.rs`
- Modify: `crates/ode-cli/src/main.rs` (add `mod knowledge;`)

- [ ] **Step 1: Write knowledge.rs with tests**

```rust
use std::path::{Path, PathBuf};

/// Discover the design-knowledge directory.
/// Search order:
/// 1. ODE_KNOWLEDGE_PATH env var
/// 2. Build-time path (CARGO_MANIFEST_DIR)
/// 3. Relative to binary: ../design-knowledge/
/// 4. CWD/design-knowledge/
/// 5. ~/.ode/design-knowledge/
pub fn find_knowledge_dir() -> Option<PathBuf> {
    // 1. Environment variable
    if let Ok(path) = std::env::var("ODE_KNOWLEDGE_PATH") {
        let p = PathBuf::from(path);
        if p.is_dir() {
            return Some(p);
        }
    }

    // 2. Build-time path (workspace root / design-knowledge)
    let build_time = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()  // crates/
        .and_then(|p| p.parent())  // workspace root
        .map(|p| p.join("design-knowledge"));
    if let Some(ref p) = build_time {
        if p.is_dir() {
            return Some(p.clone());
        }
    }

    // 3. Relative to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent().and_then(|p| p.parent()) {
            let p = parent.join("design-knowledge");
            if p.is_dir() {
                return Some(p);
            }
        }
    }

    // 4. Current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join("design-knowledge");
        if p.is_dir() {
            return Some(p);
        }
    }

    // 5. Home directory
    if let Some(home) = std::env::var_os("HOME") {
        let p = PathBuf::from(home).join(".ode").join("design-knowledge");
        if p.is_dir() {
            return Some(p);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_knowledge_dir_from_build_time_path() {
        // This test runs within the workspace, so the build-time path should work
        // if design-knowledge/ exists (it will after we create it).
        // For now, just verify the function doesn't panic.
        let _ = find_knowledge_dir();
    }
}
```

- [ ] **Step 2: Add `mod knowledge;` to main.rs**

In `crates/ode-cli/src/main.rs`, add `mod knowledge;` alongside other module declarations.

- [ ] **Step 3: Commit**

```bash
git add crates/ode-cli/src/knowledge.rs crates/ode-cli/src/main.rs
git commit -m "feat(ode-cli): add knowledge path discovery"
```

---

### Task 11: Add ode-review dependency and output types to ode-cli

**Files:**
- Modify: `crates/ode-cli/Cargo.toml`
- Modify: `crates/ode-cli/src/output.rs`

- [ ] **Step 1: Add ode-review to ode-cli Cargo.toml**

Add to `[dependencies]`:
```toml
ode-review = { path = "../ode-review" }
```

- [ ] **Step 2: Add ReviewResponse to output.rs**

Add to `crates/ode-cli/src/output.rs`:
```rust
#[derive(Serialize)]
pub struct ReviewResponse {
    pub status: &'static str,
    pub context: serde_json::Value, // String or Vec<String>
    pub summary: ode_review::result::ReviewSummary,
    pub issues: Vec<ode_review::result::ReviewIssue>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped_rules: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

#[derive(Serialize)]
pub struct GuideContentResponse {
    pub status: &'static str,
    pub format: &'static str,
    pub content: String,
}

#[derive(Serialize)]
pub struct GuideListResponse {
    pub status: &'static str,
    pub layers: Vec<GuideLayerInfo>,
}

#[derive(Serialize)]
pub struct GuideLayerInfo {
    pub id: String,
    pub name: String,
    pub contexts: Vec<String>,
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p ode-cli`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/ode-cli/Cargo.toml crates/ode-cli/src/output.rs
git commit -m "feat(ode-cli): add ode-review dependency and review/guide output types"
```

---

### Task 12: Add `ode guide` command

**Files:**
- Modify: `crates/ode-cli/src/main.rs`
- Modify: `crates/ode-cli/src/commands.rs`

- [ ] **Step 1: Add Guide subcommand to main.rs**

Add to the `Command` enum:
```rust
/// Query design knowledge guides
Guide {
    /// Guide layer ID (e.g., "accessibility", "spatial-composition")
    layer_id: Option<String>,

    /// Filter by context (e.g., "web", "print")
    #[arg(long)]
    context: Option<String>,

    /// Show only a specific section
    #[arg(long)]
    section: Option<String>,

    /// List guides related to a layer
    #[arg(long)]
    related: Option<String>,
},
```

Add dispatch in `main()`:
```rust
Command::Guide { layer_id, context, section, related } => {
    commands::cmd_guide(layer_id.as_deref(), context.as_deref(), section.as_deref(), related.as_deref())
}
```

- [ ] **Step 2: Implement cmd_guide in commands.rs**

```rust
pub fn cmd_guide(
    layer_id: Option<&str>,
    context: Option<&str>,
    section: Option<&str>,
    _related: Option<&str>,
) -> i32 {
    let knowledge_dir = match crate::knowledge::find_knowledge_dir() {
        Some(d) => d,
        None => {
            print_json(&ErrorResponse::new(
                "KNOWLEDGE_NOT_FOUND",
                "guide",
                "design-knowledge directory not found. Set ODE_KNOWLEDGE_PATH or place design-knowledge/ in the working directory.",
            ));
            return EXIT_IO;
        }
    };

    let index_path = knowledge_dir.join("index.json");
    let index_content = match std::fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(e) => {
            print_json(&ErrorResponse::new("IO_ERROR", "guide", &format!("failed to read index.json: {e}")));
            return EXIT_IO;
        }
    };

    let index: serde_json::Value = match serde_json::from_str(&index_content) {
        Ok(v) => v,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "guide", &format!("failed to parse index.json: {e}")));
            return EXIT_INPUT;
        }
    };

    // If no layer_id, list all layers
    let Some(layer_id) = layer_id else {
        let layers: Vec<output::GuideLayerInfo> = index["layers"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|l| {
                let id = l["id"].as_str()?.to_string();
                let name = l["name"].as_str()?.to_string();
                let contexts = l.get("contexts")
                    .and_then(|c| c.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();

                // Filter by context if specified
                if let Some(ctx) = context {
                    if !contexts.is_empty() && !contexts.contains(&ctx.to_string()) {
                        return None;
                    }
                }

                Some(output::GuideLayerInfo { id, name, contexts })
            })
            .collect();

        print_json(&output::GuideListResponse { status: "ok", layers });
        return EXIT_OK;
    };

    // Find the layer
    let layer = index["layers"]
        .as_array()
        .and_then(|arr| arr.iter().find(|l| l["id"].as_str() == Some(layer_id)));

    let Some(layer) = layer else {
        print_json(&ErrorResponse::new(
            "LAYER_NOT_FOUND",
            "guide",
            &format!("layer '{layer_id}' not found"),
        ));
        return EXIT_INPUT;
    };

    // Read the guide file
    let guide_paths = layer["guides"].as_array().unwrap_or(&vec![]);
    let guide_path = guide_paths.first().and_then(|v| v.as_str());

    let Some(guide_path) = guide_path else {
        print_json(&ErrorResponse::new("NO_GUIDE", "guide", &format!("no guide file for layer '{layer_id}'")));
        return EXIT_INPUT;
    };

    let full_path = knowledge_dir.join(guide_path);
    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => {
            print_json(&ErrorResponse::new("IO_ERROR", "guide", &format!("failed to read {guide_path}: {e}")));
            return EXIT_IO;
        }
    };

    // If section specified, extract only that section
    let content = if let Some(section) = section {
        extract_section(&content, section).unwrap_or(content)
    } else {
        content
    };

    print_json(&output::GuideContentResponse {
        status: "ok",
        format: "markdown",
        content,
    });

    EXIT_OK
}

fn extract_section(markdown: &str, section_name: &str) -> Option<String> {
    let target = format!("## {}", section_name.replace('-', " "));
    let target_lower = target.to_lowercase();

    let mut in_section = false;
    let mut result = Vec::new();

    for line in markdown.lines() {
        if line.to_lowercase().starts_with(&target_lower) {
            in_section = true;
            result.push(line);
            continue;
        }
        if in_section {
            if line.starts_with("## ") {
                break;
            }
            result.push(line);
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result.join("\n"))
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p ode-cli`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/ode-cli/src/main.rs crates/ode-cli/src/commands.rs
git commit -m "feat(ode-cli): add ode guide command"
```

---

### Task 13: Add `ode review` command

**Files:**
- Modify: `crates/ode-cli/src/main.rs`
- Modify: `crates/ode-cli/src/commands.rs`

- [ ] **Step 1: Add Review subcommand to main.rs**

Add to the `Command` enum:
```rust
/// Review a design against knowledge-based rules
Review {
    /// Input file (.ode.json) or - for stdin
    file: String,

    /// Override context detection (e.g., "web", "print")
    #[arg(long)]
    context: Option<String>,

    /// Only check rules from a specific layer
    #[arg(long)]
    layer: Option<String>,
},
```

Add dispatch in `main()`:
```rust
Command::Review { file, context, layer } => {
    commands::cmd_review(&file, context.as_deref(), layer.as_deref())
}
```

- [ ] **Step 2: Implement cmd_review in commands.rs**

```rust
pub fn cmd_review(file: &str, context: Option<&str>, layer_filter: Option<&str>) -> i32 {
    // 1. Find knowledge directory
    let knowledge_dir = match crate::knowledge::find_knowledge_dir() {
        Some(d) => d,
        None => {
            print_json(&ErrorResponse::new(
                "KNOWLEDGE_NOT_FOUND",
                "review",
                "design-knowledge directory not found. Set ODE_KNOWLEDGE_PATH or place design-knowledge/ in the working directory.",
            ));
            return EXIT_IO;
        }
    };

    // 2. Read and parse document
    let json_str = match load_input(file) {
        Ok(s) => s,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    let doc: ode_format::Document = match serde_json::from_str(&json_str) {
        Ok(d) => d,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "review", &format!("{e}")));
            return EXIT_INPUT;
        }
    };

    // 3. Load index and rules
    let index_path = knowledge_dir.join("index.json");
    let index_content = match std::fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(e) => {
            print_json(&ErrorResponse::new("IO_ERROR", "review", &format!("failed to read index.json: {e}")));
            return EXIT_IO;
        }
    };

    let index: serde_json::Value = match serde_json::from_str(&index_content) {
        Ok(v) => v,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "review", &format!("failed to parse index.json: {e}")));
            return EXIT_INPUT;
        }
    };

    let mut all_rules = Vec::new();
    if let Some(layers) = index["layers"].as_array() {
        for layer_val in layers {
            let layer_id = layer_val["id"].as_str().unwrap_or("");
            if let Some(filter) = layer_filter {
                if layer_id != filter {
                    continue;
                }
            }
            if let Some(rule_paths) = layer_val["rules"].as_array() {
                let paths: Vec<String> = rule_paths.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                match ode_review::rule::load_rules_from_paths(&knowledge_dir, &paths) {
                    Ok(rules) => all_rules.extend(rules),
                    Err(e) => {
                        // Non-fatal: warn but continue
                        eprintln!("warning: {e}");
                    }
                }
            }
        }
    }

    // 4. Run review
    let registry = ode_review::checkers::default_registry();
    let result = ode_review::review_document(&doc, &all_rules, context, &registry);

    // 5. Output
    let context_value = if result.contexts.len() == 1 {
        serde_json::Value::String(result.contexts[0].clone())
    } else {
        serde_json::json!(result.contexts)
    };

    print_json(&output::ReviewResponse {
        status: "ok",
        context: context_value,
        summary: result.summary,
        issues: result.issues,
        skipped_rules: result.skipped_rules,
        warnings: vec![],
    });

    EXIT_OK
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p ode-cli`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/ode-cli/src/main.rs crates/ode-cli/src/commands.rs
git commit -m "feat(ode-cli): add ode review command"
```

---

### Task 14: Create design-knowledge directory with Phase 1 content

**Files:**
- Create: `design-knowledge/index.json`
- Create: `design-knowledge/rules/accessibility/contrast-ratio.json`
- Create: `design-knowledge/rules/accessibility/touch-target-size.json`
- Create: `design-knowledge/rules/accessibility/font-size-minimum.json`
- Create: `design-knowledge/rules/spatial-composition/minimum-spacing.json`
- Create: `design-knowledge/rules/spatial-composition/density-range.json`
- Create: `design-knowledge/rules/spatial-composition/alignment-consistency.json`

- [ ] **Step 1: Create directory structure**

Run:
```bash
mkdir -p design-knowledge/rules/accessibility
mkdir -p design-knowledge/rules/spatial-composition
mkdir -p design-knowledge/guides
```

- [ ] **Step 2: Write index.json**

Create `design-knowledge/index.json` — listing all Phase 1 layers with explicit file paths.

- [ ] **Step 3: Write accessibility rules**

Create `contrast-ratio.json`, `touch-target-size.json`, `font-size-minimum.json` using the schema from the spec, each referencing a checker (`contrast_ratio`, `min_value`, `min_value`).

- [ ] **Step 4: Write spatial-composition rules**

Create `minimum-spacing.json`, `density-range.json`, `alignment-consistency.json` using `spacing_scale` and `min_value` checkers.

- [ ] **Step 5: Commit**

```bash
git add design-knowledge/
git commit -m "feat: add Phase 1 design-knowledge rules (accessibility + spatial-composition)"
```

---

### Task 15: Write Phase 1 guide content

**Files:**
- Create: `design-knowledge/guides/accessibility.md`
- Create: `design-knowledge/guides/spatial-composition.md`

- [ ] **Step 1: Write accessibility guide**

Write `guides/accessibility.md` with YAML frontmatter (`id`, `name`, `layer`, `contexts`, `related`), sections: 핵심 원칙, 규칙, 맥락별 적용, ODE 매핑 (inline JSON snippets), 안티패턴. Content gathered from WCAG 2.2, Apple HIG, Material Design 3. Use `WebSearch`/`WebFetch` and `context7` MCP to gather current guidelines.

- [ ] **Step 2: Write spatial-composition guide**

Write `guides/spatial-composition.md` with same structure. Content from 8-point grid, Gestalt principles, density levels, visual rhythm. Use `WebSearch`/`WebFetch` for current best practices.

- [ ] **Step 3: Commit**

```bash
git add design-knowledge/guides/
git commit -m "feat: add Phase 1 design guides (accessibility + spatial-composition)"
```

---

### Task 16: CLI integration tests

**Files:**
- Modify: `crates/ode-cli/tests/integration.rs`

- [ ] **Step 1: Write integration tests**

Add tests to `integration.rs`:

```rust
#[test]
fn guide_lists_layers() {
    let output = ode_cmd().args(["guide"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert!(json["layers"].as_array().unwrap().len() >= 2);
}

#[test]
fn guide_shows_accessibility() {
    let output = ode_cmd().args(["guide", "accessibility"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["format"], "markdown");
    assert!(json["content"].as_str().unwrap().contains("접근성") || json["content"].as_str().unwrap().contains("Accessibility"));
}

#[test]
fn guide_unknown_layer_returns_error() {
    let output = ode_cmd().args(["guide", "nonexistent"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let json = parse_json(&output);
    assert_eq!(json["status"], "error");
}

#[test]
fn review_validates_document() {
    let dir = std::env::temp_dir().join("ode_review_test");
    let file = dir.join("test.ode.json");
    // Create a doc with ode new
    ode_cmd().args(["new", file.to_str().unwrap(), "--width", "400", "--height", "300"]).output().unwrap();

    let output = ode_cmd().args(["review", file.to_str().unwrap()]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert!(json["summary"]["total"].as_u64().is_some());
}

#[test]
fn review_with_context_flag() {
    let dir = std::env::temp_dir().join("ode_review_test");
    let file = dir.join("test.ode.json");
    ode_cmd().args(["new", file.to_str().unwrap(), "--width", "400", "--height", "300"]).output().unwrap();

    let output = ode_cmd().args(["review", file.to_str().unwrap(), "--context", "print"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["context"], "print");
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p ode-cli --test integration`
Expected: All tests pass (including existing ones).

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test --workspace`
Expected: All 315+ tests pass (existing + new).

- [ ] **Step 4: Commit**

```bash
git add crates/ode-cli/tests/integration.rs
git commit -m "test(ode-cli): add integration tests for guide and review commands"
```
