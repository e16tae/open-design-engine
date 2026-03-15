use ode_format::node::NodeId;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("empty scene — no canvas roots")]
    EmptyScene,
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("pixmap creation failed: {width}x{height}")]
    PixmapCreationFailed { width: u32, height: u32 },
    #[error("boolean operation failed: {0}")]
    BooleanOpFailed(String),
}

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("root node not found: {0:?}")]
    RootNodeNotFound(NodeId),
    #[error("token resolution failed: {0}")]
    TokenError(#[from] ode_format::tokens::TokenError),
    #[error("document has no canvas roots")]
    NoCanvasRoots,
    #[error("text rendering failed: {0}")]
    TextError(#[from] ode_text::TextError),
    #[error("instance cycle detected: {0}")]
    InstanceCycle(String),
}
