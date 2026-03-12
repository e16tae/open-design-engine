use crate::error::ExportError;

/// PNG export using tiny-skia's built-in encoder.
pub struct PngExporter;

impl PngExporter {
    /// Save a Pixmap to a PNG file.
    pub fn export(pixmap: &tiny_skia::Pixmap, path: &std::path::Path) -> Result<(), ExportError> {
        let bytes = Self::export_bytes(pixmap)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Encode a Pixmap to PNG bytes in memory.
    pub fn export_bytes(pixmap: &tiny_skia::Pixmap) -> Result<Vec<u8>, ExportError> {
        pixmap.encode_png().map_err(|e| ExportError::PngEncodeFailed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_bytes_produces_valid_png() {
        let mut pixmap = tiny_skia::Pixmap::new(10, 10).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let bytes = PngExporter::export_bytes(&pixmap).unwrap();
        // PNG magic bytes
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn export_to_file_creates_file() {
        let mut pixmap = tiny_skia::Pixmap::new(10, 10).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(0, 255, 0, 255));
        let path = std::env::temp_dir().join("ode_test_export.png");
        PngExporter::export(&pixmap, &path).unwrap();
        assert!(path.exists());
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
        std::fs::remove_file(&path).ok();
    }
}
