use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("PNG encoding failed: {0}")]
    PngEncodeFailed(String),
    #[error("SVG generation failed: {0}")]
    SvgGenerationFailed(String),
    #[error("PDF generation failed: {0}")]
    PdfGenerationFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
