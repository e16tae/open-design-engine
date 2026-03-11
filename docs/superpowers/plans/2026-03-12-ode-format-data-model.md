# ODE Format Data Model Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the complete `ode-format` data model — Color, Node, Style, Typography, Tokens, and Document — as specified in `docs/superpowers/specs/2026-03-12-ode-core-architecture-design.md`.

**Architecture:** The data model is a pure Rust library (`ode-format` crate) with zero rendering dependencies. All types derive `Serialize`/`Deserialize` for JSON persistence. The node tree uses `slotmap` for arena-based storage with `StableId` (nanoid) for serialization-safe references.

**Tech Stack:** Rust 2024 edition, serde/serde_json, slotmap, nanoid, thiserror

**Spec:** `docs/superpowers/specs/2026-03-12-ode-core-architecture-design.md`

---

## File Structure

```
crates/ode-format/
  Cargo.toml              — MODIFY: add slotmap dependency
  src/
    lib.rs                — MODIFY: update module declarations and re-exports
    color.rs              — MODIFY: update Color enum per spec (add DisplayP3, Spot, fix CMYK alpha, fix Lab fields)
    style.rs              — CREATE: StyleValue<T>, TokenRef, Paint, Fill, Stroke, Effect, VisualProps, BlendMode, CollectionId/TokenId type aliases
    typography.rs         — CREATE: TextStyle, OpenTypeFeature, VariableFontAxis, LineHeight, text enums
    node.rs               — CREATE: Node, NodeId, StableId, NodeKind, FrameData, GroupData, VectorData, ContainerProps, etc.
    tokens.rs             — MODIFY: DesignTokens, TokenCollection, Token, TokenValue, TokenRef, resolution + cycle detection
    document.rs           — MODIFY: Document, View, ViewKind, WorkingColorSpace, CanvasRoot, Version
```

Each file has one clear responsibility:
- `color.rs` — Color spaces and conversions
- `style.rs` — Visual properties (paint, fill, stroke, effect) + StyleValue generic + TokenRef + shared ID types
- `typography.rs` — Text-specific styling + FontFamily/FontWeight types
- `node.rs` — Node tree structure, kinds, and ContainerProps
- `tokens.rs` — Design token system with resolution
- `document.rs` — Top-level document, canvas, views

---

## Chunk 1: Foundation (Color + Style Primitives)

### Task 1: Workspace Setup — Add slotmap dependency

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/ode-format/Cargo.toml`

- [ ] **Step 1: Add slotmap to workspace dependencies**

In workspace root `Cargo.toml`, add to `[workspace.dependencies]`:
```toml
slotmap = "1"
```

In `crates/ode-format/Cargo.toml`, add to `[dependencies]`:
```toml
slotmap = { workspace = true }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p ode-format`
Expected: Compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml crates/ode-format/Cargo.toml
git commit -m "deps: add slotmap for arena-based node storage"
```

---

### Task 2: Update Color Enum

**Files:**
- Modify: `crates/ode-format/src/color.rs`

The existing Color enum needs: DisplayP3 variant, Spot variant, CMYK alpha, Lab field rename (`a` → `a_axis`), ICC alpha. All variants must have a consistent `a: f32` alpha field.

- [ ] **Step 1: Write failing tests for new variants**

Add to `color.rs` tests module:

```rust
#[test]
fn display_p3_roundtrip() {
    let color = Color::DisplayP3 { r: 1.0, g: 0.5, b: 0.0, a: 1.0 };
    let json = serde_json::to_string(&color).unwrap();
    let parsed: Color = serde_json::from_str(&json).unwrap();
    assert_eq!(color, parsed);
}

#[test]
fn cmyk_has_alpha() {
    let color = Color::Cmyk { c: 1.0, m: 0.0, y: 0.0, k: 0.0, a: 0.5 };
    assert!((color.alpha() - 0.5).abs() < f32::EPSILON);
}

#[test]
fn spot_color_roundtrip() {
    let color = Color::Spot {
        name: "Pantone 2728 C".to_string(),
        fallback_rgb: [0.0, 0.318, 0.729],
        a: 1.0,
    };
    let json = serde_json::to_string(&color).unwrap();
    let parsed: Color = serde_json::from_str(&json).unwrap();
    assert_eq!(color, parsed);
}

#[test]
fn alpha_consistent_across_all_variants() {
    let variants: Vec<Color> = vec![
        Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 0.5 },
        Color::DisplayP3 { r: 1.0, g: 0.0, b: 0.0, a: 0.5 },
        Color::Cmyk { c: 1.0, m: 0.0, y: 0.0, k: 0.0, a: 0.5 },
        Color::Oklch { l: 0.7, c: 0.15, h: 30.0, a: 0.5 },
        Color::Lab { l: 50.0, a_axis: 20.0, b_axis: -10.0, a: 0.5 },
        Color::Icc { profile: "sRGB".to_string(), channels: vec![1.0, 0.0, 0.0], a: 0.5 },
        Color::Spot { name: "Gold".to_string(), fallback_rgb: [0.8, 0.7, 0.2], a: 0.5 },
    ];
    for color in &variants {
        assert!((color.alpha() - 0.5).abs() < f32::EPSILON, "Failed for {:?}", color);
    }
}

#[test]
fn with_alpha_preserves_color() {
    let color = Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
    let transparent = color.with_alpha(0.3);
    assert!((transparent.alpha() - 0.3).abs() < f32::EPSILON);
    if let Color::Srgb { r, g, b, .. } = transparent {
        assert!((r - 1.0).abs() < f32::EPSILON);
        assert!((g - 0.0).abs() < f32::EPSILON);
        assert!((b - 0.0).abs() < f32::EPSILON);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-format -- color`
Expected: FAIL — `DisplayP3` not found, `alpha()` not found, etc.

- [ ] **Step 3: Update Color enum and implement helpers**

Replace the entire `color.rs` with:

```rust
use serde::{Deserialize, Serialize};

/// Color representation supporting multiple color spaces.
/// All variants carry an alpha channel (`a: f32`) for consistent compositing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "space", rename_all = "lowercase")]
pub enum Color {
    Srgb {
        r: f32,
        g: f32,
        b: f32,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    #[serde(rename = "display-p3")]
    DisplayP3 {
        r: f32,
        g: f32,
        b: f32,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    Cmyk {
        c: f32,
        m: f32,
        y: f32,
        k: f32,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    Oklch {
        l: f32,
        c: f32,
        h: f32,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    Lab {
        l: f32,
        a_axis: f32,
        b_axis: f32,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    Icc {
        profile: String,
        channels: Vec<f32>,
        #[serde(default = "default_alpha")]
        a: f32,
    },
    Spot {
        name: String,
        fallback_rgb: [f32; 3],
        #[serde(default = "default_alpha")]
        a: f32,
    },
}

fn default_alpha() -> f32 {
    1.0
}

impl Color {
    pub fn black() -> Self {
        Self::Srgb { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }
    }

    pub fn white() -> Self {
        Self::Srgb { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }
    }

    /// Returns the alpha value, consistent across all variants.
    pub fn alpha(&self) -> f32 {
        match self {
            Self::Srgb { a, .. }
            | Self::DisplayP3 { a, .. }
            | Self::Cmyk { a, .. }
            | Self::Oklch { a, .. }
            | Self::Lab { a, .. }
            | Self::Icc { a, .. }
            | Self::Spot { a, .. } => *a,
        }
    }

    /// Returns a new Color with the given alpha, preserving all other fields.
    pub fn with_alpha(&self, new_a: f32) -> Self {
        let mut cloned = self.clone();
        match &mut cloned {
            Self::Srgb { a, .. }
            | Self::DisplayP3 { a, .. }
            | Self::Cmyk { a, .. }
            | Self::Oklch { a, .. }
            | Self::Lab { a, .. }
            | Self::Icc { a, .. }
            | Self::Spot { a, .. } => *a = new_a,
        }
        cloned
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        let (r, g, b, a) = match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                (r, g, b, 255u8)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                (r, g, b, a)
            }
            _ => return None,
        };
        Some(Self::Srgb {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        })
    }

    pub fn to_rgba_u8(&self) -> [u8; 4] {
        match self {
            Self::Srgb { r, g, b, a } | Self::DisplayP3 { r, g, b, a } => [
                (r.clamp(0.0, 1.0) * 255.0) as u8,
                (g.clamp(0.0, 1.0) * 255.0) as u8,
                (b.clamp(0.0, 1.0) * 255.0) as u8,
                (a.clamp(0.0, 1.0) * 255.0) as u8,
            ],
            Self::Spot { fallback_rgb, a, .. } => [
                (fallback_rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                (fallback_rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                (fallback_rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
                (a.clamp(0.0, 1.0) * 255.0) as u8,
            ],
            // TODO: color space conversion via lcms2 for CMYK, Oklch, Lab, ICC
            _ => {
                let a = self.alpha();
                [0, 0, 0, (a.clamp(0.0, 1.0) * 255.0) as u8]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color() {
        let color = Color::from_hex("#3b82f6").unwrap();
        if let Color::Srgb { r, g, b, a } = color {
            assert!((r - 0.231).abs() < 0.01);
            assert!((g - 0.510).abs() < 0.01);
            assert!((b - 0.965).abs() < 0.01);
            assert!((a - 1.0).abs() < f32::EPSILON);
        } else {
            panic!("Expected Srgb color");
        }
    }

    #[test]
    fn serialize_roundtrip() {
        let color = Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
        let json = serde_json::to_string(&color).unwrap();
        let parsed: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(color, parsed);
    }

    #[test]
    fn display_p3_roundtrip() {
        let color = Color::DisplayP3 { r: 1.0, g: 0.5, b: 0.0, a: 1.0 };
        let json = serde_json::to_string(&color).unwrap();
        let parsed: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(color, parsed);
    }

    #[test]
    fn cmyk_has_alpha() {
        let color = Color::Cmyk { c: 1.0, m: 0.0, y: 0.0, k: 0.0, a: 0.5 };
        assert!((color.alpha() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn spot_color_roundtrip() {
        let color = Color::Spot {
            name: "Pantone 2728 C".to_string(),
            fallback_rgb: [0.0, 0.318, 0.729],
            a: 1.0,
        };
        let json = serde_json::to_string(&color).unwrap();
        let parsed: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(color, parsed);
    }

    #[test]
    fn alpha_consistent_across_all_variants() {
        let variants: Vec<Color> = vec![
            Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 0.5 },
            Color::DisplayP3 { r: 1.0, g: 0.0, b: 0.0, a: 0.5 },
            Color::Cmyk { c: 1.0, m: 0.0, y: 0.0, k: 0.0, a: 0.5 },
            Color::Oklch { l: 0.7, c: 0.15, h: 30.0, a: 0.5 },
            Color::Lab { l: 50.0, a_axis: 20.0, b_axis: -10.0, a: 0.5 },
            Color::Icc { profile: "sRGB".to_string(), channels: vec![1.0, 0.0, 0.0], a: 0.5 },
            Color::Spot { name: "Gold".to_string(), fallback_rgb: [0.8, 0.7, 0.2], a: 0.5 },
        ];
        for color in &variants {
            assert!((color.alpha() - 0.5).abs() < f32::EPSILON, "Failed for {:?}", color);
        }
    }

    #[test]
    fn with_alpha_preserves_color() {
        let color = Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
        let transparent = color.with_alpha(0.3);
        assert!((transparent.alpha() - 0.3).abs() < f32::EPSILON);
        if let Color::Srgb { r, g, b, .. } = transparent {
            assert!((r - 1.0).abs() < f32::EPSILON);
            assert!((g - 0.0).abs() < f32::EPSILON);
            assert!((b - 0.0).abs() < f32::EPSILON);
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ode-format -- color`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ode-format/src/color.rs
git commit -m "feat(color): update Color enum per spec — add DisplayP3, Spot, CMYK alpha, Lab field rename, consistent alpha()/with_alpha()"
```

---

### Task 3: Style Primitives — StyleValue, Paint, Fill, Stroke, Effect

**Files:**
- Create: `crates/ode-format/src/style.rs`
- Modify: `crates/ode-format/src/lib.rs`

- [ ] **Step 1: Write failing test for StyleValue**

Create `crates/ode-format/src/style.rs` starting with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn style_value_raw() {
        let val: StyleValue<f32> = StyleValue::Raw(1.0);
        assert!((val.value() - 1.0).abs() < f32::EPSILON);
        assert!(!val.is_bound());
    }

    #[test]
    fn style_value_bound() {
        use crate::style::{CollectionId, TokenId};
        let val: StyleValue<f32> = StyleValue::Bound {
            token: TokenRef { collection_id: 0 as CollectionId, token_id: 0 as TokenId },
            resolved: 42.0,
        };
        assert!((val.value() - 42.0).abs() < f32::EPSILON);
        assert!(val.is_bound());
    }

    #[test]
    fn style_value_roundtrip() {
        let val: StyleValue<f32> = StyleValue::Raw(3.14);
        let json = serde_json::to_string(&val).unwrap();
        let parsed: StyleValue<f32> = serde_json::from_str(&json).unwrap();
        assert_eq!(val, parsed);
    }

    #[test]
    fn fill_with_solid_paint() {
        let fill = Fill {
            paint: Paint::Solid { color: StyleValue::Raw(Color::black()) },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        };
        let json = serde_json::to_string(&fill).unwrap();
        let parsed: Fill = serde_json::from_str(&json).unwrap();
        assert_eq!(fill, parsed);
    }

    #[test]
    fn linear_gradient_roundtrip() {
        let paint = Paint::LinearGradient {
            stops: vec![
                GradientStop { position: 0.0, color: StyleValue::Raw(Color::black()) },
                GradientStop { position: 1.0, color: StyleValue::Raw(Color::white()) },
            ],
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 1.0, y: 1.0 },
        };
        let json = serde_json::to_string(&paint).unwrap();
        let parsed: Paint = serde_json::from_str(&json).unwrap();
        assert_eq!(paint, parsed);
    }

    #[test]
    fn stroke_roundtrip() {
        let stroke = Stroke {
            paint: Paint::Solid { color: StyleValue::Raw(Color::black()) },
            width: StyleValue::Raw(2.0),
            position: StrokePosition::Center,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            miter_limit: 4.0,
            dash: Some(DashPattern { segments: vec![5.0, 3.0], offset: 0.0 }),
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        };
        let json = serde_json::to_string(&stroke).unwrap();
        let parsed: Stroke = serde_json::from_str(&json).unwrap();
        assert_eq!(stroke, parsed);
    }

    #[test]
    fn drop_shadow_roundtrip() {
        let effect = Effect::DropShadow {
            color: StyleValue::Raw(Color::Srgb { r: 0.0, g: 0.0, b: 0.0, a: 0.25 }),
            offset: Point { x: 0.0, y: 4.0 },
            blur: StyleValue::Raw(8.0),
            spread: StyleValue::Raw(0.0),
        };
        let json = serde_json::to_string(&effect).unwrap();
        let parsed: Effect = serde_json::from_str(&json).unwrap();
        assert_eq!(effect, parsed);
    }

    #[test]
    fn visual_props_default_is_empty() {
        let vp = VisualProps::default();
        assert!(vp.fills.is_empty());
        assert!(vp.strokes.is_empty());
        assert!(vp.effects.is_empty());
    }
}
```

- [ ] **Step 2: Update lib.rs to declare style module**

Update `crates/ode-format/src/lib.rs`:
```rust
pub mod color;
pub mod style;
```

(Remove the non-existent modules for now — we'll add them back as we create the files.)

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ode-format -- style`
Expected: FAIL — types not defined.

- [ ] **Step 4: Implement style.rs**

Write the full `crates/ode-format/src/style.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::color::Color;

// ─── Token ID Types (shared with tokens module) ───

pub type CollectionId = u32;
pub type TokenId = u32;

// ─── Token Reference (forward declaration for StyleValue) ───

/// Reference to a design token. Used by both StyleValue::Bound and TokenResolve::Alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenRef {
    pub collection_id: CollectionId,
    pub token_id: TokenId,
}

// ─── StyleValue<T> ───

/// A style property value that may be bound to a design token.
/// When bound, `resolved` is a cache — `token` is the source of truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum StyleValue<T> {
    Raw(T),
    Bound { token: TokenRef, resolved: T },
}

impl<T: Clone> StyleValue<T> {
    /// Returns the effective value (resolved cache or raw).
    pub fn value(&self) -> T {
        match self {
            Self::Raw(v) => v.clone(),
            Self::Bound { resolved, .. } => resolved.clone(),
        }
    }

    /// Returns true if this value is bound to a token.
    pub fn is_bound(&self) -> bool {
        matches!(self, Self::Bound { .. })
    }
}

// ─── Geometry ───

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// ─── BlendMode ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Default for BlendMode {
    fn default() -> Self { Self::Normal }
}

// ─── Paint ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Paint {
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
        mode: ImageFillMode,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub position: f32,
    pub color: StyleValue<Color>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshGradientData {
    pub rows: u32,
    pub columns: u32,
    pub points: Vec<MeshPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshPoint {
    pub position: Point,
    pub color: StyleValue<Color>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ImageSource {
    Embedded { data: Vec<u8> },
    Linked { path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageFillMode {
    Fill,
    Fit,
    Crop,
    Tile,
}

// ─── Fill ───

/// A fill layer. Nodes can have multiple fills (`Vec<Fill>`).
/// Render order: index 0 is bottom, last index is top.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub paint: Paint,
    pub opacity: StyleValue<f32>,
    pub blend_mode: BlendMode,
    pub visible: bool,
}

// ─── Stroke ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub paint: Paint,
    pub width: StyleValue<f32>,
    pub position: StrokePosition,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f32,
    pub dash: Option<DashPattern>,
    pub opacity: StyleValue<f32>,
    pub blend_mode: BlendMode,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrokePosition { Inside, Outside, Center }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrokeCap { Butt, Round, Square }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrokeJoin { Miter, Round, Bevel }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashPattern {
    pub segments: Vec<f32>,
    pub offset: f32,
}

// ─── Effect ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Effect {
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

// ─── Composable Property Structs ───

/// Visual properties shared by nodes that render visually.
/// Render order: fills (bottom→top), then strokes (bottom→top).
/// Effects applied to combined result.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct VisualProps {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<Fill>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strokes: Vec<Stroke>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ode-format -- style`
Expected: All tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/src/style.rs crates/ode-format/src/lib.rs
git commit -m "feat(style): add StyleValue<T>, Paint, Fill, Stroke, Effect, VisualProps"
```

---

## Chunk 2: Typography + Node System

### Task 4: Typography

**Files:**
- Create: `crates/ode-format/src/typography.rs`
- Modify: `crates/ode-format/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/ode-format/src/typography.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::StyleValue;

    #[test]
    fn text_style_roundtrip() {
        let style = TextStyle::default();
        let json = serde_json::to_string(&style).unwrap();
        let parsed: TextStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(style, parsed);
    }

    #[test]
    fn opentype_feature_tag() {
        let feat = OpenTypeFeature { tag: *b"liga", enabled: true };
        assert_eq!(&feat.tag, b"liga");
    }

    #[test]
    fn variable_axis_roundtrip() {
        let axis = VariableFontAxis {
            tag: *b"wght",
            value: StyleValue::Raw(700.0),
        };
        let json = serde_json::to_string(&axis).unwrap();
        let parsed: VariableFontAxis = serde_json::from_str(&json).unwrap();
        assert_eq!(axis, parsed);
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Add to `crates/ode-format/src/lib.rs`:
```rust
pub mod typography;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ode-format -- typography`
Expected: FAIL — types not defined.

- [ ] **Step 4: Implement typography.rs**

```rust
use serde::{Deserialize, Serialize};

use crate::style::StyleValue;

/// Font family name. Newtype alias for spec alignment — allows future refinement.
pub type FontFamily = String;
/// Font weight (1–1000). Newtype alias for spec alignment.
pub type FontWeight = u16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font_family: StyleValue<FontFamily>,
    pub font_weight: StyleValue<FontWeight>,
    pub font_size: StyleValue<f32>,
    pub line_height: LineHeight,
    pub letter_spacing: StyleValue<f32>,
    pub paragraph_spacing: StyleValue<f32>,
    pub text_align: TextAlign,
    pub vertical_align: VerticalAlign,
    pub decoration: TextDecoration,
    pub transform: TextTransform,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opentype_features: Vec<OpenTypeFeature>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variable_axes: Vec<VariableFontAxis>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: StyleValue::Raw("Inter".into()),
            font_weight: StyleValue::Raw(400 as FontWeight),
            font_size: StyleValue::Raw(16.0),
            line_height: LineHeight::Auto,
            letter_spacing: StyleValue::Raw(0.0),
            paragraph_spacing: StyleValue::Raw(0.0),
            text_align: TextAlign::Left,
            vertical_align: VerticalAlign::Top,
            decoration: TextDecoration::None,
            transform: TextTransform::None,
            opentype_features: Vec::new(),
            variable_axes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum LineHeight {
    Auto,
    Fixed { value: StyleValue<f32> },
    Percent { value: StyleValue<f32> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextAlign { Left, Center, Right, Justify }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerticalAlign { Top, Middle, Bottom }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextDecoration { None, Underline, Strikethrough, Both }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextTransform { None, Uppercase, Lowercase, Capitalize }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenTypeFeature {
    pub tag: [u8; 4],
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableFontAxis {
    pub tag: [u8; 4],
    pub value: StyleValue<f32>,
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ode-format -- typography`
Expected: All tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/src/typography.rs crates/ode-format/src/lib.rs
git commit -m "feat(typography): add TextStyle, OpenTypeFeature, VariableFontAxis"
```

---

### Task 5: Node System

**Files:**
- Create: `crates/ode-format/src/node.rs`
- Modify: `crates/ode-format/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/ode-format/src/node.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::style::{StyleValue, Paint, Fill, BlendMode};

    #[test]
    fn create_frame_node() {
        let mut tree = NodeTree::new();
        let node = Node::new_frame("Header");
        let id = tree.insert(node);
        assert_eq!(tree[id].name, "Header");
        assert!(matches!(tree[id].kind, NodeKind::Frame(_)));
    }

    #[test]
    fn create_group_node() {
        let mut tree = NodeTree::new();
        let node = Node::new_group("Icons");
        let id = tree.insert(node);
        assert!(matches!(tree[id].kind, NodeKind::Group(_)));
    }

    #[test]
    fn frame_has_visual_props() {
        let node = Node::new_frame("Card");
        assert!(node.kind.visual().is_some());
    }

    #[test]
    fn group_has_no_visual_props() {
        let node = Node::new_group("Group");
        assert!(node.kind.visual().is_none());
    }

    #[test]
    fn frame_has_children() {
        let node = Node::new_frame("Parent");
        assert!(node.kind.children().is_some());
        assert!(node.kind.children().unwrap().is_empty());
    }

    #[test]
    fn vector_has_no_children() {
        let node = Node::new_vector("Path");
        assert!(node.kind.children().is_none());
    }

    #[test]
    fn stable_ids_are_unique() {
        let a = Node::new_frame("A");
        let b = Node::new_frame("B");
        assert_ne!(a.stable_id, b.stable_id);
    }

    #[test]
    fn node_kind_visual_accessor() {
        let mut node = Node::new_frame("Colored");
        if let NodeKind::Frame(ref mut data) = node.kind {
            data.visual.fills.push(Fill {
                paint: Paint::Solid { color: StyleValue::Raw(Color::black()) },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let visual = node.kind.visual().unwrap();
        assert_eq!(visual.fills.len(), 1);
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Add to `crates/ode-format/src/lib.rs`:
```rust
pub mod node;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ode-format -- node`
Expected: FAIL — types not defined.

- [ ] **Step 4: Implement node.rs**

```rust
use serde::{Deserialize, Serialize};
use slotmap::{new_key_type, SlotMap};

use crate::style::VisualProps;

// ─── IDs ───

new_key_type! {
    /// Runtime arena key. Not stable across save/load.
    pub struct NodeId;
}

/// Stable, serialization-safe identifier (nanoid).
pub type StableId = String;

/// Arena-based node storage.
pub type NodeTree = SlotMap<NodeId, Node>;

// ─── Transform ───

/// 2D affine transform matrix: [a, b, c, d, tx, ty]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 }
    }
}

// ─── Constraints ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintAxis { Fixed, Scale, Stretch, Center }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraints {
    pub horizontal: ConstraintAxis,
    pub vertical: ConstraintAxis,
}

// ─── ContainerProps ───

/// **Serialization note:** `Vec<NodeId>` round-trips correctly via slotmap's
/// Serialize impl. For the `.ode` file format (v0.2+), children will be
/// serialized as `Vec<StableId>` with a NodeId↔StableId mapping table
/// built on load (per spec). This v0.1 approach uses NodeId directly for
/// simplicity; the save/load pipeline in ode-core will handle the conversion.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ContainerProps {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeId>,
    pub layout: Option<LayoutConfig>,
}

/// Placeholder for layout configuration (designed when taffy is integrated).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutConfig {
    // Will be defined during Format 1-2 implementation
    _placeholder: (),
}

// ─── BooleanOperation ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BooleanOperation { Union, Subtract, Intersect, Exclude }

// ─── NodeKind ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NodeKind {
    Frame(Box<FrameData>),
    Group(Box<GroupData>),
    Vector(Box<VectorData>),
    BooleanOp(Box<BooleanOpData>),
    Text(Box<TextData>),
    Image(Box<ImageData>),
    Instance(Box<InstanceData>),
}

impl NodeKind {
    /// Access visual properties if this node kind has them.
    pub fn visual(&self) -> Option<&VisualProps> {
        match self {
            Self::Frame(d) => Some(&d.visual),
            Self::Vector(d) => Some(&d.visual),
            Self::BooleanOp(d) => Some(&d.visual),
            Self::Text(d) => Some(&d.visual),
            Self::Image(d) => Some(&d.visual),
            Self::Group(_) | Self::Instance(_) => None,
        }
    }

    /// Access visual properties mutably.
    pub fn visual_mut(&mut self) -> Option<&mut VisualProps> {
        match self {
            Self::Frame(d) => Some(&mut d.visual),
            Self::Vector(d) => Some(&mut d.visual),
            Self::BooleanOp(d) => Some(&mut d.visual),
            Self::Text(d) => Some(&mut d.visual),
            Self::Image(d) => Some(&mut d.visual),
            Self::Group(_) | Self::Instance(_) => None,
        }
    }

    /// Access children if this node kind has them.
    pub fn children(&self) -> Option<&[NodeId]> {
        match self {
            Self::Frame(d) => Some(&d.container.children),
            Self::Instance(d) => Some(&d.container.children),
            Self::Group(d) => Some(&d.children),
            Self::BooleanOp(d) => Some(&d.children),
            Self::Vector(_) | Self::Text(_) | Self::Image(_) => None,
        }
    }

    /// Access children mutably.
    pub fn children_mut(&mut self) -> Option<&mut Vec<NodeId>> {
        match self {
            Self::Frame(d) => Some(&mut d.container.children),
            Self::Instance(d) => Some(&mut d.container.children),
            Self::Group(d) => Some(&mut d.children),
            Self::BooleanOp(d) => Some(&mut d.children),
            Self::Vector(_) | Self::Text(_) | Self::Image(_) => None,
        }
    }
}

// ─── Kind-Specific Data ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameData {
    #[serde(default)]
    pub visual: VisualProps,
    #[serde(default)]
    pub container: ContainerProps,
    pub component_def: Option<ComponentDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorData {
    #[serde(default)]
    pub visual: VisualProps,
    // Path data deferred to Format 1 (kurbo integration)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BooleanOpData {
    #[serde(default)]
    pub visual: VisualProps,
    pub op: BooleanOperation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextData {
    #[serde(default)]
    pub visual: VisualProps,
    pub content: String, // Rich text model deferred to Format 2
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageData {
    #[serde(default)]
    pub visual: VisualProps,
    // ImageSource deferred to Format 2
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceData {
    #[serde(default)]
    pub container: ContainerProps,
    pub source_component: StableId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<serde_json::Value>, // Override model deferred to Format 5
}

/// Component definition (attached to a Frame).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentDef {
    pub name: String,
    pub description: String,
}

// ─── Node ───

use crate::style::BlendMode;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    #[serde(skip)]
    pub id: NodeId,
    pub stable_id: StableId,
    pub name: String,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub blend_mode: BlendMode,
    pub constraints: Option<Constraints>,
    pub kind: NodeKind,
}

fn default_opacity() -> f32 { 1.0 }

// Note: `impl Default for BlendMode` is in style.rs (where BlendMode is defined).

impl Node {
    pub fn new_frame(name: &str) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Frame(Box::new(FrameData {
                visual: VisualProps::default(),
                container: ContainerProps::default(),
                component_def: None,
            })),
        }
    }

    pub fn new_group(name: &str) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Group(Box::new(GroupData {
                children: Vec::new(),
            })),
        }
    }

    pub fn new_vector(name: &str) -> Self {
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
            })),
        }
    }

    pub fn new_text(name: &str, content: &str) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Text(Box::new(TextData {
                visual: VisualProps::default(),
                content: content.to_string(),
            })),
        }
    }

    pub fn new_boolean_op(name: &str, op: BooleanOperation) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::BooleanOp(Box::new(BooleanOpData {
                visual: VisualProps::default(),
                op,
                children: Vec::new(),
            })),
        }
    }

    pub fn new_image(name: &str) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Image(Box::new(ImageData {
                visual: VisualProps::default(),
            })),
        }
    }

    pub fn new_instance(name: &str, source_component: StableId) -> Self {
        Self {
            id: NodeId::default(),
            stable_id: nanoid::nanoid!(),
            name: name.to_string(),
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            constraints: None,
            kind: NodeKind::Instance(Box::new(InstanceData {
                container: ContainerProps::default(),
                source_component,
                overrides: Vec::new(),
            })),
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ode-format -- node`
Expected: All tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/src/node.rs crates/ode-format/src/lib.rs
git commit -m "feat(node): add Node, NodeKind, NodeTree with arena storage and type-safe accessors"
```

---

## Chunk 3: Token System + Document Model

### Task 6: Design Tokens System

**Files:**
- Create: `crates/ode-format/src/tokens.rs`
- Modify: `crates/ode-format/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/ode-format/src/tokens.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn make_simple_system() -> DesignTokens {
        let mut tokens = DesignTokens::new();
        let light = tokens.add_collection("Colors", vec!["Light", "Dark"]);
        tokens.add_token(light, "blue-500", TokenValue::Color(
            Color::Srgb { r: 0.231, g: 0.510, b: 0.965, a: 1.0 }
        ));
        tokens
    }

    #[test]
    fn resolve_direct_token() {
        let tokens = make_simple_system();
        let col_id = tokens.collections[0].id;
        let tok_id = tokens.collections[0].tokens[0].id;
        let resolved = tokens.resolve(col_id, tok_id).unwrap();
        assert!(matches!(resolved, TokenValue::Color(_)));
    }

    #[test]
    fn resolve_alias_token() {
        let mut tokens = make_simple_system();
        let col_id = tokens.collections[0].id;
        let blue_id = tokens.collections[0].tokens[0].id;
        tokens.add_alias_token(col_id, "color.primary", col_id, blue_id);

        let primary_id = tokens.collections[0].tokens[1].id;
        let resolved = tokens.resolve(col_id, primary_id).unwrap();
        assert!(matches!(resolved, TokenValue::Color(_)));
    }

    #[test]
    fn detect_cycle() {
        let mut tokens = DesignTokens::new();
        let col = tokens.add_collection("Test", vec!["Default"]);

        // Create A -> B -> A cycle
        let a_id = tokens.add_token(col, "a", TokenValue::Number(1.0));
        let b_id = tokens.add_alias_token(col, "b", col, a_id);
        let result = tokens.set_alias(col, a_id, col, b_id);
        assert!(result.is_err());
    }

    #[test]
    fn mode_fallback() {
        let mut tokens = DesignTokens::new();
        let col = tokens.add_collection("Colors", vec!["Light", "Dark"]);
        // Use add_token_for_mode to set value ONLY for Light (default), not Dark
        let light_mode = tokens.collections[0].modes[0].id;
        tokens.add_token_for_mode(col, "bg", light_mode, TokenValue::Color(Color::white()));

        // Switch to Dark
        let dark_mode = tokens.collections[0].modes[1].id;
        tokens.set_active_mode(col, dark_mode);

        // Should fall back to default (Light) since Dark has no value
        let tok_id = tokens.collections[0].tokens[0].id;
        let resolved = tokens.resolve(col, tok_id).unwrap();
        assert!(matches!(resolved, TokenValue::Color(_)));
    }

    #[test]
    fn cross_collection_alias() {
        let mut tokens = DesignTokens::new();
        let colors = tokens.add_collection("Colors", vec!["Default"]);
        let blue_id = tokens.add_token(colors, "blue-500", TokenValue::Color(
            Color::Srgb { r: 0.231, g: 0.510, b: 0.965, a: 1.0 }
        ));

        let components = tokens.add_collection("Components", vec!["Default"]);
        let _alias_id = tokens.add_alias_token(components, "button.bg", colors, blue_id);

        let alias_tok_id = tokens.collections[1].tokens[0].id;
        let resolved = tokens.resolve(components, alias_tok_id).unwrap();
        assert!(matches!(resolved, TokenValue::Color(_)));
    }

    #[test]
    fn token_type_derived_from_value() {
        let val = TokenValue::Color(Color::black());
        assert_eq!(val.token_type(), TokenType::Color);
        let val = TokenValue::Number(42.0);
        assert_eq!(val.token_type(), TokenType::Number);
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Add to `crates/ode-format/src/lib.rs`:
```rust
pub mod tokens;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ode-format -- tokens`
Expected: FAIL — types not defined.

- [ ] **Step 4: Implement tokens.rs**

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::color::Color;
use crate::style::{CollectionId, TokenId, TokenRef};

// ─── IDs ───

// CollectionId and TokenId are re-exported from style.rs (shared types).
pub type ModeId = u32;

// ─── Errors ───

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("token not found: collection={0}, token={1}")]
    NotFound(CollectionId, TokenId),
    #[error("cyclic alias detected involving token {0}")]
    CyclicAlias(TokenId),
    #[error("no value for active or default mode")]
    MissingValue,
    #[error("collection not found: {0}")]
    CollectionNotFound(CollectionId),
}

// ─── TokenType (derived, not stored) ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TokenType {
    Color,
    Number,
    Dimension,
    FontFamily,
    FontWeight,
    Duration,
    CubicBezier,
    String,
}

// ─── TokenValue ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum TokenValue {
    Color(Color),
    Number(f32),
    Dimension { value: f32, unit: DimensionUnit },
    FontFamily(String),
    FontWeight(u16),
    Duration(f32),
    CubicBezier([f32; 4]),
    String(String),
}

impl TokenValue {
    pub fn token_type(&self) -> TokenType {
        match self {
            Self::Color(_) => TokenType::Color,
            Self::Number(_) => TokenType::Number,
            Self::Dimension { .. } => TokenType::Dimension,
            Self::FontFamily(_) => TokenType::FontFamily,
            Self::FontWeight(_) => TokenType::FontWeight,
            Self::Duration(_) => TokenType::Duration,
            Self::CubicBezier(_) => TokenType::CubicBezier,
            Self::String(_) => TokenType::String,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DimensionUnit { Px, Pt, Mm, In, Rem, Em, Percent }

// ─── Token ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TokenResolve {
    Direct(TokenValue),
    Alias(TokenRef),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub id: TokenId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub values: std::collections::HashMap<ModeId, TokenResolve>,
}

// ─── Mode ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mode {
    pub id: ModeId,
    pub name: String,
}

// ─── TokenCollection ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenCollection {
    pub id: CollectionId,
    pub name: String,
    pub modes: Vec<Mode>,
    pub default_mode: ModeId,
    pub tokens: Vec<Token>,
}

// ─── DesignTokens ───

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DesignTokens {
    pub collections: Vec<TokenCollection>,
    pub active_modes: std::collections::HashMap<CollectionId, ModeId>,
    #[serde(skip)]
    next_collection_id: u32,
    #[serde(skip)]
    next_token_id: u32,
    #[serde(skip)]
    next_mode_id: u32,
}

impl DesignTokens {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_collection(&mut self, name: &str, mode_names: Vec<&str>) -> CollectionId {
        let col_id = self.next_collection_id;
        self.next_collection_id += 1;

        let modes: Vec<Mode> = mode_names.iter().map(|n| {
            let id = self.next_mode_id;
            self.next_mode_id += 1;
            Mode { id, name: n.to_string() }
        }).collect();

        let default_mode = modes[0].id;
        self.active_modes.insert(col_id, default_mode);

        self.collections.push(TokenCollection {
            id: col_id,
            name: name.to_string(),
            modes,
            default_mode,
            tokens: Vec::new(),
        });

        col_id
    }

    /// Add a token with a value set only for a specific mode.
    pub fn add_token_for_mode(&mut self, collection_id: CollectionId, name: &str, mode_id: ModeId, value: TokenValue) -> TokenId {
        let tok_id = self.next_token_id;
        self.next_token_id += 1;

        let col = self.collections.iter_mut().find(|c| c.id == collection_id).unwrap();
        let mut values = std::collections::HashMap::new();
        values.insert(mode_id, TokenResolve::Direct(value));

        col.tokens.push(Token {
            id: tok_id,
            name: name.to_string(),
            group: None,
            values,
        });

        tok_id
    }

    /// Add a token with the same value for all modes in the collection.
    pub fn add_token(&mut self, collection_id: CollectionId, name: &str, value: TokenValue) -> TokenId {
        let tok_id = self.next_token_id;
        self.next_token_id += 1;

        let col = self.collections.iter_mut().find(|c| c.id == collection_id).unwrap();
        let mut values = std::collections::HashMap::new();
        // Set value for all modes
        for mode in &col.modes {
            values.insert(mode.id, TokenResolve::Direct(value.clone()));
        }

        col.tokens.push(Token {
            id: tok_id,
            name: name.to_string(),
            group: None,
            values,
        });

        tok_id
    }

    pub fn add_alias_token(
        &mut self,
        collection_id: CollectionId,
        name: &str,
        target_collection: CollectionId,
        target_token: TokenId,
    ) -> TokenId {
        let tok_id = self.next_token_id;
        self.next_token_id += 1;

        let col = self.collections.iter_mut().find(|c| c.id == collection_id).unwrap();
        let mut values = std::collections::HashMap::new();
        for mode in &col.modes {
            values.insert(mode.id, TokenResolve::Alias(TokenRef {
                collection_id: target_collection,
                token_id: target_token,
            }));
        }

        col.tokens.push(Token {
            id: tok_id,
            name: name.to_string(),
            group: None,
            values,
        });

        tok_id
    }

    pub fn set_alias(
        &mut self,
        collection_id: CollectionId,
        token_id: TokenId,
        target_collection: CollectionId,
        target_token: TokenId,
    ) -> Result<(), TokenError> {
        // Check for cycles before applying
        if self.would_create_cycle(target_collection, target_token, collection_id, token_id) {
            return Err(TokenError::CyclicAlias(token_id));
        }

        let col = self.collections.iter_mut()
            .find(|c| c.id == collection_id)
            .ok_or(TokenError::CollectionNotFound(collection_id))?;
        let token = col.tokens.iter_mut()
            .find(|t| t.id == token_id)
            .ok_or(TokenError::NotFound(collection_id, token_id))?;

        for (_, resolve) in token.values.iter_mut() {
            *resolve = TokenResolve::Alias(TokenRef {
                collection_id: target_collection,
                token_id: target_token,
            });
        }

        Ok(())
    }

    pub fn set_active_mode(&mut self, collection_id: CollectionId, mode_id: ModeId) {
        self.active_modes.insert(collection_id, mode_id);
    }

    /// Resolve a token to its final value, following alias chains.
    /// Falls back to default_mode if active mode has no value.
    pub fn resolve(&self, collection_id: CollectionId, token_id: TokenId) -> Result<TokenValue, TokenError> {
        self.resolve_with_visited(collection_id, token_id, &mut Vec::new())
    }

    fn resolve_with_visited(
        &self,
        collection_id: CollectionId,
        token_id: TokenId,
        visited: &mut Vec<(CollectionId, TokenId)>,
    ) -> Result<TokenValue, TokenError> {
        if visited.contains(&(collection_id, token_id)) {
            return Err(TokenError::CyclicAlias(token_id));
        }
        visited.push((collection_id, token_id));

        let col = self.collections.iter()
            .find(|c| c.id == collection_id)
            .ok_or(TokenError::CollectionNotFound(collection_id))?;
        let token = col.tokens.iter()
            .find(|t| t.id == token_id)
            .ok_or(TokenError::NotFound(collection_id, token_id))?;

        let active_mode = self.active_modes.get(&collection_id).copied().unwrap_or(col.default_mode);

        // Try active mode, fall back to default mode
        let resolve = token.values.get(&active_mode)
            .or_else(|| token.values.get(&col.default_mode))
            .ok_or(TokenError::MissingValue)?;

        match resolve {
            TokenResolve::Direct(value) => Ok(value.clone()),
            TokenResolve::Alias(ref tref) => {
                self.resolve_with_visited(tref.collection_id, tref.token_id, visited)
            }
        }
    }

    fn would_create_cycle(
        &self,
        from_collection: CollectionId,
        from_token: TokenId,
        to_collection: CollectionId,
        to_token: TokenId,
    ) -> bool {
        // Walk the alias chain from `from` and see if we reach `to`
        let mut visited = Vec::new();
        let mut current_col = from_collection;
        let mut current_tok = from_token;

        loop {
            if current_col == to_collection && current_tok == to_token {
                return true;
            }
            if visited.contains(&(current_col, current_tok)) {
                return false; // Already a cycle elsewhere
            }
            visited.push((current_col, current_tok));

            let Some(col) = self.collections.iter().find(|c| c.id == current_col) else { return false };
            let Some(token) = col.tokens.iter().find(|t| t.id == current_tok) else { return false };
            let active_mode = self.active_modes.get(&current_col).copied().unwrap_or(col.default_mode);

            match token.values.get(&active_mode).or_else(|| token.values.get(&col.default_mode)) {
                Some(TokenResolve::Alias(tref)) => {
                    current_col = tref.collection_id;
                    current_tok = tref.token_id;
                }
                _ => return false,
            }
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ode-format -- tokens`
Expected: All tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/src/tokens.rs crates/ode-format/src/lib.rs
git commit -m "feat(tokens): add DesignTokens with collection/mode system, alias resolution, and cycle detection"
```

---

### Task 7: Document Model

**Files:**
- Create: `crates/ode-format/src/document.rs`
- Modify: `crates/ode-format/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/ode-format/src/document.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;

    #[test]
    fn create_empty_document() {
        let doc = Document::new("My Design");
        assert_eq!(doc.name, "My Design");
        assert_eq!(doc.format_version, Version(0, 1, 0));
        assert!(doc.canvas.is_empty());
        assert!(doc.views.is_empty());
    }

    #[test]
    fn add_frame_to_canvas() {
        let mut doc = Document::new("Test");
        let frame = Node::new_frame("Artboard 1");
        let id = doc.nodes.insert(frame);
        doc.canvas.push(id);
        assert_eq!(doc.canvas.len(), 1);
        assert_eq!(doc.nodes[id].name, "Artboard 1");
    }

    #[test]
    fn add_export_view() {
        let mut doc = Document::new("Test");
        let frame = Node::new_frame("Icon");
        let id = doc.nodes.insert(frame);
        doc.canvas.push(id);

        doc.views.push(View {
            id: ViewId(0),
            name: "PNG Export".to_string(),
            kind: ViewKind::Export {
                targets: vec![],
            },
        });
        assert_eq!(doc.views.len(), 1);
    }

    #[test]
    fn document_roundtrip_json() {
        let doc = Document::new("Roundtrip Test");
        let json = serde_json::to_string_pretty(&doc).unwrap();
        let parsed: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, parsed);
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Final `crates/ode-format/src/lib.rs`:
```rust
pub mod color;
pub mod style;
pub mod typography;
pub mod node;
pub mod tokens;
pub mod document;

pub use color::Color;
pub use document::Document;
pub use node::{Node, NodeId, NodeKind, NodeTree, StableId};
pub use style::{StyleValue, BlendMode, Fill, Stroke, Effect, Paint, VisualProps, TokenRef, CollectionId, TokenId};
pub use tokens::DesignTokens;
pub use typography::{TextStyle, FontFamily, FontWeight};
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ode-format -- document`
Expected: FAIL — types not defined.

- [ ] **Step 4: Implement document.rs**

```rust
use serde::{Deserialize, Serialize};

use crate::node::{NodeId, NodeTree};
use crate::tokens::DesignTokens;

// ─── Version ───

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version(pub u32, pub u32, pub u32);

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

// ─── IDs ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewId(pub u32);

// ─── Working Color Space ───

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkingColorSpace {
    Srgb,
    DisplayP3,
    AdobeRgb,
    ProPhotoRgb,
}

impl Default for WorkingColorSpace {
    fn default() -> Self { Self::Srgb }
}

// ─── Canvas ───

/// Root-level frame reference on the infinite canvas.
pub type CanvasRoot = NodeId;

// ─── View ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub id: ViewId,
    pub name: String,
    pub kind: ViewKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ViewKind {
    Print {
        pages: Vec<NodeId>,
        // IccProfile and Margins deferred to Format 3
    },
    Web {
        root: NodeId,
        // Breakpoints deferred to Format 5
    },
    Presentation {
        slides: Vec<NodeId>,
        // Transitions deferred to Format 4
    },
    Export {
        targets: Vec<serde_json::Value>, // ExportTarget deferred to Format 1
    },
}

// ─── Document ───

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub format_version: Version,
    pub name: String,
    pub nodes: NodeTree,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canvas: Vec<CanvasRoot>,
    #[serde(default)]
    pub tokens: DesignTokens,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<View>,
    #[serde(default)]
    pub working_color_space: WorkingColorSpace,
}

impl Document {
    pub fn new(name: &str) -> Self {
        Self {
            format_version: Version(0, 1, 0),
            name: name.to_string(),
            nodes: NodeTree::with_key(),
            canvas: Vec::new(),
            tokens: DesignTokens::new(),
            views: Vec::new(),
            working_color_space: WorkingColorSpace::default(),
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ode-format -- document`
Expected: All tests PASS.

- [ ] **Step 6: Run all tests**

Run: `cargo test -p ode-format`
Expected: ALL tests across all modules PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/ode-format/src/document.rs crates/ode-format/src/lib.rs
git commit -m "feat(document): add Document, View, ViewKind, WorkingColorSpace — complete ode-format data model"
```

---

## Chunk 4: Integration Test + Cleanup

### Task 8: Integration Test — Full Document Roundtrip

**Files:**
- Create: `crates/ode-format/tests/integration.rs`

- [ ] **Step 1: Write the integration test**

Create `crates/ode-format/tests/integration.rs`:

```rust
use ode_format::color::Color;
use ode_format::document::{Document, View, ViewId, ViewKind};
use ode_format::node::Node;
use ode_format::style::*;

/// End-to-end test: create a document with nodes, styles, tokens, and views,
/// serialize to JSON, deserialize, and verify equality.
#[test]
fn full_document_roundtrip() {
    let mut doc = Document::new("Integration Test");

    // Create a frame with a fill
    let mut frame = Node::new_frame("Card");
    if let ode_format::node::NodeKind::Frame(ref mut data) = frame.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::from_hex("#3b82f6").unwrap()),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
        data.visual.strokes.push(Stroke {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::black()),
            },
            width: StyleValue::Raw(1.0),
            position: StrokePosition::Inside,
            cap: StrokeCap::Butt,
            join: StrokeJoin::Miter,
            miter_limit: 4.0,
            dash: None,
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }

    // Create a text node
    let text = Node::new_text("Title", "Hello, ODE!");

    // Insert nodes and build tree
    let frame_id = doc.nodes.insert(frame);
    let text_id = doc.nodes.insert(text);

    // Add text as child of frame
    if let ode_format::node::NodeKind::Frame(ref mut data) = doc.nodes[frame_id].kind {
        data.container.children.push(text_id);
    }

    // Register frame as canvas root
    doc.canvas.push(frame_id);

    // Add an export view
    doc.views.push(View {
        id: ViewId(0),
        name: "PNG Export".to_string(),
        kind: ViewKind::Export { targets: vec![] },
    });

    // Serialize
    let json = serde_json::to_string_pretty(&doc).unwrap();

    // Verify JSON is not empty and contains key fields
    assert!(json.contains("Integration Test"));
    assert!(json.contains("Card"));
    assert!(json.contains("Hello, ODE!"));
    assert!(json.contains("3b82f6") || json.contains("0.231"));

    // Deserialize
    let parsed: Document = serde_json::from_str(&json).unwrap();

    // Verify document structure
    assert_eq!(parsed.name, "Integration Test");
    assert_eq!(parsed.canvas.len(), 1);
    assert_eq!(parsed.views.len(), 1);
    assert_eq!(parsed.format_version, ode_format::document::Version(0, 1, 0));

    // Verify parent-child relationship survived roundtrip
    let parsed_frame_id = parsed.canvas[0];
    if let ode_format::node::NodeKind::Frame(ref data) = parsed.nodes[parsed_frame_id].kind {
        assert_eq!(data.container.children.len(), 1, "Frame should have 1 child after roundtrip");
    } else {
        panic!("Expected Frame node");
    }

    // Verify working color space roundtrip
    assert_eq!(parsed.working_color_space, doc.working_color_space);
}

#[test]
fn style_value_bound_with_token_roundtrip() {
    use ode_format::style::{TokenRef, CollectionId, TokenId};
    use ode_format::tokens::TokenValue;

    let mut doc = Document::new("Bound Token Test");

    // Create token
    let col = doc.tokens.add_collection("Colors", vec!["Light"]);
    let tok_id = doc.tokens.add_token(col, "primary", TokenValue::Color(
        Color::from_hex("#3b82f6").unwrap()
    ));

    // Create frame with a token-bound fill
    let mut frame = Node::new_frame("Card");
    if let ode_format::node::NodeKind::Frame(ref mut data) = frame.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Bound {
                    token: TokenRef { collection_id: col as CollectionId, token_id: tok_id as TokenId },
                    resolved: Color::from_hex("#3b82f6").unwrap(),
                },
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let frame_id = doc.nodes.insert(frame);
    doc.canvas.push(frame_id);

    // Roundtrip
    let json = serde_json::to_string_pretty(&doc).unwrap();
    let parsed: Document = serde_json::from_str(&json).unwrap();

    // Verify bound style value survived
    let parsed_frame_id = parsed.canvas[0];
    if let ode_format::node::NodeKind::Frame(ref data) = parsed.nodes[parsed_frame_id].kind {
        assert_eq!(data.visual.fills.len(), 1);
        if let Paint::Solid { ref color } = data.visual.fills[0].paint {
            assert!(color.is_bound(), "Fill color should still be bound to token after roundtrip");
        }
    }

#[test]
fn document_with_tokens() {
    use ode_format::tokens::TokenValue;

    let mut doc = Document::new("Token Test");

    // Add color tokens
    let col = doc.tokens.add_collection("Colors", vec!["Light", "Dark"]);
    doc.tokens.add_token(col, "primary", TokenValue::Color(
        Color::from_hex("#3b82f6").unwrap()
    ));

    // Resolve token
    let tok_id = doc.tokens.collections[0].tokens[0].id;
    let resolved = doc.tokens.resolve(col, tok_id).unwrap();
    assert!(matches!(resolved, TokenValue::Color(_)));

    // Serialize
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("primary"));
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test -p ode-format --test integration`
Expected: All tests PASS.

- [ ] **Step 3: Run the full test suite**

Run: `cargo test -p ode-format`
Expected: ALL tests PASS (unit + integration).

- [ ] **Step 4: Commit**

```bash
git add crates/ode-format/tests/integration.rs
git commit -m "test: add integration tests for full document roundtrip with nodes, styles, tokens, and views"
```

---

### Task 9: Verify Workspace Integrity

**Files:**
- Verify: `crates/ode-format/src/lib.rs` has no references to non-existent modules

- [ ] **Step 1: Verify lib.rs is clean**

Run: `cargo check -p ode-format`
Expected: No warnings, no errors.

- [ ] **Step 2: Run full workspace check**

Run: `cargo check`
Expected: All crates compile (ode-core, ode-export, ode-cli, ode-mcp may have empty modules but should compile).

- [ ] **Step 3: Final test run**

Run: `cargo test --workspace`
Expected: All tests pass across the workspace.

- [ ] **Step 4: Final commit**

```bash
git add crates/ode-format/src/lib.rs
git commit -m "chore: verify workspace integrity — all crates compile with updated ode-format"
```

(Only stage lib.rs if cleanup was needed. If no changes, skip this commit.)
