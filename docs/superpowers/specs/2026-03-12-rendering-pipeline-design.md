# Rendering Pipeline — Core Architecture Design

> Stage 1 of Format 1 (Icons/Illustrations): Rendering pipeline + PNG export

## Overview

Build the rendering pipeline that transforms an ODE Document into a pixel buffer (PNG). This is the first end-to-end pipeline: `Document → Scene IR → Pixmap → PNG`.

### Scope

**Included:**

| Category | Features |
|----------|----------|
| Nodes | Frame (with size + corner radius), Group, Vector (kurbo paths), BooleanOp |
| Paint | Solid, LinearGradient, RadialGradient, AngularGradient, DiamondGradient |
| Stroke | All positions (Inside/Outside/Center), all caps/joins, dash patterns |
| Compositing | Opacity, all 16 BlendModes |
| Effects | DropShadow, InnerShadow, LayerBlur, BackgroundBlur |
| Output | PNG |

**Cleanly deferred (no architectural impact when added later):**

| Category | Reason |
|----------|--------|
| Text rendering | Independent subsystem — font loading, shaping, rasterization |
| Image nodes | Independent — image loading/compositing |
| MeshGradient, ImageFill | Depends on Image system |
| SVG/PDF output | Separate output backends |
| CLI tool | UI layer on top of pipeline |

---

## Pipeline Architecture

```
Document (ode-format)
    ↓  convert (ode-core::convert)
Scene IR — flat list of render commands
    ↓  rasterize (ode-core::render)
Pixmap (tiny-skia pixel buffer)
    ↓  export (ode-export::png)
PNG file
```

### Crate Responsibilities

| Crate | Role |
|-------|------|
| `ode-core` | Document → Scene conversion, Scene → Pixmap rasterization |
| `ode-export` | Pixmap → PNG file output |

---

## Data Model Changes (ode-format)

### FrameData — add size and corner radius

```rust
pub struct FrameData {
    pub width: f32,
    pub height: f32,
    pub corner_radius: [f32; 4],  // [top-left, top-right, bottom-right, bottom-left]
    pub visual: VisualProps,
    pub container: ContainerProps,
    pub component_def: Option<ComponentDef>,
}
```

- `width/height`: Required for rendering and layout. Determines canvas size for root frames.
- `corner_radius: [f32; 4]`: Per-corner independent control. All zeros = sharp corners. Essential for icon/UI design.
- `#[serde(default)]` on `corner_radius` for backward compatibility with existing v0.1 documents.

### VectorData — add path and fill rule

```rust
pub struct VectorData {
    pub visual: VisualProps,
    pub path: VectorPath,
    pub fill_rule: FillRule,
}
```

### VectorPath — serializable path representation

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorPath {
    pub segments: Vec<PathSegment>,
    pub closed: bool,
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

// tiny-skia mapping:
// FillRule::NonZero → tiny_skia::FillRule::Winding (semantically identical)
// FillRule::EvenOdd → tiny_skia::FillRule::EvenOdd
```

**Why custom VectorPath instead of kurbo::BezPath directly:**
- `BezPath` does not implement `Serialize/Deserialize`
- The `.ode` file format must be stable — depending on an external library's internal representation is risky
- `VectorPath ↔ BezPath` conversion functions live in `ode-core::path`

### Node constructor changes

```rust
// Frame now requires size
Node::new_frame(name: &str, width: f32, height: f32) -> Self

// Vector now requires path
Node::new_vector(name: &str, path: VectorPath) -> Self
```

**Breaking change:** `Node::new_frame` and `Node::new_vector` signatures change. Existing tests that call these constructors (e.g., `node.rs`, `integration.rs`) must be updated.

### Default values for backward compatibility

- `FrameData.width/height`: `#[serde(default)]` → 0.0 (existing documents without size still deserialize; zero-size frames produce no visual output during rendering — the renderer skips them)
- `FrameData.corner_radius`: `#[serde(default)]` → [0.0; 4]
- `VectorData.path`: `#[serde(default)]` with `VectorPath { segments: vec![], closed: false }`
- `VectorData.fill_rule`: `#[serde(default)]` → `FillRule::NonZero`

---

## Scene IR (ode-core)

### Scene

```rust
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub commands: Vec<RenderCommand>,
}

impl Scene {
    /// Convert a Document into a Scene.
    /// Traverses canvas roots depth-first, resolves token bindings,
    /// and produces a flat command list.
    pub fn from_document(doc: &Document) -> Result<Self, ConvertError>;
}
```

### RenderCommand

```rust
pub enum RenderCommand {
    /// Begin a new compositing layer (creates temp Pixmap).
    /// `transform` is used ONLY for transforming the clip path when building
    /// the Mask. The temp Pixmap renders in parent coordinate space.
    /// Each FillPath/StrokePath carries its own accumulated transform.
    PushLayer {
        opacity: f32,
        blend_mode: BlendMode,
        clip: Option<kurbo::BezPath>,
        transform: tiny_skia::Transform,
    },
    /// End current layer — composite temp Pixmap into parent
    PopLayer,
    /// Fill a path
    FillPath {
        path: kurbo::BezPath,
        paint: ResolvedPaint,
        fill_rule: FillRule,
        transform: tiny_skia::Transform,
    },
    /// Stroke a path
    StrokePath {
        path: kurbo::BezPath,
        paint: ResolvedPaint,
        stroke: StrokeStyle,
        transform: tiny_skia::Transform,
    },
    /// Apply an effect
    ApplyEffect {
        effect: ResolvedEffect,
    },
}
```

### StrokeStyle

```rust
pub struct StrokeStyle {
    pub width: f32,
    pub position: StrokePosition,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f32,
    pub dash: Option<DashPattern>,
}
```

### ResolvedPaint

Token bindings resolved to final values. The renderer never sees `StyleValue` or `TokenRef`.

```rust
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

/// Resolved gradient stop — distinct from ode_format::style::GradientStop which uses StyleValue<Color>.
/// This type contains the final resolved Color after token resolution.
pub struct ResolvedGradientStop {
    pub position: f32,
    pub color: Color,
}
```

### ResolvedEffect

```rust
pub enum ResolvedEffect {
    DropShadow {
        color: Color,
        offset_x: f32,
        offset_y: f32,
        blur_radius: f32,
        spread: f32,
    },
    InnerShadow {
        color: Color,
        offset_x: f32,
        offset_y: f32,
        blur_radius: f32,
        spread: f32,
    },
    LayerBlur {
        radius: f32,
    },
    BackgroundBlur {
        radius: f32,
    },
}
```

---

## Document → Scene Conversion

### Traversal Algorithm

```
for each canvas root in doc.canvas:
    convert_node(doc, root_id, &mut commands)

fn convert_node(doc, node_id, parent_transform, commands):
    node = doc.nodes[node_id]

    // Compute accumulated transform: parent × node.transform
    current_transform = parent_transform * node.transform.to_tiny_skia()

    1. PushLayer(node.opacity, node.blend_mode, clip_from_frame_if_applicable, current_transform)

    2. If node.kind.visual() is Some(visual):   // Group/Instance return None — skip fills/strokes
       a. For each effect that renders BEHIND content (DropShadow):
          → ApplyEffect
       b. For each fill in visual.fills (if visible):
          → resolve paint (StyleValue → ResolvedPaint via token system)
          → FillPath(node_path, resolved_paint, fill_rule, current_transform)
       c. For each stroke in visual.strokes (if visible):
          → resolve paint
          → StrokePath(node_path, resolved_paint, stroke_style, current_transform)
       d. For each effect that renders ON content (InnerShadow, LayerBlur, BackgroundBlur):
          → ApplyEffect

    3. For each child in node.children():
       → convert_node(doc, child_id, current_transform, commands)  // recurse

    4. PopLayer

    // Multiple effects of same type: processed in Vec order (first in vec = rendered first)
```

### Node-to-path mapping

| NodeKind | Path generation |
|----------|----------------|
| Frame | Rectangle from (0, 0, width, height) with corner_radius → rounded rect BezPath |
| Vector | VectorPath → BezPath conversion |
| BooleanOp | Collect child paths → apply boolean operation (union/subtract/intersect/exclude) → single BezPath |
| Group | No path — just PushLayer/PopLayer for opacity/blend, then recurse children |

### Token resolution during conversion

When converting `StyleValue<T>`:
- `StyleValue::Raw(v)` → use `v` directly
- `StyleValue::Bound { resolved, .. }` → use `resolved` (pre-resolved cached value)

This is intentionally simple for v0.1. Future versions may re-resolve from the token system for live theme switching.

---

## Renderer (Scene → Pixmap)

### Renderer

```rust
pub struct Renderer;

impl Renderer {
    /// Render a Scene to a new Pixmap.
    pub fn render(scene: &Scene) -> Result<tiny_skia::Pixmap, RenderError>;
}
```

Stateless — creates a Pixmap internally based on scene dimensions.

### Command execution

Iterates `scene.commands` sequentially:

**Note:** tiny-skia 0.11 has no save/restore state stack and no clip_path() method. Layer compositing
and clipping are implemented manually using temporary Pixmap buffers, `draw_pixmap()`, and `Mask` objects.

| Command | tiny-skia approach |
|---------|-------------------|
| `PushLayer` | Create a new temporary `Pixmap`. All subsequent commands render into this temp buffer. Build a `Mask` from the clip path (if present) for use during compositing. |
| `PopLayer` | Composite the temp `Pixmap` onto the parent `Pixmap` using `draw_pixmap()` with a `PixmapPaint { opacity, blend_mode, quality: FilterQuality::Nearest }` (Nearest is correct — no scaling during layer compositing). Apply the `Mask` (if any) during compositing. Then discard the temp buffer. |
| `FillPath` | Convert BezPath → `tiny_skia::Path`, convert paint → `tiny_skia::Paint`, call `pixmap.fill_path(path, &paint, fill_rule, transform, mask)` |
| `StrokePath` | Same conversion, call `pixmap.stroke_path(path, &paint, &stroke, transform, mask)` |
| `ApplyEffect` | Multi-pass: render to temp pixmap, apply blur, composite with `draw_pixmap()` |

The renderer maintains a stack of `(Pixmap, Option<Mask>, PixmapPaint)` tuples. `PushLayer` pushes a new
entry, `PopLayer` pops it and composites onto the previous entry.

### Paint conversion (ResolvedPaint → tiny_skia::Paint)

| ResolvedPaint | tiny-skia approach |
|---------------|-------------------|
| Solid | `tiny_skia::Paint` with `shader = Shader::SolidColor` |
| LinearGradient | `tiny_skia::LinearGradient::new()` |
| RadialGradient | `tiny_skia::RadialGradient::new(center, center, radius_x, stops, spread_mode, Transform::from_scale(1.0, radius_y/radius_x))` — two-point conical with `(start=center, end=center, radius=radius_x)`. Elliptical via scale transform on the Y axis. |
| AngularGradient | **Manual implementation** — see [AngularGradient details](#angulargradient-implementation) below |
| DiamondGradient | **Manual implementation** — see [DiamondGradient details](#diamondgradient-implementation) below |

#### AngularGradient implementation

tiny-skia has no native conic/sweep gradient. Implemented as a per-pixel shader pixmap.

**Algorithm:**
1. Allocate a temporary `Pixmap` matching the node's bounding box
2. For each pixel `(px, py)`, compute angle: `θ = atan2(py - center.y, px - center.x) + start_angle`
3. Normalize `θ` to `[0, 1)` range, sample color from gradient stops via linear interpolation
4. Write RGBA to the shader pixmap
5. Use `Paint { shader: Pattern::new(shader_pixmap, ...) }` to fill/stroke the path

**Performance:**
- **Time complexity**: O(W × H) per gradient — one `atan2` + interpolation per pixel
- **Benchmark reference**: 512×512 region ≈ 262K pixels, `atan2` ≈ 20ns/call → ~5ms per gradient fill
- For typical icon/illustration sizes (≤ 1024×1024), overhead is negligible
- **No caching**: shader pixmap is regenerated per render call (stateless renderer). Caching is a future optimization if profiling shows need

**Edge handling:**
- Seam at 360°→0° wrap: adjacent pixels may span the discontinuity. Apply 1px linear interpolation across the seam to prevent aliasing artifacts
- Stop positions outside `[0, 1]` are clamped

#### DiamondGradient implementation

Diamond gradient uses Manhattan distance (L1 norm) instead of Euclidean distance (L2, radial).

**Algorithm:**
1. Allocate a temporary `Pixmap` matching the node's bounding box
2. For each pixel `(px, py)`, compute normalized distance: `d = (|px - center.x| / radius.x) + (|py - center.y| / radius.y)`
3. Clamp `d` to `[0, 1]`, sample color from gradient stops
4. Write RGBA to the shader pixmap
5. Use as `Pattern` shader, same as AngularGradient

**Performance:**
- **Time complexity**: O(W × H) per gradient — simpler math than Angular (no trig), ~2× faster per pixel
- Abs + division + addition vs. `atan2`: ≈ 3ns vs. 20ns per pixel
- 512×512 region ≈ ~0.8ms per gradient fill

**Resolution:**
- Both AngularGradient and DiamondGradient shader pixmaps render at **1:1 pixel ratio** — no resolution loss
- Shader pixmap dimensions match the node's bounding box in device pixels, so output quality equals the final render resolution

### BlendMode mapping

All 16 blend modes map 1:1 to tiny-skia:

```rust
fn to_skia_blend(mode: BlendMode) -> tiny_skia::BlendMode {
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
```

### StrokeStyle → tiny_skia::Stroke mapping

```rust
fn to_skia_stroke(style: &StrokeStyle) -> tiny_skia::Stroke {
    let mut stroke = tiny_skia::Stroke {
        width: style.width,
        line_cap: match style.cap {
            StrokeCap::Butt => tiny_skia::LineCap::Butt,
            StrokeCap::Round => tiny_skia::LineCap::Round,
            StrokeCap::Square => tiny_skia::LineCap::Square,
        },
        line_join: match style.join {
            StrokeJoin::Miter => tiny_skia::LineJoin::Miter,
            StrokeJoin::Round => tiny_skia::LineJoin::Round,
            StrokeJoin::Bevel => tiny_skia::LineJoin::Bevel,
        },
        miter_limit: style.miter_limit,
        dash: None,
    };
    if let Some(ref dash) = style.dash {
        stroke.dash = tiny_skia::StrokeDash::new(dash.array.clone(), dash.offset);
    }
    stroke
}
```

Note: `StrokePosition` (Inside/Outside/Center) has no tiny-skia equivalent — it is a higher-level concept handled via Mask (see below).

### Stroke position handling

| StrokePosition | Approach |
|----------------|----------|
| Center | Default — stroke directly on path |
| Inside | Build `Mask` from path, stroke with width × 2 using mask to constrain to interior |
| Outside | Build inverted `Mask` from path, stroke with width × 2 using mask to constrain to exterior |

### Effect rendering

**DropShadow:**
1. Clone the content path, translate by (offset_x, offset_y)
2. If spread > 0, expand path outward
3. Render shadow-colored fill to temporary pixmap
4. Apply Gaussian blur with blur_radius
5. Composite temp pixmap UNDER main content

**InnerShadow:**
1. Invert the content path, translate by offset
2. Clip to original path boundary
3. Render shadow-colored fill + Gaussian blur
4. Composite OVER main content

**LayerBlur:**
1. Render current layer to temporary pixmap
2. Apply Gaussian blur
3. Composite blurred result to parent

**BackgroundBlur:**

BackgroundBlur는 디자인 툴의 표준 기능이다 (Figma의 "Background blur", CSS `backdrop-filter: blur()`).
현재 노드 *뒤에* 이미 렌더링된 모든 콘텐츠를 블러 처리하는 것이 핵심이다.

구현 단계:
1. **배경 캡처**: 현재 레이어 스택에서 부모 pixmap의 현재 상태를 스냅샷한다.
   이 시점에서 부모 pixmap에는 현재 노드보다 먼저 렌더링된 모든 형제/조상 콘텐츠가 포함되어 있다.
2. **영역 클리핑**: 현재 노드의 경계(path 또는 frame rect)를 Mask로 만들어 스냅샷에서 해당 영역만 추출한다.
3. **Gaussian blur 적용**: 추출된 영역에 blur_radius만큼 3-pass box blur를 적용한다.
4. **합성**: 블러된 배경을 현재 노드의 레이어에 `SourceOver`로 먼저 그린 뒤, 노드 자신의 fills/strokes를 그 위에 렌더링한다.

렌더러 구조적 요구사항:
- `PushLayer` 시점에 부모 pixmap에 대한 **읽기 접근**이 필요하다. 레이어 스택이 이미 `Vec<(Pixmap, ...)>` 구조이므로, 스택의 이전 항목을 참조하면 된다.
- BackgroundBlur가 있는 노드는 반드시 자체 레이어(`PushLayer`)를 가져야 한다 — 이는 이미 모든 노드에 대해 보장되어 있다.

성능 특성:
- **추가 pixmap 복사 1회**: 부모 pixmap → 클리핑된 영역 복사. O(W × H) 여기서 W, H는 노드 바운딩 박스 크기.
- **blur 비용**: 3-pass box blur, 각 pass O(W × H). 총 O(3 × W × H).
- 전체 추가 비용: blur가 있는 노드당 pixmap 할당 1회 + 복사 1회 + blur 3 pass.
- 아이콘/일러스트 규모에서는 병목이 아님. 전체 캔버스 크기의 BackgroundBlur가 여러 개 중첩될 경우에만 성능 영향 가능.

**Gaussian blur implementation:**
- tiny-skia has no built-in blur
- Implement as 3-pass box blur (box blur approximation of Gaussian)
- O(n) per pass regardless of radius — efficient for large radii
- Separate horizontal and vertical passes (separable filter)

---

## Path Operations (ode-core)

### VectorPath ↔ BezPath conversion

```rust
/// Convert serializable VectorPath to kurbo BezPath for rendering
pub fn to_bezpath(path: &VectorPath) -> kurbo::BezPath;

/// Convert kurbo BezPath back to serializable VectorPath
pub fn from_bezpath(path: &kurbo::BezPath) -> VectorPath;
```

### Boolean operations

```rust
/// Apply a boolean operation to two paths
pub fn boolean_op(
    a: &kurbo::BezPath,
    b: &kurbo::BezPath,
    op: BooleanOperation,
) -> Result<kurbo::BezPath, RenderError>;
```

**Note:** Neither kurbo 0.11 nor lyon 1.0 provides boolean path operations.
Add `i_overlay` crate to workspace dependencies for path boolean operations (union, subtract, intersect, exclude).
The `i_overlay` crate provides `overlay()` with `OverlayRule` variants matching our `BooleanOperation` enum.

For BooleanOpData with multiple children:
1. Start with first child's path
2. Sequentially apply the boolean operation with each subsequent child
3. Result is a single BezPath rendered with the BooleanOpData's visual props

---

## Export (ode-export)

### PngExporter

```rust
pub struct PngExporter;

impl PngExporter {
    /// Save a Pixmap to a PNG file
    pub fn export(pixmap: &tiny_skia::Pixmap, path: &std::path::Path) -> Result<(), ExportError>;

    /// Encode a Pixmap to PNG bytes in memory
    pub fn export_bytes(pixmap: &tiny_skia::Pixmap) -> Result<Vec<u8>, ExportError>;
}
```

Uses `tiny_skia::Pixmap::encode_png()` internally.

---

## Integration API

End-to-end usage:

```rust
use ode_format::Document;
use ode_core::{Scene, Renderer};
use ode_export::PngExporter;

// 1. Build or load a document
let doc: Document = /* ... */;

// 2. Convert to scene
let scene = Scene::from_document(&doc)?;

// 3. Render to pixels
let pixmap = Renderer::render(&scene)?;

// 4. Export to PNG
PngExporter::export(&pixmap, Path::new("output.png"))?;
```

---

## File Structure

```
crates/ode-format/src/
  ├── node.rs           (MODIFY — FrameData size/corner_radius, VectorData path/fill_rule,
  │                       VectorPath, PathSegment, FillRule, constructor changes)
  └── lib.rs            (MODIFY — re-export new types)

crates/ode-core/src/
  ├── lib.rs            — module declarations, re-exports
  ├── scene.rs          — Scene, RenderCommand, ResolvedPaint, ResolvedEffect, StrokeStyle, ResolvedGradientStop
  ├── convert.rs        — Scene::from_document(), node traversal, token resolution
  ├── render.rs         — Renderer::render(), command execution loop
  ├── paint.rs          — ResolvedPaint → tiny_skia::Paint, custom gradient implementations
  ├── effects.rs        — Shadow/blur rendering, Gaussian blur (box blur approximation)
  ├── blend.rs          — BlendMode → tiny_skia::BlendMode mapping
  ├── path.rs           — VectorPath ↔ BezPath, BooleanOp execution, frame-to-rounded-rect
  └── error.rs          — RenderError, ConvertError

crates/ode-export/src/
  ├── lib.rs            — module declarations, re-exports
  ├── png.rs            — PngExporter
  └── error.rs          — ExportError
```

---

## Error Types

```rust
// ode-core::error
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

// ode-export::error
#[derive(Debug, Error)]
pub enum ExportError {
    #[error("PNG encoding failed: {0}")]
    PngEncodeFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

---

## Testing Strategy

| Layer | Test Type | What to Verify |
|-------|-----------|---------------|
| `ode-format` changes | Unit | VectorPath serde roundtrip, FrameData with size roundtrip, FillRule default |
| `path.rs` | Unit | VectorPath ↔ BezPath conversion correctness, boolean operations |
| `convert.rs` | Unit | Document → Scene produces correct command count/order, token resolution |
| `paint.rs` | Unit | ResolvedPaint → tiny_skia paint conversion, gradient stop interpolation |
| `blend.rs` | Unit | All 16 BlendMode mappings |
| `effects.rs` | Unit | Gaussian blur kernel correctness, shadow offset/spread |
| `render.rs` | Integration | Scene → Pixmap, verify specific pixel colors (solid fill, gradient sample points) |
| `png.rs` | Integration | Pixmap → PNG bytes → decode → pixel comparison |
| **End-to-end** | Integration | Document → Scene → Pixmap → PNG → decode → verify pixels |

---

## Key Dependencies

| Crate | Purpose | Already in workspace |
|-------|---------|---------------------|
| kurbo | Path/curve math, BezPath | Yes |
| tiny-skia | CPU rasterization, Pixmap, Mask | Yes |
| i_overlay | Boolean path operations (union, subtract, intersect, exclude) | **No — add to workspace** |
| image | PNG decode for test verification (dev-dependency only) | Yes |
| thiserror | Error types | Yes |

Note: `lyon` (already in workspace) is available for tessellation but is not used for boolean ops.
The `image` crate is only needed as a `[dev-dependency]` in `ode-export` for test verification.
`PngExporter` uses `tiny_skia::Pixmap::encode_png()` directly.
