use crate::TextError;
use skrifa::instance::Size;
use skrifa::outline::OutlinePen;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use std::collections::HashMap;

/// Pen that converts skrifa outline commands to kurbo::BezPath.
struct KurboPen {
    path: kurbo::BezPath,
}

impl KurboPen {
    fn new() -> Self {
        Self {
            path: kurbo::BezPath::new(),
        }
    }
}

impl OutlinePen for KurboPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to((x as f64, y as f64));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to((x as f64, y as f64));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.path
            .quad_to((cx0 as f64, cy0 as f64), (x as f64, y as f64));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path.curve_to(
            (cx0 as f64, cy0 as f64),
            (cx1 as f64, cy1 as f64),
            (x as f64, y as f64),
        );
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

/// Extract a glyph outline as a kurbo::BezPath at the given font size.
///
/// Returns `Ok(None)` for glyphs with no outline (e.g., space characters).
pub fn get_glyph_outline(
    font_data: &[u8],
    glyph_id: u16,
    font_size: f32,
) -> Result<Option<kurbo::BezPath>, TextError> {
    let font = FontRef::new(font_data).map_err(|e| TextError::FontParseFailed(format!("{e}")))?;

    let outlines = font.outline_glyphs();
    let glyph = match outlines.get(GlyphId::new(glyph_id as u32)) {
        Some(g) => g,
        None => return Ok(None),
    };

    let mut pen = KurboPen::new();
    let size = Size::new(font_size);
    let settings =
        skrifa::outline::DrawSettings::unhinted(size, skrifa::instance::LocationRef::default());

    match glyph.draw(settings, &mut pen) {
        Ok(_) => {
            if pen.path.elements().is_empty() {
                Ok(None)
            } else {
                Ok(Some(pen.path))
            }
        }
        Err(_) => Ok(None),
    }
}

/// Cached glyph outline extractor.
///
/// Caches glyph outlines by (glyph_id, font_size_bits) to avoid
/// redundant outline extraction for repeated glyphs.
pub struct GlyphCache {
    cache: HashMap<(u16, u32), Option<kurbo::BezPath>>,
}

impl GlyphCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn get_outline(
        &mut self,
        font_data: &[u8],
        glyph_id: u16,
        font_size: f32,
    ) -> Result<Option<kurbo::BezPath>, TextError> {
        let key = (glyph_id, font_size.to_bits());
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached.clone());
        }

        let result = get_glyph_outline(font_data, glyph_id, font_size)?;
        self.cache.insert(key, result.clone());
        Ok(result)
    }
}

impl Default for GlyphCache {
    fn default() -> Self {
        Self::new()
    }
}
