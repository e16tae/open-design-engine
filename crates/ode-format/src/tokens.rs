use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use thiserror::Error;
use crate::color::Color;
use crate::style::{CollectionId, TokenId, TokenRef};

// ─── ModeId ───
pub type ModeId = u32;

// ─── TokenError ───
#[derive(Debug, Error)]
pub enum TokenError {
    #[error("token not found")]
    NotFound,
    #[error("cyclic alias detected")]
    CyclicAlias,
    #[error("missing value for token in current mode")]
    MissingValue,
    #[error("collection not found")]
    CollectionNotFound,
}

// ─── TokenType ───
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TokenType {
    Color,
    Number,
    Dimension,
    FontFamily,
    FontWeight,
    Duration,
    CubicBezier,
    String,
}

// ─── DimensionUnit ───
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DimensionUnit {
    Px,
    Pt,
    Mm,
    In,
    Rem,
    Em,
    Percent,
}

// ─── TokenValue ───
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum TokenValue {
    Color(Color),
    Number(f32),
    Dimension { value: f32, unit: DimensionUnit },
    FontFamily(std::string::String),
    FontWeight(u16),
    Duration(f32),
    CubicBezier([f32; 4]),
    String(std::string::String),
}

impl TokenValue {
    pub fn token_type(&self) -> TokenType {
        match self {
            Self::Color(_) => TokenType::Color,
            Self::Number(_) => TokenType::Number,
            Self::Dimension { .. } => TokenType::Dimension,
            Self::FontFamily(_) => TokenType::FontFamily,
            Self::FontWeight(_) => TokenType::FontWeight,
            Self::Duration(_) => TokenType::Duration,
            Self::CubicBezier(_) => TokenType::CubicBezier,
            Self::String(_) => TokenType::String,
        }
    }
}

// ─── TokenResolve ───
/// Uses untagged serde so that `Direct(TokenValue)` serializes as the bare TokenValue
/// (adjacently tagged with its own `type`/`value` fields) and `Alias(TokenRef)` serializes
/// as `{"collection_id":...,"token_id":...}`. Using internally-tagged here would produce
/// a "duplicate field `type`" error because TokenValue is already adjacently tagged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TokenResolve {
    Direct(TokenValue),
    Alias(TokenRef),
}

// ─── Token ───
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Token {
    pub id: TokenId,
    pub name: std::string::String,
    pub group: Option<std::string::String>,
    pub values: HashMap<ModeId, TokenResolve>,
}

// ─── Mode ───
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Mode {
    pub id: ModeId,
    pub name: std::string::String,
}

// ─── TokenCollection ───
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TokenCollection {
    pub id: CollectionId,
    pub name: std::string::String,
    pub modes: Vec<Mode>,
    pub default_mode: ModeId,
    pub tokens: Vec<Token>,
}

// ─── DesignTokens ───
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

impl DesignTokens {
    pub fn new() -> Self {
        Self {
            collections: Vec::new(),
            active_modes: HashMap::new(),
            next_collection_id: 0,
            next_token_id: 0,
            next_mode_id: 0,
        }
    }

    /// Add a new collection with the given name and mode names.
    /// Returns the collection's ID.
    pub fn add_collection(&mut self, name: &str, mode_names: Vec<&str>) -> CollectionId {
        let col_id = self.next_collection_id;
        self.next_collection_id += 1;

        let modes: Vec<Mode> = mode_names
            .iter()
            .map(|n| {
                let id = self.next_mode_id;
                self.next_mode_id += 1;
                Mode { id, name: n.to_string() }
            })
            .collect();

        let default_mode = modes.first().map(|m| m.id).unwrap_or(0);

        self.collections.push(TokenCollection {
            id: col_id,
            name: name.to_string(),
            modes,
            default_mode,
            tokens: Vec::new(),
        });

        col_id
    }

    /// Add a token with the given value for ALL modes in the collection.
    /// Returns the token's ID.
    pub fn add_token(
        &mut self,
        collection_id: CollectionId,
        name: &str,
        value: TokenValue,
    ) -> TokenId {
        let tok_id = self.next_token_id;
        self.next_token_id += 1;

        if let Some(col) = self.collections.iter_mut().find(|c| c.id == collection_id) {
            let mode_ids: Vec<ModeId> = col.modes.iter().map(|m| m.id).collect();
            let mut values = HashMap::new();
            for mode_id in mode_ids {
                values.insert(mode_id, TokenResolve::Direct(value.clone()));
            }
            col.tokens.push(Token {
                id: tok_id,
                name: name.to_string(),
                group: None,
                values,
            });
        }

        tok_id
    }

    /// Add a token with the given value for ONLY the specified mode.
    /// Returns the token's ID.
    pub fn add_token_for_mode(
        &mut self,
        collection_id: CollectionId,
        name: &str,
        mode_id: ModeId,
        value: TokenValue,
    ) -> TokenId {
        let tok_id = self.next_token_id;
        self.next_token_id += 1;

        if let Some(col) = self.collections.iter_mut().find(|c| c.id == collection_id) {
            let mut values = HashMap::new();
            values.insert(mode_id, TokenResolve::Direct(value));
            col.tokens.push(Token {
                id: tok_id,
                name: name.to_string(),
                group: None,
                values,
            });
        }

        tok_id
    }

    /// Add an alias token that points to another token.
    /// Returns the alias token's ID.
    pub fn add_alias_token(
        &mut self,
        collection_id: CollectionId,
        name: &str,
        target_collection_id: CollectionId,
        target_token_id: TokenId,
    ) -> TokenId {
        let tok_id = self.next_token_id;
        self.next_token_id += 1;

        if let Some(col) = self.collections.iter_mut().find(|c| c.id == collection_id) {
            let mode_ids: Vec<ModeId> = col.modes.iter().map(|m| m.id).collect();
            let alias = TokenResolve::Alias(TokenRef {
                collection_id: target_collection_id,
                token_id: target_token_id,
            });
            let mut values = HashMap::new();
            for mode_id in mode_ids {
                values.insert(mode_id, alias.clone());
            }
            col.tokens.push(Token {
                id: tok_id,
                name: name.to_string(),
                group: None,
                values,
            });
        }

        tok_id
    }

    /// Update an existing token to be an alias. Returns Err if this would create a cycle.
    pub fn set_alias(
        &mut self,
        collection_id: CollectionId,
        token_id: TokenId,
        target_collection_id: CollectionId,
        target_token_id: TokenId,
    ) -> Result<(), TokenError> {
        // Check for cycles before making the change
        if self.would_create_cycle(
            collection_id,
            token_id,
            target_collection_id,
            target_token_id,
        ) {
            return Err(TokenError::CyclicAlias);
        }

        if let Some(col) = self.collections.iter_mut().find(|c| c.id == collection_id) {
            let mode_ids: Vec<ModeId> = col.modes.iter().map(|m| m.id).collect();
            if let Some(tok) = col.tokens.iter_mut().find(|t| t.id == token_id) {
                let alias = TokenResolve::Alias(TokenRef {
                    collection_id: target_collection_id,
                    token_id: target_token_id,
                });
                for mode_id in mode_ids {
                    tok.values.insert(mode_id, alias.clone());
                }
                return Ok(());
            }
        }
        Err(TokenError::NotFound)
    }

    /// Set the active mode for a collection.
    pub fn set_active_mode(&mut self, collection_id: CollectionId, mode_id: ModeId) {
        self.active_modes.insert(collection_id, mode_id);
    }

    /// Resolve a token to its final value, following aliases and respecting active modes.
    pub fn resolve(
        &self,
        collection_id: CollectionId,
        token_id: TokenId,
    ) -> Result<TokenValue, TokenError> {
        let mut visited = HashSet::new();
        self.resolve_with_visited(collection_id, token_id, &mut visited)
    }

    fn resolve_with_visited(
        &self,
        collection_id: CollectionId,
        token_id: TokenId,
        visited: &mut HashSet<(CollectionId, TokenId)>,
    ) -> Result<TokenValue, TokenError> {
        let key = (collection_id, token_id);
        if visited.contains(&key) {
            return Err(TokenError::CyclicAlias);
        }
        visited.insert(key);

        let col = self
            .collections
            .iter()
            .find(|c| c.id == collection_id)
            .ok_or(TokenError::CollectionNotFound)?;

        let tok = col
            .tokens
            .iter()
            .find(|t| t.id == token_id)
            .ok_or(TokenError::NotFound)?;

        // Try active mode first, then fall back to default mode
        let active_mode = self.active_modes.get(&collection_id).copied();
        let resolve = active_mode
            .and_then(|m| tok.values.get(&m))
            .or_else(|| tok.values.get(&col.default_mode))
            .ok_or(TokenError::MissingValue)?;

        match resolve {
            TokenResolve::Direct(value) => Ok(value.clone()),
            TokenResolve::Alias(token_ref) => self.resolve_with_visited(
                token_ref.collection_id,
                token_ref.token_id,
                visited,
            ),
        }
    }

    /// Check whether setting token `from` to alias `to` would create a cycle.
    fn would_create_cycle(
        &self,
        from_collection: CollectionId,
        from_token: TokenId,
        to_collection: CollectionId,
        to_token: TokenId,
    ) -> bool {
        // If `to` points back to `from` (directly or transitively), it's a cycle.
        // Walk alias chain starting from `to` and check if it reaches `from`.
        let mut visited = HashSet::new();
        let mut current_col = to_collection;
        let mut current_tok = to_token;

        loop {
            if current_col == from_collection && current_tok == from_token {
                return true;
            }
            if visited.contains(&(current_col, current_tok)) {
                // Already a cycle in the existing chain (unrelated), stop
                return false;
            }
            visited.insert((current_col, current_tok));

            let col = match self.collections.iter().find(|c| c.id == current_col) {
                Some(c) => c,
                None => return false,
            };
            let tok = match col.tokens.iter().find(|t| t.id == current_tok) {
                Some(t) => t,
                None => return false,
            };

            // Look up the value using the default mode
            let resolve = match tok.values.get(&col.default_mode) {
                Some(r) => r,
                None => return false,
            };

            match resolve {
                TokenResolve::Alias(token_ref) => {
                    current_col = token_ref.collection_id;
                    current_tok = token_ref.token_id;
                }
                TokenResolve::Direct(_) => return false,
            }
        }
    }
}

impl Default for DesignTokens {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn make_simple_system() -> DesignTokens {
        let mut tokens = DesignTokens::new();
        let light = tokens.add_collection("Colors", vec!["Light", "Dark"]);
        tokens.add_token(light, "blue-500", TokenValue::Color(
            Color::Srgb { r: 0.231, g: 0.510, b: 0.965, a: 1.0 }
        ));
        tokens
    }

    #[test]
    fn resolve_direct_token() {
        let tokens = make_simple_system();
        let col_id = tokens.collections[0].id;
        let tok_id = tokens.collections[0].tokens[0].id;
        let resolved = tokens.resolve(col_id, tok_id).unwrap();
        assert!(matches!(resolved, TokenValue::Color(_)));
    }

    #[test]
    fn resolve_alias_token() {
        let mut tokens = make_simple_system();
        let col_id = tokens.collections[0].id;
        let blue_id = tokens.collections[0].tokens[0].id;
        tokens.add_alias_token(col_id, "color.primary", col_id, blue_id);
        let primary_id = tokens.collections[0].tokens[1].id;
        let resolved = tokens.resolve(col_id, primary_id).unwrap();
        assert!(matches!(resolved, TokenValue::Color(_)));
    }

    #[test]
    fn detect_cycle() {
        let mut tokens = DesignTokens::new();
        let col = tokens.add_collection("Test", vec!["Default"]);
        let a_id = tokens.add_token(col, "a", TokenValue::Number(1.0));
        let b_id = tokens.add_alias_token(col, "b", col, a_id);
        let result = tokens.set_alias(col, a_id, col, b_id);
        assert!(result.is_err());
    }

    #[test]
    fn mode_fallback() {
        let mut tokens = DesignTokens::new();
        let col = tokens.add_collection("Colors", vec!["Light", "Dark"]);
        let light_mode = tokens.collections[0].modes[0].id;
        tokens.add_token_for_mode(col, "bg", light_mode, TokenValue::Color(Color::white()));
        let dark_mode = tokens.collections[0].modes[1].id;
        tokens.set_active_mode(col, dark_mode);
        let tok_id = tokens.collections[0].tokens[0].id;
        let resolved = tokens.resolve(col, tok_id).unwrap();
        assert!(matches!(resolved, TokenValue::Color(_)));
    }

    #[test]
    fn cross_collection_alias() {
        let mut tokens = DesignTokens::new();
        let colors = tokens.add_collection("Colors", vec!["Default"]);
        let blue_id = tokens.add_token(colors, "blue-500", TokenValue::Color(
            Color::Srgb { r: 0.231, g: 0.510, b: 0.965, a: 1.0 }
        ));
        let components = tokens.add_collection("Components", vec!["Default"]);
        let _alias_id = tokens.add_alias_token(components, "button.bg", colors, blue_id);
        let alias_tok_id = tokens.collections[1].tokens[0].id;
        let resolved = tokens.resolve(components, alias_tok_id).unwrap();
        assert!(matches!(resolved, TokenValue::Color(_)));
    }

    #[test]
    fn token_type_derived_from_value() {
        let val = TokenValue::Color(Color::black());
        assert_eq!(val.token_type(), TokenType::Color);
        let val = TokenValue::Number(42.0);
        assert_eq!(val.token_type(), TokenType::Number);
    }

    #[test]
    fn token_value_schema_generates() {
        let schema = schemars::schema_for!(TokenValue);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("TokenValue"));
    }
}
