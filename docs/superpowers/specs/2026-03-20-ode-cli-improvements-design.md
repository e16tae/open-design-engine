# ODE CLI Improvements Design Spec

**Date:** 2026-03-20
**Status:** Approved
**Scope:** 5 CLI improvements to unblock programmatic poster/design generation

## Context

An AIOBIO brand poster was created using the ODE CLI (`ode new` → `ode add` → `ode set` → `ode build`). The process revealed 5 engine-level gaps that limited the quality and workflow of programmatic design generation. These are tool limitations, not design judgment issues.

## Changes

### 1. Negative coordinate parsing fix

**Problem:** `ode set doc.ode nodeId --y -50` fails — clap interprets `-50` as an unknown flag, not a value for `--y`.

**Root cause:** The `Set` and `Add` subcommands lack clap's `allow_negative_numbers` attribute.

**Fix:** Add `#[command(allow_negative_numbers = true)]` to the `Set` and `Add` command variants in `crates/ode-cli/src/main.rs`.

**Files:** `crates/ode-cli/src/main.rs`
**Test:** `ode set` with `--x -100 --y -50` should succeed.

### 2. TextSizingMode CLI exposure

**Problem:** `mutate.rs` hardcodes `TextSizingMode::Fixed` when adding text. Users cannot set `AutoHeight` or `AutoWidth` via CLI, causing text to clip instead of wrapping.

**Fix:** Add `--text-sizing <MODE>` flag to both `ode add text` and `ode set` commands.

**Accepted values:** `fixed`, `auto-height`, `auto-width` (kebab-case, matching serde serialization format).

**Behavior:**
- `ode add text` defaults to `auto-height` (changed from `fixed` — most useful default for agents)
- `ode set` applies the mode to existing text nodes

**Files:** `crates/ode-cli/src/main.rs` (flag definition), `crates/ode-cli/src/mutate.rs` (apply logic)

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
- Stop positions (`%`) are optional; omitted → evenly distributed
- Angle conversion: CSS degrees → start/end point pairs (0deg = bottom-to-top, 90deg = left-to-right)

**Scope limitation:** Only `linear-gradient` and `radial-gradient` in this iteration. Angular, diamond, and mesh gradients are deferred.

**Files:** `crates/ode-cli/src/mutate.rs` (new `parse_fill()` function), `crates/ode-cli/src/main.rs` (no change needed — `--fill` remains `String`)

### 4. Font CLI commands

**Problem:** No way to discover available fonts, verify a font exists, or audit font usage in a document via CLI.

**Fix:** Add `ode fonts` subcommand with 3 actions.

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
```json
{
  "used": ["Inter", "Pretendard"],
  "available": ["Inter"],
  "missing": ["Pretendard"],
  "warnings": ["Pretendard: not found in system, will use fallback"]
}
```

**Files:** `crates/ode-cli/src/main.rs` (subcommand definition), `crates/ode-cli/src/commands.rs` (handler logic). FontDatabase already has all required methods.

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
