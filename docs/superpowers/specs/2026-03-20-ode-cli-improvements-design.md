# ODE CLI Improvements Design Spec

**Date:** 2026-03-20
**Status:** Approved
**Scope:** 4 CLI improvements to unblock programmatic poster/design generation

## Context

An AIOBIO brand poster was created using the ODE CLI (`ode new` → `ode add` → `ode set` → `ode build`). The process revealed 4 engine-level gaps that limited the quality and workflow of programmatic design generation. These are tool limitations, not design judgment issues.

## Changes

### 1. Negative coordinate parsing fix

**Problem:** `ode set doc.ode nodeId --y -50` fails — clap interprets `-50` as an unknown flag, not a value for `--y`.

**Root cause:** The `Set` subcommand lacks clap's `allow_negative_numbers` attribute.

**Fix:** Add `#[command(allow_negative_numbers = true)]` to the `Set` command variant in `crates/ode-cli/src/main.rs`. The `Add` command does not have `--x`/`--y` flags and does not need this change.

**Files:** `crates/ode-cli/src/main.rs`

**Test:** `ode set doc.ode nodeId --x -100 --y -50` should succeed and produce `{"status":"ok","modified":["x","y"]}`.

### 2. TextSizingMode CLI exposure

**Problem:** `mutate.rs` hardcodes `TextSizingMode::Fixed` when adding text. Users cannot set `AutoHeight` or `AutoWidth` via CLI, causing text to clip instead of wrapping.

**Fix:** Add `--text-sizing <MODE>` flag to both `ode add text` and `ode set` commands.

**Accepted values:** `fixed`, `auto-height`, `auto-width`.

**Implementation detail:** Define a `TextSizingArg` enum with `#[derive(clap::ValueEnum)]` using kebab-case variants, then convert to `TextSizingMode` in the handler. Do not rely on serde for CLI parsing — clap's `ValueEnum` is the correct mechanism.

**Behavior:**
- `ode add text` defaults to `auto-height` (changed from `fixed` — most useful default for agents)
- `ode set` applies the mode to existing text nodes; errors if node is not a text node

**Files:** `crates/ode-cli/src/main.rs` (flag + enum definition), `crates/ode-cli/src/mutate.rs` (apply logic)

**Test:**
- `ode add text doc.ode --content "Hello" --text-sizing auto-height` → node has `sizing_mode: "auto-height"` in document JSON
- `ode set doc.ode textNodeId --text-sizing auto-width` → changes existing node's sizing mode
- `ode inspect doc.ode` confirms the sizing mode is persisted

### 3. Gradient CLI support (CSS-like syntax)

**Problem:** `--fill` only accepts hex colors (`#RRGGBB`). Gradients exist in the data model (`Paint::LinearGradient`, `Paint::RadialGradient`, etc.) but have no CLI path.

**Fix:** Extend the `--fill` parser to accept CSS-like gradient syntax alongside hex colors.

**Supported syntax:**
```
# Solid (existing, unchanged)
--fill "#16C1F3"

# Linear gradient
--fill "linear-gradient(0deg, #16C1F3, #0A1628)"
--fill "linear-gradient(90deg, #16C1F3 0%, #0AA1CD 50%, #0A1628 100%)"

# Radial gradient
--fill "radial-gradient(#16C1F3, #0A1628)"
```

**Parsing rules:**
- Starts with `#` → `Paint::Solid` (existing path)
- Starts with `linear-gradient(` → parse angle + color stops → `Paint::LinearGradient`
- Starts with `radial-gradient(` → parse color stops → `Paint::RadialGradient`
- Stop positions (`%`) are optional; omitted → evenly distributed across 0.0..1.0
- Minimum 2 color stops required; fewer → parse error

**Angle-to-coordinate conversion (normalized 0..1 bounding box):**

| CSS Angle | Direction | start | end |
|-----------|-----------|-------|-----|
| `0deg` | bottom → top | `{x: 0.5, y: 1.0}` | `{x: 0.5, y: 0.0}` |
| `90deg` | left → right | `{x: 0.0, y: 0.5}` | `{x: 1.0, y: 0.5}` |
| `180deg` | top → bottom | `{x: 0.5, y: 0.0}` | `{x: 0.5, y: 1.0}` |
| `270deg` | right → left | `{x: 1.0, y: 0.5}` | `{x: 0.0, y: 0.5}` |

General formula:
```
start.x = 0.5 - 0.5 * sin(angle_rad)
start.y = 0.5 + 0.5 * cos(angle_rad)
end.x   = 0.5 + 0.5 * sin(angle_rad)
end.y   = 0.5 - 0.5 * cos(angle_rad)
```

**Radial gradient defaults:**
- `center: {x: 0.5, y: 0.5}` (center of bounding box)
- `radius: {x: 0.5, y: 0.5}` (extends to edges)
- Optional center/radius parameters are deferred to a future iteration.

**Fill transition behavior:** `--fill` always replaces `fills[0]` regardless of whether the existing fill is solid or gradient, and regardless of whether the new fill is solid or gradient. This matches current behavior.

**Error handling:** Malformed gradient strings produce a CLI error with message format: `invalid fill value: <description>`. Examples:
- `"linear-gradient(abc, #123)"` → `invalid fill value: expected angle like '90deg', got 'abc'`
- `"linear-gradient(90deg)"` → `invalid fill value: at least 2 color stops required`
- `"linear-gradient(90deg, xyz)"` → `invalid fill value: invalid color 'xyz'`

**Scope limitation:** Only `linear-gradient` and `radial-gradient` in this iteration. Angular, diamond, and mesh gradients are deferred.

**Files:** `crates/ode-cli/src/mutate.rs` (new `parse_fill()` function), `crates/ode-cli/src/main.rs` (no change — `--fill` remains `String`)

**Test:**
- `ode add frame doc.ode --fill "linear-gradient(90deg, #FF0000, #0000FF)"` → frame has `LinearGradient` paint with 2 stops
- `ode set doc.ode nodeId --fill "radial-gradient(#16C1F3, #0A1628)"` → replaces fill with `RadialGradient`
- `ode set doc.ode nodeId --fill "#FF0000"` on a gradient-filled node → replaces with solid fill
- Invalid input → exits with non-zero code and descriptive error message

### 4. Font CLI commands

**Problem:** No way to discover available fonts, verify a font exists, or audit font usage in a document via CLI.

**Fix:** Add `ode fonts` subcommand with 3 actions.

**New FontDatabase methods required:** The current `FontDatabase` API does not expose family listing or per-family weight enumeration. The following public methods must be added to `crates/ode-text/src/font_db.rs`:
- `pub fn families(&self) -> Vec<String>` — returns sorted, deduplicated list of family names from `family_index` keys
- `pub fn weights_for_family(&self, family: &str) -> Vec<u16>` — returns sorted list of available weights for a family, empty if family not found

#### `ode fonts list`
Lists all available system font families as JSON array.
```json
["AppleSDGothicNeo", "Arial", "Helvetica", "Inter", "Pretendard"]
```
Sorted alphabetically, deduplicated by family name.

#### `ode fonts check <FAMILY>`
Checks if a specific font family is available and reports weights.
```json
{"family": "Inter", "available": true, "weights": [400, 500, 600, 700]}
```
If not found:
```json
{"family": "Kaleko", "available": false, "weights": []}
```

#### `ode fonts audit <FILE>`
Scans all text nodes in a document, collects used font families, cross-references with FontDatabase.

**Traversal:** Collects `font_family` from both `default_style` and each entry in `runs` (per-run style overrides), since fonts can vary within a single text node.

```json
{
  "used": ["Inter", "Pretendard"],
  "available": ["Inter"],
  "missing": ["Pretendard"],
  "warnings": ["Pretendard: not found in system, will use fallback"]
}
```

**Files:** `crates/ode-text/src/font_db.rs` (new public methods), `crates/ode-cli/src/main.rs` (subcommand definition), `crates/ode-cli/src/commands.rs` (handler logic)

**Test:**
- `ode fonts list` → returns non-empty JSON array of strings
- `ode fonts check "Arial"` → `{"available": true, ...}` with at least one weight
- `ode fonts check "NonexistentFont123"` → `{"available": false, "weights": []}`
- `ode fonts audit doc.ode` on a document with text → returns JSON with `used`, `available`, `missing` fields

## Implementation Order

1. **Negative coordinate fix** — smallest change, unblocks basic workflows
2. **TextSizingMode CLI** — enables proper text wrapping, high impact
3. **Font CLI commands** — independent of other changes, provides diagnostic capability
4. **Gradient CLI** — most complex parser addition, benefits from prior changes being stable

## Out of Scope

- Angular, diamond, mesh gradient CLI syntax
- Font embedding/bundling in .ode files
- Text run-level styling via CLI (already possible via JSON editing)
- Stroke gradient support (fills only)
- Optional center/radius parameters for radial gradients
