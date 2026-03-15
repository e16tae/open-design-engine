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
}
