//! Figma REST API type definitions.
//!
//! All structs mirror the Figma REST API JSON response format.
//! Fields use `Option<T>` to handle incomplete API responses gracefully.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Top-Level Response ──────────────────────────────────────────────────────

/// Response from `GET /v1/files/:key`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaFileResponse {
    pub name: String,
    pub document: FigmaNode,
    pub components: HashMap<String, FigmaComponentMeta>,
    pub component_sets: HashMap<String, FigmaComponentSetMeta>,
    pub schema_version: u32,
    pub styles: HashMap<String, FigmaStyleMeta>,
}

/// Metadata for a single component.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaComponentMeta {
    pub key: String,
    pub name: String,
    pub description: String,
    pub component_set_id: Option<String>,
    pub documentation_links: Option<Vec<FigmaDocLink>>,
}

/// A documentation link attached to a component.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaDocLink {
    pub uri: String,
}

/// Metadata for a component set (group of variants).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaComponentSetMeta {
    pub key: String,
    pub name: String,
    pub description: String,
}

/// Metadata for a named style (fill, text, effect, grid).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaStyleMeta {
    pub key: String,
    pub name: String,
    pub style_type: String,
    pub description: String,
}

// ─── Node ────────────────────────────────────────────────────────────────────

/// A single node in the Figma document tree.
///
/// The `node_type` field is deserialized from the JSON key `"type"`.
/// All optional fields default to `None` so that any node type can be
/// represented with the same struct.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FigmaNode {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub node_type: String,
    pub visible: Option<bool>,
    pub children: Option<Vec<FigmaNode>>,

    // Visual properties
    pub fills: Option<Vec<FigmaPaint>>,
    pub strokes: Option<Vec<FigmaPaint>>,
    pub stroke_weight: Option<f32>,
    pub stroke_align: Option<String>,
    pub stroke_cap: Option<String>,
    pub stroke_join: Option<String>,
    pub stroke_dashes: Option<Vec<f32>>,
    pub stroke_miter_angle: Option<f32>,
    pub effects: Option<Vec<FigmaEffect>>,
    pub opacity: Option<f32>,
    pub blend_mode: Option<String>,
    pub is_mask: Option<bool>,
    pub corner_radius: Option<f32>,
    pub rectangle_corner_radii: Option<[f32; 4]>,

    // Geometry
    pub absolute_bounding_box: Option<FigmaRect>,
    pub relative_transform: Option<[[f64; 3]; 2]>,
    pub size: Option<FigmaVector>,
    pub clips_content: Option<bool>,

    // Layout constraints
    pub constraints: Option<FigmaLayoutConstraint>,

    // Auto Layout
    pub layout_mode: Option<String>,
    pub layout_sizing_horizontal: Option<String>,
    pub layout_sizing_vertical: Option<String>,
    pub layout_wrap: Option<String>,
    pub primary_axis_align_items: Option<String>,
    pub counter_axis_align_items: Option<String>,
    pub padding_left: Option<f32>,
    pub padding_right: Option<f32>,
    pub padding_top: Option<f32>,
    pub padding_bottom: Option<f32>,
    pub item_spacing: Option<f32>,
    pub counter_axis_spacing: Option<f32>,
    pub layout_align: Option<String>,
    pub layout_positioning: Option<String>,
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,

    // Text
    pub characters: Option<String>,
    pub style: Option<FigmaTypeStyle>,
    pub character_style_overrides: Option<Vec<usize>>,
    pub style_override_table: Option<HashMap<String, FigmaTypeStyle>>,

    // Component / Instance
    pub component_id: Option<String>,
    pub component_properties: Option<HashMap<String, FigmaComponentProperty>>,
    pub overrides: Option<Vec<FigmaOverride>>,

    // Boolean operation
    pub boolean_operation: Option<String>,

    // Path data
    pub fill_geometry: Option<Vec<FigmaPath>>,
    pub stroke_geometry: Option<Vec<FigmaPath>>,

    // Variable bindings
    pub bound_variables: Option<HashMap<String, FigmaVariableAlias>>,
}

// ─── Override / Component ────────────────────────────────────────────────────

/// An override entry for a component instance.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaOverride {
    pub id: String,
    pub overridden_fields: Vec<String>,
}

/// A component property value. Tagged by `"type"` in JSON.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FigmaComponentProperty {
    Text { value: String },
    Boolean { value: bool },
    InstanceSwap { value: String },
    Variant { value: String },
}

// ─── Paint ───────────────────────────────────────────────────────────────────

/// A fill or stroke paint.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FigmaPaint {
    #[serde(rename = "type", default)]
    pub paint_type: String,
    pub visible: Option<bool>,
    pub opacity: Option<f32>,
    pub color: Option<FigmaColor>,
    pub blend_mode: Option<String>,
    pub gradient_handle_positions: Option<Vec<FigmaVector>>,
    pub gradient_stops: Option<Vec<FigmaColorStop>>,
    pub scale_mode: Option<String>,
    pub image_ref: Option<String>,
    pub image_transform: Option<[[f64; 3]; 2]>,
    pub bound_variables: Option<HashMap<String, FigmaVariableAlias>>,
}

/// An RGBA color with values in the range [0, 1].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FigmaColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// A gradient color stop.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaColorStop {
    pub position: f32,
    pub color: FigmaColor,
    pub bound_variables: Option<HashMap<String, FigmaVariableAlias>>,
}

// ─── Effect ──────────────────────────────────────────────────────────────────

/// A visual effect (shadow, blur).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FigmaEffect {
    #[serde(rename = "type", default)]
    pub effect_type: String,
    pub visible: Option<bool>,
    pub radius: Option<f32>,
    pub color: Option<FigmaColor>,
    pub offset: Option<FigmaVector>,
    pub spread: Option<f32>,
    pub blend_mode: Option<String>,
    pub bound_variables: Option<HashMap<String, FigmaVariableAlias>>,
}

// ─── Text Style ──────────────────────────────────────────────────────────────

/// Text style properties.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FigmaTypeStyle {
    pub font_family: Option<String>,
    pub font_weight: Option<f32>,
    pub font_size: Option<f32>,
    pub text_align_horizontal: Option<String>,
    pub text_align_vertical: Option<String>,
    pub letter_spacing: Option<f32>,
    pub line_height_px: Option<f32>,
    pub line_height_percent_font_size: Option<f32>,
    pub line_height_unit: Option<String>,
    pub text_decoration: Option<String>,
    pub text_case: Option<String>,
    pub text_auto_resize: Option<String>,
    pub paragraph_spacing: Option<f32>,
    pub paragraph_indent: Option<f32>,
    pub fills: Option<Vec<FigmaPaint>>,
    pub opentype_flags: Option<HashMap<String, u32>>,
    pub italic: Option<bool>,
    pub bound_variables: Option<HashMap<String, FigmaVariableAlias>>,
}

// ─── Variables ───────────────────────────────────────────────────────────────

/// Response from `GET /v1/files/:key/variables/local`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaVariablesResponse {
    pub status: u32,
    pub error: Option<bool>,
    pub meta: FigmaVariablesMeta,
}

/// Container for variable collections and variables.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaVariablesMeta {
    pub variable_collections: HashMap<String, FigmaVariableCollection>,
    pub variables: HashMap<String, FigmaVariable>,
}

/// A variable collection (e.g. "Colors", "Spacing").
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaVariableCollection {
    pub id: String,
    pub name: String,
    pub modes: Vec<FigmaVariableMode>,
    pub default_mode_id: String,
    pub variable_ids: Vec<String>,
    pub remote: bool,
    pub hidden_from_publishing: bool,
}

/// A mode within a variable collection.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaVariableMode {
    pub mode_id: String,
    pub name: String,
}

/// A single design variable.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaVariable {
    pub id: String,
    pub name: String,
    pub variable_collection_id: String,
    pub resolved_type: String,
    pub values_by_mode: HashMap<String, serde_json::Value>,
    pub description: String,
    pub hidden_from_publishing: bool,
    pub scopes: Vec<String>,
    pub code_syntax: Option<HashMap<String, String>>,
}

/// A reference to a variable (alias).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaVariableAlias {
    #[serde(rename = "type")]
    pub alias_type: String,
    pub id: String,
}

// ─── Geometry ────────────────────────────────────────────────────────────────

/// An axis-aligned bounding rectangle.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FigmaRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A 2D vector / point.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FigmaVector {
    pub x: f64,
    pub y: f64,
}

/// An SVG path with winding rule.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaPath {
    pub path: String,
    pub winding_rule: Option<String>,
    pub overridden_fields: Option<Vec<String>>,
}

/// Layout constraints for a node within its parent frame.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FigmaLayoutConstraint {
    pub vertical: String,
    pub horizontal: String,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_minimal_file_response() {
        let json = r#"{
            "name": "Test File",
            "document": {
                "id": "0:0",
                "name": "Document",
                "type": "DOCUMENT",
                "children": []
            },
            "components": {},
            "componentSets": {},
            "schemaVersion": 0,
            "styles": {}
        }"#;
        let response: FigmaFileResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.name, "Test File");
        assert_eq!(response.document.node_type, "DOCUMENT");
    }

    #[test]
    fn deserialize_frame_node_with_fills() {
        let json = r#"{
            "id": "1:1",
            "name": "Frame",
            "type": "FRAME",
            "fills": [
                {"type": "SOLID", "color": {"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0}}
            ],
            "absoluteBoundingBox": {"x": 0, "y": 0, "width": 100, "height": 100},
            "size": {"x": 100, "y": 100},
            "blendMode": "NORMAL",
            "children": []
        }"#;
        let node: FigmaNode = serde_json::from_str(json).unwrap();
        assert_eq!(node.fills.unwrap().len(), 1);
    }

    #[test]
    fn deserialize_text_node_with_style() {
        let json = r#"{
            "id": "2:1",
            "name": "Title",
            "type": "TEXT",
            "characters": "Hello World",
            "style": {
                "fontFamily": "Inter",
                "fontWeight": 400,
                "fontSize": 16,
                "textAlignHorizontal": "LEFT",
                "textAlignVertical": "TOP",
                "letterSpacing": 0,
                "lineHeightPx": 24,
                "lineHeightUnit": "PIXELS"
            },
            "characterStyleOverrides": [0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1],
            "styleOverrideTable": {
                "1": {"fontWeight": 700}
            }
        }"#;
        let node: FigmaNode = serde_json::from_str(json).unwrap();
        assert_eq!(node.characters.unwrap(), "Hello World");
        assert_eq!(node.character_style_overrides.unwrap().len(), 11);
    }

    #[test]
    fn deserialize_gradient_paint() {
        let json = r#"{
            "type": "GRADIENT_LINEAR",
            "gradientHandlePositions": [
                {"x": 0, "y": 0.5},
                {"x": 1, "y": 0.5},
                {"x": 0, "y": 1}
            ],
            "gradientStops": [
                {"position": 0, "color": {"r": 1, "g": 0, "b": 0, "a": 1}},
                {"position": 1, "color": {"r": 0, "g": 0, "b": 1, "a": 1}}
            ]
        }"#;
        let paint: FigmaPaint = serde_json::from_str(json).unwrap();
        assert_eq!(paint.paint_type, "GRADIENT_LINEAR");
        assert_eq!(paint.gradient_stops.unwrap().len(), 2);
    }

    #[test]
    fn deserialize_effect() {
        let json = r#"{
            "type": "DROP_SHADOW",
            "visible": true,
            "radius": 4.0,
            "color": {"r": 0, "g": 0, "b": 0, "a": 0.25},
            "offset": {"x": 0, "y": 4},
            "spread": 0
        }"#;
        let effect: FigmaEffect = serde_json::from_str(json).unwrap();
        assert_eq!(effect.effect_type, "DROP_SHADOW");
        assert_eq!(effect.radius, Some(4.0));
    }

    #[test]
    fn deserialize_variables_response() {
        let json = r#"{
            "status": 200,
            "meta": {
                "variableCollections": {
                    "VC:1": {
                        "id": "VC:1",
                        "name": "Colors",
                        "modes": [{"modeId": "1:0", "name": "Light"}],
                        "defaultModeId": "1:0",
                        "variableIds": ["V:1"],
                        "remote": false,
                        "hiddenFromPublishing": false
                    }
                },
                "variables": {
                    "V:1": {
                        "id": "V:1",
                        "name": "primary",
                        "variableCollectionId": "VC:1",
                        "resolvedType": "COLOR",
                        "valuesByMode": {
                            "1:0": {"r": 1, "g": 0, "b": 0, "a": 1}
                        },
                        "description": "",
                        "hiddenFromPublishing": false,
                        "scopes": []
                    }
                }
            }
        }"#;
        let resp: FigmaVariablesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.meta.variable_collections.len(), 1);
        assert_eq!(resp.meta.variables.len(), 1);
    }
}
