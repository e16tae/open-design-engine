# Figma Import Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Import Figma REST API JSON responses into ODE Document format via a new `ode-import` crate.

**Architecture:** Two-pass DFS converter (pre-pass for ID assignment, main pass for conversion) reads Figma API JSON (deserialized into Rust types), maps each node/style/layout property to ODE equivalents, and produces a complete `Document`. Separate HTTP client handles API calls and image downloads. CLI gets an `import figma` subcommand.

**Tech Stack:** Rust, serde/serde_json, reqwest (HTTP), tokio (async), thiserror, clap, nanoid

**Spec:** `docs/superpowers/specs/2026-03-14-figma-import-design.md`

---

## File Structure

### New files
| File | Responsibility |
|---|---|
| `crates/ode-import/Cargo.toml` | Crate manifest with dependencies |
| `crates/ode-import/src/lib.rs` | Public module exports |
| `crates/ode-import/src/error.rs` | `ImportError`, `ImportWarning` types |
| `crates/ode-import/src/figma/mod.rs` | Figma module re-exports |
| `crates/ode-import/src/figma/types.rs` | Figma REST API serde structs |
| `crates/ode-import/src/figma/convert.rs` | `FigmaConverter` — all conversion logic |
| `crates/ode-import/src/figma/convert_style.rs` | Paint, effect, blend mode conversions |
| `crates/ode-import/src/figma/convert_text.rs` | Text style & run conversion |
| `crates/ode-import/src/figma/convert_layout.rs` | Auto layout & constraint conversion |
| `crates/ode-import/src/figma/convert_tokens.rs` | Variables → DesignTokens conversion |
| `crates/ode-import/src/figma/svg_path.rs` | SVG path string → VectorPath parser |
| `crates/ode-import/src/figma/client.rs` | Figma HTTP API client |

### Modified files
| File | Change |
|---|---|
| `Cargo.toml` (workspace root) | Add `ode-import` to workspace members + new deps |
| `crates/ode-format/src/node.rs` | Add `visible` to `Node`, `clips_content` to `FrameData` |
| `crates/ode-format/src/wire.rs` | Mirror `visible`/`clips_content` in wire types |
| `crates/ode-cli/Cargo.toml` | Add `ode-import`, `tokio` dependencies |
| `crates/ode-cli/src/main.rs` | Add `Import` command variant |
| `crates/ode-cli/src/commands.rs` | Add `cmd_import_figma()` |

---

## Chunk 1: Foundation — ode-format changes + crate scaffold

### Task 1: Add `visible` and `clips_content` to ode-format

**Files:**
- Modify: `crates/ode-format/src/node.rs:458-474` (Node struct)
- Modify: `crates/ode-format/src/node.rs:340-354` (FrameData struct)
- Modify: `crates/ode-format/src/node.rs:480-502` (Node::new_frame)
- Modify: `crates/ode-format/src/wire.rs` (NodeWire, FrameDataWire if present)

- [ ] **Step 1: Add `visible` field to `Node` struct**

In `crates/ode-format/src/node.rs`, add to the `Node` struct after `blend_mode`:
```rust
#[serde(default = "default_visible")]
pub visible: bool,
```
Add the default function:
```rust
fn default_visible() -> bool { true }
```

- [ ] **Step 2: Add `clips_content` field to `FrameData` struct**

In `crates/ode-format/src/node.rs`, add to `FrameData` after `corner_radius`:
```rust
#[serde(default = "default_clips_content")]
pub clips_content: bool,
```
Add:
```rust
fn default_clips_content() -> bool { true }
```

- [ ] **Step 3: Update `Node::new_frame`, `new_group`, `new_vector`, `new_text` constructors**

Add `visible: true` to each constructor. Add `clips_content: true` to `FrameData` in `new_frame`.

- [ ] **Step 4: Update wire.rs serialization**

Search `wire.rs` for `NodeWire` and `FrameDataWire` (or equivalent). Add `visible` and `clips_content` fields mirroring the same serde attributes.

- [ ] **Step 5: Run existing tests**

```bash
cd /Users/lmuffin/Documents/Workspace/open-design-engine && cargo test -p ode-format
```
Expected: All existing tests pass (new fields have defaults, so serialization is backward-compatible).

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/
git commit -m "feat(ode-format): add visible and clips_content fields to Node/FrameData"
```

### Task 2: Create ode-import crate scaffold

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/ode-import/Cargo.toml`
- Create: `crates/ode-import/src/lib.rs`
- Create: `crates/ode-import/src/error.rs`
- Create: `crates/ode-import/src/figma/mod.rs`

- [ ] **Step 1: Add workspace dependencies and member**

In root `Cargo.toml`, add to `[workspace]` members:
```toml
members = [
    "crates/ode-format",
    "crates/ode-text",
    "crates/ode-core",
    "crates/ode-export",
    "crates/ode-cli",
    "crates/ode-import",
]
```

Add to `[workspace.dependencies]`:
```toml
# HTTP client
reqwest = { version = "0.12", features = ["json"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

- [ ] **Step 2: Create ode-import Cargo.toml**

Create `crates/ode-import/Cargo.toml`:
```toml
[package]
name = "ode-import"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "ODE import — convert design files from external formats"

[dependencies]
ode-format = { path = "../ode-format" }
serde = { workspace = true }
serde_json = { workspace = true }
reqwest = { workspace = true }
tokio = { workspace = true }
nanoid = { workspace = true }
thiserror = { workspace = true }
```

- [ ] **Step 3: Create error.rs**

Create `crates/ode-import/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Figma API error: {status} - {message}")]
    Api { status: u32, message: String },

    #[error("Missing required field: {field} on node {node_id}")]
    MissingField { node_id: String, field: String },
}

#[derive(Debug, Clone)]
pub struct ImportWarning {
    pub node_id: String,
    pub node_name: String,
    pub message: String,
}

impl std::fmt::Display for ImportWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.node_id, self.node_name, self.message)
    }
}
```

- [ ] **Step 4: Create figma/mod.rs**

Create `crates/ode-import/src/figma/mod.rs`:
```rust
pub mod types;
pub mod convert;
mod convert_style;
mod convert_text;
mod convert_layout;
mod convert_tokens;
mod svg_path;
pub mod client;
```

- [ ] **Step 5: Create lib.rs**

Create `crates/ode-import/src/lib.rs`:
```rust
pub mod error;
pub mod figma;

pub use error::{ImportError, ImportWarning};
```

- [ ] **Step 6: Create stub files for all modules**

Create empty stubs for `convert.rs`, `convert_style.rs`, `convert_text.rs`, `convert_layout.rs`, `convert_tokens.rs`, `svg_path.rs`, `client.rs` — each with just a comment `// TODO` so the crate compiles.

- [ ] **Step 7: Verify the crate compiles**

```bash
cargo check -p ode-import
```
Expected: Compiles with no errors.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ode-import/
git commit -m "feat(ode-import): scaffold crate with error types and module structure"
```

---

## Chunk 2: Figma Types + SVG Path Parser

### Task 3: Define Figma REST API types

**Files:**
- Create: `crates/ode-import/src/figma/types.rs`

- [ ] **Step 1: Write all Figma type definitions**

Create `crates/ode-import/src/figma/types.rs` with all types from the spec's "Figma Type Definitions" section. Use `#[serde(rename_all = "camelCase")]` on all structs. Every field except `id`, `name`, `node_type` should be `Option<T>`. Use `#[serde(default)]` liberally.

Key structs: `FigmaFileResponse`, `FigmaNode`, `FigmaPaint`, `FigmaColor`, `FigmaColorStop`, `FigmaEffect`, `FigmaTypeStyle`, `FigmaVariablesResponse`, `FigmaVariablesMeta`, `FigmaVariableCollection`, `FigmaVariable`, `FigmaVariableAlias`, `FigmaRect`, `FigmaVector`, `FigmaPath`, `FigmaLayoutConstraint`, `FigmaComponentMeta`, `FigmaOverride`, `FigmaComponentProperty`.

Refer to spec lines 55-334 for exact field names and types.

- [ ] **Step 2: Write deserialization test with minimal JSON**

Add test at bottom of `types.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_minimal_file_response() {
        let json = r#"{
            "name": "Test File",
            "document": {
                "id": "0:0",
                "name": "Document",
                "type": "DOCUMENT",
                "children": []
            },
            "components": {},
            "componentSets": {},
            "schemaVersion": 0,
            "styles": {}
        }"#;
        let response: FigmaFileResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.name, "Test File");
        assert_eq!(response.document.node_type, "DOCUMENT");
    }

    #[test]
    fn deserialize_frame_node_with_fills() {
        let json = r#"{
            "id": "1:1",
            "name": "Frame",
            "type": "FRAME",
            "fills": [
                {"type": "SOLID", "color": {"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0}}
            ],
            "absoluteBoundingBox": {"x": 0, "y": 0, "width": 100, "height": 100},
            "size": {"x": 100, "y": 100},
            "blendMode": "NORMAL",
            "children": []
        }"#;
        let node: FigmaNode = serde_json::from_str(json).unwrap();
        assert_eq!(node.fills.unwrap().len(), 1);
    }

    #[test]
    fn deserialize_text_node_with_style() {
        let json = r#"{
            "id": "2:1",
            "name": "Title",
            "type": "TEXT",
            "characters": "Hello World",
            "style": {
                "fontFamily": "Inter",
                "fontWeight": 400,
                "fontSize": 16,
                "textAlignHorizontal": "LEFT",
                "textAlignVertical": "TOP",
                "letterSpacing": 0,
                "lineHeightPx": 24,
                "lineHeightUnit": "PIXELS"
            },
            "characterStyleOverrides": [0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1],
            "styleOverrideTable": {
                "1": {"fontWeight": 700}
            }
        }"#;
        let node: FigmaNode = serde_json::from_str(json).unwrap();
        assert_eq!(node.characters.unwrap(), "Hello World");
        assert_eq!(node.character_style_overrides.unwrap().len(), 11);
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p ode-import -- types::tests
```
Expected: All 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ode-import/src/figma/types.rs
git commit -m "feat(ode-import): define Figma REST API serde types"
```

### Task 4: Implement SVG path parser

**Files:**
- Create: `crates/ode-import/src/figma/svg_path.rs`

- [ ] **Step 1: Write failing tests for SVG path parsing**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::node::PathSegment;

    #[test]
    fn parse_simple_rect() {
        let path = parse_svg_path("M 0 0 L 100 0 L 100 100 L 0 100 Z").unwrap();
        assert_eq!(path.segments.len(), 5);
        assert!(matches!(path.segments[0], PathSegment::MoveTo { x, y } if x == 0.0 && y == 0.0));
        assert!(matches!(path.segments[4], PathSegment::Close));
        assert!(path.closed);
    }

    #[test]
    fn parse_relative_commands() {
        let path = parse_svg_path("M 10 10 l 50 0 l 0 50 z").unwrap();
        assert_eq!(path.segments.len(), 4);
        // l 50 0 from (10,10) = LineTo(60, 10)
        assert!(matches!(path.segments[1], PathSegment::LineTo { x, y } if x == 60.0 && y == 10.0));
    }

    #[test]
    fn parse_cubic_bezier() {
        let path = parse_svg_path("M 0 0 C 10 20 30 40 50 60").unwrap();
        assert_eq!(path.segments.len(), 2);
        assert!(matches!(path.segments[1], PathSegment::CurveTo { .. }));
    }

    #[test]
    fn parse_h_v_commands() {
        let path = parse_svg_path("M 0 0 H 100 V 50").unwrap();
        assert_eq!(path.segments.len(), 3);
        assert!(matches!(path.segments[1], PathSegment::LineTo { x, y } if x == 100.0 && y == 0.0));
        assert!(matches!(path.segments[2], PathSegment::LineTo { x, y } if x == 100.0 && y == 50.0));
    }

    #[test]
    fn parse_quadratic() {
        let path = parse_svg_path("M 0 0 Q 50 100 100 0").unwrap();
        assert_eq!(path.segments.len(), 2);
        assert!(matches!(path.segments[1], PathSegment::QuadTo { .. }));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p ode-import -- svg_path::tests
```
Expected: FAIL — `parse_svg_path` not defined.

- [ ] **Step 3: Implement the SVG path parser**

Implement `pub fn parse_svg_path(input: &str) -> Result<VectorPath, ImportError>` that:
1. Tokenizes the SVG path string into commands + coordinates
2. Handles absolute (`M L H V C S Q T Z`) and relative (`m l h v c s q t z`) commands
3. Tracks current position for relative→absolute conversion
4. For `S` (smooth cubic): reflects previous control point
5. For `T` (smooth quad): reflects previous control point
6. Returns `VectorPath { segments, closed }` where `closed = true` if ends with `Z`

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p ode-import -- svg_path::tests
```
Expected: All 5 tests pass.

- [ ] **Step 5: Add SVG arc (`A`) command support**

Implement arc-to-cubic-bezier approximation. The `A` command has parameters `rx ry x-rotation large-arc-flag sweep-flag x y`. Convert to a series of cubic bezier `CurveTo` segments using the standard endpoint-to-center parameterization algorithm. Add test:
```rust
#[test]
fn parse_arc_command() {
    // Half circle arc
    let path = parse_svg_path("M 0 50 A 50 50 0 0 1 100 50").unwrap();
    // Arc converts to one or more CurveTo segments
    assert!(path.segments.len() >= 2); // MoveTo + at least one CurveTo
    assert!(matches!(path.segments[0], PathSegment::MoveTo { .. }));
    assert!(matches!(path.segments[1], PathSegment::CurveTo { .. }));
}
```

- [ ] **Step 6: Add tests for `S` (smooth cubic) and `T` (smooth quad)**

```rust
#[test]
fn parse_smooth_cubic() {
    let path = parse_svg_path("M 0 0 C 10 20 30 40 50 50 S 80 60 100 50").unwrap();
    assert_eq!(path.segments.len(), 3); // M, C, C (S becomes C with reflected cp)
}

#[test]
fn parse_smooth_quad() {
    let path = parse_svg_path("M 0 0 Q 50 100 100 0 T 200 0").unwrap();
    assert_eq!(path.segments.len(), 3); // M, Q, Q (T becomes Q with reflected cp)
}
```

- [ ] **Step 7: Run tests**

```bash
cargo test -p ode-import -- svg_path::tests
```
Expected: All pass.

- [ ] **Step 9: Add `merge_paths` function**

Add a function to merge multiple FigmaPaths into one VectorPath:
```rust
pub fn merge_figma_paths(
    paths: &[FigmaPath],
) -> Result<(VectorPath, FillRule), ImportError> {
    let mut all_segments = Vec::new();
    let mut closed = false;
    for fp in paths {
        let vp = parse_svg_path(&fp.path)?;
        all_segments.extend(vp.segments);
        closed = closed || vp.closed;
    }
    let fill_rule = paths.first()
        .and_then(|p| p.winding_rule.as_deref())
        .map(|w| match w {
            "EVENODD" => FillRule::EvenOdd,
            _ => FillRule::NonZero,
        })
        .unwrap_or(FillRule::NonZero);
    Ok((VectorPath { segments: all_segments, closed }, fill_rule))
}
```

- [ ] **Step 10: Test merge_paths**

```rust
#[test]
fn merge_multiple_paths() {
    let paths = vec![
        FigmaPath { path: "M 0 0 L 10 10 Z".into(), winding_rule: Some("EVENODD".into()), overridden_fields: None },
        FigmaPath { path: "M 20 20 L 30 30 Z".into(), winding_rule: None, overridden_fields: None },
    ];
    let (vp, rule) = merge_figma_paths(&paths).unwrap();
    assert_eq!(vp.segments.len(), 6); // M L Z M L Z
    assert_eq!(rule, FillRule::EvenOdd);
}
```

- [ ] **Step 11: Run all tests**

```bash
cargo test -p ode-import
```
Expected: All pass.

- [ ] **Step 12: Commit**

```bash
git add crates/ode-import/src/figma/svg_path.rs
git commit -m "feat(ode-import): implement SVG path string parser with arc support"
```

---

## Chunk 3: Style Conversion (Paint, Effect, BlendMode, Stroke)

### Task 5: Implement style conversions

**Files:**
- Create: `crates/ode-import/src/figma/convert_style.rs`

- [ ] **Step 1: Write failing tests for paint conversion**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_solid_paint() {
        let fp = FigmaPaint {
            paint_type: "SOLID".into(),
            color: Some(FigmaColor { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
            opacity: Some(0.5),
            visible: Some(true),
            blend_mode: Some("NORMAL".into()),
            ..Default::default()
        };
        let mut warnings = Vec::new();
        let fill = convert_fill(&fp, &mut warnings).unwrap();
        assert!(matches!(fill.paint, Paint::Solid { .. }));
        assert_eq!(fill.opacity.value(), 0.5);
    }

    #[test]
    fn convert_linear_gradient_paint() {
        let fp = FigmaPaint {
            paint_type: "GRADIENT_LINEAR".into(),
            gradient_handle_positions: Some(vec![
                FigmaVector { x: 0.0, y: 0.5 },
                FigmaVector { x: 1.0, y: 0.5 },
                FigmaVector { x: 0.0, y: 1.0 },
            ]),
            gradient_stops: Some(vec![
                FigmaColorStop { position: 0.0, color: FigmaColor { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }, bound_variables: None },
                FigmaColorStop { position: 1.0, color: FigmaColor { r: 0.0, g: 0.0, b: 1.0, a: 1.0 }, bound_variables: None },
            ]),
            ..Default::default()
        };
        let mut warnings = Vec::new();
        let fill = convert_fill(&fp, &mut warnings).unwrap();
        assert!(matches!(fill.paint, Paint::LinearGradient { .. }));
    }

    #[test]
    fn convert_unsupported_paint_returns_none() {
        let fp = FigmaPaint { paint_type: "VIDEO".into(), ..Default::default() };
        let mut warnings = Vec::new();
        let fill = convert_fill(&fp, &mut warnings);
        assert!(fill.is_none());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn convert_blend_mode() {
        assert_eq!(convert_blend_mode("MULTIPLY"), BlendMode::Multiply);
        assert_eq!(convert_blend_mode("PASS_THROUGH"), BlendMode::Normal);
    }

    #[test]
    fn convert_drop_shadow_effect() {
        let fe = FigmaEffect {
            effect_type: "DROP_SHADOW".into(),
            visible: Some(true),
            radius: Some(4.0),
            color: Some(FigmaColor { r: 0.0, g: 0.0, b: 0.0, a: 0.25 }),
            offset: Some(FigmaVector { x: 0.0, y: 4.0 }),
            spread: Some(0.0),
            ..Default::default()
        };
        let effect = convert_effect(&fe, &mut Vec::new()).unwrap();
        assert!(matches!(effect, Effect::DropShadow { .. }));
    }

    #[test]
    fn convert_stroke_properties() {
        let stroke = convert_stroke_props(
            Some("INSIDE"),
            Some("ROUND"),
            Some("BEVEL"),
            Some(28.96),
            Some(&[5.0, 3.0]),
            &mut Vec::new(),
        );
        assert_eq!(stroke.position, StrokePosition::Inside);
        assert_eq!(stroke.cap, StrokeCap::Round);
        assert_eq!(stroke.join, StrokeJoin::Bevel);
        assert!(stroke.dash.is_some());
    }

    #[test]
    fn convert_image_scale_mode() {
        assert_eq!(convert_image_fill_mode("FILL"), ImageFillMode::Fill);
        assert_eq!(convert_image_fill_mode("FIT"), ImageFillMode::Fit);
        assert_eq!(convert_image_fill_mode("TILE"), ImageFillMode::Tile);
        // STRETCH falls back to Fill (no ODE equivalent)
        assert_eq!(convert_image_fill_mode("STRETCH"), ImageFillMode::Fill);
    }

    #[test]
    fn convert_miter_angle_to_limit() {
        let limit = convert_miter(28.96);
        // 1 / sin(14.48°) ≈ 4.0
        assert!((limit - 4.0).abs() < 0.1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p ode-import -- convert_style::tests
```
Expected: FAIL.

- [ ] **Step 3: Implement conversion functions**

Implement in `convert_style.rs`:
- `pub fn convert_color(c: &FigmaColor) -> Color` — FigmaColor → Color::Srgb
- `pub fn convert_blend_mode(s: &str) -> BlendMode` — string match with fallback to Normal + warning for LINEAR_BURN/LINEAR_DODGE
- `pub fn convert_fill(fp: &FigmaPaint, warnings: &mut Vec<ImportWarning>) -> Option<Fill>` — handles SOLID, all gradients, IMAGE, returns None for unsupported. Collects `imageRef` strings for IMAGE paints and pushes them to warnings with a special tag for later collection.
- `pub fn convert_stroke(fp: &FigmaPaint, weight: f32, stroke_props: StrokeProps, warnings: &mut Vec<ImportWarning>) -> Option<Stroke>` — paint + stroke-specific fields
- `pub fn convert_effect(fe: &FigmaEffect, warnings: &mut Vec<ImportWarning>) -> Option<Effect>` — shadow radius→blur, etc.
- `pub fn convert_miter(miter_angle_deg: f32) -> f32` — `1.0 / sin(angle/2)`
- `pub fn convert_stroke_props(...)` — stroke_align→StrokePosition, stroke_cap→StrokeCap, etc.

Refer to spec "Paint Mapping", "Effect Mapping", "BlendMode Mapping", "Stroke Conversion Details" sections.

- [ ] **Step 4: Run tests**

```bash
cargo test -p ode-import -- convert_style::tests
```
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ode-import/src/figma/convert_style.rs
git commit -m "feat(ode-import): implement paint, effect, blend mode, stroke conversion"
```

---

## Chunk 4: Text + Layout Conversion

### Task 6: Implement text style conversion

**Files:**
- Create: `crates/ode-import/src/figma/convert_text.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_basic_text_style() {
        let fts = FigmaTypeStyle {
            font_family: Some("Inter".into()),
            font_weight: Some(400.0),
            font_size: Some(16.0),
            text_align_horizontal: Some("LEFT".into()),
            text_align_vertical: Some("TOP".into()),
            letter_spacing: Some(0.0),
            line_height_px: Some(24.0),
            line_height_unit: Some("PIXELS".into()),
            ..Default::default()
        };
        let style = convert_text_style(&fts);
        assert_eq!(style.font_family.value(), "Inter");
        assert_eq!(style.font_weight.value(), 400);
        assert_eq!(style.font_size.value(), 16.0);
    }

    #[test]
    fn convert_text_runs_ascii() {
        // "Hello World" with overrides: first 5 chars style 0, next 6 chars style 1
        let content = "Hello World";
        let overrides = vec![0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1];
        let mut table = HashMap::new();
        table.insert("1".to_string(), FigmaTypeStyle {
            font_weight: Some(700.0),
            ..Default::default()
        });
        let runs = convert_text_runs(content, &overrides, &table);
        assert_eq!(runs.len(), 1); // Only style 1 produces a run (style 0 = default)
        assert_eq!(runs[0].start, 5); // byte offset of " World"
        assert_eq!(runs[0].end, 11);
    }

    #[test]
    fn convert_text_runs_emoji() {
        // Emoji "😀" is 4 bytes in UTF-8, 2 code units in UTF-16
        let content = "A😀B";
        // UTF-16 indices: A=0, 😀=1,2 (surrogate pair), B=3
        let overrides = vec![0, 1, 1, 0]; // emoji chars get style 1
        let mut table = HashMap::new();
        table.insert("1".to_string(), FigmaTypeStyle {
            font_weight: Some(700.0),
            ..Default::default()
        });
        let runs = convert_text_runs(content, &overrides, &table);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].start, 1);  // byte offset after 'A'
        assert_eq!(runs[0].end, 5);    // byte offset after '😀' (4 bytes)
    }

    #[test]
    fn convert_text_sizing_mode() {
        assert_eq!(convert_sizing_mode(Some("HEIGHT")), TextSizingMode::AutoHeight);
        assert_eq!(convert_sizing_mode(Some("WIDTH_AND_HEIGHT")), TextSizingMode::AutoWidth);
        assert_eq!(convert_sizing_mode(None), TextSizingMode::Fixed);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p ode-import -- convert_text::tests
```

- [ ] **Step 3: Implement text conversion**

Implement in `convert_text.rs`:
- `pub fn convert_text_style(fts: &FigmaTypeStyle) -> TextStyle` — maps all fields per spec "Text Conversion" table
- `pub fn convert_text_runs(content: &str, overrides: &[usize], table: &HashMap<String, FigmaTypeStyle>) -> Vec<TextRun>` — groups consecutive same-index chars, converts UTF-16 indices→UTF-8 byte offsets, builds TextRunStyle
- `pub fn convert_sizing_mode(s: Option<&str>) -> TextSizingMode`
- `fn utf16_index_to_byte_offset(content: &str, utf16_idx: usize) -> usize` — walks content chars, counting UTF-16 code units

- [ ] **Step 4: Run tests**

```bash
cargo test -p ode-import -- convert_text::tests
```
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ode-import/src/figma/convert_text.rs
git commit -m "feat(ode-import): implement text style and run conversion with UTF-16 handling"
```

### Task 7: Implement layout conversion

**Files:**
- Create: `crates/ode-import/src/figma/convert_layout.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_auto_layout_horizontal() {
        let config = convert_layout_config(
            Some("HORIZONTAL"),
            Some("MIN"),
            Some("CENTER"),
            Some(10.0), Some(10.0), Some(20.0), Some(20.0),
            Some(8.0),
            Some("NO_WRAP"),
            &mut Vec::new(),
        );
        let config = config.unwrap();
        assert_eq!(config.direction, LayoutDirection::Horizontal);
        assert_eq!(config.primary_axis_align, PrimaryAxisAlign::Start);
        assert_eq!(config.counter_axis_align, CounterAxisAlign::Center);
        assert_eq!(config.padding.left, 10.0);
        assert_eq!(config.item_spacing, 8.0);
    }

    #[test]
    fn convert_layout_sizing() {
        let sizing = convert_layout_sizing(
            Some("HUG"), Some("FILL"),
            Some("STRETCH"),
            Some(50.0), Some(200.0), None, None,
        );
        assert_eq!(sizing.width, SizingMode::Hug);
        assert_eq!(sizing.height, SizingMode::Fill);
        assert_eq!(sizing.align_self, Some(CounterAxisAlign::Stretch));
        assert_eq!(sizing.min_width, Some(50.0));
    }

    #[test]
    fn convert_constraints() {
        let c = convert_constraints(&FigmaLayoutConstraint {
            vertical: "TOP_BOTTOM".into(),
            horizontal: "CENTER".into(),
        });
        assert_eq!(c.vertical, ConstraintAxis::Stretch);
        assert_eq!(c.horizontal, ConstraintAxis::Center);
    }

    #[test]
    fn no_layout_when_mode_none() {
        let config = convert_layout_config(
            Some("NONE"), None, None,
            None, None, None, None, None, None,
            &mut Vec::new(),
        );
        assert!(config.is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p ode-import -- convert_layout::tests
```

- [ ] **Step 3: Implement layout conversion**

Implement in `convert_layout.rs`:
- `pub fn convert_layout_config(...) -> Option<LayoutConfig>` — returns None for NONE/missing, maps all auto-layout props per spec
- `pub fn convert_layout_sizing(...) -> LayoutSizing` — per-child sizing modes
- `pub fn convert_constraints(c: &FigmaLayoutConstraint) -> Constraints`
- `pub fn convert_transform(ft: &[[f64; 3]; 2]) -> Transform`

- [ ] **Step 4: Run tests**

```bash
cargo test -p ode-import -- convert_layout::tests
```
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ode-import/src/figma/convert_layout.rs
git commit -m "feat(ode-import): implement auto layout and constraint conversion"
```

---

## Chunk 5: Variables/Tokens Conversion

### Task 8: Implement variables → design tokens conversion

**Files:**
- Create: `crates/ode-import/src/figma/convert_tokens.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_variable_collection() {
        let vc = FigmaVariableCollection {
            id: "VC:1".into(),
            name: "Colors".into(),
            modes: vec![
                FigmaVariableMode { mode_id: "1:0".into(), name: "Light".into() },
                FigmaVariableMode { mode_id: "1:1".into(), name: "Dark".into() },
            ],
            default_mode_id: "1:0".into(),
            variable_ids: vec!["V:1".into()],
            remote: false,
            hidden_from_publishing: false,
        };
        let mut id_gen = IdGenerator::new();
        let tc = convert_collection(&vc, &mut id_gen);
        assert_eq!(tc.name, "Colors");
        assert_eq!(tc.modes.len(), 2);
        assert_eq!(tc.modes[0].name, "Light");
    }

    #[test]
    fn convert_color_variable() {
        let var = FigmaVariable {
            id: "V:1".into(),
            name: "primary".into(),
            variable_collection_id: "VC:1".into(),
            resolved_type: "COLOR".into(),
            values_by_mode: {
                let mut m = HashMap::new();
                m.insert("1:0".into(), serde_json::json!({"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0}));
                m
            },
            description: "Primary color".into(),
            hidden_from_publishing: false,
            scopes: vec![],
            code_syntax: None,
        };
        let mut id_gen = IdGenerator::new();
        let mode_map = HashMap::from([("1:0".to_string(), 0u32)]);
        let token = convert_variable(&var, &mode_map, &mut id_gen);
        assert_eq!(token.name, "primary");
        let val = token.values.get(&0).unwrap();
        assert!(matches!(val, TokenResolve::Direct(TokenValue::Color(_))));
    }

    #[test]
    fn convert_boolean_variable_to_number() {
        let var = FigmaVariable {
            id: "V:2".into(),
            name: "is_visible".into(),
            variable_collection_id: "VC:1".into(),
            resolved_type: "BOOLEAN".into(),
            values_by_mode: {
                let mut m = HashMap::new();
                m.insert("1:0".into(), serde_json::json!(true));
                m
            },
            description: "".into(),
            hidden_from_publishing: false,
            scopes: vec![],
            code_syntax: None,
        };
        let mode_map = HashMap::from([("1:0".to_string(), 0u32)]);
        let mut id_gen = IdGenerator::new();
        let token = convert_variable(&var, &mode_map, &mut id_gen);
        let val = token.values.get(&0).unwrap();
        assert!(matches!(val, TokenResolve::Direct(TokenValue::Number(v)) if *v == 1.0));
    }

    #[test]
    fn convert_variable_alias() {
        let var = FigmaVariable {
            id: "V:3".into(),
            name: "alias_color".into(),
            variable_collection_id: "VC:1".into(),
            resolved_type: "COLOR".into(),
            values_by_mode: {
                let mut m = HashMap::new();
                m.insert("1:0".into(), serde_json::json!({"type": "VARIABLE_ALIAS", "id": "V:1"}));
                m
            },
            description: "".into(),
            hidden_from_publishing: false,
            scopes: vec![],
            code_syntax: None,
        };
        let mode_map = HashMap::from([("1:0".to_string(), 0u32)]);
        let variable_map = HashMap::from([("V:1".to_string(), (0u32, 0u32))]);
        let mut id_gen = IdGenerator::new();
        let token = convert_variable_with_aliases(&var, &mode_map, &variable_map, &mut id_gen);
        let val = token.values.get(&0).unwrap();
        assert!(matches!(val, TokenResolve::Alias(_)));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p ode-import -- convert_tokens::tests
```

- [ ] **Step 3: Implement token conversion**

Implement in `convert_tokens.rs`:
- `pub struct IdGenerator` — holds counters for collection_id, token_id, mode_id; maps Figma string IDs → u32
- `pub fn convert_collection(vc: &FigmaVariableCollection, gen: &mut IdGenerator) -> TokenCollection`
- `pub fn convert_variable(var: &FigmaVariable, mode_map: &HashMap<String, ModeId>, gen: &mut IdGenerator) -> Token`
- `pub fn convert_variable_with_aliases(...)` — handles VariableAlias in values_by_mode
- `pub fn convert_all_variables(response: &FigmaVariablesMeta) -> (DesignTokens, HashMap<String, (CollectionId, TokenId)>)` — top-level orchestrator

- [ ] **Step 4: Run tests**

```bash
cargo test -p ode-import -- convert_tokens::tests
```
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ode-import/src/figma/convert_tokens.rs
git commit -m "feat(ode-import): implement Figma Variables to DesignTokens conversion"
```

---

## Chunk 6: Main Converter (Node DFS Traversal)

### Task 9: Implement the main FigmaConverter

**Files:**
- Create: `crates/ode-import/src/figma/convert.rs`

- [ ] **Step 1: Write a test fixture JSON and integration test**

Create `crates/ode-import/tests/fixtures/simple_frame.json`:
```json
{
    "name": "Test File",
    "document": {
        "id": "0:0",
        "name": "Document",
        "type": "DOCUMENT",
        "children": [{
            "id": "0:1",
            "name": "Page 1",
            "type": "CANVAS",
            "children": [{
                "id": "1:1",
                "name": "Frame",
                "type": "FRAME",
                "fills": [{"type": "SOLID", "color": {"r": 1, "g": 1, "b": 1, "a": 1}}],
                "strokes": [],
                "effects": [],
                "blendMode": "NORMAL",
                "opacity": 1,
                "absoluteBoundingBox": {"x": 0, "y": 0, "width": 375, "height": 812},
                "size": {"x": 375, "y": 812},
                "relativeTransform": [[1, 0, 0], [0, 1, 0]],
                "clipsContent": true,
                "children": [
                    {
                        "id": "2:1",
                        "name": "Title",
                        "type": "TEXT",
                        "characters": "Hello",
                        "style": {
                            "fontFamily": "Inter",
                            "fontWeight": 400,
                            "fontSize": 24,
                            "textAlignHorizontal": "LEFT",
                            "textAlignVertical": "TOP",
                            "lineHeightPx": 32,
                            "lineHeightUnit": "PIXELS",
                            "letterSpacing": 0
                        },
                        "characterStyleOverrides": [],
                        "styleOverrideTable": {},
                        "fills": [{"type": "SOLID", "color": {"r": 0, "g": 0, "b": 0, "a": 1}}],
                        "strokes": [],
                        "effects": [],
                        "blendMode": "NORMAL",
                        "absoluteBoundingBox": {"x": 16, "y": 16, "width": 100, "height": 32},
                        "size": {"x": 100, "y": 32},
                        "relativeTransform": [[1, 0, 16], [0, 1, 16]]
                    },
                    {
                        "id": "3:1",
                        "name": "Icon",
                        "type": "VECTOR",
                        "fillGeometry": [{"path": "M 0 0 L 24 0 L 24 24 L 0 24 Z", "windingRule": "NONZERO"}],
                        "fills": [{"type": "SOLID", "color": {"r": 0.2, "g": 0.4, "b": 0.8, "a": 1}}],
                        "strokes": [],
                        "effects": [],
                        "blendMode": "NORMAL",
                        "absoluteBoundingBox": {"x": 16, "y": 60, "width": 24, "height": 24},
                        "size": {"x": 24, "y": 24},
                        "relativeTransform": [[1, 0, 16], [0, 1, 60]]
                    }
                ]
            }]
        }]
    },
    "components": {},
    "componentSets": {},
    "schemaVersion": 0,
    "styles": {}
}
```

Create `crates/ode-import/tests/integration_test.rs`:
```rust
use std::fs;
use ode_import::figma::convert::FigmaConverter;
use ode_import::figma::types::FigmaFileResponse;

#[test]
fn convert_simple_frame() {
    let json = fs::read_to_string("tests/fixtures/simple_frame.json").unwrap();
    let file: FigmaFileResponse = serde_json::from_str(&json).unwrap();
    let result = FigmaConverter::convert(file, None, Default::default()).unwrap();

    assert_eq!(result.document.name, "Test File");
    assert_eq!(result.document.canvas.len(), 1);
    // Frame + Text + Vector = 3 nodes
    assert_eq!(result.document.nodes.len(), 3);
    assert!(result.warnings.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p ode-import --test integration_test
```
Expected: FAIL — FigmaConverter not implemented.

- [ ] **Step 3: Implement FigmaConverter**

In `convert.rs`, implement:
```rust
pub struct FigmaConverter {
    component_map: HashMap<String, StableId>,
    node_id_map: HashMap<String, StableId>,
    variable_map: HashMap<String, (CollectionId, TokenId)>,
    image_refs: Vec<String>,
    warnings: Vec<ImportWarning>,
}

pub struct ImportResult {
    pub document: Document,
    pub warnings: Vec<ImportWarning>,
}

impl FigmaConverter {
    pub fn convert(
        file: FigmaFileResponse,
        variables: Option<FigmaVariablesResponse>,
        images: HashMap<String, Vec<u8>>,
    ) -> Result<ImportResult, ImportError> { ... }
}
```

The `convert` method:
1. **Pre-pass**: DFS to assign StableIds to all nodes, identify components
2. **Variables**: If present, call `convert_all_variables()` from convert_tokens
3. **Main DFS**: Walk DOCUMENT → CANVAS → children, calling `convert_node()` for each
4. **`convert_node()`** dispatches by `node_type` string:
   - `"FRAME"` / `"SECTION"` / `"GROUP"` / `"COMPONENT"` / `"COMPONENT_SET"` → Frame/Group
   - `"VECTOR"` / `"RECTANGLE"` / `"ELLIPSE"` / `"LINE"` / `"STAR"` / `"REGULAR_POLYGON"` → Vector (using svg_path parser)
   - `"BOOLEAN_OPERATION"` → BooleanOp
   - `"TEXT"` → Text (using convert_text)
   - `"INSTANCE"` → Instance
   - `"TABLE"` / `"TABLE_CELL"` → Frame
   - Unknown → warning + skip

Each node sets: stable_id, name, transform, opacity, blend_mode, visible, constraints, layout_sizing, kind.

**Image node promotion:** For `"FRAME"` and `"RECTANGLE"` nodes where the only fill is `type: "IMAGE"` and there are no strokes/effects/children, convert to ODE `Image` node instead of Frame/Vector. Otherwise, keep as Frame/Vector with `Paint::ImageFill` in fills.

**GRID layout mode:** If `layoutMode` is `"GRID"`, treat as Frame with no auto-layout + emit warning.

**Variable binding:** After converting each node's fills/strokes/effects, check `boundVariables` on the Figma node. For each bound property, look up the Figma variable ID in `variable_map` to get `(CollectionId, TokenId)`, then wrap the corresponding value in `StyleValue::Bound { token: TokenRef { collection_id, token_id }, resolved: <actual_value> }`.

- [ ] **Step 4: Run integration test**

```bash
cargo test -p ode-import --test integration_test
```
Expected: PASS.

- [ ] **Step 5: Add component/instance fixture test**

Create `crates/ode-import/tests/fixtures/component_instance.json` with a COMPONENT and INSTANCE node. Write test:
```rust
#[test]
fn convert_component_and_instance() {
    let json = fs::read_to_string("tests/fixtures/component_instance.json").unwrap();
    let file: FigmaFileResponse = serde_json::from_str(&json).unwrap();
    let result = FigmaConverter::convert(file, None, Default::default()).unwrap();

    // Find instance node
    let instance = result.document.nodes.iter()
        .find(|(_, n)| matches!(&n.kind, NodeKind::Instance(_)))
        .map(|(_, n)| n);
    assert!(instance.is_some());
}
```

- [ ] **Step 6: Run all tests**

```bash
cargo test -p ode-import
```
Expected: All pass.

- [ ] **Step 7: Commit**

```bash
git add crates/ode-import/src/figma/convert.rs crates/ode-import/tests/
git commit -m "feat(ode-import): implement main FigmaConverter with DFS node traversal"
```

---

## Chunk 7: Figma API Client

### Task 10: Implement Figma HTTP client

**Files:**
- Create: `crates/ode-import/src/figma/client.rs`

- [ ] **Step 1: Implement FigmaClient**

```rust
use reqwest::Client;
use std::collections::HashMap;
use crate::error::ImportError;
use super::types::{FigmaFileResponse, FigmaVariablesResponse};

pub struct FigmaClient {
    token: String,
    client: Client,
    base_url: String,
}

impl FigmaClient {
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: Client::new(),
            base_url: "https://api.figma.com".into(),
        }
    }

    pub async fn get_file(&self, file_key: &str) -> Result<FigmaFileResponse, ImportError> {
        let url = format!("{}/v1/files/{}", self.base_url, file_key);
        let resp = self.client.get(&url)
            .header("X-Figma-Token", &self.token)
            .send().await?
            .error_for_status()
            .map_err(|e| ImportError::Http(e))?;
        Ok(resp.json().await?)
    }

    pub async fn get_variables(&self, file_key: &str) -> Result<FigmaVariablesResponse, ImportError> {
        let url = format!("{}/v1/files/{}/variables/local", self.base_url, file_key);
        let resp = self.client.get(&url)
            .header("X-Figma-Token", &self.token)
            .send().await?
            .error_for_status()
            .map_err(|e| ImportError::Http(e))?;
        Ok(resp.json().await?)
    }

    pub async fn get_images(
        &self,
        file_key: &str,
        image_refs: &[String],
    ) -> Result<HashMap<String, Vec<u8>>, ImportError> {
        if image_refs.is_empty() {
            return Ok(HashMap::new());
        }
        // Use GET /v1/files/:key/images to get imageRef → URL mapping
        // Note: This is NOT /v1/images/:key (which renders nodes to images)
        let url = format!("{}/v1/files/{}/images", self.base_url, file_key);
        let resp: serde_json::Value = self.client.get(&url)
            .header("X-Figma-Token", &self.token)
            .send().await?
            .json().await?;

        let mut result = HashMap::new();
        if let Some(images) = resp.get("images").and_then(|v| v.as_object()) {
            for (ref_id, url_val) in images {
                if let Some(image_url) = url_val.as_str() {
                    match self.client.get(image_url).send().await {
                        Ok(r) => {
                            if let Ok(bytes) = r.bytes().await {
                                result.insert(ref_id.clone(), bytes.to_vec());
                            }
                        }
                        Err(_) => {} // individual image failure is non-fatal
                    }
                }
            }
        }
        Ok(result)
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p ode-import
```
Expected: Compiles. (No tests for HTTP client — requires live API. Integration testing is manual.)

- [ ] **Step 3: Commit**

```bash
git add crates/ode-import/src/figma/client.rs
git commit -m "feat(ode-import): implement Figma REST API HTTP client"
```

---

## Chunk 8: CLI Integration

### Task 11: Add `import figma` CLI command

**Files:**
- Modify: `crates/ode-cli/Cargo.toml`
- Modify: `crates/ode-cli/src/main.rs`
- Modify: `crates/ode-cli/src/commands.rs`

- [ ] **Step 1: Add dependencies to ode-cli**

In `crates/ode-cli/Cargo.toml`, add:
```toml
ode-import = { path = "../ode-import" }
tokio = { workspace = true }
```

- [ ] **Step 2: Add Import command to CLI**

In `crates/ode-cli/src/main.rs`, add the `Import` variant to the `Command` enum:
```rust
/// Import a design file from an external format
Import {
    #[command(subcommand)]
    source: ImportSource,
},
```

Add:
```rust
#[derive(Subcommand)]
enum ImportSource {
    /// Import from Figma REST API
    Figma {
        /// Figma Personal Access Token (or set FIGMA_TOKEN env var)
        #[arg(short, long, env = "FIGMA_TOKEN")]
        token: Option<String>,
        /// Figma file key
        #[arg(short = 'k', long)]
        file_key: Option<String>,
        /// Local JSON file (alternative to API)
        #[arg(short, long)]
        input: Option<String>,
        /// Output .ode.json file path
        #[arg(short, long)]
        output: String,
        /// Include Figma Variables as DesignTokens
        #[arg(long)]
        with_variables: bool,
        /// Skip downloading images
        #[arg(long)]
        skip_images: bool,
    },
}
```

Add the match arm in `main()`:
```rust
Command::Import { source } => match source {
    ImportSource::Figma { token, file_key, input, output, with_variables, skip_images } => {
        commands::cmd_import_figma(token, file_key, input, &output, with_variables, skip_images)
    }
},
```

- [ ] **Step 3: Implement cmd_import_figma**

In `crates/ode-cli/src/commands.rs`, add `cmd_import_figma` that:
1. If `--input` is provided: read JSON file, deserialize as `FigmaFileResponse`
2. If `--token` + `--file-key`: use `FigmaClient` to fetch (requires tokio runtime)
3. Optionally fetch variables if `--with-variables`
4. Optionally fetch images unless `--skip-images`
5. Call `FigmaConverter::convert()`
6. Serialize `Document` to JSON, write to `--output`
7. Print warnings to stderr
8. Return exit code

Use `tokio::runtime::Runtime::new()` for async in the sync CLI.

- [ ] **Step 4: Verify it compiles**

```bash
cargo build -p ode-cli
```
Expected: Compiles.

- [ ] **Step 5: Test with local JSON fixture**

```bash
cargo run -p ode-cli -- import figma --input crates/ode-import/tests/fixtures/simple_frame.json --output /tmp/test_import.ode.json
cat /tmp/test_import.ode.json | head -20
```
Expected: Valid ODE JSON output.

- [ ] **Step 6: Validate the output**

```bash
cargo run -p ode-cli -- validate /tmp/test_import.ode.json
```
Expected: Validation passes.

- [ ] **Step 7: Commit**

```bash
git add crates/ode-cli/ crates/ode-import/
git commit -m "feat(ode-cli): add import figma CLI command"
```

---

## Chunk 9: Final Integration + Round-Trip Test

### Task 12: End-to-end validation

**Files:**
- Create: `crates/ode-import/tests/round_trip.rs`

- [ ] **Step 1: Write round-trip test**

```rust
use std::fs;
use ode_import::figma::convert::FigmaConverter;
use ode_import::figma::types::FigmaFileResponse;
use ode_format::Document;

#[test]
fn round_trip_simple_frame() {
    // Figma JSON → Document
    let json = fs::read_to_string("tests/fixtures/simple_frame.json").unwrap();
    let file: FigmaFileResponse = serde_json::from_str(&json).unwrap();
    let result = FigmaConverter::convert(file, None, Default::default()).unwrap();

    // Document → JSON string
    let ode_json = serde_json::to_string_pretty(&result.document).unwrap();

    // JSON string → Document (round-trip)
    let doc2: Document = serde_json::from_str(&ode_json).unwrap();

    // Verify key properties survive round-trip
    assert_eq!(result.document.name, doc2.name);
    assert_eq!(result.document.canvas.len(), doc2.canvas.len());
    assert_eq!(result.document.nodes.len(), doc2.nodes.len());
}
```

- [ ] **Step 2: Run round-trip test**

```bash
cargo test -p ode-import --test round_trip
```
Expected: PASS.

- [ ] **Step 3: Write Image node promotion test**

Add to integration_test.rs:
```rust
#[test]
fn single_image_fill_frame_becomes_image_node() {
    let json = r#"{
        "name": "Image Test",
        "document": {"id": "0:0", "name": "Doc", "type": "DOCUMENT", "children": [
            {"id": "0:1", "name": "Page", "type": "CANVAS", "children": [
                {"id": "1:1", "name": "Photo", "type": "RECTANGLE",
                 "fills": [{"type": "IMAGE", "imageRef": "img123", "scaleMode": "FILL"}],
                 "strokes": [], "effects": [], "blendMode": "NORMAL",
                 "size": {"x": 200, "y": 200},
                 "relativeTransform": [[1,0,0],[0,1,0]]}
            ]}
        ]},
        "components": {}, "componentSets": {}, "schemaVersion": 0, "styles": {}
    }"#;
    let file: FigmaFileResponse = serde_json::from_str(json).unwrap();
    let result = FigmaConverter::convert(file, None, Default::default()).unwrap();
    let has_image = result.document.nodes.iter()
        .any(|(_, n)| matches!(&n.kind, NodeKind::Image(_)));
    assert!(has_image);
}
```

- [ ] **Step 4: Write warning test**

Create `crates/ode-import/tests/fixtures/unsupported_nodes.json` with STICKY and VIDEO paint. Test:
```rust
#[test]
fn unsupported_nodes_produce_warnings() {
    let json = fs::read_to_string("tests/fixtures/unsupported_nodes.json").unwrap();
    let file: FigmaFileResponse = serde_json::from_str(&json).unwrap();
    let result = FigmaConverter::convert(file, None, Default::default()).unwrap();
    assert!(!result.warnings.is_empty());
}
```

- [ ] **Step 4: Run all workspace tests**

```bash
cargo test --workspace
```
Expected: All tests pass across all crates.

- [ ] **Step 5: Commit**

```bash
git add crates/ode-import/tests/
git commit -m "test(ode-import): add round-trip and warning integration tests"
```

### Task 13: Final cleanup and full build verification

- [ ] **Step 1: Run clippy**

```bash
cargo clippy --workspace -- -D warnings
```
Fix any warnings.

- [ ] **Step 2: Check formatting**

```bash
cargo fmt --all -- --check
```
Fix any formatting issues.

- [ ] **Step 3: Full release build**

```bash
cargo build --release --workspace
```
Expected: Clean build.

- [ ] **Step 4: Final commit if any fixes**

```bash
git add -A
git commit -m "chore: fix clippy warnings and formatting"
```
