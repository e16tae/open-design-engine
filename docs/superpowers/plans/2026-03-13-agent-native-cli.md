# Agent-Native CLI Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite ode-cli as a declarative, document-first CLI optimized for AI agents, migrate .ode.json to StableId-based references, and remove ode-mcp.

**Architecture:** Wire-type pattern for serialization — runtime types keep NodeId, JSON uses StableId via intermediate "wire" structs. Document-level custom Serialize/Deserialize with 2-pass deserialization. CLI outputs JSON-only on stdout with structured error codes.

**Tech Stack:** Rust 2024, clap 4, serde, schemars 0.8, tiny-skia, anyhow, thiserror

**Spec:** `docs/superpowers/specs/2026-03-13-agent-native-cli-design.md`

---

## File Structure

### ode-format (modified)
| File | Responsibility |
|------|---------------|
| `src/lib.rs` | Add `wire` module export |
| `src/wire.rs` | **NEW** — Wire types for JSON representation + Document Serialize/Deserialize |
| `src/document.rs` | Remove `Serialize`/`Deserialize` derives, bump version to 0.2.0 |
| `src/node.rs` | Remove `Serialize`/`Deserialize` from `NodeTree`, keep on `Node` internals |
| `Cargo.toml` | Add `schemars` dependency |
| `tests/integration.rs` | Update all roundtrip tests for new format |

### ode-cli (rewritten)
| File | Responsibility |
|------|---------------|
| `src/main.rs` | Arg parsing (clap), command dispatch |
| `src/output.rs` | **NEW** — JSON output types (CliResult, CliError), serialization helpers |
| `src/validate.rs` | **NEW** — Validation engine (reference checks, cycle detection, schema) |
| `src/commands.rs` | **NEW** — All 6 command implementations |
| `Cargo.toml` | Add `schemars` dependency |
| `tests/integration.rs` | **NEW** — CLI integration tests |

### Workspace root
| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Remove `ode-mcp` from members, add `schemars` to workspace deps |

### Deleted
| File | Reason |
|------|--------|
| `crates/ode-mcp/` | Entire crate removed (MCP deprecated) |

---

## Chunk 1: ode-format StableId Serialization Migration

### Task 1: Remove ode-mcp and update workspace

**Files:**
- Delete: `crates/ode-mcp/` (entire directory)
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Delete ode-mcp crate**

```bash
rm -rf crates/ode-mcp
```

- [ ] **Step 2: Remove ode-mcp from workspace members and add schemars**

In `Cargo.toml` (workspace root), remove `"crates/ode-mcp"` from `members` and add schemars to `[workspace.dependencies]`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/ode-format",
    "crates/ode-core",
    "crates/ode-export",
    "crates/ode-cli",
]

# ... in [workspace.dependencies] section, add:

# JSON Schema generation (serde_json feature needed for serde_json::Value support)
schemars = { version = "0.8", features = ["serde_json"] }
```

- [ ] **Step 3: Add schemars to ode-format Cargo.toml**

Add to `crates/ode-format/Cargo.toml` `[dependencies]`:

```toml
schemars = { workspace = true }
```

- [ ] **Step 4: Verify workspace compiles**

Run: `cargo check --workspace`
Expected: Compiles with no errors (ode-mcp gone, schemars added)

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: remove ode-mcp crate, add schemars dependency"
```

---

### Task 2: Add JsonSchema derives to leaf types

Types that don't contain `NodeId` can get `JsonSchema` derive directly. These are the "leaf" types shared between wire and runtime.

**Files:**
- Modify: `crates/ode-format/src/color.rs`
- Modify: `crates/ode-format/src/style.rs`
- Modify: `crates/ode-format/src/typography.rs`
- Modify: `crates/ode-format/src/tokens.rs`
- Modify: `crates/ode-format/src/node.rs` (leaf types only: Transform, Constraints, VectorPath, PathSegment, FillRule, BooleanOperation, ComponentDef)

- [ ] **Step 1: Add schemars import and derive to color.rs**

In `crates/ode-format/src/color.rs`, add `use schemars::JsonSchema;` and add `JsonSchema` to the derive of `Color`:

```rust
use schemars::JsonSchema;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "space", rename_all = "lowercase")]
pub enum Color {
```

- [ ] **Step 2: Add JsonSchema to style.rs types**

In `crates/ode-format/src/style.rs`, add `use schemars::JsonSchema;` and add `JsonSchema` derive to ALL types: `TokenRef`, `StyleValue<T>`, `Point`, `BlendMode`, `Paint`, `GradientStop`, `MeshGradientData`, `MeshPoint`, `ImageSource`, `ImageFillMode`, `Fill`, `Stroke`, `StrokePosition`, `StrokeCap`, `StrokeJoin`, `DashPattern`, `Effect`, `VisualProps`.

For `StyleValue<T>`, add a description to help agents understand the untagged enum:

```rust
/// A value that is either a raw value or bound to a design token.
/// Raw: bare value (e.g., `1.0`). Bound: `{"token":{...},"resolved":...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum StyleValue<T> {
```

- [ ] **Step 3: Add JsonSchema to typography.rs types**

In `crates/ode-format/src/typography.rs`, add `use schemars::JsonSchema;` and add `JsonSchema` derive to: `TextStyle`, `LineHeight`, `TextAlign`, `VerticalAlign`, `TextDecoration`, `TextTransform`, `OpenTypeFeature`, `VariableFontAxis`. Note: `FontFamily` and `FontWeight` are type aliases (`String` / `u16`) — they already implement `JsonSchema` via their base types, do NOT try to add derives to them.

- [ ] **Step 4: Add JsonSchema to tokens.rs types**

In `crates/ode-format/src/tokens.rs`, add `use schemars::JsonSchema;` and add `JsonSchema` derive to: `TokenType`, `DimensionUnit`, `TokenValue`, `TokenResolve`, `Token`, `Mode`, `TokenCollection`, `DesignTokens`.

For `DesignTokens`, skip the `#[serde(skip)]` fields in schema:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DesignTokens {
    pub collections: Vec<TokenCollection>,
    pub active_modes: HashMap<CollectionId, ModeId>,
    #[serde(skip)]
    #[schemars(skip)]
    next_collection_id: CollectionId,
    #[serde(skip)]
    #[schemars(skip)]
    next_token_id: TokenId,
    #[serde(skip)]
    #[schemars(skip)]
    next_mode_id: ModeId,
}
```

- [ ] **Step 5: Add JsonSchema to node.rs leaf types**

In `crates/ode-format/src/node.rs`, add `use schemars::JsonSchema;` and add `JsonSchema` derive to: `Transform`, `ConstraintAxis`, `Constraints`, `LayoutConfig`, `BooleanOperation`, `VectorPath`, `PathSegment`, `FillRule`, `ComponentDef`.

Also add `JsonSchema` to `VectorData`, `TextData`, `ImageData` — these do NOT contain `NodeId` and are reused directly in wire types (VectorData is used as `NodeKindWire::Vector(Box<VectorData>)`).

Do NOT add `JsonSchema` to `NodeId`, `NodeTree`, `Node`, `NodeKind`, `ContainerProps`, `FrameData`, `GroupData`, `BooleanOpData`, `InstanceData` — these contain `NodeId` and will use wire types for schema.

- [ ] **Step 6: Add JsonSchema to document.rs leaf types**

In `crates/ode-format/src/document.rs`, add `use schemars::JsonSchema;` and add `JsonSchema` derive to: `Version`, `ViewId`, `WorkingColorSpace`.

Do NOT add to `View`, `ViewKind`, `Document` — these contain `NodeId`.

- [ ] **Step 7: Add schemars verification test for adjacently-tagged enum**

Add to `crates/ode-format/src/tokens.rs` test module:

```rust
    #[test]
    fn token_value_schema_generates() {
        // Verify schemars handles adjacently-tagged enum without panic
        let schema = schemars::schema_for!(TokenValue);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("TokenValue"));
    }
```

- [ ] **Step 8: Verify compilation and run schema test**

Run: `cargo check --workspace && cargo test -p ode-format token_value_schema`
Expected: Compiles. Schema test passes.

- [ ] **Step 9: Commit**

```bash
git add crates/ode-format/src/
git commit -m "feat(ode-format): add JsonSchema derives to leaf types"
```

---

### Task 3: Create wire module with wire types

The wire module contains intermediate types that represent the JSON format exactly. Types that contain `NodeId` references get a wire counterpart using `String` (StableId).

**Files:**
- Create: `crates/ode-format/src/wire.rs`
- Modify: `crates/ode-format/src/lib.rs`

- [ ] **Step 1: Write the test for wire type round-trip**

Add to end of `crates/ode-format/src/wire.rs` (file will be created in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::style::*;

    #[test]
    fn document_wire_roundtrip() {
        let wire = DocumentWire {
            format_version: crate::document::Version(0, 2, 0),
            name: "Test".to_string(),
            nodes: vec![
                NodeWire {
                    stable_id: "root".to_string(),
                    name: "Root".to_string(),
                    transform: crate::node::Transform::default(),
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    constraints: None,
                    kind: NodeKindWire::Frame(Box::new(FrameDataWire {
                        width: 100.0,
                        height: 100.0,
                        corner_radius: [0.0; 4],
                        visual: VisualProps::default(),
                        container: ContainerPropsWire {
                            children: vec!["child1".to_string()],
                            layout: None,
                        },
                        component_def: None,
                    })),
                },
                NodeWire {
                    stable_id: "child1".to_string(),
                    name: "Child".to_string(),
                    transform: crate::node::Transform::default(),
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    constraints: None,
                    kind: NodeKindWire::Vector(Box::new(crate::node::VectorData {
                        visual: VisualProps::default(),
                        path: crate::node::VectorPath::default(),
                        fill_rule: crate::node::FillRule::default(),
                    })),
                },
            ],
            canvas: vec!["root".to_string()],
            tokens: crate::tokens::DesignTokens::new(),
            views: vec![],
            working_color_space: crate::document::WorkingColorSpace::Srgb,
        };
        let json = serde_json::to_string_pretty(&wire).unwrap();
        let parsed: DocumentWire = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test");
        assert_eq!(parsed.nodes.len(), 2);
        assert_eq!(parsed.canvas, vec!["root"]);
        // Verify children are StableId strings
        if let NodeKindWire::Frame(ref data) = parsed.nodes[0].kind {
            assert_eq!(data.container.children, vec!["child1"]);
        } else {
            panic!("Expected Frame");
        }
    }

    #[test]
    fn view_kind_wire_roundtrip() {
        let view = ViewWire {
            id: crate::document::ViewId(0),
            name: "Web View".to_string(),
            kind: ViewKindWire::Web { root: "page-root".to_string() },
        };
        let json = serde_json::to_string(&view).unwrap();
        let parsed: ViewWire = serde_json::from_str(&json).unwrap();
        if let ViewKindWire::Web { root } = parsed.kind {
            assert_eq!(root, "page-root");
        } else {
            panic!("Expected Web view kind");
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-format wire`
Expected: FAIL — module `wire` doesn't exist yet

- [ ] **Step 3: Create wire.rs with wire types**

Create `crates/ode-format/src/wire.rs`:

```rust
//! Wire types for .ode.json serialization.
//!
//! These types represent the JSON format exactly — using `StableId` (String)
//! for all node references instead of runtime `NodeId`. Types that don't
//! contain `NodeId` are reused directly from their original modules.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::document::{Version, ViewId, WorkingColorSpace};
use crate::node::{
    BooleanOperation, ComponentDef, Constraints, FillRule, LayoutConfig,
    StableId, Transform, VectorData, VectorPath,
};
use crate::style::{BlendMode, VisualProps};
use crate::tokens::DesignTokens;

// ─── DocumentWire ───

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentWire {
    pub format_version: Version,
    pub name: String,
    pub nodes: Vec<NodeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canvas: Vec<String>,
    #[serde(default)]
    pub tokens: DesignTokens,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<ViewWire>,
    #[serde(default)]
    pub working_color_space: WorkingColorSpace,
}

// ─── NodeWire ───

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeWire {
    pub stable_id: StableId,
    pub name: String,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub blend_mode: BlendMode,
    pub constraints: Option<Constraints>,
    pub kind: NodeKindWire,
}

fn default_opacity() -> f32 { 1.0 }

// ─── NodeKindWire ───

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NodeKindWire {
    Frame(Box<FrameDataWire>),
    Group(Box<GroupDataWire>),
    Vector(Box<VectorData>),
    BooleanOp(Box<BooleanOpDataWire>),
    Text(Box<TextDataWire>),
    Image(Box<ImageDataWire>),
    Instance(Box<InstanceDataWire>),
}

// ─── Kind-Specific Wire Data ───

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FrameDataWire {
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub corner_radius: [f32; 4],
    #[serde(default)]
    pub visual: VisualProps,
    #[serde(default)]
    pub container: ContainerPropsWire,
    pub component_def: Option<ComponentDef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ContainerPropsWire {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    pub layout: Option<LayoutConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GroupDataWire {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BooleanOpDataWire {
    #[serde(default)]
    pub visual: VisualProps,
    pub op: BooleanOperation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TextDataWire {
    #[serde(default)]
    pub visual: VisualProps,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImageDataWire {
    #[serde(default)]
    pub visual: VisualProps,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InstanceDataWire {
    #[serde(default)]
    pub container: ContainerPropsWire,
    pub source_component: StableId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<serde_json::Value>,
}

// ─── ViewWire ───

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ViewWire {
    pub id: ViewId,
    pub name: String,
    pub kind: ViewKindWire,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ViewKindWire {
    Print { pages: Vec<String> },
    Web { root: String },
    Presentation { slides: Vec<String> },
    Export { targets: Vec<serde_json::Value> },
}

// Tests at the end (from Step 1)
```

Append the tests from Step 1 at the end of this file.

- [ ] **Step 4: Add wire module to lib.rs**

In `crates/ode-format/src/lib.rs`, add:

```rust
pub mod wire;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ode-format wire`
Expected: PASS — 2 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/src/wire.rs crates/ode-format/src/lib.rs
git commit -m "feat(ode-format): add wire types for StableId-based JSON serialization"
```

---

### Task 4: Implement Document ↔ Wire conversion and custom Serialize/Deserialize

This is the core of the migration. Implement conversion between runtime types (with NodeId) and wire types (with StableId), then use custom Serialize/Deserialize for Document.

**Files:**
- Modify: `crates/ode-format/src/wire.rs` (add conversion + custom serde)
- Modify: `crates/ode-format/src/document.rs` (remove derives)
- Modify: `crates/ode-format/src/node.rs` (remove NodeTree Serialize/Deserialize)

- [ ] **Step 1: Write test for Document JSON roundtrip with StableId format**

Add to `crates/ode-format/src/wire.rs` tests module:

```rust
    #[test]
    fn document_to_wire_and_back() {
        use crate::document::Document;
        use crate::node::Node;

        // Build a document with parent-child relationship
        let mut doc = Document::new("Roundtrip");
        let mut frame = Node::new_frame("Parent", 200.0, 100.0);
        let child = Node::new_text("Child", "Hello");
        let child_stable_id = child.stable_id.clone();

        let child_id = doc.nodes.insert(child);
        if let crate::node::NodeKind::Frame(ref mut data) = frame.kind {
            data.container.children.push(child_id);
        }
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);

        // Serialize (Document → JSON via wire)
        let json = serde_json::to_string_pretty(&doc).unwrap();

        // JSON should contain StableId strings, not opaque NodeId numbers
        assert!(json.contains(&child_stable_id));
        assert!(!json.contains("\"key\""), "Should not expose slotmap keys");

        // Deserialize (JSON → Document via wire)
        let parsed: Document = serde_json::from_str(&json).unwrap();

        // Verify structure survived roundtrip
        assert_eq!(parsed.name, "Roundtrip");
        assert_eq!(parsed.canvas.len(), 1);
        let parsed_frame = &parsed.nodes[parsed.canvas[0]];
        let children = parsed_frame.kind.children().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(parsed.nodes[children[0]].name, "Child");
    }

    #[test]
    fn document_wire_with_views() {
        use crate::document::{Document, View, ViewId};
        use crate::node::Node;

        let mut doc = Document::new("View Test");
        let frame = Node::new_frame("Page", 1440.0, 900.0);
        let frame_stable_id = frame.stable_id.clone();
        let frame_id = doc.nodes.insert(frame);
        doc.canvas.push(frame_id);
        doc.views.push(View {
            id: ViewId(0),
            name: "Web".to_string(),
            kind: crate::document::ViewKind::Web { root: frame_id },
        });

        let json = serde_json::to_string_pretty(&doc).unwrap();
        // View should reference stable_id string
        assert!(json.contains(&frame_stable_id));

        let parsed: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.views.len(), 1);
        if let crate::document::ViewKind::Web { root } = parsed.views[0].kind {
            assert_eq!(parsed.nodes[root].name, "Page");
        } else {
            panic!("Expected Web view");
        }
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-format document_to_wire_and_back`
Expected: FAIL — custom Serialize/Deserialize not yet implemented

- [ ] **Step 3: Remove Serialize/Deserialize derives from Document and NodeTree FIRST**

This MUST happen before adding custom impls to avoid conflicting implementations.

In `crates/ode-format/src/document.rs`, remove derives from `Document`:

```rust
// Change:
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
// To:
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
```

In `crates/ode-format/src/node.rs`, remove derives from `NodeTree` and add `iter()`:

```rust
// Change:
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NodeTree(SlotMap<NodeId, Node>);
// To:
#[derive(Debug, Default, Clone)]
pub struct NodeTree(SlotMap<NodeId, Node>);
```

Add `iter` method to `NodeTree`:

```rust
impl NodeTree {
    // ... existing methods ...

    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.0.iter()
    }
}
```

Bump format version in `Document::new`:

```rust
format_version: Version(0, 2, 0),
```

**NOTE:** The workspace will NOT compile after this step until Step 4 adds the custom impls. Do Steps 3 and 4 together without intermediate compilation.

- [ ] **Step 4: Add conversion functions and custom Serialize/Deserialize to wire.rs**

Add above the tests module in `crates/ode-format/src/wire.rs`:

```rust
use std::collections::HashMap;
use crate::document::{Document, View, ViewKind};
use crate::node::{
    Node, NodeId, NodeKind, NodeTree, ContainerProps,
    FrameData, GroupData, BooleanOpData, TextData, ImageData, InstanceData,
};

// ─── Error ───

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("unknown stable_id reference: {0}")]
    UnknownReference(String),
}

// ─── Document → DocumentWire (for serialization) ───

impl DocumentWire {
    pub fn from_document(doc: &Document) -> Self {
        // Build NodeId → StableId lookup
        let id_to_stable: HashMap<NodeId, &str> = doc.nodes.iter()
            .map(|(id, node)| (id, node.stable_id.as_str()))
            .collect();

        let nodes = doc.nodes.iter()
            .map(|(_, node)| NodeWire::from_node(node, &id_to_stable))
            .collect();

        let canvas = doc.canvas.iter()
            .filter_map(|id| id_to_stable.get(id).map(|s| s.to_string()))
            .collect();

        let views = doc.views.iter()
            .map(|v| ViewWire::from_view(v, &id_to_stable))
            .collect();

        DocumentWire {
            format_version: doc.format_version.clone(),
            name: doc.name.clone(),
            nodes,
            canvas,
            tokens: doc.tokens.clone(),
            views,
            working_color_space: doc.working_color_space,
        }
    }
}

impl NodeWire {
    fn from_node(node: &Node, lookup: &HashMap<NodeId, &str>) -> Self {
        NodeWire {
            stable_id: node.stable_id.clone(),
            name: node.name.clone(),
            transform: node.transform,
            opacity: node.opacity,
            blend_mode: node.blend_mode,
            constraints: node.constraints,
            kind: NodeKindWire::from_kind(&node.kind, lookup),
        }
    }
}

impl NodeKindWire {
    fn from_kind(kind: &NodeKind, lookup: &HashMap<NodeId, &str>) -> Self {
        let map_children = |ids: &[NodeId]| -> Vec<String> {
            ids.iter()
                .filter_map(|id| lookup.get(id).map(|s| s.to_string()))
                .collect()
        };
        let map_container = |c: &ContainerProps| -> ContainerPropsWire {
            ContainerPropsWire {
                children: map_children(&c.children),
                layout: c.layout.clone(),
            }
        };

        match kind {
            NodeKind::Frame(d) => NodeKindWire::Frame(Box::new(FrameDataWire {
                width: d.width,
                height: d.height,
                corner_radius: d.corner_radius,
                visual: d.visual.clone(),
                container: map_container(&d.container),
                component_def: d.component_def.clone(),
            })),
            NodeKind::Group(d) => NodeKindWire::Group(Box::new(GroupDataWire {
                children: map_children(&d.children),
            })),
            NodeKind::Vector(d) => NodeKindWire::Vector(d.clone()),
            NodeKind::BooleanOp(d) => NodeKindWire::BooleanOp(Box::new(BooleanOpDataWire {
                visual: d.visual.clone(),
                op: d.op,
                children: map_children(&d.children),
            })),
            NodeKind::Text(d) => NodeKindWire::Text(Box::new(TextDataWire {
                visual: d.visual.clone(),
                content: d.content.clone(),
            })),
            NodeKind::Image(d) => NodeKindWire::Image(Box::new(ImageDataWire {
                visual: d.visual.clone(),
            })),
            NodeKind::Instance(d) => NodeKindWire::Instance(Box::new(InstanceDataWire {
                container: map_container(&d.container),
                source_component: d.source_component.clone(),
                overrides: d.overrides.clone(),
            })),
        }
    }
}

impl ViewWire {
    fn from_view(view: &View, lookup: &HashMap<NodeId, &str>) -> Self {
        let map_id = |id: &NodeId| -> String {
            lookup.get(id).map(|s| s.to_string()).unwrap_or_default()
        };
        ViewWire {
            id: view.id,
            name: view.name.clone(),
            kind: match &view.kind {
                ViewKind::Print { pages } => ViewKindWire::Print {
                    pages: pages.iter().map(|id| map_id(id)).collect(),
                },
                ViewKind::Web { root } => ViewKindWire::Web {
                    root: map_id(root),
                },
                ViewKind::Presentation { slides } => ViewKindWire::Presentation {
                    slides: slides.iter().map(|id| map_id(id)).collect(),
                },
                ViewKind::Export { targets } => ViewKindWire::Export {
                    targets: targets.clone(),
                },
            },
        }
    }
}

// ─── DocumentWire → Document (for deserialization — 2-pass) ───

impl DocumentWire {
    pub fn into_document(self) -> Result<Document, WireError> {
        let mut tree = NodeTree::new();
        let mut stable_to_id: HashMap<String, NodeId> = HashMap::new();

        // Pass 1: Insert all nodes, build stable_id → NodeId mapping
        // Store wire nodes alongside their assigned NodeIds for pass 2
        let mut wire_nodes: Vec<(NodeId, NodeWire)> = Vec::with_capacity(self.nodes.len());
        for nw in self.nodes {
            let stable_id = nw.stable_id.clone();
            let node = Node {
                id: NodeId::default(), // will be overwritten by slotmap
                stable_id: nw.stable_id.clone(),
                name: nw.name.clone(),
                transform: nw.transform,
                opacity: nw.opacity,
                blend_mode: nw.blend_mode,
                constraints: nw.constraints,
                kind: NodeKind::Group(Box::new(GroupData { children: vec![] })), // placeholder
            };
            let id = tree.insert(node);
            stable_to_id.insert(stable_id, id);
            wire_nodes.push((id, nw));
        }

        let resolve = |s: &str| -> Result<NodeId, WireError> {
            stable_to_id.get(s).copied().ok_or_else(|| WireError::UnknownReference(s.to_string()))
        };
        let resolve_vec = |v: &[String]| -> Result<Vec<NodeId>, WireError> {
            v.iter().map(|s| resolve(s)).collect()
        };

        // Pass 2: Resolve all StableId references → NodeId
        for (id, nw) in wire_nodes {
            let kind = match nw.kind {
                NodeKindWire::Frame(d) => NodeKind::Frame(Box::new(FrameData {
                    width: d.width,
                    height: d.height,
                    corner_radius: d.corner_radius,
                    visual: d.visual,
                    container: ContainerProps {
                        children: resolve_vec(&d.container.children)?,
                        layout: d.container.layout,
                    },
                    component_def: d.component_def,
                })),
                NodeKindWire::Group(d) => NodeKind::Group(Box::new(GroupData {
                    children: resolve_vec(&d.children)?,
                })),
                NodeKindWire::Vector(d) => NodeKind::Vector(d),
                NodeKindWire::BooleanOp(d) => NodeKind::BooleanOp(Box::new(BooleanOpData {
                    visual: d.visual,
                    op: d.op,
                    children: resolve_vec(&d.children)?,
                })),
                NodeKindWire::Text(d) => NodeKind::Text(Box::new(TextData {
                    visual: d.visual,
                    content: d.content,
                })),
                NodeKindWire::Image(d) => NodeKind::Image(Box::new(ImageData {
                    visual: d.visual,
                })),
                NodeKindWire::Instance(d) => NodeKind::Instance(Box::new(InstanceData {
                    container: ContainerProps {
                        children: resolve_vec(&d.container.children)?,
                        layout: d.container.layout,
                    },
                    source_component: d.source_component,
                    overrides: d.overrides,
                })),
            };
            tree[id].kind = kind;
        }

        // Resolve canvas
        let canvas = resolve_vec(&self.canvas)?;

        // Resolve views
        let views = self.views.into_iter().map(|vw| {
            let kind = match vw.kind {
                ViewKindWire::Print { pages } => {
                    ViewKind::Print { pages: resolve_vec(&pages)? }
                }
                ViewKindWire::Web { root } => {
                    ViewKind::Web { root: resolve(&root)? }
                }
                ViewKindWire::Presentation { slides } => {
                    ViewKind::Presentation { slides: resolve_vec(&slides)? }
                }
                ViewKindWire::Export { targets } => {
                    ViewKind::Export { targets }
                }
            };
            Ok(View { id: vw.id, name: vw.name, kind })
        }).collect::<Result<Vec<_>, WireError>>()?;

        Ok(Document {
            format_version: self.format_version,
            name: self.name,
            nodes: tree,
            canvas,
            tokens: self.tokens,
            views,
            working_color_space: self.working_color_space,
        })
    }
}

// ─── Custom Serialize/Deserialize for Document ───

impl Serialize for Document {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        DocumentWire::from_document(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = DocumentWire::deserialize(deserializer)?;
        wire.into_document().map_err(serde::de::Error::custom)
    }
}
```

- [ ] **Step 5: Run the new tests**

Run: `cargo test -p ode-format document_to_wire_and_back document_wire_with_views`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/
git commit -m "feat(ode-format): implement StableId-based Document serialization via wire types"
```

---

### Task 5: Update existing tests for new format

All existing tests that rely on Document JSON roundtrip or specific format version need updating.

**Files:**
- Modify: `crates/ode-format/src/document.rs` (unit tests)
- Modify: `crates/ode-format/tests/integration.rs`
- Modify: `crates/ode-export/tests/integration.rs`

- [ ] **Step 1: Update document.rs unit tests**

In `crates/ode-format/src/document.rs`, update the `document_roundtrip_json` test's version assertion:

```rust
    #[test]
    fn document_roundtrip_json() {
        let doc = Document::new("Roundtrip Test");
        let json = serde_json::to_string_pretty(&doc).unwrap();
        let parsed: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc.name, parsed.name);
        assert_eq!(parsed.format_version, Version(0, 2, 0));
    }
```

Update `add_frame_to_canvas` test — the PartialEq comparison on `Document` still works since `NodeTree::eq` compares by stable_id. But we need to verify that the JSON roundtrip works for docs with nodes. This test doesn't do JSON roundtrip so it should be fine as-is.

Update the `create_empty_document` test version assertion:

```rust
    #[test]
    fn create_empty_document() {
        let doc = Document::new("My Design");
        assert_eq!(doc.name, "My Design");
        assert_eq!(doc.format_version, Version(0, 2, 0));
        assert!(doc.canvas.is_empty());
        assert!(doc.views.is_empty());
    }
```

- [ ] **Step 2: Update ode-format integration tests**

In `crates/ode-format/tests/integration.rs`:

Update `full_document_roundtrip` — change version assertion and verify StableId format:

```rust
    assert_eq!(parsed.format_version, ode_format::document::Version(0, 2, 0));
```

The rest of the test logic (checking children, canvas, views) should work as-is since the runtime types haven't changed — only the JSON representation.

- [ ] **Step 3: Run all ode-format tests**

Run: `cargo test -p ode-format`
Expected: ALL PASS

- [ ] **Step 4: Run ode-export integration tests**

Run: `cargo test -p ode-export`
Expected: ALL PASS (these tests create Documents programmatically, never touch JSON serialization)

- [ ] **Step 5: Run full workspace tests**

Run: `cargo test --workspace`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add crates/
git commit -m "test: update all tests for v0.2.0 StableId-based format"
```

---

## Chunk 2: CLI Infrastructure

### Task 6: Create CLI output module

**Files:**
- Create: `crates/ode-cli/src/output.rs`
- Modify: `crates/ode-cli/Cargo.toml`

- [ ] **Step 1: Add serde to ode-cli Cargo.toml**

In `crates/ode-cli/Cargo.toml`, add:

```toml
serde = { workspace = true }
schemars = { workspace = true }
```

- [ ] **Step 2: Create output.rs with JSON output types**

Create `crates/ode-cli/src/output.rs`:

```rust
use serde::Serialize;

// ─── Exit codes ───

pub const EXIT_OK: i32 = 0;
pub const EXIT_INPUT: i32 = 1;    // parse + validation errors
pub const EXIT_IO: i32 = 2;       // file I/O errors
pub const EXIT_PROCESS: i32 = 3;  // render + export errors
pub const EXIT_INTERNAL: i32 = 4; // unexpected errors

// ─── Success responses ───

#[derive(Serialize)]
pub struct OkResponse {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

impl OkResponse {
    pub fn simple() -> Self {
        Self { status: "ok", path: None, width: None, height: None, warnings: vec![] }
    }

    pub fn with_path(path: &str) -> Self {
        Self { status: "ok", path: Some(path.to_string()), width: None, height: None, warnings: vec![] }
    }

    pub fn with_render(path: &str, width: u32, height: u32) -> Self {
        Self { status: "ok", path: Some(path.to_string()), width: Some(width), height: Some(height), warnings: vec![] }
    }
}

// ─── Validation responses ───

#[derive(Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ValidationIssue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

#[derive(Serialize)]
pub struct ValidationIssue {
    pub path: String,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Serialize)]
pub struct Warning {
    pub path: String,
    pub code: String,
    pub message: String,
}

// ─── Error responses ───

#[derive(Serialize)]
pub struct ErrorResponse {
    pub status: &'static str,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ValidationIssue>,
}

impl ErrorResponse {
    pub fn new(code: &str, phase: &str, message: &str) -> Self {
        Self {
            status: "error",
            code: code.to_string(),
            phase: Some(phase.to_string()),
            message: message.to_string(),
            suggestion: None,
            errors: vec![],
        }
    }

    pub fn validation(errors: Vec<ValidationIssue>) -> Self {
        Self {
            status: "error",
            code: "VALIDATION_FAILED".to_string(),
            phase: Some("validate".to_string()),
            message: format!("{} validation error(s)", errors.len()),
            suggestion: None,
            errors,
        }
    }
}

// ─── Print helpers ───

pub fn print_json<T: Serialize>(value: &T) {
    println!("{}", serde_json::to_string(value).unwrap_or_else(|e| {
        format!(r#"{{"status":"error","code":"INTERNAL","message":"JSON serialization failed: {}"}}"#, e)
    }));
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p ode-cli`
Expected: Compiles (output.rs is not yet used, but should compile)

- [ ] **Step 4: Commit**

```bash
git add crates/ode-cli/
git commit -m "feat(ode-cli): add JSON output types and helpers"
```

---

### Task 7: Create validation engine

**Files:**
- Create: `crates/ode-cli/src/validate.rs`

- [ ] **Step 1: Write validation engine tests**

Add to end of `crates/ode-cli/src/validate.rs` (file created in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_json() -> String {
        r#"{
            "format_version": [0, 2, 0],
            "name": "Test",
            "nodes": [
                {"stable_id": "root", "name": "Root", "kind": {"type": "frame", "width": 100, "height": 100, "visual": {}, "container": {}, "component_def": null}}
            ],
            "canvas": ["root"],
            "tokens": {"collections": [], "active_modes": {}},
            "views": []
        }"#.to_string()
    }

    #[test]
    fn valid_document_passes() {
        let result = validate_json(&make_valid_json());
        assert!(result.valid, "Expected valid, got errors: {:?}", result.errors);
    }

    #[test]
    fn duplicate_stable_id_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "dup", "name": "A", "kind": {"type": "frame", "width": 10, "height": 10, "visual": {}, "container": {}, "component_def": null}},
                {"stable_id": "dup", "name": "B", "kind": {"type": "frame", "width": 10, "height": 10, "visual": {}, "container": {}, "component_def": null}}
            ],
            "canvas": ["dup"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "DUPLICATE_ID"));
    }

    #[test]
    fn invalid_reference_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "root", "name": "Root", "kind": {"type": "frame", "width": 10, "height": 10, "visual": {}, "container": {"children": ["nonexistent"]}, "component_def": null}}
            ],
            "canvas": ["root"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "INVALID_REFERENCE"));
    }

    #[test]
    fn invalid_canvas_reference_detected() {
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [],
            "canvas": ["missing"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "INVALID_REFERENCE"));
    }

    #[test]
    fn circular_hierarchy_detected() {
        // A's child is B, B's child is A
        let json = r#"{
            "format_version": [0, 2, 0], "name": "Test",
            "nodes": [
                {"stable_id": "a", "name": "A", "kind": {"type": "frame", "width": 10, "height": 10, "visual": {}, "container": {"children": ["b"]}, "component_def": null}},
                {"stable_id": "b", "name": "B", "kind": {"type": "frame", "width": 10, "height": 10, "visual": {}, "container": {"children": ["a"]}, "component_def": null}}
            ],
            "canvas": ["a"], "tokens": {"collections": [], "active_modes": {}}, "views": []
        }"#;
        let result = validate_json(json);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "CIRCULAR_HIERARCHY"));
    }

    #[test]
    fn parse_error_returns_parse_failed() {
        let result = validate_json("not json at all");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "PARSE_FAILED"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-cli validate`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement validation engine**

Create `crates/ode-cli/src/validate.rs`:

```rust
use std::collections::{HashMap, HashSet};
use ode_format::wire::{
    DocumentWire, NodeKindWire, ViewKindWire,
};
use crate::output::{ValidateResponse, ValidationIssue, Warning};

pub fn validate_json(json: &str) -> ValidateResponse {
    // Phase 1: Parse
    let wire: DocumentWire = match serde_json::from_str(json) {
        Ok(w) => w,
        Err(e) => return ValidateResponse {
            valid: false,
            errors: vec![ValidationIssue {
                path: String::new(),
                code: "PARSE_FAILED".to_string(),
                message: e.to_string(),
                suggestion: None,
            }],
            warnings: vec![],
        },
    };

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Collect all stable_ids
    let mut id_set: HashSet<&str> = HashSet::new();
    let mut all_ids: Vec<&str> = Vec::new();
    for (i, node) in wire.nodes.iter().enumerate() {
        if !id_set.insert(&node.stable_id) {
            errors.push(ValidationIssue {
                path: format!("nodes[{}].stable_id", i),
                code: "DUPLICATE_ID".to_string(),
                message: format!("duplicate stable_id '{}'", node.stable_id),
                suggestion: None,
            });
        }
        all_ids.push(&node.stable_id);
    }

    // Helper: check reference validity
    let available: String = format!("{:?}", all_ids);
    let mut check_ref = |path: &str, ref_id: &str| {
        if !id_set.contains(ref_id) {
            errors.push(ValidationIssue {
                path: path.to_string(),
                code: "INVALID_REFERENCE".to_string(),
                message: format!("referenced stable_id '{}' not found", ref_id),
                suggestion: Some(format!("available stable_ids: {}", available)),
            });
        }
    };

    // Check children references
    for (i, node) in wire.nodes.iter().enumerate() {
        let children = get_children_wire(&node.kind);
        for (j, child_id) in children.iter().enumerate() {
            check_ref(
                &format!("nodes[{}].kind.children[{}]", i, j),
                child_id,
            );
        }
    }

    // Check canvas references
    for (i, canvas_id) in wire.canvas.iter().enumerate() {
        check_ref(&format!("canvas[{}]", i), canvas_id);
    }

    // Check view references
    for (i, view) in wire.views.iter().enumerate() {
        match &view.kind {
            ViewKindWire::Print { pages } => {
                for (j, p) in pages.iter().enumerate() {
                    check_ref(&format!("views[{}].kind.pages[{}]", i, j), p);
                }
            }
            ViewKindWire::Web { root } => {
                check_ref(&format!("views[{}].kind.root", i), root);
            }
            ViewKindWire::Presentation { slides } => {
                for (j, s) in slides.iter().enumerate() {
                    check_ref(&format!("views[{}].kind.slides[{}]", i, j), s);
                }
            }
            ViewKindWire::Export { .. } => {}
        }
    }

    // Check circular hierarchy
    if errors.is_empty() {
        check_circular_hierarchy(&wire, &mut errors);
    }

    // Check Instance source_component references
    check_component_refs(&wire, &id_set, &mut errors);

    // Check token cycles by attempting resolution
    check_token_cycles(&wire, &mut errors);

    // Warnings: CMYK colors that will fall back
    check_cmyk_warnings(&wire, &mut warnings);

    ValidateResponse {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

fn get_children_wire(kind: &NodeKindWire) -> Vec<&str> {
    match kind {
        NodeKindWire::Frame(d) => d.container.children.iter().map(|s| s.as_str()).collect(),
        NodeKindWire::Group(d) => d.children.iter().map(|s| s.as_str()).collect(),
        NodeKindWire::BooleanOp(d) => d.children.iter().map(|s| s.as_str()).collect(),
        NodeKindWire::Instance(d) => d.container.children.iter().map(|s| s.as_str()).collect(),
        NodeKindWire::Vector(_) | NodeKindWire::Text(_) | NodeKindWire::Image(_) => vec![],
    }
}

fn check_circular_hierarchy(wire: &DocumentWire, errors: &mut Vec<ValidationIssue>) {
    // Build adjacency: stable_id → children stable_ids
    let adj: HashMap<&str, Vec<&str>> = wire.nodes.iter()
        .map(|n| (n.stable_id.as_str(), get_children_wire(&n.kind)))
        .collect();

    // DFS cycle detection
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    for node in &wire.nodes {
        if !visited.contains(node.stable_id.as_str()) {
            if has_cycle(&adj, &node.stable_id, &mut visited, &mut in_stack) {
                errors.push(ValidationIssue {
                    path: format!("nodes (stable_id='{}')", node.stable_id),
                    code: "CIRCULAR_HIERARCHY".to_string(),
                    message: "circular parent-child relationship detected".to_string(),
                    suggestion: None,
                });
                break; // Report once
            }
        }
    }
}

fn has_cycle<'a>(
    adj: &HashMap<&'a str, Vec<&'a str>>,
    node: &'a str,
    visited: &mut HashSet<&'a str>,
    in_stack: &mut HashSet<&'a str>,
) -> bool {
    visited.insert(node);
    in_stack.insert(node);

    if let Some(children) = adj.get(node) {
        for child in children {
            if !visited.contains(child) {
                if has_cycle(adj, child, visited, in_stack) {
                    return true;
                }
            } else if in_stack.contains(child) {
                return true;
            }
        }
    }

    in_stack.remove(node);
    false
}

fn check_component_refs(wire: &DocumentWire, id_set: &HashSet<&str>, errors: &mut Vec<ValidationIssue>) {
    // Build a set of stable_ids that have component_def
    let component_ids: HashSet<&str> = wire.nodes.iter()
        .filter(|n| matches!(&n.kind, NodeKindWire::Frame(d) if d.component_def.is_some()))
        .map(|n| n.stable_id.as_str())
        .collect();

    for (i, node) in wire.nodes.iter().enumerate() {
        if let NodeKindWire::Instance(ref d) = node.kind {
            if !id_set.contains(d.source_component.as_str()) {
                errors.push(ValidationIssue {
                    path: format!("nodes[{}].kind.source_component", i),
                    code: "INVALID_COMPONENT_REF".to_string(),
                    message: format!("source_component '{}' not found", d.source_component),
                    suggestion: None,
                });
            } else if !component_ids.contains(d.source_component.as_str()) {
                errors.push(ValidationIssue {
                    path: format!("nodes[{}].kind.source_component", i),
                    code: "INVALID_COMPONENT_REF".to_string(),
                    message: format!("source_component '{}' exists but has no component_def", d.source_component),
                    suggestion: None,
                });
            }
        }
    }
}

fn check_token_cycles(wire: &DocumentWire, errors: &mut Vec<ValidationIssue>) {
    // Attempt to resolve all tokens — cycles will surface as TokenError::CyclicAlias
    for col in &wire.tokens.collections {
        for tok in &col.tokens {
            if let Err(e) = wire.tokens.resolve(col.id, tok.id) {
                if matches!(e, ode_format::tokens::TokenError::CyclicAlias) {
                    errors.push(ValidationIssue {
                        path: format!("tokens.collections[{}].tokens[{}]", col.id, tok.id),
                        code: "CYCLIC_TOKEN".to_string(),
                        message: format!("token '{}' has a cyclic alias", tok.name),
                        suggestion: None,
                    });
                }
            }
        }
    }
}

fn check_cmyk_warnings(wire: &DocumentWire, warnings: &mut Vec<Warning>) {
    // Scan fills for CMYK colors — they fall back to black in PNG export
    for (i, node) in wire.nodes.iter().enumerate() {
        let visual = match &node.kind {
            NodeKindWire::Frame(d) => Some(&d.visual),
            NodeKindWire::Vector(d) => Some(&d.visual),
            NodeKindWire::BooleanOp(d) => Some(&d.visual),
            NodeKindWire::Text(d) => Some(&d.visual),
            NodeKindWire::Image(d) => Some(&d.visual),
            NodeKindWire::Group(_) | NodeKindWire::Instance(_) => None,
        };
        if let Some(vis) = visual {
            for (j, fill) in vis.fills.iter().enumerate() {
                if let ode_format::style::Paint::Solid { ref color } = fill.paint {
                    if let ode_format::style::StyleValue::Raw(ref c) = color {
                        if matches!(c, ode_format::color::Color::Cmyk { .. }) {
                            warnings.push(Warning {
                                path: format!("nodes[{}].kind.visual.fills[{}].paint.color", i, j),
                                code: "CMYK_FALLBACK".to_string(),
                                message: "CMYK color will fall back to black in PNG export".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: Add validate module to main.rs temporarily**

In `crates/ode-cli/src/main.rs`, add at top:

```rust
mod output;
mod validate;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p ode-cli validate`
Expected: ALL 6 PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ode-cli/
git commit -m "feat(ode-cli): add validation engine with reference, cycle, and parse checks"
```

---

### Task 8: Rewrite CLI main with all 6 commands

**Files:**
- Create: `crates/ode-cli/src/commands.rs`
- Rewrite: `crates/ode-cli/src/main.rs`

- [ ] **Step 1: Create commands.rs**

Create `crates/ode-cli/src/commands.rs`:

```rust
use std::path::Path;
use anyhow::{Context, Result};
use ode_core::{Renderer, Scene};
use ode_export::PngExporter;
use ode_format::Document;
use ode_format::wire::DocumentWire;
use crate::output::*;
use crate::validate::validate_json;

pub fn load_input(file: &str) -> Result<String, (i32, ErrorResponse)> {
    if file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)
            .map_err(|e| (EXIT_IO, ErrorResponse::new("IO_ERROR", "io", &e.to_string())))?;
        Ok(buf)
    } else {
        std::fs::read_to_string(file)
            .map_err(|e| (EXIT_IO, ErrorResponse::new("IO_ERROR", "io",
                &format!("failed to read '{}': {}", file, e))))
    }
}

// ─── ode new ───

pub fn cmd_new(file: &str, name: Option<&str>, width: Option<f32>, height: Option<f32>) -> i32 {
    let mut doc = Document::new(name.unwrap_or("Untitled"));

    if let (Some(w), Some(h)) = (width, height) {
        let frame = ode_format::node::Node::new_frame("Root", w, h);
        let id = doc.nodes.insert(frame);
        doc.canvas.push(id);
    }

    let json = match serde_json::to_string_pretty(&doc) {
        Ok(j) => j,
        Err(e) => {
            print_json(&ErrorResponse::new("INTERNAL", "serialize", &e.to_string()));
            return EXIT_INTERNAL;
        }
    };

    if let Err(e) = std::fs::write(file, &json) {
        print_json(&ErrorResponse::new("IO_ERROR", "io", &e.to_string()));
        return EXIT_IO;
    }

    print_json(&OkResponse::with_path(file));
    EXIT_OK
}

// ─── ode validate ───

pub fn cmd_validate(file: &str) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => { print_json(&err); return code; }
    };

    let result = validate_json(&json);
    let exit = if result.valid { EXIT_OK } else { EXIT_INPUT };
    print_json(&result);
    exit
}

// ─── ode build ───

pub fn cmd_build(file: &str, output: &str) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => { print_json(&err); return code; }
    };

    // Validate first
    let validation = validate_json(&json);
    if !validation.valid {
        print_json(&ErrorResponse::validation(validation.errors));
        return EXIT_INPUT;
    }

    // Parse into Document
    let doc: Document = match serde_json::from_str(&json) {
        Ok(d) => d,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
            return EXIT_INPUT;
        }
    };

    // Convert + Render + Export
    render_and_export(&doc, output, validation.warnings)
}

// ─── ode render ───

pub fn cmd_render(file: &str, output: &str) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => { print_json(&err); return code; }
    };

    let doc: Document = match serde_json::from_str(&json) {
        Ok(d) => d,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
            return EXIT_INPUT;
        }
    };

    render_and_export(&doc, output, vec![])
}

fn render_and_export(doc: &Document, output: &str, warnings: Vec<Warning>) -> i32 {
    let scene = match Scene::from_document(doc) {
        Ok(s) => s,
        Err(e) => {
            print_json(&ErrorResponse::new("RENDER_FAILED", "render", &e.to_string()));
            return EXIT_PROCESS;
        }
    };

    let pixmap = match Renderer::render(&scene) {
        Ok(p) => p,
        Err(e) => {
            print_json(&ErrorResponse::new("RENDER_FAILED", "render", &e.to_string()));
            return EXIT_PROCESS;
        }
    };

    if let Err(e) = PngExporter::export(&pixmap, Path::new(output)) {
        print_json(&ErrorResponse::new("EXPORT_FAILED", "export", &e.to_string()));
        return EXIT_PROCESS;
    }

    let mut resp = OkResponse::with_render(output, pixmap.width(), pixmap.height());
    resp.warnings = warnings;
    print_json(&resp);
    EXIT_OK
}

// ─── ode inspect ───

pub fn cmd_inspect(file: &str, full: bool) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => { print_json(&err); return code; }
    };

    if full {
        // Full mode: output the raw wire representation
        let wire: DocumentWire = match serde_json::from_str(&json) {
            Ok(w) => w,
            Err(e) => {
                print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
                return EXIT_INPUT;
            }
        };
        print_json(&wire);
    } else {
        // Summary mode: tree view
        let wire: DocumentWire = match serde_json::from_str(&json) {
            Ok(w) => w,
            Err(e) => {
                print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
                return EXIT_INPUT;
            }
        };
        let summary = build_inspect_summary(&wire);
        print_json(&summary);
    }
    EXIT_OK
}

#[derive(serde::Serialize)]
struct InspectSummary {
    name: String,
    format_version: String,
    working_color_space: String,
    node_count: usize,
    canvas: Vec<String>,
    tree: Vec<InspectNode>,
    tokens: TokensSummary,
}

#[derive(serde::Serialize)]
struct InspectNode {
    stable_id: String,
    name: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<InspectNode>,
}

#[derive(serde::Serialize)]
struct TokensSummary {
    collections: Vec<String>,
    total_tokens: usize,
}

fn build_inspect_summary(wire: &DocumentWire) -> InspectSummary {
    use std::collections::HashMap;
    let node_map: HashMap<&str, &ode_format::wire::NodeWire> = wire.nodes.iter()
        .map(|n| (n.stable_id.as_str(), n))
        .collect();

    let tree = wire.canvas.iter()
        .filter_map(|id| node_map.get(id.as_str()).map(|n| build_tree_node(n, &node_map)))
        .collect();

    InspectSummary {
        name: wire.name.clone(),
        format_version: format!("{}.{}.{}", wire.format_version.0, wire.format_version.1, wire.format_version.2),
        working_color_space: serde_json::to_value(&wire.working_color_space)
            .ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
        node_count: wire.nodes.len(),
        canvas: wire.canvas.clone(),
        tree,
        tokens: TokensSummary {
            collections: wire.tokens.collections.iter().map(|c| c.name.clone()).collect(),
            total_tokens: wire.tokens.collections.iter().map(|c| c.tokens.len()).sum(),
        },
    }
}

fn build_tree_node(
    node: &ode_format::wire::NodeWire,
    node_map: &std::collections::HashMap<&str, &ode_format::wire::NodeWire>,
) -> InspectNode {
    use ode_format::wire::NodeKindWire;
    let (kind, size, child_ids) = match &node.kind {
        NodeKindWire::Frame(d) => ("frame", Some([d.width, d.height]),
            d.container.children.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
        NodeKindWire::Group(d) => ("group", None,
            d.children.iter().map(|s| s.as_str()).collect()),
        NodeKindWire::Vector(_) => ("vector", None, vec![]),
        NodeKindWire::BooleanOp(d) => ("boolean-op", None,
            d.children.iter().map(|s| s.as_str()).collect()),
        NodeKindWire::Text(_) => ("text", None, vec![]),
        NodeKindWire::Image(_) => ("image", None, vec![]),
        NodeKindWire::Instance(d) => ("instance", None,
            d.container.children.iter().map(|s| s.as_str()).collect()),
    };

    let children = child_ids.iter()
        .filter_map(|id| node_map.get(id).map(|n| build_tree_node(n, node_map)))
        .collect();

    InspectNode {
        stable_id: node.stable_id.clone(),
        name: node.name.clone(),
        kind: kind.to_string(),
        size,
        children,
    }
}

// ─── ode schema ───

pub fn cmd_schema(topic: Option<&str>) -> i32 {
    let schema = match topic {
        None | Some("document") => schemars::schema_for!(DocumentWire),
        Some("node") => schemars::schema_for!(ode_format::wire::NodeWire),
        Some("paint") => schemars::schema_for!(ode_format::style::Paint),
        Some("token") => schemars::schema_for!(ode_format::tokens::DesignTokens),
        Some("color") => schemars::schema_for!(ode_format::color::Color),
        Some(unknown) => {
            print_json(&ErrorResponse::new(
                "INVALID_TOPIC", "schema",
                &format!("unknown schema topic '{}'. Available: document, node, paint, token, color", unknown),
            ));
            return EXIT_INPUT;
        }
    };

    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    EXIT_OK
}
```

- [ ] **Step 2: Rewrite main.rs**

Rewrite `crates/ode-cli/src/main.rs`:

```rust
use clap::{Parser, Subcommand};

mod commands;
mod output;
mod validate;

#[derive(Parser)]
#[command(name = "ode", about = "Open Design Engine CLI — Agent-native design tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new empty .ode.json document
    New {
        /// Output file path
        file: String,
        /// Document name
        #[arg(long)]
        name: Option<String>,
        /// Root frame width (requires --height)
        #[arg(long, requires = "height")]
        width: Option<f32>,
        /// Root frame height (requires --width)
        #[arg(long, requires = "width")]
        height: Option<f32>,
    },
    /// Validate an .ode.json document
    Validate {
        /// Input file (or "-" for stdin)
        file: String,
    },
    /// Validate, render, and export in one step
    Build {
        /// Input file (or "-" for stdin)
        file: String,
        /// Output PNG path
        #[arg(short, long)]
        output: String,
    },
    /// Render without validation (fast path)
    Render {
        /// Input file (or "-" for stdin)
        file: String,
        /// Output PNG path
        #[arg(short, long)]
        output: String,
    },
    /// Inspect document structure
    Inspect {
        /// Input file (or "-" for stdin)
        file: String,
        /// Show full properties (not just tree summary)
        #[arg(long)]
        full: bool,
    },
    /// Output JSON Schema for the .ode.json format
    Schema {
        /// Schema topic: document, node, paint, token, color
        topic: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Command::New { file, name, width, height } => {
            commands::cmd_new(&file, name.as_deref(), width, height)
        }
        Command::Validate { file } => {
            commands::cmd_validate(&file)
        }
        Command::Build { file, output } => {
            commands::cmd_build(&file, &output)
        }
        Command::Render { file, output } => {
            commands::cmd_render(&file, &output)
        }
        Command::Inspect { file, full } => {
            commands::cmd_inspect(&file, full)
        }
        Command::Schema { topic } => {
            commands::cmd_schema(topic.as_deref())
        }
    };

    std::process::exit(exit_code);
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check --workspace`
Expected: Compiles with no errors

- [ ] **Step 4: Run all tests**

Run: `cargo test --workspace`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ode-cli/
git commit -m "feat(ode-cli): rewrite as agent-native CLI with 6 JSON-output commands"
```

---

## Chunk 3: CLI Integration Tests & Final Validation

### Task 9: Add CLI integration tests

**Files:**
- Create: `crates/ode-cli/tests/integration.rs`

- [ ] **Step 1: Create CLI integration tests**

Create `crates/ode-cli/tests/integration.rs`:

```rust
use std::process::Command;

fn ode_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ode"))
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("Failed to parse JSON: {}\nOutput: {}", e, stdout))
}

// ─── ode new ───

#[test]
fn new_creates_file() {
    let dir = std::env::temp_dir().join("ode_test_new");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("test.ode.json");
    let _ = std::fs::remove_file(&file);

    let output = ode_cmd()
        .args(["new", file.to_str().unwrap(), "--name", "Test Doc", "--width", "100", "--height", "50"])
        .output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert!(file.exists());

    // Verify the created file is valid
    let content: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&file).unwrap()
    ).unwrap();
    assert_eq!(content["name"], "Test Doc");
    assert_eq!(content["canvas"].as_array().unwrap().len(), 1);

    std::fs::remove_dir_all(&dir).ok();
}

// ─── ode validate ───

#[test]
fn validate_valid_document() {
    let dir = std::env::temp_dir().join("ode_test_validate");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("valid.ode.json");

    // Create a valid document first
    ode_cmd().args(["new", file.to_str().unwrap()]).output().unwrap();

    let output = ode_cmd()
        .args(["validate", file.to_str().unwrap()])
        .output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["valid"], true);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn validate_invalid_json() {
    let dir = std::env::temp_dir().join("ode_test_invalid");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("bad.ode.json");
    std::fs::write(&file, "not json").unwrap();

    let output = ode_cmd()
        .args(["validate", file.to_str().unwrap()])
        .output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let json = parse_json(&output);
    assert_eq!(json["valid"], false);
    assert!(json["errors"][0]["code"].as_str().unwrap() == "PARSE_FAILED");

    std::fs::remove_dir_all(&dir).ok();
}

// ─── ode build ───

#[test]
fn build_creates_png() {
    let dir = std::env::temp_dir().join("ode_test_build");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("design.ode.json");
    let png = dir.join("output.png");

    // Create a document with a colored frame
    ode_cmd().args(["new", file.to_str().unwrap(), "--width", "64", "--height", "64"]).output().unwrap();

    let output = ode_cmd()
        .args(["build", file.to_str().unwrap(), "-o", png.to_str().unwrap()])
        .output().unwrap();

    assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert!(png.exists());

    // Verify PNG magic bytes
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);

    std::fs::remove_dir_all(&dir).ok();
}

// ─── ode inspect ───

#[test]
fn inspect_shows_tree() {
    let dir = std::env::temp_dir().join("ode_test_inspect");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("doc.ode.json");

    ode_cmd().args(["new", file.to_str().unwrap(), "--name", "Inspect Me", "--width", "100", "--height", "50"]).output().unwrap();

    let output = ode_cmd()
        .args(["inspect", file.to_str().unwrap()])
        .output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["name"], "Inspect Me");
    assert_eq!(json["node_count"], 1);
    assert!(!json["tree"].as_array().unwrap().is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

// ─── ode schema ───

#[test]
fn schema_outputs_valid_json_schema() {
    let output = ode_cmd()
        .args(["schema"])
        .output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    // JSON Schema should have a title or $schema field
    assert!(json.get("title").is_some() || json.get("$schema").is_some() || json.get("type").is_some(),
        "Expected JSON Schema, got: {}", serde_json::to_string_pretty(&json).unwrap());
}

#[test]
fn schema_invalid_topic() {
    let output = ode_cmd()
        .args(["schema", "nonsense"])
        .output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let json = parse_json(&output);
    assert_eq!(json["status"], "error");
}

// ─── stdin support ───

#[test]
fn validate_stdin() {
    let json = r#"{"format_version":[0,2,0],"name":"Stdin","nodes":[],"canvas":[],"tokens":{"collections":[],"active_modes":{}},"views":[]}"#;

    let output = ode_cmd()
        .args(["validate", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(json.as_bytes()).unwrap();
            child.wait_with_output()
        })
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["valid"], true);
}

#[test]
fn inspect_stdin() {
    let json = r#"{"format_version":[0,2,0],"name":"Stdin Inspect","nodes":[{"stable_id":"r","name":"Root","kind":{"type":"frame","width":50,"height":50,"visual":{},"container":{},"component_def":null}}],"canvas":["r"],"tokens":{"collections":[],"active_modes":{}},"views":[]}"#;

    let output = ode_cmd()
        .args(["inspect", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(json.as_bytes()).unwrap();
            child.wait_with_output()
        })
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["name"], "Stdin Inspect");
    assert_eq!(result["node_count"], 1);
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p ode-cli --test integration`
Expected: ALL PASS (8 tests)

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test --workspace`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add crates/ode-cli/tests/
git commit -m "test(ode-cli): add integration tests for all 6 CLI commands"
```

---

### Task 10: Final workspace validation

- [ ] **Step 1: Full build check**

Run: `cargo check --workspace`
Expected: Compiles with 0 errors, minimal warnings

- [ ] **Step 2: Full test suite**

Run: `cargo test --workspace`
Expected: ALL PASS

- [ ] **Step 3: Fix any remaining warnings**

Fix the existing unused variable `s` in `crates/ode-core/src/paint.rs:230` and any new warnings.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: fix warnings, finalize agent-native CLI"
```
