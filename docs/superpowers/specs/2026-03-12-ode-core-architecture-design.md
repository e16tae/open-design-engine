# Open Design Engine — Core Architecture Design

> "You don't have to know design."

## Overview

Open Design Engine (ODE) is an open-source design engine written in Rust. It provides:

1. **A universal design file format** (`.ode`) — an open alternative to Figma/Sketch
2. **An AI-driven design generation pipeline** — input (prompt/spec) to output (PNG/SVG/PDF)

Future extensions include a design tool UI and import from existing formats (Figma, Penpot, Sketch).

### Supported Design Formats (Priority Order)

| Priority | Format | Key Capability Added |
|----------|--------|---------------------|
| 1 | Icons / Illustrations | Vector primitives, paths, boolean ops, groups |
| 2 | Social Media (banners, thumbnails) | Fixed canvas + image placement + text rendering |
| 3 | Print (business cards, posters) | CMYK, spot colors, bleed/trim, PDF export, unit system |
| 4 | Presentations | Multi-page document model, page ordering |
| 5 | Web Design | Responsive layout (flexbox/grid), component system, states |
| 6 | App Design | Screen navigation flows, device frames, platform conventions |

Each priority builds on the capabilities established by the previous ones.

---

## Workspace Structure

```
crates/
  ode-format/   — Document format: parsing, serialization, document model
  ode-core/     — Rendering pipeline, layout, vector operations
  ode-export/   — SVG, PNG, PDF output
  ode-cli/      — CLI tool (binary: `ode`)
  ode-mcp/      — MCP server for AI agent interface
```

### Dependency Flow

```
ode-format  (data model)
    ^
ode-core    (rendering/layout, depends on format)
    ^
ode-export  (output, depends on format + core)
    ^
ode-cli     (CLI, depends on format + core + export)
ode-mcp     (MCP, depends on format + core + export)
```

---

## Node Architecture

### Design Principle: 3-Layer Hybrid

| Layer | Purpose | Implementation |
|-------|---------|---------------|
| Layer 1: Arena Storage | Performance | `SlotMap<NodeId, Node>` — O(1) access, cache-friendly |
| Layer 2: Composable Properties | Extensibility | `VisualProps`, `ContainerProps` — attach/detach freely |
| Layer 3: Semantic Node Types | Quality/Safety | `NodeKind` enum — compile-time invalid state prevention |

### Node

```rust
type NodeId = slotmap::DefaultKey;
type NodeTree = SlotMap<NodeId, Node>;

/// Stable, serialization-safe identifier.
/// NodeId is a runtime arena key (not stable across save/load).
/// StableId persists in files; a StableId <-> NodeId mapping table
/// is built on load and maintained at runtime.
type StableId = String; // nanoid

struct Node {
    id: NodeId,
    stable_id: StableId,
    name: String,
    transform: Transform,
    opacity: f32,
    blend_mode: BlendMode,
    constraints: Option<Constraints>,
    kind: NodeKind,
}
```

Only universally applicable properties live on `Node`. Everything else is inside `NodeKind`.

**Serialization:** All cross-node references (children, View targets, token refs) use `StableId`
in the file format. On load, a `HashMap<StableId, NodeId>` resolves them back to arena keys.

### NodeKind

```rust
enum NodeKind {
    Frame(Box<FrameData>),
    Group(Box<GroupData>),
    Vector(Box<VectorData>),
    BooleanOp(Box<BooleanOpData>),
    Text(Box<TextData>),
    Image(Box<ImageData>),
    Instance(Box<InstanceData>),
}
```

All variants are `Box`ed to keep enum size uniform (~8 bytes + discriminant).

### Kind-Specific Data

```rust
struct FrameData {
    visual: VisualProps,
    container: ContainerProps,
    component_def: Option<ComponentDef>,  // Frame doubles as Component when set
}

struct GroupData {
    children: Vec<NodeId>,
    // No visual properties — transparent container
}

struct VectorData {
    visual: VisualProps,
    paths: Vec<Path>,
}

struct BooleanOpData {
    visual: VisualProps,
    op: BooleanOperation,  // Union, Subtract, Intersect, Exclude
    children: Vec<NodeId>,
}

struct TextData {
    visual: VisualProps,
    content: TextContent,
}

struct ImageData {
    visual: VisualProps,
    source: ImageSource,
}

struct InstanceData {
    container: ContainerProps,
    source: ComponentId,
    overrides: Vec<Override>,
}
```

### Composable Property Structs

```rust
struct VisualProps {
    fills: Vec<Fill>,
    strokes: Vec<Stroke>,
    effects: Vec<Effect>,
}

struct ContainerProps {
    children: Vec<NodeId>,
    layout: Option<LayoutConfig>,
}
```

### Type Safety Guarantees

- `Group` cannot have fills/strokes — no `VisualProps` field
- `Vector` cannot have layout/children — no `ContainerProps` field
- `Component` is `Frame` with `component_def: Some(...)` — no duplication
- `Instance` has children (via `ContainerProps`) for override rendering

### Common Accessor

```rust
impl NodeKind {
    fn visual(&self) -> Option<&VisualProps> {
        match self {
            Self::Frame(d) => Some(&d.visual),
            Self::Vector(d) => Some(&d.visual),
            Self::Text(d) => Some(&d.visual),
            Self::Image(d) => Some(&d.visual),
            Self::BooleanOp(d) => Some(&d.visual),
            Self::Group(_) | Self::Instance(_) => None,
        }
    }
}
```

---

## Document / Canvas / View Model

### Design Principle: Separation of Design and Presentation

- **Canvas** — the single source of truth for design elements
- **View** — a format-specific "lens" that references canvas elements and adds output metadata
- **Tokens** — shared design system variables

```rust
/// Root-level frame reference on the infinite canvas.
/// Wraps a NodeId pointing to a top-level Frame node.
type CanvasRoot = NodeId;

struct Document {
    format_version: Version,       // e.g., "0.1.0" — file format version
    name: String,                  // Human-readable document name
    nodes: NodeTree,
    canvas: Vec<CanvasRoot>,       // top-level frames on the infinite canvas
    tokens: DesignTokens,
    views: Vec<View>,
    working_color_space: WorkingColorSpace,
}
```

ID types (`ViewId`, `CollectionId`, `ModeId`, `TokenId`) follow the `NodeId` pattern
(arena keys or newtype wrappers over unique identifiers).

### View System

```rust
struct View {
    id: ViewId,
    name: String,
    kind: ViewKind,
}

enum ViewKind {
    Print {
        pages: Vec<PageDef>,           // ordered Frame references
        color_profile: IccProfile,
        bleed: Margins,
    },
    Web {
        root: NodeId,
        breakpoints: Vec<Breakpoint>,
    },
    Presentation {
        slides: Vec<SlideDef>,         // ordered Frame references
        transitions: Vec<Transition>,
    },
    Export {
        targets: Vec<ExportTarget>,    // simple image/SVG export
    },
}
```

### Why Canvas + View

| Concern | Location | Example |
|---------|----------|---------|
| Design elements (what exists) | Canvas / NodeTree | Shapes, text, images, layout |
| Format interpretation (how to output) | View | Page order, bleed, breakpoints, transitions |
| Shared rules | Tokens | Colors, typography, spacing |

Benefits:
- No duplication: one logo used by PrintView and WebView references the same node
- Clean separation: design without thinking about format, configure output via View
- AI workflow: AI creates on canvas, "export as business card" creates a PrintView
- Multi-format: same Frame exported as CMYK PDF and sRGB PNG without metadata conflicts

### Working Color Space

```rust
enum WorkingColorSpace {
    Srgb,          // Web/social media default
    DisplayP3,     // Apple ecosystem
    AdobeRgb,      // Photography/illustration
    ProPhotoRgb,   // Wide gamut work
}
```

All colors are converted to the working space for blending/compositing, then converted to the output space on export.

---

## Color System

### Color Enum

```rust
pub enum Color {
    Srgb {
        r: f32, g: f32, b: f32,
        a: f32,
    },
    DisplayP3 {
        r: f32, g: f32, b: f32,
        a: f32,
    },
    Cmyk {
        c: f32, m: f32, y: f32, k: f32,
        a: f32,
    },
    Oklch {
        l: f32, c: f32, h: f32,
        a: f32,
    },
    Lab {
        l: f32, a_axis: f32, b_axis: f32,
        a: f32,
    },
    Icc {
        profile: String,
        channels: Vec<f32>,
        a: f32,
    },
    Spot {
        name: String,              // "Pantone 2728 C", "Gold Foil"
        fallback_rgb: [f32; 3],    // Screen approximation (RGB only)
        a: f32,                    // Alpha — consistent with all other variants
    },
}
```

### Design Decisions

- **All variants have alpha (`a: f32`)** — enables compositing regardless of color space
- **Spot color uses fallback** — physical inks cannot be exactly represented digitally; `fallback_rgb` provides screen preview, name is used for print matching. Alpha is a separate `a` field, consistent with all other variants.
- **Lab uses `a_axis`** — avoids confusion with alpha field `a`
- **Display P3 is explicit** — too common on modern devices to route through ICC every time

### Color Conversion Matrix

| From \ To | sRGB | P3 | CMYK | Oklch | Lab | Spot |
|-----------|------|----|------|-------|-----|------|
| sRGB | - | lossless | lossy (ICC) | lossless | lossless | impossible |
| P3 | gamut clip | - | lossy (ICC) | lossless | lossless | impossible |
| CMYK | lossy (ICC) | lossy (ICC) | - | lossy | lossy | impossible |
| Oklch | lossless | lossless | lossy (ICC) | - | lossless | impossible |
| Lab | lossless | lossless | lossy (ICC) | lossless | - | impossible |
| Spot | approx only | approx only | approx only | approx only | approx only | - |

Lossy conversions use lcms2 with ICC profiles and gamut mapping. This is the industry standard approach used by Photoshop, Illustrator, and all professional print tools.

### Rendering Pipeline Color Flow

```
Input (mixed color spaces)
  |
  v
[Convert to Working Color Space]  <-- lcms2
  |
  v
[Blend / Composite]               <-- unified space
  |
  v
[Convert to Output Color Space]
  +-- Screen: sRGB or P3
  +-- Web: sRGB
  +-- Print: CMYK (via ICC profile)
  +-- Original: preserve input space
```

---

## Style System

### StyleValue\<T\> — Token-Aware Property Values

Every style property that can be bound to a design token uses `StyleValue<T>`:

```rust
enum StyleValue<T> {
    /// Direct value — no token binding
    Raw(T),

    /// Token reference + cached resolved value
    Bound {
        token: TokenRef,
        resolved: T,     // Cache. Token is the source of truth.
    },
}
```

**Why this hybrid:**

| Criterion | StyleValue\<T\> |
|-----------|----------------|
| Type safety | Strong — Raw has no token, Bound always has both |
| Render performance | Fast — `resolved` is always available, no resolution step |
| Single source of truth | Yes — `token` is truth, `resolved` is cache |
| AI workflow | Natural — generate Raw values first, bind tokens later |
| Cross-document copy | Graceful — `resolved` survives even if token is missing |

**Application rule:** Only values that designers would share via design systems use `StyleValue<T>` (colors, dimensions, font families). Structural settings (blend mode, stroke cap) do not.

### Paint (Shared by Fill and Stroke)

```rust
enum Paint {
    Solid {
        color: StyleValue<Color>,
    },
    LinearGradient {
        stops: Vec<GradientStop>,
        start: Point,
        end: Point,
    },
    RadialGradient {
        stops: Vec<GradientStop>,
        center: Point,
        radius: Point,
    },
    AngularGradient {
        stops: Vec<GradientStop>,
        center: Point,
        angle: f32,
    },
    DiamondGradient {
        stops: Vec<GradientStop>,
        center: Point,
        radius: Point,
    },
    MeshGradient(Box<MeshGradientData>),
    ImageFill {
        source: ImageSource,
        mode: ImageFillMode,   // Fill, Fit, Crop, Tile
    },
}

struct GradientStop {
    position: f32,
    color: StyleValue<Color>,
}

struct MeshGradientData {
    rows: u32,
    columns: u32,
    points: Vec<MeshPoint>,
}

struct MeshPoint {
    position: Point,
    color: StyleValue<Color>,
}
```

### Fill

```rust
struct Fill {
    paint: Paint,
    opacity: StyleValue<f32>,
    blend_mode: BlendMode,
    visible: bool,
}
```

Multiple fills per node (`Vec<Fill>`), each with independent blend mode and opacity.

**Render order:** Vec index 0 is rendered first (bottom layer), last index is on top.
Fills are rendered before strokes. Effects are applied to the combined fill+stroke result.

### Stroke

```rust
struct Stroke {
    paint: Paint,
    width: StyleValue<f32>,
    position: StrokePosition,    // Inside, Outside, Center
    cap: StrokeCap,              // Butt, Round, Square
    join: StrokeJoin,            // Miter, Round, Bevel
    miter_limit: f32,
    dash: Option<DashPattern>,
    opacity: StyleValue<f32>,
    blend_mode: BlendMode,
    visible: bool,
}

struct DashPattern {
    segments: Vec<f32>,
    offset: f32,
}
```

### Effect

```rust
enum Effect {
    DropShadow {
        color: StyleValue<Color>,
        offset: Point,
        blur: StyleValue<f32>,
        spread: StyleValue<f32>,
    },
    InnerShadow {
        color: StyleValue<Color>,
        offset: Point,
        blur: StyleValue<f32>,
        spread: StyleValue<f32>,
    },
    LayerBlur {
        radius: StyleValue<f32>,
    },
    BackgroundBlur {
        radius: StyleValue<f32>,
    },
}
```

### Typography

```rust
struct TextStyle {
    font_family: StyleValue<FontFamily>,
    font_weight: StyleValue<FontWeight>,
    font_size: StyleValue<f32>,
    line_height: LineHeight,
    letter_spacing: StyleValue<f32>,
    paragraph_spacing: StyleValue<f32>,
    text_align: TextAlign,
    vertical_align: VerticalAlign,
    decoration: TextDecoration,
    transform: TextTransform,
    opentype_features: Vec<OpenTypeFeature>,
    variable_axes: Vec<VariableFontAxis>,
}

enum LineHeight {
    Auto,
    Fixed(StyleValue<f32>),
    Percent(StyleValue<f32>),
}

struct OpenTypeFeature {
    tag: [u8; 4],    // "liga", "kern", "smcp" — 4-byte OpenType tag
    enabled: bool,
}

struct VariableFontAxis {
    tag: [u8; 4],    // "wght", "wdth", "ital"
    value: StyleValue<f32>,
}
```

---

## Design Tokens System

### Design Principle: Flexible Hierarchy (W3C Aligned)

Single token type with free alias chains. Hierarchy (Primitive/Semantic/Component) is expressed through naming conventions, not enforced by types.

### Structure

```rust
struct DesignTokens {
    collections: Vec<TokenCollection>,
    active_modes: HashMap<CollectionId, ModeId>,
}

struct TokenCollection {
    id: CollectionId,
    name: String,
    modes: Vec<Mode>,
    default_mode: ModeId,
    tokens: Vec<Token>,
}

struct Mode {
    id: ModeId,
    name: String,
}

struct Token {
    id: TokenId,
    name: String,
    group: Option<String>,
    values: HashMap<ModeId, TokenResolve>,
}

enum TokenResolve {
    Direct(TokenValue),
    Alias(TokenRef),
}

enum TokenValue {
    Color(Color),
    Number(f32),
    Dimension(f32, DimensionUnit),
    FontFamily(FontFamily),
    FontWeight(FontWeight),
    Duration(f32),
    CubicBezier([f32; 4]),
    String(String),
}

/// TokenType is derived from TokenValue, not stored separately.
/// This eliminates the dual-enum sync problem — adding a new token
/// type only requires updating TokenValue.
impl TokenValue {
    fn token_type(&self) -> TokenType { /* match self → variant tag */ }
}

/// For Alias tokens, type is validated at registration time by
/// resolving the alias chain to its terminal Direct value and
/// checking token_type() compatibility.

struct TokenRef {
    collection_id: CollectionId,
    token_id: TokenId,
}
```

### Token Resolution

```
TokenCollection "Colors"
+-- modes: [Light, Dark]
+-- tokens:
    +-- "blue-500"
    |   +-- Light: Direct(#3b82f6)
    |   +-- Dark:  Direct(#60a5fa)
    +-- "color.primary"
    |   +-- Light: Alias -> "blue-500"  --> resolves to #3b82f6
    |   +-- Dark:  Alias -> "blue-500"  --> resolves to #60a5fa
    +-- "button.bg"
        +-- Light: Alias -> "color.primary" --> resolves to #3b82f6
        +-- Dark:  Alias -> "color.primary" --> resolves to #60a5fa
```

- Theme switch: change `active_modes` entry, propagate `resolved` cache in all `StyleValue::Bound`
- Cycle detection: DAG validation on token registration/modification
- Type safety: `TokenType` checked when binding `StyleValue<Color>` to a token
- Mode fallback: if active mode has no value for a token, fall back to `default_mode`. If `default_mode` also has no value, return `TokenError::MissingValue`

---

## Areas Deferred to Implementation Phase

These areas build on the foundation defined above and will be designed when their corresponding format priority is implemented.

### Core Types (minimal definition, detailed at implementation)

| Type | Minimal Definition | Designed When |
|------|-------------------|---------------|
| Transform | 2D affine matrix (6 floats: a, b, c, d, tx, ty) | Format 1 (icons) |
| BlendMode | Enum: Normal, Multiply, Screen, Overlay, ... (standard Porter-Duff + blend modes) | Format 1 (icons) |
| Constraints | Horizontal/Vertical: Fixed, Scale, Stretch, Center | Format 2 (social) |
| ImageSource | Enum: Embedded(bytes), Linked(path/url) | Format 2 (social) |
| ImageFillMode | Enum: Fill, Fit, Crop, Tile | Format 2 (social) |
| Point | Struct: { x: f32, y: f32 } | Format 1 (icons) |

### Feature Areas

| Area | Designed When | Depends On |
|------|--------------|------------|
| Layout system (LayoutConfig) | Formats 1-2 (icons, social) | taffy integration |
| Text content model (TextContent) | Format 2 (social media) | Font rendering pipeline |
| Vector path model (Path, BooleanOperation) | Format 1 (icons) | kurbo integration |
| Component/Instance system (ComponentDef, Override, ComponentId) | Format 5 (web design) | Component system maturity |
| Export pipeline details | Each format incrementally | ode-core rendering |
| MCP interface design | After export pipeline | ode-export stability |

### View Types (defined when their format is implemented)

| Type | Defined When |
|------|-------------|
| PageDef | Format 3 (print) |
| SlideDef | Format 4 (presentations) |
| Breakpoint | Format 5 (web design) |
| Transition | Format 4 (presentations) |
| ExportTarget | Format 1 (icons — first export) |
| IccProfile | Format 3 (print) |
| Margins | Format 3 (print) |

The foundation (Node, Style, Token, Document/Canvas/View, Color) is designed to support all these areas without structural changes.

---

## File Format Versioning & Migration

### Version Strategy

`Document.format_version` uses semantic versioning:

- **Patch** (0.1.0 → 0.1.1): New optional fields with `#[serde(default)]`. Fully backward compatible.
- **Minor** (0.1.x → 0.2.0): New enum variants, structural additions. Requires migration function.
- **Major** (0.x → 1.0): Breaking changes. Old files require explicit migration tool.

### Migration Pipeline

```
v0.1 file → migrate_0_1_to_0_2() → v0.2 file → migrate_0_2_to_0_3() → ... → current
```

Each version bump provides a migration function. Migrations are chained sequentially.
The engine always works with the current version internally.

### Serialization Rules

- **Node references** in files use `StableId` (nanoid), never `NodeId` (arena key)
- **On save:** `NodeId` → `StableId` for all cross-references (children, View targets)
- **On load:** Build `HashMap<StableId, NodeId>` mapping, resolve all references

---

## Cross-Collection Token References

Token aliases can reference tokens in other collections (`TokenRef` contains `collection_id`).

- **Allowed:** Alias in Collection "Components" → token in Collection "Colors"
- **Cycle detection** spans across collections (DAG validation is document-wide, not per-collection)
- **Mode resolution:** When an alias crosses collections, the target collection's active mode determines the resolved value

---

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| kurbo | Path/curve math |
| tiny-skia | CPU rendering |
| lyon | Tessellation |
| taffy | Layout engine (Flexbox/Grid) |
| krilla | PDF generation |
| lcms2 | Color management (ICC profiles, gamut mapping) |
| slotmap | Arena-based node storage |
| serde / serde_json | Serialization |
| nanoid | ID generation |
