pub mod font_db;
pub mod glyph;
pub mod layout;
pub mod shaper;

pub use font_db::FontDatabase;

use ode_format::node::TextData;
use ode_format::style::Fill;
use ode_format::typography::{TextSizingMode, TextStyle};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TextError {
    #[error("no font found for family '{family}' weight {weight}")]
    FontNotFound { family: String, weight: u16 },
    #[error("font parsing failed: {0}")]
    FontParseFailed(String),
    #[error("shaping failed: {0}")]
    ShapingFailed(String),
}

/// A fully processed text node ready for rendering.
pub struct ProcessedText {
    /// Positioned glyph outlines, each translated to its final position.
    pub glyphs: Vec<PositionedGlyph>,
    /// Decoration lines (underline, strikethrough) as thin rectangles.
    pub decorations: Vec<DecorationRect>,
    /// Computed width of the text content (may differ from TextData.width for auto-sizing).
    pub computed_width: f32,
    /// Computed height of the text content.
    pub computed_height: f32,
}

/// A single glyph outline positioned within the text node's coordinate space.
pub struct PositionedGlyph {
    /// The glyph outline path, already translated to its final position.
    pub path: kurbo::BezPath,
    /// Index into the resolved run list (for determining fill color).
    pub run_index: usize,
}

/// A decoration line (underline or strikethrough) as a thin rectangle.
pub struct DecorationRect {
    /// Rectangle path for the decoration line.
    pub path: kurbo::BezPath,
    /// Index into the resolved run list (for determining fill color).
    pub run_index: usize,
}

/// Resolved style for a run, with all Option fields resolved against the default style.
pub struct ResolvedRunStyle {
    pub fills: Vec<Fill>,
}

/// Main entry point: process a TextData node into positioned glyph outlines.
pub fn process_text(data: &TextData, font_db: &FontDatabase) -> Result<ProcessedText, TextError> {
    let default_style = &data.default_style;

    // If content is empty, return empty result
    if data.content.is_empty() {
        return Ok(ProcessedText {
            glyphs: Vec::new(),
            decorations: Vec::new(),
            computed_width: 0.0,
            computed_height: 0.0,
        });
    }

    // Build resolved runs: if no explicit runs, entire content uses default_style
    let runs = if data.runs.is_empty() {
        vec![(0, data.content.len(), default_style.clone())]
    } else {
        data.runs
            .iter()
            .map(|run| {
                let resolved = resolve_run_style(&run.style, default_style);
                (
                    run.start.min(data.content.len()),
                    run.end.min(data.content.len()),
                    resolved,
                )
            })
            .collect()
    };

    // Find font for the default style
    let family = default_style.font_family.value();
    let weight = default_style.font_weight.value();
    let font_data = font_db
        .find_font(&family, weight)
        .ok_or_else(|| TextError::FontNotFound {
            family: family.clone(),
            weight,
        })?;

    // Shape text (with fallback for characters not in the primary font)
    let font_size = default_style.font_size.value();
    let letter_spacing = default_style.letter_spacing.value();
    let (shaped, font_runs) = shaper::shape_text_with_fallback(
        &data.content,
        &font_data,
        font_db,
        font_size,
        letter_spacing,
        default_style.transform,
        weight,
    )?;

    // Layout text into lines
    let available_width = match data.sizing_mode {
        TextSizingMode::AutoWidth => f32::INFINITY,
        _ => data.width,
    };
    let laid_out = layout::layout_text(
        &shaped,
        &data.content,
        available_width,
        default_style.text_align,
        default_style.vertical_align,
        default_style.line_height.clone(),
        font_size,
        data.height,
        data.sizing_mode,
    );

    // Extract glyph outlines
    let mut glyphs = Vec::new();
    let mut decorations = Vec::new();

    for line in &laid_out.lines {
        for positioned in &line.glyphs {
            // Determine which run this glyph belongs to
            let run_index = find_run_index(&runs, positioned.cluster);

            // Find the correct font for this glyph's cluster
            let glyph_font = font_for_cluster(&font_runs, positioned.cluster);

            // Get glyph outline
            if let Some(outline) =
                glyph::get_glyph_outline(glyph_font, positioned.glyph_id, font_size)?
            {
                // Translate to final position
                let mut path = outline;
                // Flip Y (font coords are Y-up, canvas is Y-down) and translate
                path.apply_affine(kurbo::Affine::new([
                    1.0,
                    0.0,
                    0.0,
                    -1.0,
                    positioned.x as f64,
                    positioned.y as f64,
                ]));
                glyphs.push(PositionedGlyph { path, run_index });
            }
        }

        // Generate decorations for this line
        let decoration = default_style.decoration;
        if decoration != ode_format::typography::TextDecoration::None && !line.glyphs.is_empty() {
            let line_start_x = line.glyphs.first().map(|g| g.x).unwrap_or(0.0);
            let line_end_x = line.glyphs.last().map(|g| g.x + g.advance).unwrap_or(0.0);
            let line_width = line_end_x - line_start_x;

            if line_width > 0.0 {
                let thickness = (font_size * 0.07).max(1.0);

                if matches!(
                    decoration,
                    ode_format::typography::TextDecoration::Underline
                        | ode_format::typography::TextDecoration::Both
                ) {
                    let y = line.baseline_y + font_size * 0.15;
                    let rect = make_rect_path(line_start_x, y, line_width, thickness);
                    decorations.push(DecorationRect {
                        path: rect,
                        run_index: 0,
                    });
                }
                if matches!(
                    decoration,
                    ode_format::typography::TextDecoration::Strikethrough
                        | ode_format::typography::TextDecoration::Both
                ) {
                    let y = line.baseline_y - font_size * 0.3;
                    let rect = make_rect_path(line_start_x, y, line_width, thickness);
                    decorations.push(DecorationRect {
                        path: rect,
                        run_index: 0,
                    });
                }
            }
        }
    }

    Ok(ProcessedText {
        glyphs,
        decorations,
        computed_width: laid_out.computed_width,
        computed_height: laid_out.computed_height,
    })
}

/// Resolve a TextRunStyle against the default TextStyle.
fn resolve_run_style(
    run_style: &ode_format::typography::TextRunStyle,
    default: &TextStyle,
) -> TextStyle {
    TextStyle {
        font_family: run_style
            .font_family
            .clone()
            .unwrap_or_else(|| default.font_family.clone()),
        font_weight: run_style
            .font_weight
            .clone()
            .unwrap_or_else(|| default.font_weight.clone()),
        font_size: run_style
            .font_size
            .clone()
            .unwrap_or_else(|| default.font_size.clone()),
        line_height: run_style
            .line_height
            .clone()
            .unwrap_or_else(|| default.line_height.clone()),
        letter_spacing: run_style
            .letter_spacing
            .clone()
            .unwrap_or_else(|| default.letter_spacing.clone()),
        paragraph_spacing: default.paragraph_spacing.clone(),
        text_align: default.text_align,
        vertical_align: default.vertical_align,
        decoration: run_style.decoration.unwrap_or(default.decoration),
        transform: run_style.transform.unwrap_or(default.transform),
        opentype_features: run_style
            .opentype_features
            .clone()
            .unwrap_or_else(|| default.opentype_features.clone()),
        variable_axes: run_style
            .variable_axes
            .clone()
            .unwrap_or_else(|| default.variable_axes.clone()),
    }
}

/// Resolve fills for a given run index.
pub fn resolve_run_fills(
    data: &TextData,
    _runs: &[(usize, usize, TextStyle)],
    run_index: usize,
) -> Vec<Fill> {
    // Check if the corresponding TextRun has fill overrides
    if run_index < data.runs.len() {
        if let Some(ref fills) = data.runs[run_index].style.fills {
            if !fills.is_empty() {
                return fills.clone();
            }
        }
    }
    // Fall back to TextData's visual fills
    data.visual.fills.clone()
}

/// Find the font data for a given cluster (byte offset) by looking through font runs.
fn font_for_cluster<'a>(font_runs: &'a [shaper::FontRun], cluster: usize) -> &'a [u8] {
    debug_assert!(!font_runs.is_empty(), "font_runs should never be empty");
    for run in font_runs {
        if cluster >= run.byte_start && cluster < run.byte_end {
            return &run.font_data;
        }
    }
    // Fallback to first run's font — cluster may equal byte_end of the last run
    // for trailing glyphs. This is safe since we always have at least one run.
    &font_runs.first().expect("font_runs is non-empty").font_data
}

fn find_run_index(runs: &[(usize, usize, TextStyle)], byte_offset: usize) -> usize {
    for (i, (start, end, _)) in runs.iter().enumerate() {
        if byte_offset >= *start && byte_offset < *end {
            return i;
        }
    }
    0
}

fn make_rect_path(x: f32, y: f32, width: f32, height: f32) -> kurbo::BezPath {
    let mut path = kurbo::BezPath::new();
    path.move_to((x as f64, y as f64));
    path.line_to(((x + width) as f64, y as f64));
    path.line_to(((x + width) as f64, (y + height) as f64));
    path.line_to((x as f64, (y + height) as f64));
    path.close_path();
    path
}
