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
    #[serde(alias = "fixed")]
    Start,      // Pin to left/top edge — fixed distance from start
    End,        // Pin to right/bottom edge — fixed distance from end
    #[serde(alias = "stretch")]
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

Breaking change to `.ode.json` v0.2. Serialized as kebab-case: `start`, `end`, `start-end`, `center`, `scale`. Serde aliases (`fixed` → `Start`, `stretch` → `StartEnd`) provide backward compatibility for existing v0.2 files.

### Default Behavior

When a node has `constraints: None` (the default), it is treated as implicit `Start`/`Start` — the child stays at its current position during parent resize. This matches Figma's default behavior (`LEFT` + `TOP`).

## 2. Constraints Calculation Engine

### Core Function

```rust
fn apply_constraints(
    child_rect: LayoutRect,    // reuse existing LayoutRect type
    constraints: &Constraints,
    original_parent: (f32, f32),  // (width, height) design-time
    current_parent: (f32, f32),   // (width, height) current
) -> LayoutRect
```

### Child Rect Construction

The `child_rect` is constructed from the node's transform and intrinsic size:

```rust
LayoutRect {
    x: node.transform.tx,       // position in parent space
    y: node.transform.ty,
    width: <node_kind_width>,   // from FrameData.width, TextData.width, ImageData.width, etc.
    height: <node_kind_height>,
}
```

For rotated children (`transform.b != 0` or `transform.c != 0`), constraints modify only `tx`/`ty` (and `width`/`height` for StartEnd/Scale). The rotation components (`a, b, c, d`) are preserved in the transform, matching Figma's behavior of constraining the origin point in parent-local space.

### Per-Axis Calculation

Given: original parent width `ow`, current parent width `nw`, child position `x`, child width `w`:

| Axis Type | new_x | new_w |
|---|---|---|
| **Start** | `x` | `w` |
| **End** | `x + (nw - ow)` | `w` |
| **StartEnd** | `x` | `max(0, w + (nw - ow))` |
| **Center** | `x + (nw - ow) / 2` | `w` |
| **Scale** | `x * (nw / ow)` | `w * (nw / ow)` |

Vertical axis uses the same formulas with `y`, `h`, `oh`, `nh`.

**Edge case guards:**
- **Scale with zero parent:** If `ow == 0` (or `oh == 0`), Scale degrades to Start (no change) to avoid division by zero.
- **StartEnd negative width:** Clamped to 0 via `max(0, ...)`.

### Execution Flow

Inside `compute_layout()`:

1. **Phase 1:** `walk_for_layout()` — existing Auto Layout (Taffy) computation
2. **Phase 2:** `walk_for_constraints()` — NEW: depth-first **top-down** walk applying constraints

```
walk_for_constraints(node_id, resize_map, result):
  node = doc.nodes[node_id]

  // Only process Frame and Instance containers WITHOUT auto-layout
  if node is Frame or Instance, AND has no LayoutConfig:
    design_size = (frame.width, frame.height)
    current_size = resize_map.get(node_id) OR design_size

    // Also check if this node was itself resized by its parent's constraints
    if result contains node_id with different size:
      current_size = (result[node_id].width, result[node_id].height)

    if current_size ≠ design_size:
      for each child:
        constraints = child.constraints OR default (Start, Start)
        if constraints == (Start, Start): skip  // no-op optimization
        child_rect = LayoutRect from child's transform + intrinsic size
        new_rect = apply_constraints(child_rect, constraints, design_size, current_size)
        insert new_rect into result

  // Group nodes are transparent — skip to children directly
  // Recurse into children (top-down: parent resolved before children)
  for each child of node:
    walk_for_constraints(child_id, resize_map, result)
```

**Key design decisions:**
- **Top-down traversal:** Parent constraints are resolved before children, so nested constrained frames work correctly.
- **Frame and Instance containers:** Both can have children and constraints. Group nodes are transparent — they don't participate in constraint resolution; their children are constrained against the nearest Frame/Instance ancestor.
- **Auto Layout mutual exclusivity:** Auto Layout frames' children are always skipped. If a frame has `LayoutConfig`, constraints on its children are ignored.

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
    // Existing API — passes empty ResizeMap internally
    pub fn from_document(doc: &Document, font_db: &FontDatabase) -> Result<Self, ConvertError> {
        Self::from_document_with_resize(doc, font_db, &ResizeMap::new())
    }

    // New API with resize support
    pub fn from_document_with_resize(
        doc: &Document,
        font_db: &FontDatabase,
        resize_map: &ResizeMap,
    ) -> Result<Self, ConvertError> {
        // ...
        let layout_map = compute_layout(doc, &stable_id_index, resize_map);
        // rest unchanged
    }
}
```

### convert_node Changes

When a `LayoutRect` exists for a node (from either Auto Layout or Constraints):
- **Position:** use `rect.x`, `rect.y` (already implemented)
- **Size:** use `rect.width`, `rect.height` for clip paths and visual rendering (new — currently uses frame's design-time width/height)

Specific code paths that need layout rect size:
- `get_clip_path()` — already uses layout_rect for clip dimensions
- `get_node_path()` Image arm — currently uses `ImageData.width/height`, must prefer layout rect
- `emit_image()` — currently uses `ImageData.width/height` for `DrawImage` command, must prefer layout rect
- `get_frame_size()` — already uses layout rect (no change needed)

## 4. CLI Interface

Add `--resize WxH` option to `render` and `build` commands:

```
ode render input.ode.json -o output.png --resize 1920x1080
```

The CLI resolves `doc.canvas[0]` to its `NodeId` and inserts `(node_id, (width, height))` into the `ResizeMap`. Children of that root frame respond according to their constraints.

## 5. Files Changed

| File | Change |
|---|---|
| `crates/ode-format/src/node.rs` | `ConstraintAxis` enum: `Fixed` → `Start`/`End`, `Stretch` → `StartEnd`, add serde aliases |
| `crates/ode-import/src/figma/convert_layout.rs` | Update Figma → ODE constraint mapping |
| `crates/ode-core/src/layout.rs` | Add `apply_constraints()`, `walk_for_constraints()`, `ResizeMap` param, child rect construction |
| `crates/ode-core/src/convert.rs` | Add `from_document_with_resize()`, use layout rect size in `emit_image`/`get_node_path` Image arm |
| `crates/ode-cli/src/main.rs` | Add `--resize` CLI option to `render` and `build` commands |

## 6. Testing Strategy

1. **Unit tests** for `apply_constraints()` — 5 axis types × 2 axes = 10 cases, plus edge cases (zero parent, negative StartEnd)
2. **Integration tests** — parent resize with children verifying correct repositioning/resizing
3. **Nested constraints test** — grandparent resized → parent StartEnd stretches → child End repositions
4. **Mutual exclusivity test** — auto-layout frame children with constraints are unaffected by constraints
5. **Group transparency test** — constrained child inside a Group inside a resized Frame
6. **Instance container test** — Instance with children and no auto-layout responds to constraints
7. **None constraints test** — child with `constraints: None` behaves as Start/Start
