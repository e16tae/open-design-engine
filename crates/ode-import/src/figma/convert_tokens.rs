//! Figma Variables → ODE DesignTokens conversion.
//!
//! Converts Figma variable collections and variables into the ODE
//! `DesignTokens` format. Handles ID mapping (Figma string IDs → ODE u32 IDs),
//! alias resolution, and value parsing for COLOR, FLOAT, STRING, and BOOLEAN types.

use std::collections::HashMap;

use ode_format::style::{CollectionId, TokenId, TokenRef};
use ode_format::tokens::{
    DesignTokens, Mode, ModeId, Token, TokenCollection, TokenResolve, TokenValue,
};

use super::convert_style::convert_color;
use super::types::{FigmaColor, FigmaVariable, FigmaVariableCollection, FigmaVariablesMeta};

// ─── IdGenerator ─────────────────────────────────────────────────────────────

/// Maps Figma string IDs to auto-incrementing ODE u32 IDs.
pub struct IdGenerator {
    next_collection: u32,
    next_token: u32,
    next_mode: u32,
    collection_map: HashMap<String, CollectionId>,
    token_map: HashMap<String, TokenId>,
    mode_map: HashMap<String, ModeId>,
    /// Maps Figma variable ID → (CollectionId, TokenId) for alias resolution.
    variable_lookup: HashMap<String, (CollectionId, TokenId)>,
}

impl IdGenerator {
    fn new() -> Self {
        Self {
            next_collection: 0,
            next_token: 0,
            next_mode: 0,
            collection_map: HashMap::new(),
            token_map: HashMap::new(),
            mode_map: HashMap::new(),
            variable_lookup: HashMap::new(),
        }
    }

    fn collection_id(&mut self, figma_id: &str) -> CollectionId {
        if let Some(&id) = self.collection_map.get(figma_id) {
            return id;
        }
        let id = self.next_collection;
        self.next_collection += 1;
        self.collection_map.insert(figma_id.to_string(), id);
        id
    }

    fn token_id(&mut self, figma_id: &str) -> TokenId {
        if let Some(&id) = self.token_map.get(figma_id) {
            return id;
        }
        let id = self.next_token;
        self.next_token += 1;
        self.token_map.insert(figma_id.to_string(), id);
        id
    }

    fn mode_id(&mut self, figma_id: &str) -> ModeId {
        if let Some(&id) = self.mode_map.get(figma_id) {
            return id;
        }
        let id = self.next_mode;
        self.next_mode += 1;
        self.mode_map.insert(figma_id.to_string(), id);
        id
    }

    /// Register a variable's Figma ID mapped to its (CollectionId, TokenId).
    fn register_variable(
        &mut self,
        figma_var_id: &str,
        collection_id: CollectionId,
        token_id: TokenId,
    ) {
        self.variable_lookup
            .insert(figma_var_id.to_string(), (collection_id, token_id));
    }

    /// Look up an alias target by Figma variable ID.
    fn variable_map_lookup(&self, figma_var_id: &str) -> Option<&(CollectionId, TokenId)> {
        self.variable_lookup.get(figma_var_id)
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Convert all Figma variables and collections into ODE `DesignTokens`.
///
/// Returns `(DesignTokens, variable_map)` where `variable_map` maps each
/// Figma variable ID to its `(CollectionId, TokenId)` pair for use when
/// resolving variable bindings on nodes.
pub fn convert_all_variables(
    meta: &FigmaVariablesMeta,
) -> (DesignTokens, HashMap<String, (CollectionId, TokenId)>) {
    let mut id_gen = IdGenerator::new();

    // Build a deterministic ordering of collections by sorting on Figma ID.
    let mut collection_ids: Vec<&String> = meta.variable_collections.keys().collect();
    collection_ids.sort();

    // ── Phase 0: Convert collections (assigns collection & mode IDs) ──
    let mut collections: Vec<TokenCollection> = Vec::new();
    for coll_id in &collection_ids {
        let vc = &meta.variable_collections[*coll_id];
        collections.push(convert_collection(vc, &mut id_gen));
    }

    // ── Phase 1: Register all variable IDs (so alias lookups work) ────
    let mut variable_ids: Vec<&String> = meta.variables.keys().collect();
    variable_ids.sort();

    for var_id in &variable_ids {
        let var = &meta.variables[*var_id];
        let coll_ode_id = id_gen
            .collection_map
            .get(&var.variable_collection_id)
            .copied()
            .unwrap_or(0);
        let tok_ode_id = id_gen.token_id(&var.id);
        id_gen.register_variable(&var.id, coll_ode_id, tok_ode_id);
    }

    // ── Phase 2: Convert variable values ─────────────────────────────
    for var_id in &variable_ids {
        let var = &meta.variables[*var_id];
        let token = convert_variable(var, &id_gen);

        // Find the matching collection and push the token.
        let coll_ode_id = id_gen
            .collection_map
            .get(&var.variable_collection_id)
            .copied()
            .unwrap_or(0);
        if let Some(coll) = collections.iter_mut().find(|c| c.id == coll_ode_id) {
            coll.tokens.push(token);
        }
    }

    // Build active_modes: set each collection's default mode as the active mode.
    let mut active_modes = HashMap::new();
    for coll in &collections {
        active_modes.insert(coll.id, coll.default_mode);
    }

    let mut tokens = DesignTokens::new();
    tokens.collections = collections;
    tokens.active_modes = active_modes;

    // Build the external variable map from the generator's lookup.
    let variable_map = id_gen.variable_lookup.clone();

    (tokens, variable_map)
}

// ─── Collection ──────────────────────────────────────────────────────────────

/// Convert a Figma variable collection to an ODE `TokenCollection`.
fn convert_collection(vc: &FigmaVariableCollection, id_gen: &mut IdGenerator) -> TokenCollection {
    let coll_id = id_gen.collection_id(&vc.id);

    let modes: Vec<Mode> = vc
        .modes
        .iter()
        .map(|m| Mode {
            id: id_gen.mode_id(&m.mode_id),
            name: m.name.clone(),
        })
        .collect();

    let default_mode = id_gen
        .mode_map
        .get(&vc.default_mode_id)
        .copied()
        .unwrap_or(0);

    TokenCollection {
        id: coll_id,
        name: vc.name.clone(),
        modes,
        default_mode,
        tokens: Vec::new(),
    }
}

// ─── Variable ────────────────────────────────────────────────────────────────

/// Convert a Figma variable to an ODE `Token`.
///
/// The `id_gen` is used read-only here (Phase 2) to look up already-registered
/// mode IDs and alias targets.
fn convert_variable(var: &FigmaVariable, id_gen: &IdGenerator) -> Token {
    let tok_id = id_gen.token_map.get(&var.id).copied().unwrap_or(0);

    // Parse values for each mode.
    let mut values: HashMap<ModeId, TokenResolve> = HashMap::new();
    for (mode_figma_id, value) in &var.values_by_mode {
        let mode_ode_id = id_gen.mode_map.get(mode_figma_id).copied().unwrap_or(0);
        let resolve = parse_variable_value(&var.resolved_type, value, id_gen);
        values.insert(mode_ode_id, resolve);
    }

    // Extract group from name: "colors/primary/500" → group = "colors/primary"
    let group = extract_group(&var.name);

    // Extract the leaf name: "colors/primary/500" → "500"
    let name = var.name.rsplit('/').next().unwrap_or(&var.name).to_string();

    Token {
        id: tok_id,
        name,
        group,
        values,
    }
}

/// Extract the group from a slash-separated variable name.
///
/// `"colors/primary/500"` → `Some("colors/primary")`
/// `"spacing"` → `None`
fn extract_group(name: &str) -> Option<String> {
    let last_slash = name.rfind('/')?;
    if last_slash == 0 {
        return None;
    }
    Some(name[..last_slash].to_string())
}

// ─── Value Parsing ───────────────────────────────────────────────────────────

/// Parse a single variable value from its JSON representation.
///
/// Checks for alias objects first (`{"type": "VARIABLE_ALIAS", "id": "..."}`),
/// then parses based on the variable's `resolved_type`.
fn parse_variable_value(
    resolved_type: &str,
    value: &serde_json::Value,
    id_gen: &IdGenerator,
) -> TokenResolve {
    // Check if it's an alias reference.
    if let Some(alias_type) = value.get("type").and_then(|t| t.as_str()) {
        if alias_type == "VARIABLE_ALIAS" {
            if let Some(id) = value.get("id").and_then(|i| i.as_str()) {
                if let Some(&(coll_id, tok_id)) = id_gen.variable_map_lookup(id) {
                    return TokenResolve::Alias(TokenRef {
                        collection_id: coll_id,
                        token_id: tok_id,
                    });
                }
            }
            // Alias target not found — fall through to produce a default value.
        }
    }

    // Parse as a direct value based on resolved_type.
    match resolved_type {
        "COLOR" => {
            // Expect {"r": f, "g": f, "b": f, "a": f}
            let fc: FigmaColor = serde_json::from_value(value.clone()).unwrap_or(FigmaColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            });
            TokenResolve::Direct(TokenValue::Color(convert_color(&fc)))
        }
        "FLOAT" => {
            let n = value.as_f64().unwrap_or(0.0) as f32;
            TokenResolve::Direct(TokenValue::Number(n))
        }
        "STRING" => {
            let s = value.as_str().unwrap_or("").to_string();
            TokenResolve::Direct(TokenValue::String(s))
        }
        "BOOLEAN" => {
            let b = value.as_bool().unwrap_or(false);
            TokenResolve::Direct(TokenValue::Number(if b { 1.0 } else { 0.0 }))
        }
        _ => TokenResolve::Direct(TokenValue::Number(0.0)),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figma::types::{FigmaVariableCollection, FigmaVariableMode};
    use ode_format::color::Color;

    /// Helper: build a minimal FigmaVariablesMeta.
    fn make_meta(
        collections: Vec<(&str, &str, Vec<(&str, &str)>, Vec<&str>)>,
        variables: Vec<(&str, &str, &str, &str, Vec<(&str, serde_json::Value)>)>,
    ) -> FigmaVariablesMeta {
        let mut vc_map = HashMap::new();
        for (id, name, modes, var_ids) in &collections {
            vc_map.insert(
                id.to_string(),
                FigmaVariableCollection {
                    id: id.to_string(),
                    name: name.to_string(),
                    modes: modes
                        .iter()
                        .map(|(mid, mname)| FigmaVariableMode {
                            mode_id: mid.to_string(),
                            name: mname.to_string(),
                        })
                        .collect(),
                    default_mode_id: modes
                        .first()
                        .map(|(mid, _)| mid.to_string())
                        .unwrap_or_default(),
                    variable_ids: var_ids.iter().map(|s| s.to_string()).collect(),
                    remote: false,
                    hidden_from_publishing: false,
                },
            );
        }

        let mut var_map = HashMap::new();
        for (id, name, coll_id, resolved_type, values) in &variables {
            let mut values_by_mode = HashMap::new();
            for (mode_id, val) in values {
                values_by_mode.insert(mode_id.to_string(), val.clone());
            }
            var_map.insert(
                id.to_string(),
                FigmaVariable {
                    id: id.to_string(),
                    name: name.to_string(),
                    variable_collection_id: coll_id.to_string(),
                    resolved_type: resolved_type.to_string(),
                    values_by_mode,
                    description: String::new(),
                    hidden_from_publishing: false,
                    scopes: Vec::new(),
                    code_syntax: None,
                },
            );
        }

        FigmaVariablesMeta {
            variable_collections: vc_map,
            variables: var_map,
        }
    }

    #[test]
    fn convert_single_collection_with_modes() {
        let meta = make_meta(
            vec![(
                "VC:1",
                "Colors",
                vec![("1:0", "Light"), ("1:1", "Dark")],
                vec!["V:1"],
            )],
            vec![(
                "V:1",
                "primary",
                "VC:1",
                "COLOR",
                vec![
                    (
                        "1:0",
                        serde_json::json!({"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0}),
                    ),
                    (
                        "1:1",
                        serde_json::json!({"r": 0.0, "g": 0.0, "b": 1.0, "a": 1.0}),
                    ),
                ],
            )],
        );

        let (tokens, _) = convert_all_variables(&meta);
        assert_eq!(tokens.collections.len(), 1);

        let coll = &tokens.collections[0];
        assert_eq!(coll.name, "Colors");
        assert_eq!(coll.modes.len(), 2);
        assert_eq!(coll.modes[0].name, "Light");
        assert_eq!(coll.modes[1].name, "Dark");
        assert_eq!(coll.default_mode, coll.modes[0].id);
        assert_eq!(coll.tokens.len(), 1);
    }

    #[test]
    fn convert_color_variable() {
        let meta = make_meta(
            vec![("VC:1", "Colors", vec![("1:0", "Default")], vec!["V:1"])],
            vec![(
                "V:1",
                "red",
                "VC:1",
                "COLOR",
                vec![(
                    "1:0",
                    serde_json::json!({"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0}),
                )],
            )],
        );

        let (tokens, _) = convert_all_variables(&meta);
        let token = &tokens.collections[0].tokens[0];
        let mode_id = tokens.collections[0].modes[0].id;
        let resolve = &token.values[&mode_id];
        assert_eq!(
            *resolve,
            TokenResolve::Direct(TokenValue::Color(Color::Srgb {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }))
        );
    }

    #[test]
    fn convert_float_variable() {
        let meta = make_meta(
            vec![("VC:1", "Spacing", vec![("1:0", "Default")], vec!["V:1"])],
            vec![(
                "V:1",
                "spacing-md",
                "VC:1",
                "FLOAT",
                vec![("1:0", serde_json::json!(16.0))],
            )],
        );

        let (tokens, _) = convert_all_variables(&meta);
        let token = &tokens.collections[0].tokens[0];
        let mode_id = tokens.collections[0].modes[0].id;
        let resolve = &token.values[&mode_id];
        assert_eq!(*resolve, TokenResolve::Direct(TokenValue::Number(16.0)));
    }

    #[test]
    fn convert_boolean_variable_to_number() {
        let meta = make_meta(
            vec![("VC:1", "Flags", vec![("1:0", "Default")], vec!["V:1"])],
            vec![(
                "V:1",
                "is-dark",
                "VC:1",
                "BOOLEAN",
                vec![("1:0", serde_json::json!(true))],
            )],
        );

        let (tokens, _) = convert_all_variables(&meta);
        let token = &tokens.collections[0].tokens[0];
        let mode_id = tokens.collections[0].modes[0].id;
        let resolve = &token.values[&mode_id];
        assert_eq!(*resolve, TokenResolve::Direct(TokenValue::Number(1.0)));
    }

    #[test]
    fn convert_string_variable() {
        let meta = make_meta(
            vec![("VC:1", "Strings", vec![("1:0", "Default")], vec!["V:1"])],
            vec![(
                "V:1",
                "greeting",
                "VC:1",
                "STRING",
                vec![("1:0", serde_json::json!("hello"))],
            )],
        );

        let (tokens, _) = convert_all_variables(&meta);
        let token = &tokens.collections[0].tokens[0];
        let mode_id = tokens.collections[0].modes[0].id;
        let resolve = &token.values[&mode_id];
        assert_eq!(
            *resolve,
            TokenResolve::Direct(TokenValue::String("hello".to_string()))
        );
    }

    #[test]
    fn convert_variable_alias() {
        let meta = make_meta(
            vec![(
                "VC:1",
                "Colors",
                vec![("1:0", "Default")],
                vec!["V:1", "V:2"],
            )],
            vec![
                (
                    "V:1",
                    "red",
                    "VC:1",
                    "COLOR",
                    vec![(
                        "1:0",
                        serde_json::json!({"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0}),
                    )],
                ),
                (
                    "V:2",
                    "primary",
                    "VC:1",
                    "COLOR",
                    vec![(
                        "1:0",
                        serde_json::json!({"type": "VARIABLE_ALIAS", "id": "V:1"}),
                    )],
                ),
            ],
        );

        let (tokens, _) = convert_all_variables(&meta);
        let coll = &tokens.collections[0];

        // Find the alias token (name "primary")
        let alias_token = coll.tokens.iter().find(|t| t.name == "primary").unwrap();
        let mode_id = coll.modes[0].id;
        let resolve = &alias_token.values[&mode_id];

        // Find the target token (name "red")
        let target_token = coll.tokens.iter().find(|t| t.name == "red").unwrap();

        assert_eq!(
            *resolve,
            TokenResolve::Alias(TokenRef {
                collection_id: coll.id,
                token_id: target_token.id,
            })
        );
    }

    #[test]
    fn variable_group_from_name() {
        assert_eq!(
            extract_group("colors/primary/500"),
            Some("colors/primary".to_string())
        );
        assert_eq!(extract_group("spacing"), None);
        assert_eq!(extract_group("a/b"), Some("a".to_string()));
    }

    #[test]
    fn convert_all_returns_variable_map() {
        let meta = make_meta(
            vec![(
                "VC:1",
                "Colors",
                vec![("1:0", "Default")],
                vec!["V:1", "V:2"],
            )],
            vec![
                (
                    "V:1",
                    "red",
                    "VC:1",
                    "COLOR",
                    vec![(
                        "1:0",
                        serde_json::json!({"r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0}),
                    )],
                ),
                (
                    "V:2",
                    "blue",
                    "VC:1",
                    "COLOR",
                    vec![(
                        "1:0",
                        serde_json::json!({"r": 0.0, "g": 0.0, "b": 1.0, "a": 1.0}),
                    )],
                ),
            ],
        );

        let (tokens, variable_map) = convert_all_variables(&meta);
        assert_eq!(variable_map.len(), 2);
        assert!(variable_map.contains_key("V:1"));
        assert!(variable_map.contains_key("V:2"));

        // Verify the map points to the correct IDs.
        let (coll_id, tok_id) = variable_map["V:1"];
        assert_eq!(coll_id, tokens.collections[0].id);
        let target = tokens.collections[0]
            .tokens
            .iter()
            .find(|t| t.id == tok_id)
            .unwrap();
        assert_eq!(target.name, "red");
    }

    #[test]
    fn boolean_false_converts_to_zero() {
        let meta = make_meta(
            vec![("VC:1", "Flags", vec![("1:0", "Default")], vec!["V:1"])],
            vec![(
                "V:1",
                "disabled",
                "VC:1",
                "BOOLEAN",
                vec![("1:0", serde_json::json!(false))],
            )],
        );

        let (tokens, _) = convert_all_variables(&meta);
        let token = &tokens.collections[0].tokens[0];
        let mode_id = tokens.collections[0].modes[0].id;
        let resolve = &token.values[&mode_id];
        assert_eq!(*resolve, TokenResolve::Direct(TokenValue::Number(0.0)));
    }

    #[test]
    fn multi_mode_color_variable() {
        let meta = make_meta(
            vec![(
                "VC:1",
                "Theme",
                vec![("1:0", "Light"), ("1:1", "Dark")],
                vec!["V:1"],
            )],
            vec![(
                "V:1",
                "background",
                "VC:1",
                "COLOR",
                vec![
                    (
                        "1:0",
                        serde_json::json!({"r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0}),
                    ),
                    (
                        "1:1",
                        serde_json::json!({"r": 0.0, "g": 0.0, "b": 0.0, "a": 1.0}),
                    ),
                ],
            )],
        );

        let (tokens, _) = convert_all_variables(&meta);
        let coll = &tokens.collections[0];
        let token = &coll.tokens[0];
        assert_eq!(token.values.len(), 2);

        let light_mode_id = coll.modes[0].id;
        let dark_mode_id = coll.modes[1].id;

        assert_eq!(
            token.values[&light_mode_id],
            TokenResolve::Direct(TokenValue::Color(Color::Srgb {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            }))
        );
        assert_eq!(
            token.values[&dark_mode_id],
            TokenResolve::Direct(TokenValue::Color(Color::Srgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }))
        );
    }
}
