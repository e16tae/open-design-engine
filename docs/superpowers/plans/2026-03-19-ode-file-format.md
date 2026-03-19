# `.ode` File Format Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transition the ODE file format from `.ode.json` (plain JSON) to `.ode` (ZIP container) with `pack`/`unpack` workflow for agent-friendly editing.

**Architecture:** `OdeContainer` abstracts ZIP (Packed) and directory (Unpacked) formats behind a unified open/save API. `AssetStore` manages binary assets with content-hash filenames. `Meta` tracks container metadata. CLI commands auto-detect input format.

**Tech Stack:** Rust, `zip` crate (ZIP I/O), `sha2` crate (content hashing), existing serde/serde_json serialization

**Spec:** `docs/superpowers/specs/2026-03-19-ode-file-format-design.md`

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `crates/ode-format/src/meta.rs` | `Meta` struct — container metadata (format_version, generator, timestamps) |
| `crates/ode-format/src/asset.rs` | `AssetStore` — binary asset management with SHA-256 content hashing |
| `crates/ode-format/src/container.rs` | `OdeContainer`, `OdeSource` — unified open/save for Packed, Unpacked, Stdin, LegacyJson |

### Modified Files

| File | Changes |
|------|---------|
| `crates/ode-format/Cargo.toml` | Add `zip`, `sha2` dependencies |
| `crates/ode-format/src/lib.rs` | Export new modules (`meta`, `asset`, `container`) |
| `crates/ode-core/src/convert.rs:321-358` | `emit_image()` accepts `&AssetStore`, loads bytes from store |
| `crates/ode-core/src/convert.rs:21-30` | `Scene::from_document` / `from_document_with_resize` accept `&AssetStore` |
| `crates/ode-cli/src/main.rs` | Add `Pack`/`Unpack` commands, update help strings `.ode.json` → `.ode` |
| `crates/ode-cli/src/commands.rs` | Replace `load_input()` with `OdeContainer::open()`, add `cmd_pack`/`cmd_unpack` |
| `crates/ode-cli/src/mutate.rs` | Use `OdeContainer` for load/save in mutation commands |
| `crates/ode-import/src/figma/convert.rs:520-530` | Use `AssetStore::add_image()` instead of `ImageSource::Embedded` |
| `crates/ode-cli/tests/*.rs` | Update `.ode.json` → `.ode`, add Packed/Unpacked/Legacy test variants |

---

## Task 1: `Meta` struct

**Files:**
- Create: `crates/ode-format/src/meta.rs`
- Modify: `crates/ode-format/src/lib.rs`

- [ ] **Step 1: Write the failing test**

In `crates/ode-format/src/meta.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_new_has_correct_defaults() {
        let meta = Meta::new("ode-cli 0.1.0");
        assert_eq!(meta.format_version, "1.0.0");
        assert_eq!(meta.generator, "ode-cli 0.1.0");
        assert!(!meta.created_at.is_empty());
        assert!(!meta.modified_at.is_empty());
    }

    #[test]
    fn meta_roundtrip_json() {
        let meta = Meta::new("ode-cli 0.1.0");
        let json = serde_json::to_string_pretty(&meta).unwrap();
        let parsed: Meta = serde_json::from_str(&json).unwrap();
        assert_eq!(meta.format_version, parsed.format_version);
        assert_eq!(meta.generator, parsed.generator);
    }

    #[test]
    fn meta_legacy_defaults() {
        let meta = Meta::legacy();
        assert_eq!(meta.generator, "ode-format (legacy)");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-format meta`
Expected: compilation error — `Meta` not defined

- [ ] **Step 3: Implement Meta**

In `crates/ode-format/src/meta.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub format_version: String,
    pub generator: String,
    pub created_at: String,
    pub modified_at: String,
}

impl Meta {
    pub fn new(generator: &str) -> Self {
        let now = now_iso8601();
        Self {
            format_version: "1.0.0".to_string(),
            generator: generator.to_string(),
            created_at: now.clone(),
            modified_at: now,
        }
    }

    /// Create Meta with defaults for legacy .ode.json files.
    pub fn legacy() -> Self {
        let now = now_iso8601();
        Self {
            format_version: "1.0.0".to_string(),
            generator: "ode-format (legacy)".to_string(),
            created_at: now.clone(),
            modified_at: now,
        }
    }

    pub fn touch(&mut self) {
        self.modified_at = now_iso8601();
    }
}

fn now_iso8601() -> String {
    // Simple UTC timestamp without external crate.
    // NOTE: Approximate date (ignores leap years, assumes 30-day months).
    // Acceptable for metadata timestamps — exact dates are not critical.
    use std::time::SystemTime;
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let (s, m, h) = (secs % 60, (secs / 60) % 60, (secs / 3600) % 24);
    let days = secs / 86400;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        1970 + days / 365,
        (days % 365) / 30 + 1,
        (days % 365) % 30 + 1,
        h, m, s
    )
}
```

- [ ] **Step 4: Add module to lib.rs**

In `crates/ode-format/src/lib.rs`, add:

```rust
pub mod meta;
pub use meta::Meta;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ode-format meta`
Expected: 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/src/meta.rs crates/ode-format/src/lib.rs
git commit -m "feat(ode-format): add Meta struct for container metadata"
```

---

## Task 2: `AssetStore`

**Files:**
- Create: `crates/ode-format/src/asset.rs`
- Modify: `crates/ode-format/Cargo.toml`
- Modify: `crates/ode-format/src/lib.rs`

- [ ] **Step 1: Add dependencies**

In `crates/ode-format/Cargo.toml`, add to `[dependencies]`:

```toml
sha2 = "0.10"
```

- [ ] **Step 2: Write the failing tests**

In `crates/ode-format/src/asset.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_image_returns_hash_filename() {
        let mut store = AssetStore::new();
        let name = store.add_image(vec![0x89, 0x50, 0x4E, 0x47], "png");
        assert!(name.ends_with(".png"));
        assert_eq!(name.len(), "assets/".len() + 16 + ".png".len());
        assert!(name.starts_with("assets/"));
    }

    #[test]
    fn add_same_image_deduplicates() {
        let mut store = AssetStore::new();
        let data = vec![1, 2, 3, 4, 5];
        let name1 = store.add_image(data.clone(), "png");
        let name2 = store.add_image(data, "png");
        assert_eq!(name1, name2);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn get_image_returns_bytes() {
        let mut store = AssetStore::new();
        let data = vec![0x89, 0x50, 0x4E, 0x47];
        let name = store.add_image(data.clone(), "png");
        let hash = name.strip_prefix("assets/").unwrap().split('.').next().unwrap();
        let retrieved = store.get_image(hash).unwrap();
        assert_eq!(retrieved, &data);
    }

    #[test]
    fn get_nonexistent_returns_error() {
        let mut store = AssetStore::new();
        assert!(store.get_image("nonexistent").is_err());
    }

    #[test]
    fn hash_is_16_hex_chars() {
        let store = AssetStore::new();
        let hash = AssetStore::compute_hash(&[1, 2, 3]);
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ode-format asset`
Expected: compilation error — `AssetStore` not defined

- [ ] **Step 4: Implement AssetStore**

In `crates/ode-format/src/asset.rs`:

```rust
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum AssetEntry {
    /// On disk, not yet loaded into memory
    OnDisk(PathBuf),
    /// Loaded into memory
    Loaded(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct AssetStore {
    /// hash → entry
    entries: HashMap<String, AssetEntry>,
}

impl AssetStore {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Compute SHA-256 hash, return first 16 hex chars.
    pub fn compute_hash(data: &[u8]) -> String {
        let digest = Sha256::digest(data);
        hex_encode(&digest[..8]) // 8 bytes = 16 hex chars
    }

    /// Register image bytes. Returns relative path like "assets/{hash}.{ext}".
    /// Deduplicates identical content.
    pub fn add_image(&mut self, data: Vec<u8>, ext: &str) -> String {
        let hash = Self::compute_hash(&data);

        // Check for collision: same hash, different content
        if let Some(existing) = self.entries.get(&hash) {
            let existing_bytes = match existing {
                AssetEntry::Loaded(b) => b,
                AssetEntry::OnDisk(_) => {
                    // Already on disk with same hash — assume dedup
                    return format!("assets/{hash}.{ext}");
                }
            };
            if existing_bytes == &data {
                // Same content — dedup
                return format!("assets/{hash}.{ext}");
            }
            // Collision: different content, same hash prefix — add suffix
            let mut suffix = 2;
            loop {
                let key = format!("{hash}_{suffix}");
                if !self.entries.contains_key(&key) {
                    self.entries.insert(key.clone(), AssetEntry::Loaded(data));
                    return format!("assets/{key}.{ext}");
                }
                suffix += 1;
            }
        }

        self.entries.insert(hash.clone(), AssetEntry::Loaded(data));
        format!("assets/{hash}.{ext}")
    }

    /// Get image bytes by hash key (lazy loads from disk if needed).
    /// Takes `&mut self` because it may load from disk into memory.
    pub fn get_image(&mut self, hash: &str) -> Result<&[u8], AssetError> {
        // Two-phase: check if OnDisk and load, then return reference
        if let Some(AssetEntry::OnDisk(path)) = self.entries.get(hash) {
            let path = path.clone();
            let bytes = std::fs::read(&path).map_err(|e| AssetError::Io(e.to_string()))?;
            self.entries.insert(hash.to_string(), AssetEntry::Loaded(bytes));
        }

        match self.entries.get(hash) {
            Some(AssetEntry::Loaded(data)) => Ok(data),
            Some(AssetEntry::OnDisk(_)) => unreachable!(),
            None => Err(AssetError::NotFound(hash.to_string())),
        }
    }

    /// Get already-loaded image bytes (no disk I/O, safe for render hot path).
    /// Returns None for OnDisk entries that haven't been loaded yet.
    pub fn get_loaded(&self, hash: &str) -> Option<&[u8]> {
        match self.entries.get(hash) {
            Some(AssetEntry::Loaded(data)) => Some(data),
            _ => None,
        }
    }

    /// Ensure all OnDisk entries are loaded into memory.
    /// Call this before passing to render pipeline.
    pub fn preload_all(&mut self) -> Result<(), AssetError> {
        let on_disk: Vec<(String, PathBuf)> = self
            .entries
            .iter()
            .filter_map(|(k, v)| {
                if let AssetEntry::OnDisk(p) = v {
                    Some((k.clone(), p.clone()))
                } else {
                    None
                }
            })
            .collect();
        for (hash, path) in on_disk {
            let bytes = std::fs::read(&path).map_err(|e| AssetError::Io(e.to_string()))?;
            self.entries.insert(hash, AssetEntry::Loaded(bytes));
        }
        Ok(())
    }

    /// Register an on-disk asset (for lazy loading during open).
    pub fn register_on_disk(&mut self, hash: String, path: PathBuf) {
        self.entries.insert(hash, AssetEntry::OnDisk(path));
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries (for writing to ZIP/disk).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &AssetEntry)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }
}

impl Default for AssetStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("asset not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(String),
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 5: Add module to lib.rs**

In `crates/ode-format/src/lib.rs`, add:

```rust
pub mod asset;
pub use asset::{AssetStore, AssetEntry, AssetError};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p ode-format asset`
Expected: 5 tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/ode-format/src/asset.rs crates/ode-format/src/lib.rs crates/ode-format/Cargo.toml
git commit -m "feat(ode-format): add AssetStore with content-hash dedup"
```

---

## Task 3: `OdeContainer` — Unpacked & LegacyJson

**Files:**
- Create: `crates/ode-format/src/container.rs`
- Modify: `crates/ode-format/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

In `crates/ode-format/src/container.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use std::fs;
    use tempfile::TempDir;

    fn make_test_doc() -> Document {
        Document::new("Test")
    }

    #[test]
    fn detect_source_directory() {
        let dir = TempDir::new().unwrap();
        let path = dir.path();
        fs::write(path.join("document.json"), "{}").unwrap();
        let source = OdeSource::detect(path.to_str().unwrap());
        assert!(matches!(source, OdeSource::Unpacked(_)));
    }

    #[test]
    fn detect_source_stdin() {
        let source = OdeSource::detect("-");
        assert!(matches!(source, OdeSource::Stdin));
    }

    #[test]
    fn detect_source_legacy_json() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.ode.json");
        fs::write(&file, r#"{"format_version":[0,2,0],"name":"T","nodes":[]}"#).unwrap();
        let source = OdeSource::detect(file.to_str().unwrap());
        assert!(matches!(source, OdeSource::LegacyJson(_)));
    }

    #[test]
    fn save_and_open_unpacked() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("design");

        let doc = make_test_doc();
        let mut container = OdeContainer::from_document(doc, "ode-test");
        container.save_unpacked(&out).unwrap();

        // Verify files exist
        assert!(out.join("document.json").exists());
        assert!(out.join("meta.json").exists());
        assert!(out.join("assets").is_dir());

        // Re-open
        let loaded = OdeContainer::open(out.to_str().unwrap()).unwrap();
        assert_eq!(loaded.document.name, "Test");
        assert_eq!(loaded.meta.format_version, "1.0.0");
    }

    #[test]
    fn open_legacy_json() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("old.ode.json");
        let doc = make_test_doc();
        let json = serde_json::to_string_pretty(&doc).unwrap();
        fs::write(&file, &json).unwrap();

        let loaded = OdeContainer::open(file.to_str().unwrap()).unwrap();
        assert_eq!(loaded.document.name, "Test");
        assert_eq!(loaded.meta.generator, "ode-format (legacy)");
    }

    #[test]
    fn extract_embedded_assets_on_save() {
        use crate::node::{Node, NodeKind};
        use crate::style::ImageSource;

        let dir = TempDir::new().unwrap();
        let out = dir.path().join("design");

        let mut doc = Document::new("ImageTest");
        let mut frame = Node::new_frame("Root", 200.0, 150.0);
        let mut img = Node::new_image("Photo", 100.0, 80.0);

        // Set up Embedded image
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        if let NodeKind::Image(ref mut data) = img.kind {
            data.source = Some(ImageSource::Embedded {
                data: png_data.clone(),
            });
        }

        frame.children.push(doc.nodes.insert(img));
        let root = doc.nodes.insert(frame);
        doc.canvas.push(root);

        let mut container = OdeContainer::from_document(doc, "ode-test");
        container.save_unpacked(&out).unwrap();

        // Verify: assets/ has a .png file
        let assets_dir = out.join("assets");
        let asset_files: Vec<_> = fs::read_dir(&assets_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(asset_files.len(), 1);
        assert!(asset_files[0].path().extension().unwrap() == "png");

        // Verify: document.json has Linked, not Embedded
        let doc_json = fs::read_to_string(out.join("document.json")).unwrap();
        assert!(doc_json.contains("linked"));
        assert!(!doc_json.contains("embedded"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ode-format container`
Expected: compilation error — `OdeContainer` not defined

- [ ] **Step 3: Add `tempfile` dev-dependency**

In `crates/ode-format/Cargo.toml`, add:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Implement OdeContainer (Unpacked + LegacyJson + Stdin)**

In `crates/ode-format/src/container.rs`:

```rust
use crate::asset::AssetStore;
use crate::meta::Meta;
use crate::Document;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum OdeSource {
    Packed(PathBuf),
    Unpacked(PathBuf),
    Stdin,
    LegacyJson(PathBuf),
}

impl OdeSource {
    /// Auto-detect source type from path string.
    pub fn detect(path: &str) -> Self {
        if path == "-" {
            return Self::Stdin;
        }
        let p = Path::new(path);
        if p.is_dir() {
            return Self::Unpacked(p.to_path_buf());
        }
        if p.is_file() {
            // Check ZIP magic bytes (PK\x03\x04) — only read first 4 bytes
            if let Ok(f) = std::fs::File::open(p) {
                use std::io::Read;
                let mut magic = [0u8; 4];
                if (&f).take(4).read(&mut magic).unwrap_or(0) == 4
                    && magic == [0x50, 0x4B, 0x03, 0x04]
                {
                    return Self::Packed(p.to_path_buf());
                }
            }
            // Not a ZIP — legacy JSON
            return Self::LegacyJson(p.to_path_buf());
        }
        // Path doesn't exist yet — guess from extension
        if path.ends_with('/') {
            Self::Unpacked(p.to_path_buf())
        } else if path.ends_with(".ode") {
            Self::Packed(p.to_path_buf())
        } else {
            Self::LegacyJson(p.to_path_buf())
        }
    }
}

pub struct OdeContainer {
    pub document: Document,
    pub meta: Meta,
    pub assets: AssetStore,
    pub source: OdeSource,
}

#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cannot save stdin source — use save_packed() or save_unpacked()")]
    StdinNotSaveable,
    #[error("zip error: {0}")]
    Zip(String),
}

impl OdeContainer {
    /// Create a new container from an in-memory Document.
    pub fn from_document(document: Document, generator: &str) -> Self {
        Self {
            document,
            meta: Meta::new(generator),
            assets: AssetStore::new(),
            source: OdeSource::Stdin, // no disk source yet
        }
    }

    /// Open from path (auto-detects format).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ContainerError> {
        let path_str = path.as_ref().to_string_lossy();
        let source = OdeSource::detect(&path_str);
        match source {
            OdeSource::Unpacked(ref dir) => Self::open_unpacked(dir),
            OdeSource::LegacyJson(ref file) => Self::open_legacy(file),
            OdeSource::Packed(ref file) => Self::open_packed(file),
            OdeSource::Stdin => Self::open_stdin(),
        }
    }

    fn open_unpacked(dir: &Path) -> Result<Self, ContainerError> {
        let doc_json = std::fs::read_to_string(dir.join("document.json"))?;
        let document: Document = serde_json::from_str(&doc_json)?;

        let meta = if dir.join("meta.json").exists() {
            let meta_json = std::fs::read_to_string(dir.join("meta.json"))?;
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
                        assets.register_on_disk(stem.to_string(), path);
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

    fn open_legacy(file: &Path) -> Result<Self, ContainerError> {
        let json = std::fs::read_to_string(file)?;
        let document: Document = serde_json::from_str(&json)?;
        let meta = Meta::legacy();
        Ok(Self {
            document,
            meta,
            assets: AssetStore::new(),
            source: OdeSource::LegacyJson(file.to_path_buf()),
        })
    }

    fn open_stdin() -> Result<Self, ContainerError> {
        use std::io::Read;
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

    fn open_packed(_file: &Path) -> Result<Self, ContainerError> {
        // Implemented in Task 4
        todo!("ZIP support")
    }

    /// Save to original source.
    pub fn save(&mut self) -> Result<(), ContainerError> {
        match &self.source {
            OdeSource::Unpacked(dir) => {
                let dir = dir.clone();
                self.save_unpacked(&dir)
            }
            OdeSource::Packed(file) => {
                let file = file.clone();
                self.save_packed(&file)
            }
            OdeSource::LegacyJson(file) => {
                // Save as legacy JSON (same format)
                self.meta.touch();
                let json = serde_json::to_string_pretty(&self.document)?;
                std::fs::write(file, json)?;
                Ok(())
            }
            OdeSource::Stdin => Err(ContainerError::StdinNotSaveable),
        }
    }

    /// Save as unpacked directory.
    pub fn save_unpacked(&mut self, dir: &Path) -> Result<(), ContainerError> {
        self.meta.touch();
        self.extract_embedded_assets();

        std::fs::create_dir_all(dir)?;
        std::fs::create_dir_all(dir.join("assets"))?;

        // Write document.json
        let doc_json = serde_json::to_string_pretty(&self.document)?;
        std::fs::write(dir.join("document.json"), doc_json)?;

        // Write meta.json
        let meta_json = serde_json::to_string_pretty(&self.meta)?;
        std::fs::write(dir.join("meta.json"), meta_json)?;

        // Write assets
        for (hash, entry) in self.assets.iter() {
            if let crate::asset::AssetEntry::Loaded(data) = entry {
                // Find the extension from document references (default to bin)
                let ext = self.find_asset_ext(hash).unwrap_or("bin".to_string());
                std::fs::write(dir.join("assets").join(format!("{hash}.{ext}")), data)?;
            }
        }

        Ok(())
    }

    /// Save as packed .ode ZIP file.
    pub fn save_packed(&mut self, _path: &Path) -> Result<(), ContainerError> {
        // Implemented in Task 4
        todo!("ZIP support")
    }

    /// Walk all nodes and convert Embedded images to Linked + asset store.
    fn extract_embedded_assets(&mut self) {
        use crate::style::ImageSource;

        let node_ids: Vec<_> = self.document.nodes.iter().map(|(id, _)| id).collect();
        for node_id in node_ids {
            let node = &self.document.nodes[node_id];

            // Check ImageData.source
            if let crate::node::NodeKind::Image(ref img_data) = node.kind {
                if let Some(ImageSource::Embedded { ref data }) = img_data.source {
                    if !data.is_empty() {
                        let ext = detect_image_ext(data);
                        let path = self.assets.add_image(data.clone(), &ext);
                        // Need mutable access — do in second pass
                    }
                }
            }
        }

        // Second pass: replace Embedded → Linked
        let node_ids: Vec<_> = self.document.nodes.iter().map(|(id, _)| id).collect();
        for node_id in node_ids {
            let node = &mut self.document.nodes[node_id];

            // ImageData.source
            if let crate::node::NodeKind::Image(ref mut img_data) = node.kind {
                if let Some(ImageSource::Embedded { ref data }) = img_data.source {
                    if !data.is_empty() {
                        let ext = detect_image_ext(data);
                        let path = self.assets.add_image(data.clone(), &ext);
                        img_data.source = Some(ImageSource::Linked { path });
                    }
                }
            }

            // Paint::ImageFill in fills — use NodeKind::visual_mut()
            // (defined at node.rs:384, returns Option<&mut VisualProps>)
            if let Some(visual) = node.kind.visual_mut() {
                extract_embedded_from_fills(&mut visual.fills, &mut self.assets);
            }
        }
    }

    fn find_asset_ext(&self, hash: &str) -> Option<String> {
        // Scan document for Linked paths containing this hash
        for (_, node) in self.document.nodes.iter() {
            if let crate::node::NodeKind::Image(ref img) = node.kind {
                if let Some(crate::style::ImageSource::Linked { ref path }) = img.source {
                    if path.contains(hash) {
                        return path.rsplit('.').next().map(|s| s.to_string());
                    }
                }
            }
        }
        None
    }
}

fn extract_embedded_from_fills(fills: &mut [crate::style::Fill], assets: &mut AssetStore) {
    use crate::style::{ImageSource, Paint};
    for fill in fills.iter_mut() {
        if let Paint::ImageFill {
            ref mut source,
            mode: _,
        } = fill.paint
        {
            if let ImageSource::Embedded { ref data } = source {
                if !data.is_empty() {
                    let ext = detect_image_ext(data);
                    let path = assets.add_image(data.clone(), &ext);
                    *source = ImageSource::Linked { path };
                }
            }
        }
    }
}

fn extract_embedded_from_strokes(strokes: &mut [crate::style::Stroke], _assets: &mut AssetStore) {
    // Strokes don't currently have ImageFill paint, but include for completeness
    let _ = strokes;
}

/// Detect image format from magic bytes.
fn detect_image_ext(data: &[u8]) -> String {
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "png".to_string()
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg".to_string()
    } else if data.starts_with(b"RIFF") && data.len() > 11 && &data[8..12] == b"WEBP" {
        "webp".to_string()
    } else if data.starts_with(b"GIF8") {
        "gif".to_string()
    } else if data.starts_with(b"<svg") || data.starts_with(b"<?xml") {
        "svg".to_string()
    } else if data.starts_with(b"BM") {
        "bmp".to_string()
    } else {
        "bin".to_string()
    }
}
```

- [ ] **Step 5: Add module to lib.rs**

In `crates/ode-format/src/lib.rs`, add:

```rust
pub mod container;
pub use container::{OdeContainer, OdeSource, ContainerError};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p ode-format container`
Expected: 5 tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/ode-format/src/container.rs crates/ode-format/src/lib.rs crates/ode-format/Cargo.toml
git commit -m "feat(ode-format): add OdeContainer with Unpacked and LegacyJson support"
```

---

## Task 4: `OdeContainer` — ZIP (Packed) support

**Files:**
- Modify: `crates/ode-format/Cargo.toml`
- Modify: `crates/ode-format/src/container.rs`

- [ ] **Step 1: Add zip dependency**

In `crates/ode-format/Cargo.toml`, add:

```toml
zip = { version = "2", default-features = false, features = ["deflate"] }
```

- [ ] **Step 2: Write the failing tests**

Add to `crates/ode-format/src/container.rs` tests:

```rust
#[test]
fn save_and_open_packed() {
    let dir = TempDir::new().unwrap();
    let ode_file = dir.path().join("design.ode");

    let doc = make_test_doc();
    let mut container = OdeContainer::from_document(doc, "ode-test");
    container.save_packed(&ode_file).unwrap();

    assert!(ode_file.exists());

    // Verify it's a valid ZIP
    let bytes = fs::read(&ode_file).unwrap();
    assert_eq!(&bytes[..4], &[0x50, 0x4B, 0x03, 0x04]);

    // Re-open
    let loaded = OdeContainer::open(ode_file.to_str().unwrap()).unwrap();
    assert_eq!(loaded.document.name, "Test");
    assert!(matches!(loaded.source, OdeSource::Packed(_)));
}

#[test]
fn detect_source_packed_ode() {
    let dir = TempDir::new().unwrap();
    let ode_file = dir.path().join("test.ode");

    let doc = make_test_doc();
    let mut container = OdeContainer::from_document(doc, "ode-test");
    container.save_packed(&ode_file).unwrap();

    let source = OdeSource::detect(ode_file.to_str().unwrap());
    assert!(matches!(source, OdeSource::Packed(_)));
}

#[test]
fn packed_atomic_write() {
    let dir = TempDir::new().unwrap();
    let ode_file = dir.path().join("design.ode");

    // First save
    let doc = make_test_doc();
    let mut c1 = OdeContainer::from_document(doc, "ode-test");
    c1.save_packed(&ode_file).unwrap();

    // Overwrite — should not corrupt on success
    let doc2 = Document::new("Updated");
    let mut c2 = OdeContainer::from_document(doc2, "ode-test");
    c2.save_packed(&ode_file).unwrap();

    let loaded = OdeContainer::open(ode_file.to_str().unwrap()).unwrap();
    assert_eq!(loaded.document.name, "Updated");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ode-format container::tests::save_and_open_packed`
Expected: panics at `todo!("ZIP support")`

- [ ] **Step 4: Implement open_packed and save_packed**

Replace the `todo!()` stubs in `container.rs`:

```rust
fn open_packed(file: &Path) -> Result<Self, ContainerError> {
    use std::io::Read;
    let reader = std::fs::File::open(file)?;
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| ContainerError::Zip(e.to_string()))?;

    // Read document.json
    let doc_json = {
        let mut f = archive
            .by_name("document.json")
            .map_err(|e| ContainerError::Zip(e.to_string()))?;
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        s
    };
    let document: Document = serde_json::from_str(&doc_json)?;

    // Read meta.json
    let meta = match archive.by_name("meta.json") {
        Ok(mut f) => {
            let mut s = String::new();
            f.read_to_string(&mut s)?;
            serde_json::from_str(&s)?
        }
        Err(_) => Meta::legacy(),
    };

    // Read assets
    let mut assets = AssetStore::new();
    let asset_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let name = archive.by_index(i).ok()?.name().to_string();
            if name.starts_with("assets/") && name.len() > "assets/".len() {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    for name in asset_names {
        let mut f = archive
            .by_name(&name)
            .map_err(|e| ContainerError::Zip(e.to_string()))?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        if let Some(filename) = name.strip_prefix("assets/") {
            if let Some(hash) = filename.split('.').next() {
                assets.add_image_with_hash(hash.to_string(), data);
            }
        }
    }

    Ok(Self {
        document,
        meta,
        assets,
        source: OdeSource::Packed(file.to_path_buf()),
    })
}

pub fn save_packed(&mut self, path: &Path) -> Result<(), ContainerError> {
    use std::io::Write;
    self.meta.touch();
    self.extract_embedded_assets();

    // Atomic write: write to temp file, then rename
    let temp_path = path.with_extension("ode.tmp");
    {
        let file = std::fs::File::create(&temp_path)?;
        let mut zip = zip::ZipWriter::new(file);
        let deflate =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let store =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        // document.json
        zip.start_file("document.json", deflate)
            .map_err(|e| ContainerError::Zip(e.to_string()))?;
        let doc_json = serde_json::to_string_pretty(&self.document)?;
        zip.write_all(doc_json.as_bytes())?;

        // meta.json
        zip.start_file("meta.json", deflate)
            .map_err(|e| ContainerError::Zip(e.to_string()))?;
        let meta_json = serde_json::to_string_pretty(&self.meta)?;
        zip.write_all(meta_json.as_bytes())?;

        // assets
        for (hash, entry) in self.assets.iter() {
            if let crate::asset::AssetEntry::Loaded(data) = entry {
                let ext = self.find_asset_ext(hash).unwrap_or("bin".to_string());
                let is_compressed = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif");
                let options = if is_compressed { store } else { deflate };
                zip.start_file(format!("assets/{hash}.{ext}"), options)
                    .map_err(|e| ContainerError::Zip(e.to_string()))?;
                zip.write_all(data)?;
            }
        }

        zip.finish().map_err(|e| ContainerError::Zip(e.to_string()))?;
    }

    // Atomic rename
    std::fs::rename(&temp_path, path)?;
    Ok(())
}
```

Also add to `AssetStore`:

```rust
/// Register image with a known hash (for loading from ZIP).
pub fn add_image_with_hash(&mut self, hash: String, data: Vec<u8>) {
    self.entries.insert(hash, AssetEntry::Loaded(data));
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p ode-format container`
Expected: all container tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ode-format/src/container.rs crates/ode-format/src/asset.rs crates/ode-format/Cargo.toml
git commit -m "feat(ode-format): add ZIP packed support to OdeContainer"
```

---

## Task 5: Update `ode-core` — AssetStore in render pipeline

**Files:**
- Modify: `crates/ode-core/src/convert.rs`

Note: `ode-core` already depends on `ode-format` — no Cargo.toml change needed.

- [ ] **Step 1: Write the failing test**

Add to existing tests in `crates/ode-core/src/convert.rs` (near line ~1993):

```rust
#[test]
fn emit_image_from_asset_store() {
    use ode_format::asset::AssetStore;

    let mut doc = Document::new("Test");
    let mut frame = Node::new_frame("Root", 200.0, 150.0);
    let mut img = Node::new_image("Photo", 100.0, 80.0);

    // Add image to asset store instead of embedding
    let mut assets = AssetStore::new();
    let png_bytes = minimal_png_bytes();
    let asset_path = assets.add_image(png_bytes.clone(), "png");

    if let NodeKind::Image(ref mut data) = img.kind {
        data.source = Some(ImageSource::Linked {
            path: asset_path,
        });
    }

    frame.children.push(doc.nodes.insert(img));
    let root = doc.nodes.insert(frame);
    doc.canvas.push(root);

    let font_db = FontDatabase::new_system();
    let scene = Scene::from_document(&doc, &font_db, &assets).unwrap();
    assert!(!scene.commands.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p ode-core emit_image_from_asset_store`
Expected: compilation error — `Scene::from_document` doesn't accept `&AssetStore`

- [ ] **Step 3: Update Scene::from_document signatures**

In `crates/ode-core/src/convert.rs`, update public API:

```rust
// Line ~21
pub fn from_document(
    doc: &Document,
    font_db: &FontDatabase,
    assets: &AssetStore,
) -> Result<Self, ConvertError>

// Line ~26
pub fn from_document_with_resize(
    doc: &Document,
    font_db: &FontDatabase,
    assets: &AssetStore,
    resize_map: &crate::layout::ResizeMap,
) -> Result<Self, ConvertError>
```

- [ ] **Step 4: Thread `assets` through all intermediate functions**

The `assets` parameter must be threaded through the call chain. Update these function signatures (all in `convert.rs`):

```rust
// convert_node — called recursively for each node
fn convert_node(
    doc: &Document,
    node_id: NodeId,
    parent_transform: tiny_skia::Transform,
    font_db: &FontDatabase,
    assets: &AssetStore,           // NEW
    commands: &mut Vec<RenderCommand>,
    layout_rects: &HashMap<NodeId, LayoutRect>,
)

// resolve_instance — if it exists and calls emit_image internally
// Thread `assets` parameter through it as well

// emit_image
fn emit_image(
    img_data: &ode_format::node::ImageData,
    current_transform: tiny_skia::Transform,
    commands: &mut Vec<RenderCommand>,
    layout_rect: Option<&crate::layout::LayoutRect>,
    assets: &AssetStore,           // NEW
)
```

Update ALL callsites of these functions to pass `assets`.

- [ ] **Step 5: Update emit_image image loading logic**

Replace the image byte resolution logic (~line 335). Use `get_loaded(&self)` which takes `&self` (no mutation needed — all assets are preloaded before rendering):

```rust
let image_bytes = match &img_data.source {
    Some(ode_format::style::ImageSource::Embedded { data }) => {
        if data.is_empty() {
            return;
        }
        data.clone()
    }
    Some(ode_format::style::ImageSource::Linked { path }) => {
        // Try asset store first (for "assets/..." paths)
        let hash = path
            .strip_prefix("assets/")
            .and_then(|f| f.split('.').next());
        if let Some(hash) = hash {
            match assets.get_loaded(hash) {
                Some(data) => data.to_vec(),
                None => return, // Not loaded — skip gracefully
            }
        } else {
            // External path — read from disk
            match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(_) => return,
            }
        }
    }
    None => return,
};
```

Note: `get_loaded(&self)` only returns already-loaded entries. The caller (CLI) must call `assets.preload_all()` before rendering. This avoids `&mut self` in the render hot path.

Note: `Paint::ImageFill` rendering is currently deferred in ode-core (line ~1112). When it is implemented, it will also need `AssetStore` access via the same pattern.

- [ ] **Step 6: Fix existing tests**

All existing tests that call `Scene::from_document` need an `&AssetStore::new()` argument added:

```rust
// Before:
let scene = Scene::from_document(&doc, &font_db).unwrap();
// After:
let scene = Scene::from_document(&doc, &font_db, &AssetStore::new()).unwrap();
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p ode-core`
Expected: all tests PASS

- [ ] **Step 8: Commit**

```bash
git add crates/ode-core/src/convert.rs
git commit -m "feat(ode-core): thread AssetStore through render pipeline"
```

---

## Task 6: Update `ode-cli` — use OdeContainer

**Files:**
- Modify: `crates/ode-cli/src/commands.rs`
- Modify: `crates/ode-cli/src/mutate.rs`
- Modify: `crates/ode-cli/src/main.rs`

- [ ] **Step 1: Update commands.rs render pipeline**

In `crates/ode-cli/src/commands.rs`, update `cmd_build` and `cmd_render` to use `OdeContainer`:

```rust
pub fn cmd_build(file: &str, output: &str, format: Option<&str>, resize: Option<&str>) -> i32 {
    let container = match OdeContainer::open(file) {
        Ok(c) => c,
        Err(e) => {
            print_json(&ErrorResponse::new("OPEN_FAILED", "io", &e.to_string()));
            return EXIT_IO;
        }
    };

    // Validate the document JSON
    let json = match serde_json::to_string(&container.document) {
        Ok(j) => j,
        Err(e) => {
            print_json(&ErrorResponse::new("INTERNAL", "serialize", &e.to_string()));
            return EXIT_INTERNAL;
        }
    };
    let validation = validate_json(&json);
    if !validation.valid {
        print_json(&ErrorResponse::validation(validation.errors));
        return EXIT_INPUT;
    }

    render_and_export(&container.document, &container.assets, output, format, validation.warnings, resize)
}
```

Preload assets before rendering, then pass to render pipeline:

```rust
pub fn cmd_build(file: &str, output: &str, format: Option<&str>, resize: Option<&str>) -> i32 {
    let mut container = match OdeContainer::open(file) { ... };

    // Preload all assets into memory for render pipeline
    if let Err(e) = container.assets.preload_all() {
        print_json(&ErrorResponse::new("ASSET_ERROR", "assets", &e.to_string()));
        return EXIT_IO;
    }

    // ... validate ...
    render_and_export(&container.document, &container.assets, output, format, warnings, resize)
}

fn render_and_export(
    doc: &Document,
    assets: &AssetStore,
    output: &str,
    format: Option<&str>,
    warnings: Vec<Warning>,
    resize: Option<&str>,
) -> i32 {
    // ... existing logic, but pass assets:
    // Scene::from_document(doc, &font_db, assets)
    // Scene::from_document_with_resize(doc, &font_db, assets, &resize_map)
}
```

Similarly update `cmd_render`, `cmd_inspect`, `cmd_validate`, `cmd_tokens_*`, `cmd_review`.

- [ ] **Step 2: Rewrite `cmd_new` to output .ode or directory**

```rust
pub fn cmd_new(file: &str, name: Option<&str>, width: Option<f32>, height: Option<f32>) -> i32 {
    let mut doc = Document::new(name.unwrap_or("Untitled"));

    if let (Some(w), Some(h)) = (width, height) {
        let frame = ode_format::node::Node::new_frame("Root", w, h);
        let id = doc.nodes.insert(frame);
        doc.canvas.push(id);
    }

    let mut container = OdeContainer::from_document(doc, "ode-cli");
    let path = Path::new(file);

    // Detect output format: directory or packed .ode
    let result = if file.ends_with('/') || path.is_dir() {
        container.save_unpacked(path)
    } else {
        container.save_packed(path)
    };

    match result {
        Ok(()) => {
            print_json(&OkResponse::with_path(file));
            EXIT_OK
        }
        Err(e) => {
            print_json(&ErrorResponse::new("IO_ERROR", "io", &e.to_string()));
            EXIT_IO
        }
    }
}
```

- [ ] **Step 3: Update mutate.rs for OdeContainer**

Mutation commands (`ode set`, `ode add`, `ode delete`, `ode move`) currently work at the `DocumentWire` level (raw JSON). The approach: keep using DocumentWire for the actual mutations, but wrap file I/O with OdeContainer-aware logic.

In `mutate.rs`, replace the `load_wire` / `save_wire` helpers:

```rust
fn load_wire(file: &str) -> Result<(String, DocumentWire), i32> {
    // Use OdeSource to detect format, but read raw JSON for wire-level ops
    let source = OdeSource::detect(file);
    let json = match source {
        OdeSource::Unpacked(ref dir) => {
            std::fs::read_to_string(dir.join("document.json"))
                .map_err(|_| EXIT_IO)?
        }
        OdeSource::Packed(ref path) => {
            // Extract document.json from ZIP
            let reader = std::fs::File::open(path).map_err(|_| EXIT_IO)?;
            let mut archive = zip::ZipArchive::new(reader).map_err(|_| EXIT_IO)?;
            let mut f = archive.by_name("document.json").map_err(|_| EXIT_IO)?;
            let mut s = String::new();
            std::io::Read::read_to_string(&mut f, &mut s).map_err(|_| EXIT_IO)?;
            s
        }
        _ => std::fs::read_to_string(file).map_err(|_| EXIT_IO)?,
    };
    let wire: DocumentWire = serde_json::from_str(&json).map_err(|_| EXIT_INPUT)?;
    Ok((json, wire))
}

fn save_wire(file: &str, wire: &DocumentWire) -> Result<(), i32> {
    let json = serde_json::to_string_pretty(wire).map_err(|_| EXIT_INTERNAL)?;
    let source = OdeSource::detect(file);
    match source {
        OdeSource::Unpacked(ref dir) => {
            std::fs::write(dir.join("document.json"), &json).map_err(|_| EXIT_IO)?;
        }
        OdeSource::Packed(ref path) => {
            // Read existing ZIP, replace document.json, write back atomically
            // (reuse OdeContainer for this — open, replace document, save)
            let mut container = OdeContainer::open(path).map_err(|_| EXIT_IO)?;
            container.document = serde_json::from_str(&json).map_err(|_| EXIT_INPUT)?;
            container.save_packed(path).map_err(|_| EXIT_IO)?;
        }
        _ => {
            std::fs::write(file, &json).map_err(|_| EXIT_IO)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Update main.rs help strings**

Replace all `.ode.json` references with `.ode`:

```rust
/// Create a new empty .ode document
/// Validate an .ode document
/// Input file (.ode) or - for stdin
/// Output .ode file path
```

- [ ] **Step 6: Run full CLI test suite**

Run: `cargo test -p ode-cli`
Expected: all tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/ode-cli/src/
git commit -m "feat(ode-cli): use OdeContainer for file I/O, update help strings"
```

---

## Task 7: CLI `pack` / `unpack` commands

**Files:**
- Modify: `crates/ode-cli/src/main.rs`
- Modify: `crates/ode-cli/src/commands.rs`

- [ ] **Step 1: Add Pack/Unpack to CLI**

In `main.rs`, add to `Command` enum:

```rust
/// Pack a directory into a .ode file
Pack {
    /// Input directory path
    input: String,
    /// Output .ode file (default: derived from input)
    #[arg(short, long)]
    output: Option<String>,
},
/// Unpack a .ode file into a directory
Unpack {
    /// Input .ode file
    input: String,
    /// Output directory (default: derived from input)
    #[arg(short, long)]
    output: Option<String>,
},
```

- [ ] **Step 2: Implement pack/unpack commands**

In `commands.rs`:

```rust
pub fn cmd_pack(input: &str, output: Option<&str>) -> i32 {
    let out_path = output
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let p = Path::new(input);
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            p.parent().unwrap_or(Path::new(".")).join(format!("{name}.ode"))
        });

    let mut container = match OdeContainer::open(input) {
        Ok(c) => c,
        Err(e) => {
            print_json(&ErrorResponse::new("OPEN_FAILED", "io", &e.to_string()));
            return EXIT_IO;
        }
    };

    if let Err(e) = container.save_packed(&out_path) {
        print_json(&ErrorResponse::new("PACK_FAILED", "io", &e.to_string()));
        return EXIT_PROCESS;
    }

    print_json(&OkResponse::with_path(out_path.to_str().unwrap_or("")));
    EXIT_OK
}

pub fn cmd_unpack(input: &str, output: Option<&str>) -> i32 {
    let out_path = output
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let p = Path::new(input);
            let stem = p.file_stem().unwrap_or_default().to_string_lossy();
            p.parent().unwrap_or(Path::new(".")).join(stem.as_ref())
        });

    let mut container = match OdeContainer::open(input) {
        Ok(c) => c,
        Err(e) => {
            print_json(&ErrorResponse::new("OPEN_FAILED", "io", &e.to_string()));
            return EXIT_IO;
        }
    };

    if let Err(e) = container.save_unpacked(&out_path) {
        print_json(&ErrorResponse::new("UNPACK_FAILED", "io", &e.to_string()));
        return EXIT_PROCESS;
    }

    print_json(&OkResponse::with_path(out_path.to_str().unwrap_or("")));
    EXIT_OK
}
```

- [ ] **Step 3: Wire up in main.rs**

```rust
Command::Pack { input, output } => commands::cmd_pack(&input, output.as_deref()),
Command::Unpack { input, output } => commands::cmd_unpack(&input, output.as_deref()),
```

- [ ] **Step 4: Write integration tests**

In `crates/ode-cli/tests/integration.rs`, add:

```rust
#[test]
fn pack_and_unpack_roundtrip() {
    let dir = tempfile::TempDir::new().unwrap();
    let ode_file = dir.path().join("test.ode");
    let unpacked = dir.path().join("test");

    // Create new design
    cmd(&["new", ode_file.to_str().unwrap(), "--width", "100", "--height", "100"]);

    // Unpack
    cmd(&["unpack", ode_file.to_str().unwrap(), "-o", unpacked.to_str().unwrap()]);
    assert!(unpacked.join("document.json").exists());
    assert!(unpacked.join("meta.json").exists());

    // Pack again
    let repacked = dir.path().join("repacked.ode");
    cmd(&["pack", unpacked.to_str().unwrap(), "-o", repacked.to_str().unwrap()]);
    assert!(repacked.exists());
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p ode-cli`
Expected: all tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ode-cli/src/main.rs crates/ode-cli/src/commands.rs crates/ode-cli/tests/
git commit -m "feat(ode-cli): add pack and unpack commands"
```

---

## Task 8: Update `ode-import` — Figma with AssetStore

**Files:**
- Modify: `crates/ode-import/src/figma/convert.rs:520-530`

- [ ] **Step 1: Update Figma converter to use AssetStore**

In `crates/ode-import/src/figma/convert.rs`, the `FigmaConverter` struct needs an `AssetStore` field. Update the conversion to register images in the store:

```rust
// In try_promote_to_image (~line 522)
let source = fill.image_ref.as_ref().map(|image_ref| {
    if let Some(data) = self.images.get(image_ref) {
        // Register in asset store and return Linked reference
        let ext = detect_image_ext(data);
        let path = self.asset_store.add_image(data.clone(), &ext);
        ImageSource::Linked { path }
    } else {
        ImageSource::Linked {
            path: image_ref.clone(),
        }
    }
});
```

- [ ] **Step 2: Update FigmaConverter::convert() to return AssetStore**

The `ConvertResult` should include the `AssetStore` so the CLI can package it:

```rust
pub struct ConvertResult {
    pub document: Document,
    pub warnings: Vec<ConvertWarning>,
    pub asset_store: AssetStore,
}
```

- [ ] **Step 3: Update CLI cmd_import_figma**

Use the returned `AssetStore` when creating the `OdeContainer`:

```rust
let mut container = OdeContainer::from_document(result.document, "ode-cli");
container.assets = result.asset_store;
container.save_packed(&output_path)?; // or save_unpacked
```

- [ ] **Step 4: Run import tests**

Run: `cargo test -p ode-import`
Expected: all tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ode-import/src/figma/convert.rs crates/ode-cli/src/commands.rs
git commit -m "feat(ode-import): use AssetStore for Figma image handling"
```

---

## Task 9: Update tests — `.ode.json` → `.ode`

**Files:**
- Modify: `crates/ode-cli/tests/integration.rs`
- Modify: `crates/ode-cli/tests/set_test.rs`
- Modify: `crates/ode-cli/tests/delete_test.rs`
- Modify: `crates/ode-cli/tests/add_test.rs`
- Modify: `crates/ode-cli/tests/workflow_test.rs`
- Modify: `crates/ode-cli/tests/move_test.rs` (if exists)

- [ ] **Step 1: Find and replace all `.ode.json` in test files**

In each test file, replace:
- `"test.ode.json"` → `"test.ode.json"` (keep as legacy JSON for backward compat tests)
- Add new test variants using `.ode` and unpacked directories

- [ ] **Step 2: Add legacy + packed + unpacked test variants for key workflows**

```rust
#[test]
fn build_from_packed_ode() { ... }
#[test]
fn build_from_unpacked_dir() { ... }
#[test]
fn build_from_legacy_json() { ... }
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/ode-cli/tests/
git commit -m "test(ode-cli): add packed, unpacked, and legacy format test variants"
```

---

## Task 10: Final integration test — end-to-end

**Files:**
- Modify: `crates/ode-cli/tests/integration.rs`

- [ ] **Step 1: Write end-to-end workflow test**

```rust
#[test]
fn full_workflow_create_edit_pack_unpack_render() {
    let dir = tempfile::TempDir::new().unwrap();
    let unpacked = dir.path().join("design");

    // 1. Create unpacked
    cmd(&["new", unpacked.to_str().unwrap(), "--width", "800", "--height", "600"]);
    assert!(unpacked.join("document.json").exists());

    // 2. Add a frame
    cmd(&["add", "frame", unpacked.to_str().unwrap(),
        "--name", "Card", "--width", "400", "--height", "300", "--fill", "#336699"]);

    // 3. Render from unpacked
    let png = dir.path().join("preview.png");
    cmd(&["build", unpacked.to_str().unwrap(), "-o", png.to_str().unwrap()]);
    assert!(png.exists());

    // 4. Pack
    let ode_file = dir.path().join("design.ode");
    cmd(&["pack", unpacked.to_str().unwrap(), "-o", ode_file.to_str().unwrap()]);
    assert!(ode_file.exists());

    // 5. Render from packed
    let png2 = dir.path().join("preview2.png");
    cmd(&["build", ode_file.to_str().unwrap(), "-o", png2.to_str().unwrap()]);
    assert!(png2.exists());

    // 6. Unpack to new location
    let unpacked2 = dir.path().join("design2");
    cmd(&["unpack", ode_file.to_str().unwrap(), "-o", unpacked2.to_str().unwrap()]);
    assert!(unpacked2.join("document.json").exists());
    assert!(unpacked2.join("meta.json").exists());
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p ode-cli full_workflow`
Expected: PASS

- [ ] **Step 3: Run complete workspace tests**

Run: `cargo test --workspace`
Expected: all tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/ode-cli/tests/integration.rs
git commit -m "test: add end-to-end .ode workflow integration test"
```
