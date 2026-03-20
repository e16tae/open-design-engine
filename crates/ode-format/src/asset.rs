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
    /// hash -> entry
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

    /// Register image with a known hash (for loading from ZIP).
    pub fn add_image_with_hash(&mut self, hash: String, data: Vec<u8>) {
        self.entries.insert(hash, AssetEntry::Loaded(data));
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
        let hash = AssetStore::compute_hash(&[1, 2, 3]);
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn on_disk_lazy_loads_via_get_image() {
        let data = b"fake-png-data-for-test";
        let mut tmp = tempfile::NamedTempFile::new().expect("create temp file");
        std::io::Write::write_all(&mut tmp, data).expect("write temp file");

        let hash = AssetStore::compute_hash(data);
        let mut store = AssetStore::new();
        store.register_on_disk(hash.clone(), tmp.path().to_path_buf());

        // Before get_image, get_loaded should return None (still OnDisk)
        assert!(store.get_loaded(&hash).is_none());

        // get_image should lazy-load from disk
        let loaded = store.get_image(&hash).expect("lazy load should succeed");
        assert_eq!(loaded, data);

        // After lazy load, get_loaded should now succeed
        assert_eq!(store.get_loaded(&hash).unwrap(), data);
    }

    #[test]
    fn preload_all_converts_on_disk_to_loaded() {
        let data_a = b"image-alpha";
        let data_b = b"image-bravo";

        let mut tmp_a = tempfile::NamedTempFile::new().expect("create temp a");
        std::io::Write::write_all(&mut tmp_a, data_a).expect("write a");
        let mut tmp_b = tempfile::NamedTempFile::new().expect("create temp b");
        std::io::Write::write_all(&mut tmp_b, data_b).expect("write b");

        let hash_a = AssetStore::compute_hash(data_a);
        let hash_b = AssetStore::compute_hash(data_b);

        let mut store = AssetStore::new();
        store.register_on_disk(hash_a.clone(), tmp_a.path().to_path_buf());
        store.register_on_disk(hash_b.clone(), tmp_b.path().to_path_buf());

        // Both should be None via get_loaded before preload
        assert!(store.get_loaded(&hash_a).is_none());
        assert!(store.get_loaded(&hash_b).is_none());

        store.preload_all().expect("preload should succeed");

        // After preload, both should be accessible via get_loaded
        assert_eq!(store.get_loaded(&hash_a).unwrap(), data_a);
        assert_eq!(store.get_loaded(&hash_b).unwrap(), data_b);
    }
}
