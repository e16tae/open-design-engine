use std::io::Read;
use std::path::{Path, PathBuf};

use crate::asset::{AssetEntry, AssetStore};
use crate::document::Document;
use crate::meta::Meta;
use crate::style::{Fill, ImageSource, Paint};

// ─── OdeSource ───

/// Where this container was loaded from (or will be saved to).
#[derive(Debug, Clone)]
pub enum OdeSource {
    /// A packed `.ode` ZIP archive.
    Packed(PathBuf),
    /// An unpacked directory containing `document.json`, `meta.json`, and `assets/`.
    Unpacked(PathBuf),
    /// Read from stdin (not saveable without an explicit path).
    Stdin,
    /// A single `.ode.json` legacy file (document only, no meta/assets directory).
    LegacyJson(PathBuf),
}

impl OdeSource {
    /// Auto-detect source kind from a path string.
    ///
    /// Rules:
    /// - `"-"` -> Stdin
    /// - existing directory -> Unpacked
    /// - existing file: first 4 bytes are ZIP magic (`PK\x03\x04`) -> Packed, else -> LegacyJson
    /// - non-existent path: ends with `"/"` -> Unpacked, ends with `".ode"` -> Packed, else -> LegacyJson
    pub fn detect(path: &str) -> Self {
        if path == "-" {
            return Self::Stdin;
        }

        let p = Path::new(path);

        if p.is_dir() {
            return Self::Unpacked(p.to_path_buf());
        }

        if p.is_file() {
            // Check ZIP magic bytes
            if let Ok(mut f) = std::fs::File::open(p) {
                let mut magic = [0u8; 4];
                if f.read_exact(&mut magic).is_ok() && magic == [0x50, 0x4B, 0x03, 0x04] {
                    return Self::Packed(p.to_path_buf());
                }
            }
            return Self::LegacyJson(p.to_path_buf());
        }

        // Non-existent path — guess by suffix
        if path.ends_with('/') {
            Self::Unpacked(p.to_path_buf())
        } else if path.ends_with(".ode") {
            Self::Packed(p.to_path_buf())
        } else {
            Self::LegacyJson(p.to_path_buf())
        }
    }
}

// ─── ContainerError ───

#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("cannot save a stdin-sourced container without an explicit path")]
    StdinNotSaveable,

    #[error("zip error: {0}")]
    Zip(String),
}

// ─── OdeContainer ───

/// Top-level container that ties together the document, metadata, and assets.
#[derive(Debug)]
pub struct OdeContainer {
    pub document: Document,
    pub meta: Meta,
    pub assets: AssetStore,
    pub source: OdeSource,
}

impl OdeContainer {
    // ── Constructors ──

    /// Create a brand-new container from a `Document`.
    pub fn from_document(document: Document, generator: &str) -> Self {
        Self {
            document,
            meta: Meta::new(generator),
            assets: AssetStore::new(),
            source: OdeSource::Unpacked(PathBuf::new()),
        }
    }

    /// Open a container, auto-detecting the format from `path`.
    pub fn open(path: &str) -> Result<Self, ContainerError> {
        match OdeSource::detect(path) {
            OdeSource::Packed(p) => Self::open_packed(&p),
            OdeSource::Unpacked(p) => Self::open_unpacked(&p),
            OdeSource::Stdin => Self::open_stdin(),
            OdeSource::LegacyJson(p) => Self::open_legacy(&p),
        }
    }

    /// Open an unpacked directory: reads `document.json`, `meta.json`, and registers assets.
    pub fn open_unpacked(dir: &Path) -> Result<Self, ContainerError> {
        let doc_path = dir.join("document.json");
        let meta_path = dir.join("meta.json");

        let doc_json = std::fs::read_to_string(&doc_path)?;
        let document: Document = serde_json::from_str(&doc_json)?;

        let meta: Meta = if meta_path.exists() {
            let meta_json = std::fs::read_to_string(&meta_path)?;
            serde_json::from_str(&meta_json)?
        } else {
            Meta::legacy()
        };

        let mut assets = AssetStore::new();
        let assets_dir = dir.join("assets");
        if assets_dir.is_dir() {
            for entry in std::fs::read_dir(&assets_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let hash = stem.to_string();
                        assets.register_on_disk(hash, path);
                    }
                }
            }
        }

        Ok(Self {
            document,
            meta,
            assets,
            source: OdeSource::Unpacked(dir.to_path_buf()),
        })
    }

    /// Open a legacy `.ode.json` file (document only).
    pub fn open_legacy(file: &Path) -> Result<Self, ContainerError> {
        let json = std::fs::read_to_string(file)?;
        let document: Document = serde_json::from_str(&json)?;

        Ok(Self {
            document,
            meta: Meta::legacy(),
            assets: AssetStore::new(),
            source: OdeSource::LegacyJson(file.to_path_buf()),
        })
    }

    /// Open from stdin (reads the entire stream as JSON).
    pub fn open_stdin() -> Result<Self, ContainerError> {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        let document: Document = serde_json::from_str(&buf)?;

        Ok(Self {
            document,
            meta: Meta::legacy(),
            assets: AssetStore::new(),
            source: OdeSource::Stdin,
        })
    }

    /// Open a packed `.ode` ZIP file.
    pub fn open_packed(_file: &Path) -> Result<Self, ContainerError> {
        todo!("ZIP support (Task 4)")
    }

    // ── Save ──

    /// Save to the original source location.
    pub fn save(&mut self) -> Result<(), ContainerError> {
        match self.source.clone() {
            OdeSource::Unpacked(dir) => self.save_unpacked(&dir),
            OdeSource::Packed(path) => self.save_packed(&path),
            OdeSource::LegacyJson(path) => {
                // Save back as legacy single-file JSON
                self.meta.touch();
                let json = serde_json::to_string_pretty(&self.document)?;
                std::fs::write(&path, json)?;
                Ok(())
            }
            OdeSource::Stdin => Err(ContainerError::StdinNotSaveable),
        }
    }

    /// Save as an unpacked directory.
    pub fn save_unpacked(&mut self, dir: &Path) -> Result<(), ContainerError> {
        self.extract_embedded_assets();
        self.meta.touch();

        std::fs::create_dir_all(dir)?;

        // Write document.json
        let doc_json = serde_json::to_string_pretty(&self.document)?;
        std::fs::write(dir.join("document.json"), doc_json)?;

        // Write meta.json
        let meta_json = serde_json::to_string_pretty(&self.meta)?;
        std::fs::write(dir.join("meta.json"), meta_json)?;

        // Write assets
        if !self.assets.is_empty() {
            let assets_dir = dir.join("assets");
            std::fs::create_dir_all(&assets_dir)?;

            for (hash, entry) in self.assets.iter() {
                let data = match entry {
                    AssetEntry::Loaded(bytes) => bytes,
                    AssetEntry::OnDisk(_) => continue, // already on disk
                };
                // Find extension from document references
                let ext = self.find_asset_ext(hash).unwrap_or_else(|| "bin".to_string());
                let filename = format!("{hash}.{ext}");
                std::fs::write(assets_dir.join(filename), data)?;
            }
        }

        // Update source to point here
        self.source = OdeSource::Unpacked(dir.to_path_buf());
        Ok(())
    }

    /// Save as a packed `.ode` ZIP file.
    pub fn save_packed(&mut self, _path: &Path) -> Result<(), ContainerError> {
        todo!("ZIP support (Task 4)")
    }

    // ── Asset Extraction ──

    /// Walk all nodes and convert `Embedded` image sources to `Linked` references,
    /// adding the image data to the `AssetStore`.
    pub fn extract_embedded_assets(&mut self) {
        // Collect all node IDs first (to avoid borrow conflicts).
        let node_ids: Vec<_> = self.document.nodes.iter().map(|(id, _)| id).collect();

        for node_id in node_ids {
            let node = &mut self.document.nodes[node_id];

            // Extract from ImageData source
            if let crate::node::NodeKind::Image(ref mut img_data) = node.kind {
                if let Some(ImageSource::Embedded { data }) = &img_data.source {
                    let ext = detect_image_ext(data);
                    let path = self.assets.add_image(data.clone(), ext);
                    img_data.source = Some(ImageSource::Linked { path });
                }
            }

            // Extract from fills in VisualProps
            if let Some(visual) = node.kind.visual_mut() {
                extract_embedded_from_fills(&mut visual.fills, &mut self.assets);
            }
        }
    }

    /// Find the file extension for an asset hash by scanning document references.
    pub fn find_asset_ext(&self, hash: &str) -> Option<String> {
        let prefix = format!("assets/{hash}.");

        for (_, node) in self.document.nodes.iter() {
            // Check ImageData source
            if let crate::node::NodeKind::Image(ref img_data) = node.kind {
                if let Some(ImageSource::Linked { path }) = &img_data.source {
                    if path.starts_with(&prefix) {
                        return path.rsplit('.').next().map(|s| s.to_string());
                    }
                }
            }

            // Check fills
            if let Some(visual) = node.kind.visual() {
                for fill in &visual.fills {
                    if let Paint::ImageFill {
                        source: ImageSource::Linked { path },
                        ..
                    } = &fill.paint
                    {
                        if path.starts_with(&prefix) {
                            return path.rsplit('.').next().map(|s| s.to_string());
                        }
                    }
                }
            }
        }
        None
    }
}

// ─── Helper Functions ───

/// Extract embedded images from a slice of fills, replacing them with linked references.
fn extract_embedded_from_fills(fills: &mut [Fill], assets: &mut AssetStore) {
    for fill in fills.iter_mut() {
        if let Paint::ImageFill {
            source: ref mut src,
            ..
        } = fill.paint
        {
            if let ImageSource::Embedded { data } = src {
                let ext = detect_image_ext(data);
                let path = assets.add_image(data.clone(), ext);
                *src = ImageSource::Linked { path };
            }
        }
    }
}

/// Detect image format from magic bytes. Returns file extension string.
pub fn detect_image_ext(data: &[u8]) -> &'static str {
    if data.len() >= 8 && data[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        "png"
    } else if data.len() >= 3 && data[..3] == [0xFF, 0xD8, 0xFF] {
        "jpg"
    } else if data.len() >= 4 && &data[..4] == b"GIF8" {
        "gif"
    } else if data.len() >= 4 && &data[..4] == b"RIFF" {
        // Could be WEBP — check further
        if data.len() >= 12 && &data[8..12] == b"WEBP" {
            "webp"
        } else {
            "bin"
        }
    } else if data.len() >= 4 && &data[..4] == b"<svg" {
        "svg"
    } else {
        "bin"
    }
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::node::{Node, NodeKind};
    use crate::style::{BlendMode, Fill, ImageFillMode, ImageSource, Paint, StyleValue};

    #[test]
    fn detect_source_directory() {
        let dir = tempfile::tempdir().unwrap();
        let source = OdeSource::detect(dir.path().to_str().unwrap());
        assert!(matches!(source, OdeSource::Unpacked(_)));
    }

    #[test]
    fn detect_source_stdin() {
        let source = OdeSource::detect("-");
        assert!(matches!(source, OdeSource::Stdin));
    }

    #[test]
    fn detect_source_legacy_json() {
        // Non-existent path ending in .ode.json
        let source = OdeSource::detect("/tmp/nonexistent-test-file.ode.json");
        assert!(matches!(source, OdeSource::LegacyJson(_)));
    }

    #[test]
    fn save_and_open_unpacked() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("my_design");

        // Create a document with one frame
        let mut doc = Document::new("Test Doc");
        let frame = Node::new_frame("Artboard", 800.0, 600.0);
        let id = doc.nodes.insert(frame);
        doc.canvas.push(id);

        let mut container = OdeContainer::from_document(doc, "test");
        container.save_unpacked(&out).unwrap();

        // Verify files exist
        assert!(out.join("document.json").exists());
        assert!(out.join("meta.json").exists());

        // Re-open
        let loaded = OdeContainer::open_unpacked(&out).unwrap();
        assert_eq!(loaded.document.name, "Test Doc");
        assert_eq!(loaded.document.canvas.len(), 1);
        assert_eq!(loaded.meta.format_version, "1.0.0");
    }

    #[test]
    fn open_legacy_json() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("design.ode.json");

        // Write a minimal document JSON
        let doc = Document::new("Legacy");
        let json = serde_json::to_string_pretty(&doc).unwrap();
        std::fs::write(&file, &json).unwrap();

        let container = OdeContainer::open_legacy(&file).unwrap();
        assert_eq!(container.document.name, "Legacy");
        assert_eq!(container.meta.generator, "ode-format (legacy)");
    }

    #[test]
    fn extract_embedded_assets_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("extracted");

        // Fake PNG data (starts with PNG magic)
        let png_data = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG header
            0x00, 0x00, 0x00, 0x0D, // IHDR chunk length
        ];

        // Create a node with an embedded image fill
        let mut doc = Document::new("Embedded Test");
        let mut frame = Node::new_frame("Frame", 100.0, 100.0);
        if let NodeKind::Frame(ref mut data) = frame.kind {
            data.visual.fills.push(Fill {
                paint: Paint::ImageFill {
                    source: ImageSource::Embedded {
                        data: png_data.clone(),
                    },
                    mode: ImageFillMode::Fill,
                },
                opacity: StyleValue::Raw(1.0),
                blend_mode: BlendMode::Normal,
                visible: true,
            });
        }
        let id = doc.nodes.insert(frame);
        doc.canvas.push(id);

        let mut container = OdeContainer::from_document(doc, "test");
        container.save_unpacked(&out).unwrap();

        // After save, the embedded image should be extracted:
        // 1. Fill should now be Linked (not Embedded)
        let node = &container.document.nodes[id];
        let visual = node.kind.visual().unwrap();
        if let Paint::ImageFill { source, .. } = &visual.fills[0].paint {
            match source {
                ImageSource::Linked { path } => {
                    assert!(path.starts_with("assets/"));
                    assert!(path.ends_with(".png"));
                }
                ImageSource::Embedded { .. } => panic!("Expected Linked, got Embedded"),
            }
        } else {
            panic!("Expected ImageFill");
        }

        // 2. Asset file should exist on disk
        assert!(out.join("assets").is_dir());
        let asset_files: Vec<_> = std::fs::read_dir(out.join("assets"))
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(asset_files.len(), 1);
        let asset_name = asset_files[0].file_name().into_string().unwrap();
        assert!(asset_name.ends_with(".png"));

        // 3. Asset store should have one entry
        assert_eq!(container.assets.len(), 1);
    }
}
