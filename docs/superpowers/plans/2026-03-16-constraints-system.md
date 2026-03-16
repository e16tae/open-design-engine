# Constraints System Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a Figma-compatible constraints engine so child nodes reposition/resize when their parent frame is resized.

**Architecture:** Independent constraints engine alongside existing Auto Layout (Taffy). Non-auto-layout Frame/Instance containers apply constraints to their children via a top-down depth-first walk. A `ResizeMap` allows overriding parent sizes at render time.

**Tech Stack:** Rust, serde (with aliases for backward compat), clap (CLI args), existing taffy layout pipeline

**Spec:** `docs/superpowers/specs/2026-03-16-constraints-system-design.md`

---

## Chunk 1: Format and Import Changes

### Task 1: Expand ConstraintAxis enum

**Files:**
- Modify: `crates/ode-format/src/node.rs:115-128`

- [ ] **Step 1: Write the test for new enum variants**

In `crates/ode-format/src/node.rs`, add at the bottom of the file (or in an existing `#[cfg(test)] mod tests` block if present):

```rust
#[cfg(test)]
mod constraint_tests {
    use super::*;

    #[test]
    fn constraint_axis_serialization() {
        // New variants serialize to kebab-case
        assert_eq!(serde_json::to_string(&ConstraintAxis::Start).unwrap(), "\"start\"");
        assert_eq!(serde_json::to_string(&ConstraintAxis::End).unwrap(), "\"end\"");
        assert_eq!(serde_json::to_string(&ConstraintAxis::StartEnd).unwrap(), "\"start-end\"");
        assert_eq!(serde_json::to_string(&ConstraintAxis::Center).unwrap(), "\"center\"");
        assert_eq!(serde_json::to_string(&ConstraintAxis::Scale).unwrap(), "\"scale\"");
    }

    #[test]
    fn constraint_axis_backward_compat() {
        // Old v0.2 values deserialize via serde aliases
        assert_eq!(serde_json::from_str::<ConstraintAxis>("\"fixed\"").unwrap(), ConstraintAxis::Start);
        assert_eq!(serde_json::from_str::<ConstraintAxis>("\"stretch\"").unwrap(), ConstraintAxis::StartEnd);
    }

    #[test]
    fn constraints_round_trip() {
        let c = Constraints {
            horizontal: ConstraintAxis::End,
            vertical: ConstraintAxis::StartEnd,
        };
        let json = serde_json::to_string(&c).unwrap();
        let parsed: Constraints = serde_json::from_str(&json).unwrap();
        assert_eq!(c, parsed);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/lmuffin/Documents/Workspace/open-design-engine && cargo test -p ode-format constraint_tests`
Expected: FAIL — `Start`, `End`, `StartEnd` variants don't exist yet.

- [ ] **Step 3: Replace ConstraintAxis enum**

In `crates/ode-format/src/node.rs`, replace lines 115-122:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintAxis {
    #[serde(alias = "fixed")]
    Start,
    End,
    #[serde(alias = "stretch")]
    StartEnd,
    Center,
    Scale,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ode-format constraint_tests`
Expected: PASS — all 3 tests green.

- [ ] **Step 5: Run full ode-format tests to check nothing else broke**

Run: `cargo test -p ode-format`
Expected: All tests pass. If any existing tests reference `ConstraintAxis::Fixed` or `ConstraintAxis::Stretch`, update them to `Start`/`StartEnd`.

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/src/node.rs
git commit -m "feat(ode-format): expand ConstraintAxis enum with Start/End/StartEnd

Replace Fixed→Start/End and Stretch→StartEnd to distinguish
left-pinned from right-pinned constraints. Serde aliases provide
backward compatibility for existing .ode.json v0.2 files."
```

---

### Task 2: Update Figma import constraint mapping

**Files:**
- Modify: `crates/ode-import/src/figma/convert_layout.rs:127-152`

- [ ] **Step 1: Write the test for updated mapping**

In `crates/ode-import/src/figma/convert_layout.rs`, add (in existing test module or a new one):

```rust
#[cfg(test)]
mod constraint_mapping_tests {
    use super::*;
    use ode_format::node::ConstraintAxis;

    fn figma_constraint(h: &str, v: &str) -> FigmaLayoutConstraint {
        FigmaLayoutConstraint {
            horizontal: h.to_string(),
            vertical: v.to_string(),
        }
    }

    #[test]
    fn left_top_maps_to_start() {
        let c = convert_constraints(&figma_constraint("LEFT", "TOP"));
        assert_eq!(c.horizontal, ConstraintAxis::Start);
        assert_eq!(c.vertical, ConstraintAxis::Start);
    }

    #[test]
    fn right_bottom_maps_to_end() {
        let c = convert_constraints(&figma_constraint("RIGHT", "BOTTOM"));
        assert_eq!(c.horizontal, ConstraintAxis::End);
        assert_eq!(c.vertical, ConstraintAxis::End);
    }

    #[test]
    fn left_right_top_bottom_maps_to_start_end() {
        let c = convert_constraints(&figma_constraint("LEFT_RIGHT", "TOP_BOTTOM"));
        assert_eq!(c.horizontal, ConstraintAxis::StartEnd);
        assert_eq!(c.vertical, ConstraintAxis::StartEnd);
    }

    #[test]
    fn center_maps_to_center() {
        let c = convert_constraints(&figma_constraint("CENTER", "CENTER"));
        assert_eq!(c.horizontal, ConstraintAxis::Center);
        assert_eq!(c.vertical, ConstraintAxis::Center);
    }

    #[test]
    fn scale_maps_to_scale() {
        let c = convert_constraints(&figma_constraint("SCALE", "SCALE"));
        assert_eq!(c.horizontal, ConstraintAxis::Scale);
        assert_eq!(c.vertical, ConstraintAxis::Scale);
    }

    #[test]
    fn unknown_falls_back_to_start() {
        let c = convert_constraints(&figma_constraint("UNKNOWN", "WHATEVER"));
        assert_eq!(c.horizontal, ConstraintAxis::Start);
        assert_eq!(c.vertical, ConstraintAxis::Start);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-import constraint_mapping_tests`
Expected: FAIL — `right_bottom_maps_to_end` fails because `RIGHT`/`BOTTOM` currently map to `Fixed` (now `Start`), not `End`.

- [ ] **Step 3: Update convert_constraint_axis function**

In `crates/ode-import/src/figma/convert_layout.rs`, replace the `convert_constraint_axis` function (lines 138-152):

```rust
fn convert_constraint_axis(s: &str, _is_horizontal: bool) -> ConstraintAxis {
    match s {
        "LEFT" | "TOP" => ConstraintAxis::Start,
        "RIGHT" | "BOTTOM" => ConstraintAxis::End,
        "CENTER" => ConstraintAxis::Center,
        "LEFT_RIGHT" | "TOP_BOTTOM" => ConstraintAxis::StartEnd,
        "SCALE" => ConstraintAxis::Scale,
        _ => ConstraintAxis::Start,
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ode-import constraint_mapping_tests`
Expected: PASS — all 6 tests green.

- [ ] **Step 5: Update existing import tests for new enum variants**

In `crates/ode-import/src/figma/convert_layout.rs`, update three existing tests:

- `test_convert_constraints_mapping` (line 316): `ConstraintAxis::Stretch` → `ConstraintAxis::StartEnd`
- `test_convert_constraints_fixed_edges` (lines 337-338): `ConstraintAxis::Fixed` → `ConstraintAxis::Start` for LEFT/TOP
- `test_convert_constraints_fixed_edges` (lines 345-346): `ConstraintAxis::Fixed` → `ConstraintAxis::End` for RIGHT/BOTTOM

- [ ] **Step 6: Run full ode-import tests**

Run: `cargo test -p ode-import`
Expected: All pass.

- [ ] **Step 7: Commit**

```bash
git add crates/ode-import/src/figma/convert_layout.rs
git commit -m "feat(ode-import): update Figma constraint mapping for Start/End/StartEnd

RIGHT/BOTTOM now correctly map to End instead of Fixed.
LEFT_RIGHT/TOP_BOTTOM map to StartEnd."
```

---

## Chunk 2: Constraints Engine

### Task 3: Implement apply_constraints core function

**Files:**
- Modify: `crates/ode-core/src/layout.rs`

- [ ] **Step 1: Write unit tests for apply_constraints**

Add at the bottom of the `#[cfg(test)] mod tests` block in `crates/ode-core/src/layout.rs`:

```rust
    // ─── Constraints tests ───

    #[test]
    fn constraint_start_no_change() {
        let child = LayoutRect { x: 20.0, y: 30.0, width: 50.0, height: 40.0 };
        let constraints = Constraints {
            horizontal: ConstraintAxis::Start,
            vertical: ConstraintAxis::Start,
        };
        let result = apply_constraints(child, &constraints, (200.0, 100.0), (300.0, 150.0));
        assert!((result.x - 20.0).abs() < 0.01);
        assert!((result.y - 30.0).abs() < 0.01);
        assert!((result.width - 50.0).abs() < 0.01);
        assert!((result.height - 40.0).abs() < 0.01);
    }

    #[test]
    fn constraint_end_shifts_position() {
        let child = LayoutRect { x: 130.0, y: 50.0, width: 50.0, height: 40.0 };
        let constraints = Constraints {
            horizontal: ConstraintAxis::End,
            vertical: ConstraintAxis::End,
        };
        // Parent grows: 200→300 (delta +100), 100→150 (delta +50)
        let result = apply_constraints(child, &constraints, (200.0, 100.0), (300.0, 150.0));
        assert!((result.x - 230.0).abs() < 0.01, "x = {}", result.x);   // 130 + 100
        assert!((result.y - 100.0).abs() < 0.01, "y = {}", result.y);   // 50 + 50
        assert!((result.width - 50.0).abs() < 0.01);
        assert!((result.height - 40.0).abs() < 0.01);
    }

    #[test]
    fn constraint_start_end_stretches() {
        let child = LayoutRect { x: 20.0, y: 10.0, width: 160.0, height: 80.0 };
        let constraints = Constraints {
            horizontal: ConstraintAxis::StartEnd,
            vertical: ConstraintAxis::StartEnd,
        };
        // Parent grows: 200→300 (+100), 100→150 (+50)
        let result = apply_constraints(child, &constraints, (200.0, 100.0), (300.0, 150.0));
        assert!((result.x - 20.0).abs() < 0.01);
        assert!((result.y - 10.0).abs() < 0.01);
        assert!((result.width - 260.0).abs() < 0.01, "w = {}", result.width);  // 160 + 100
        assert!((result.height - 130.0).abs() < 0.01, "h = {}", result.height); // 80 + 50
    }

    #[test]
    fn constraint_start_end_clamps_to_zero() {
        let child = LayoutRect { x: 20.0, y: 10.0, width: 50.0, height: 40.0 };
        let constraints = Constraints {
            horizontal: ConstraintAxis::StartEnd,
            vertical: ConstraintAxis::StartEnd,
        };
        // Parent shrinks drastically: 200→10 (delta -190 > width 50)
        let result = apply_constraints(child, &constraints, (200.0, 100.0), (10.0, 5.0));
        assert!((result.width).abs() < 0.01, "w clamped to 0, got {}", result.width);
        assert!((result.height).abs() < 0.01, "h clamped to 0, got {}", result.height);
    }

    #[test]
    fn constraint_center_shifts_half_delta() {
        let child = LayoutRect { x: 75.0, y: 30.0, width: 50.0, height: 40.0 };
        let constraints = Constraints {
            horizontal: ConstraintAxis::Center,
            vertical: ConstraintAxis::Center,
        };
        // Parent grows: 200→300 (delta/2 = 50), 100→150 (delta/2 = 25)
        let result = apply_constraints(child, &constraints, (200.0, 100.0), (300.0, 150.0));
        assert!((result.x - 125.0).abs() < 0.01, "x = {}", result.x);  // 75 + 50
        assert!((result.y - 55.0).abs() < 0.01, "y = {}", result.y);    // 30 + 25
        assert!((result.width - 50.0).abs() < 0.01);
        assert!((result.height - 40.0).abs() < 0.01);
    }

    #[test]
    fn constraint_scale_proportional() {
        let child = LayoutRect { x: 40.0, y: 20.0, width: 80.0, height: 40.0 };
        let constraints = Constraints {
            horizontal: ConstraintAxis::Scale,
            vertical: ConstraintAxis::Scale,
        };
        // Parent doubles: 200→400, 100→200
        let result = apply_constraints(child, &constraints, (200.0, 100.0), (400.0, 200.0));
        assert!((result.x - 80.0).abs() < 0.01, "x = {}", result.x);
        assert!((result.y - 40.0).abs() < 0.01, "y = {}", result.y);
        assert!((result.width - 160.0).abs() < 0.01, "w = {}", result.width);
        assert!((result.height - 80.0).abs() < 0.01, "h = {}", result.height);
    }

    #[test]
    fn constraint_scale_zero_parent_degrades_to_start() {
        let child = LayoutRect { x: 40.0, y: 20.0, width: 80.0, height: 40.0 };
        let constraints = Constraints {
            horizontal: ConstraintAxis::Scale,
            vertical: ConstraintAxis::Scale,
        };
        // Zero original parent — should not divide by zero
        let result = apply_constraints(child, &constraints, (0.0, 0.0), (300.0, 150.0));
        assert!((result.x - 40.0).abs() < 0.01);
        assert!((result.y - 20.0).abs() < 0.01);
        assert!((result.width - 80.0).abs() < 0.01);
        assert!((result.height - 40.0).abs() < 0.01);
    }

    #[test]
    fn constraint_mixed_axes() {
        let child = LayoutRect { x: 20.0, y: 50.0, width: 60.0, height: 30.0 };
        let constraints = Constraints {
            horizontal: ConstraintAxis::StartEnd,  // stretches width
            vertical: ConstraintAxis::End,          // shifts y
        };
        // Parent: 200→300 (+100), 100→150 (+50)
        let result = apply_constraints(child, &constraints, (200.0, 100.0), (300.0, 150.0));
        assert!((result.x - 20.0).abs() < 0.01);
        assert!((result.width - 160.0).abs() < 0.01, "w = {}", result.width); // 60 + 100
        assert!((result.y - 100.0).abs() < 0.01, "y = {}", result.y);         // 50 + 50
        assert!((result.height - 30.0).abs() < 0.01);
    }
```

- [ ] **Step 2: Add the necessary imports to the test module**

At the top of the existing test module in `layout.rs`, add:

```rust
    use ode_format::node::{ConstraintAxis, Constraints};
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ode-core constraint_`
Expected: FAIL — `apply_constraints` function doesn't exist yet.

- [ ] **Step 4: Implement apply_constraints**

Add to `crates/ode-core/src/layout.rs`, after the existing imports and before `compute_layout`:

```rust
use ode_format::node::{ConstraintAxis, Constraints};

/// Apply a single axis constraint.
///
/// - `pos`: child position on this axis (x or y)
/// - `size`: child size on this axis (width or height)
/// - `original`: parent's design-time dimension
/// - `current`: parent's current (resized) dimension
fn apply_axis(axis: ConstraintAxis, pos: f32, size: f32, original: f32, current: f32) -> (f32, f32) {
    let delta = current - original;
    match axis {
        ConstraintAxis::Start => (pos, size),
        ConstraintAxis::End => (pos + delta, size),
        ConstraintAxis::StartEnd => (pos, (size + delta).max(0.0)),
        ConstraintAxis::Center => (pos + delta * 0.5, size),
        ConstraintAxis::Scale => {
            if original.abs() < f32::EPSILON {
                // Degrade to Start if original parent dimension is zero
                (pos, size)
            } else {
                let ratio = current / original;
                (pos * ratio, size * ratio)
            }
        }
    }
}

/// Apply constraints to reposition/resize a child rect when its parent is resized.
pub fn apply_constraints(
    child: LayoutRect,
    constraints: &Constraints,
    original_parent: (f32, f32),
    current_parent: (f32, f32),
) -> LayoutRect {
    let (new_x, new_w) = apply_axis(
        constraints.horizontal,
        child.x,
        child.width,
        original_parent.0,
        current_parent.0,
    );
    let (new_y, new_h) = apply_axis(
        constraints.vertical,
        child.y,
        child.height,
        original_parent.1,
        current_parent.1,
    );
    LayoutRect {
        x: new_x,
        y: new_y,
        width: new_w,
        height: new_h,
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ode-core constraint_`
Expected: PASS — all 8 constraint tests green.

- [ ] **Step 6: Commit**

```bash
git add crates/ode-core/src/layout.rs
git commit -m "feat(ode-core): implement apply_constraints with per-axis calculation

Supports Start, End, StartEnd, Center, Scale axes with edge case
guards for zero parent (Scale) and negative width (StartEnd)."
```

---

### Task 4: Implement walk_for_constraints and integrate with compute_layout

**Files:**
- Modify: `crates/ode-core/src/layout.rs`

- [ ] **Step 1: Write integration test for constraints in layout**

Add to the test module in `crates/ode-core/src/layout.rs`:

```rust
    #[test]
    fn constraints_reposition_on_resize() {
        let mut doc = Document::new("Test");

        // Parent frame 200x100, NO auto layout
        let mut parent = Node::new_frame("Parent", 200.0, 100.0);

        // Child at (150, 60) with End constraint — pinned to right/bottom
        let mut child = Node::new_frame("Child", 30.0, 20.0);
        child.transform.tx = 150.0;
        child.transform.ty = 60.0;
        child.constraints = Some(Constraints {
            horizontal: ConstraintAxis::End,
            vertical: ConstraintAxis::End,
        });

        let c_id = doc.nodes.insert(child);
        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c_id];
        }
        let p_id = doc.nodes.insert(parent);
        doc.canvas.push(p_id);

        // Resize parent to 300x150
        let mut resize_map = ResizeMap::new();
        resize_map.insert(p_id, (300.0, 150.0));

        let index: HashMap<&str, NodeId> = doc
            .nodes
            .iter()
            .map(|(nid, node)| (node.stable_id.as_str(), nid))
            .collect();
        let layout = compute_layout(&doc, &index, &resize_map);

        let r = layout.get(&c_id).expect("Child should have layout rect");
        // End: x = 150 + (300-200) = 250, y = 60 + (150-100) = 110
        assert!((r.x - 250.0).abs() < 0.1, "x = {}", r.x);
        assert!((r.y - 110.0).abs() < 0.1, "y = {}", r.y);
        assert!((r.width - 30.0).abs() < 0.1);
        assert!((r.height - 20.0).abs() < 0.1);
    }

    #[test]
    fn constraints_ignored_for_auto_layout_frames() {
        let mut doc = Document::new("Test");

        // Parent with auto layout
        let config = default_config();
        let mut parent = make_auto_layout_frame("Parent", 200.0, 100.0, config);

        // Child with End constraint — should be ignored because parent has auto layout
        let mut child = Node::new_frame("Child", 50.0, 40.0);
        child.transform.tx = 10.0;
        child.transform.ty = 10.0;
        child.constraints = Some(Constraints {
            horizontal: ConstraintAxis::End,
            vertical: ConstraintAxis::End,
        });

        let c_id = doc.nodes.insert(child);
        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c_id];
        }
        let p_id = doc.nodes.insert(parent);
        doc.canvas.push(p_id);

        let mut resize_map = ResizeMap::new();
        resize_map.insert(p_id, (300.0, 150.0));

        let index: HashMap<&str, NodeId> = doc
            .nodes
            .iter()
            .map(|(nid, node)| (node.stable_id.as_str(), nid))
            .collect();
        let layout = compute_layout(&doc, &index, &resize_map);

        // Child position should come from auto layout, not constraints
        let r = layout.get(&c_id).expect("Child should have auto-layout rect");
        assert!((r.x - 0.0).abs() < 0.1, "x = {} (auto layout, not constraint)", r.x);
    }

    #[test]
    fn nested_constraints_top_down() {
        let mut doc = Document::new("Test");

        // Grandparent 400x200, no auto layout
        let mut grandparent = Node::new_frame("GP", 400.0, 200.0);

        // Parent 200x100 at (100, 50), StartEnd horizontal → stretches
        let mut parent = Node::new_frame("Parent", 200.0, 100.0);
        parent.transform.tx = 100.0;
        parent.transform.ty = 50.0;
        parent.constraints = Some(Constraints {
            horizontal: ConstraintAxis::StartEnd,
            vertical: ConstraintAxis::Start,
        });

        // Child at (150, 60) with End constraint inside parent
        let mut child = Node::new_frame("Child", 30.0, 20.0);
        child.transform.tx = 150.0;
        child.transform.ty = 60.0;
        child.constraints = Some(Constraints {
            horizontal: ConstraintAxis::End,
            vertical: ConstraintAxis::Start,
        });

        let c_id = doc.nodes.insert(child);
        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c_id];
        }
        let parent_id = doc.nodes.insert(parent);
        if let NodeKind::Frame(ref mut data) = grandparent.kind {
            data.container.children = vec![parent_id];
        }
        let gp_id = doc.nodes.insert(grandparent);
        doc.canvas.push(gp_id);

        // Resize grandparent to 600x200 (+200 horizontal)
        let mut resize_map = ResizeMap::new();
        resize_map.insert(gp_id, (600.0, 200.0));

        let index: HashMap<&str, NodeId> = doc
            .nodes
            .iter()
            .map(|(nid, node)| (node.stable_id.as_str(), nid))
            .collect();
        let layout = compute_layout(&doc, &index, &resize_map);

        // Parent: StartEnd horizontal → width 200 + 200 = 400, x stays 100
        let pr = layout.get(&parent_id).expect("Parent should have layout rect");
        assert!((pr.x - 100.0).abs() < 0.1, "parent.x = {}", pr.x);
        assert!((pr.width - 400.0).abs() < 0.1, "parent.w = {}", pr.width);

        // Child: End inside parent that grew from 200→400 (delta +200)
        // child.x = 150 + 200 = 350
        let cr = layout.get(&c_id).expect("Child should have layout rect");
        assert!((cr.x - 350.0).abs() < 0.1, "child.x = {}", cr.x);
    }

    #[test]
    fn no_resize_no_constraints_applied() {
        let mut doc = Document::new("Test");

        let mut parent = Node::new_frame("Parent", 200.0, 100.0);
        let mut child = Node::new_frame("Child", 50.0, 40.0);
        child.transform.tx = 20.0;
        child.transform.ty = 10.0;
        child.constraints = Some(Constraints {
            horizontal: ConstraintAxis::End,
            vertical: ConstraintAxis::End,
        });

        let c_id = doc.nodes.insert(child);
        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c_id];
        }
        let p_id = doc.nodes.insert(parent);
        doc.canvas.push(p_id);

        // Empty resize map — no resize, constraints should not produce LayoutRects
        let resize_map = ResizeMap::new();
        let index: HashMap<&str, NodeId> = doc
            .nodes
            .iter()
            .map(|(nid, node)| (node.stable_id.as_str(), nid))
            .collect();
        let layout = compute_layout(&doc, &index, &resize_map);

        assert!(layout.get(&c_id).is_none(), "No resize → no constraint layout");
    }

    #[test]
    fn constraints_none_treated_as_start_start() {
        let mut doc = Document::new("Test");

        let mut parent = Node::new_frame("Parent", 200.0, 100.0);
        let mut child = Node::new_frame("Child", 50.0, 40.0);
        child.transform.tx = 20.0;
        child.transform.ty = 10.0;
        child.constraints = None; // implicit Start/Start

        let c_id = doc.nodes.insert(child);
        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![c_id];
        }
        let p_id = doc.nodes.insert(parent);
        doc.canvas.push(p_id);

        let mut resize_map = ResizeMap::new();
        resize_map.insert(p_id, (300.0, 150.0));

        let index: HashMap<&str, NodeId> = doc
            .nodes
            .iter()
            .map(|(nid, node)| (node.stable_id.as_str(), nid))
            .collect();
        let layout = compute_layout(&doc, &index, &resize_map);

        // None = Start/Start = no-op, no LayoutRect produced
        assert!(layout.get(&c_id).is_none(), "None constraints = no layout entry");
    }

    #[test]
    fn group_transparent_to_constraints() {
        use ode_format::node::GroupData;

        let mut doc = Document::new("Test");

        // Parent frame 200x100
        let mut parent = Node::new_frame("Parent", 200.0, 100.0);

        // Group inside parent (transparent — no size of its own)
        let mut group = Node {
            id: NodeId::default(),
            stable_id: ode_format::node::StableId::generate(),
            name: "Group".to_string(),
            transform: Default::default(),
            opacity: 1.0,
            blend_mode: Default::default(),
            visible: true,
            constraints: None,
            layout_sizing: None,
            kind: NodeKind::Group(GroupData { children: vec![] }),
        };

        // Child inside Group with End constraint
        let mut child = Node::new_frame("Child", 30.0, 20.0);
        child.transform.tx = 150.0;
        child.transform.ty = 60.0;
        child.constraints = Some(Constraints {
            horizontal: ConstraintAxis::End,
            vertical: ConstraintAxis::Start,
        });

        let c_id = doc.nodes.insert(child);
        if let NodeKind::Group(ref mut data) = group.kind {
            data.children = vec![c_id];
        }
        let g_id = doc.nodes.insert(group);
        if let NodeKind::Frame(ref mut data) = parent.kind {
            data.container.children = vec![g_id];
        }
        let p_id = doc.nodes.insert(parent);
        doc.canvas.push(p_id);

        // Resize parent to 300x100
        let mut resize_map = ResizeMap::new();
        resize_map.insert(p_id, (300.0, 100.0));

        let index: HashMap<&str, NodeId> = doc
            .nodes
            .iter()
            .map(|(nid, node)| (node.stable_id.as_str(), nid))
            .collect();
        let layout = compute_layout(&doc, &index, &resize_map);

        // Group is transparent — walk_for_constraints recurses through it
        // but the child's constraint is against the parent Frame (the nearest Frame ancestor)
        // Since Group is not a constraint container, the child won't be resolved here.
        // The child's constraints only apply if its direct parent (Group) is a Frame/Instance.
        // Groups are transparent: they just pass through recursion.
        // In this case, the parent Frame owns the Group which owns the child.
        // The parent Frame iterates its direct children (Group), not grandchildren.
        // So constraints on grandchildren inside Groups are NOT applied by the parent Frame.
        // This matches Figma: Groups don't participate in constraint resolution.
        assert!(layout.get(&c_id).is_none(),
            "Child inside Group is not directly constrained by grandparent Frame");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-core constraints_reposition`
Expected: FAIL — `compute_layout` doesn't accept `resize_map` yet.

- [ ] **Step 3: Add ResizeMap type and update compute_layout signature**

In `crates/ode-core/src/layout.rs`, add after the `LayoutMap` type alias:

```rust
/// Maps NodeIds to override sizes for constraint-based resize simulation.
pub type ResizeMap = HashMap<NodeId, (f32, f32)>;
```

Update the `compute_layout` function signature and body:

```rust
pub fn compute_layout<'a>(
    doc: &'a Document,
    stable_id_index: &HashMap<&'a str, NodeId>,
    resize_map: &ResizeMap,
) -> LayoutMap {
    let mut result = LayoutMap::new();

    // Phase 1: Auto Layout (Taffy) — bottom-up
    for &root_id in &doc.canvas {
        walk_for_layout(doc, root_id, &mut result, stable_id_index);
    }

    // Phase 2: Constraints — top-down
    for &root_id in &doc.canvas {
        walk_for_constraints(doc, root_id, resize_map, &mut result, stable_id_index);
    }

    result
}
```

Also update the `test_compute_layout` helper in the test module (around line 469) to pass the new parameter:

```rust
    fn test_compute_layout(doc: &Document) -> LayoutMap {
        let index: HashMap<&str, NodeId> = doc
            .nodes
            .iter()
            .map(|(nid, node)| (node.stable_id.as_str(), nid))
            .collect();
        compute_layout(doc, &index, &ResizeMap::new())
    }
```

Add `ResizeMap` to the test module imports (it's already accessible via `use super::*`).

- [ ] **Step 4: Implement walk_for_constraints**

Add to `crates/ode-core/src/layout.rs`:

```rust
/// Get the design-time size of a container node (Frame or Instance).
fn get_container_design_size(
    node: &Node,
    doc: &Document,
    stable_id_index: &HashMap<&str, NodeId>,
) -> Option<(f32, f32)> {
    match &node.kind {
        NodeKind::Frame(data) => Some((data.width, data.height)),
        NodeKind::Instance(data) => {
            // Use instance's own size if available, else component size
            let comp_size = stable_id_index
                .get(data.source_component.as_str())
                .and_then(|&comp_id| {
                    if let NodeKind::Frame(ref fd) = doc.nodes[comp_id].kind {
                        Some((fd.width, fd.height))
                    } else {
                        None
                    }
                })
                .unwrap_or((0.0, 0.0));
            Some((
                data.width.unwrap_or(comp_size.0),
                data.height.unwrap_or(comp_size.1),
            ))
        }
        _ => None,
    }
}

/// Get the intrinsic size of any node (for building child_rect).
fn get_node_intrinsic_size(node: &Node) -> (f32, f32) {
    match &node.kind {
        NodeKind::Frame(data) => (data.width, data.height),
        NodeKind::Text(data) => (data.width, data.height),
        NodeKind::Image(data) => (data.width, data.height),
        NodeKind::Instance(data) => (
            data.width.unwrap_or(0.0),
            data.height.unwrap_or(0.0),
        ),
        _ => (0.0, 0.0),
    }
}

/// Top-down depth-first walk: apply constraints to children of non-auto-layout containers.
fn walk_for_constraints(
    doc: &Document,
    node_id: NodeId,
    resize_map: &ResizeMap,
    result: &mut LayoutMap,
    stable_id_index: &HashMap<&str, NodeId>,
) {
    let node = &doc.nodes[node_id];

    // Determine if this node is a non-auto-layout container (Frame or Instance without LayoutConfig)
    let is_constraint_container = match &node.kind {
        NodeKind::Frame(data) => data.container.layout.is_none(),
        NodeKind::Instance(data) => data.container.layout.is_none(),
        _ => false,
    };

    if is_constraint_container {
        if let Some(design_size) = get_container_design_size(node, doc, stable_id_index) {
            // Current size: from resize_map, or from constraint result (if parent resized this node), or design size
            let current_size = resize_map
                .get(&node_id)
                .copied()
                .or_else(|| result.get(&node_id).map(|r| (r.width, r.height)))
                .unwrap_or(design_size);

            // Only apply constraints if size actually changed
            if (current_size.0 - design_size.0).abs() > f32::EPSILON
                || (current_size.1 - design_size.1).abs() > f32::EPSILON
            {
                if let Some(children) = node.kind.children() {
                    for &child_id in children {
                        let child_node = &doc.nodes[child_id];
                        let constraints = match child_node.constraints {
                            Some(c) => c,
                            None => continue, // None = implicit Start/Start = no-op
                        };
                        // Skip Start/Start — it's a no-op
                        if constraints.horizontal == ConstraintAxis::Start
                            && constraints.vertical == ConstraintAxis::Start
                        {
                            continue;
                        }

                        let (iw, ih) = get_node_intrinsic_size(child_node);
                        let child_rect = LayoutRect {
                            x: child_node.transform.tx,
                            y: child_node.transform.ty,
                            width: iw,
                            height: ih,
                        };

                        let new_rect = apply_constraints(
                            child_rect,
                            &constraints,
                            design_size,
                            current_size,
                        );
                        result.insert(child_id, new_rect);
                    }
                }
            }
        }
    }

    // Recurse into children (top-down: parent resolved before children)
    // Group nodes are transparent — just recurse without constraint resolution
    if let Some(children) = node.kind.children() {
        for &child_id in children {
            walk_for_constraints(doc, child_id, resize_map, result, stable_id_index);
        }
    }
}
```

- [ ] **Step 5: Fix the existing compute_layout call site**

In `crates/ode-core/src/convert.rs` line 37, update:

```rust
let layout_map = crate::layout::compute_layout(doc, &stable_id_index, &Default::default());
```

This ensures existing `from_document()` callers pass an empty resize map.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p ode-core`
Expected: All tests pass — both new constraint tests and existing auto-layout tests.

- [ ] **Step 7: Commit**

```bash
git add crates/ode-core/src/layout.rs crates/ode-core/src/convert.rs
git commit -m "feat(ode-core): add constraints engine with top-down walk and ResizeMap

walk_for_constraints applies constraints to children of non-auto-layout
Frame/Instance containers. Group nodes are transparent. Supports nested
constraint resolution via top-down depth-first traversal."
```

---

## Chunk 3: Scene Conversion and CLI

### Task 5: Wire from_document_with_resize and update convert_node for layout rect sizes

**Files:**
- Modify: `crates/ode-core/src/convert.rs`

- [ ] **Step 1: Add from_document_with_resize method**

In `crates/ode-core/src/convert.rs`, inside the `impl Scene` block, refactor:

```rust
impl Scene {
    /// Convert a Document into a Scene.
    pub fn from_document(doc: &Document, font_db: &FontDatabase) -> Result<Self, ConvertError> {
        Self::from_document_with_resize(doc, font_db, &crate::layout::ResizeMap::new())
    }

    /// Convert a Document into a Scene with optional frame resize overrides.
    pub fn from_document_with_resize(
        doc: &Document,
        font_db: &FontDatabase,
        resize_map: &crate::layout::ResizeMap,
    ) -> Result<Self, ConvertError> {
        if doc.canvas.is_empty() {
            return Err(ConvertError::NoCanvasRoots);
        }

        let stable_id_index: StableIdIndex = doc
            .nodes
            .iter()
            .map(|(nid, node)| (node.stable_id.as_str(), nid))
            .collect();

        let layout_map = crate::layout::compute_layout(doc, &stable_id_index, resize_map);

        // If root is resized, use the resize dimensions for scene size
        let first_root = doc.canvas[0];
        let (width, height) = resize_map
            .get(&first_root)
            .map(|&(w, h)| (w, h))
            .unwrap_or_else(|| get_frame_size(&doc.nodes[first_root], layout_map.get(&first_root)));

        let mut commands = Vec::new();
        let identity = tiny_skia::Transform::identity();

        for &root_id in &doc.canvas {
            convert_node(
                doc,
                root_id,
                identity,
                &mut commands,
                font_db,
                &layout_map,
                &stable_id_index,
            )?;
        }

        Ok(Scene {
            width,
            height,
            commands,
        })
    }
}
```

- [ ] **Step 2: Update emit_image to accept layout rect**

Modify `emit_image` to accept an optional layout rect for size override:

```rust
fn emit_image(
    img_data: &ode_format::node::ImageData,
    current_transform: tiny_skia::Transform,
    commands: &mut Vec<RenderCommand>,
    layout_rect: Option<&crate::layout::LayoutRect>,
) {
    let (w, h) = layout_rect
        .map(|r| (r.width, r.height))
        .unwrap_or((img_data.width, img_data.height));

    if w <= 0.0 || h <= 0.0 {
        return;
    }

    let image_bytes = match &img_data.source {
        Some(ode_format::style::ImageSource::Embedded { data }) => {
            if data.is_empty() {
                return;
            }
            data.clone()
        }
        Some(ode_format::style::ImageSource::Linked { path }) => {
            match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(_) => return,
            }
        }
        None => return,
    };

    commands.push(RenderCommand::DrawImage {
        data: image_bytes,
        width: w,
        height: h,
        transform: current_transform,
    });
}
```

- [ ] **Step 3: Update get_node_path Image arm**

In `get_node_path`, update the Image arm to use layout_rect:

```rust
        NodeKind::Image(data) => {
            let (w, h) = layout_rect
                .map(|r| (r.width, r.height))
                .unwrap_or((data.width, data.height));
            if w > 0.0 && h > 0.0 {
                Some(path::rounded_rect_path(w, h, [0.0; 4]))
            } else {
                None
            }
        }
```

- [ ] **Step 4: Update emit_image call site to pass layout_rect**

There is one call site in `convert_node` (around line 142 of `convert.rs`). Update it to pass `layout_rect` (which is already in scope as `layout_map.get(&node_id)`).

- [ ] **Step 5: Re-export ResizeMap from ode-core lib.rs**

In `crates/ode-core/src/lib.rs`, update the existing export line (line 11):

```rust
pub use layout::{LayoutMap, LayoutRect, ResizeMap};
```

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: All tests pass across all crates.

- [ ] **Step 7: Commit**

```bash
git add crates/ode-core/
git commit -m "feat(ode-core): add from_document_with_resize and layout rect size support

Scene conversion now supports resize overrides. Image nodes and clip
paths use layout-resolved sizes when constraints modify dimensions."
```

---

### Task 6: Add --resize CLI option

**Files:**
- Modify: `crates/ode-cli/src/main.rs`
- Modify: `crates/ode-cli/src/commands.rs`

- [ ] **Step 1: Add --resize arg to Build and Render commands**

In `crates/ode-cli/src/main.rs`, add to both `Build` and `Render` variants:

```rust
        /// Resize the root frame (e.g., 1920x1080)
        #[arg(long, value_name = "WxH")]
        resize: Option<String>,
```

- [ ] **Step 2: Update main.rs command handlers to pass resize**

Pass the `resize` value through to the command functions:

```rust
        Command::Build {
            file,
            output,
            format,
            resize,
        } => commands::cmd_build(&file, &output, format.as_deref(), resize.as_deref()),
        Command::Render {
            file,
            output,
            format,
            resize,
        } => commands::cmd_render(&file, &output, format.as_deref(), resize.as_deref()),
```

- [ ] **Step 3: Update cmd_build and cmd_render signatures**

In `crates/ode-cli/src/commands.rs`:

```rust
pub fn cmd_build(file: &str, output: &str, format: Option<&str>, resize: Option<&str>) -> i32 {
    // ... existing validation logic ...
    render_and_export(&doc, output, format, vec![], resize)
}

pub fn cmd_render(file: &str, output: &str, format: Option<&str>, resize: Option<&str>) -> i32 {
    // ... existing parse logic ...
    render_and_export(&doc, output, format, vec![], resize)
}
```

- [ ] **Step 4: Update render_and_export to parse resize and use from_document_with_resize**

```rust
fn render_and_export(
    doc: &Document,
    output: &str,
    format: Option<&str>,
    warnings: Vec<Warning>,
    resize: Option<&str>,
) -> i32 {
    let font_db = FontDatabase::new_system();

    let scene = if let Some(resize_str) = resize {
        // Parse "WxH" format
        let parts: Vec<&str> = resize_str.split('x').collect();
        if parts.len() != 2 {
            print_json(&ErrorResponse::new(
                "INVALID_RESIZE",
                "parse",
                "resize must be in WxH format (e.g., 1920x1080)",
            ));
            return EXIT_INPUT;
        }
        let w: f32 = match parts[0].parse() {
            Ok(v) => v,
            Err(_) => {
                print_json(&ErrorResponse::new(
                    "INVALID_RESIZE",
                    "parse",
                    "invalid width in resize",
                ));
                return EXIT_INPUT;
            }
        };
        let h: f32 = match parts[1].parse() {
            Ok(v) => v,
            Err(_) => {
                print_json(&ErrorResponse::new(
                    "INVALID_RESIZE",
                    "parse",
                    "invalid height in resize",
                ));
                return EXIT_INPUT;
            }
        };

        // Build resize map targeting first canvas root
        let mut resize_map = ode_core::ResizeMap::new();
        if let Some(&root_id) = doc.canvas.first() {
            resize_map.insert(root_id, (w, h));
        }

        match Scene::from_document_with_resize(doc, &font_db, &resize_map) {
            Ok(s) => s,
            Err(e) => {
                print_json(&ErrorResponse::new("RENDER_FAILED", "render", &e.to_string()));
                return EXIT_PROCESS;
            }
        }
    } else {
        match Scene::from_document(doc, &font_db) {
            Ok(s) => s,
            Err(e) => {
                print_json(&ErrorResponse::new("RENDER_FAILED", "render", &e.to_string()));
                return EXIT_PROCESS;
            }
        }
    };

    // ... rest of export logic unchanged ...
```

- [ ] **Step 5: Build and verify CLI compiles**

Run: `cargo build -p ode-cli`
Expected: Compiles without errors.

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/ode-cli/
git commit -m "feat(ode-cli): add --resize option to render and build commands

Supports WxH format (e.g., --resize 1920x1080) to resize the root
frame and apply constraints to its children."
```

---

### Task 7: Final verification

- [ ] **Step 1: Run cargo clippy**

Run: `cargo clippy --all-targets`
Expected: No warnings.

- [ ] **Step 2: Run full test suite**

Run: `cargo test --all`
Expected: All tests pass.

- [ ] **Step 3: Verify CLI help shows new option**

Run: `cargo run -p ode-cli -- render --help`
Expected: Shows `--resize <WxH>` option in output.

- [ ] **Step 4: Commit any fixes from clippy/tests**

If any fixes needed, commit them.
