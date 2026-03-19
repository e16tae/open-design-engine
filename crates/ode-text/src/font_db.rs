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

/// CJK font families to try first when looking for fallback fonts.
/// Ordered by platform prevalence: macOS → cross-platform → other.
const CJK_FALLBACK_FAMILIES: &[&str] = &[
    "applesdgothicneo",
    "apple sd gothic neo",
    "noto sans cjk kr",
    "pingfang sc",
    "hiragino sans",
    "noto sans cjk sc",
    "noto sans cjk jp",
    "malgun gothic",
    "microsoft yahei",
];

/// Check whether a codepoint falls in a CJK range (Hangul, CJK Unified, etc.).
fn is_cjk_codepoint(ch: char) -> bool {
    let c = ch as u32;
    // Hangul Jamo
    (0x1100..=0x11FF).contains(&c)
    // CJK Radicals Supplement, Kangxi Radicals
    || (0x2E80..=0x2FFF).contains(&c)
    // CJK Symbols and Punctuation, Hiragana, Katakana, Bopomofo, Hangul Compat Jamo
    || (0x3000..=0x31FF).contains(&c)
    // CJK Unified Ideographs Extension A
    || (0x3400..=0x4DBF).contains(&c)
    // CJK Unified Ideographs
    || (0x4E00..=0x9FFF).contains(&c)
    // Hangul Syllables
    || (0xAC00..=0xD7AF).contains(&c)
    // Hangul Jamo Extended-A/B
    || (0xA960..=0xA97F).contains(&c)
    || (0xD7B0..=0xD7FF).contains(&c)
    // CJK Compatibility Ideographs
    || (0xF900..=0xFAFF).contains(&c)
    // Halfwidth and Fullwidth Forms
    || (0xFF00..=0xFFEF).contains(&c)
    // CJK Unified Ideographs Extension B-F
    || (0x20000..=0x2FA1F).contains(&c)
}

/// Check whether a font's cmap contains a glyph for the given character.
pub fn font_has_char(font_data: &[u8], ch: char) -> bool {
    use skrifa::MetadataProvider;
    let Ok(font) = FontRef::new(font_data) else {
        return false;
    };
    let charmap = font.charmap();
    charmap.map(ch).is_some()
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

    /// Find a fallback font that can render the given character.
    ///
    /// For CJK codepoints, tries well-known CJK families first (AppleSDGothicNeo, etc.)
    /// then falls back to scanning all loaded fonts.
    pub fn find_fallback_for_char(&self, ch: char, weight: u16) -> Option<Arc<Vec<u8>>> {
        // For CJK characters, prioritize known CJK families
        if is_cjk_codepoint(ch) {
            for &family in CJK_FALLBACK_FAMILIES {
                if let Some(indices) = self.family_index.get(family) {
                    let data = find_closest_weight(&self.fonts, indices, weight);
                    if font_has_char(&data, ch) {
                        return Some(data);
                    }
                }
            }
        }

        // Scan all loaded fonts
        for entry in &self.fonts {
            if font_has_char(&entry.data, ch) {
                return Some(Arc::clone(&entry.data));
            }
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

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());

            if let Some("ttf" | "otf" | "ttc") = ext.as_deref() {
                if let Ok(data) = std::fs::read(&path) {
                    // For TTC (font collections), we only load the first face
                    self.add_font(data);
                }
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

    let family = font
        .localized_strings(StringId::FAMILY_NAME)
        .find_map(|s| {
            let chars: String = s.chars().collect();
            if !chars.is_empty() { Some(chars) } else { None }
        })
        .unwrap_or_else(|| "Unknown".to_string());

    // Extract weight from OS/2 table
    use skrifa::raw::TableProvider;
    let weight = font
        .os2()
        .ok()
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
            assert!(
                !db.is_empty(),
                "System font database should not be empty on macOS"
            );
        }
    }

    #[test]
    fn is_cjk_detects_hangul() {
        assert!(is_cjk_codepoint('가')); // U+AC00 Hangul Syllables
        assert!(is_cjk_codepoint('힣')); // U+D7A3
        assert!(is_cjk_codepoint('中')); // U+4E2D CJK Unified
        assert!(!is_cjk_codepoint('A'));
        assert!(!is_cjk_codepoint(' '));
    }

    #[test]
    fn font_has_char_ascii() {
        let db = FontDatabase::new_system();
        if db.is_empty() {
            return;
        }
        // Any system font should have 'A'
        let font = db.find_font("Helvetica", 400).unwrap();
        assert!(font_has_char(&font, 'A'));
    }

    #[test]
    fn font_has_char_hangul_in_helvetica() {
        let db = FontDatabase::new_system();
        if db.is_empty() {
            return;
        }
        if let Some(font) = db.find_font("Helvetica", 400) {
            // Helvetica should NOT have Korean glyphs
            assert!(
                !font_has_char(&font, '가'),
                "Helvetica should not have Hangul glyphs"
            );
        }
    }

    #[test]
    fn find_fallback_for_hangul() {
        let db = FontDatabase::new_system();
        if db.is_empty() {
            return;
        }
        // On macOS, there should be a CJK fallback font
        if cfg!(target_os = "macos") {
            let fallback = db.find_fallback_for_char('가', 400);
            assert!(
                fallback.is_some(),
                "Should find a fallback font for Hangul on macOS"
            );
            // Verify the fallback actually has the character
            if let Some(ref data) = fallback {
                assert!(font_has_char(data, '가'));
            }
        }
    }
}
