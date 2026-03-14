# Figma Import Design Spec

## Overview

Figma REST API JSON 응답을 ODE Document로 변환하는 `ode-import` 크레이트를 구현한다. 단일 패스 DFS 변환 방식으로, Figma의 모든 주요 노드 타입, 스타일, Auto Layout, 컴포넌트/인스턴스, 이미지, Variables를 ODE 포맷으로 매핑한다.

## Architecture

### Crate Structure

```
crates/ode-import/
├── Cargo.toml
└── src/
    ├── lib.rs              # pub mod figma, error
    ├── error.rs            # ImportError, ImportWarning
    └── figma/
        ├── mod.rs           # pub use
        ├── types.rs         # Figma REST API serde 구조체
        ├── client.rs        # Figma API HTTP 클라이언트
        └── convert.rs       # FigmaFile → ODE Document 변환
```

### Dependencies

```toml
[dependencies]
ode-format = { path = "../ode-format" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["json"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
nanoid = "0.4"
```

### Data Flow

```
Figma REST API                    ODE
─────────────                    ───
GET /v1/files/:key ──┐
                     ├──→ FigmaFile (serde) ──→ Converter::convert() ──→ Document
GET /v1/files/:key/  │         │
  variables/local ───┘         │
                               ▼
GET /v1/images/:key ────→ Image bytes ──→ ImageSource::Embedded
```

## Figma Type Definitions (`types.rs`)

Figma REST API 응답에 대응하는 Rust 타입을 정의한다. 모든 필드는 `Option`으로 감싸서 API 응답의 불완전함에 대응한다.

### Top-Level Response

```rust
pub struct FigmaFileResponse {
    pub name: String,
    pub document: FigmaNode,
    pub components: HashMap<String, FigmaComponentMeta>,
    pub component_sets: HashMap<String, FigmaComponentSetMeta>,
    pub schema_version: u32,
    pub styles: HashMap<String, FigmaStyleMeta>,
}

pub struct FigmaComponentMeta {
    pub key: String,
    pub name: String,
    pub description: String,
    pub component_set_id: Option<String>,
    pub documentation_links: Option<Vec<FigmaDocLink>>,
}

pub struct FigmaDocLink {
    pub uri: String,
}

pub struct FigmaComponentSetMeta {
    pub key: String,
    pub name: String,
    pub description: String,
}

pub struct FigmaStyleMeta {
    pub key: String,
    pub name: String,
    pub style_type: String,   // FILL | TEXT | EFFECT | GRID
    pub description: String,
}
```

### Node Types

노드 타입은 `node_type` 문자열 필드로 식별하고 match로 분기한다.

```rust
pub struct FigmaNode {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,             // DOCUMENT | CANVAS | FRAME | GROUP | ...
    pub visible: Option<bool>,         // default true
    pub children: Option<Vec<FigmaNode>>,

    // Visual properties
    pub fills: Option<Vec<FigmaPaint>>,
    pub strokes: Option<Vec<FigmaPaint>>,
    pub stroke_weight: Option<f32>,
    pub stroke_align: Option<String>,  // INSIDE | OUTSIDE | CENTER
    pub stroke_cap: Option<String>,    // NONE | ROUND | SQUARE | LINE_ARROW | ...
    pub stroke_join: Option<String>,   // MITER | BEVEL | ROUND
    pub stroke_dashes: Option<Vec<f32>>,
    pub stroke_miter_angle: Option<f32>, // angle threshold (NOT miter_limit ratio)
    pub effects: Option<Vec<FigmaEffect>>,
    pub opacity: Option<f32>,          // default 1.0
    pub blend_mode: Option<String>,
    pub is_mask: Option<bool>,         // ODE에 mask 개념 없음 — 경고 후 무시
    pub corner_radius: Option<f32>,
    pub rectangle_corner_radii: Option<[f32; 4]>,

    // Geometry
    pub absolute_bounding_box: Option<FigmaRect>,
    pub relative_transform: Option<[[f64; 3]; 2]>,
    pub size: Option<FigmaVector>,
    pub clips_content: Option<bool>,   // ODE FrameData에 clips_content 필드 추가 필요

    // Layout constraints
    pub constraints: Option<FigmaLayoutConstraint>,

    // Auto Layout
    pub layout_mode: Option<String>,           // NONE | HORIZONTAL | VERTICAL
    pub layout_sizing_horizontal: Option<String>, // FIXED | HUG | FILL
    pub layout_sizing_vertical: Option<String>,
    pub layout_wrap: Option<String>,           // NO_WRAP | WRAP
    pub primary_axis_align_items: Option<String>, // MIN | CENTER | MAX | SPACE_BETWEEN
    pub counter_axis_align_items: Option<String>, // MIN | CENTER | MAX | BASELINE
    pub padding_left: Option<f32>,
    pub padding_right: Option<f32>,
    pub padding_top: Option<f32>,
    pub padding_bottom: Option<f32>,
    pub item_spacing: Option<f32>,
    pub counter_axis_spacing: Option<f32>,     // wrap 시 행/열 간격 (ODE에 미구현, 경고)
    pub layout_align: Option<String>,          // INHERIT | STRETCH | MIN | CENTER | MAX
    pub layout_positioning: Option<String>,     // AUTO | ABSOLUTE
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,

    // Text
    pub characters: Option<String>,
    pub style: Option<FigmaTypeStyle>,
    pub character_style_overrides: Option<Vec<usize>>,
    pub style_override_table: Option<HashMap<String, FigmaTypeStyle>>,

    // Component / Instance
    pub component_id: Option<String>,
    pub component_properties: Option<HashMap<String, FigmaComponentProperty>>,
    pub overrides: Option<Vec<FigmaOverride>>,

    // Boolean operation
    pub boolean_operation: Option<String>,  // UNION | INTERSECT | SUBTRACT | EXCLUDE

    // Path data
    pub fill_geometry: Option<Vec<FigmaPath>>,
    pub stroke_geometry: Option<Vec<FigmaPath>>,

    // Variable bindings
    pub bound_variables: Option<HashMap<String, FigmaVariableAlias>>,
}

/// Figma Instance override entry
pub struct FigmaOverride {
    pub id: String,
    pub overridden_fields: Vec<String>,
}

/// Figma component property (text, boolean, instance-swap, variant)
#[serde(tag = "type")]
pub enum FigmaComponentProperty {
    TEXT { value: String },
    BOOLEAN { value: bool },
    INSTANCE_SWAP { value: String },           // component ID
    VARIANT { value: String },                 // variant value
}
```

### Paint Types

```rust
pub struct FigmaPaint {
    #[serde(rename = "type")]
    pub paint_type: String,          // SOLID | GRADIENT_LINEAR | GRADIENT_RADIAL |
                                      // GRADIENT_ANGULAR | GRADIENT_DIAMOND | IMAGE
    pub visible: Option<bool>,        // default true
    pub opacity: Option<f32>,         // default 1.0
    pub color: Option<FigmaColor>,    // SOLID only
    pub blend_mode: Option<String>,
    pub gradient_handle_positions: Option<Vec<FigmaVector>>,
    pub gradient_stops: Option<Vec<FigmaColorStop>>,
    pub scale_mode: Option<String>,   // IMAGE: FILL | FIT | TILE | STRETCH
    pub image_ref: Option<String>,    // IMAGE: reference for GET /v1/images
    pub image_transform: Option<[[f64; 3]; 2]>,
    pub bound_variables: Option<HashMap<String, FigmaVariableAlias>>,
}

pub struct FigmaColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

pub struct FigmaColorStop {
    pub position: f32,
    pub color: FigmaColor,
    pub bound_variables: Option<HashMap<String, FigmaVariableAlias>>,
}
```

### Effect Types

```rust
pub struct FigmaEffect {
    #[serde(rename = "type")]
    pub effect_type: String,   // DROP_SHADOW | INNER_SHADOW | LAYER_BLUR | BACKGROUND_BLUR
    pub visible: Option<bool>,
    pub radius: Option<f32>,
    pub color: Option<FigmaColor>,
    pub offset: Option<FigmaVector>,
    pub spread: Option<f32>,
    pub blend_mode: Option<String>,
    pub bound_variables: Option<HashMap<String, FigmaVariableAlias>>,
}
```

### Text Style

```rust
pub struct FigmaTypeStyle {
    pub font_family: Option<String>,
    pub font_weight: Option<f32>,
    pub font_size: Option<f32>,
    pub text_align_horizontal: Option<String>,  // LEFT | RIGHT | CENTER | JUSTIFIED
    pub text_align_vertical: Option<String>,    // TOP | CENTER | BOTTOM
    pub letter_spacing: Option<f32>,
    pub line_height_px: Option<f32>,
    pub line_height_percent_font_size: Option<f32>,
    pub line_height_unit: Option<String>,       // PIXELS | FONT_SIZE_% | INTRINSIC_%
    pub text_decoration: Option<String>,        // NONE | UNDERLINE | STRIKETHROUGH
    pub text_case: Option<String>,              // ORIGINAL | UPPER | LOWER | TITLE
    pub text_auto_resize: Option<String>,       // NONE | HEIGHT | WIDTH_AND_HEIGHT
    pub paragraph_spacing: Option<f32>,
    pub paragraph_indent: Option<f32>,
    pub fills: Option<Vec<FigmaPaint>>,
    pub opentype_flags: Option<HashMap<String, u32>>,
    pub italic: Option<bool>,
    pub bound_variables: Option<HashMap<String, FigmaVariableAlias>>,
}
```

### Variables

```rust
pub struct FigmaVariablesResponse {
    pub status: u32,
    pub error: Option<bool>,
    pub meta: FigmaVariablesMeta,
}

pub struct FigmaVariablesMeta {
    pub variable_collections: HashMap<String, FigmaVariableCollection>,
    pub variables: HashMap<String, FigmaVariable>,
}

pub struct FigmaVariableCollection {
    pub id: String,
    pub name: String,
    pub modes: Vec<FigmaVariableMode>,  // API에서도 [{modeId, name}] 배열로 옴
    pub default_mode_id: String,
    pub variable_ids: Vec<String>,
    pub remote: bool,
    pub hidden_from_publishing: bool,
}

pub struct FigmaVariableMode {
    pub mode_id: String,
    pub name: String,
}

pub struct FigmaVariable {
    pub id: String,
    pub name: String,
    pub variable_collection_id: String,
    pub resolved_type: String,            // BOOLEAN | FLOAT | STRING | COLOR
    pub values_by_mode: HashMap<String, serde_json::Value>,  // mode_id → value
    pub description: String,
    pub hidden_from_publishing: bool,
    pub scopes: Vec<String>,
    pub code_syntax: Option<HashMap<String, String>>,
}

pub struct FigmaVariableAlias {
    #[serde(rename = "type")]
    pub alias_type: String,   // "VARIABLE_ALIAS"
    pub id: String,
}
```

### Geometry

```rust
pub struct FigmaRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub struct FigmaVector {
    pub x: f64,
    pub y: f64,
}

pub struct FigmaPath {
    pub path: String,              // SVG path string
    pub winding_rule: Option<String>,  // NONZERO | EVENODD
    pub overridden_fields: Option<Vec<String>>,
}

pub struct FigmaLayoutConstraint {
    pub vertical: String,   // TOP | BOTTOM | CENTER | TOP_BOTTOM | SCALE
    pub horizontal: String, // LEFT | RIGHT | CENTER | LEFT_RIGHT | SCALE
}
```

## Conversion Logic (`convert.rs`)

### Node Type Mapping

| Figma Type | ODE NodeKind | Notes |
|---|---|---|
| `DOCUMENT` | — | Top-level, children become canvas roots |
| `CANVAS` | — | Each canvas's children become `Document.canvas` entries |
| `FRAME` | `Frame` | Direct mapping |
| `GROUP` | `Group` | No visual props |
| `SECTION` | `Frame` | Treated as Frame (container with fills) |
| `VECTOR` | `Vector` | SVG path → VectorPath segments |
| `RECTANGLE` | `Vector` | Converted to rect path with corner_radius |
| `ELLIPSE` | `Vector` | Converted to ellipse path |
| `LINE` | `Vector` | Converted to line path |
| `STAR` | `Vector` | Converted to star path using fillGeometry |
| `REGULAR_POLYGON` | `Vector` | Converted to polygon path using fillGeometry |
| `BOOLEAN_OPERATION` | `BooleanOp` | Maps op type directly |
| `TEXT` | `Text` | Characters + style runs |
| `COMPONENT` | `Frame` | Frame with `component_def` set |
| `COMPONENT_SET` | `Frame` | Frame (variant container) |
| `INSTANCE` | `Instance` | References `source_component` by StableId |
| `SLICE` | — | Skipped (export-only artifact) |
| `TABLE` | `Frame` | Auto-layout frame with cell children |
| `TABLE_CELL` | `Frame` | Frame with text content |
| FigJam types | — | Skipped with warning (STICKY, CONNECTOR, SHAPE_WITH_TEXT) |

**참고:** Figma에는 `IMAGE` 노드 타입이 없다. 이미지는 Frame/Rectangle 등의 `fills`에 `type: "IMAGE"` Paint로 적용된다. 유일한 fill이 IMAGE paint인 Frame/Rectangle은 ODE `Image` 노드로 변환을 시도하고, 그 외에는 해당 fill을 `Paint::ImageFill`로 변환한다.

### Paint Mapping

| Figma Paint | ODE Paint | Notes |
|---|---|---|
| `SOLID` | `Paint::Solid` | `FigmaColor` → `Color::Srgb` |
| `GRADIENT_LINEAR` | `Paint::LinearGradient` | Handle positions → start/end points |
| `GRADIENT_RADIAL` | `Paint::RadialGradient` | Handle positions → center/radius |
| `GRADIENT_ANGULAR` | `Paint::AngularGradient` | Handle positions → center/angle |
| `GRADIENT_DIAMOND` | `Paint::DiamondGradient` | Handle positions → center/radius |
| `IMAGE` | `Paint::ImageFill` | `scaleMode` → `ImageFillMode` (STRETCH→Fill+warn), download via API |
| `EMOJI` | — | Skipped |
| `VIDEO` | — | Skipped with warning |
| `PATTERN` | — | Skipped with warning |

Figma `gradientHandlePositions` (3 점): 첫 번째 = 시작점, 두 번째 = 끝점, 세 번째 = 폭 방향. Linear에서는 `[0] → start`, `[1] → end`. Radial/Angular/Diamond에서는 `[0] → center`, `[1]`과 `[2]`로 radius/angle 계산.

### Effect Mapping

| Figma Effect | ODE Effect | Notes |
|---|---|---|
| `DROP_SHADOW` | `Effect::DropShadow` | Figma `radius` → ODE `blur`, `offset`/`color`/`spread` direct |
| `INNER_SHADOW` | `Effect::InnerShadow` | Figma `radius` → ODE `blur`, same as above |
| `LAYER_BLUR` | `Effect::LayerBlur` | `radius` → `radius` |
| `BACKGROUND_BLUR` | `Effect::BackgroundBlur` | `radius` → `radius` |
| `TEXTURE` | — | Skipped (beta feature) |
| `NOISE` | — | Skipped (beta feature) |

### BlendMode Mapping

| Figma | ODE |
|---|---|
| `PASS_THROUGH` | `Normal` |
| `NORMAL` | `Normal` |
| `MULTIPLY` | `Multiply` |
| `SCREEN` | `Screen` |
| `OVERLAY` | `Overlay` |
| `DARKEN` | `Darken` |
| `LIGHTEN` | `Lighten` |
| `COLOR_DODGE` | `ColorDodge` |
| `COLOR_BURN` | `ColorBurn` |
| `HARD_LIGHT` | `HardLight` |
| `SOFT_LIGHT` | `SoftLight` |
| `DIFFERENCE` | `Difference` |
| `EXCLUSION` | `Exclusion` |
| `HUE` | `Hue` |
| `SATURATION` | `Saturation` |
| `COLOR` | `Color` |
| `LUMINOSITY` | `Luminosity` |
| `LINEAR_BURN` | `Normal` (unsupported, warn) |
| `LINEAR_DODGE` | `Normal` (unsupported, warn) |

### Auto Layout Mapping

| Figma Property | ODE Property | Mapping |
|---|---|---|
| `layoutMode: "HORIZONTAL"` | `LayoutConfig.direction: Horizontal` | Direct |
| `layoutMode: "VERTICAL"` | `LayoutConfig.direction: Vertical` | Direct |
| `primaryAxisAlignItems: "MIN"` | `primary_axis_align: Start` | |
| `primaryAxisAlignItems: "CENTER"` | `primary_axis_align: Center` | |
| `primaryAxisAlignItems: "MAX"` | `primary_axis_align: End` | |
| `primaryAxisAlignItems: "SPACE_BETWEEN"` | `primary_axis_align: SpaceBetween` | |
| `counterAxisAlignItems: "MIN"` | `counter_axis_align: Start` | |
| `counterAxisAlignItems: "CENTER"` | `counter_axis_align: Center` | |
| `counterAxisAlignItems: "MAX"` | `counter_axis_align: End` | |
| `counterAxisAlignItems: "BASELINE"` | `counter_axis_align: Baseline` | |
| `counterAxisAlignItems: "STRETCH"` | `counter_axis_align: Stretch` | ODE에 Stretch 존재 |
| `paddingLeft/Right/Top/Bottom` | `LayoutPadding` | Direct |
| `itemSpacing` | `item_spacing` | Direct |
| `layoutWrap: "WRAP"` | `wrap: Wrap` | Direct |
| `layoutSizingHorizontal: "FIXED"` | `width: SizingMode::Fixed` | Per-child |
| `layoutSizingHorizontal: "HUG"` | `width: SizingMode::Hug` | Per-child |
| `layoutSizingHorizontal: "FILL"` | `width: SizingMode::Fill` | Per-child |
| `minWidth/maxWidth/minHeight/maxHeight` | Same fields on `LayoutSizing` | Direct |
| `layoutAlign: "STRETCH"` | `LayoutSizing.align_self: Stretch` | Per-child cross-axis align |
| `layoutPositioning: "ABSOLUTE"` | — | 절대 위치 자식: auto-layout sizing 미적용, transform 사용 |
| `counterAxisSpacing` | — | ODE LayoutConfig에 미구현, wrap 시 행/열 간격 (경고) |

### Constraint Mapping

| Figma Constraint | ODE ConstraintAxis |
|---|---|
| `TOP` / `LEFT` | `Fixed` |
| `BOTTOM` / `RIGHT` | `Fixed` |
| `CENTER` | `Center` |
| `SCALE` | `Scale` |
| `TOP_BOTTOM` / `LEFT_RIGHT` | `Stretch` |

### Text Conversion

Figma의 텍스트 스타일 시스템은 ODE와 구조가 다르므로 변환이 필요하다:

1. **기본 스타일**: `FigmaTypeStyle` → `TextStyle` (노드의 `style` 필드)
2. **스타일 런 변환**: Figma는 `characterStyleOverrides` (문자별 스타일 인덱스 배열) + `styleOverrideTable` (인덱스 → 스타일 맵)을 사용. ODE는 `TextRun` (바이트 범위 + 스타일)을 사용.

변환 알고리즘:
```
1. characterStyleOverrides에서 연속된 같은 스타일 인덱스를 그룹화
2. 각 그룹을 TextRun으로 변환:
   - **주의**: Figma의 characterStyleOverrides는 JavaScript 문자열 인덱스
     (UTF-16 code unit) 기반이다. 이모지 등 BMP 밖 문자는 surrogate pair로
     2개 인덱스를 차지한다. 따라서 UTF-16 인덱스 → UTF-8 바이트 오프셋 변환이 필요.
   - styleOverrideTable에서 스타일 조회
   - FigmaTypeStyle → TextRunStyle (부분 오버라이드)
```

| Figma TypeStyle | ODE TextStyle | Notes |
|---|---|---|
| `fontFamily` | `font_family` | Direct |
| `fontWeight` | `font_weight` | `f32.round() as u16` (반올림) |
| `fontSize` | `font_size` | Direct |
| `textAlignHorizontal` | `text_align` | LEFT→Left, CENTER→Center, RIGHT→Right, JUSTIFIED→Justify |
| `textAlignVertical` | `vertical_align` | TOP→Top, CENTER→Middle, BOTTOM→Bottom |
| `letterSpacing` | `letter_spacing` | Direct |
| `lineHeightUnit: PIXELS` | `LineHeight::Fixed` | Use `lineHeightPx` |
| `lineHeightUnit: FONT_SIZE_%` | `LineHeight::Percent` | Use `lineHeightPercentFontSize / 100` |
| `lineHeightUnit: INTRINSIC_%` | `LineHeight::Auto` | |
| `textDecoration: UNDERLINE` | `TextDecoration::Underline` | |
| `textDecoration: STRIKETHROUGH` | `TextDecoration::Strikethrough` | |
| `textCase: UPPER` | `TextTransform::Uppercase` | |
| `textCase: LOWER` | `TextTransform::Lowercase` | |
| `textCase: TITLE` | `TextTransform::Capitalize` | |
| `textAutoResize: NONE` | `TextSizingMode::Fixed` | |
| `textAutoResize: HEIGHT` | `TextSizingMode::AutoHeight` | |
| `textAutoResize: WIDTH_AND_HEIGHT` | `TextSizingMode::AutoWidth` | |
| `paragraphSpacing` | `paragraph_spacing` | Direct |
| `openTypeFlags` | `opentype_features` | key → tag bytes, value → enabled |
| `fills` | TextRunStyle fills | Per-run fill override |
| `italic` | — | ODE TextStyle에 italic 필드 없음. 경고 후 무시 |
| `paragraphIndent` | — | ODE TextStyle에 paragraph_indent 없음. 경고 후 무시 |

### Component / Instance Conversion

**Components:**
- Figma `COMPONENT` 노드 → ODE `Frame` with `component_def: Some(ComponentDef { name, description })`
- 컴포넌트 메타데이터는 `FigmaFileResponse.components` 맵에서 가져옴
- `stable_id`를 생성하고, `component_id → stable_id` 매핑 테이블 유지

**Instances:**
- Figma `INSTANCE` 노드 → ODE `Instance`
- `componentId` → `component_id → stable_id` 매핑으로 `source_component` 설정
- Figma `overrides` → ODE `Override` 변환:
  - Figma override는 `{id, overriddenFields}` 구조
  - `overriddenFields`에 따라 해당 프로퍼티를 ODE Override 변형으로 변환
  - 대상 노드의 Figma ID → ODE StableId 매핑 필요

### Variables → DesignTokens Conversion

별도 API 호출(`GET /v1/files/:key/variables/local`)로 Variables 데이터를 가져온다.

| Figma | ODE | Notes |
|---|---|---|
| `VariableCollection` | `TokenCollection` | |
| `VariableCollection.modes` | `Vec<Mode>` | Figma string mode_id → ODE u32 ModeId (자동 증분 매핑) |
| `VariableCollection.defaultModeId` | `default_mode` | 같은 string→u32 매핑 적용 |
| `Variable` | `Token` | Figma string variable ID → ODE u32 TokenId (자동 증분 매핑) |
| `Variable.resolvedType: COLOR` | `TokenValue::Color` | FigmaColor → Color::Srgb |
| `Variable.resolvedType: FLOAT` | `TokenValue::Number` | |
| `Variable.resolvedType: STRING` | `TokenValue::String` | |
| `Variable.resolvedType: BOOLEAN` | `TokenValue::Number` | true→1.0, false→0.0 (의도적 lossy 변환, ODE에 Boolean 타입 없음) |
| `VariableAlias` | `TokenResolve::Alias` | Figma variable id → variable_map으로 (CollectionId, TokenId) 조회 |
| `Variable.valuesByMode` | `Token.values` | Figma string mode_id → ODE u32 ModeId 매핑 후 mode별 값 저장 |

**ID 매핑 전략:** Figma는 문자열 ID(`"1:0"`, `"VariableID:123"` 등)를 사용하고 ODE는 `u32`를 사용한다. 컨버터는 `mode_id_map: HashMap<String, ModeId>`와 `token_id_map: HashMap<String, TokenId>`를 유지하며, 등장 순서대로 0부터 자동 증분하여 할당한다.

**Variable Binding:**
노드의 `boundVariables` 필드에서 프로퍼티별 바인딩을 추출하여 `StyleValue::Bound { token, resolved }` 형태로 설정한다. `resolved` 값은 실제 노드에 적용된 값(fills의 color 등)을 사용한다.

### Image Download

1. 변환 중 `IMAGE` 타입 Paint의 `imageRef` 수집
2. 모든 imageRef를 모아서 `GET /v1/images/:key?ids=...&format=png` 배치 호출
3. 응답의 URL에서 이미지 바이트 다운로드
4. `ImageSource::Embedded { data }` 로 저장

JSON 파일 입력 모드에서는 이미지 다운로드를 위해 별도로 API 토큰이 필요하다. 토큰이 없으면 `ImageSource::Linked { path: imageRef }` 로 대체하고 경고.

### SVG Path Parsing

Figma의 `fillGeometry`와 `strokeGeometry`는 SVG path 문자열(`"M 0 0 L 100 0 L 100 100 Z"`)을 포함한다. 이를 ODE의 `VectorPath` 세그먼트로 파싱:

```
M x y        → PathSegment::MoveTo { x, y }
L x y        → PathSegment::LineTo { x, y }
H x          → PathSegment::LineTo { x, current_y }     (horizontal line)
V y          → PathSegment::LineTo { current_x, y }     (vertical line)
Q x1 y1 x y → PathSegment::QuadTo { x1, y1, x, y }
T x y        → PathSegment::QuadTo (reflected control)  (smooth quad)
C x1 y1 x2 y2 x y → PathSegment::CurveTo { x1, y1, x2, y2, x, y }
S x2 y2 x y → PathSegment::CurveTo (reflected cp1)     (smooth cubic)
A rx ry ... x y → cubic bezier 근사 변환               (arc to cubics)
Z            → PathSegment::Close
```

상대 좌표 명령어(`m`, `l`, `h`, `v`, `q`, `t`, `c`, `s`, `a`, `z`)도 지원하며, 절대 좌표로 변환한다.

**Multiple fillGeometry paths:** Figma의 `fillGeometry`는 `Vec<FigmaPath>`이지만 ODE `VectorData`는 단일 `VectorPath`를 가진다. 복수 서브패스는 모든 세그먼트를 하나의 `VectorPath.segments`에 순서대로 연결한다 (각 서브패스는 자체 MoveTo로 시작). `fill_rule`은 첫 번째 FigmaPath의 `winding_rule`을 사용한다.

### Stroke Conversion Details

**StrokeCap 매핑:**
- Figma `NONE` → ODE `Butt` (SVG default)
- Figma `ROUND` → ODE `Round`
- Figma `SQUARE` → ODE `Square`
- Figma `LINE_ARROW`, `TRIANGLE_ARROW` 등 화살표 캡 → ODE `Butt` + 경고 (ODE에 화살표 캡 없음)

**MiterAngle → MiterLimit 변환:**
Figma는 `strokeMiterAngle` (각도 임계값, 기본 28.96°)을 사용하고, ODE는 SVG 스타일의 `miter_limit` (비율)을 사용한다:
```rust
fn convert_miter(miter_angle_deg: f32) -> f32 {
    let half_rad = (miter_angle_deg / 2.0).to_radians();
    if half_rad.sin().abs() < f32::EPSILON {
        f32::MAX
    } else {
        1.0 / half_rad.sin()
    }
}
```

### Transform Mapping

Figma의 `relativeTransform`은 `[[a, c, tx], [b, d, ty]]` 형식의 2x3 행렬이다.

```rust
// Figma [[a, c, tx], [b, d, ty]] → ODE Transform { a, b, c, d, tx, ty }
fn convert_transform(ft: [[f64; 3]; 2]) -> Transform {
    Transform {
        a: ft[0][0] as f32,
        b: ft[1][0] as f32,
        c: ft[0][1] as f32,
        d: ft[1][1] as f32,
        tx: ft[0][2] as f32,
        ty: ft[1][2] as f32,
    }
}
```

## Converter Structure

```rust
pub struct FigmaConverter {
    /// Figma component ID → ODE StableId mapping
    component_map: HashMap<String, StableId>,
    /// Figma node ID → ODE StableId mapping (for overrides)
    node_id_map: HashMap<String, StableId>,
    /// Figma variable ID → (CollectionId, TokenId) mapping
    variable_map: HashMap<String, (CollectionId, TokenId)>,
    /// Collected image references for batch download
    image_refs: Vec<String>,
    /// Import warnings (non-fatal)
    warnings: Vec<ImportWarning>,
}

impl FigmaConverter {
    pub fn convert(
        file_response: FigmaFileResponse,
        variables: Option<FigmaVariablesResponse>,
        images: HashMap<String, Vec<u8>>,
    ) -> Result<ImportResult, ImportError>;
}

pub struct ImportResult {
    pub document: Document,
    pub warnings: Vec<ImportWarning>,
}
```

### Conversion Algorithm

```
1. Pre-pass: DOCUMENT → CANVAS 순회
   - COMPONENT 노드의 Figma ID 수집 → component_map에 StableId 사전 생성
   - 모든 노드의 ID → StableId 매핑 생성

2. Variables 변환 (있으면):
   - VariableCollection → TokenCollection
   - Variable → Token (mode별 값 매핑)
   - variable_map 구축 (variable ID → token reference)

3. Main DFS traversal:
   - 각 노드 타입에 맞는 convert_* 함수 호출
   - children 재귀 처리
   - boundVariables → StyleValue::Bound 바인딩

4. Post-pass:
   - 이미지 바이트 주입 (image_refs → ImageSource::Embedded)
   - INSTANCE의 source_component 연결 검증
```

## API Client (`client.rs`)

```rust
pub struct FigmaClient {
    token: String,
    client: reqwest::Client,
}

impl FigmaClient {
    pub fn new(token: String) -> Self;

    /// GET /v1/files/:file_key
    pub async fn get_file(&self, file_key: &str) -> Result<FigmaFileResponse, ImportError>;

    /// GET /v1/files/:file_key/variables/local
    pub async fn get_variables(&self, file_key: &str) -> Result<FigmaVariablesResponse, ImportError>;

    /// GET /v1/images/:file_key?ids=...&format=png
    /// Returns image_ref → image bytes mapping
    pub async fn get_images(
        &self,
        file_key: &str,
        image_refs: &[String],
    ) -> Result<HashMap<String, Vec<u8>>, ImportError>;
}
```

## CLI Integration

`ode-cli`에 `import` 서브커맨드를 추가한다:

```
# API에서 직접 가져오기
ode import figma --token <FIGMA_TOKEN> --file-key <FILE_KEY> --output design.ode.json

# 이미 다운로드된 JSON 파일에서 변환
ode import figma --input response.json --output design.ode.json

# Variables 포함
ode import figma --token <TOKEN> --file-key <KEY> --output design.ode.json --with-variables

# 이미지 다운로드 생략
ode import figma --token <TOKEN> --file-key <KEY> --output design.ode.json --skip-images
```

CLI 옵션:
- `--token` / `-t`: Figma Personal Access Token (또는 `FIGMA_TOKEN` 환경변수)
- `--file-key` / `-k`: Figma 파일 키
- `--input` / `-i`: 로컬 JSON 파일 경로 (API 호출 대신)
- `--output` / `-o`: 출력 .ode.json 파일 경로
- `--with-variables`: Variables API도 호출하여 DesignTokens 변환
- `--skip-images`: 이미지 다운로드 생략 (Linked로 대체)

## Error Handling

```rust
#[derive(Debug, thiserror::Error)]
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

#[derive(Debug)]
pub struct ImportWarning {
    pub node_id: String,
    pub node_name: String,
    pub message: String,
}
```

경고가 발생하는 경우:
- 지원하지 않는 노드 타입 (FigJam STICKY, CONNECTOR 등)
- 지원하지 않는 Paint 타입 (VIDEO, PATTERN, EMOJI)
- 지원하지 않는 Effect 타입 (TEXTURE, NOISE)
- 지원하지 않는 BlendMode (LINEAR_BURN, LINEAR_DODGE)
- 이미지 다운로드 실패 (개별 이미지)
- 컴포넌트 참조를 찾을 수 없는 Instance
- Variables API 접근 불가 (Enterprise 미가입)

## Testing Strategy

1. **Unit tests**: 각 변환 함수 (paint, effect, text style, layout 등) 단위 테스트
2. **Fixture-based tests**: 실제 Figma API 응답 JSON 스냅샷을 `tests/fixtures/` 에 저장, 전체 변환 후 결과 검증
3. **Round-trip test**: Figma JSON → ODE Document → .ode.json 저장 → 다시 로드 → 내용 일치 확인
4. **Warning tests**: 미지원 요소가 포함된 JSON으로 경고 발생 확인

## Required ode-format Changes

임포트 구현에 앞서 `ode-format`에 필요한 변경사항:

1. **`Node`에 `visible: bool` 필드 추가** (default `true`) — Figma의 `visible` 속성 매핑
2. **`FrameData`에 `clips_content: bool` 필드 추가** (default `true`) — Figma의 `clipsContent` 매핑
3. **wire.rs 직렬화에 위 필드 반영**

## Opacity Handling

Figma의 opacity는 3개 레벨로 분리 유지:
- **노드 레벨**: `node.opacity` → `Node.opacity`
- **Paint 레벨**: `paint.opacity` → `Fill.opacity` / `Stroke.opacity`
- **색상 알파**: `color.a` → `Color::Srgb.a`

각 레벨을 곱하지 않고 분리 보존한다.

## Scope Exclusions

- Figma 프로토타이핑 (interactions, transitions) — ODE에 해당 개념 없음
- Figma 댓글, 버전 히스토리 — 메타데이터, 디자인 데이터 아님
- FigJam 전용 노드 (Sticky, Connector, ShapeWithText) — 경고 후 스킵
- Grid Layout (`layoutMode: "GRID"`) — ODE에 grid 레이아웃 미구현, Frame으로 대체 후 경고
- Figma Dev Mode 관련 속성 — ODE에 해당 없음
- `.fig` 바이너리 파싱 — 비공개 포맷, 범위 외
- Mask (`isMask`) — ODE에 mask 개념 미구현, 경고 후 일반 노드로 변환
- `counterAxisSpacing` (wrap spacing) — ODE LayoutConfig에 미구현, 경고
- `italic` 속성 — ODE TextStyle에 italic 필드 없음
- `paragraphIndent` — ODE TextStyle에 미구현
