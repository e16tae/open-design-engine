# CLI Document Mutation System

**Date:** 2026-03-17
**Status:** Approved
**Scope:** Phase 1 — individual commands (`add`, `set`, `delete`, `move`); Phase 2 — `pipe` batch mode

## Problem

ODE CLI is branded "Agent-native design tool" but has no way to create or modify design content through the CLI. After `ode new`, the only path to a populated document is importing from Figma or hand-editing JSON. This breaks the agent workflow loop:

```
ode new → ??? → ode review → ode build
```

## Solution

Add four mutation commands that complete the loop:

```
ode new → ode add → ode set → ode review → ode build
```

All commands operate on the wire format (`DocumentWire`) to preserve stable_id-based references. All output follows the existing JSON + exit code pattern.

## Design Principles

- **Wire format only.** Load `DocumentWire`, mutate, save. Never round-trip through `Document`/`NodeTree` (runtime `NodeId` is not serialization-safe).
- **stable_id addressing.** All node references use stable_id strings.
- **JSON output.** Every command returns structured JSON via `print_json`. Exit codes follow existing convention (0=ok, 1=input, 2=io, 3=process, 4=internal).
- **Atomic writes.** File is only written on full success. Any error leaves the file untouched.
- **No new crate.** Commands live in `ode-cli/src/mutate.rs`. Shape generators live in `ode-format/src/shapes.rs`. Wire helpers live in `ode-format/src/wire.rs`.

---

## Phase 1: Individual Commands

### 1. `ode add <kind> <file> [options]`

Creates a node and inserts it into the document tree.

#### Supported Kinds and Parameters

| Kind | Required | Optional |
|------|----------|----------|
| `frame` | `--name`, `--width`, `--height` | `--parent`, `--index`, `--fill`, `--corner-radius`, `--clips-content` |
| `group` | (none) | `--parent`, `--index`, `--name` |
| `text` | `--content` | `--parent`, `--index`, `--name`, `--font-size`, `--font-family`, `--fill`, `--width`, `--height` |
| `vector` | `--shape` | `--parent`, `--index`, `--name`, `--width`, `--height`, `--fill` |
| `image` | `--width`, `--height` | `--parent`, `--index`, `--name`, `--src` |

Default names when `--name` is omitted: text → "Text", vector → shape name ("Rectangle", "Ellipse", "Line", "Star", "Polygon"), image → "Image", group → "Group".

`--index N` inserts at position N in parent's children (0-based). Omitted → append at end.

#### Shape Presets (`--shape`)

- `rect` — rectangle path (default 100x100)
- `ellipse` — ellipse path (width/height based)
- `line` — horizontal line (width based)
- `star` — 5-pointed star (width based)
- `polygon --sides N` — regular polygon

Shape generators are pure functions in `ode-format/src/shapes.rs` that produce `VectorPath` data.

#### Parent Resolution

| `--parent` value | Behavior |
|------------------|----------|
| omitted + canvas non-empty | Append to `doc.canvas[0]`'s children |
| omitted + canvas empty | Append to `doc.canvas` as top-level node (same as `root`) |
| `root` | Append to `doc.canvas` (top-level node) |
| `<stable_id>` | Append to that node's children |

Containers that accept children: Frame, Group, BooleanOp, Instance. Error if target parent is a non-container (Vector, Text, Image).

#### Response

```json
{
  "status": "ok",
  "stable_id": "V1StGXR8_Z5jdHi6B-myT",
  "name": "Card",
  "kind": "frame",
  "parent": "root"
}
```

### 2. `ode set <file> <stable_id> [properties...]`

Modifies properties of an existing node.

#### Property Categories

**Common (all nodes):**

| Flag | Type | Description |
|------|------|-------------|
| `--name` | string | Node name |
| `--visible` | bool | Visibility |
| `--opacity` | f32 | 0.0–1.0 |
| `--blend-mode` | enum | normal, multiply, screen, overlay, ... |
| `--x`, `--y` | f32 | Position (transform.tx, transform.ty) |

**Size (frame, text, image):**

| Flag | Type | Description |
|------|------|-------------|
| `--width` | f32 | Node width |
| `--height` | f32 | Node height |

Note: Vector size is defined by its path geometry, not by explicit width/height fields.

**Visual (frame, vector, text, image, boolean-op — nodes with VisualProps):**

| Flag | Type | Description |
|------|------|-------------|
| `--fill` | color | Set first solid fill |
| `--fill-opacity` | f32 | Fill color alpha |
| `--stroke` | color | Set first solid stroke |
| `--stroke-width` | f32 | Stroke width |
| `--stroke-position` | enum | center, inside, outside |

**Frame-specific:**

| Flag | Type | Description |
|------|------|-------------|
| `--corner-radius` | f32 or "TL,TR,BR,BL" | Corner radii |
| `--clips-content` | bool | Clip children |
| `--layout` | enum | horizontal, vertical (enables auto-layout) |
| `--padding` | f32 or "T,R,B,L" | Auto-layout padding |
| `--gap` | f32 | Auto-layout item spacing |

**Text-specific:**

| Flag | Type | Description |
|------|------|-------------|
| `--content` | string | Text content |
| `--font-size` | f32 | Font size |
| `--font-family` | string | Font family |
| `--font-weight` | u16 | Font weight |
| `--text-align` | enum | left, center, right |
| `--line-height` | f32 or "auto" | Line height: bare number → Percent variant, "auto" → Auto variant |

#### Color Parsing

Supported formats:
- `#RRGGBB` — e.g., `#FF0000`
- `#RRGGBBAA` — e.g., `#FF000080`
- `#RGB` — e.g., `#F00` (expanded to `#RRGGBB`; requires extending `Color::from_hex`)

#### Type Safety

Applying a property that doesn't match the node kind returns an error:
- `--layout` on a vector → `INVALID_PROPERTY` error
- `--font-size` on a frame → `INVALID_PROPERTY` error

#### Response

```json
{
  "status": "ok",
  "stable_id": "abc123",
  "modified": ["fill", "corner-radius", "opacity"]
}
```

### 3. `ode delete <file> <stable_id>`

Removes a node and all its descendants.

- Collects all descendant stable_ids recursively
- Removes all collected nodes from the flat node list
- Removes the node's stable_id from its parent's children array
- If the node is a canvas root, removes from `doc.canvas`
- If a remaining Instance references a deleted component, emit a warning (do not block)
- If a View references the deleted node (e.g., as a page or root), remove the reference from the view and emit a warning

#### Response

```json
{
  "status": "ok",
  "deleted": ["abc123", "child1", "child2"],
  "warnings": []
}
```

### 4. `ode move <file> <stable_id> --parent <target_id> [--index N]`

Moves a node to a different parent.

- Remove from old parent's children
- Insert into new parent's children at `--index` (or append if omitted)
- `--parent root` moves to canvas root level (`doc.canvas` array; `--index` controls position within it)
- If the node is already at canvas root and `--parent root` is given, it reorders within `doc.canvas`
- Cycle detection: if target is a descendant of the moved node → `CYCLE_DETECTED` error
- Non-container target → `NOT_CONTAINER` error

#### Response

```json
{
  "status": "ok",
  "stable_id": "abc123",
  "new_parent": "target456",
  "index": 0
}
```

---

## Implementation Architecture

### New Files

| File | Purpose |
|------|---------|
| `crates/ode-cli/src/mutate.rs` | Command implementations for add, set, delete, move |
| `crates/ode-format/src/shapes.rs` | Preset shape path generators (rect, ellipse, line, star, polygon) |

### Modified Files

| File | Changes |
|------|---------|
| `crates/ode-cli/src/main.rs` | Register Add, Set, Delete, Move subcommands |
| `crates/ode-cli/src/output.rs` | Add MutateResponse types |
| `crates/ode-format/src/wire.rs` | Add find/insert/remove helper methods |
| `crates/ode-format/src/lib.rs` | `pub mod shapes` |

### Wire Format Helpers (`wire.rs`)

```rust
impl DocumentWire {
    fn find_node(&self, stable_id: &str) -> Option<&NodeWire>;
    fn find_node_mut(&mut self, stable_id: &str) -> Option<&mut NodeWire>;
    fn find_parent(&self, child_id: &str) -> Option<String>;
    fn remove_child_from_parent(&mut self, child_id: &str);
    fn collect_descendants(&self, stable_id: &str) -> Vec<String>;
    fn is_container(kind: &NodeKindWire) -> bool;
    fn children_mut(kind: &mut NodeKindWire) -> Option<&mut Vec<String>>;
}
```

### Implementation Notes

- Setting `--x`/`--y` modifies only `transform.tx`/`transform.ty`, preserving rotation and scale components (`a`, `b`, `c`, `d`).
- New stable_ids are generated using `nanoid` (matching existing `Node` constructors).
- `--corner-radius` on `ode add vector --shape rect` affects path generation (rounded rect path), not a stored property. `ode set --corner-radius` only applies to Frame nodes.
- `is_container()` returns true for Frame, Group, BooleanOp, Instance — matching runtime `NodeKind::children()`.
- `--layout horizontal|vertical` enables auto-layout with defaults: align-primary=Start, align-counter=Start, wrap=NoWrap. Alignment and wrap flags are deferred to a later iteration.

### Shared Load/Save Pattern

All mutation commands follow:

```
1. load_input(file) → JSON string
2. serde_json::from_str → DocumentWire
3. mutate DocumentWire
4. serde_json::to_string_pretty → JSON string
5. std::fs::write(file, json)
6. print_json(response)
```

---

## Phase 2: `ode pipe` (Future)

Batch mode for multiple operations in a single file I/O cycle.

```bash
ode pipe doc.ode.json <<'EOF'
add frame --name "Card" --width 320 --height 200 --as card
add text --content "Title" --parent $card --as title
set $title --fill "#1E293B" --font-size 24
EOF
```

- `--as <alias>` assigns a local alias to a created node
- `$alias` substitutes the stable_id in subsequent lines
- Single file read at start, single write at end
- Any line failure → full rollback (no file modification)

Phase 1 command parsers are reused; pipe adds alias substitution and transactional wrapping.

---

## Testing Strategy

### Unit Tests (ode-format)
- `shapes.rs`: Each preset produces a valid, non-empty `VectorPath`
- `wire.rs`: find_node, find_parent, collect_descendants, remove_child_from_parent, cycle detection

### Integration Tests (ode-cli)
- `add` → verify file contains new node, inspect confirms tree structure
- `set` → verify properties changed, unchanged properties preserved
- `delete` → verify node and descendants removed, parent children updated
- `move` → verify node relocated, old parent updated, cycle rejected
- Round-trip: `new` → `add` (multiple) → `set` → `build` → valid PNG output
- Error cases: missing parent, invalid color, non-container parent, cycle detection

### Agent Workflow Test
End-to-end: `ode new` → series of `add`/`set` → `ode review` → `ode build` → verify output file exists and is valid.
