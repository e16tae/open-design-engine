# Rendering Pipeline Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Document → Scene IR → Pixmap → PNG rendering pipeline for ODE.

**Architecture:** 3-stage pipeline. `ode-core::convert` transforms a Document into a flat Scene IR (list of render commands). `ode-core::render` executes those commands via tiny-skia to produce a Pixmap. `ode-export::png` encodes the Pixmap to PNG. Layer compositing uses a manual stack of temporary Pixmaps since tiny-skia 0.11 has no save/restore.

**Tech Stack:** Rust 2024 edition, tiny-skia 0.11 (CPU rasterization), kurbo 0.11 (path math), i_overlay (boolean path ops), thiserror (errors)

**Spec:** `docs/superpowers/specs/2026-03-12-rendering-pipeline-design.md`

---

## File Map

```
MODIFY  Cargo.toml                              — add i_overlay to workspace deps
MODIFY  crates/ode-core/Cargo.toml              — add i_overlay dep
MODIFY  crates/ode-format/src/node.rs           — VectorPath, FillRule, FrameData fields, constructor changes
MODIFY  crates/ode-format/src/lib.rs            — re-export new types
MODIFY  crates/ode-format/tests/integration.rs  — update constructor calls
CREATE  crates/ode-core/src/error.rs            — RenderError, ConvertError
CREATE  crates/ode-core/src/scene.rs            — Scene, RenderCommand, ResolvedPaint, ResolvedEffect, StrokeStyle
CREATE  crates/ode-core/src/path.rs             — VectorPath ↔ BezPath, rounded rect, BezPath ↔ tiny_skia::Path, boolean ops
CREATE  crates/ode-core/src/blend.rs            — BlendMode → tiny_skia::BlendMode
CREATE  crates/ode-core/src/paint.rs            — ResolvedPaint → tiny_skia rendering, custom gradients
CREATE  crates/ode-core/src/effects.rs          — Gaussian blur, shadow/blur rendering
CREATE  crates/ode-core/src/convert.rs          — Scene::from_document, node traversal, token resolution
CREATE  crates/ode-core/src/render.rs           — Renderer::render, layer stack, command dispatch
MODIFY  crates/ode-core/src/lib.rs              — module declarations, re-exports
CREATE  crates/ode-export/src/error.rs          — ExportError
CREATE  crates/ode-export/src/png.rs            — PngExporter
MODIFY  crates/ode-export/src/lib.rs            — module declarations, re-exports
CREATE  crates/ode-export/tests/integration.rs  — end-to-end Document → PNG test
```

---

## Chunk 1: Foundation

### Task 1: Workspace Dependencies and Error Types

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/ode-format/Cargo.toml`
- Modify: `crates/ode-core/Cargo.toml`
- Create: `crates/ode-core/src/error.rs`
- Create: `crates/ode-export/src/error.rs`
- Modify: `crates/ode-core/src/lib.rs`
- Modify: `crates/ode-export/src/lib.rs`

- [ ] **Step 1: Add i_overlay to workspace dependencies**

In `Cargo.toml` (workspace root), add under `[workspace.dependencies]`:

```toml
# Boolean path operations
i_overlay = "1"
```

- [ ] **Step 2: Verify ode-format already has thiserror**

`crates/ode-format/Cargo.toml` already has `thiserror` as a dependency and `TokenError` in `tokens.rs` already derives `thiserror::Error`. Verify this by reading `tokens.rs` — if `#[derive(Debug, Error)]` and `#[error("...")]` attributes are present on `TokenError`, no changes needed. The current `TokenError` has unit variants (`NotFound`, `CyclicAlias`, `MissingValue`, `CollectionNotFound`) — do not change the variant signatures.

- [ ] **Step 3: Add i_overlay to ode-core**

In `crates/ode-core/Cargo.toml`, add under `[dependencies]`:

```toml
i_overlay = { workspace = true }
```

- [ ] **Step 4: Create ode-core error types**

Create `crates/ode-core/src/error.rs`:

```rust
use thiserror::Error;
use ode_format::node::NodeId;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("empty scene — no canvas roots")]
    EmptyScene,
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("pixmap creation failed: {width}x{height}")]
    PixmapCreationFailed { width: u32, height: u32 },
    #[error("boolean operation failed: {0}")]
    BooleanOpFailed(String),
}

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("root node not found: {0:?}")]
    RootNodeNotFound(NodeId),
    #[error("token resolution failed: {0}")]
    TokenError(#[from] ode_format::tokens::TokenError),
    #[error("document has no canvas roots")]
    NoCanvasRoots,
}
```

- [ ] **Step 5: Create ode-export error types**

Create `crates/ode-export/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("PNG encoding failed: {0}")]
    PngEncodeFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

- [ ] **Step 6: Wire up lib.rs modules**

`crates/ode-core/src/lib.rs`:

```rust
pub mod error;
```

`crates/ode-export/src/lib.rs`:

```rust
pub mod error;
```

- [ ] **Step 7: Verify compilation**

Run: `cargo check --workspace`
Expected: Clean compilation, no errors.

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "feat: add workspace deps (i_overlay) and error types for ode-core/ode-export"
```

---

### Task 2: Data Model Changes (ode-format)

**Files:**
- Modify: `crates/ode-format/src/node.rs`
- Modify: `crates/ode-format/src/lib.rs`
- Modify: `crates/ode-format/tests/integration.rs`

- [ ] **Step 1: Write tests for new types**

Add to the `#[cfg(test)] mod tests` in `crates/ode-format/src/node.rs`:

```rust
#[test]
fn vectorpath_serde_roundtrip() {
    let path = VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 100.0, y: 0.0 },
            PathSegment::CurveTo { x1: 100.0, y1: 50.0, x2: 50.0, y2: 100.0, x: 0.0, y: 100.0 },
            PathSegment::Close,
        ],
        closed: true,
    };
    let json = serde_json::to_string(&path).unwrap();
    let parsed: VectorPath = serde_json::from_str(&json).unwrap();
    assert_eq!(path, parsed);
}

#[test]
fn fillrule_default_is_nonzero() {
    assert_eq!(FillRule::default(), FillRule::NonZero);
}

#[test]
fn frame_data_has_size_and_corner_radius() {
    let node = Node::new_frame("Card", 200.0, 100.0);
    if let NodeKind::Frame(ref data) = node.kind {
        assert!((data.width - 200.0).abs() < f32::EPSILON);
        assert!((data.height - 100.0).abs() < f32::EPSILON);
        assert_eq!(data.corner_radius, [0.0; 4]);
    } else {
        panic!("Expected Frame node");
    }
}

#[test]
fn vector_data_has_path_and_fill_rule() {
    let path = VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 50.0, y: 50.0 },
        ],
        closed: false,
    };
    let node = Node::new_vector("Line", path.clone());
    if let NodeKind::Vector(ref data) = node.kind {
        assert_eq!(data.path, path);
        assert_eq!(data.fill_rule, FillRule::NonZero);
    } else {
        panic!("Expected Vector node");
    }
}

#[test]
fn frame_data_backward_compat_no_size() {
    // A FrameData without width/height should deserialize with defaults
    let json = r#"{"type":"frame","visual":{},"container":{},"component_def":null}"#;
    let kind: NodeKind = serde_json::from_str(json).unwrap();
    if let NodeKind::Frame(data) = kind {
        assert!((data.width - 0.0).abs() < f32::EPSILON);
        assert!((data.height - 0.0).abs() < f32::EPSILON);
        assert_eq!(data.corner_radius, [0.0; 4]);
    } else {
        panic!("Expected Frame");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-format`
Expected: FAIL — `VectorPath`, `PathSegment`, `FillRule` not found, `new_frame` wrong number of args.

- [ ] **Step 3: Add VectorPath, PathSegment, FillRule types**

In `crates/ode-format/src/node.rs`, add after the `BooleanOperation` enum (before `NodeKind`):

```rust
// ─── VectorPath ───

/// Serializable path representation.
/// Conversion to/from kurbo::BezPath lives in ode-core::path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorPath {
    pub segments: Vec<PathSegment>,
    pub closed: bool,
}

impl Default for VectorPath {
    fn default() -> Self {
        Self { segments: vec![], closed: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PathSegment {
    MoveTo { x: f32, y: f32 },
    LineTo { x: f32, y: f32 },
    QuadTo { x1: f32, y1: f32, x: f32, y: f32 },
    CurveTo { x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32 },
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FillRule { NonZero, EvenOdd }

impl Default for FillRule {
    fn default() -> Self { Self::NonZero }
}
```

- [ ] **Step 4: Update FrameData with size and corner_radius**

Replace the existing `FrameData` struct:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameData {
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub corner_radius: [f32; 4],
    #[serde(default)]
    pub visual: VisualProps,
    #[serde(default)]
    pub container: ContainerProps,
    pub component_def: Option<ComponentDef>,
}
```

- [ ] **Step 5: Update VectorData with path and fill_rule**

Replace the existing `VectorData` struct:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorData {
    #[serde(default)]
    pub visual: VisualProps,
    #[serde(default)]
    pub path: VectorPath,
    #[serde(default)]
    pub fill_rule: FillRule,
}
```

- [ ] **Step 6: Update constructors**

`Node::new_frame`:

```rust
pub fn new_frame(name: &str, width: f32, height: f32) -> Self {
    Self {
        id: NodeId::default(),
        stable_id: nanoid::nanoid!(),
        name: name.to_string(),
        transform: Transform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        constraints: None,
        kind: NodeKind::Frame(Box::new(FrameData {
            width,
            height,
            corner_radius: [0.0; 4],
            visual: VisualProps::default(),
            container: ContainerProps::default(),
            component_def: None,
        })),
    }
}
```

`Node::new_vector`:

```rust
pub fn new_vector(name: &str, path: VectorPath) -> Self {
    Self {
        id: NodeId::default(),
        stable_id: nanoid::nanoid!(),
        name: name.to_string(),
        transform: Transform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        constraints: None,
        kind: NodeKind::Vector(Box::new(VectorData {
            visual: VisualProps::default(),
            path,
            fill_rule: FillRule::default(),
        })),
    }
}
```

- [ ] **Step 7: Fix all existing callers**

In `node.rs` tests, update every call:
- `Node::new_frame("Header")` → `Node::new_frame("Header", 100.0, 100.0)`
- `Node::new_frame("Card")` → `Node::new_frame("Card", 200.0, 150.0)`
- `Node::new_frame("Parent")` → `Node::new_frame("Parent", 100.0, 100.0)`
- `Node::new_frame("A")` / `Node::new_frame("B")` → add `, 100.0, 100.0`
- `Node::new_frame("Colored")` → `Node::new_frame("Colored", 100.0, 100.0)`
- `Node::new_vector("Path")` → `Node::new_vector("Path", VectorPath::default())`

In `crates/ode-format/tests/integration.rs`, update ALL calls:
- Line 13: `Node::new_frame("Card")` → `Node::new_frame("Card", 300.0, 200.0)`
- Line 105: `Node::new_frame("Card")` → `Node::new_frame("Card", 300.0, 200.0)`

In `crates/ode-format/src/document.rs` tests, update ALL calls. Search for `new_frame(` — there are calls around lines 95 and 105 that need updating.

**Important:** Run `grep -rn 'new_frame\|new_vector' crates/ode-format/src/ crates/ode-format/tests/` to find ALL callers. Every one must be updated.

- [ ] **Step 8: Update lib.rs re-exports**

In `crates/ode-format/src/lib.rs`, update the `node` re-export line:

```rust
pub use node::{Node, NodeId, NodeKind, NodeTree, StableId, VectorPath, PathSegment, FillRule};
```

- [ ] **Step 9: Run tests**

Run: `cargo test -p ode-format`
Expected: All tests pass (existing + new).

- [ ] **Step 10: Commit**

```bash
git add -A && git commit -m "feat(ode-format): add VectorPath, FillRule, FrameData size/corner_radius"
```

---

### Task 3: Scene IR Types

**Files:**
- Create: `crates/ode-core/src/scene.rs`
- Modify: `crates/ode-core/src/lib.rs`

- [ ] **Step 1: Write tests for Scene IR construction**

Create `crates/ode-core/src/scene.rs` with tests at the bottom:

```rust
use ode_format::color::Color;
use ode_format::node::FillRule;
use ode_format::style::BlendMode;

/// Flat list of render commands produced by converting a Document.
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub commands: Vec<RenderCommand>,
}

pub enum RenderCommand {
    /// Begin a new compositing layer.
    /// `transform` is used ONLY for transforming the clip path when building the Mask.
    PushLayer {
        opacity: f32,
        blend_mode: BlendMode,
        clip: Option<kurbo::BezPath>,
        transform: tiny_skia::Transform,
    },
    /// End current layer — composite temp Pixmap into parent.
    PopLayer,
    /// Fill a path.
    FillPath {
        path: kurbo::BezPath,
        paint: ResolvedPaint,
        fill_rule: FillRule,
        transform: tiny_skia::Transform,
    },
    /// Stroke a path.
    StrokePath {
        path: kurbo::BezPath,
        paint: ResolvedPaint,
        stroke: StrokeStyle,
        transform: tiny_skia::Transform,
    },
    /// Apply an effect to the current layer.
    ApplyEffect {
        effect: ResolvedEffect,
    },
}

/// Token-resolved paint. The renderer never sees StyleValue or TokenRef.
pub enum ResolvedPaint {
    Solid(Color),
    LinearGradient {
        stops: Vec<ResolvedGradientStop>,
        start: kurbo::Point,
        end: kurbo::Point,
    },
    RadialGradient {
        stops: Vec<ResolvedGradientStop>,
        center: kurbo::Point,
        radius: kurbo::Point,
    },
    AngularGradient {
        stops: Vec<ResolvedGradientStop>,
        center: kurbo::Point,
        angle: f32,
    },
    DiamondGradient {
        stops: Vec<ResolvedGradientStop>,
        center: kurbo::Point,
        radius: kurbo::Point,
    },
}

pub struct ResolvedGradientStop {
    pub position: f32,
    pub color: Color,
}

pub enum ResolvedEffect {
    DropShadow { color: Color, offset_x: f32, offset_y: f32, blur_radius: f32, spread: f32 },
    InnerShadow { color: Color, offset_x: f32, offset_y: f32, blur_radius: f32, spread: f32 },
    LayerBlur { radius: f32 },
    BackgroundBlur { radius: f32 },
}

pub struct StrokeStyle {
    pub width: f32,
    pub position: ode_format::style::StrokePosition,
    pub cap: ode_format::style::StrokeCap,
    pub join: ode_format::style::StrokeJoin,
    pub miter_limit: f32,
    pub dash: Option<ode_format::style::DashPattern>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_can_hold_commands() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::FillPath {
                    path: kurbo::BezPath::new(),
                    paint: ResolvedPaint::Solid(Color::black()),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        assert_eq!(scene.commands.len(), 3);
    }

    #[test]
    fn resolved_paint_variants() {
        let _solid = ResolvedPaint::Solid(Color::white());
        let _linear = ResolvedPaint::LinearGradient {
            stops: vec![
                ResolvedGradientStop { position: 0.0, color: Color::black() },
                ResolvedGradientStop { position: 1.0, color: Color::white() },
            ],
            start: kurbo::Point::new(0.0, 0.0),
            end: kurbo::Point::new(100.0, 0.0),
        };
        let _angular = ResolvedPaint::AngularGradient {
            stops: vec![],
            center: kurbo::Point::new(50.0, 50.0),
            angle: 0.0,
        };
    }
}
```

- [ ] **Step 2: Add derives**

Add `#[derive(Debug, Clone)]` to `Scene`, `RenderCommand`, `ResolvedPaint`, `ResolvedGradientStop`, `ResolvedEffect`, `StrokeStyle`. These types don't need Serialize/Deserialize — they are internal IR.

- [ ] **Step 3: Update lib.rs**

`crates/ode-core/src/lib.rs`:

```rust
pub mod error;
pub mod scene;

pub use scene::{Scene, RenderCommand, ResolvedPaint, ResolvedEffect};
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p ode-core`
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(ode-core): add Scene IR types (RenderCommand, ResolvedPaint, ResolvedEffect)"
```

---

### Task 4: Path Utilities

**Files:**
- Create: `crates/ode-core/src/path.rs`
- Modify: `crates/ode-core/src/lib.rs`

- [ ] **Step 1: Write tests**

```rust
// crates/ode-core/src/path.rs

use kurbo::{BezPath, Shape};
use ode_format::node::{VectorPath, PathSegment};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectorpath_to_bezpath_line() {
        let vp = VectorPath {
            segments: vec![
                PathSegment::MoveTo { x: 0.0, y: 0.0 },
                PathSegment::LineTo { x: 100.0, y: 0.0 },
                PathSegment::LineTo { x: 100.0, y: 100.0 },
                PathSegment::Close,
            ],
            closed: true,
        };
        let bp = to_bezpath(&vp);
        // Should have 4 elements: MoveTo, LineTo, LineTo, ClosePath
        assert_eq!(bp.elements().len(), 4);
    }

    #[test]
    fn bezpath_roundtrip() {
        let vp = VectorPath {
            segments: vec![
                PathSegment::MoveTo { x: 10.0, y: 20.0 },
                PathSegment::CurveTo { x1: 30.0, y1: 40.0, x2: 50.0, y2: 60.0, x: 70.0, y: 80.0 },
                PathSegment::Close,
            ],
            closed: true,
        };
        let bp = to_bezpath(&vp);
        let vp2 = from_bezpath(&bp);
        assert_eq!(vp.closed, vp2.closed);
        assert_eq!(vp.segments.len(), vp2.segments.len());
    }

    #[test]
    fn rounded_rect_sharp_corners() {
        let bp = rounded_rect_path(100.0, 50.0, [0.0; 4]);
        // Sharp rect should have MoveTo, 3x LineTo, Close = 5 elements
        let elems: Vec<_> = bp.elements().collect();
        assert_eq!(elems.len(), 5);
    }

    #[test]
    fn rounded_rect_with_radii() {
        let bp = rounded_rect_path(100.0, 50.0, [10.0, 10.0, 10.0, 10.0]);
        // Should have curves at corners
        let has_curve = bp.elements().any(|el| matches!(el, kurbo::PathEl::CurveTo(..)));
        assert!(has_curve, "Rounded rect should have curves");
    }

    #[test]
    fn bezpath_to_skia_simple() {
        let mut bp = BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((100.0, 0.0));
        bp.line_to((100.0, 100.0));
        bp.close_path();
        let skia_path = bezpath_to_skia(&bp);
        assert!(skia_path.is_some(), "Should produce a valid tiny-skia path");
    }
}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p ode-core`
Expected: FAIL — functions not defined.

- [ ] **Step 3: Implement path utilities**

```rust
use kurbo::{BezPath, PathEl, RoundedRect, RoundedRectRadii, Rect, Shape};
use ode_format::node::{VectorPath, PathSegment};

/// Convert serializable VectorPath to kurbo BezPath for rendering.
pub fn to_bezpath(path: &VectorPath) -> BezPath {
    let mut bp = BezPath::new();
    for seg in &path.segments {
        match *seg {
            PathSegment::MoveTo { x, y } => bp.move_to((x as f64, y as f64)),
            PathSegment::LineTo { x, y } => bp.line_to((x as f64, y as f64)),
            PathSegment::QuadTo { x1, y1, x, y } =>
                bp.quad_to((x1 as f64, y1 as f64), (x as f64, y as f64)),
            PathSegment::CurveTo { x1, y1, x2, y2, x, y } =>
                bp.curve_to((x1 as f64, y1 as f64), (x2 as f64, y2 as f64), (x as f64, y as f64)),
            PathSegment::Close => bp.close_path(),
        }
    }
    // If path is marked closed but doesn't end with Close segment, close it
    if path.closed && !path.segments.last().is_some_and(|s| matches!(s, PathSegment::Close)) {
        bp.close_path();
    }
    bp
}

/// Convert kurbo BezPath back to serializable VectorPath.
pub fn from_bezpath(bp: &BezPath) -> VectorPath {
    let mut segments = Vec::new();
    let mut closed = false;
    for el in bp.elements() {
        match *el {
            PathEl::MoveTo(p) => segments.push(PathSegment::MoveTo { x: p.x as f32, y: p.y as f32 }),
            PathEl::LineTo(p) => segments.push(PathSegment::LineTo { x: p.x as f32, y: p.y as f32 }),
            PathEl::QuadTo(p1, p2) => segments.push(PathSegment::QuadTo {
                x1: p1.x as f32, y1: p1.y as f32, x: p2.x as f32, y: p2.y as f32,
            }),
            PathEl::CurveTo(p1, p2, p3) => segments.push(PathSegment::CurveTo {
                x1: p1.x as f32, y1: p1.y as f32,
                x2: p2.x as f32, y2: p2.y as f32,
                x: p3.x as f32, y: p3.y as f32,
            }),
            PathEl::ClosePath => {
                segments.push(PathSegment::Close);
                closed = true;
            }
        }
    }
    VectorPath { segments, closed }
}

/// Generate a rounded rectangle path.
pub fn rounded_rect_path(width: f32, height: f32, radii: [f32; 4]) -> BezPath {
    let rect = Rect::new(0.0, 0.0, width as f64, height as f64);
    let rr = RoundedRect::from_rect(
        rect,
        RoundedRectRadii::new(
            radii[0] as f64, radii[1] as f64,
            radii[2] as f64, radii[3] as f64,
        ),
    );
    rr.to_path(0.1)
}

/// Convert kurbo BezPath to tiny_skia Path.
pub fn bezpath_to_skia(bp: &BezPath) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for el in bp.elements() {
        match *el {
            PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(p1, p2) => pb.quad_to(
                p1.x as f32, p1.y as f32,
                p2.x as f32, p2.y as f32,
            ),
            PathEl::CurveTo(p1, p2, p3) => pb.cubic_to(
                p1.x as f32, p1.y as f32,
                p2.x as f32, p2.y as f32,
                p3.x as f32, p3.y as f32,
            ),
            PathEl::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

/// Convert ode-format Transform to tiny_skia Transform.
pub fn transform_to_skia(t: &ode_format::node::Transform) -> tiny_skia::Transform {
    tiny_skia::Transform::from_row(t.a, t.b, t.c, t.d, t.tx, t.ty)
}
```

- [ ] **Step 4: Add module to lib.rs**

In `crates/ode-core/src/lib.rs`, add:

```rust
pub mod path;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p ode-core`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(ode-core): add path utilities (VectorPath ↔ BezPath, rounded rect, transform)"
```

---

## Chunk 2: Rendering Utilities

### Task 5: BlendMode Mapping

**Files:**
- Create: `crates/ode-core/src/blend.rs`
- Modify: `crates/ode-core/src/lib.rs`

- [ ] **Step 1: Write tests and implementation**

Create `crates/ode-core/src/blend.rs`:

```rust
use ode_format::style::BlendMode;

/// Map ODE BlendMode to tiny-skia BlendMode.
pub fn to_skia_blend(mode: BlendMode) -> tiny_skia::BlendMode {
    match mode {
        BlendMode::Normal => tiny_skia::BlendMode::SourceOver,
        BlendMode::Multiply => tiny_skia::BlendMode::Multiply,
        BlendMode::Screen => tiny_skia::BlendMode::Screen,
        BlendMode::Overlay => tiny_skia::BlendMode::Overlay,
        BlendMode::Darken => tiny_skia::BlendMode::Darken,
        BlendMode::Lighten => tiny_skia::BlendMode::Lighten,
        BlendMode::ColorDodge => tiny_skia::BlendMode::ColorDodge,
        BlendMode::ColorBurn => tiny_skia::BlendMode::ColorBurn,
        BlendMode::HardLight => tiny_skia::BlendMode::HardLight,
        BlendMode::SoftLight => tiny_skia::BlendMode::SoftLight,
        BlendMode::Difference => tiny_skia::BlendMode::Difference,
        BlendMode::Exclusion => tiny_skia::BlendMode::Exclusion,
        BlendMode::Hue => tiny_skia::BlendMode::Hue,
        BlendMode::Saturation => tiny_skia::BlendMode::Saturation,
        BlendMode::Color => tiny_skia::BlendMode::Color,
        BlendMode::Luminosity => tiny_skia::BlendMode::Luminosity,
    }
}

/// Map ODE FillRule to tiny-skia FillRule.
pub fn to_skia_fill_rule(rule: ode_format::node::FillRule) -> tiny_skia::FillRule {
    match rule {
        ode_format::node::FillRule::NonZero => tiny_skia::FillRule::Winding,
        ode_format::node::FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_blend_modes_map() {
        let modes = [
            BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen,
            BlendMode::Overlay, BlendMode::Darken, BlendMode::Lighten,
            BlendMode::ColorDodge, BlendMode::ColorBurn, BlendMode::HardLight,
            BlendMode::SoftLight, BlendMode::Difference, BlendMode::Exclusion,
            BlendMode::Hue, BlendMode::Saturation, BlendMode::Color,
            BlendMode::Luminosity,
        ];
        for mode in modes {
            let _ = to_skia_blend(mode); // Should not panic
        }
    }

    #[test]
    fn normal_maps_to_source_over() {
        assert!(matches!(to_skia_blend(BlendMode::Normal), tiny_skia::BlendMode::SourceOver));
    }

    #[test]
    fn fill_rule_mapping() {
        assert!(matches!(
            to_skia_fill_rule(ode_format::node::FillRule::NonZero),
            tiny_skia::FillRule::Winding
        ));
        assert!(matches!(
            to_skia_fill_rule(ode_format::node::FillRule::EvenOdd),
            tiny_skia::FillRule::EvenOdd
        ));
    }
}
```

- [ ] **Step 2: Add to lib.rs**

```rust
pub mod blend;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p ode-core`
Expected: All pass.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(ode-core): add BlendMode and FillRule mappings to tiny-skia"
```

---

### Task 6: Paint Conversion

**Files:**
- Create: `crates/ode-core/src/paint.rs`
- Modify: `crates/ode-core/src/lib.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::color::Color;

    #[test]
    fn color_to_skia_black() {
        let c = color_to_skia(&Color::black());
        // tiny_skia::Color fields are f32 (0.0 to 1.0)
        assert!((c.red() - 0.0).abs() < f32::EPSILON);
        assert!((c.green() - 0.0).abs() < f32::EPSILON);
        assert!((c.blue() - 0.0).abs() < f32::EPSILON);
        assert!((c.alpha() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn color_to_skia_white() {
        let c = color_to_skia(&Color::white());
        assert!((c.red() - 1.0).abs() < f32::EPSILON);
        assert!((c.green() - 1.0).abs() < f32::EPSILON);
        assert!((c.blue() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn solid_paint_fills_pixel() {
        let mut pixmap = tiny_skia::Pixmap::new(10, 10).unwrap();
        let paint = ResolvedPaint::Solid(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 });
        // Create a rectangle path covering the pixmap
        let mut bp = kurbo::BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((10.0, 0.0));
        bp.line_to((10.0, 10.0));
        bp.line_to((0.0, 10.0));
        bp.close_path();
        let skia_path = crate::path::bezpath_to_skia(&bp).unwrap();
        fill_with_paint(
            &mut pixmap, &skia_path, &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(), None,
        );
        // pixmap.pixel() returns PremultipliedColorU8 — red/green/blue/alpha are u8
        let pixel = pixmap.pixel(5, 5).unwrap();
        assert_eq!(pixel.red(), 255);
        assert_eq!(pixel.green(), 0);
        assert_eq!(pixel.blue(), 0);
    }

    #[test]
    fn linear_gradient_paint_fills() {
        let mut pixmap = tiny_skia::Pixmap::new(100, 10).unwrap();
        let paint = ResolvedPaint::LinearGradient {
            stops: vec![
                ResolvedGradientStop { position: 0.0, color: Color::black() },
                ResolvedGradientStop { position: 1.0, color: Color::white() },
            ],
            start: kurbo::Point::new(0.0, 0.0),
            end: kurbo::Point::new(100.0, 0.0),
        };
        let mut bp = kurbo::BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((100.0, 0.0));
        bp.line_to((100.0, 10.0));
        bp.line_to((0.0, 10.0));
        bp.close_path();
        let skia_path = crate::path::bezpath_to_skia(&bp).unwrap();
        fill_with_paint(
            &mut pixmap, &skia_path, &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(), None,
        );
        // Left side should be dark, right side should be light
        let left = pixmap.pixel(5, 5).unwrap();
        let right = pixmap.pixel(95, 5).unwrap();
        assert!(left.red() < 50, "Left should be dark, got {}", left.red());
        assert!(right.red() > 200, "Right should be light, got {}", right.red());
    }
}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p ode-core`
Expected: FAIL.

- [ ] **Step 3: Implement paint conversion**

```rust
use ode_format::color::Color;
use crate::scene::{ResolvedPaint, ResolvedGradientStop};

/// Convert ODE Color to tiny-skia Color.
pub fn color_to_skia(color: &Color) -> tiny_skia::Color {
    let [r, g, b, a] = color.to_rgba_u8();
    tiny_skia::Color::from_rgba8(r, g, b, a)
}

/// Fill a path with a ResolvedPaint. Handles all paint types including custom gradients.
pub fn fill_with_paint(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    paint: &ResolvedPaint,
    fill_rule: tiny_skia::FillRule,
    transform: tiny_skia::Transform,
    mask: Option<&tiny_skia::Mask>,
) {
    match paint {
        ResolvedPaint::Solid(color) => {
            let mut p = tiny_skia::Paint::default();
            p.shader = tiny_skia::Shader::SolidColor(color_to_skia(color));
            p.anti_alias = true;
            pixmap.fill_path(path, &p, fill_rule, transform, mask);
        }
        ResolvedPaint::LinearGradient { stops, start, end } => {
            if let Some(shader) = make_linear_gradient(stops, *start, *end) {
                let mut p = tiny_skia::Paint::default();
                p.shader = shader;
                p.anti_alias = true;
                pixmap.fill_path(path, &p, fill_rule, transform, mask);
            }
        }
        ResolvedPaint::RadialGradient { stops, center, radius } => {
            if let Some(shader) = make_radial_gradient(stops, *center, *radius) {
                let mut p = tiny_skia::Paint::default();
                p.shader = shader;
                p.anti_alias = true;
                pixmap.fill_path(path, &p, fill_rule, transform, mask);
            }
        }
        ResolvedPaint::AngularGradient { stops, center, angle } => {
            fill_angular_gradient(pixmap, path, stops, *center, *angle, fill_rule, transform, mask);
        }
        ResolvedPaint::DiamondGradient { stops, center, radius } => {
            fill_diamond_gradient(pixmap, path, stops, *center, *radius, fill_rule, transform, mask);
        }
    }
}

/// Stroke a path with a ResolvedPaint.
pub fn stroke_with_paint(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    paint: &ResolvedPaint,
    stroke: &tiny_skia::Stroke,
    transform: tiny_skia::Transform,
    mask: Option<&tiny_skia::Mask>,
) {
    // For strokes, custom gradients are rare. Use solid/linear/radial directly.
    // For angular/diamond, create a temp pixmap with the gradient and composite.
    let skia_paint = match paint {
        ResolvedPaint::Solid(color) => {
            let mut p = tiny_skia::Paint::default();
            p.shader = tiny_skia::Shader::SolidColor(color_to_skia(color));
            p.anti_alias = true;
            p
        }
        ResolvedPaint::LinearGradient { stops, start, end } => {
            let mut p = tiny_skia::Paint::default();
            if let Some(shader) = make_linear_gradient(stops, *start, *end) {
                p.shader = shader;
            }
            p.anti_alias = true;
            p
        }
        ResolvedPaint::RadialGradient { stops, center, radius } => {
            let mut p = tiny_skia::Paint::default();
            if let Some(shader) = make_radial_gradient(stops, *center, *radius) {
                p.shader = shader;
            }
            p.anti_alias = true;
            p
        }
        _ => {
            // For angular/diamond gradient strokes, fall back to first stop color
            let color = match paint {
                ResolvedPaint::AngularGradient { stops, .. }
                | ResolvedPaint::DiamondGradient { stops, .. } => {
                    stops.first().map(|s| color_to_skia(&s.color))
                        .unwrap_or(tiny_skia::Color::BLACK)
                }
                _ => tiny_skia::Color::BLACK,
            };
            let mut p = tiny_skia::Paint::default();
            p.shader = tiny_skia::Shader::SolidColor(color);
            p.anti_alias = true;
            p
        }
    };
    pixmap.stroke_path(path, &skia_paint, stroke, transform, mask);
}

fn gradient_stops_to_skia(stops: &[ResolvedGradientStop]) -> Vec<tiny_skia::GradientStop> {
    stops.iter().map(|s| {
        tiny_skia::GradientStop::new(s.position, color_to_skia(&s.color))
    }).collect()
}

fn make_linear_gradient(
    stops: &[ResolvedGradientStop],
    start: kurbo::Point,
    end: kurbo::Point,
) -> Option<tiny_skia::Shader<'static>> {
    tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(start.x as f32, start.y as f32),
        tiny_skia::Point::from_xy(end.x as f32, end.y as f32),
        gradient_stops_to_skia(stops),
        tiny_skia::SpreadMode::Pad,
        tiny_skia::Transform::identity(),
    )
}

fn make_radial_gradient(
    stops: &[ResolvedGradientStop],
    center: kurbo::Point,
    radius: kurbo::Point,
) -> Option<tiny_skia::Shader<'static>> {
    // Two-point conical: start=center, end=center, r=radius.x
    // Elliptical via scale transform on Y axis
    let rx = radius.x as f32;
    let ry = radius.y as f32;
    let transform = if (rx - ry).abs() > f32::EPSILON && rx > 0.0 {
        tiny_skia::Transform::from_scale(1.0, ry / rx)
    } else {
        tiny_skia::Transform::identity()
    };
    tiny_skia::RadialGradient::new(
        tiny_skia::Point::from_xy(center.x as f32, center.y as f32),
        tiny_skia::Point::from_xy(center.x as f32, center.y as f32),
        rx,
        gradient_stops_to_skia(stops),
        tiny_skia::SpreadMode::Pad,
        transform,
    )
}

/// Sample a gradient color at a given position (0.0 to 1.0) from sorted stops.
fn sample_gradient(stops: &[ResolvedGradientStop], t: f32) -> tiny_skia::Color {
    if stops.is_empty() { return tiny_skia::Color::TRANSPARENT; }
    if t <= stops[0].position { return color_to_skia(&stops[0].color); }
    if t >= stops[stops.len() - 1].position { return color_to_skia(&stops[stops.len() - 1].color); }
    for i in 0..stops.len() - 1 {
        if t >= stops[i].position && t <= stops[i + 1].position {
            let range = stops[i + 1].position - stops[i].position;
            let frac = if range > 0.0 { (t - stops[i].position) / range } else { 0.0 };
            let c0 = color_to_skia(&stops[i].color);
            let c1 = color_to_skia(&stops[i + 1].color);
            return tiny_skia::Color::from_rgba8(
                lerp_u8(c0.red(), c1.red(), frac),
                lerp_u8(c0.green(), c1.green(), frac),
                lerp_u8(c0.blue(), c1.blue(), frac),
                lerp_u8(c0.alpha(), c1.alpha(), frac),
            );
        }
    }
    color_to_skia(&stops[stops.len() - 1].color)
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

/// Manual angular gradient: for each pixel compute angle from center, sample gradient.
fn fill_angular_gradient(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    stops: &[ResolvedGradientStop],
    center: kurbo::Point,
    angle: f32,
    fill_rule: tiny_skia::FillRule,
    transform: tiny_skia::Transform,
    mask: Option<&tiny_skia::Mask>,
) {
    let w = pixmap.width();
    let h = pixmap.height();
    if let Some(mut grad_pixmap) = tiny_skia::Pixmap::new(w, h) {
        let cx = center.x as f32;
        let cy = center.y as f32;
        let angle_offset = angle.to_radians();
        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let mut a = dy.atan2(dx) - angle_offset;
                if a < 0.0 { a += std::f32::consts::TAU; }
                let t = a / std::f32::consts::TAU;
                let color = sample_gradient(stops, t);
                // Convert non-premultiplied Color to premultiplied pixel
                let pm = color.premultiply().to_color_u8();
                grad_pixmap.pixels_mut()[(y * w + x) as usize] = pm;
            }
        }
        // Now fill the path using this gradient pixmap as a pattern
        // First, create a mask from the path
        if let Some(mut clip_mask) = tiny_skia::Mask::new(w, h) {
            clip_mask.fill_path(path, fill_rule, true, transform);
            let paint = tiny_skia::PixmapPaint {
                opacity: 1.0,
                blend_mode: tiny_skia::BlendMode::SourceOver,
                quality: tiny_skia::FilterQuality::Nearest,
            };
            pixmap.draw_pixmap(0, 0, grad_pixmap.as_ref(), &paint, tiny_skia::Transform::identity(), Some(&clip_mask));
        }
    }
}

/// Manual diamond gradient: Manhattan distance-based color sampling.
fn fill_diamond_gradient(
    pixmap: &mut tiny_skia::Pixmap,
    path: &tiny_skia::Path,
    stops: &[ResolvedGradientStop],
    center: kurbo::Point,
    radius: kurbo::Point,
    fill_rule: tiny_skia::FillRule,
    transform: tiny_skia::Transform,
    mask: Option<&tiny_skia::Mask>,
) {
    let w = pixmap.width();
    let h = pixmap.height();
    if let Some(mut grad_pixmap) = tiny_skia::Pixmap::new(w, h) {
        let cx = center.x as f32;
        let cy = center.y as f32;
        let rx = radius.x as f32;
        let ry = radius.y as f32;
        for y in 0..h {
            for x in 0..w {
                let dx = ((x as f32 - cx) / rx).abs();
                let dy = ((y as f32 - cy) / ry).abs();
                let t = (dx + dy).min(1.0);
                let color = sample_gradient(stops, t);
                // Convert non-premultiplied Color to premultiplied pixel
                let pm = color.premultiply().to_color_u8();
                grad_pixmap.pixels_mut()[(y * w + x) as usize] = pm;
            }
        }
        if let Some(mut clip_mask) = tiny_skia::Mask::new(w, h) {
            clip_mask.fill_path(path, fill_rule, true, transform);
            let paint = tiny_skia::PixmapPaint {
                opacity: 1.0,
                blend_mode: tiny_skia::BlendMode::SourceOver,
                quality: tiny_skia::FilterQuality::Nearest,
            };
            pixmap.draw_pixmap(0, 0, grad_pixmap.as_ref(), &paint, tiny_skia::Transform::identity(), Some(&clip_mask));
        }
    }
}
```

- [ ] **Step 4: Add to lib.rs**

```rust
pub mod paint;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p ode-core`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(ode-core): add paint conversion (solid, linear, radial, angular, diamond)"
```

---

### Task 7: Gaussian Blur and Effects

**Files:**
- Create: `crates/ode-core/src/effects.rs`
- Modify: `crates/ode-core/src/lib.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_blur_single_pass_averages() {
        // 5x1 image: [0, 0, 255, 0, 0] with radius 1 should average the center
        let mut pixmap = tiny_skia::Pixmap::new(5, 1).unwrap();
        // Set center pixel to white
        pixmap.pixels_mut()[2] = tiny_skia::PremultipliedColorU8::from_rgba8(255, 255, 255, 255);
        gaussian_blur(&mut pixmap, 1.0);
        // Center should be dimmer, neighbors should be non-zero
        let center = pixmap.pixel(2, 0).unwrap();
        let left = pixmap.pixel(1, 0).unwrap();
        assert!(center.alpha() > 0);
        assert!(left.alpha() > 0, "Blur should spread to neighbors");
    }

    #[test]
    fn gaussian_blur_zero_radius_is_noop() {
        let mut pixmap = tiny_skia::Pixmap::new(10, 10).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(128, 64, 32, 255));
        let original = pixmap.pixel(5, 5).unwrap();
        gaussian_blur(&mut pixmap, 0.0);
        let after = pixmap.pixel(5, 5).unwrap();
        assert_eq!(original, after);
    }
}
```

- [ ] **Step 2: Run tests, verify fail**

Run: `cargo test -p ode-core`
Expected: FAIL.

- [ ] **Step 3: Implement Gaussian blur and effect rendering**

```rust
use crate::scene::ResolvedEffect;

/// Apply Gaussian blur to a pixmap using 3-pass box blur approximation.
/// Each pass is O(n) regardless of radius (separable horizontal + vertical).
pub fn gaussian_blur(pixmap: &mut tiny_skia::Pixmap, radius: f32) {
    if radius <= 0.0 { return; }
    let w = pixmap.width() as usize;
    let h = pixmap.height() as usize;
    if w == 0 || h == 0 { return; }

    // Box blur radius for 3-pass approximation of Gaussian
    // See: http://blog.ivank.net/fastest-gaussian-blur.html
    let boxes = boxes_for_gauss(radius, 3);

    let mut src = extract_channels(pixmap);
    let mut dst = vec![vec![0.0f32; w * h]; 4];

    for &box_r in &boxes {
        box_blur_h(&src, &mut dst, w, h, box_r);
        box_blur_v(&dst, &mut src, w, h, box_r);
    }

    write_channels(pixmap, &src);
}

fn boxes_for_gauss(sigma: f32, n: usize) -> Vec<f32> {
    let w_ideal = ((12.0 * sigma * sigma / n as f32) + 1.0).sqrt();
    let mut wl = w_ideal.floor();
    if wl as i32 % 2 == 0 { wl -= 1.0; }
    let wu = wl + 2.0;
    let m_ideal = (12.0 * sigma * sigma - n as f32 * wl * wl - 4.0 * n as f32 * wl - 3.0 * n as f32)
        / (-4.0 * wl - 4.0);
    let m = m_ideal.round() as usize;
    (0..n).map(|i| if i < m { wl } else { wu }).collect()
}

fn extract_channels(pixmap: &tiny_skia::Pixmap) -> Vec<Vec<f32>> {
    let pixels = pixmap.pixels();
    let len = pixels.len();
    let mut channels = vec![vec![0.0f32; len]; 4];
    for (i, px) in pixels.iter().enumerate() {
        // Unpremultiply
        let a = px.alpha() as f32;
        if a > 0.0 {
            channels[0][i] = px.red() as f32 * 255.0 / a;
            channels[1][i] = px.green() as f32 * 255.0 / a;
            channels[2][i] = px.blue() as f32 * 255.0 / a;
        }
        channels[3][i] = a;
    }
    channels
}

fn write_channels(pixmap: &mut tiny_skia::Pixmap, channels: &[Vec<f32>]) {
    let pixels = pixmap.pixels_mut();
    for (i, px) in pixels.iter_mut().enumerate() {
        let a = channels[3][i].clamp(0.0, 255.0) as u8;
        let r = channels[0][i].clamp(0.0, 255.0) as u8;
        let g = channels[1][i].clamp(0.0, 255.0) as u8;
        let b = channels[2][i].clamp(0.0, 255.0) as u8;
        *px = tiny_skia::PremultipliedColorU8::from_rgba8(r, g, b, a);
    }
}

fn box_blur_h(src: &[Vec<f32>], dst: &mut [Vec<f32>], w: usize, h: usize, r: f32) {
    let r = (r as usize) / 2;
    if r == 0 {
        for c in 0..4 { dst[c].copy_from_slice(&src[c]); }
        return;
    }
    let iarr = 1.0 / (2 * r + 1) as f32;
    for c in 0..4 {
        for y in 0..h {
            let row = y * w;
            let mut val = src[c][row] * (r + 1) as f32;
            for i in 0..r { val += src[c][row + i.min(w - 1)]; }
            for i in 0..r { val += src[c][row]; }

            let mut li = 0usize;
            let mut ri = r;
            for x in 0..w {
                dst[c][row + x] = val * iarr;
                let right = (ri + 1).min(w - 1);
                let left = if li > 0 { li - 1 } else { 0 };
                val += src[c][row + right] - src[c][row + left];
                li += 1;
                ri += 1;
            }
        }
    }
}

fn box_blur_v(src: &[Vec<f32>], dst: &mut [Vec<f32>], w: usize, h: usize, r: f32) {
    let r = (r as usize) / 2;
    if r == 0 {
        for c in 0..4 { dst[c].copy_from_slice(&src[c]); }
        return;
    }
    let iarr = 1.0 / (2 * r + 1) as f32;
    for c in 0..4 {
        for x in 0..w {
            let mut val = src[c][x] * (r + 1) as f32;
            for i in 0..r { val += src[c][i.min(h - 1) * w + x]; }
            for _ in 0..r { val += src[c][x]; }

            let mut li = 0usize;
            let mut ri = r;
            for y in 0..h {
                dst[c][y * w + x] = val * iarr;
                let bottom = ((ri + 1).min(h - 1)) * w + x;
                let top = (if li > 0 { li - 1 } else { 0 }) * w + x;
                val += src[c][bottom] - src[c][top];
                li += 1;
                ri += 1;
            }
        }
    }
}

/// Render a drop shadow effect. Returns a pixmap with the shadow to composite UNDER content.
pub fn render_drop_shadow(
    content_path: &tiny_skia::Path,
    color: &ode_format::color::Color,
    offset_x: f32,
    offset_y: f32,
    blur_radius: f32,
    _spread: f32,
    width: u32,
    height: u32,
) -> Option<tiny_skia::Pixmap> {
    let mut shadow = tiny_skia::Pixmap::new(width, height)?;
    let mut paint = tiny_skia::Paint::default();
    paint.shader = tiny_skia::Shader::SolidColor(crate::paint::color_to_skia(color));
    paint.anti_alias = true;
    let transform = tiny_skia::Transform::from_translate(offset_x, offset_y);
    shadow.fill_path(content_path, &paint, tiny_skia::FillRule::Winding, transform, None);
    if blur_radius > 0.0 {
        gaussian_blur(&mut shadow, blur_radius);
    }
    Some(shadow)
}

/// Render an inner shadow effect. Returns a pixmap to composite OVER content.
pub fn render_inner_shadow(
    content_path: &tiny_skia::Path,
    color: &ode_format::color::Color,
    offset_x: f32,
    offset_y: f32,
    blur_radius: f32,
    _spread: f32,
    width: u32,
    height: u32,
) -> Option<tiny_skia::Pixmap> {
    let mut shadow = tiny_skia::Pixmap::new(width, height)?;
    // Fill entire pixmap with shadow color
    shadow.fill(crate::paint::color_to_skia(color));
    // Cut out the content shape (translated by offset) to create inner shadow
    let mut cutout = tiny_skia::Paint::default();
    cutout.shader = tiny_skia::Shader::SolidColor(tiny_skia::Color::TRANSPARENT);
    cutout.blend_mode = tiny_skia::BlendMode::Source;
    cutout.anti_alias = true;
    let transform = tiny_skia::Transform::from_translate(offset_x, offset_y);
    shadow.fill_path(content_path, &cutout, tiny_skia::FillRule::Winding, transform, None);
    if blur_radius > 0.0 {
        gaussian_blur(&mut shadow, blur_radius);
    }
    // Clip to original path
    if let Some(mut mask) = tiny_skia::Mask::new(width, height) {
        mask.fill_path(content_path, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
        let mut clipped = tiny_skia::Pixmap::new(width, height)?;
        let paint = tiny_skia::PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: tiny_skia::FilterQuality::Nearest,
        };
        clipped.draw_pixmap(0, 0, shadow.as_ref(), &paint, tiny_skia::Transform::identity(), Some(&mask));
        return Some(clipped);
    }
    Some(shadow)
}

/// Apply layer blur: blur the given pixmap in-place.
pub fn apply_layer_blur(pixmap: &mut tiny_skia::Pixmap, radius: f32) {
    gaussian_blur(pixmap, radius);
}

/// Apply background blur: blur a region of the background pixmap.
pub fn render_background_blur(
    background: &tiny_skia::Pixmap,
    content_path: &tiny_skia::Path,
    radius: f32,
    width: u32,
    height: u32,
) -> Option<tiny_skia::Pixmap> {
    let mut blurred = background.clone();
    gaussian_blur(&mut blurred, radius);
    // Mask to content path
    if let Some(mut mask) = tiny_skia::Mask::new(width, height) {
        mask.fill_path(content_path, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
        let mut result = tiny_skia::Pixmap::new(width, height)?;
        let paint = tiny_skia::PixmapPaint {
            opacity: 1.0,
            blend_mode: tiny_skia::BlendMode::SourceOver,
            quality: tiny_skia::FilterQuality::Nearest,
        };
        result.draw_pixmap(0, 0, blurred.as_ref(), &paint, tiny_skia::Transform::identity(), Some(&mask));
        return Some(result);
    }
    Some(blurred)
}
```

- [ ] **Step 4: Add to lib.rs**

```rust
pub mod effects;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p ode-core`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(ode-core): add Gaussian blur (3-pass box blur) and effect rendering"
```

---

## Chunk 3: Core Pipeline

### Task 8: Document → Scene Conversion

**Files:**
- Create: `crates/ode-core/src/convert.rs`
- Modify: `crates/ode-core/src/lib.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::document::Document;
    use ode_format::node::{Node, NodeKind, VectorPath, PathSegment};
    use ode_format::style::*;
    use ode_format::color::Color;

    fn make_simple_doc() -> Document {
        let mut doc = Document::new("Test");
        let mut frame = Node::new_frame("Root", 100.0, 80.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);
        doc
    }

    #[test]
    fn simple_frame_produces_commands() {
        let doc = make_simple_doc();
        let scene = Scene::from_document(&doc).unwrap();
        assert!((scene.width - 100.0).abs() < f32::EPSILON);
        assert!((scene.height - 80.0).abs() < f32::EPSILON);
        // Should have: PushLayer, FillPath (red fill), PopLayer
        assert!(scene.commands.len() >= 3, "Expected at least 3 commands, got {}", scene.commands.len());
    }

    #[test]
    fn empty_canvas_is_error() {
        let doc = Document::new("Empty");
        let result = Scene::from_document(&doc);
        assert!(result.is_err());
    }

    #[test]
    fn group_produces_no_fill() {
        let mut doc = Document::new("Group Test");
        let group = Node::new_group("G");
        let gid = doc.nodes.insert(group);
        let mut frame = Node::new_frame("Container", 200.0, 200.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.container.children.push(gid);
        }
        let fid = doc.nodes.insert(frame);
        doc.canvas.push(fid);
        let scene = Scene::from_document(&doc).unwrap();
        // Group has no visual — should have PushLayer/PopLayer but no FillPath for the group itself
        let fill_count = scene.commands.iter().filter(|c| matches!(c, RenderCommand::FillPath { .. })).count();
        // Only the frame may have fills, not the group
        assert!(fill_count <= 1, "Group should not produce FillPath");
    }
}
```

- [ ] **Step 2: Run tests, verify fail**

Run: `cargo test -p ode-core`
Expected: FAIL.

- [ ] **Step 3: Implement conversion**

```rust
use ode_format::document::Document;
use ode_format::node::{Node, NodeId, NodeKind, FrameData, FillRule as OdeFillRule};
use ode_format::style::{Paint, Effect, StyleValue, Fill, Stroke};
use ode_format::color::Color;
use crate::error::ConvertError;
use crate::scene::*;
use crate::path;

impl Scene {
    /// Convert a Document into a Scene.
    pub fn from_document(doc: &Document) -> Result<Self, ConvertError> {
        if doc.canvas.is_empty() {
            return Err(ConvertError::NoCanvasRoots);
        }

        // Determine scene size from first canvas root
        let first_root = doc.canvas[0];
        let (width, height) = get_frame_size(&doc.nodes[first_root]);

        let mut commands = Vec::new();
        let identity = tiny_skia::Transform::identity();

        for &root_id in &doc.canvas {
            convert_node(doc, root_id, identity, &mut commands);
        }

        Ok(Scene { width, height, commands })
    }
}

fn get_frame_size(node: &Node) -> (f32, f32) {
    if let NodeKind::Frame(ref data) = node.kind {
        (data.width, data.height)
    } else {
        (100.0, 100.0) // Default fallback
    }
}

fn convert_node(
    doc: &Document,
    node_id: NodeId,
    parent_transform: tiny_skia::Transform,
    commands: &mut Vec<RenderCommand>,
) {
    let node = &doc.nodes[node_id];

    // Accumulate transform
    let node_transform = path::transform_to_skia(&node.transform);
    let current_transform = parent_transform.post_concat(node_transform);

    // Get clip path for frames (clipping to frame bounds)
    let clip = get_clip_path(node);

    // PushLayer
    commands.push(RenderCommand::PushLayer {
        opacity: node.opacity,
        blend_mode: node.blend_mode,
        clip,
        transform: current_transform,
    });

    // Visual content (fills, strokes, effects)
    if let Some(visual) = node.kind.visual() {
        let node_path = get_node_path(doc, node);

        // Effects that render BEHIND content (DropShadow)
        for effect in &visual.effects {
            if let Effect::DropShadow { color, offset, blur, spread } = effect {
                commands.push(RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::DropShadow {
                        color: color.value(),
                        offset_x: offset.x,
                        offset_y: offset.y,
                        blur_radius: blur.value(),
                        spread: spread.value(),
                    },
                });
            }
        }

        // Fills
        if let Some(ref bp) = node_path {
            let fill_rule = get_fill_rule(node);
            for fill in &visual.fills {
                if !fill.visible { continue; }
                if let Some(resolved) = resolve_paint(&fill.paint) {
                    commands.push(RenderCommand::FillPath {
                        path: bp.clone(),
                        paint: resolved,
                        fill_rule,
                        transform: current_transform,
                    });
                }
            }

            // Strokes
            for stroke in &visual.strokes {
                if !stroke.visible { continue; }
                if let Some(resolved) = resolve_paint(&stroke.paint) {
                    commands.push(RenderCommand::StrokePath {
                        path: bp.clone(),
                        paint: resolved,
                        stroke: StrokeStyle {
                            width: stroke.width.value(),
                            position: stroke.position,
                            cap: stroke.cap,
                            join: stroke.join,
                            miter_limit: stroke.miter_limit,
                            dash: stroke.dash.clone(),
                        },
                        transform: current_transform,
                    });
                }
            }
        }

        // Effects that render ON content (InnerShadow, LayerBlur, BackgroundBlur)
        for effect in &visual.effects {
            match effect {
                Effect::InnerShadow { color, offset, blur, spread } => {
                    commands.push(RenderCommand::ApplyEffect {
                        effect: ResolvedEffect::InnerShadow {
                            color: color.value(),
                            offset_x: offset.x,
                            offset_y: offset.y,
                            blur_radius: blur.value(),
                            spread: spread.value(),
                        },
                    });
                }
                Effect::LayerBlur { radius } => {
                    commands.push(RenderCommand::ApplyEffect {
                        effect: ResolvedEffect::LayerBlur { radius: radius.value() },
                    });
                }
                Effect::BackgroundBlur { radius } => {
                    commands.push(RenderCommand::ApplyEffect {
                        effect: ResolvedEffect::BackgroundBlur { radius: radius.value() },
                    });
                }
                Effect::DropShadow { .. } => {} // Already handled above
            }
        }
    }

    // Recurse into children
    if let Some(children) = node.kind.children() {
        for &child_id in children {
            convert_node(doc, child_id, current_transform, commands);
        }
    }

    // PopLayer
    commands.push(RenderCommand::PopLayer);
}

fn get_clip_path(node: &Node) -> Option<kurbo::BezPath> {
    if let NodeKind::Frame(ref data) = node.kind {
        if data.width > 0.0 && data.height > 0.0 {
            return Some(path::rounded_rect_path(data.width, data.height, data.corner_radius));
        }
    }
    None
}

fn get_node_path(doc: &Document, node: &Node) -> Option<kurbo::BezPath> {
    match &node.kind {
        NodeKind::Frame(data) => {
            if data.width > 0.0 && data.height > 0.0 {
                Some(path::rounded_rect_path(data.width, data.height, data.corner_radius))
            } else {
                None
            }
        }
        NodeKind::Vector(data) => {
            Some(path::to_bezpath(&data.path))
        }
        NodeKind::BooleanOp(data) => {
            // Collect child paths and apply boolean operation
            if let Some(children) = node.kind.children() {
                let mut paths: Vec<kurbo::BezPath> = Vec::new();
                for &child_id in children {
                    let child = &doc.nodes[child_id];
                    if let Some(child_path) = get_node_path(doc, child) {
                        paths.push(child_path);
                    }
                }
                if paths.len() >= 2 {
                    let mut result = paths[0].clone();
                    for p in &paths[1..] {
                        if let Ok(r) = path::boolean_op(&result, p, data.op) {
                            result = r;
                        }
                    }
                    Some(result)
                } else {
                    paths.into_iter().next()
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn get_fill_rule(node: &Node) -> ode_format::node::FillRule {
    if let NodeKind::Vector(ref data) = node.kind {
        data.fill_rule
    } else {
        OdeFillRule::NonZero
    }
}

/// Resolve a format-level Paint to a render-level ResolvedPaint.
fn resolve_paint(paint: &Paint) -> Option<ResolvedPaint> {
    match paint {
        Paint::Solid { color } => Some(ResolvedPaint::Solid(color.value())),
        Paint::LinearGradient { stops, start, end } => Some(ResolvedPaint::LinearGradient {
            stops: stops.iter().map(|s| ResolvedGradientStop {
                position: s.position,
                color: s.color.value(),
            }).collect(),
            start: kurbo::Point::new(start.x as f64, start.y as f64),
            end: kurbo::Point::new(end.x as f64, end.y as f64),
        }),
        Paint::RadialGradient { stops, center, radius } => Some(ResolvedPaint::RadialGradient {
            stops: stops.iter().map(|s| ResolvedGradientStop { position: s.position, color: s.color.value() }).collect(),
            center: kurbo::Point::new(center.x as f64, center.y as f64),
            radius: kurbo::Point::new(radius.x as f64, radius.y as f64),
        }),
        Paint::AngularGradient { stops, center, angle } => Some(ResolvedPaint::AngularGradient {
            stops: stops.iter().map(|s| ResolvedGradientStop { position: s.position, color: s.color.value() }).collect(),
            center: kurbo::Point::new(center.x as f64, center.y as f64),
            angle: *angle,
        }),
        Paint::DiamondGradient { stops, center, radius } => Some(ResolvedPaint::DiamondGradient {
            stops: stops.iter().map(|s| ResolvedGradientStop { position: s.position, color: s.color.value() }).collect(),
            center: kurbo::Point::new(center.x as f64, center.y as f64),
            radius: kurbo::Point::new(radius.x as f64, radius.y as f64),
        }),
        // MeshGradient and ImageFill deferred
        _ => None,
    }
}
```

- [ ] **Step 4: Add to lib.rs**

```rust
pub mod convert;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p ode-core`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(ode-core): add Document → Scene IR conversion"
```

---

### Task 9: Renderer

**Files:**
- Create: `crates/ode-core/src/render.rs`
- Modify: `crates/ode-core/src/lib.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::color::Color;
    use ode_format::node::FillRule;
    use ode_format::style::BlendMode;
    use crate::scene::*;

    fn red_rect_scene() -> Scene {
        let mut bp = kurbo::BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((50.0, 0.0));
        bp.line_to((50.0, 50.0));
        bp.line_to((0.0, 50.0));
        bp.close_path();

        Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::FillPath {
                    path: bp,
                    paint: ResolvedPaint::Solid(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        }
    }

    #[test]
    fn render_red_rectangle() {
        let scene = red_rect_scene();
        let pixmap = Renderer::render(&scene).unwrap();
        assert_eq!(pixmap.width(), 50);
        assert_eq!(pixmap.height(), 50);
        let center = pixmap.pixel(25, 25).unwrap();
        assert_eq!(center.red(), 255);
        assert_eq!(center.green(), 0);
        assert_eq!(center.blue(), 0);
        assert_eq!(center.alpha(), 255);
    }

    #[test]
    fn render_with_opacity() {
        let mut bp = kurbo::BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((50.0, 0.0));
        bp.line_to((50.0, 50.0));
        bp.line_to((0.0, 50.0));
        bp.close_path();

        let scene = Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 0.5,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::FillPath {
                    path: bp,
                    paint: ResolvedPaint::Solid(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        let pixmap = Renderer::render(&scene).unwrap();
        let center = pixmap.pixel(25, 25).unwrap();
        // 50% opacity red on transparent = ~128 alpha
        assert!(center.alpha() > 100 && center.alpha() < 160,
            "Expected ~128 alpha, got {}", center.alpha());
    }

    #[test]
    fn empty_scene_error() {
        let scene = Scene { width: 0.0, height: 0.0, commands: vec![] };
        assert!(Renderer::render(&scene).is_err());
    }
}
```

- [ ] **Step 2: Run tests, verify fail**

Run: `cargo test -p ode-core`
Expected: FAIL.

- [ ] **Step 3: Implement renderer**

```rust
use crate::error::RenderError;
use crate::scene::*;
use crate::blend;
use crate::paint;
use crate::effects;
use crate::path;

/// Stateless renderer: converts a Scene into a Pixmap.
pub struct Renderer;

struct LayerEntry {
    pixmap: tiny_skia::Pixmap,
    mask: Option<tiny_skia::Mask>,
    paint: tiny_skia::PixmapPaint,
}

impl Renderer {
    pub fn render(scene: &Scene) -> Result<tiny_skia::Pixmap, RenderError> {
        let w = scene.width.ceil() as u32;
        let h = scene.height.ceil() as u32;
        if w == 0 || h == 0 {
            return Err(RenderError::PixmapCreationFailed { width: w, height: h });
        }
        let root = tiny_skia::Pixmap::new(w, h).ok_or(RenderError::PixmapCreationFailed { width: w, height: h })?;

        let mut stack: Vec<LayerEntry> = vec![LayerEntry {
            pixmap: root,
            mask: None,
            paint: tiny_skia::PixmapPaint::default(),
        }];

        for cmd in &scene.commands {
            match cmd {
                RenderCommand::PushLayer { opacity, blend_mode, clip, transform } => {
                    let layer_pixmap = tiny_skia::Pixmap::new(w, h)
                        .ok_or(RenderError::PixmapCreationFailed { width: w, height: h })?;
                    let mask = clip.as_ref().and_then(|clip_path| {
                        let mut m = tiny_skia::Mask::new(w, h)?;
                        if let Some(skia_path) = path::bezpath_to_skia(clip_path) {
                            m.fill_path(&skia_path, tiny_skia::FillRule::Winding, true, *transform);
                        }
                        Some(m)
                    });
                    let paint = tiny_skia::PixmapPaint {
                        opacity: *opacity,
                        blend_mode: blend::to_skia_blend(*blend_mode),
                        quality: tiny_skia::FilterQuality::Nearest,
                    };
                    stack.push(LayerEntry { pixmap: layer_pixmap, mask, paint });
                }
                RenderCommand::PopLayer => {
                    if stack.len() <= 1 { continue; }
                    let entry = stack.pop().unwrap();
                    let parent = stack.last_mut().unwrap();
                    parent.pixmap.draw_pixmap(
                        0, 0,
                        entry.pixmap.as_ref(),
                        &entry.paint,
                        tiny_skia::Transform::identity(),
                        entry.mask.as_ref(),
                    );
                }
                RenderCommand::FillPath { path: bp, paint: resolved_paint, fill_rule, transform } => {
                    let current = stack.last_mut().unwrap();
                    if let Some(skia_path) = path::bezpath_to_skia(bp) {
                        let skia_fill_rule = blend::to_skia_fill_rule(*fill_rule);
                        paint::fill_with_paint(
                            &mut current.pixmap,
                            &skia_path,
                            resolved_paint,
                            skia_fill_rule,
                            *transform,
                            None,
                        );
                    }
                }
                RenderCommand::StrokePath { path: bp, paint: resolved_paint, stroke, transform } => {
                    let current = stack.last_mut().unwrap();
                    if let Some(skia_path) = path::bezpath_to_skia(bp) {
                        let skia_stroke = to_skia_stroke(stroke);
                        // Handle stroke position
                        match stroke.position {
                            ode_format::style::StrokePosition::Center => {
                                paint::stroke_with_paint(
                                    &mut current.pixmap, &skia_path, resolved_paint,
                                    &skia_stroke, *transform, None,
                                );
                            }
                            ode_format::style::StrokePosition::Inside => {
                                // Stroke with 2x width, masked to interior
                                let mut wide_stroke = skia_stroke.clone();
                                wide_stroke.width *= 2.0;
                                let mask = build_fill_mask(&skia_path, w, h);
                                paint::stroke_with_paint(
                                    &mut current.pixmap, &skia_path, resolved_paint,
                                    &wide_stroke, *transform, mask.as_ref(),
                                );
                            }
                            ode_format::style::StrokePosition::Outside => {
                                // Stroke with 2x width, masked to exterior
                                let mut wide_stroke = skia_stroke.clone();
                                wide_stroke.width *= 2.0;
                                let mask = build_inverted_fill_mask(&skia_path, w, h);
                                paint::stroke_with_paint(
                                    &mut current.pixmap, &skia_path, resolved_paint,
                                    &wide_stroke, *transform, mask.as_ref(),
                                );
                            }
                        }
                    }
                }
                RenderCommand::ApplyEffect { effect } => {
                    let current = stack.last_mut().unwrap();
                    match effect {
                        ResolvedEffect::DropShadow { color, offset_x, offset_y, blur_radius, spread } => {
                            // Render shadow to temp pixmap, then composite UNDER current content
                            // Use the entire current layer as the shadow shape
                            let rect_path = tiny_skia::PathBuilder::from_rect(
                                tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap()
                            );
                            if let Some(shadow) = effects::render_drop_shadow(
                                &rect_path, color, *offset_x, *offset_y, *blur_radius, *spread, w, h,
                            ) {
                                // Draw shadow under current content: save current, clear, draw shadow, draw content on top
                                let content = current.pixmap.clone();
                                current.pixmap.fill(tiny_skia::Color::TRANSPARENT);
                                let paint = tiny_skia::PixmapPaint {
                                    opacity: 1.0,
                                    blend_mode: tiny_skia::BlendMode::SourceOver,
                                    quality: tiny_skia::FilterQuality::Nearest,
                                };
                                current.pixmap.draw_pixmap(0, 0, shadow.as_ref(), &paint, tiny_skia::Transform::identity(), None);
                                current.pixmap.draw_pixmap(0, 0, content.as_ref(), &paint, tiny_skia::Transform::identity(), None);
                            }
                        }
                        ResolvedEffect::InnerShadow { color, offset_x, offset_y, blur_radius, spread } => {
                            let rect_path = tiny_skia::PathBuilder::from_rect(
                                tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap()
                            );
                            if let Some(shadow) = effects::render_inner_shadow(
                                &rect_path, color, *offset_x, *offset_y, *blur_radius, *spread, w, h,
                            ) {
                                let paint = tiny_skia::PixmapPaint {
                                    opacity: 1.0,
                                    blend_mode: tiny_skia::BlendMode::SourceOver,
                                    quality: tiny_skia::FilterQuality::Nearest,
                                };
                                current.pixmap.draw_pixmap(0, 0, shadow.as_ref(), &paint, tiny_skia::Transform::identity(), None);
                            }
                        }
                        ResolvedEffect::LayerBlur { radius } => {
                            effects::apply_layer_blur(&mut current.pixmap, *radius);
                        }
                        ResolvedEffect::BackgroundBlur { radius } => {
                            // Get background from parent layer (if available)
                            if stack.len() >= 2 {
                                let parent_pixmap = &stack[stack.len() - 2].pixmap;
                                let rect_path = tiny_skia::PathBuilder::from_rect(
                                    tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap()
                                );
                                if let Some(blurred_bg) = effects::render_background_blur(
                                    parent_pixmap, &rect_path, *radius, w, h,
                                ) {
                                    let paint = tiny_skia::PixmapPaint {
                                        opacity: 1.0,
                                        blend_mode: tiny_skia::BlendMode::DestinationOver,
                                        quality: tiny_skia::FilterQuality::Nearest,
                                    };
                                    current.pixmap.draw_pixmap(0, 0, blurred_bg.as_ref(), &paint, tiny_skia::Transform::identity(), None);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Return the root pixmap
        if let Some(entry) = stack.into_iter().next() {
            Ok(entry.pixmap)
        } else {
            Err(RenderError::EmptyScene)
        }
    }
}

fn to_skia_stroke(style: &StrokeStyle) -> tiny_skia::Stroke {
    let mut stroke = tiny_skia::Stroke {
        width: style.width,
        line_cap: match style.cap {
            ode_format::style::StrokeCap::Butt => tiny_skia::LineCap::Butt,
            ode_format::style::StrokeCap::Round => tiny_skia::LineCap::Round,
            ode_format::style::StrokeCap::Square => tiny_skia::LineCap::Square,
        },
        line_join: match style.join {
            ode_format::style::StrokeJoin::Miter => tiny_skia::LineJoin::Miter,
            ode_format::style::StrokeJoin::Round => tiny_skia::LineJoin::Round,
            ode_format::style::StrokeJoin::Bevel => tiny_skia::LineJoin::Bevel,
        },
        miter_limit: style.miter_limit,
        dash: None,
    };
    if let Some(ref dash) = style.dash {
        stroke.dash = tiny_skia::StrokeDash::new(dash.segments.clone(), dash.offset);
    }
    stroke
}

fn build_fill_mask(path: &tiny_skia::Path, w: u32, h: u32) -> Option<tiny_skia::Mask> {
    let mut mask = tiny_skia::Mask::new(w, h)?;
    mask.fill_path(path, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
    Some(mask)
}

fn build_inverted_fill_mask(path: &tiny_skia::Path, w: u32, h: u32) -> Option<tiny_skia::Mask> {
    // Build a fill mask, then invert all bytes manually
    let mut mask = tiny_skia::Mask::new(w, h)?;
    mask.fill_path(path, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
    // Invert: every byte 0→255, 255→0
    for byte in mask.data_mut() {
        *byte = 255 - *byte;
    }
    Some(mask)
}
```

- [ ] **Step 4: Add to lib.rs**

Update `crates/ode-core/src/lib.rs`:

```rust
pub mod error;
pub mod scene;
pub mod path;
pub mod blend;
pub mod paint;
pub mod effects;
pub mod convert;
pub mod render;

pub use scene::{Scene, RenderCommand, ResolvedPaint, ResolvedEffect};
pub use render::Renderer;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p ode-core`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(ode-core): add Renderer with layer stack, command dispatch, stroke position"
```

---

### Task 10: Boolean Path Operations

**Files:**
- Modify: `crates/ode-core/src/path.rs`
- Modify: `crates/ode-core/src/convert.rs`

- [ ] **Step 1: Write tests**

Add to `crates/ode-core/src/path.rs` tests:

```rust
#[test]
fn boolean_union_two_overlapping_rects() {
    use ode_format::node::BooleanOperation;
    let mut r1 = BezPath::new();
    r1.move_to((0.0, 0.0));
    r1.line_to((60.0, 0.0));
    r1.line_to((60.0, 60.0));
    r1.line_to((0.0, 60.0));
    r1.close_path();

    let mut r2 = BezPath::new();
    r2.move_to((30.0, 30.0));
    r2.line_to((90.0, 30.0));
    r2.line_to((90.0, 90.0));
    r2.line_to((30.0, 90.0));
    r2.close_path();

    let result = boolean_op(&r1, &r2, BooleanOperation::Union);
    assert!(result.is_ok(), "Union should succeed");
    let path = result.unwrap();
    // Union of two overlapping rects should have more than 4 points
    assert!(path.elements().len() > 4);
}

#[test]
fn boolean_subtract() {
    use ode_format::node::BooleanOperation;
    let mut r1 = BezPath::new();
    r1.move_to((0.0, 0.0));
    r1.line_to((100.0, 0.0));
    r1.line_to((100.0, 100.0));
    r1.line_to((0.0, 100.0));
    r1.close_path();

    let mut r2 = BezPath::new();
    r2.move_to((25.0, 25.0));
    r2.line_to((75.0, 25.0));
    r2.line_to((75.0, 75.0));
    r2.line_to((25.0, 75.0));
    r2.close_path();

    let result = boolean_op(&r1, &r2, BooleanOperation::Subtract);
    assert!(result.is_ok(), "Subtract should succeed");
}
```

- [ ] **Step 2: Run tests, verify fail**

Run: `cargo test -p ode-core`
Expected: FAIL — `boolean_op` not found.

- [ ] **Step 3: Implement boolean operations**

Add to `crates/ode-core/src/path.rs`:

```rust
use ode_format::node::BooleanOperation;
use crate::error::RenderError;
use i_overlay::core::fill_rule::FillRule as OverlayFillRule;
use i_overlay::core::overlay::ShapeType;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::f64::overlay::F64Overlay;

/// Apply a boolean operation to two paths using i_overlay.
pub fn boolean_op(
    a: &BezPath,
    b: &BezPath,
    op: BooleanOperation,
) -> Result<BezPath, RenderError> {
    let contours_a = bezpath_to_contours(a);
    let contours_b = bezpath_to_contours(b);

    let rule = match op {
        BooleanOperation::Union => OverlayRule::Union,
        BooleanOperation::Subtract => OverlayRule::Difference,
        BooleanOperation::Intersect => OverlayRule::Intersect,
        BooleanOperation::Exclude => OverlayRule::Xor,
    };

    let mut overlay = F64Overlay::new();
    for contour in &contours_a {
        overlay.add_path(contour.clone(), ShapeType::Subject);
    }
    for contour in &contours_b {
        overlay.add_path(contour.clone(), ShapeType::Clip);
    }

    let graph = overlay.into_graph(OverlayFillRule::NonZero);
    let shapes = graph.extract_shapes(rule);

    // Convert back to BezPath
    let mut result = BezPath::new();
    for shape in &shapes {
        for contour in shape {
            if contour.is_empty() { continue; }
            result.move_to((contour[0].x, contour[0].y));
            for pt in &contour[1..] {
                result.line_to((pt.x, pt.y));
            }
            result.close_path();
        }
    }

    Ok(result)
}

/// Convert BezPath into contours (Vec of Vec of points) for i_overlay.
/// i_overlay works with polygons, so curves are flattened to line segments.
fn bezpath_to_contours(bp: &BezPath) -> Vec<Vec<i_overlay::f64::point::F64Point>> {
    use i_overlay::f64::point::F64Point;
    let mut contours = Vec::new();
    let mut current = Vec::new();

    // Flatten curves to line segments with tolerance.
    // kurbo 0.11 flatten() takes a callback, not an iterator.
    bp.flatten(0.25, |el| {
        match el {
            PathEl::MoveTo(p) => {
                if !current.is_empty() {
                    contours.push(std::mem::take(&mut current));
                }
                current.push(F64Point::new(p.x, p.y));
            }
            PathEl::LineTo(p) => {
                current.push(F64Point::new(p.x, p.y));
            }
            PathEl::ClosePath => {
                if !current.is_empty() {
                    contours.push(std::mem::take(&mut current));
                }
            }
            _ => {} // flatten() only produces MoveTo, LineTo, ClosePath
        }
    });
    if !current.is_empty() {
        contours.push(current);
    }
    contours
}
```

Note: `get_node_path(doc, node)` already handles BooleanOp from Task 8. No changes to `convert.rs` are needed — boolean ops were integrated into `get_node_path` from the start.

- [ ] **Step 4: Run tests**

Run: `cargo test -p ode-core`
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(ode-core): add boolean path operations via i_overlay"
```

---

## Chunk 4: Export and Integration

### Task 11: PNG Export

**Files:**
- Create: `crates/ode-export/src/png.rs`
- Modify: `crates/ode-export/src/lib.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_bytes_produces_valid_png() {
        let mut pixmap = tiny_skia::Pixmap::new(10, 10).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let bytes = PngExporter::export_bytes(&pixmap).unwrap();
        // PNG magic bytes
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn export_to_file_creates_file() {
        let mut pixmap = tiny_skia::Pixmap::new(10, 10).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(0, 255, 0, 255));
        let path = std::env::temp_dir().join("ode_test_export.png");
        PngExporter::export(&pixmap, &path).unwrap();
        assert!(path.exists());
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
        std::fs::remove_file(&path).ok();
    }
}
```

- [ ] **Step 2: Run tests, verify fail**

Run: `cargo test -p ode-export`
Expected: FAIL.

- [ ] **Step 3: Implement PngExporter**

```rust
use crate::error::ExportError;

/// PNG export using tiny-skia's built-in encoder.
pub struct PngExporter;

impl PngExporter {
    /// Save a Pixmap to a PNG file.
    pub fn export(pixmap: &tiny_skia::Pixmap, path: &std::path::Path) -> Result<(), ExportError> {
        let bytes = Self::export_bytes(pixmap)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Encode a Pixmap to PNG bytes in memory.
    pub fn export_bytes(pixmap: &tiny_skia::Pixmap) -> Result<Vec<u8>, ExportError> {
        pixmap.encode_png().map_err(|e| ExportError::PngEncodeFailed(e.to_string()))
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
pub mod error;
pub mod png;

pub use png::PngExporter;
pub use error::ExportError;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p ode-export`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(ode-export): add PngExporter"
```

---

### Task 12: End-to-End Integration Test

**Files:**
- Create: `crates/ode-export/tests/integration.rs`

- [ ] **Step 1: Write end-to-end test**

```rust
use ode_format::color::Color;
use ode_format::document::Document;
use ode_format::node::{Node, NodeKind, VectorPath, PathSegment};
use ode_format::style::*;
use ode_core::{Scene, Renderer};
use ode_export::PngExporter;

/// End-to-end: Build document → convert to scene → render → export PNG → verify pixels
#[test]
fn document_to_png_red_frame() {
    // 1. Build document with a red-filled frame
    let mut doc = Document::new("E2E Test");
    let mut frame = Node::new_frame("Red Box", 64.0, 64.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let frame_id = doc.nodes.insert(frame);
    doc.canvas.push(frame_id);

    // 2. Convert to scene
    let scene = Scene::from_document(&doc).unwrap();
    assert!((scene.width - 64.0).abs() < f32::EPSILON);
    assert!((scene.height - 64.0).abs() < f32::EPSILON);

    // 3. Render to pixels
    let pixmap = Renderer::render(&scene).unwrap();
    assert_eq!(pixmap.width(), 64);
    assert_eq!(pixmap.height(), 64);

    // 4. Verify center pixel is red
    let center = pixmap.pixel(32, 32).unwrap();
    assert_eq!(center.red(), 255, "Center should be red");
    assert_eq!(center.green(), 0);
    assert_eq!(center.blue(), 0);
    assert_eq!(center.alpha(), 255);

    // 5. Export to PNG bytes
    let png_bytes = PngExporter::export_bytes(&pixmap).unwrap();
    assert!(!png_bytes.is_empty());
    assert_eq!(&png_bytes[..4], &[0x89, b'P', b'N', b'G']);

    // 6. Write to temp file and verify
    let path = std::env::temp_dir().join("ode_e2e_red_frame.png");
    PngExporter::export(&pixmap, &path).unwrap();
    assert!(path.exists());
    let file_bytes = std::fs::read(&path).unwrap();
    assert_eq!(png_bytes, file_bytes);
    std::fs::remove_file(&path).ok();
}

#[test]
fn document_with_vector_path() {
    let mut doc = Document::new("Vector Test");
    // Triangle path
    let path = VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 32.0, y: 0.0 },
            PathSegment::LineTo { x: 64.0, y: 64.0 },
            PathSegment::LineTo { x: 0.0, y: 64.0 },
            PathSegment::Close,
        ],
        closed: true,
    };
    let mut frame = Node::new_frame("Container", 64.0, 64.0);
    let mut vector = Node::new_vector("Triangle", path);
    if let NodeKind::Vector(ref mut data) = vector.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb { r: 0.0, g: 0.0, b: 1.0, a: 1.0 }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let vec_id = doc.nodes.insert(vector);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.container.children.push(vec_id);
    }
    let frame_id = doc.nodes.insert(frame);
    doc.canvas.push(frame_id);

    let scene = Scene::from_document(&doc).unwrap();
    let pixmap = Renderer::render(&scene).unwrap();

    // Bottom-center of the triangle should be blue
    let bottom_center = pixmap.pixel(32, 60).unwrap();
    assert!(bottom_center.blue() > 200, "Bottom center of triangle should be blue, got b={}", bottom_center.blue());

    // Top-left corner should be transparent (outside triangle)
    let corner = pixmap.pixel(2, 2).unwrap();
    assert_eq!(corner.alpha(), 0, "Top-left corner should be transparent");
}

#[test]
fn document_with_gradient_fill() {
    let mut doc = Document::new("Gradient Test");
    let mut frame = Node::new_frame("Gradient Box", 100.0, 10.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.visual.fills.push(Fill {
            paint: Paint::LinearGradient {
                stops: vec![
                    GradientStop { position: 0.0, color: StyleValue::Raw(Color::black()) },
                    GradientStop { position: 1.0, color: StyleValue::Raw(Color::white()) },
                ],
                start: ode_format::style::Point { x: 0.0, y: 0.0 },
                end: ode_format::style::Point { x: 100.0, y: 0.0 },
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let frame_id = doc.nodes.insert(frame);
    doc.canvas.push(frame_id);

    let scene = Scene::from_document(&doc).unwrap();
    let pixmap = Renderer::render(&scene).unwrap();

    // Left should be dark, right should be light
    let left = pixmap.pixel(5, 5).unwrap();
    let right = pixmap.pixel(95, 5).unwrap();
    assert!(left.red() < 50, "Left should be dark, got r={}", left.red());
    assert!(right.red() > 200, "Right should be light, got r={}", right.red());
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p ode-export`
Expected: All pass.

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test --workspace`
Expected: All tests pass across all crates.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "test: add end-to-end Document → PNG integration tests"
```
