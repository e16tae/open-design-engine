# CLI Document Mutation System Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `ode add`, `ode set`, `ode delete`, `ode move` commands to complete the agent workflow loop (new → add → set → review → build).

**Architecture:** All commands operate on `DocumentWire` (flat `Vec<NodeWire>` with stable_id string references). Wire helpers in `ode-format/src/wire.rs` provide find/insert/remove primitives. Shape presets in `ode-format/src/shapes.rs` generate `VectorPath` data. CLI commands in `ode-cli/src/mutate.rs` wire everything together.

**Tech Stack:** Rust, clap (CLI), serde_json (wire format), nanoid (stable_id generation)

**Spec:** `docs/superpowers/specs/2026-03-17-cli-document-mutation-design.md`

---

## Chunk 1: Foundation

### File Structure (Chunk 1)

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/ode-format/src/wire.rs` | Modify | Add `DocumentWire` helper methods (find, insert, remove, container checks) |
| `crates/ode-format/src/shapes.rs` | Create | Preset shape path generators (rect, ellipse, line, star, polygon) |
| `crates/ode-format/src/color.rs` | Modify | Extend `from_hex` to support 3-char `#RGB` |
| `crates/ode-format/src/lib.rs` | Modify | Add `pub mod shapes` |
| `crates/ode-cli/src/output.rs` | Modify | Add mutation response types |

---

### Task 1: Wire Format Helpers

**Files:**
- Modify: `crates/ode-format/src/wire.rs`

These helpers are the foundation for all mutation commands. They operate on `DocumentWire` using stable_id string lookups.

- [ ] **Step 1: Write tests for `children_of_kind` and `is_container`**

Add to the existing `mod tests` block at the bottom of `wire.rs`:

```rust
#[test]
fn is_container_returns_true_for_frame_group_booleanop_instance() {
    assert!(DocumentWire::is_container(&NodeKindWire::Frame(FrameDataWire {
        width: 0.0, height: 0.0,
        width_sizing: SizingMode::Fixed, height_sizing: SizingMode::Fixed,
        corner_radius: [0.0; 4], clips_content: true,
        visual: VisualProps::default(),
        container: ContainerPropsWire::default(),
        component_def: None,
    })));
    assert!(DocumentWire::is_container(&NodeKindWire::Group(GroupDataWire {
        children: vec![],
    })));
    assert!(!DocumentWire::is_container(&NodeKindWire::Text(TextDataWire {
        visual: VisualProps::default(),
        content: "hi".into(),
        runs: vec![], default_style: TextStyle::default(),
        width: 100.0, height: 100.0,
        sizing_mode: TextSizingMode::Fixed,
    })));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-format is_container_returns_true -- --nocapture`
Expected: FAIL — `is_container` method doesn't exist yet.

- [ ] **Step 3: Implement `is_container` and `children_of_kind_mut`**

Add to `impl DocumentWire` block (after `into_document` method, before the conversion helpers):

```rust
// ─── Wire Mutation Helpers ───

/// Check if a node kind can have children.
pub fn is_container(kind: &NodeKindWire) -> bool {
    matches!(
        kind,
        NodeKindWire::Frame(_)
            | NodeKindWire::Group(_)
            | NodeKindWire::BooleanOp(_)
            | NodeKindWire::Instance(_)
    )
}

/// Get mutable reference to a node kind's children list.
pub fn children_of_kind_mut(kind: &mut NodeKindWire) -> Option<&mut Vec<String>> {
    match kind {
        NodeKindWire::Frame(d) => Some(&mut d.container.children),
        NodeKindWire::Group(d) => Some(&mut d.children),
        NodeKindWire::BooleanOp(d) => Some(&mut d.children),
        NodeKindWire::Instance(d) => Some(&mut d.container.children),
        _ => None,
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ode-format is_container_returns_true`
Expected: PASS

- [ ] **Step 5: Write tests for `find_node` and `find_node_mut`**

```rust
#[test]
fn find_node_by_stable_id() {
    let wire = make_test_wire();
    assert!(wire.find_node("frame-1").is_some());
    assert_eq!(wire.find_node("frame-1").unwrap().name, "Parent Frame");
    assert!(wire.find_node("nonexistent").is_none());
}

#[test]
fn find_node_mut_modifies_in_place() {
    let mut wire = make_test_wire();
    wire.find_node_mut("text-1").unwrap().name = "Changed".to_string();
    assert_eq!(wire.find_node("text-1").unwrap().name, "Changed");
}
```

Also add a shared test helper at the top of `mod tests`:

```rust
/// Shared test fixture: frame-1 with child text-1.
fn make_test_wire() -> DocumentWire {
    DocumentWire {
        format_version: Version(0, 2, 0),
        name: "Test".to_string(),
        nodes: vec![
            NodeWire {
                stable_id: "frame-1".to_string(),
                name: "Parent Frame".to_string(),
                transform: Transform::default(),
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                visible: true,
                constraints: None,
                layout_sizing: None,
                kind: NodeKindWire::Frame(FrameDataWire {
                    width: 200.0, height: 100.0,
                    width_sizing: SizingMode::Fixed,
                    height_sizing: SizingMode::Fixed,
                    corner_radius: [0.0; 4],
                    clips_content: true,
                    visual: VisualProps::default(),
                    container: ContainerPropsWire {
                        children: vec!["text-1".to_string()],
                        layout: None,
                    },
                    component_def: None,
                }),
            },
            NodeWire {
                stable_id: "text-1".to_string(),
                name: "Child Text".to_string(),
                transform: Transform::default(),
                opacity: 1.0,
                blend_mode: BlendMode::Normal,
                visible: true,
                constraints: None,
                layout_sizing: None,
                kind: NodeKindWire::Text(TextDataWire {
                    visual: VisualProps::default(),
                    content: "Hello".to_string(),
                    runs: Vec::new(),
                    default_style: TextStyle::default(),
                    width: 100.0, height: 100.0,
                    sizing_mode: TextSizingMode::Fixed,
                }),
            },
        ],
        canvas: vec!["frame-1".to_string()],
        tokens: DesignTokens::new(),
        views: vec![],
        working_color_space: WorkingColorSpace::default(),
    }
}
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cargo test -p ode-format find_node`
Expected: FAIL — methods don't exist.

- [ ] **Step 7: Implement `find_node` and `find_node_mut`**

Add to `impl DocumentWire`:

```rust
/// Find a node by stable_id.
pub fn find_node(&self, stable_id: &str) -> Option<&NodeWire> {
    self.nodes.iter().find(|n| n.stable_id == stable_id)
}

/// Find a node by stable_id (mutable).
pub fn find_node_mut(&mut self, stable_id: &str) -> Option<&mut NodeWire> {
    self.nodes.iter_mut().find(|n| n.stable_id == stable_id)
}
```

- [ ] **Step 8: Run test to verify it passes**

Run: `cargo test -p ode-format find_node`
Expected: PASS

- [ ] **Step 9: Write tests for `find_parent` and `remove_child_from_parent`**

```rust
#[test]
fn find_parent_returns_parent_stable_id() {
    let wire = make_test_wire();
    assert_eq!(wire.find_parent("text-1"), Some("frame-1".to_string()));
    assert_eq!(wire.find_parent("frame-1"), None); // canvas root has no parent node
}

#[test]
fn remove_child_from_parent_updates_children() {
    let mut wire = make_test_wire();
    wire.remove_child_from_parent("text-1");
    if let NodeKindWire::Frame(d) = &wire.find_node("frame-1").unwrap().kind {
        assert!(d.container.children.is_empty());
    } else {
        panic!("Expected Frame");
    }
}
```

- [ ] **Step 10: Implement `find_parent` and `remove_child_from_parent`**

```rust
/// Find the stable_id of the parent that has `child_id` in its children.
pub fn find_parent(&self, child_id: &str) -> Option<String> {
    for node in &self.nodes {
        let children = match &node.kind {
            NodeKindWire::Frame(d) => &d.container.children,
            NodeKindWire::Group(d) => &d.children,
            NodeKindWire::BooleanOp(d) => &d.children,
            NodeKindWire::Instance(d) => &d.container.children,
            _ => continue,
        };
        if children.iter().any(|c| c == child_id) {
            return Some(node.stable_id.clone());
        }
    }
    None
}

/// Remove `child_id` from whichever parent's children list contains it.
pub fn remove_child_from_parent(&mut self, child_id: &str) {
    for node in &mut self.nodes {
        if let Some(children) = Self::children_of_kind_mut(&mut node.kind) {
            if let Some(pos) = children.iter().position(|c| c == child_id) {
                children.remove(pos);
                return;
            }
        }
    }
}
```

- [ ] **Step 11: Run tests**

Run: `cargo test -p ode-format find_parent remove_child`
Expected: PASS

- [ ] **Step 12: Write test for `collect_descendants`**

```rust
#[test]
fn collect_descendants_returns_all_nested() {
    let mut wire = make_test_wire();
    // Add a group child under frame-1 with its own child
    wire.nodes.push(NodeWire {
        stable_id: "group-1".to_string(),
        name: "Group".to_string(),
        transform: Transform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        visible: true,
        constraints: None,
        layout_sizing: None,
        kind: NodeKindWire::Group(GroupDataWire {
            children: vec!["text-1".to_string()],
        }),
    });
    // Re-parent text-1 under group-1, group-1 under frame-1
    if let Some(n) = wire.find_node_mut("frame-1") {
        if let NodeKindWire::Frame(d) = &mut n.kind {
            d.container.children = vec!["group-1".to_string()];
        }
    }
    let desc = wire.collect_descendants("frame-1");
    assert_eq!(desc.len(), 2);
    assert!(desc.contains(&"group-1".to_string()));
    assert!(desc.contains(&"text-1".to_string()));
}

#[test]
fn collect_descendants_of_leaf_is_empty() {
    let wire = make_test_wire();
    assert!(wire.collect_descendants("text-1").is_empty());
}
```

- [ ] **Step 13: Implement `collect_descendants`**

```rust
/// Collect all descendant stable_ids recursively.
pub fn collect_descendants(&self, stable_id: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut stack = vec![stable_id.to_string()];
    while let Some(id) = stack.pop() {
        if let Some(node) = self.find_node(&id) {
            let children: &[String] = match &node.kind {
                NodeKindWire::Frame(d) => &d.container.children,
                NodeKindWire::Group(d) => &d.children,
                NodeKindWire::BooleanOp(d) => &d.children,
                NodeKindWire::Instance(d) => &d.container.children,
                _ => &[],
            };
            for child_id in children {
                result.push(child_id.clone());
                stack.push(child_id.clone());
            }
        }
    }
    result
}
```

- [ ] **Step 14: Run all wire helper tests**

Run: `cargo test -p ode-format -- collect_descendants is_container find_node find_parent remove_child`
Expected: all PASS

- [ ] **Step 15: Write test for `visual_props_mut`**

```rust
#[test]
fn visual_props_mut_works_for_frame_and_text() {
    let mut wire = make_test_wire();
    // Frame has visual
    assert!(DocumentWire::visual_props_mut(&mut wire.find_node_mut("frame-1").unwrap().kind).is_some());
    // Text has visual
    assert!(DocumentWire::visual_props_mut(&mut wire.find_node_mut("text-1").unwrap().kind).is_some());
}
```

- [ ] **Step 16: Implement `visual_props_mut`**

```rust
/// Get mutable reference to a node kind's VisualProps (if it has one).
pub fn visual_props_mut(kind: &mut NodeKindWire) -> Option<&mut VisualProps> {
    match kind {
        NodeKindWire::Frame(d) => Some(&mut d.visual),
        NodeKindWire::Vector(d) => Some(&mut d.visual),
        NodeKindWire::BooleanOp(d) => Some(&mut d.visual),
        NodeKindWire::Text(d) => Some(&mut d.visual),
        NodeKindWire::Image(d) => Some(&mut d.visual),
        NodeKindWire::Group(_) | NodeKindWire::Instance(_) => None,
    }
}
```

- [ ] **Step 17: Run test and verify pass**

Run: `cargo test -p ode-format visual_props_mut`
Expected: PASS

- [ ] **Step 18: Commit**

```bash
git add crates/ode-format/src/wire.rs
git commit -m "feat(ode-format): add wire mutation helpers — find, insert, remove, container checks"
```

---

### Task 2: Shape Preset Generators

**Files:**
- Create: `crates/ode-format/src/shapes.rs`
- Modify: `crates/ode-format/src/lib.rs`

- [ ] **Step 1: Write tests for rect and ellipse shapes**

Create `crates/ode-format/src/shapes.rs`:

```rust
//! Preset shape path generators for `ode add vector --shape <preset>`.

use crate::node::{PathSegment, VectorPath};

/// Generate a rectangle path.
pub fn rect(width: f32, height: f32) -> VectorPath {
    todo!()
}

/// Generate a rounded rectangle path.
/// `radii` is [top-left, top-right, bottom-right, bottom-left].
pub fn rounded_rect(width: f32, height: f32, radii: [f32; 4]) -> VectorPath {
    todo!()
}

/// Generate an ellipse path (approximated with 4 cubic Bézier curves).
pub fn ellipse(width: f32, height: f32) -> VectorPath {
    todo!()
}

/// Generate a horizontal line.
pub fn line(width: f32) -> VectorPath {
    todo!()
}

/// Generate a 5-pointed star inscribed in a circle of given diameter.
pub fn star(diameter: f32) -> VectorPath {
    todo!()
}

/// Generate a regular polygon with N sides inscribed in a circle of given diameter.
pub fn polygon(sides: u32, diameter: f32) -> VectorPath {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_produces_4_lines_closed() {
        let path = rect(100.0, 50.0);
        assert!(path.closed);
        // MoveTo + 3 LineTo = 4 segments (close is implicit via path.closed)
        assert_eq!(path.segments.len(), 4);
        assert!(matches!(path.segments[0], PathSegment::MoveTo { x, y } if x == 0.0 && y == 0.0));
    }

    #[test]
    fn ellipse_produces_curves() {
        let path = ellipse(100.0, 80.0);
        assert!(path.closed);
        let curve_count = path.segments.iter().filter(|s| matches!(s, PathSegment::CurveTo { .. })).count();
        assert_eq!(curve_count, 4, "Ellipse should have 4 cubic curves");
    }

    #[test]
    fn line_is_not_closed() {
        let path = line(200.0);
        assert!(!path.closed);
        assert_eq!(path.segments.len(), 2); // MoveTo + LineTo
    }

    #[test]
    fn star_has_10_points() {
        let path = star(100.0);
        assert!(path.closed);
        // 5-pointed star: MoveTo + 9 LineTo = 10 segments
        assert_eq!(path.segments.len(), 10);
    }

    #[test]
    fn polygon_hexagon() {
        let path = polygon(6, 100.0);
        assert!(path.closed);
        // MoveTo + 5 LineTo = 6 segments
        assert_eq!(path.segments.len(), 6);
    }

    #[test]
    fn rounded_rect_with_zero_radii_equals_rect() {
        let r = rounded_rect(100.0, 50.0, [0.0; 4]);
        let plain = rect(100.0, 50.0);
        assert_eq!(r.segments.len(), plain.segments.len());
    }

    #[test]
    fn rounded_rect_with_radii_has_curves() {
        let path = rounded_rect(100.0, 50.0, [8.0, 8.0, 8.0, 8.0]);
        let curve_count = path.segments.iter().filter(|s| matches!(s, PathSegment::CurveTo { .. })).count();
        assert_eq!(curve_count, 4, "Should have 4 corner curves");
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/ode-format/src/lib.rs`, add after `pub mod wire;`:

```rust
pub mod shapes;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ode-format shapes`
Expected: FAIL — all `todo!()`

- [ ] **Step 4: Implement all shape generators**

Replace the `todo!()` bodies:

```rust
pub fn rect(width: f32, height: f32) -> VectorPath {
    VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: width, y: 0.0 },
            PathSegment::LineTo { x: width, y: height },
            PathSegment::LineTo { x: 0.0, y: height },
        ],
        closed: true,
    }
}

pub fn rounded_rect(width: f32, height: f32, radii: [f32; 4]) -> VectorPath {
    let [tl, tr, br, bl] = radii;
    // If all zero, return plain rect
    if tl == 0.0 && tr == 0.0 && br == 0.0 && bl == 0.0 {
        return rect(width, height);
    }
    // Clamp radii to half of smallest dimension
    let max_r = (width / 2.0).min(height / 2.0);
    let tl = tl.min(max_r);
    let tr = tr.min(max_r);
    let br = br.min(max_r);
    let bl = bl.min(max_r);
    // Kappa for 90-degree circular arc approximation
    let k = 0.552_284_75;
    let mut segs = Vec::new();
    // Start at top-left after the TL corner
    segs.push(PathSegment::MoveTo { x: tl, y: 0.0 });
    // Top edge → TR corner
    segs.push(PathSegment::LineTo { x: width - tr, y: 0.0 });
    if tr > 0.0 {
        segs.push(PathSegment::CurveTo {
            x1: width - tr + tr * k, y1: 0.0,
            x2: width, y2: tr - tr * k,
            x: width, y: tr,
        });
    }
    // Right edge → BR corner
    segs.push(PathSegment::LineTo { x: width, y: height - br });
    if br > 0.0 {
        segs.push(PathSegment::CurveTo {
            x1: width, y1: height - br + br * k,
            x2: width - br + br * k, y2: height,
            x: width - br, y: height,
        });
    }
    // Bottom edge → BL corner
    segs.push(PathSegment::LineTo { x: bl, y: height });
    if bl > 0.0 {
        segs.push(PathSegment::CurveTo {
            x1: bl - bl * k, y1: height,
            x2: 0.0, y2: height - bl + bl * k,
            x: 0.0, y: height - bl,
        });
    }
    // Left edge → TL corner
    segs.push(PathSegment::LineTo { x: 0.0, y: tl });
    if tl > 0.0 {
        segs.push(PathSegment::CurveTo {
            x1: 0.0, y1: tl - tl * k,
            x2: tl - tl * k, y2: 0.0,
            x: tl, y: 0.0,
        });
    }
    VectorPath { segments: segs, closed: true }
}

pub fn ellipse(width: f32, height: f32) -> VectorPath {
    let rx = width / 2.0;
    let ry = height / 2.0;
    let cx = rx;
    let cy = ry;
    let k = 0.552_284_75;
    VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: cx + rx, y: cy },
            PathSegment::CurveTo {
                x1: cx + rx, y1: cy + ry * k,
                x2: cx + rx * k, y2: cy + ry,
                x: cx, y: cy + ry,
            },
            PathSegment::CurveTo {
                x1: cx - rx * k, y1: cy + ry,
                x2: cx - rx, y2: cy + ry * k,
                x: cx - rx, y: cy,
            },
            PathSegment::CurveTo {
                x1: cx - rx, y1: cy - ry * k,
                x2: cx - rx * k, y2: cy - ry,
                x: cx, y: cy - ry,
            },
            PathSegment::CurveTo {
                x1: cx + rx * k, y1: cy - ry,
                x2: cx + rx, y2: cy - ry * k,
                x: cx + rx, y: cy,
            },
        ],
        closed: true,
    }
}

pub fn line(width: f32) -> VectorPath {
    VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: width, y: 0.0 },
        ],
        closed: false,
    }
}

pub fn star(diameter: f32) -> VectorPath {
    let r_outer = diameter / 2.0;
    let r_inner = r_outer * 0.381_966; // golden ratio based inner radius
    let cx = r_outer;
    let cy = r_outer;
    let mut segs = Vec::new();
    for i in 0..10 {
        let angle = std::f32::consts::FRAC_PI_2 * -1.0 + std::f32::consts::PI * i as f32 / 5.0;
        let r = if i % 2 == 0 { r_outer } else { r_inner };
        let x = cx + r * angle.cos();
        let y = cy + r * angle.sin();
        if i == 0 {
            segs.push(PathSegment::MoveTo { x, y });
        } else {
            segs.push(PathSegment::LineTo { x, y });
        }
    }
    VectorPath { segments: segs, closed: true }
}

pub fn polygon(sides: u32, diameter: f32) -> VectorPath {
    let sides = sides.max(3);
    let r = diameter / 2.0;
    let cx = r;
    let cy = r;
    let mut segs = Vec::new();
    for i in 0..sides {
        let angle = std::f32::consts::FRAC_PI_2 * -1.0
            + 2.0 * std::f32::consts::PI * i as f32 / sides as f32;
        let x = cx + r * angle.cos();
        let y = cy + r * angle.sin();
        if i == 0 {
            segs.push(PathSegment::MoveTo { x, y });
        } else {
            segs.push(PathSegment::LineTo { x, y });
        }
    }
    VectorPath { segments: segs, closed: true }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ode-format shapes`
Expected: all PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/src/shapes.rs crates/ode-format/src/lib.rs
git commit -m "feat(ode-format): add shape preset generators — rect, ellipse, line, star, polygon"
```

---

### Task 3: Extend `Color::from_hex` for `#RGB`

**Files:**
- Modify: `crates/ode-format/src/color.rs`

- [ ] **Step 1: Write test for 3-char hex**

Add to existing test module in `color.rs`:

```rust
#[test]
fn from_hex_3_char() {
    let c = Color::from_hex("#F00").unwrap();
    if let Color::Srgb { r, g, b, a } = c {
        assert!((r - 1.0).abs() < 0.01);
        assert!((g - 0.0).abs() < 0.01);
        assert!((b - 0.0).abs() < 0.01);
        assert!((a - 1.0).abs() < 0.01);
    } else {
        panic!("Expected Srgb");
    }
}

#[test]
fn from_hex_3_char_mixed() {
    let c = Color::from_hex("#ABC").unwrap();
    if let Color::Srgb { r, g, b, .. } = c {
        assert!((r - 0xAA as f32 / 255.0).abs() < 0.01);
        assert!((g - 0xBB as f32 / 255.0).abs() < 0.01);
        assert!((b - 0xCC as f32 / 255.0).abs() < 0.01);
    } else {
        panic!("Expected Srgb");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-format from_hex_3_char`
Expected: FAIL — returns `None` for 3-char hex.

- [ ] **Step 3: Add 3-char hex support**

In `color.rs`, modify the `from_hex` match to add a `3` arm before the existing `_ => return None`:

```rust
3 => {
    let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
    let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
    let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
    (r * 17, g * 17, b * 17, 255u8) // 0xA → 0xAA
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ode-format from_hex_3_char`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ode-format/src/color.rs
git commit -m "feat(ode-format): extend Color::from_hex to support 3-char #RGB shorthand"
```

---

### Task 4: Output Response Types

**Files:**
- Modify: `crates/ode-cli/src/output.rs`

- [ ] **Step 1: Add mutation response structs**

Append before `// ─── Print helpers ───` in `output.rs`:

```rust
// ─── Mutation responses ───

#[derive(Serialize)]
pub struct AddResponse {
    pub status: &'static str,
    pub stable_id: String,
    pub name: String,
    pub kind: String,
    pub parent: String,
}

#[derive(Serialize)]
pub struct SetResponse {
    pub status: &'static str,
    pub stable_id: String,
    pub modified: Vec<String>,
}

#[derive(Serialize)]
pub struct DeleteResponse {
    pub status: &'static str,
    pub deleted: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

#[derive(Serialize)]
pub struct MoveResponse {
    pub status: &'static str,
    pub stable_id: String,
    pub new_parent: String,
    pub index: usize,
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p ode-cli`
Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/ode-cli/src/output.rs
git commit -m "feat(ode-cli): add mutation response types — Add, Set, Delete, Move"
```

---

## Chunk 2: Commands

### File Structure (Chunk 2)

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/ode-cli/src/mutate.rs` | Create | All mutation command logic (add, set, delete, move) |
| `crates/ode-cli/src/main.rs` | Modify | Register new subcommands with clap |

---

### Task 5: `ode add` Command

**Files:**
- Create: `crates/ode-cli/src/mutate.rs`
- Modify: `crates/ode-cli/src/main.rs`

- [ ] **Step 1: Create `mutate.rs` with shared load/save helpers and `cmd_add`**

Create `crates/ode-cli/src/mutate.rs`:

```rust
use crate::output::*;
use ode_format::wire::{
    ContainerPropsWire, DocumentWire, FrameDataWire, GroupDataWire, ImageDataWire, NodeKindWire,
    NodeWire, TextDataWire,
};
use ode_format::{
    BlendMode, Color, Fill, Paint, SizingMode, Stroke, StyleValue, VectorPath, VisualProps,
};
use ode_format::node::{Transform, VectorData};
use ode_format::style::StrokePosition;
use ode_format::typography::{TextSizingMode, TextStyle};

// ─── Shared load/save ───

fn load_wire(file: &str) -> Result<(String, DocumentWire), (i32, ErrorResponse)> {
    let json = crate::commands::load_input(file)?;
    let wire: DocumentWire = serde_json::from_str(&json).map_err(|e| {
        (
            EXIT_INPUT,
            ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()),
        )
    })?;
    Ok((file.to_string(), wire))
}

fn save_wire(file: &str, wire: &DocumentWire) -> Result<(), (i32, ErrorResponse)> {
    let json = serde_json::to_string_pretty(wire).map_err(|e| {
        (
            EXIT_INTERNAL,
            ErrorResponse::new("INTERNAL", "serialize", &e.to_string()),
        )
    })?;
    std::fs::write(file, &json).map_err(|e| {
        (
            EXIT_IO,
            ErrorResponse::new("IO_ERROR", "io", &e.to_string()),
        )
    })?;
    Ok(())
}

fn parse_color(s: &str) -> Result<Color, String> {
    Color::from_hex(s).ok_or_else(|| format!("invalid color: {s}"))
}

fn make_solid_fill(color: Color) -> Fill {
    Fill {
        paint: Paint::Solid {
            color: StyleValue::Raw(color),
        },
        opacity: StyleValue::Raw(1.0),
        blend_mode: BlendMode::Normal,
        visible: true,
    }
}

// ─── ode add ───

pub fn cmd_add(
    kind: &str,
    file: &str,
    name: Option<&str>,
    parent: Option<&str>,
    index: Option<usize>,
    width: Option<f32>,
    height: Option<f32>,
    fill: Option<&str>,
    corner_radius: Option<&str>,
    clips_content: Option<bool>,
    content: Option<&str>,
    font_size: Option<f32>,
    font_family: Option<&str>,
    shape: Option<&str>,
    sides: Option<u32>,
    src: Option<&str>,
) -> i32 {
    let (file, mut wire) = match load_wire(file) {
        Ok(v) => v,
        Err((code, err)) => { print_json(&err); return code; }
    };

    let stable_id = nanoid::nanoid!();

    // Build the node
    let (node_name, node_kind) = match kind {
        "frame" => {
            let n = name.unwrap_or("Frame").to_string();
            let w = match width {
                Some(v) => v,
                None => { print_json(&ErrorResponse::new("MISSING_ARG", "add", "--width is required for frame")); return EXIT_INPUT; }
            };
            let h = match height {
                Some(v) => v,
                None => { print_json(&ErrorResponse::new("MISSING_ARG", "add", "--height is required for frame")); return EXIT_INPUT; }
            };
            let mut visual = VisualProps::default();
            if let Some(fill_str) = fill {
                match parse_color(fill_str) {
                    Ok(c) => visual.fills.push(make_solid_fill(c)),
                    Err(e) => { print_json(&ErrorResponse::new("INVALID_VALUE", "add", &e)); return EXIT_INPUT; }
                }
            }
            let cr = match corner_radius {
                Some(s) => parse_corner_radius(s),
                None => [0.0; 4],
            };
            let kind = NodeKindWire::Frame(FrameDataWire {
                width: w, height: h,
                width_sizing: SizingMode::Fixed,
                height_sizing: SizingMode::Fixed,
                corner_radius: cr,
                clips_content: clips_content.unwrap_or(true),
                visual,
                container: ContainerPropsWire::default(),
                component_def: None,
            });
            (n, kind)
        }
        "group" => {
            let n = name.unwrap_or("Group").to_string();
            (n, NodeKindWire::Group(GroupDataWire { children: vec![] }))
        }
        "text" => {
            let text_content = match content {
                Some(c) => c.to_string(),
                None => { print_json(&ErrorResponse::new("MISSING_ARG", "add", "--content is required for text")); return EXIT_INPUT; }
            };
            let n = name.unwrap_or("Text").to_string();
            let mut visual = VisualProps::default();
            if let Some(fill_str) = fill {
                match parse_color(fill_str) {
                    Ok(c) => visual.fills.push(make_solid_fill(c)),
                    Err(e) => { print_json(&ErrorResponse::new("INVALID_VALUE", "add", &e)); return EXIT_INPUT; }
                }
            }
            let mut style = TextStyle::default();
            if let Some(fs) = font_size {
                style.font_size = StyleValue::Raw(fs);
            }
            if let Some(ff) = font_family {
                style.font_family = StyleValue::Raw(ff.to_string());
            }
            let w = width.unwrap_or(100.0);
            let h = height.unwrap_or(100.0);
            let kind = NodeKindWire::Text(TextDataWire {
                visual,
                content: text_content,
                runs: vec![],
                default_style: style,
                width: w, height: h,
                sizing_mode: TextSizingMode::Fixed,
            });
            (n, kind)
        }
        "vector" => {
            let shape_name = match shape {
                Some(s) => s,
                None => { print_json(&ErrorResponse::new("MISSING_ARG", "add", "--shape is required for vector")); return EXIT_INPUT; }
            };
            let w = width.unwrap_or(100.0);
            let h = height.unwrap_or(100.0);
            let default_name = match shape_name {
                "rect" => "Rectangle",
                "ellipse" => "Ellipse",
                "line" => "Line",
                "star" => "Star",
                "polygon" => "Polygon",
                _ => "Vector",
            };
            let n = name.unwrap_or(default_name).to_string();
            let path = match shape_name {
                "rect" => {
                    match corner_radius {
                        Some(cr) => ode_format::shapes::rounded_rect(w, h, parse_corner_radius(cr)),
                        None => ode_format::shapes::rect(w, h),
                    }
                }
                "ellipse" => ode_format::shapes::ellipse(w, h),
                "line" => ode_format::shapes::line(w),
                "star" => ode_format::shapes::star(w),
                "polygon" => ode_format::shapes::polygon(sides.unwrap_or(5), w),
                _ => {
                    print_json(&ErrorResponse::new("INVALID_VALUE", "add", &format!("unknown shape: {shape_name}")));
                    return EXIT_INPUT;
                }
            };
            let mut visual = VisualProps::default();
            if let Some(fill_str) = fill {
                match parse_color(fill_str) {
                    Ok(c) => visual.fills.push(make_solid_fill(c)),
                    Err(e) => { print_json(&ErrorResponse::new("INVALID_VALUE", "add", &e)); return EXIT_INPUT; }
                }
            }
            let kind = NodeKindWire::Vector(Box::new(VectorData {
                visual,
                path,
                fill_rule: Default::default(),
            }));
            (n, kind)
        }
        "image" => {
            let w = match width {
                Some(v) => v,
                None => { print_json(&ErrorResponse::new("MISSING_ARG", "add", "--width is required for image")); return EXIT_INPUT; }
            };
            let h = match height {
                Some(v) => v,
                None => { print_json(&ErrorResponse::new("MISSING_ARG", "add", "--height is required for image")); return EXIT_INPUT; }
            };
            let n = name.unwrap_or("Image").to_string();
            let source = src.map(|p| ode_format::style::ImageSource::Linked { path: p.to_string() });
            let kind = NodeKindWire::Image(ImageDataWire {
                visual: VisualProps::default(),
                source,
                width: w, height: h,
            });
            (n, kind)
        }
        _ => {
            print_json(&ErrorResponse::new("INVALID_VALUE", "add", &format!("unknown kind: {kind}. Expected: frame, group, text, vector, image")));
            return EXIT_INPUT;
        }
    };

    let node = NodeWire {
        stable_id: stable_id.clone(),
        name: node_name.clone(),
        transform: Transform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        visible: true,
        constraints: None,
        layout_sizing: None,
        kind: node_kind,
    };

    wire.nodes.push(node);

    // Insert into parent
    let parent_label = match parent {
        Some("root") | None if wire.canvas.is_empty() => {
            // Add as canvas root
            let pos = index.unwrap_or(wire.canvas.len());
            let pos = pos.min(wire.canvas.len());
            wire.canvas.insert(pos, stable_id.clone());
            "root".to_string()
        }
        None => {
            // Add to first canvas root's children
            let first_root_id = wire.canvas[0].clone();
            let first_root = match wire.find_node_mut(&first_root_id) {
                Some(n) => n,
                None => {
                    print_json(&ErrorResponse::new("INTERNAL", "add", "canvas root not found in nodes"));
                    return EXIT_INTERNAL;
                }
            };
            if let Some(children) = DocumentWire::children_of_kind_mut(&mut first_root.kind) {
                let pos = index.unwrap_or(children.len());
                let pos = pos.min(children.len());
                children.insert(pos, stable_id.clone());
            } else {
                print_json(&ErrorResponse::new("NOT_CONTAINER", "add", "canvas root is not a container"));
                return EXIT_INPUT;
            }
            first_root_id
        }
        Some("root") => {
            let pos = index.unwrap_or(wire.canvas.len());
            let pos = pos.min(wire.canvas.len());
            wire.canvas.insert(pos, stable_id.clone());
            "root".to_string()
        }
        Some(parent_id) => {
            let target = match wire.find_node_mut(parent_id) {
                Some(n) => n,
                None => {
                    print_json(&ErrorResponse::new("NOT_FOUND", "add", &format!("parent '{parent_id}' not found")));
                    return EXIT_INPUT;
                }
            };
            if !DocumentWire::is_container(&target.kind) {
                print_json(&ErrorResponse::new("NOT_CONTAINER", "add", &format!("'{parent_id}' is not a container")));
                return EXIT_INPUT;
            }
            let children = DocumentWire::children_of_kind_mut(&mut target.kind).unwrap();
            let pos = index.unwrap_or(children.len());
            let pos = pos.min(children.len());
            children.insert(pos, stable_id.clone());
            parent_id.to_string()
        }
    };

    if let Err((code, err)) = save_wire(&file, &wire) {
        print_json(&err);
        return code;
    }

    print_json(&AddResponse {
        status: "ok",
        stable_id,
        name: node_name,
        kind: kind.to_string(),
        parent: parent_label,
    });
    EXIT_OK
}

fn parse_corner_radius(s: &str) -> [f32; 4] {
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match parts.len() {
        1 => [parts[0]; 4],
        4 => [parts[0], parts[1], parts[2], parts[3]],
        _ => [0.0; 4],
    }
}
```

- [ ] **Step 2: Register `mod mutate` and the `Add` subcommand in `main.rs`**

In `main.rs`, add `mod mutate;` after existing mod declarations.

Add to the `Command` enum:

```rust
/// Add a node to a document
Add {
    /// Node kind: frame, group, text, vector, image
    kind: String,
    /// Document file path
    file: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    parent: Option<String>,
    #[arg(long)]
    index: Option<usize>,
    #[arg(long)]
    width: Option<f32>,
    #[arg(long)]
    height: Option<f32>,
    #[arg(long)]
    fill: Option<String>,
    #[arg(long, value_name = "R or TL,TR,BR,BL")]
    corner_radius: Option<String>,
    #[arg(long)]
    clips_content: Option<bool>,
    #[arg(long)]
    content: Option<String>,
    #[arg(long)]
    font_size: Option<f32>,
    #[arg(long)]
    font_family: Option<String>,
    #[arg(long)]
    shape: Option<String>,
    #[arg(long)]
    sides: Option<u32>,
    #[arg(long)]
    src: Option<String>,
},
```

Add the match arm in `main()`:

```rust
Command::Add { kind, file, name, parent, index, width, height, fill, corner_radius, clips_content, content, font_size, font_family, shape, sides, src } => {
    mutate::cmd_add(&kind, &file, name.as_deref(), parent.as_deref(), index, width, height, fill.as_deref(), corner_radius.as_deref(), clips_content, content.as_deref(), font_size, font_family.as_deref(), shape.as_deref(), sides, src.as_deref())
}
```

- [ ] **Step 3: Add `nanoid` dependency to ode-cli and `Default` to `LayoutConfig`**

In `crates/ode-cli/Cargo.toml`, add under `[dependencies]`:

```toml
nanoid = { workspace = true }
```

In `crates/ode-format/src/node.rs`, add `Default` to `LayoutConfig`'s derive:

```rust
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct LayoutConfig {
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p ode-cli`
Expected: compiles.

- [ ] **Step 5: Write integration test**

Create `crates/ode-cli/tests/add_test.rs`:

```rust
use std::process::Command;

fn ode_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ode"))
}

#[test]
fn add_frame_to_new_document() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");

    // Create empty doc with root frame
    let out = ode_cmd()
        .args(["new", file.to_str().unwrap(), "--width", "800", "--height", "600"])
        .output().unwrap();
    assert!(out.status.success());

    // Add a child frame
    let out = ode_cmd()
        .args(["add", "frame", file.to_str().unwrap(), "--name", "Card", "--width", "320", "--height", "200"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["status"], "ok");
    assert_eq!(resp["kind"], "frame");
    assert_eq!(resp["name"], "Card");
    assert!(!resp["stable_id"].as_str().unwrap().is_empty());
}

#[test]
fn add_text_with_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");
    ode_cmd().args(["new", file.to_str().unwrap(), "--width", "800", "--height", "600"]).output().unwrap();

    let out = ode_cmd()
        .args(["add", "text", file.to_str().unwrap(), "--content", "Hello World"])
        .output().unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["name"], "Text"); // default name
}

#[test]
fn add_vector_rect_with_fill() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");
    ode_cmd().args(["new", file.to_str().unwrap(), "--width", "800", "--height", "600"]).output().unwrap();

    let out = ode_cmd()
        .args(["add", "vector", file.to_str().unwrap(), "--shape", "rect", "--width", "48", "--height", "48", "--fill", "#3B82F6"])
        .output().unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["name"], "Rectangle");
}

#[test]
fn add_to_empty_canvas_creates_root() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");
    // Create doc WITHOUT root frame (empty canvas)
    ode_cmd().args(["new", file.to_str().unwrap()]).output().unwrap();

    let out = ode_cmd()
        .args(["add", "frame", file.to_str().unwrap(), "--name", "Root", "--width", "800", "--height", "600"])
        .output().unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["parent"], "root");
}

#[test]
fn add_to_non_container_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");
    ode_cmd().args(["new", file.to_str().unwrap(), "--width", "800", "--height", "600"]).output().unwrap();

    // Add a text node
    let out = ode_cmd()
        .args(["add", "text", file.to_str().unwrap(), "--content", "Hi"])
        .output().unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text_id = resp["stable_id"].as_str().unwrap().to_string();

    // Try to add child to text (should fail)
    let out = ode_cmd()
        .args(["add", "frame", file.to_str().unwrap(), "--name", "Bad", "--width", "10", "--height", "10", "--parent", &text_id])
        .output().unwrap();
    assert!(!out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["code"], "NOT_CONTAINER");
}
```

- [ ] **Step 6: Add `tempfile` dev-dependency**

In `crates/ode-cli/Cargo.toml`, add:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 7: Run integration tests**

Run: `cargo test -p ode-cli --test add_test`
Expected: all PASS

- [ ] **Step 8: Commit**

```bash
git add crates/ode-cli/
git commit -m "feat(ode-cli): implement ode add command — frame, group, text, vector, image"
```

---

### Task 6: `ode set` Command

**Files:**
- Modify: `crates/ode-cli/src/mutate.rs`
- Modify: `crates/ode-cli/src/main.rs`

- [ ] **Step 1: Add `cmd_set` to `mutate.rs`**

Append to `mutate.rs`:

```rust
// ─── ode set ───

pub fn cmd_set(
    file: &str,
    stable_id: &str,
    name: Option<&str>,
    visible: Option<bool>,
    opacity: Option<f32>,
    blend_mode: Option<&str>,
    x: Option<f32>,
    y: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
    fill: Option<&str>,
    fill_opacity: Option<f32>,
    stroke: Option<&str>,
    stroke_width: Option<f32>,
    stroke_position: Option<&str>,
    corner_radius: Option<&str>,
    clips_content: Option<bool>,
    layout: Option<&str>,
    padding: Option<&str>,
    gap: Option<f32>,
    content: Option<&str>,
    font_size: Option<f32>,
    font_family: Option<&str>,
    font_weight: Option<u16>,
    text_align: Option<&str>,
    line_height: Option<&str>,
) -> i32 {
    let (file, mut wire) = match load_wire(file) {
        Ok(v) => v,
        Err((code, err)) => { print_json(&err); return code; }
    };

    let node = match wire.find_node_mut(stable_id) {
        Some(n) => n,
        None => {
            print_json(&ErrorResponse::new("NOT_FOUND", "set", &format!("node '{stable_id}' not found")));
            return EXIT_INPUT;
        }
    };

    let mut modified = Vec::new();

    // Common properties
    if let Some(v) = name { node.name = v.to_string(); modified.push("name"); }
    if let Some(v) = visible { node.visible = v; modified.push("visible"); }
    if let Some(v) = opacity { node.opacity = v.clamp(0.0, 1.0); modified.push("opacity"); }
    if let Some(v) = blend_mode {
        match parse_blend_mode(v) {
            Some(bm) => { node.blend_mode = bm; modified.push("blend-mode"); }
            None => { print_json(&ErrorResponse::new("INVALID_VALUE", "set", &format!("unknown blend mode: {v}"))); return EXIT_INPUT; }
        }
    }
    if let Some(v) = x { node.transform.tx = v; modified.push("x"); }
    if let Some(v) = y { node.transform.ty = v; modified.push("y"); }

    // Size properties
    if width.is_some() || height.is_some() {
        match &mut node.kind {
            NodeKindWire::Frame(d) => {
                if let Some(w) = width { d.width = w; modified.push("width"); }
                if let Some(h) = height { d.height = h; modified.push("height"); }
            }
            NodeKindWire::Text(d) => {
                if let Some(w) = width { d.width = w; modified.push("width"); }
                if let Some(h) = height { d.height = h; modified.push("height"); }
            }
            NodeKindWire::Image(d) => {
                if let Some(w) = width { d.width = w; modified.push("width"); }
                if let Some(h) = height { d.height = h; modified.push("height"); }
            }
            _ => {
                print_json(&ErrorResponse::new("INVALID_PROPERTY", "set", "width/height not applicable to this node type"));
                return EXIT_INPUT;
            }
        }
    }

    // Visual properties
    if fill.is_some() || fill_opacity.is_some() || stroke.is_some() || stroke_width.is_some() || stroke_position.is_some() {
        let visual = match DocumentWire::visual_props_mut(&mut node.kind) {
            Some(v) => v,
            None => {
                print_json(&ErrorResponse::new("INVALID_PROPERTY", "set", "visual properties not applicable to this node type"));
                return EXIT_INPUT;
            }
        };
        if let Some(fill_str) = fill {
            match parse_color(fill_str) {
                Ok(c) => {
                    let fill_obj = make_solid_fill(c);
                    if visual.fills.is_empty() {
                        visual.fills.push(fill_obj);
                    } else {
                        visual.fills[0] = fill_obj;
                    }
                    modified.push("fill");
                }
                Err(e) => { print_json(&ErrorResponse::new("INVALID_VALUE", "set", &e)); return EXIT_INPUT; }
            }
        }
        if let Some(fo) = fill_opacity {
            if let Some(f) = visual.fills.first_mut() {
                f.opacity = StyleValue::Raw(fo.clamp(0.0, 1.0));
                modified.push("fill-opacity");
            }
        }
        if let Some(stroke_str) = stroke {
            match parse_color(stroke_str) {
                Ok(c) => {
                    let stroke_obj = Stroke {
                        paint: Paint::Solid { color: StyleValue::Raw(c) },
                        width: StyleValue::Raw(stroke_width.unwrap_or(1.0)),
                        position: stroke_position.and_then(parse_stroke_position).unwrap_or(StrokePosition::Center),
                        cap: ode_format::style::StrokeCap::Butt,
                        join: ode_format::style::StrokeJoin::Miter,
                        miter_limit: 4.0,
                        dash: None,
                        opacity: StyleValue::Raw(1.0),
                        blend_mode: BlendMode::Normal,
                        visible: true,
                    };
                    if visual.strokes.is_empty() {
                        visual.strokes.push(stroke_obj);
                    } else {
                        visual.strokes[0] = stroke_obj;
                    }
                    modified.push("stroke");
                }
                Err(e) => { print_json(&ErrorResponse::new("INVALID_VALUE", "set", &e)); return EXIT_INPUT; }
            }
        } else {
            // stroke-width and stroke-position without --stroke: modify existing
            if let Some(sw) = stroke_width {
                if let Some(s) = visual.strokes.first_mut() {
                    s.width = StyleValue::Raw(sw);
                    modified.push("stroke-width");
                }
            }
            if let Some(sp) = stroke_position {
                if let Some(pos) = parse_stroke_position(sp) {
                    if let Some(s) = visual.strokes.first_mut() {
                        s.position = pos;
                        modified.push("stroke-position");
                    }
                }
            }
        }
    }

    // Frame-specific
    if corner_radius.is_some() || clips_content.is_some() || layout.is_some() || padding.is_some() || gap.is_some() {
        match &mut node.kind {
            NodeKindWire::Frame(d) => {
                if let Some(cr) = corner_radius {
                    d.corner_radius = parse_corner_radius(cr);
                    modified.push("corner-radius");
                }
                if let Some(cc) = clips_content {
                    d.clips_content = cc;
                    modified.push("clips-content");
                }
                if let Some(dir_str) = layout {
                    use ode_format::node::{LayoutConfig, LayoutDirection};
                    let direction = match dir_str {
                        "horizontal" => LayoutDirection::Horizontal,
                        "vertical" => LayoutDirection::Vertical,
                        _ => {
                            print_json(&ErrorResponse::new("INVALID_VALUE", "set", &format!("unknown layout direction: {dir_str}")));
                            return EXIT_INPUT;
                        }
                    };
                    let mut config = d.container.layout.clone().unwrap_or_default();
                    config.direction = direction;
                    if let Some(g) = gap { config.item_spacing = g; }
                    if let Some(p) = padding { config.padding = parse_padding(p); }
                    d.container.layout = Some(config);
                    modified.push("layout");
                } else {
                    if let Some(g) = gap {
                        if let Some(ref mut config) = d.container.layout {
                            config.item_spacing = g;
                            modified.push("gap");
                        }
                    }
                    if let Some(p) = padding {
                        if let Some(ref mut config) = d.container.layout {
                            config.padding = parse_padding(p);
                            modified.push("padding");
                        }
                    }
                }
            }
            _ => {
                print_json(&ErrorResponse::new("INVALID_PROPERTY", "set", "frame-specific properties not applicable to this node type"));
                return EXIT_INPUT;
            }
        }
    }

    // Text-specific
    if content.is_some() || font_size.is_some() || font_family.is_some() || font_weight.is_some() || text_align.is_some() || line_height.is_some() {
        match &mut node.kind {
            NodeKindWire::Text(d) => {
                if let Some(c) = content { d.content = c.to_string(); modified.push("content"); }
                if let Some(fs) = font_size { d.default_style.font_size = StyleValue::Raw(fs); modified.push("font-size"); }
                if let Some(ff) = font_family { d.default_style.font_family = StyleValue::Raw(ff.to_string()); modified.push("font-family"); }
                if let Some(fw) = font_weight { d.default_style.font_weight = StyleValue::Raw(fw); modified.push("font-weight"); }
                if let Some(ta) = text_align {
                    use ode_format::typography::TextAlign;
                    let align = match ta {
                        "left" => TextAlign::Left,
                        "center" => TextAlign::Center,
                        "right" => TextAlign::Right,
                        "justify" => TextAlign::Justify,
                        _ => { print_json(&ErrorResponse::new("INVALID_VALUE", "set", &format!("unknown text-align: {ta}"))); return EXIT_INPUT; }
                    };
                    d.default_style.text_align = align;
                    modified.push("text-align");
                }
                if let Some(lh) = line_height {
                    use ode_format::typography::LineHeight;
                    let parsed = if lh == "auto" {
                        LineHeight::Auto
                    } else if let Ok(v) = lh.parse::<f32>() {
                        LineHeight::Percent { value: StyleValue::Raw(v) }
                    } else {
                        print_json(&ErrorResponse::new("INVALID_VALUE", "set", &format!("invalid line-height: {lh}")));
                        return EXIT_INPUT;
                    };
                    d.default_style.line_height = parsed;
                    modified.push("line-height");
                }
            }
            _ => {
                print_json(&ErrorResponse::new("INVALID_PROPERTY", "set", "text-specific properties not applicable to this node type"));
                return EXIT_INPUT;
            }
        }
    }

    if modified.is_empty() {
        print_json(&ErrorResponse::new("NO_CHANGES", "set", "no properties specified to modify"));
        return EXIT_INPUT;
    }

    if let Err((code, err)) = save_wire(&file, &wire) {
        print_json(&err);
        return code;
    }

    print_json(&SetResponse {
        status: "ok",
        stable_id: stable_id.to_string(),
        modified: modified.into_iter().map(String::from).collect(),
    });
    EXIT_OK
}

fn parse_blend_mode(s: &str) -> Option<BlendMode> {
    match s {
        "normal" => Some(BlendMode::Normal),
        "multiply" => Some(BlendMode::Multiply),
        "screen" => Some(BlendMode::Screen),
        "overlay" => Some(BlendMode::Overlay),
        "darken" => Some(BlendMode::Darken),
        "lighten" => Some(BlendMode::Lighten),
        "color-dodge" => Some(BlendMode::ColorDodge),
        "color-burn" => Some(BlendMode::ColorBurn),
        "hard-light" => Some(BlendMode::HardLight),
        "soft-light" => Some(BlendMode::SoftLight),
        "difference" => Some(BlendMode::Difference),
        "exclusion" => Some(BlendMode::Exclusion),
        "hue" => Some(BlendMode::Hue),
        "saturation" => Some(BlendMode::Saturation),
        "color" => Some(BlendMode::Color),
        "luminosity" => Some(BlendMode::Luminosity),
        _ => None,
    }
}

fn parse_stroke_position(s: &str) -> Option<StrokePosition> {
    match s {
        "center" => Some(StrokePosition::Center),
        "inside" => Some(StrokePosition::Inside),
        "outside" => Some(StrokePosition::Outside),
        _ => None,
    }
}

fn parse_padding(s: &str) -> ode_format::node::LayoutPadding {
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match parts.len() {
        1 => ode_format::node::LayoutPadding { top: parts[0], right: parts[0], bottom: parts[0], left: parts[0] },
        4 => ode_format::node::LayoutPadding { top: parts[0], right: parts[1], bottom: parts[2], left: parts[3] },
        _ => ode_format::node::LayoutPadding::default(),
    }
}
```

- [ ] **Step 2: Register `Set` subcommand in `main.rs`**

Add to `Command` enum:

```rust
/// Set properties on an existing node
Set {
    /// Document file path
    file: String,
    /// Node stable_id to modify
    stable_id: String,
    #[arg(long)] name: Option<String>,
    #[arg(long)] visible: Option<bool>,
    #[arg(long)] opacity: Option<f32>,
    #[arg(long)] blend_mode: Option<String>,
    #[arg(long)] x: Option<f32>,
    #[arg(long)] y: Option<f32>,
    #[arg(long)] width: Option<f32>,
    #[arg(long)] height: Option<f32>,
    #[arg(long)] fill: Option<String>,
    #[arg(long)] fill_opacity: Option<f32>,
    #[arg(long)] stroke: Option<String>,
    #[arg(long)] stroke_width: Option<f32>,
    #[arg(long)] stroke_position: Option<String>,
    #[arg(long, value_name = "R or TL,TR,BR,BL")] corner_radius: Option<String>,
    #[arg(long)] clips_content: Option<bool>,
    #[arg(long)] layout: Option<String>,
    #[arg(long, value_name = "P or T,R,B,L")] padding: Option<String>,
    #[arg(long)] gap: Option<f32>,
    #[arg(long)] content: Option<String>,
    #[arg(long)] font_size: Option<f32>,
    #[arg(long)] font_family: Option<String>,
    #[arg(long)] font_weight: Option<u16>,
    #[arg(long)] text_align: Option<String>,
    #[arg(long)] line_height: Option<String>,
},
```

Add match arm:

```rust
Command::Set { file, stable_id, name, visible, opacity, blend_mode, x, y, width, height, fill, fill_opacity, stroke, stroke_width, stroke_position, corner_radius, clips_content, layout, padding, gap, content, font_size, font_family, font_weight, text_align, line_height } => {
    mutate::cmd_set(&file, &stable_id, name.as_deref(), visible, opacity, blend_mode.as_deref(), x, y, width, height, fill.as_deref(), fill_opacity, stroke.as_deref(), stroke_width, stroke_position.as_deref(), corner_radius.as_deref(), clips_content, layout.as_deref(), padding.as_deref(), gap, content.as_deref(), font_size, font_family.as_deref(), font_weight, text_align.as_deref(), line_height.as_deref())
}
```

- [ ] **Step 3: Write integration tests**

Create `crates/ode-cli/tests/set_test.rs`:

```rust
use std::process::Command;

fn ode_cmd() -> Command { Command::new(env!("CARGO_BIN_EXE_ode")) }

fn setup_doc_with_frame() -> (tempfile::TempDir, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd().args(["new", &file, "--width", "800", "--height", "600"]).output().unwrap();
    let out = ode_cmd().args(["add", "frame", &file, "--name", "Card", "--width", "320", "--height", "200"]).output().unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = resp["stable_id"].as_str().unwrap().to_string();
    (dir, file, id)
}

#[test]
fn set_fill_and_opacity() {
    let (_dir, file, id) = setup_doc_with_frame();
    let out = ode_cmd().args(["set", &file, &id, "--fill", "#FF0000", "--opacity", "0.5"]).output().unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let modified: Vec<String> = resp["modified"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(modified.contains(&"fill".to_string()));
    assert!(modified.contains(&"opacity".to_string()));
}

#[test]
fn set_layout_on_non_frame_fails() {
    let (_dir, file, _) = setup_doc_with_frame();
    let out = ode_cmd().args(["add", "text", &file, "--content", "Hi"]).output().unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text_id = resp["stable_id"].as_str().unwrap();

    let out = ode_cmd().args(["set", &file, text_id, "--layout", "horizontal"]).output().unwrap();
    assert!(!out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["code"], "INVALID_PROPERTY");
}

#[test]
fn set_nonexistent_node_fails() {
    let (_dir, file, _) = setup_doc_with_frame();
    let out = ode_cmd().args(["set", &file, "bogus-id", "--name", "X"]).output().unwrap();
    assert!(!out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["code"], "NOT_FOUND");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p ode-cli --test set_test`
Expected: all PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ode-cli/
git commit -m "feat(ode-cli): implement ode set command — common, visual, frame, text properties"
```

---

### Task 7: `ode delete` Command

**Files:**
- Modify: `crates/ode-cli/src/mutate.rs`
- Modify: `crates/ode-cli/src/main.rs`

- [ ] **Step 1: Add `cmd_delete` to `mutate.rs`**

```rust
// ─── ode delete ───

pub fn cmd_delete(file: &str, stable_id: &str) -> i32 {
    let (file, mut wire) = match load_wire(file) {
        Ok(v) => v,
        Err((code, err)) => { print_json(&err); return code; }
    };

    // Verify node exists
    if wire.find_node(stable_id).is_none() {
        print_json(&ErrorResponse::new("NOT_FOUND", "delete", &format!("node '{stable_id}' not found")));
        return EXIT_INPUT;
    }

    // Collect all IDs to delete
    let descendants = wire.collect_descendants(stable_id);
    let mut to_delete: Vec<String> = vec![stable_id.to_string()];
    to_delete.extend(descendants);

    let mut warnings = Vec::new();

    // Remove from parent's children
    wire.remove_child_from_parent(stable_id);

    // Remove from canvas if it's a root
    wire.canvas.retain(|c| c != stable_id);

    // Clean up view references
    for view in &mut wire.views {
        match &mut view.kind {
            ode_format::wire::ViewKindWire::Print { pages } => {
                let before = pages.len();
                pages.retain(|p| !to_delete.contains(p));
                if pages.len() < before {
                    warnings.push(Warning { path: stable_id.to_string(), code: "VIEW_REF_REMOVED".to_string(), message: format!("removed from Print view '{}'", view.name) });
                }
            }
            ode_format::wire::ViewKindWire::Web { root } => {
                if to_delete.contains(root) {
                    warnings.push(Warning { path: stable_id.to_string(), code: "VIEW_REF_REMOVED".to_string(), message: format!("Web view '{}' root was deleted — view will be removed", view.name) });
                }
            }
            ode_format::wire::ViewKindWire::Presentation { slides } => {
                let before = slides.len();
                slides.retain(|s| !to_delete.contains(s));
                if slides.len() < before {
                    warnings.push(Warning { path: stable_id.to_string(), code: "VIEW_REF_REMOVED".to_string(), message: format!("removed from Presentation view '{}'", view.name) });
                }
            }
            ode_format::wire::ViewKindWire::Export { .. } => {}
        }
    }

    // Remove views with dangling Web root
    wire.views.retain(|v| {
        if let ode_format::wire::ViewKindWire::Web { root } = &v.kind {
            !to_delete.contains(root)
        } else {
            true
        }
    });

    // Check for dangling instance references
    for node in &wire.nodes {
        if let NodeKindWire::Instance(inst) = &node.kind {
            if to_delete.contains(&inst.source_component) {
                warnings.push(Warning {
                    path: node.stable_id.clone(),
                    code: "DANGLING_INSTANCE".to_string(),
                    message: format!("instance '{}' references deleted component '{}'", node.name, inst.source_component),
                });
            }
        }
    }

    // Remove nodes
    wire.nodes.retain(|n| !to_delete.contains(&n.stable_id));

    if let Err((code, err)) = save_wire(&file, &wire) {
        print_json(&err);
        return code;
    }

    print_json(&DeleteResponse {
        status: "ok",
        deleted: to_delete,
        warnings,
    });
    EXIT_OK
}
```

- [ ] **Step 2: Register `Delete` subcommand**

Add to `Command` enum in `main.rs`:

```rust
/// Delete a node and its descendants
Delete {
    /// Document file path
    file: String,
    /// Node stable_id to delete
    stable_id: String,
},
```

Match arm:

```rust
Command::Delete { file, stable_id } => mutate::cmd_delete(&file, &stable_id),
```

- [ ] **Step 3: Write integration tests**

Create `crates/ode-cli/tests/delete_test.rs`:

```rust
use std::process::Command;

fn ode_cmd() -> Command { Command::new(env!("CARGO_BIN_EXE_ode")) }

#[test]
fn delete_node_and_descendants() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd().args(["new", &file, "--width", "800", "--height", "600"]).output().unwrap();

    // Add parent frame
    let out = ode_cmd().args(["add", "frame", &file, "--name", "Parent", "--width", "200", "--height", "200"]).output().unwrap();
    let parent_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    // Add child text
    let out = ode_cmd().args(["add", "text", &file, "--content", "Child", "--parent", &parent_id]).output().unwrap();
    let _child_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    // Delete parent (should remove child too)
    let out = ode_cmd().args(["delete", &file, &parent_id]).output().unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["deleted"].as_array().unwrap().len(), 2);
}

#[test]
fn delete_nonexistent_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd().args(["new", &file, "--width", "800", "--height", "600"]).output().unwrap();

    let out = ode_cmd().args(["delete", &file, "nonexistent"]).output().unwrap();
    assert!(!out.status.success());
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p ode-cli --test delete_test`
Expected: all PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ode-cli/
git commit -m "feat(ode-cli): implement ode delete command — recursive removal with view cleanup"
```

---

### Task 8: `ode move` Command

**Files:**
- Modify: `crates/ode-cli/src/mutate.rs`
- Modify: `crates/ode-cli/src/main.rs`

- [ ] **Step 1: Add `cmd_move` to `mutate.rs`**

```rust
// ─── ode move ───

pub fn cmd_move(file: &str, stable_id: &str, parent: &str, index: Option<usize>) -> i32 {
    let (file, mut wire) = match load_wire(file) {
        Ok(v) => v,
        Err((code, err)) => { print_json(&err); return code; }
    };

    // Verify source exists
    if wire.find_node(stable_id).is_none() {
        print_json(&ErrorResponse::new("NOT_FOUND", "move", &format!("node '{stable_id}' not found")));
        return EXIT_INPUT;
    }

    // Cycle detection: target must not be a descendant of source
    if parent != "root" {
        let descendants = wire.collect_descendants(stable_id);
        if descendants.contains(&parent.to_string()) || parent == stable_id {
            print_json(&ErrorResponse::new("CYCLE_DETECTED", "move", "target is a descendant of the moved node"));
            return EXIT_INPUT;
        }
    }

    // Remove from old parent
    wire.remove_child_from_parent(stable_id);
    wire.canvas.retain(|c| c != stable_id);

    // Insert into new parent
    let (parent_label, final_index) = if parent == "root" {
        let pos = index.unwrap_or(wire.canvas.len()).min(wire.canvas.len());
        wire.canvas.insert(pos, stable_id.to_string());
        ("root".to_string(), pos)
    } else {
        let target = match wire.find_node_mut(parent) {
            Some(n) => n,
            None => {
                print_json(&ErrorResponse::new("NOT_FOUND", "move", &format!("parent '{parent}' not found")));
                return EXIT_INPUT;
            }
        };
        if !DocumentWire::is_container(&target.kind) {
            print_json(&ErrorResponse::new("NOT_CONTAINER", "move", &format!("'{parent}' is not a container")));
            return EXIT_INPUT;
        }
        let children = DocumentWire::children_of_kind_mut(&mut target.kind).unwrap();
        let pos = index.unwrap_or(children.len()).min(children.len());
        children.insert(pos, stable_id.to_string());
        (parent.to_string(), pos)
    };

    if let Err((code, err)) = save_wire(&file, &wire) {
        print_json(&err);
        return code;
    }

    print_json(&MoveResponse {
        status: "ok",
        stable_id: stable_id.to_string(),
        new_parent: parent_label,
        index: final_index,
    });
    EXIT_OK
}
```

- [ ] **Step 2: Register `Move` subcommand**

Add to `Command` enum:

```rust
/// Move a node to a different parent
Move {
    /// Document file path
    file: String,
    /// Node stable_id to move
    stable_id: String,
    /// Target parent stable_id (or "root")
    #[arg(long)]
    parent: String,
    /// Insertion index (0-based, default: append)
    #[arg(long)]
    index: Option<usize>,
},
```

Match arm:

```rust
Command::Move { file, stable_id, parent, index } => mutate::cmd_move(&file, &stable_id, &parent, index),
```

- [ ] **Step 3: Write integration tests**

Create `crates/ode-cli/tests/move_test.rs`:

```rust
use std::process::Command;

fn ode_cmd() -> Command { Command::new(env!("CARGO_BIN_EXE_ode")) }

#[test]
fn move_node_between_parents() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd().args(["new", &file, "--width", "800", "--height", "600"]).output().unwrap();

    let out = ode_cmd().args(["add", "frame", &file, "--name", "A", "--width", "100", "--height", "100"]).output().unwrap();
    let a_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    let out = ode_cmd().args(["add", "frame", &file, "--name", "B", "--width", "100", "--height", "100"]).output().unwrap();
    let b_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    let out = ode_cmd().args(["add", "text", &file, "--content", "Hi", "--parent", &a_id]).output().unwrap();
    let text_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    // Move text from A to B
    let out = ode_cmd().args(["move", &file, &text_id, "--parent", &b_id]).output().unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["new_parent"], b_id);
}

#[test]
fn move_to_descendant_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd().args(["new", &file, "--width", "800", "--height", "600"]).output().unwrap();

    let out = ode_cmd().args(["add", "frame", &file, "--name", "Parent", "--width", "200", "--height", "200"]).output().unwrap();
    let parent_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    let out = ode_cmd().args(["add", "group", &file, "--parent", &parent_id]).output().unwrap();
    let child_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    // Try to move parent into its own child (cycle)
    let out = ode_cmd().args(["move", &file, &parent_id, "--parent", &child_id]).output().unwrap();
    assert!(!out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["code"], "CYCLE_DETECTED");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p ode-cli --test move_test`
Expected: all PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ode-cli/
git commit -m "feat(ode-cli): implement ode move command — with cycle detection"
```

---

## Chunk 3: End-to-End Validation

### Task 9: Agent Workflow Integration Test

**Files:**
- Create: `crates/ode-cli/tests/workflow_test.rs`

- [ ] **Step 1: Write the full agent workflow test**

```rust
use std::process::Command;

fn ode_cmd() -> Command { Command::new(env!("CARGO_BIN_EXE_ode")) }

#[test]
fn agent_workflow_new_add_set_build() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("card.ode.json").to_str().unwrap().to_string();
    let png = dir.path().join("card.png").to_str().unwrap().to_string();

    // 1. Create document
    let out = ode_cmd().args(["new", &file, "--width", "400", "--height", "300"]).output().unwrap();
    assert!(out.status.success(), "new failed");

    // 2. Add a colored background rect
    let out = ode_cmd()
        .args(["add", "vector", &file, "--shape", "rect", "--width", "400", "--height", "300", "--fill", "#3B82F6"])
        .output().unwrap();
    assert!(out.status.success(), "add vector failed");

    // 3. Add a text label
    let out = ode_cmd()
        .args(["add", "text", &file, "--content", "Hello ODE", "--font-size", "32"])
        .output().unwrap();
    assert!(out.status.success(), "add text failed");
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text_id = resp["stable_id"].as_str().unwrap().to_string();

    // 4. Set text position
    let out = ode_cmd()
        .args(["set", &file, &text_id, "--x", "50", "--y", "130"])
        .output().unwrap();
    assert!(out.status.success(), "set failed");

    // 5. Build to PNG
    let out = ode_cmd()
        .args(["build", &file, "--output", &png])
        .output().unwrap();
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stdout));

    // 6. Verify PNG exists and is non-empty
    let metadata = std::fs::metadata(&png).unwrap();
    assert!(metadata.len() > 100, "PNG too small: {} bytes", metadata.len());
}

#[test]
fn agent_workflow_add_set_delete_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd().args(["new", &file, "--width", "800", "--height", "600"]).output().unwrap();

    // Add 3 frames
    let mut ids = Vec::new();
    for i in 0..3 {
        let out = ode_cmd()
            .args(["add", "frame", &file, "--name", &format!("Frame {i}"), "--width", "100", "--height", "100"])
            .output().unwrap();
        let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        ids.push(resp["stable_id"].as_str().unwrap().to_string());
    }

    // Delete the middle one
    let out = ode_cmd().args(["delete", &file, &ids[1]]).output().unwrap();
    assert!(out.status.success());

    // Verify document still valid by building
    let svg = dir.path().join("test.svg").to_str().unwrap().to_string();
    let out = ode_cmd().args(["build", &file, "--output", &svg]).output().unwrap();
    assert!(out.status.success(), "build after delete failed: {}", String::from_utf8_lossy(&out.stdout));
}
```

- [ ] **Step 2: Run the workflow tests**

Run: `cargo test -p ode-cli --test workflow_test`
Expected: all PASS

- [ ] **Step 3: Run ALL tests to ensure nothing is broken**

Run: `cargo test --workspace`
Expected: all tests PASS, zero failures.

- [ ] **Step 4: Commit**

```bash
git add crates/ode-cli/tests/workflow_test.rs
git commit -m "test(ode-cli): add agent workflow integration tests — new→add→set→build roundtrip"
```
