use std::sync::Arc;

use crate::font_db::{font_has_char, FontDatabase};
use crate::TextError;
use ode_format::typography::TextTransform;

/// A shaped glyph with positioning information.
pub struct ShapedGlyph {
    /// Glyph ID in the font.
    pub glyph_id: u16,
    /// Byte offset into the original text (cluster).
    pub cluster: usize,
    /// X advance in pixels (scaled to font_size).
    pub x_advance: f32,
    /// Y advance in pixels (usually 0 for horizontal text).
    pub y_advance: f32,
    /// X offset from the pen position in pixels.
    pub x_offset: f32,
    /// Y offset from the pen position in pixels.
    pub y_offset: f32,
}

/// Result of shaping a text string.
pub struct ShapedText {
    pub glyphs: Vec<ShapedGlyph>,
    /// Font units per em, needed for coordinate conversion.
    pub units_per_em: u16,
    /// Font ascent in pixels (positive, above baseline).
    pub ascent: f32,
    /// Font descent in pixels (negative, below baseline).
    pub descent: f32,
}

/// Shape text using rustybuzz and return positioned glyphs.
pub fn shape_text(
    text: &str,
    font_data: &[u8],
    font_size: f32,
    letter_spacing: f32,
    transform: TextTransform,
) -> Result<ShapedText, TextError> {
    // Apply text transform
    let transformed: String;
    let text = match transform {
        TextTransform::None => text,
        TextTransform::Uppercase => {
            transformed = text.to_uppercase();
            &transformed
        }
        TextTransform::Lowercase => {
            transformed = text.to_lowercase();
            &transformed
        }
        TextTransform::Capitalize => {
            transformed = capitalize_words(text);
            &transformed
        }
    };

    // Create rustybuzz face
    let face = rustybuzz::Face::from_slice(font_data, 0)
        .ok_or_else(|| TextError::FontParseFailed("rustybuzz failed to parse font".into()))?;

    let units_per_em = face.units_per_em() as u16;
    let scale = font_size / units_per_em as f32;

    // Get font metrics
    let ascent = face.ascender() as f32 * scale;
    let descent = face.descender() as f32 * scale;

    // Create and fill buffer
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);

    // Shape
    let output = rustybuzz::shape(&face, &[], buffer);

    // Convert to ShapedGlyphs
    let infos = output.glyph_infos();
    let positions = output.glyph_positions();

    let glyphs: Vec<ShapedGlyph> = infos
        .iter()
        .zip(positions.iter())
        .map(|(info, pos)| ShapedGlyph {
            glyph_id: info.glyph_id as u16,
            cluster: info.cluster as usize,
            x_advance: pos.x_advance as f32 * scale + letter_spacing,
            y_advance: pos.y_advance as f32 * scale,
            x_offset: pos.x_offset as f32 * scale,
            y_offset: pos.y_offset as f32 * scale,
        })
        .collect();

    Ok(ShapedText {
        glyphs,
        units_per_em,
        ascent,
        descent,
    })
}

/// A contiguous run of text that uses the same font.
pub struct FontRun {
    /// Byte start offset in the original text.
    pub byte_start: usize,
    /// Byte end offset in the original text.
    pub byte_end: usize,
    /// Font data for this run.
    pub font_data: Arc<Vec<u8>>,
    /// Whether this run uses a fallback font (not the primary).
    pub is_fallback: bool,
}

/// Shape text with automatic font fallback for characters not covered by the primary font.
///
/// Partitions the text into runs based on primary font coverage, finds fallback
/// fonts for unsupported characters (e.g. CJK), shapes each run independently,
/// and merges the results into a single `ShapedText`.
pub fn shape_text_with_fallback(
    text: &str,
    primary_font_data: &Arc<Vec<u8>>,
    font_db: &FontDatabase,
    font_size: f32,
    letter_spacing: f32,
    transform: TextTransform,
    weight: u16,
) -> Result<(ShapedText, Vec<FontRun>), TextError> {
    // Apply text transform first
    let transformed: String;
    let text = match transform {
        TextTransform::None => text,
        TextTransform::Uppercase => {
            transformed = text.to_uppercase();
            &transformed
        }
        TextTransform::Lowercase => {
            transformed = text.to_lowercase();
            &transformed
        }
        TextTransform::Capitalize => {
            transformed = capitalize_words(text);
            &transformed
        }
    };

    // Partition text into font runs based on primary font coverage
    let font_runs = partition_by_font_coverage(text, primary_font_data, font_db, weight);

    // If there's only one run using the primary font, use the fast path
    if font_runs.len() == 1 && !font_runs[0].is_fallback {
        let shaped = shape_text(text, primary_font_data, font_size, letter_spacing, TextTransform::None)?;
        return Ok((shaped, font_runs));
    }

    // Shape each run independently and merge
    let mut all_glyphs: Vec<ShapedGlyph> = Vec::new();
    let mut primary_ascent = 0.0f32;
    let mut primary_descent = 0.0f32;
    let mut primary_upem = 1000u16;

    // Get primary font metrics for consistent line height
    if let Some(face) = rustybuzz::Face::from_slice(primary_font_data, 0) {
        let upem = face.units_per_em() as u16;
        let scale = font_size / upem as f32;
        primary_ascent = face.ascender() as f32 * scale;
        primary_descent = face.descender() as f32 * scale;
        primary_upem = upem;
    }

    for run in &font_runs {
        let run_text = &text[run.byte_start..run.byte_end];
        if run_text.is_empty() {
            continue;
        }

        let shaped_run = shape_text(
            run_text,
            &run.font_data,
            font_size,
            letter_spacing,
            TextTransform::None, // already transformed above
        )?;

        // Use primary font metrics if this is the first run, otherwise keep consistent
        if !run.is_fallback && primary_ascent == 0.0 {
            primary_ascent = shaped_run.ascent;
            primary_descent = shaped_run.descent;
            primary_upem = shaped_run.units_per_em;
        }

        // Remap cluster offsets back to the original text byte offsets
        for mut glyph in shaped_run.glyphs {
            glyph.cluster += run.byte_start;
            all_glyphs.push(glyph);
        }
    }

    // If we never got metrics from the primary font (e.g. all fallback),
    // use the first run's metrics
    if primary_ascent == 0.0 {
        if let Some(run) = font_runs.first() {
            if let Some(face) = rustybuzz::Face::from_slice(&run.font_data, 0) {
                let upem = face.units_per_em() as u16;
                let scale = font_size / upem as f32;
                primary_ascent = face.ascender() as f32 * scale;
                primary_descent = face.descender() as f32 * scale;
                primary_upem = upem;
            }
        }
    }

    Ok((
        ShapedText {
            glyphs: all_glyphs,
            units_per_em: primary_upem,
            ascent: primary_ascent,
            descent: primary_descent,
        },
        font_runs,
    ))
}

/// Partition text into contiguous runs based on whether the primary font supports each character.
///
/// Characters that the primary font can't render are grouped and assigned a fallback font.
/// Adjacent characters using the same font are merged into a single run.
fn partition_by_font_coverage(
    text: &str,
    primary_font_data: &Arc<Vec<u8>>,
    font_db: &FontDatabase,
    weight: u16,
) -> Vec<FontRun> {
    if text.is_empty() {
        return vec![FontRun {
            byte_start: 0,
            byte_end: 0,
            font_data: Arc::clone(primary_font_data),
            is_fallback: false,
        }];
    }

    // Parse primary font charmap once — avoids re-parsing on every character.
    let primary_font_ref = skrifa::FontRef::new(primary_font_data).ok();
    let primary_charmap = primary_font_ref.as_ref().map(|f| {
        use skrifa::MetadataProvider;
        f.charmap()
    });

    let mut runs: Vec<FontRun> = Vec::new();
    // Cache: fallback font data for each char (avoid repeated lookups)
    let mut fallback_cache: std::collections::HashMap<char, Option<Arc<Vec<u8>>>> =
        std::collections::HashMap::new();

    for (byte_idx, ch) in text.char_indices() {
        let char_len = ch.len_utf8();
        let byte_end = byte_idx + char_len;

        // Whitespace and newlines always use primary font.
        // Use the pre-parsed charmap instead of re-parsing per character.
        let primary_has = primary_charmap
            .as_ref()
            .map_or(false, |cm| cm.map(ch).is_some());
        let (font_data, is_fallback) = if ch.is_whitespace() || primary_has {
            (Arc::clone(primary_font_data), false)
        } else {
            // Look up fallback
            let fallback = fallback_cache
                .entry(ch)
                .or_insert_with(|| font_db.find_fallback_for_char(ch, weight))
                .clone();

            match fallback {
                Some(fb) => (fb, true),
                None => {
                    // No fallback found — use primary (will render .notdef, same as before)
                    (Arc::clone(primary_font_data), false)
                }
            }
        };

        // Try to extend the previous run if same font
        let same_font = runs.last().map_or(false, |prev: &FontRun| {
            Arc::ptr_eq(&prev.font_data, &font_data) && prev.is_fallback == is_fallback
        });

        if same_font {
            runs.last_mut().unwrap().byte_end = byte_end;
        } else {
            runs.push(FontRun {
                byte_start: byte_idx,
                byte_end,
                font_data,
                is_fallback,
            });
        }
    }

    runs
}

fn capitalize_words(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut capitalize_next = true;

    for ch in text.chars() {
        if capitalize_next && ch.is_alphabetic() {
            for upper in ch.to_uppercase() {
                result.push(upper);
            }
            capitalize_next = false;
        } else {
            result.push(ch);
            if ch.is_whitespace() {
                capitalize_next = true;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalize_words_basic() {
        assert_eq!(capitalize_words("hello world"), "Hello World");
        assert_eq!(capitalize_words("HELLO"), "HELLO");
        assert_eq!(capitalize_words(""), "");
        assert_eq!(capitalize_words("a b c"), "A B C");
    }

    #[test]
    fn partition_ascii_only_is_single_primary_run() {
        let db = FontDatabase::new_system();
        if db.is_empty() {
            return;
        }
        let font = db.find_font("Helvetica", 400).unwrap();
        let runs = partition_by_font_coverage("Hello world", &font, &db, 400);
        assert_eq!(runs.len(), 1, "Pure ASCII should be a single run");
        assert!(!runs[0].is_fallback);
    }

    #[test]
    fn partition_mixed_splits_cjk() {
        let db = FontDatabase::new_system();
        if db.is_empty() || !cfg!(target_os = "macos") {
            return;
        }
        let font = db.find_font("Helvetica", 400).unwrap();
        let runs = partition_by_font_coverage("Hello 세계", &font, &db, 400);
        // Should have at least 2 runs: "Hello " (primary) + "세계" (fallback)
        assert!(
            runs.len() >= 2,
            "Mixed text should split into at least 2 runs, got {}",
            runs.len()
        );
        assert!(!runs[0].is_fallback, "First run (ASCII) should be primary");
        // Find the fallback run
        let has_fallback = runs.iter().any(|r| r.is_fallback);
        assert!(has_fallback, "Should have a fallback run for Korean");
    }

    #[test]
    fn shape_with_fallback_ascii_only() {
        let db = FontDatabase::new_system();
        if db.is_empty() {
            return;
        }
        let font = db.find_font("Helvetica", 400).unwrap();
        let (shaped, runs) = shape_text_with_fallback(
            "Hello",
            &font,
            &db,
            16.0,
            0.0,
            TextTransform::None,
            400,
        )
        .unwrap();
        assert_eq!(runs.len(), 1);
        assert!(!runs[0].is_fallback);
        assert!(!shaped.glyphs.is_empty());
    }

    #[test]
    fn shape_with_fallback_mixed_text() {
        let db = FontDatabase::new_system();
        if db.is_empty() || !cfg!(target_os = "macos") {
            return;
        }
        let font = db.find_font("Helvetica", 400).unwrap();
        let (shaped, _runs) = shape_text_with_fallback(
            "Hello 세계",
            &font,
            &db,
            16.0,
            0.0,
            TextTransform::None,
            400,
        )
        .unwrap();
        // All characters should produce glyphs (including Korean)
        assert!(
            shaped.glyphs.len() >= 7,
            "Expected at least 7 glyphs (H,e,l,l,o,space + Korean), got {}",
            shaped.glyphs.len()
        );
    }
}
