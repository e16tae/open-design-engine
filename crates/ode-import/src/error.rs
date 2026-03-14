use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Figma API error: {status} - {message}")]
    Api { status: u32, message: String },

    #[error("Missing required field: {field} on node {node_id}")]
    MissingField { node_id: String, field: String },

    #[error("SVG path parse error: {0}")]
    PathParse(String),
}

#[derive(Debug, Clone)]
pub struct ImportWarning {
    pub node_id: String,
    pub node_name: String,
    pub message: String,
}

impl std::fmt::Display for ImportWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.node_id, self.node_name, self.message)
    }
}
