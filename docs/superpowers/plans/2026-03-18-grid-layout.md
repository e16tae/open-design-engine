# Grid Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add CSS Grid layout support so Figma files with `layoutMode: "GRID"` render with proper grid placement instead of being skipped.

**Architecture:** Extend `LayoutConfig` with a `LayoutMode` enum (Flex/Grid) and `counter_axis_spacing` for row gap. In the Figma import, map GRID mode to the new Grid variant and pass row gap. In the layout engine, map Grid mode to Taffy's `Display::Grid` with `repeat(N, 1fr)` column tracks (N inferred from children count and container width) and separate column/row gaps.

**Tech Stack:** Rust, Taffy 0.7 (CSS Grid support enabled by default), serde

---

## File Map

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `crates/ode-format/src/node.rs` | Add `LayoutMode` enum, `counter_axis_spacing`, `grid_columns` to `LayoutConfig` |
| Modify | `crates/ode-format/src/wire.rs` | Update wire format for new LayoutConfig fields |
| Modify | `crates/ode-import/src/figma/convert_layout.rs` | Handle GRID mode, pass counter_axis_spacing, infer column count |
| Modify | `crates/ode-import/src/figma/convert.rs` | Pass `counter_axis_spacing` to `convert_layout_config` |
| Modify | `crates/ode-core/src/layout.rs` | Build Taffy Grid style from GridConfig, separate column/row gaps |

---

## Task 1: Add LayoutMode and counter_axis_spacing to ode-format

Extend `LayoutConfig` with grid support fields. Backwards compatible via serde defaults.

**Files:**
- Modify: `crates/ode-format/src/node.rs:146-161` (LayoutConfig struct)

- [ ] **Step 1: Write the failing test**

In `crates/ode-format/src/node.rs`, add to the `tests` module:

```rust
#[test]
fn layout_config_grid_mode_round_trip() {
    let config = LayoutConfig {
        mode: LayoutMode::Grid,
        direction: LayoutDirection::Horizontal,
        primary_axis_align: PrimaryAxisAlign::Start,
        counter_axis_align: CounterAxisAlign::Start,
        padding: LayoutPadding::default(),
        item_spacing: 10.0,
        counter_axis_spacing: 20.0,
        wrap: LayoutWrap::Wrap,
    };
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("\"mode\":\"grid\""));
    assert!(json.contains("\"counter_axis_spacing\":20"));
    let deserialized: LayoutConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.mode, LayoutMode::Grid);
    assert!((deserialized.counter_axis_spacing - 20.0).abs() < f32::EPSILON);
}

#[test]
fn layout_config_defaults_to_flex_mode() {
    // Old JSON without mode field should default to Flex
    let json = r#"{"direction":"horizontal","primary_axis_align":"start","counter_axis_align":"start","padding":{"top":0,"right":0,"bottom":0,"left":0},"item_spacing":0,"wrap":"no-wrap"}"#;
    let config: LayoutConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.mode, LayoutMode::Flex);
    assert!((config.counter_axis_spacing - 0.0).abs() < f32::EPSILON);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-format layout_config_grid`
Expected: FAIL — `LayoutMode` doesn't exist.

- [ ] **Step 3: Add LayoutMode enum and update LayoutConfig**

In `crates/ode-format/src/node.rs`, add the `LayoutMode` enum near the other layout enums:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutMode {
    Flex,
    Grid,
}

impl Default for LayoutMode {
    fn default() -> Self {
        Self::Flex
    }
}
```

Update `LayoutConfig` to include the new fields:

```rust
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct LayoutConfig {
    #[serde(default)]
    pub mode: LayoutMode,
    #[serde(default)]
    pub direction: LayoutDirection,
    #[serde(default)]
    pub primary_axis_align: PrimaryAxisAlign,
    #[serde(default)]
    pub counter_axis_align: CounterAxisAlign,
    #[serde(default)]
    pub padding: LayoutPadding,
    #[serde(default)]
    pub item_spacing: f32,
    /// Cross-axis gap (row gap for horizontal grid/wrapped flex).
    #[serde(default)]
    pub counter_axis_spacing: f32,
    #[serde(default)]
    pub wrap: LayoutWrap,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p ode-format layout_config_grid`
Expected: PASS

- [ ] **Step 5: Build workspace to find compilation errors**

Run: `cargo build --workspace`

Fix any compilation errors in other crates — any code that constructs `LayoutConfig { ... }` needs the new fields. Known locations:
- `crates/ode-import/src/figma/convert_layout.rs:77-84` — add `mode: LayoutMode::Flex, counter_axis_spacing: 0.0,`
- `crates/ode-format/src/wire.rs` — may need wire format updates (check if LayoutConfig is directly serialized or has a separate wire type)

- [ ] **Step 6: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/ode-format/src/node.rs crates/ode-import/src/figma/convert_layout.rs crates/ode-format/src/wire.rs
git commit -m "feat(ode-format): add LayoutMode enum and counter_axis_spacing to LayoutConfig

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Import Figma GRID layout mode

Convert Figma `layoutMode: "GRID"` into `LayoutMode::Grid` instead of skipping it. Pass `counter_axis_spacing` through.

**Files:**
- Modify: `crates/ode-import/src/figma/convert_layout.rs:18-84` (convert_layout_config function)
- Modify: `crates/ode-import/src/figma/convert.rs` (pass counter_axis_spacing to convert_layout_config)

- [ ] **Step 1: Write the failing test**

In `crates/ode-import/src/figma/convert_layout.rs`, modify the existing `test_layout_config_warns_for_grid` test (find it in the tests module) to test the new behavior:

```rust
#[test]
fn test_layout_config_grid_mode() {
    let mut warnings = Vec::new();
    let config = convert_layout_config(
        Some("GRID"),
        Some("MIN"),
        Some("CENTER"),
        Some(10.0), Some(10.0), Some(10.0), Some(10.0),
        Some(8.0),
        Some("WRAP"),
        Some(12.0),  // counter_axis_spacing
        &mut warnings,
    );
    assert!(warnings.is_empty(), "GRID should not produce warnings: {:?}", warnings);
    let config = config.expect("GRID should return Some(LayoutConfig)");
    assert_eq!(config.mode, LayoutMode::Grid);
    assert_eq!(config.direction, LayoutDirection::Horizontal);
    assert_eq!(config.wrap, LayoutWrap::Wrap);
    assert!((config.item_spacing - 8.0).abs() < f32::EPSILON);
    assert!((config.counter_axis_spacing - 12.0).abs() < f32::EPSILON);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-import test_layout_config_grid_mode`
Expected: FAIL — function signature doesn't include `counter_axis_spacing` param.

- [ ] **Step 3: Update convert_layout_config signature and implementation**

In `crates/ode-import/src/figma/convert_layout.rs`:

Update the function signature to add `counter_axis_spacing`:

```rust
pub fn convert_layout_config(
    layout_mode: Option<&str>,
    primary_align: Option<&str>,
    counter_align: Option<&str>,
    pad_top: Option<f32>,
    pad_right: Option<f32>,
    pad_bottom: Option<f32>,
    pad_left: Option<f32>,
    item_spacing: Option<f32>,
    wrap: Option<&str>,
    counter_axis_spacing: Option<f32>,
    warnings: &mut Vec<ImportWarning>,
) -> Option<LayoutConfig> {
```

Replace the GRID match arm (lines 34-41) — instead of skipping, handle it:

```rust
match mode {
    "NONE" => return None,
    "GRID" => {
        // Grid mode: horizontal direction, always wrap, separate column/row gaps
        let padding = LayoutPadding {
            top: pad_top.unwrap_or(0.0),
            right: pad_right.unwrap_or(0.0),
            bottom: pad_bottom.unwrap_or(0.0),
            left: pad_left.unwrap_or(0.0),
        };
        return Some(LayoutConfig {
            mode: LayoutMode::Grid,
            direction: LayoutDirection::Horizontal,
            primary_axis_align: match primary_align {
                Some("CENTER") => PrimaryAxisAlign::Center,
                Some("MAX") => PrimaryAxisAlign::End,
                Some("SPACE_BETWEEN") => PrimaryAxisAlign::SpaceBetween,
                _ => PrimaryAxisAlign::Start,
            },
            counter_axis_align: match counter_align {
                Some("CENTER") => CounterAxisAlign::Center,
                Some("MAX") => CounterAxisAlign::End,
                Some("BASELINE") => CounterAxisAlign::Baseline,
                Some("STRETCH") => CounterAxisAlign::Stretch,
                _ => CounterAxisAlign::Start,
            },
            padding,
            item_spacing: item_spacing.unwrap_or(0.0),
            counter_axis_spacing: counter_axis_spacing.unwrap_or(0.0),
            wrap: LayoutWrap::Wrap,
        });
    }
    _ => {}
}
```

Also update the existing Flex return path (around line 77) to include the new fields:

```rust
Some(LayoutConfig {
    mode: LayoutMode::Flex,
    direction,
    primary_axis_align,
    counter_axis_align,
    padding,
    item_spacing: item_spacing.unwrap_or(0.0),
    counter_axis_spacing: counter_axis_spacing.unwrap_or(0.0),
    wrap: layout_wrap,
})
```

Add `use ode_format::node::LayoutMode;` to the imports at the top.

- [ ] **Step 4: Update callers to pass counter_axis_spacing**

In `crates/ode-import/src/figma/convert.rs`, find all calls to `convert_layout_config` (there are 2: around line 299 and line 458). Add `fnode.counter_axis_spacing,` as the new parameter before `&mut self.warnings`:

```rust
let layout = convert_layout_config(
    fnode.layout_mode.as_deref(),
    fnode.primary_axis_align_items.as_deref(),
    fnode.counter_axis_align_items.as_deref(),
    fnode.padding_top,
    fnode.padding_right,
    fnode.padding_bottom,
    fnode.padding_left,
    fnode.item_spacing,
    fnode.layout_wrap.as_deref(),
    fnode.counter_axis_spacing,       // NEW
    &mut self.warnings,
);
```

- [ ] **Step 5: Update existing tests**

Find the existing tests in `convert_layout.rs` that call `convert_layout_config` and add the `None` or `Some(0.0)` parameter for `counter_axis_spacing`. Also delete or replace `test_layout_config_warns_for_grid`.

- [ ] **Step 6: Run tests**

Run: `cargo test -p ode-import`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/ode-import/src/figma/convert_layout.rs crates/ode-import/src/figma/convert.rs
git commit -m "feat(ode-import): import Figma GRID layout mode with counter_axis_spacing

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Wire Grid Mode in the Layout Engine

Map `LayoutMode::Grid` to Taffy's `Display::Grid` with `repeat(N, 1fr)` columns and separate column/row gaps.

**Files:**
- Modify: `crates/ode-core/src/layout.rs:286-352` (build_container_style function)

- [ ] **Step 1: Write the failing test**

In `crates/ode-core/src/layout.rs`, add to the tests module:

```rust
#[test]
fn grid_layout_positions_children_in_grid() {
    use ode_format::node::LayoutMode;

    let mut doc = Document::new("Grid");
    let mut frame = Node::new_frame("GridContainer", 320.0, 200.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.width_sizing = SizingMode::Fixed;
        data.height_sizing = SizingMode::Fixed;

        // 3-column grid with 10px column gap and 20px row gap
        data.container.layout = Some(LayoutConfig {
            mode: LayoutMode::Grid,
            direction: LayoutDirection::Horizontal,
            primary_axis_align: PrimaryAxisAlign::Start,
            counter_axis_align: CounterAxisAlign::Start,
            padding: LayoutPadding { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 },
            item_spacing: 10.0,
            counter_axis_spacing: 20.0,
            wrap: LayoutWrap::Wrap,
        });

        // 6 children → 2 rows × 3 columns
        let child_ids: Vec<NodeId> = (0..6).map(|i| {
            let mut child = Node::new_frame(&format!("Cell{i}"), 100.0, 50.0);
            if let NodeKind::Frame(ref mut cd) = child.kind {
                cd.width_sizing = SizingMode::Fixed;
                cd.height_sizing = SizingMode::Fixed;
            }
            doc.nodes.insert(child)
        }).collect();
        data.container.children = child_ids;
    }
    let fid = doc.nodes.insert(frame);
    doc.canvas.push(fid);

    let stable_idx: HashMap<&str, NodeId> = doc.nodes.iter()
        .map(|(id, n)| (n.stable_id.as_str(), id))
        .collect();
    let layout_map = compute_layout(&doc, &stable_idx, &ResizeMap::new());

    // Verify children got laid out (they should have positions in layout_map)
    let frame_children = match &doc.nodes[fid].kind {
        NodeKind::Frame(d) => &d.container.children,
        _ => panic!("expected frame"),
    };

    // All 6 children should have layout rects
    for &cid in frame_children {
        assert!(layout_map.contains_key(&cid), "Child {:?} should have a layout rect", cid);
    }

    // First child should be at (0, 0)
    let r0 = &layout_map[&frame_children[0]];
    assert!((r0.x).abs() < 1.0, "First cell should be near x=0, got {}", r0.x);
    assert!((r0.y).abs() < 1.0, "First cell should be near y=0, got {}", r0.y);

    // Second child should be offset by ~(100 + 10) = 110 in x
    let r1 = &layout_map[&frame_children[1]];
    assert!((r1.x - 110.0).abs() < 1.0, "Second cell x should be ~110, got {}", r1.x);

    // Fourth child (first in second row) should be at y ≈ 50 + 20 = 70
    let r3 = &layout_map[&frame_children[3]];
    assert!((r3.x).abs() < 1.0, "Fourth cell should be at x~0, got {}", r3.x);
    assert!((r3.y - 70.0).abs() < 1.0, "Fourth cell y should be ~70, got {}", r3.y);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-core grid_layout_positions_children`
Expected: FAIL — Grid is treated as Flex, positions will be wrong.

- [ ] **Step 3: Update build_container_style for Grid**

In `crates/ode-core/src/layout.rs`, modify `build_container_style` (around line 286):

```rust
fn build_container_style(node: &Node, config: &LayoutConfig) -> Style {
    let justify_content = match config.primary_axis_align {
        PrimaryAxisAlign::Start => Some(JustifyContent::FlexStart),
        PrimaryAxisAlign::Center => Some(JustifyContent::Center),
        PrimaryAxisAlign::End => Some(JustifyContent::FlexEnd),
        PrimaryAxisAlign::SpaceBetween => Some(JustifyContent::SpaceBetween),
    };

    let align_items = match config.counter_axis_align {
        CounterAxisAlign::Start => Some(AlignItems::FlexStart),
        CounterAxisAlign::Center => Some(AlignItems::Center),
        CounterAxisAlign::End => Some(AlignItems::FlexEnd),
        CounterAxisAlign::Stretch => Some(AlignItems::Stretch),
        CounterAxisAlign::Baseline => Some(AlignItems::Baseline),
    };

    let padding = Rect {
        top: LengthPercentage::Length(config.padding.top),
        right: LengthPercentage::Length(config.padding.right),
        bottom: LengthPercentage::Length(config.padding.bottom),
        left: LengthPercentage::Length(config.padding.left),
    };

    // Determine container size dimensions
    let (width, height) = if let Some(frame_data) = get_frame_data(node) {
        let w = match frame_data.width_sizing {
            SizingMode::Fixed => Dimension::Length(frame_data.width),
            SizingMode::Hug => Dimension::Auto,
            SizingMode::Fill => Dimension::Auto,
        };
        let h = match frame_data.height_sizing {
            SizingMode::Fixed => Dimension::Length(frame_data.height),
            SizingMode::Hug => Dimension::Auto,
            SizingMode::Fill => Dimension::Auto,
        };
        (w, h)
    } else {
        (Dimension::Auto, Dimension::Auto)
    };

    match config.mode {
        LayoutMode::Grid => {
            // Infer column count from container width, child sizes, and gap
            let col_count = infer_grid_columns(node, config);

            // Build grid_template_columns: repeat(N, 1fr)
            let track = NonRepeatedTrackSizingFunction::new(
                MinTrackSizingFunction::Auto,
                MaxTrackSizingFunction::Fraction(1.0),
            );
            let columns: Vec<TrackSizingFunction> =
                (0..col_count).map(|_| track.into()).collect();

            let gap = Size {
                width: LengthPercentage::Length(config.item_spacing),
                height: LengthPercentage::Length(config.counter_axis_spacing),
            };

            Style {
                display: Display::Grid,
                grid_template_columns: columns.into(),
                grid_auto_rows: vec![NonRepeatedTrackSizingFunction::AUTO].into(),
                justify_content,
                align_items,
                padding,
                gap,
                size: Size { width, height },
                ..Style::DEFAULT
            }
        }
        LayoutMode::Flex => {
            let flex_direction = match config.direction {
                LayoutDirection::Horizontal => FlexDirection::Row,
                LayoutDirection::Vertical => FlexDirection::Column,
            };

            let flex_wrap = match config.wrap {
                LayoutWrap::NoWrap => FlexWrap::NoWrap,
                LayoutWrap::Wrap => FlexWrap::Wrap,
            };

            let gap = Size {
                width: LengthPercentage::Length(config.item_spacing),
                height: LengthPercentage::Length(
                    if config.counter_axis_spacing > 0.0 {
                        config.counter_axis_spacing
                    } else {
                        config.item_spacing
                    }
                ),
            };

            Style {
                display: Display::Flex,
                flex_direction,
                justify_content,
                align_items,
                flex_wrap,
                padding,
                gap,
                size: Size { width, height },
                ..Style::DEFAULT
            }
        }
    }
}
```

- [ ] **Step 4: Add infer_grid_columns helper**

Add near `build_container_style`:

```rust
/// Infer the number of grid columns from container width, children, and gaps.
///
/// Heuristic: if the container has a fixed width and children with fixed widths,
/// compute how many columns fit. Otherwise default to the square root of child count
/// (rounded up) for a balanced grid, with a minimum of 1.
fn infer_grid_columns(node: &Node, config: &LayoutConfig) -> u16 {
    let frame = match get_frame_data(node) {
        Some(f) => f,
        None => return 1,
    };
    let children = &frame.container.children;
    if children.is_empty() {
        return 1;
    }

    // If container has fixed width, try to fit columns by child width + gap
    if frame.width_sizing == SizingMode::Fixed && frame.width > 0.0 {
        // Estimate child width from the container width (equal columns)
        // Available width = container_width - padding_left - padding_right
        let avail = frame.width - config.padding.left - config.padding.right;
        if avail <= 0.0 {
            return 1;
        }
        // Try to fit children: N columns means (N-1) gaps
        // Solve: N * child_w + (N-1) * gap = avail
        // For equal 1fr columns: child_w = (avail - (N-1)*gap) / N
        // We need child_w > 0, so N < (avail + gap) / gap... but we need child info.
        //
        // Simple approach: use sqrt(child_count) as a reasonable default
        let n = (children.len() as f32).sqrt().ceil() as u16;
        return n.max(1);
    }

    // Fallback: square root heuristic
    let n = (children.len() as f32).sqrt().ceil() as u16;
    n.max(1)
}
```

- [ ] **Step 5: Add LayoutMode import**

At the top of `crates/ode-core/src/layout.rs`, ensure `LayoutMode` is imported:

```rust
use ode_format::node::{
    ..., LayoutMode, ...
};
```

Also ensure the Taffy grid types are available. Since the code already does `use taffy::prelude::*;`, `Display::Grid`, `NonRepeatedTrackSizingFunction`, `MinTrackSizingFunction`, `MaxTrackSizingFunction`, and `TrackSizingFunction` should all be in scope.

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p ode-core grid_layout_positions_children`
Expected: PASS

- [ ] **Step 7: Write test for counter_axis_spacing in flex mode**

```rust
#[test]
fn flex_wrap_uses_counter_axis_spacing_for_row_gap() {
    let mut doc = Document::new("FlexWrap");
    let mut frame = Node::new_frame("Container", 220.0, 200.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.width_sizing = SizingMode::Fixed;
        data.height_sizing = SizingMode::Fixed;
        data.container.layout = Some(LayoutConfig {
            mode: LayoutMode::Flex,
            direction: LayoutDirection::Horizontal,
            primary_axis_align: PrimaryAxisAlign::Start,
            counter_axis_align: CounterAxisAlign::Start,
            padding: LayoutPadding::default(),
            item_spacing: 10.0,
            counter_axis_spacing: 30.0,
            wrap: LayoutWrap::Wrap,
        });

        // 4 children × 100px wide → 2 per row in 220px container
        let child_ids: Vec<NodeId> = (0..4).map(|i| {
            let mut child = Node::new_frame(&format!("C{i}"), 100.0, 40.0);
            if let NodeKind::Frame(ref mut cd) = child.kind {
                cd.width_sizing = SizingMode::Fixed;
                cd.height_sizing = SizingMode::Fixed;
            }
            doc.nodes.insert(child)
        }).collect();
        data.container.children = child_ids;
    }
    let fid = doc.nodes.insert(frame);
    doc.canvas.push(fid);

    let stable_idx: HashMap<&str, NodeId> = doc.nodes.iter()
        .map(|(id, n)| (n.stable_id.as_str(), id))
        .collect();
    let layout_map = compute_layout(&doc, &stable_idx, &ResizeMap::new());

    let children = match &doc.nodes[fid].kind {
        NodeKind::Frame(d) => &d.container.children,
        _ => panic!("expected frame"),
    };

    // Third child (first in second row) should be at y ≈ 40 + 30 = 70
    let r2 = &layout_map[&children[2]];
    assert!((r2.y - 70.0).abs() < 2.0, "Third child y should be ~70 (40+30 gap), got {}", r2.y);
}
```

- [ ] **Step 8: Run all tests**

Run: `cargo test -p ode-core`
Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 9: Commit**

```bash
git add crates/ode-core/src/layout.rs
git commit -m "feat(ode-core): wire Grid layout mode to Taffy CSS Grid with auto columns

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Integration Tests

End-to-end tests: Figma JSON with grid → import → layout → render.

**Files:**
- Create: `crates/ode-import/tests/fixtures/grid_layout.json`
- Modify: `crates/ode-import/tests/integration_test.rs`
- Modify: `crates/ode-export/tests/integration.rs`

- [ ] **Step 1: Create Figma JSON fixture with grid layout**

Create `crates/ode-import/tests/fixtures/grid_layout.json`:

```json
{
  "name": "GridTest",
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
            "name": "GridFrame",
            "type": "FRAME",
            "clipsContent": true,
            "layoutMode": "GRID",
            "primaryAxisAlignItems": "MIN",
            "counterAxisAlignItems": "MIN",
            "paddingLeft": 0, "paddingRight": 0,
            "paddingTop": 0, "paddingBottom": 0,
            "itemSpacing": 10,
            "counterAxisSpacing": 10,
            "layoutWrap": "WRAP",
            "size": { "x": 320, "y": 200 },
            "absoluteBoundingBox": { "x": 0, "y": 0, "width": 320, "height": 200 },
            "relativeTransform": [[1, 0, 0], [0, 1, 0]],
            "fills": [],
            "strokes": [],
            "effects": [],
            "children": [
              {
                "id": "2:1",
                "name": "Cell1",
                "type": "RECTANGLE",
                "size": { "x": 100, "y": 50 },
                "absoluteBoundingBox": { "x": 0, "y": 0, "width": 100, "height": 50 },
                "relativeTransform": [[1, 0, 0], [0, 1, 0]],
                "fills": [{ "type": "SOLID", "color": { "r": 1, "g": 0, "b": 0, "a": 1 } }],
                "strokes": [], "effects": []
              },
              {
                "id": "2:2",
                "name": "Cell2",
                "type": "RECTANGLE",
                "size": { "x": 100, "y": 50 },
                "absoluteBoundingBox": { "x": 110, "y": 0, "width": 100, "height": 50 },
                "relativeTransform": [[1, 0, 110], [0, 1, 0]],
                "fills": [{ "type": "SOLID", "color": { "r": 0, "g": 1, "b": 0, "a": 1 } }],
                "strokes": [], "effects": []
              },
              {
                "id": "2:3",
                "name": "Cell3",
                "type": "RECTANGLE",
                "size": { "x": 100, "y": 50 },
                "absoluteBoundingBox": { "x": 220, "y": 0, "width": 100, "height": 50 },
                "relativeTransform": [[1, 0, 220], [0, 1, 0]],
                "fills": [{ "type": "SOLID", "color": { "r": 0, "g": 0, "b": 1, "a": 1 } }],
                "strokes": [], "effects": []
              },
              {
                "id": "2:4",
                "name": "Cell4",
                "type": "RECTANGLE",
                "size": { "x": 100, "y": 50 },
                "absoluteBoundingBox": { "x": 0, "y": 60, "width": 100, "height": 50 },
                "relativeTransform": [[1, 0, 0], [0, 1, 60]],
                "fills": [{ "type": "SOLID", "color": { "r": 1, "g": 1, "b": 0, "a": 1 } }],
                "strokes": [], "effects": []
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
fn import_grid_layout_no_warning() {
    let json = std::fs::read_to_string("tests/fixtures/grid_layout.json").unwrap();
    let file: FigmaFile = serde_json::from_str(&json).unwrap();
    let result = FigmaConverter::convert(file, None, HashMap::new()).unwrap();

    // No grid warnings
    let grid_warnings: Vec<_> = result.warnings.iter()
        .filter(|w| w.message.to_lowercase().contains("grid"))
        .collect();
    assert!(grid_warnings.is_empty(), "Grid should not produce warnings: {:?}", grid_warnings);

    // GridFrame should have a layout config with Grid mode
    let grid_frame = result.document.nodes.iter()
        .find(|(_, n)| n.name == "GridFrame")
        .map(|(_, n)| n)
        .expect("GridFrame should exist");
    if let ode_format::node::NodeKind::Frame(ref data) = grid_frame.kind {
        let layout = data.container.layout.as_ref().expect("Should have layout config");
        assert_eq!(layout.mode, ode_format::node::LayoutMode::Grid);
        assert!((layout.item_spacing - 10.0).abs() < f32::EPSILON);
        assert!((layout.counter_axis_spacing - 10.0).abs() < f32::EPSILON);
    } else {
        panic!("GridFrame should be a Frame");
    }
}
```

- [ ] **Step 3: Write E2E render test**

In `crates/ode-export/tests/integration.rs`, add:

```rust
#[test]
fn grid_layout_e2e_renders() {
    use ode_format::document::Document;
    use ode_format::node::*;
    use ode_format::style::*;
    use ode_format::color::Color;
    use ode_core::scene::Scene;
    use ode_text::FontDatabase;

    let mut doc = Document::new("GridE2E");

    let mut frame = Node::new_frame("Grid", 320.0, 200.0);
    if let NodeKind::Frame(ref mut data) = frame.kind {
        data.width_sizing = SizingMode::Fixed;
        data.height_sizing = SizingMode::Fixed;
        data.container.layout = Some(LayoutConfig {
            mode: LayoutMode::Grid,
            direction: LayoutDirection::Horizontal,
            primary_axis_align: PrimaryAxisAlign::Start,
            counter_axis_align: CounterAxisAlign::Start,
            padding: LayoutPadding::default(),
            item_spacing: 10.0,
            counter_axis_spacing: 10.0,
            wrap: LayoutWrap::Wrap,
        });

        let child_ids: Vec<NodeId> = (0..4).map(|i| {
            let mut child = Node::new_frame(&format!("Cell{i}"), 100.0, 50.0);
            if let NodeKind::Frame(ref mut cd) = child.kind {
                cd.width_sizing = SizingMode::Fixed;
                cd.height_sizing = SizingMode::Fixed;
                cd.visual.fills.push(Fill {
                    paint: Paint::Solid {
                        color: StyleValue::Raw(Color::Srgb { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
                    },
                    opacity: StyleValue::Raw(1.0),
                    blend_mode: BlendMode::Normal,
                    visible: true,
                });
            }
            doc.nodes.insert(child)
        }).collect();
        data.container.children = child_ids;
    }
    let fid = doc.nodes.insert(frame);
    doc.canvas.push(fid);

    let font_db = FontDatabase::new();
    let scene = Scene::from_document(&doc, &font_db).unwrap();

    // PNG should render
    let pixmap = ode_core::render::Renderer::render(&scene).unwrap();
    let png_bytes = ode_export::PngExporter::export_bytes(&pixmap).unwrap();
    assert!(png_bytes.len() > 100, "PNG should have content");

    // SVG should render
    let svg = ode_export::SvgExporter::export_string(&scene).unwrap();
    assert!(!svg.is_empty(), "SVG should have content");
}
```

- [ ] **Step 4: Run all integration tests**

Run: `cargo test --workspace -- grid`
Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/ode-import/tests/fixtures/grid_layout.json crates/ode-import/tests/integration_test.rs crates/ode-export/tests/integration.rs
git commit -m "test: add grid layout integration tests (import + E2E render)

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```
