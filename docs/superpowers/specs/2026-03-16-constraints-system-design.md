# Constraints System Design

## Overview

Implement a Figma-compatible constraints system that controls how child nodes respond when their parent frame is resized. This is an independent layout system that operates alongside (but mutually exclusive with) Auto Layout.

**Scope:** Full resize support — constraints are applied at render time with an optional resize override, enabling viewport-adaptive rendering.

## 1. ConstraintAxis Enum Expansion

### Current (broken)

```rust
pub enum ConstraintAxis {
    Fixed,    // LEFT and RIGHT both map here — information loss
    Scale,
    Stretch,
    Center,
}
```

### New

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintAxis {
    Start,      // Pin to left/top edge — fixed distance from start
    End,        // Pin to right/bottom edge — fixed distance from end
    StartEnd,   // Pin both edges — child stretches
    Center,     // Maintain center position relative to parent center
    Scale,      // Scale position and size proportionally with parent
}
```

### Figma Mapping

| Figma Constraint | ODE ConstraintAxis |
|---|---|
| `LEFT` / `TOP` | `Start` |
| `RIGHT` / `BOTTOM` | `End` |
| `LEFT_RIGHT` / `TOP_BOTTOM` | `StartEnd` |
| `CENTER` | `Center` |
| `SCALE` | `Scale` |

### File Format

Breaking change to `.ode.json` v0.2. Serialized as kebab-case: `start`, `end`, `start-end`, `center`, `scale`.

## 2. Constraints Calculation Engine

### Core Function

```rust
fn apply_constraints(
    child_rect: Rect,
    constraints: &Constraints,
    original_parent: Size,
    current_parent: Size,
) -> Rect
```

### Per-Axis Calculation

Given: original parent width `ow`, current parent width `nw`, child position `x`, child width `w`:

| Axis Type | new_x | new_w |
|---|---|---|
| **Start** | `x` | `w` |
| **End** | `x + (nw - ow)` | `w` |
| **StartEnd** | `x` | `w + (nw - ow)` |
| **Center** | `x + (nw - ow) / 2` | `w` |
| **Scale** | `x * (nw / ow)` | `w * (nw / ow)` |

Vertical axis uses the same formulas with `y`, `h`, `oh`, `nh`.

### Execution Flow

Inside `compute_layout()`:

1. **Phase 1:** `walk_for_layout()` — existing Auto Layout (Taffy) computation
2. **Phase 2:** `walk_for_constraints()` — NEW: apply constraints to children of non-auto-layout frames

```
walk_for_constraints():
  for each frame WITHOUT auto-layout:
    determine current_size (from resize_map or design-time size)
    if current_size ≠ design_size:
      for each child with constraints:
        compute child_rect from child's transform
        new_rect = apply_constraints(child_rect, constraints, design_size, current_size)
        insert new_rect into LayoutMap
```

Auto Layout frames' children are **always skipped** — Auto Layout and Constraints are mutually exclusive, matching Figma behavior.

### Resize Input

```rust
pub type ResizeMap = HashMap<NodeId, (f32, f32)>;

pub fn compute_layout(
    doc: &Document,
    stable_id_index: &HashMap<&str, NodeId>,
    resize_map: &ResizeMap,
) -> LayoutMap
```

## 3. Scene Conversion Integration

### API

```rust
impl Scene {
    // Existing API — unchanged behavior (empty ResizeMap)
    pub fn from_document(doc: &Document, font_db: &FontDatabase) -> Result<Self, ConvertError>;

    // New API with resize support
    pub fn from_document_with_resize(
        doc: &Document,
        font_db: &FontDatabase,
        resize_map: &ResizeMap,
    ) -> Result<Self, ConvertError>;
}
```

### convert_node Changes

When a `LayoutRect` exists for a node (from either Auto Layout or Constraints):
- Position: use `rect.x`, `rect.y` (already implemented)
- Size: use `rect.width`, `rect.height` for clip paths and visual rendering (new — currently uses frame's design-time width/height)

This is needed because `StartEnd` and `Scale` constraints modify child dimensions.

## 4. CLI Interface

Add `--resize WxH` option to `render` and `build` commands:

```
ode render input.ode.json -o output.png --resize 1920x1080
```

Resizes the first canvas root frame. Children respond according to their constraints.

## 5. Files Changed

| File | Change |
|---|---|
| `ode-format/src/node.rs` | `ConstraintAxis` enum: `Fixed` → `Start`/`End`, `Stretch` → `StartEnd` |
| `ode-import/src/figma/convert_layout.rs` | Update Figma → ODE constraint mapping |
| `ode-core/src/layout.rs` | Add `apply_constraints()`, `walk_for_constraints()`, `ResizeMap` param |
| `ode-core/src/convert.rs` | Add `from_document_with_resize()`, use layout rect size in rendering |
| `ode-cli/src/main.rs` | Add `--resize` CLI option |

## 6. Testing Strategy

1. **Unit tests** for `apply_constraints()` — 5 axis types × 2 axes = 10 cases
2. **Integration tests** — parent resize with children verifying correct repositioning/resizing
3. **Mutual exclusivity test** — auto-layout frame children with constraints are unaffected by constraints
4. **Edge cases** — zero parent size (avoid division by zero in Scale), negative resize delta with StartEnd (clamp width to 0)
