# Mask System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Figma-compatible mask clipping so that mask nodes clip subsequent siblings, and fix the clips_content bug where frames always clip regardless of the flag.

**Architecture:** Masks are stored as a boolean `is_mask` field on the common `Node` struct. During scene conversion, the children loop detects mask nodes, extracts their outline path, and wraps subsequent siblings in a `PushLayer` with that path as the clip. The existing clip infrastructure (PushLayer.clip → Mask in PNG renderer, clipPath in SVG, push_clip_path in PDF) handles the actual clipping — no exporter changes needed.

**Tech Stack:** Rust, kurbo (BezPath + Affine), tiny-skia (mask rendering), serde (serialization)

---

## File Map

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `crates/ode-format/src/node.rs` | Add `is_mask: bool` to `Node` struct + update constructors |
| Modify | `crates/ode-import/src/figma/convert.rs` | Set `is_mask` from Figma data, remove warning |
| Modify | `crates/ode-core/src/convert.rs` | Fix clips_content bug + implement mask rendering in children loop + component children loop |

---

## Task 1: Fix clips_content Bug

The `get_clip_path` function currently generates a clip for ALL frames, ignoring the `clips_content` flag. Frames with `clips_content: false` should not clip their children.

**Files:**
- Modify: `crates/ode-core/src/convert.rs:843-856`

- [ ] **Step 1: Write the failing test**

In `crates/ode-core/src/convert.rs`, add to the `tests` module:

```rust
#[test]
fn frame_clips_content_false_no_clip() {
    let mut doc = Document::new("NoClip");
    let mut frame = Node::new_frame("Root", 200.0, 200.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.clips_content = false;
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let fid = doc.nodes.insert(frame);
    doc.canvas.push(fid);
    let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
    // The PushLayer for this frame should have clip: None
    match &scene.commands[0] {
        RenderCommand::PushLayer { clip, .. } => {
            assert!(clip.is_none(), "clips_content=false should produce no clip");
        }
        other => panic!("Expected PushLayer, got {:?}", other),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-core frame_clips_content_false_no_clip`
Expected: FAIL — assertion fails because clip is `Some(...)`.

- [ ] **Step 3: Fix get_clip_path to check clips_content**

In `crates/ode-core/src/convert.rs`, replace the `get_clip_path` function (lines 843-856):

```rust
fn get_clip_path(
    node: &Node,
    layout_rect: Option<&crate::layout::LayoutRect>,
) -> Option<kurbo::BezPath> {
    if let NodeKind::Frame(ref data) = node.kind {
        if !data.clips_content {
            return None;
        }
        let (w, h) = layout_rect
            .map(|r| (r.width, r.height))
            .unwrap_or((data.width, data.height));
        if w > 0.0 && h > 0.0 {
            return Some(path::rounded_rect_path(w, h, data.corner_radius));
        }
    }
    None
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ode-core frame_clips_content_false_no_clip`
Expected: PASS

- [ ] **Step 5: Run all ode-core tests**

Run: `cargo test -p ode-core`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/ode-core/src/convert.rs
git commit -m "fix(ode-core): respect clips_content flag in get_clip_path"
```

---

## Task 2: Add is_mask Field to Node

Add `is_mask: bool` to the common `Node` struct so any node type can be a mask.

**Files:**
- Modify: `crates/ode-format/src/node.rs:563-580` (Node struct)
- Modify: `crates/ode-format/src/node.rs:592-734` (all `new_*` constructors)

- [ ] **Step 1: Write the failing test**

In `crates/ode-format/src/node.rs`, add to the existing tests module (or create one if it doesn't exist):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_is_mask_defaults_false() {
        let node = Node::new_frame("F", 100.0, 100.0);
        assert!(!node.is_mask);
    }

    #[test]
    fn node_is_mask_serialization_round_trip() {
        let mut node = Node::new_vector("Mask", VectorPath::default());
        node.is_mask = true;
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"is_mask\":true"));
        let deserialized: Node = serde_json::from_str(&json).unwrap();
        assert!(deserialized.is_mask);
    }

    #[test]
    fn node_is_mask_absent_in_json_defaults_false() {
        // Simulate loading an old .ode file without is_mask field
        let json = r#"{"stable_id":"abc","name":"V","transform":{"a":1,"b":0,"c":0,"d":1,"tx":0,"ty":0},"opacity":1,"blend_mode":"normal","visible":true,"constraints":null,"kind":{"type":"group","children":[]}}"#;
        let node: Node = serde_json::from_str(json).unwrap();
        assert!(!node.is_mask);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-format node_is_mask`
Expected: FAIL — `is_mask` field doesn't exist on Node.

- [ ] **Step 3: Add is_mask field to Node struct**

In `crates/ode-format/src/node.rs`, add the field to the `Node` struct (after `visible`):

```rust
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
    #[serde(default = "default_visible")]
    pub visible: bool,
    /// When true, this node's outline clips subsequent siblings (Figma mask semantics).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_mask: bool,
    pub constraints: Option<Constraints>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_sizing: Option<LayoutSizing>,
    pub kind: NodeKind,
}
```

Then add `is_mask: false,` to every `new_*` constructor (new_frame, new_group, new_vector, new_text, new_boolean_op, new_image, new_instance).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ode-format node_is_mask`
Expected: PASS

- [ ] **Step 5: Build entire workspace to check for missing fields**

Run: `cargo build --workspace`
Expected: PASS — if any code creates `Node { ... }` without `is_mask`, it will fail here.

- [ ] **Step 6: Fix any compilation errors**

If the workspace build reveals struct literal errors in other crates (e.g., ode-import's `convert_node` returns `Some(Node { ... })`), add `is_mask: false,` to those call sites. The one known location:

`crates/ode-import/src/figma/convert.rs:279-290` — add `is_mask: false,` (will be changed to use Figma data in Task 3).

- [ ] **Step 7: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/ode-format/src/node.rs crates/ode-import/src/figma/convert.rs
git commit -m "feat(ode-format): add is_mask field to Node for sibling mask clipping"
```

---

## Task 3: Import is_mask from Figma

Set `node.is_mask` from Figma's `isMask` field and remove the "not supported" warning.

**Files:**
- Modify: `crates/ode-import/src/figma/convert.rs:185-192` (mask warning block)
- Modify: `crates/ode-import/src/figma/convert.rs:279-290` (Node construction)

- [ ] **Step 1: Write the failing test**

In `crates/ode-import/src/figma/convert.rs`, modify the existing `convert_mask_node_warns` test:

```rust
#[test]
fn convert_mask_node_sets_is_mask() {
    let masked = FigmaNode {
        id: "2:1".to_string(),
        name: "MaskRect".to_string(),
        node_type: "RECTANGLE".to_string(),
        is_mask: Some(true),
        size: Some(FigmaVector { x: 50.0, y: 50.0 }),
        ..Default::default()
    };
    let frame = FigmaNode {
        id: "1:1".to_string(),
        name: "Frame".to_string(),
        node_type: "FRAME".to_string(),
        size: Some(FigmaVector { x: 100.0, y: 100.0 }),
        children: Some(vec![masked]),
        ..Default::default()
    };
    let file = make_file("Test", vec![frame]);
    let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();
    // Should have no warnings (mask is now supported)
    assert_eq!(
        result.warnings.len(),
        0,
        "Mask should not produce a warning; got: {:?}",
        result.warnings
    );
    // The mask node should have is_mask = true
    let mask_node = result
        .document
        .nodes
        .iter()
        .find(|(_, n)| n.name == "MaskRect")
        .map(|(_, n)| n)
        .expect("MaskRect node should exist");
    assert!(mask_node.is_mask, "MaskRect should have is_mask=true");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-import convert_mask_node_sets_is_mask`
Expected: FAIL — warning is still emitted, is_mask is false.

- [ ] **Step 3: Update convert_node to set is_mask**

In `crates/ode-import/src/figma/convert.rs`:

Remove the mask warning block (lines 185-192):
```rust
// DELETE THIS BLOCK:
// if fnode.is_mask == Some(true) {
//     self.warnings.push(ImportWarning { ... });
// }
```

Update the Node construction (around line 279) to include `is_mask`:
```rust
Some(Node {
    id: NodeId::default(),
    stable_id,
    name,
    transform,
    opacity,
    blend_mode,
    visible,
    is_mask: fnode.is_mask.unwrap_or(false),
    constraints,
    layout_sizing,
    kind,
})
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ode-import convert_mask_node_sets_is_mask`
Expected: PASS

- [ ] **Step 5: Delete old test**

Delete the old `convert_mask_node_warns` test since it tested the warning behavior that no longer exists (replaced by `convert_mask_node_sets_is_mask`).

- [ ] **Step 6: Run all ode-import tests**

Run: `cargo test -p ode-import`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/ode-import/src/figma/convert.rs
git commit -m "feat(ode-import): import Figma isMask flag into Node.is_mask"
```

---

## Task 4: Implement Mask Rendering in Scene Conversion

The core task: when converting children, detect mask nodes, extract their outline path, and wrap subsequent siblings in a `PushLayer` with that clip.

**Figma mask semantics:**
- A node with `is_mask = true` acts as a clip source
- All subsequent siblings (after the mask in the children array) are clipped by the mask's outline
- The mask node itself is NOT rendered (only provides its shape)
- A second mask node closes the previous mask group and starts a new one

**Files:**
- Modify: `crates/ode-core/src/convert.rs:174-187` (children loop in convert_node)

- [ ] **Step 1: Write the failing test — basic mask clips sibling**

```rust
#[test]
fn mask_node_clips_subsequent_siblings() {
    use ode_format::node::{VectorPath, PathSegment};

    let mut doc = Document::new("MaskTest");

    // Create a 50x50 rect vector as mask
    let mut mask_node = Node::new_vector("Mask", VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 50.0, y: 0.0 },
            PathSegment::LineTo { x: 50.0, y: 50.0 },
            PathSegment::LineTo { x: 0.0, y: 50.0 },
        ],
        closed: true,
    });
    mask_node.is_mask = true;
    let mask_id = doc.nodes.insert(mask_node);

    // Create a regular rectangle sibling (should be clipped)
    let mut sibling = Node::new_vector("Rect", VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 100.0, y: 0.0 },
            PathSegment::LineTo { x: 100.0, y: 100.0 },
            PathSegment::LineTo { x: 0.0, y: 100.0 },
        ],
        closed: true,
    });
    if let NodeKind::Vector(ref mut data) = sibling.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let sibling_id = doc.nodes.insert(sibling);

    // Parent frame containing [mask, sibling]
    let mut frame = Node::new_frame("Root", 200.0, 200.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.container.children = vec![mask_id, sibling_id];
    }
    let fid = doc.nodes.insert(frame);
    doc.canvas.push(fid);

    let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

    // Expected command structure:
    // PushLayer (frame, with frame clip)
    //   PushLayer (mask clip group — clip = mask's 50x50 rect)
    //     PushLayer (sibling)
    //       FillPath (sibling's red fill)
    //     PopLayer
    //   PopLayer (mask clip group end)
    // PopLayer (frame end)

    // Find the mask clip PushLayer: it should be the second PushLayer
    // and have a clip path that is NOT the frame's clip
    let push_layers: Vec<_> = scene.commands.iter().enumerate().filter(|(_, c)| {
        matches!(c, RenderCommand::PushLayer { clip: Some(_), .. })
    }).collect();

    // Should have 2 PushLayers with clips: frame clip + mask clip
    assert!(
        push_layers.len() >= 2,
        "Expected at least 2 PushLayers with clips (frame + mask), got {}",
        push_layers.len()
    );

    // The mask node itself should NOT produce any FillPath
    // (mask nodes are not rendered, only their shape is used as clip)
    // Count FillPath commands — should be 1 (only the sibling's fill)
    let fill_count = scene.commands.iter()
        .filter(|c| matches!(c, RenderCommand::FillPath { .. }))
        .count();
    assert_eq!(fill_count, 1, "Only the sibling's fill should be rendered, not the mask's");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-core mask_node_clips_subsequent_siblings`
Expected: FAIL — mask is rendered as a normal node (no clip, mask node produces PushLayer/PopLayer).

- [ ] **Step 3: Add helper function node_local_affine**

In `crates/ode-core/src/convert.rs`, add near the other helper functions:

```rust
/// Compute a node's local-to-parent affine transform (kurbo).
/// Used to transform mask paths from mask-local to parent-local coordinates.
fn node_local_affine(
    node: &Node,
    layout_rect: Option<&crate::layout::LayoutRect>,
) -> kurbo::Affine {
    let t = &node.transform;
    if let Some(rect) = layout_rect {
        kurbo::Affine::new([
            t.a as f64, t.b as f64,
            t.c as f64, t.d as f64,
            rect.x as f64, rect.y as f64,
        ])
    } else {
        kurbo::Affine::new([
            t.a as f64, t.b as f64,
            t.c as f64, t.d as f64,
            t.tx as f64, t.ty as f64,
        ])
    }
}
```

- [ ] **Step 4: Replace children loop with mask-aware version**

In `crates/ode-core/src/convert.rs`, replace the children loop (lines 174-187):

```rust
// Recurse into children (with mask support)
if let Some(children) = node.kind.children() {
    let mut mask_open = false;

    for &child_id in children {
        let child = &doc.nodes[child_id];

        if child.is_mask {
            // Close previous mask group if open
            if mask_open {
                commands.push(RenderCommand::PopLayer);
            }

            // Extract mask node's outline path
            let child_layout = layout_map.get(&child_id);
            if let Some(mut mask_path) = get_node_path(doc, child, child_layout) {
                // Transform path from mask-local to parent-local coordinates
                let affine = node_local_affine(child, child_layout);
                mask_path.apply_affine(affine);

                commands.push(RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: Some(mask_path),
                    transform: current_transform,
                });
                mask_open = true;
            }
            // Mask node is NOT rendered — skip to next sibling
            continue;
        }

        convert_node(
            doc,
            child_id,
            current_transform,
            commands,
            font_db,
            layout_map,
            stable_id_index,
        )?;
    }

    if mask_open {
        commands.push(RenderCommand::PopLayer);
    }
}
```

- [ ] **Step 5: Apply same mask logic to resolve_instance children loop**

In `crates/ode-core/src/convert.rs`, the `resolve_instance` function (around line 512-526) also iterates component children. Apply the same mask-aware pattern:

```rust
// Recurse into component's children (with mask support)
{
    let mut mask_open = false;
    for &child_id in &comp_frame.container.children {
        let child = &doc.nodes[child_id];

        if child.is_mask {
            if mask_open {
                commands.push(RenderCommand::PopLayer);
            }
            let child_layout = layout_map.get(&child_id);
            if let Some(mut mask_path) = get_node_path(doc, child, child_layout) {
                let affine = node_local_affine(child, child_layout);
                mask_path.apply_affine(affine);
                commands.push(RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: Some(mask_path),
                    transform: current_transform,
                });
                mask_open = true;
            }
            continue;
        }

        convert_component_child(
            doc,
            child_id,
            current_transform,
            commands,
            font_db,
            layout_map,
            stable_id_index,
            &override_map,
            resolution_stack,
            resolution_set,
        )?;
    }
    if mask_open {
        commands.push(RenderCommand::PopLayer);
    }
}
```

Also apply to the `convert_component_child` grandchildren loop (around line 691-707):

```rust
// Recurse into this child's children (with mask support)
if let Some(children) = child.kind.children() {
    let mut mask_open = false;
    for &grandchild_id in children {
        let grandchild = &doc.nodes[grandchild_id];

        if grandchild.is_mask {
            if mask_open {
                commands.push(RenderCommand::PopLayer);
            }
            let gc_layout = layout_map.get(&grandchild_id);
            if let Some(mut mask_path) = get_node_path(doc, grandchild, gc_layout) {
                let affine = node_local_affine(grandchild, gc_layout);
                mask_path.apply_affine(affine);
                commands.push(RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: Some(mask_path),
                    transform: current_transform,
                });
                mask_open = true;
            }
            continue;
        }

        convert_component_child(
            doc,
            grandchild_id,
            current_transform,
            commands,
            font_db,
            layout_map,
            stable_id_index,
            override_map,
            resolution_stack,
            resolution_set,
        )?;
    }
    if mask_open {
        commands.push(RenderCommand::PopLayer);
    }
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p ode-core mask_node_clips_subsequent_siblings`
Expected: PASS

- [ ] **Step 7: Write test — nodes before mask are not clipped**

```rust
#[test]
fn nodes_before_mask_are_not_clipped() {
    use ode_format::node::{VectorPath, PathSegment};

    let mut doc = Document::new("PreMask");

    // Regular node BEFORE mask (should NOT be clipped)
    let mut pre_node = Node::new_vector("PreRect", VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 100.0, y: 0.0 },
            PathSegment::LineTo { x: 100.0, y: 100.0 },
            PathSegment::LineTo { x: 0.0, y: 100.0 },
        ],
        closed: true,
    });
    if let NodeKind::Vector(ref mut data) = pre_node.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb { r: 0.0, g: 1.0, b: 0.0, a: 1.0 }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let pre_id = doc.nodes.insert(pre_node);

    // Mask node
    let mut mask_node = Node::new_vector("Mask", VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 50.0, y: 0.0 },
            PathSegment::LineTo { x: 50.0, y: 50.0 },
            PathSegment::LineTo { x: 0.0, y: 50.0 },
        ],
        closed: true,
    });
    mask_node.is_mask = true;
    let mask_id = doc.nodes.insert(mask_node);

    // Post-mask sibling (should be clipped)
    let mut post_node = Node::new_vector("PostRect", VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 80.0, y: 0.0 },
            PathSegment::LineTo { x: 80.0, y: 80.0 },
            PathSegment::LineTo { x: 0.0, y: 80.0 },
        ],
        closed: true,
    });
    if let NodeKind::Vector(ref mut data) = post_node.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let post_id = doc.nodes.insert(post_node);

    let mut frame = Node::new_frame("Root", 200.0, 200.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.clips_content = false; // no frame clip to simplify
        data.container.children = vec![pre_id, mask_id, post_id];
    }
    let fid = doc.nodes.insert(frame);
    doc.canvas.push(fid);

    let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();

    // Expected structure:
    // PushLayer (frame, no clip because clips_content=false)
    //   PushLayer (pre_node — no mask clip yet)
    //     FillPath (green)
    //   PopLayer
    //   PushLayer (mask clip group — clip = mask's 50x50 rect)
    //     PushLayer (post_node)
    //       FillPath (red)
    //     PopLayer
    //   PopLayer (mask clip)
    // PopLayer (frame)

    // Pre-mask node should NOT be inside any clip (other than frame, which is None)
    // The first child (pre_node) should have its PushLayer without a mask clip
    // We verify by checking the command sequence

    // There should be exactly 2 FillPath commands (green + red)
    let fill_count = scene.commands.iter()
        .filter(|c| matches!(c, RenderCommand::FillPath { .. }))
        .count();
    assert_eq!(fill_count, 2, "Both pre-mask and post-mask fills should render");

    // There should be exactly 1 PushLayer with a clip (the mask clip group)
    // (frame has clips_content=false, so no frame clip)
    let clip_count = scene.commands.iter()
        .filter(|c| matches!(c, RenderCommand::PushLayer { clip: Some(_), .. }))
        .count();
    assert_eq!(clip_count, 1, "Only the mask group should have a clip");
}
```

- [ ] **Step 8: Run test to verify it passes**

Run: `cargo test -p ode-core nodes_before_mask_are_not_clipped`
Expected: PASS

- [ ] **Step 9: Write test — mask node with no path is skipped**

```rust
#[test]
fn mask_group_node_skipped_no_path() {
    // A Group as mask has no own path — should be skipped gracefully
    let mut doc = Document::new("GroupMask");

    let mut mask_group = Node::new_group("MaskGroup");
    mask_group.is_mask = true;
    let mask_id = doc.nodes.insert(mask_group);

    let mut sibling = Node::new_frame("Child", 50.0, 50.0);
    if let NodeKind::Frame(ref mut data) = sibling.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let sib_id = doc.nodes.insert(sibling);

    let mut frame = Node::new_frame("Root", 200.0, 200.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.container.children = vec![mask_id, sib_id];
    }
    let fid = doc.nodes.insert(frame);
    doc.canvas.push(fid);

    // Should not panic — group mask is silently skipped (no clip applied)
    let scene = Scene::from_document(&doc, &empty_font_db()).unwrap();
    assert!(!scene.commands.is_empty());
}
```

- [ ] **Step 10: Run all ode-core tests**

Run: `cargo test -p ode-core`
Expected: All tests pass.

- [ ] **Step 11: Run full workspace tests**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 12: Commit**

```bash
git add crates/ode-core/src/convert.rs
git commit -m "feat(ode-core): implement mask node clipping for subsequent siblings"
```

---

## Task 5: End-to-End Integration Test

Verify the full pipeline: Figma JSON with mask → import → scene → render (PNG).

**Files:**
- Modify: `crates/ode-import/tests/fixtures/` (add mask test fixture)
- Modify: `crates/ode-export/tests/integration.rs` (add E2E test)

- [ ] **Step 1: Create Figma JSON fixture with mask**

Create `crates/ode-import/tests/fixtures/mask_basic.json`:

```json
{
  "name": "MaskTest",
  "document": {
    "id": "0:0",
    "name": "Document",
    "type": "DOCUMENT",
    "children": [
      {
        "id": "0:1",
        "name": "Page 1",
        "type": "CANVAS",
        "children": [
          {
            "id": "1:1",
            "name": "MaskedFrame",
            "type": "FRAME",
            "clipsContent": true,
            "size": { "x": 200, "y": 200 },
            "absoluteBoundingBox": { "x": 0, "y": 0, "width": 200, "height": 200 },
            "relativeTransform": [[1, 0, 0], [0, 1, 0]],
            "fills": [],
            "strokes": [],
            "effects": [],
            "children": [
              {
                "id": "2:1",
                "name": "MaskCircle",
                "type": "ELLIPSE",
                "isMask": true,
                "size": { "x": 100, "y": 100 },
                "absoluteBoundingBox": { "x": 50, "y": 50, "width": 100, "height": 100 },
                "relativeTransform": [[1, 0, 50], [0, 1, 50]],
                "fills": [{ "type": "SOLID", "color": { "r": 0, "g": 0, "b": 0, "a": 1 } }],
                "strokes": [],
                "effects": []
              },
              {
                "id": "2:2",
                "name": "MaskedRect",
                "type": "RECTANGLE",
                "size": { "x": 200, "y": 200 },
                "absoluteBoundingBox": { "x": 0, "y": 0, "width": 200, "height": 200 },
                "relativeTransform": [[1, 0, 0], [0, 1, 0]],
                "fills": [{ "type": "SOLID", "color": { "r": 1, "g": 0, "b": 0, "a": 1 } }],
                "strokes": [],
                "effects": []
              }
            ]
          }
        ]
      }
    ]
  },
  "components": {},
  "componentSets": {},
  "styles": {}
}
```

- [ ] **Step 2: Write import integration test**

In `crates/ode-import/tests/integration_test.rs`, add:

```rust
#[test]
fn import_mask_basic_sets_is_mask() {
    let json = std::fs::read_to_string("tests/fixtures/mask_basic.json").unwrap();
    let file: FigmaFile = serde_json::from_str(&json).unwrap();
    let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();

    // No mask warnings (mask is supported now)
    let mask_warnings: Vec<_> = result.warnings.iter()
        .filter(|w| w.message.contains("Mask") || w.message.contains("mask"))
        .collect();
    assert!(mask_warnings.is_empty(), "No mask warnings expected: {:?}", mask_warnings);

    // Find the mask node
    let mask_node = result.document.nodes.iter()
        .find(|(_, n)| n.name == "MaskCircle")
        .map(|(_, n)| n)
        .expect("MaskCircle should exist");
    assert!(mask_node.is_mask, "MaskCircle should have is_mask=true");

    // The masked sibling should NOT have is_mask
    let sibling = result.document.nodes.iter()
        .find(|(_, n)| n.name == "MaskedRect")
        .map(|(_, n)| n)
        .expect("MaskedRect should exist");
    assert!(!sibling.is_mask, "MaskedRect should not be a mask");
}
```

- [ ] **Step 3: Write E2E render test**

In `crates/ode-export/tests/integration.rs`, add:

```rust
#[test]
fn mask_e2e_renders_without_panic() {
    // Build a document with a mask programmatically
    use ode_format::document::Document;
    use ode_format::node::*;
    use ode_format::style::*;
    use ode_format::color::Color;
    use ode_core::scene::Scene;
    use ode_text::FontDatabase;

    let mut doc = Document::new("MaskE2E");

    let mut mask = Node::new_vector("Mask", VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 80.0, y: 0.0 },
            PathSegment::LineTo { x: 80.0, y: 80.0 },
            PathSegment::LineTo { x: 0.0, y: 80.0 },
        ],
        closed: true,
    });
    mask.is_mask = true;
    let mask_id = doc.nodes.insert(mask);

    let mut rect = Node::new_vector("Rect", VectorPath {
        segments: vec![
            PathSegment::MoveTo { x: 0.0, y: 0.0 },
            PathSegment::LineTo { x: 200.0, y: 0.0 },
            PathSegment::LineTo { x: 200.0, y: 200.0 },
            PathSegment::LineTo { x: 0.0, y: 200.0 },
        ],
        closed: true,
    });
    if let NodeKind::Vector(ref mut data) = rect.kind {
        data.visual.fills.push(Fill {
            paint: Paint::Solid {
                color: StyleValue::Raw(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
            },
            opacity: StyleValue::Raw(1.0),
            blend_mode: BlendMode::Normal,
            visible: true,
        });
    }
    let rect_id = doc.nodes.insert(rect);

    let mut frame = Node::new_frame("Root", 200.0, 200.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.container.children = vec![mask_id, rect_id];
    }
    let fid = doc.nodes.insert(frame);
    doc.canvas.push(fid);

    let font_db = FontDatabase::new();
    let scene = Scene::from_document(&doc, &font_db).unwrap();

    // PNG render
    let pixmap = ode_core::render::Renderer::render(&scene).unwrap();
    let png_bytes = ode_export::PngExporter::export_bytes(&pixmap).unwrap();
    assert!(png_bytes.len() > 100, "PNG should have content");

    // SVG render
    let svg = ode_export::SvgExporter::export_string(&scene).unwrap();
    assert!(svg.contains("clipPath"), "SVG should contain a clipPath for the mask");

    // PDF render
    let pdf_bytes = ode_export::PdfExporter::export_bytes(&scene).unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF"), "Should produce valid PDF");
}
```

- [ ] **Step 4: Run integration tests**

Run: `cargo test --workspace -- mask`
Expected: All mask-related tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ode-import/tests/fixtures/mask_basic.json crates/ode-import/tests/integration_test.rs crates/ode-export/tests/integration.rs
git commit -m "test: add mask system integration tests (import + E2E render)"
```
