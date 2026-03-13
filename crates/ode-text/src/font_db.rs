use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use skrifa::FontRef;

/// Database of loaded fonts for text rendering.
///
/// Stores font data keyed by family name and weight.
/// Supports both system font scanning and explicit font addition.
pub struct FontDatabase {
    fonts: Vec<FontEntry>,
    /// family name (lowercase) → indices into `fonts`
    family_index: HashMap<String, Vec<usize>>,
}

struct FontEntry {
    #[allow(dead_code)] // stored for debugging; lookup goes through family_index
    family: String,
    weight: u16,
    data: Arc<Vec<u8>>,
}

impl FontDatabase {
    /// Create an empty font database.
    pub fn new() -> Self {
        Self {
            fonts: Vec::new(),
            family_index: HashMap::new(),
        }
    }

    /// Create a font database pre-loaded with system fonts.
    pub fn new_system() -> Self {
        let mut db = Self::new();
        db.scan_system_fonts();
        db
    }

    /// Add a font from raw data. Returns true if successfully parsed and added.
    pub fn add_font(&mut self, data: Vec<u8>) -> bool {
        let (family, weight) = match extract_font_metadata(&data) {
            Some(meta) => meta,
            None => return false,
        };

        let idx = self.fonts.len();
        let family_lower = family.to_lowercase();
        self.fonts.push(FontEntry {
            family,
            weight,
            data: Arc::new(data),
        });
        self.family_index.entry(family_lower).or_default().push(idx);
        true
    }

    /// Find the best matching font for a given family and weight.
    ///
    /// Uses CSS font matching: finds exact family match, then closest weight.
    /// Falls back to any available font if no family match.
    pub fn find_font(&self, family: &str, weight: u16) -> Option<Arc<Vec<u8>>> {
        let family_lower = family.to_lowercase();

        // Try exact family match
        if let Some(indices) = self.family_index.get(&family_lower) {
            return Some(find_closest_weight(&self.fonts, indices, weight));
        }

        // Try common fallback families
        for fallback in &["arial", "helvetica", "sans-serif", "system font"] {
            if let Some(indices) = self.family_index.get(*fallback) {
                return Some(find_closest_weight(&self.fonts, indices, weight));
            }
        }

        // Fall back to any available font
        if let Some(entry) = self.fonts.first() {
            return Some(Arc::clone(&entry.data));
        }

        None
    }

    /// Returns the number of loaded fonts.
    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }

    /// Returns whether the database is empty.
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    fn scan_system_fonts(&mut self) {
        let mut dirs = vec![
            "/System/Library/Fonts".to_string(),
            "/Library/Fonts".to_string(),
        ];

        if let Some(home) = std::env::var_os("HOME") {
            let home_fonts = format!("{}/Library/Fonts", home.to_string_lossy());
            dirs.push(home_fonts);
        }

        // Linux paths
        dirs.push("/usr/share/fonts".to_string());
        dirs.push("/usr/local/share/fonts".to_string());

        for dir in &dirs {
            self.scan_font_dir(Path::new(dir));
        }
    }

    fn scan_font_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_font_dir(&path);
                continue;
            }

            let ext = path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());

            match ext.as_deref() {
                Some("ttf" | "otf" | "ttc") => {
                    if let Ok(data) = std::fs::read(&path) {
                        // For TTC (font collections), we only load the first face
                        self.add_font(data);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Default for FontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Find the font with the closest weight from a set of indices.
fn find_closest_weight(fonts: &[FontEntry], indices: &[usize], target: u16) -> Arc<Vec<u8>> {
    let mut best_idx = indices[0];
    let mut best_diff = (fonts[best_idx].weight as i32 - target as i32).unsigned_abs();

    for &idx in &indices[1..] {
        let diff = (fonts[idx].weight as i32 - target as i32).unsigned_abs();
        if diff < best_diff {
            best_diff = diff;
            best_idx = idx;
        }
    }

    Arc::clone(&fonts[best_idx].data)
}

/// Extract font family name and weight from raw font data using skrifa.
fn extract_font_metadata(data: &[u8]) -> Option<(String, u16)> {
    let font = FontRef::new(data).ok()?;

    // Extract family name from the name table
    use skrifa::MetadataProvider;
    use skrifa::string::StringId;

    let family = font.localized_strings(StringId::FAMILY_NAME)
        .into_iter()
        .find_map(|s| {
            let chars: String = s.chars().collect();
            if !chars.is_empty() { Some(chars) } else { None }
        })
        .unwrap_or_else(|| "Unknown".to_string());

    // Extract weight from OS/2 table
    use skrifa::raw::TableProvider;
    let weight = font.os2().ok()
        .map(|os2| os2.us_weight_class())
        .unwrap_or(400);

    Some((family, weight))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_db_returns_none() {
        let db = FontDatabase::new();
        assert!(db.find_font("Arial", 400).is_none());
        assert!(db.is_empty());
    }

    #[test]
    fn system_db_loads_fonts() {
        let db = FontDatabase::new_system();
        // On macOS, system fonts should be available
        if cfg!(target_os = "macos") {
            assert!(!db.is_empty(), "System font database should not be empty on macOS");
        }
    }
}
