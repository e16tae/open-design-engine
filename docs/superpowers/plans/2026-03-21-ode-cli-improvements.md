# ODE CLI Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 4 ODE CLI gaps that blocked programmatic design generation: negative coordinate parsing, text sizing mode exposure, gradient fill syntax, and font discovery commands.

**Architecture:** Each change is independent and additive. Tasks 1-2 modify existing CLI argument handling. Task 3 adds new FontDatabase methods + a new CLI subcommand. Task 4 adds a fill parser that converts CSS-like gradient strings into the existing Paint data model.

**Tech Stack:** Rust, clap 4 (derive API), serde_json, ode-format, ode-text (FontDatabase)

**Spec:** `docs/superpowers/specs/2026-03-20-ode-cli-improvements-design.md`

---

### Task 1: Fix negative coordinate parsing in `ode set`

**Files:**
- Modify: `crates/ode-cli/src/main.rs:117` (Set command variant)
- Test: `crates/ode-cli/tests/set_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/ode-cli/tests/set_test.rs`:

```rust
#[test]
fn set_negative_coordinates() {
    let (_dir, file, id) = setup_doc_with_frame();
    let out = ode_cmd()
        .args(["set", &file, &id, "--x", "-100", "--y", "-50"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["status"], "ok");
    let modified = resp["modified"].as_array().unwrap();
    let mod_strs: Vec<&str> = modified.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(mod_strs.contains(&"x"));
    assert!(mod_strs.contains(&"y"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-cli --test set_test set_negative_coordinates -- --nocapture`
Expected: FAIL — clap interprets `-100` as unknown flag

- [ ] **Step 3: Add allow_negative_numbers to Set command**

In `crates/ode-cli/src/main.rs`, add the attribute to the `Set` variant:

```rust
    /// Set properties on an existing node
    #[command(allow_negative_numbers = true)]
    Set {
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ode-cli --test set_test set_negative_coordinates -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run full test suite**

Run: `cargo test -p ode-cli`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/ode-cli/src/main.rs crates/ode-cli/tests/set_test.rs
git commit -m "fix(ode-cli): allow negative numbers in ode set coordinates"
```

---

### Task 2: Expose TextSizingMode via CLI

**Files:**
- Modify: `crates/ode-cli/src/main.rs:117-241` (Set + Add command variants)
- Modify: `crates/ode-cli/src/mutate.rs:168-297` (cmd_add text branch), `mutate.rs:595-1140` (cmd_set)
- Test: `crates/ode-cli/tests/set_test.rs`, `crates/ode-cli/tests/add_test.rs`

- [ ] **Step 1: Write the failing tests**

Add to `crates/ode-cli/tests/add_test.rs`:

```rust
#[test]
fn add_text_default_sizing_mode_is_auto_height() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args(["add", "text", &file, "--content", "Hello world"])
        .output()
        .unwrap();
    assert!(out.status.success());

    // Read document and verify sizing_mode
    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let text_node = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["type"] == "text")
        .expect("text node not found");
    assert_eq!(text_node["sizing_mode"], "auto-height");
}

#[test]
fn add_text_with_explicit_sizing_mode() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args([
            "add", "text", &file,
            "--content", "Hello",
            "--text-sizing", "fixed",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());

    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let text_node = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["type"] == "text")
        .expect("text node not found");
    assert_eq!(text_node["sizing_mode"], "fixed");
}
```

Add to `crates/ode-cli/tests/set_test.rs`:

```rust
#[test]
fn set_text_sizing_mode() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    let out = ode_cmd()
        .args(["add", "text", &file, "--content", "Hello"])
        .output()
        .unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text_id = resp["stable_id"].as_str().unwrap().to_string();

    let out = ode_cmd()
        .args(["set", &file, &text_id, "--text-sizing", "auto-width"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let modified = resp["modified"].as_array().unwrap();
    let mod_strs: Vec<&str> = modified.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(mod_strs.contains(&"text-sizing"));

    // Verify in document
    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let text_node = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["stable_id"] == text_id)
        .unwrap();
    assert_eq!(text_node["sizing_mode"], "auto-width");
}

#[test]
fn set_text_sizing_on_non_text_fails() {
    let (_dir, file, frame_id) = setup_doc_with_frame();
    let out = ode_cmd()
        .args(["set", &file, &frame_id, "--text-sizing", "auto-height"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-cli --test add_test add_text_default_sizing -- --nocapture`
Expected: FAIL — `--text-sizing` flag not recognized

- [ ] **Step 3: Define TextSizingArg enum in main.rs**

Add before the `Command` enum in `crates/ode-cli/src/main.rs`:

```rust
#[derive(Clone, Copy, clap::ValueEnum)]
enum TextSizingArg {
    Fixed,
    AutoHeight,
    AutoWidth,
}

impl TextSizingArg {
    fn to_sizing_mode(self) -> ode_format::typography::TextSizingMode {
        match self {
            Self::Fixed => ode_format::typography::TextSizingMode::Fixed,
            Self::AutoHeight => ode_format::typography::TextSizingMode::AutoHeight,
            Self::AutoWidth => ode_format::typography::TextSizingMode::AutoWidth,
        }
    }
}
```

- [ ] **Step 4: Add --text-sizing flag to Add and Set commands**

In `crates/ode-cli/src/main.rs`, add to the `Add` variant:

```rust
    #[arg(long, value_enum)]
    text_sizing: Option<TextSizingArg>,
```

Add to the `Set` variant:

```rust
    #[arg(long, value_enum)]
    text_sizing: Option<TextSizingArg>,
```

- [ ] **Step 5: Thread the new parameter through the match arms in main()**

In `crates/ode-cli/src/main.rs`, update the `Command::Add` destructuring (around line 434) to include `text_sizing`:

```rust
        Command::Add {
            kind,
            file,
            name,
            parent,
            index,
            width,
            height,
            fill,
            corner_radius,
            clips_content,
            content,
            font_size,
            font_family,
            shape,
            sides,
            src,
            text_sizing,  // ← add this
        } => mutate::cmd_add(
            &kind,
            &file,
            name.as_deref(),
            parent.as_deref(),
            index,
            width,
            height,
            fill.as_deref(),
            corner_radius.as_deref(),
            clips_content,
            content.as_deref(),
            font_size,
            font_family.as_deref(),
            shape.as_deref(),
            sides,
            src.as_deref(),
            text_sizing.map(|ts| ts.to_sizing_mode()),  // ← add this
        ),
```

Similarly update `Command::Set` (around line 375) to include `text_sizing` in the destructuring and pass `text_sizing.map(|ts| ts.to_sizing_mode())` to `mutate::cmd_set`.

- [ ] **Step 6: Update cmd_add to use text_sizing parameter**

In `crates/ode-cli/src/mutate.rs`, update the `cmd_add` function signature to accept `text_sizing: Option<TextSizingMode>`. In the `"text"` branch (around line 294), replace:

```rust
sizing_mode: TextSizingMode::Fixed,
```

with:

```rust
sizing_mode: text_sizing.unwrap_or(TextSizingMode::AutoHeight),
```

- [ ] **Step 7: Update cmd_set to handle text_sizing**

In `crates/ode-cli/src/mutate.rs`, update the `cmd_set` function signature to accept `text_sizing: Option<TextSizingMode>`. Add `text_sizing` to the `has_any` check. Then add a handler block after the `line_height` handler (around line 1126):

```rust
if let Some(ts) = text_sizing {
    match &mut node.kind {
        NodeKindWire::Text(d) => {
            d.sizing_mode = ts;
            modified.push("text-sizing".to_string());
        }
        _ => {
            print_json(&ErrorResponse::new(
                "INVALID_PROPERTY",
                "validate",
                "text-sizing is only valid for text nodes",
            ));
            return EXIT_INPUT;
        }
    }
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p ode-cli --test add_test --test set_test -- --nocapture`
Expected: All new and existing tests pass

- [ ] **Step 9: Commit**

```bash
git add crates/ode-cli/src/main.rs crates/ode-cli/src/mutate.rs \
       crates/ode-cli/tests/add_test.rs crates/ode-cli/tests/set_test.rs
git commit -m "feat(ode-cli): expose TextSizingMode via --text-sizing flag

Default for 'ode add text' changed from Fixed to AutoHeight,
matching the most common agent use case (text wraps within width)."
```

---

### Task 3: Add font CLI commands (list, check, audit)

**Files:**
- Modify: `crates/ode-text/src/font_db.rs` (new public methods)
- Modify: `crates/ode-cli/src/main.rs` (Fonts subcommand)
- Modify: `crates/ode-cli/src/commands.rs` (handler functions)
- Test: `crates/ode-text/src/font_db.rs` (unit tests), `crates/ode-cli/tests/workflow_test.rs` (integration tests)

#### Sub-task 3a: Add families() and weights_for_family() to FontDatabase

- [ ] **Step 1: Write unit tests for new methods**

Add to the `mod tests` block at the bottom of `crates/ode-text/src/font_db.rs`:

```rust
#[test]
fn families_returns_sorted_list() {
    let db = FontDatabase::new_system();
    if db.is_empty() {
        return;
    }
    let families = db.families();
    assert!(!families.is_empty());
    // Verify sorted
    let mut sorted = families.clone();
    sorted.sort();
    assert_eq!(families, sorted);
    // Verify no duplicates
    let unique: std::collections::HashSet<_> = families.iter().collect();
    assert_eq!(unique.len(), families.len());
}

#[test]
fn weights_for_existing_family() {
    let db = FontDatabase::new_system();
    if db.is_empty() {
        return;
    }
    let families = db.families();
    if families.is_empty() {
        return;
    }
    let weights = db.weights_for_family(&families[0]);
    assert!(!weights.is_empty());
    // Verify sorted
    let mut sorted = weights.clone();
    sorted.sort();
    assert_eq!(weights, sorted);
}

#[test]
fn weights_for_missing_family_is_empty() {
    let db = FontDatabase::new_system();
    let weights = db.weights_for_family("NonexistentFont12345");
    assert!(weights.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-text families_returns_sorted -- --nocapture`
Expected: FAIL — `families()` method not found

- [ ] **Step 3: Implement families() and weights_for_family()**

Add to the `impl FontDatabase` block in `crates/ode-text/src/font_db.rs`, after `is_empty()`:

```rust
/// Returns a sorted, deduplicated list of all font family names.
pub fn families(&self) -> Vec<String> {
    let mut names: Vec<String> = self.family_index.keys()
        .map(|k| {
            // Return the original-case name from the first entry
            let idx = self.family_index[k][0];
            self.fonts[idx].family.clone()
        })
        .collect();
    names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    names
}

/// Returns sorted list of available weights for a font family.
/// Returns empty vec if family not found.
pub fn weights_for_family(&self, family: &str) -> Vec<u16> {
    let family_lower = family.to_lowercase();
    match self.family_index.get(&family_lower) {
        Some(indices) => {
            let mut weights: Vec<u16> = indices.iter()
                .map(|&i| self.fonts[i].weight)
                .collect();
            weights.sort();
            weights.dedup();
            weights
        }
        None => vec![],
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ode-text -- --nocapture`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/ode-text/src/font_db.rs
git commit -m "feat(ode-text): add families() and weights_for_family() to FontDatabase"
```

#### Sub-task 3b: Add `ode fonts` CLI subcommand

- [ ] **Step 6: Write integration tests**

Add a new file `crates/ode-cli/tests/fonts_test.rs`:

```rust
use std::process::Command;

fn ode_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ode"))
}

#[test]
fn fonts_list_returns_json_array() {
    let out = ode_cmd()
        .args(["fonts", "list"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(resp.is_array(), "expected JSON array, got: {resp}");
    // On macOS there should be system fonts
    if cfg!(target_os = "macos") {
        assert!(!resp.as_array().unwrap().is_empty());
    }
}

#[test]
fn fonts_check_existing_font() {
    // Arial is available on macOS
    if !cfg!(target_os = "macos") {
        return;
    }
    let out = ode_cmd()
        .args(["fonts", "check", "Arial"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["available"], true);
    assert!(!resp["weights"].as_array().unwrap().is_empty());
}

#[test]
fn fonts_check_missing_font() {
    let out = ode_cmd()
        .args(["fonts", "check", "NonexistentFont12345"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["available"], false);
    assert!(resp["weights"].as_array().unwrap().is_empty());
}

#[test]
fn fonts_audit_document() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    ode_cmd()
        .args(["add", "text", &file, "--content", "Hello", "--font-family", "Inter"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args(["fonts", "audit", &file])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(resp["used"].as_array().unwrap().iter().any(|v| v == "Inter"));
}
```

- [ ] **Step 7: Run tests to verify they fail**

Run: `cargo test -p ode-cli --test fonts_test -- --nocapture`
Expected: Compilation error — `fonts` subcommand not defined

- [ ] **Step 8: Define FontAction subcommand in main.rs**

Add to `crates/ode-cli/src/main.rs`, after the `TokenAction` enum:

```rust
#[derive(Subcommand)]
enum FontAction {
    /// List all available system font families
    List,
    /// Check if a font family is available and show its weights
    Check {
        /// Font family name
        family: String,
    },
    /// Audit font usage in a document
    Audit {
        /// Input .ode file
        file: String,
    },
}
```

Add to the `Command` enum:

```rust
    /// Manage and inspect fonts
    Fonts {
        #[command(subcommand)]
        action: FontAction,
    },
```

Add the match arm in `main()`:

```rust
Command::Fonts { action } => match action {
    FontAction::List => commands::cmd_fonts_list(),
    FontAction::Check { family } => commands::cmd_fonts_check(&family),
    FontAction::Audit { file } => commands::cmd_fonts_audit(&file),
},
```

- [ ] **Step 9: Implement handler functions in commands.rs**

Add to `crates/ode-cli/src/commands.rs`. `FontDatabase` is re-exported from `ode_core` (already imported at the top of commands.rs). Use `load_document_json()` (same function used by `cmd_inspect`) to handle packed/unpacked/legacy formats correctly:

```rust
pub fn cmd_fonts_list() -> i32 {
    let db = FontDatabase::new_system();
    let families = db.families();
    let json = serde_json::to_string(&families).unwrap();
    println!("{json}");
    EXIT_OK
}

pub fn cmd_fonts_check(family: &str) -> i32 {
    let db = FontDatabase::new_system();
    let weights = db.weights_for_family(family);
    let available = !weights.is_empty();
    let resp = serde_json::json!({
        "family": family,
        "available": available,
        "weights": weights,
    });
    println!("{}", serde_json::to_string(&resp).unwrap());
    EXIT_OK
}

pub fn cmd_fonts_audit(file: &str) -> i32 {
    use ode_format::wire::NodeKindWire;
    use ode_format::style::StyleValue;

    // Use load_document_json — handles packed .ode, unpacked dirs, and legacy .ode.json
    let json = match load_document_json(file) {
        Ok(j) => j,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };
    let wire: DocumentWire = match serde_json::from_str(&json) {
        Ok(w) => w,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
            return EXIT_INPUT;
        }
    };

    let mut used_set = std::collections::BTreeSet::new();
    for node in &wire.nodes {
        if let NodeKindWire::Text(text_data) = &node.kind {
            // Collect from default_style
            match &text_data.default_style.font_family {
                StyleValue::Raw(f) => { used_set.insert(f.clone()); }
                StyleValue::Bound { resolved, .. } => { used_set.insert(resolved.clone()); }
            }
            // Collect from per-run overrides
            for run in &text_data.runs {
                if let Some(ff) = &run.style.font_family {
                    match ff {
                        StyleValue::Raw(f) => { used_set.insert(f.clone()); }
                        StyleValue::Bound { resolved, .. } => { used_set.insert(resolved.clone()); }
                    }
                }
            }
        }
    }

    let used: Vec<String> = used_set.into_iter().collect();
    let db = FontDatabase::new_system();

    let mut available = Vec::new();
    let mut missing = Vec::new();
    let mut warnings = Vec::new();

    for family in &used {
        if !db.weights_for_family(family).is_empty() {
            available.push(family.clone());
        } else {
            missing.push(family.clone());
            warnings.push(format!("{family}: not found in system, will use fallback"));
        }
    }

    let resp = serde_json::json!({
        "used": used,
        "available": available,
        "missing": missing,
        "warnings": warnings,
    });
    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
    EXIT_OK
}
```

- [ ] **Step 10: Run tests to verify they pass**

Run: `cargo test -p ode-cli --test fonts_test -- --nocapture`
Expected: All 4 tests pass

- [ ] **Step 11: Run full test suite**

Run: `cargo test -p ode-cli`
Expected: All tests pass

- [ ] **Step 12: Commit**

```bash
git add crates/ode-cli/src/main.rs crates/ode-cli/src/commands.rs \
       crates/ode-cli/tests/fonts_test.rs
git commit -m "feat(ode-cli): add 'ode fonts' subcommand (list, check, audit)

Enables AI agents to discover available system fonts, verify font
availability before use, and audit font references in documents."
```

---

### Task 4: Add gradient CLI support (CSS-like syntax)

**Files:**
- Modify: `crates/ode-cli/src/mutate.rs` (new `parse_fill` function, update fill handling in cmd_add and cmd_set)
- Test: `crates/ode-cli/tests/set_test.rs`, `crates/ode-cli/tests/add_test.rs`

#### Sub-task 4a: Implement parse_fill() function with unit tests

- [ ] **Step 1: Write unit tests for the parser**

Add at the bottom of `crates/ode-cli/src/mutate.rs`, in a new `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fill_solid_hex() {
        let fill = parse_fill("#FF0000").unwrap();
        assert!(matches!(fill.paint, Paint::Solid { .. }));
    }

    #[test]
    fn parse_fill_linear_gradient_basic() {
        let fill = parse_fill("linear-gradient(90deg, #FF0000, #0000FF)").unwrap();
        match &fill.paint {
            Paint::LinearGradient { stops, start, end } => {
                assert_eq!(stops.len(), 2);
                assert!((start.x - 0.0).abs() < 0.01); // 90deg: left-to-right
                assert!((end.x - 1.0).abs() < 0.01);
            }
            _ => panic!("expected LinearGradient"),
        }
    }

    #[test]
    fn parse_fill_linear_gradient_with_stops() {
        let fill = parse_fill("linear-gradient(180deg, #FF0000 0%, #00FF00 50%, #0000FF 100%)").unwrap();
        match &fill.paint {
            Paint::LinearGradient { stops, .. } => {
                assert_eq!(stops.len(), 3);
                assert!((stops[0].position - 0.0).abs() < 0.01);
                assert!((stops[1].position - 0.5).abs() < 0.01);
                assert!((stops[2].position - 1.0).abs() < 0.01);
            }
            _ => panic!("expected LinearGradient"),
        }
    }

    #[test]
    fn parse_fill_radial_gradient() {
        let fill = parse_fill("radial-gradient(#FF0000, #0000FF)").unwrap();
        match &fill.paint {
            Paint::RadialGradient { stops, center, radius } => {
                assert_eq!(stops.len(), 2);
                assert!((center.x - 0.5).abs() < 0.01);
                assert!((center.y - 0.5).abs() < 0.01);
                assert!((radius.x - 0.5).abs() < 0.01);
                assert!((radius.y - 0.5).abs() < 0.01);
            }
            _ => panic!("expected RadialGradient"),
        }
    }

    #[test]
    fn parse_fill_invalid_gradient() {
        assert!(parse_fill("linear-gradient(abc, #FF0000)").is_err());
        assert!(parse_fill("linear-gradient(90deg)").is_err());
        assert!(parse_fill("linear-gradient(90deg, xyz)").is_err());
    }

    #[test]
    fn parse_fill_angle_0deg() {
        let fill = parse_fill("linear-gradient(0deg, #000, #FFF)").unwrap();
        match &fill.paint {
            Paint::LinearGradient { start, end, .. } => {
                // 0deg = bottom-to-top
                assert!((start.x - 0.5).abs() < 0.01);
                assert!((start.y - 1.0).abs() < 0.01);
                assert!((end.x - 0.5).abs() < 0.01);
                assert!((end.y - 0.0).abs() < 0.01);
            }
            _ => panic!("expected LinearGradient"),
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-cli parse_fill -- --nocapture`
Expected: FAIL — `parse_fill` function not found

- [ ] **Step 3: Implement parse_fill() function**

Add to `crates/ode-cli/src/mutate.rs`, after the `make_solid_fill` function.

**Import note:** `GradientStop` and `Point` must be added to the **existing** `use ode_format::style::` import at line 12, not as a separate `use` statement. Update line 12 to:
```rust
use ode_format::style::{GradientStop, ImageSource, Point, StrokeCap, StrokeJoin, StrokePosition};
```

Then add the functions:

/// Parse a fill value from CLI input.
/// Accepts:
/// - "#RRGGBB" or "#RGB" → solid fill
/// - "linear-gradient(angle, color1 pos%, color2 pos%, ...)" → linear gradient
/// - "radial-gradient(color1 pos%, color2 pos%, ...)" → radial gradient
fn parse_fill(s: &str) -> Result<Fill, String> {
    let s = s.trim();

    if s.starts_with('#') {
        let color = parse_color(s)?;
        return Ok(make_solid_fill(color));
    }

    if let Some(inner) = s.strip_prefix("linear-gradient(").and_then(|s| s.strip_suffix(')')) {
        return parse_linear_gradient(inner);
    }

    if let Some(inner) = s.strip_prefix("radial-gradient(").and_then(|s| s.strip_suffix(')')) {
        return parse_radial_gradient(inner);
    }

    Err(format!("invalid fill value: expected '#RRGGBB', 'linear-gradient(...)', or 'radial-gradient(...)'; got '{s}'"))
}

fn parse_linear_gradient(inner: &str) -> Result<Fill, String> {
    let parts: Vec<&str> = split_gradient_args(inner);
    if parts.is_empty() {
        return Err("invalid fill value: empty linear-gradient".to_string());
    }

    // First part should be angle
    let angle_str = parts[0].trim();
    let angle_deg: f32 = if let Some(deg_str) = angle_str.strip_suffix("deg") {
        deg_str.trim().parse::<f32>()
            .map_err(|_| format!("invalid fill value: expected angle like '90deg', got '{angle_str}'"))?
    } else {
        return Err(format!("invalid fill value: expected angle like '90deg', got '{angle_str}'"));
    };

    let color_parts = &parts[1..];
    if color_parts.len() < 2 {
        return Err("invalid fill value: at least 2 color stops required".to_string());
    }

    let stops = parse_color_stops(color_parts)?;

    // Convert angle to start/end points
    let angle_rad = angle_deg.to_radians();
    let start = Point {
        x: 0.5 - 0.5 * angle_rad.sin(),
        y: 0.5 + 0.5 * angle_rad.cos(),
    };
    let end = Point {
        x: 0.5 + 0.5 * angle_rad.sin(),
        y: 0.5 - 0.5 * angle_rad.cos(),
    };

    Ok(Fill {
        paint: Paint::LinearGradient { stops, start, end },
        opacity: StyleValue::Raw(1.0),
        blend_mode: BlendMode::Normal,
        visible: true,
    })
}

fn parse_radial_gradient(inner: &str) -> Result<Fill, String> {
    let parts: Vec<&str> = split_gradient_args(inner);
    if parts.len() < 2 {
        return Err("invalid fill value: at least 2 color stops required".to_string());
    }

    let stops = parse_color_stops(&parts)?;

    Ok(Fill {
        paint: Paint::RadialGradient {
            stops,
            center: Point { x: 0.5, y: 0.5 },
            radius: Point { x: 0.5, y: 0.5 },
        },
        opacity: StyleValue::Raw(1.0),
        blend_mode: BlendMode::Normal,
        visible: true,
    })
}

/// Split gradient arguments by comma, respecting whitespace.
fn split_gradient_args(s: &str) -> Vec<&str> {
    s.split(',').map(|p| p.trim()).filter(|p| !p.is_empty()).collect()
}

/// Parse color stops like "#FF0000 50%" or "#FF0000".
/// If no positions given, distributes evenly.
fn parse_color_stops(parts: &[&str]) -> Result<Vec<GradientStop>, String> {
    let mut stops = Vec::with_capacity(parts.len());
    let mut has_positions = false;

    for (i, part) in parts.iter().enumerate() {
        let tokens: Vec<&str> = part.split_whitespace().collect();
        let color_str = tokens[0];
        let color = parse_color(color_str)
            .map_err(|_| format!("invalid fill value: invalid color '{color_str}'"))?;

        let position = if tokens.len() > 1 {
            let pos_str = tokens[1];
            has_positions = true;
            pos_str.strip_suffix('%')
                .and_then(|v| v.parse::<f32>().ok())
                .map(|v| v / 100.0)
                .ok_or_else(|| format!("invalid fill value: invalid stop position '{pos_str}'"))?
        } else {
            // Will be filled in below if no explicit positions
            i as f32 / (parts.len() - 1).max(1) as f32
        };

        stops.push(GradientStop {
            position,
            color: StyleValue::Raw(color),
        });
    }

    // If mixed (some with positions, some without), the ones without
    // already got evenly distributed defaults, which is acceptable
    Ok(stops)
}
```

- [ ] **Step 4: Run unit tests to verify they pass**

Run: `cargo test -p ode-cli parse_fill -- --nocapture`
Expected: All 6 parse_fill tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/ode-cli/src/mutate.rs
git commit -m "feat(ode-cli): add parse_fill() for CSS-like gradient syntax"
```

#### Sub-task 4b: Wire parse_fill into cmd_add and cmd_set

- [ ] **Step 6: Write integration tests**

Add to `crates/ode-cli/tests/add_test.rs`:

```rust
#[test]
fn add_frame_with_linear_gradient_fill() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args([
            "add", "frame", &file,
            "--width", "400", "--height", "300",
            "--fill", "linear-gradient(90deg, #FF0000, #0000FF)",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let frame = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["type"] == "frame" && n["name"] == "Frame")
        .unwrap();
    let fill_type = frame["fills"][0]["paint"]["type"].as_str().unwrap();
    assert_eq!(fill_type, "linear-gradient");
}
```

Add to `crates/ode-cli/tests/set_test.rs`:

```rust
#[test]
fn set_radial_gradient_fill() {
    let (_dir, file, id) = setup_doc_with_frame();
    let out = ode_cmd()
        .args([
            "set", &file, &id,
            "--fill", "radial-gradient(#16C1F3, #0A1628)",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let node = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["stable_id"].as_str() == Some(&id))
        .unwrap();
    let fill_type = node["fills"][0]["paint"]["type"].as_str().unwrap();
    assert_eq!(fill_type, "radial-gradient");
}

#[test]
fn set_solid_replaces_gradient() {
    let (_dir, file, id) = setup_doc_with_frame();
    // First set gradient
    ode_cmd()
        .args(["set", &file, &id, "--fill", "linear-gradient(90deg, #FF0000, #0000FF)"])
        .output()
        .unwrap();
    // Then replace with solid
    let out = ode_cmd()
        .args(["set", &file, &id, "--fill", "#00FF00"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let node = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["stable_id"].as_str() == Some(&id))
        .unwrap();
    let fill_type = node["fills"][0]["paint"]["type"].as_str().unwrap();
    assert_eq!(fill_type, "solid");
}

#[test]
fn set_invalid_gradient_fails() {
    let (_dir, file, id) = setup_doc_with_frame();
    let out = ode_cmd()
        .args(["set", &file, &id, "--fill", "linear-gradient(abc, #FF0000)"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}
```

- [ ] **Step 7: Run tests to verify they fail**

Run: `cargo test -p ode-cli --test add_test add_frame_with_linear -- --nocapture`
Expected: FAIL — gradient string parsed as invalid hex color

- [ ] **Step 8: Replace parse_color calls with parse_fill in cmd_add**

In `crates/ode-cli/src/mutate.rs`, in `cmd_add` (around line 197-207), replace the fill parsing block:

```rust
// Parse optional fill
let fill_parsed = if let Some(fill_str) = fill {
    match parse_fill(fill_str) {
        Ok(f) => Some(f),
        Err(msg) => {
            print_json(&ErrorResponse::new("INVALID_VALUE", "parse", &msg));
            return EXIT_INPUT;
        }
    }
} else {
    None
};
```

Then update all `if let Some(color) = fill_color.clone()` blocks in cmd_add to use `fill_parsed`:

```rust
if let Some(fill) = fill_parsed.clone() {
    visual.fills.push(fill);
}
```

- [ ] **Step 9: Replace parse_color calls with parse_fill in cmd_set**

In `cmd_set` (around line 775-802), replace the fill handling:

```rust
if let Some(fill_str) = fill {
    let new_fill = match parse_fill(fill_str) {
        Ok(f) => f,
        Err(msg) => {
            print_json(&ErrorResponse::new("INVALID_VALUE", "validate", &msg));
            return EXIT_INPUT;
        }
    };
    match DocumentWire::visual_props_mut(&mut node.kind) {
        Some(visual) => {
            if visual.fills.is_empty() {
                visual.fills.push(new_fill);
            } else {
                visual.fills[0] = new_fill;
            }
            modified.push("fill".to_string());
        }
        None => {
            print_json(&ErrorResponse::new(
                "INVALID_PROPERTY",
                "validate",
                "fill is not valid for this node type",
            ));
            return EXIT_INPUT;
        }
    }
}
```

- [ ] **Step 10: Run tests to verify they pass**

Run: `cargo test -p ode-cli -- --nocapture`
Expected: All tests pass (existing solid fill tests still work, new gradient tests pass)

- [ ] **Step 11: Commit**

```bash
git add crates/ode-cli/src/mutate.rs crates/ode-cli/tests/add_test.rs \
       crates/ode-cli/tests/set_test.rs
git commit -m "feat(ode-cli): support CSS-like gradient syntax in --fill

Accepts 'linear-gradient(angle, stops...)' and 'radial-gradient(stops...)'
in addition to existing '#RRGGBB' hex colors."
```

---

### Task 5: Final validation

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass across all crates

- [ ] **Step 2: Manual smoke test with poster workflow**

```bash
ODE=./target/release/ode
cargo build --release

# Create document
$ODE new /tmp/test-poster.ode --width 800 --height 600

# Add frame with gradient
$ODE add frame /tmp/test-poster.ode --width 800 --height 600 \
  --fill "linear-gradient(180deg, #16C1F3, #0A1628)"

# Add text with auto-height
$ODE add text /tmp/test-poster.ode --content "Hello AIOBIO" \
  --font-size 48 --text-sizing auto-height

# Set negative coordinates
$ODE set /tmp/test-poster.ode <text_id> --x 40 --y -20

# Check fonts
$ODE fonts list
$ODE fonts check "Inter"
$ODE fonts audit /tmp/test-poster.ode

# Build
$ODE build /tmp/test-poster.ode -o /tmp/test-poster.png
```

Expected: All commands succeed, PNG renders correctly

- [ ] **Step 3: Commit any remaining fixes**

If any issues found in smoke test, fix and commit.
