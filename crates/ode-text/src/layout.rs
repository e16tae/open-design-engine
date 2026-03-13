use ode_format::typography::{LineHeight, TextAlign, TextSizingMode, VerticalAlign};
use crate::shaper::ShapedText;

/// A positioned glyph within a laid-out line.
pub struct PositionedShapedGlyph {
    pub glyph_id: u16,
    pub cluster: usize,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
}

/// A single line of laid-out text.
pub struct LayoutedLine {
    pub glyphs: Vec<PositionedShapedGlyph>,
    pub baseline_y: f32,
    pub width: f32,
}

/// Result of the layout pass.
pub struct LayoutedText {
    pub lines: Vec<LayoutedLine>,
    pub computed_width: f32,
    pub computed_height: f32,
}

/// Layout shaped text into lines with alignment.
pub fn layout_text(
    shaped: &ShapedText,
    text: &str,
    available_width: f32,
    text_align: TextAlign,
    vertical_align: VerticalAlign,
    line_height_spec: LineHeight,
    font_size: f32,
    container_height: f32,
    sizing_mode: TextSizingMode,
) -> LayoutedText {
    let line_height = compute_line_height(&line_height_spec, font_size, shaped.ascent, shaped.descent);

    // Find line break opportunities using unicode-linebreak
    let break_opportunities = unicode_linebreak::linebreaks(text).collect::<Vec<_>>();

    // Build lines by breaking at allowed points
    let mut lines: Vec<Vec<usize>> = Vec::new(); // indices into shaped.glyphs
    let mut current_line: Vec<usize> = Vec::new();
    let mut current_width: f32 = 0.0;

    for (i, glyph) in shaped.glyphs.iter().enumerate() {
        let glyph_width = glyph.x_advance;
        let byte_offset = glyph.cluster;

        // Check if this cluster corresponds to a newline character
        let is_newline = text.as_bytes().get(byte_offset) == Some(&b'\n');

        if is_newline {
            lines.push(std::mem::take(&mut current_line));
            current_width = 0.0;
            continue;
        }

        // Check if adding this glyph would exceed available width
        let would_overflow = current_width + glyph_width > available_width && !current_line.is_empty();

        if would_overflow && available_width.is_finite() {
            // Find the best break point in the current line
            if let Some(break_idx) = find_break_point(&current_line, shaped, &break_opportunities) {
                // Break at the found point
                let rest = current_line.split_off(break_idx);
                lines.push(std::mem::take(&mut current_line));
                current_line = rest;
                current_width = current_line.iter()
                    .map(|&idx| shaped.glyphs[idx].x_advance)
                    .sum();
            } else {
                // No good break point; force break here
                lines.push(std::mem::take(&mut current_line));
                current_width = 0.0;
            }
        }

        current_line.push(i);
        current_width += glyph_width;
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    // If no lines were created (empty text), create one empty line
    if lines.is_empty() {
        lines.push(Vec::new());
    }

    // Compute total text height
    let total_height = lines.len() as f32 * line_height;

    // Compute vertical offset for alignment
    let effective_height = match sizing_mode {
        TextSizingMode::Fixed => container_height,
        TextSizingMode::AutoHeight | TextSizingMode::AutoWidth => total_height,
    };

    let vertical_offset = match vertical_align {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => (effective_height - total_height) / 2.0,
        VerticalAlign::Bottom => effective_height - total_height,
    };

    // Position glyphs within each line
    let mut laid_out_lines = Vec::new();
    let mut max_line_width: f32 = 0.0;

    for (line_idx, line_glyph_indices) in lines.iter().enumerate() {
        let baseline_y = vertical_offset + shaped.ascent + line_idx as f32 * line_height;

        // Compute line width
        let line_width: f32 = line_glyph_indices.iter()
            .map(|&idx| shaped.glyphs[idx].x_advance)
            .sum();

        max_line_width = max_line_width.max(line_width);

        // Compute horizontal offset for alignment
        let align_offset = match text_align {
            TextAlign::Left | TextAlign::Justify => 0.0,
            TextAlign::Center => (available_width - line_width).max(0.0) / 2.0,
            TextAlign::Right => (available_width - line_width).max(0.0),
        };

        // Compute justified spacing
        let extra_space_per_gap = if matches!(text_align, TextAlign::Justify)
            && line_glyph_indices.len() > 1
            && line_idx < lines.len() - 1  // Don't justify last line
            && available_width.is_finite()
        {
            let space_count = count_spaces_in_line(&line_glyph_indices, shaped, text);
            if space_count > 0 {
                (available_width - line_width) / space_count as f32
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Position each glyph
        let mut pen_x = align_offset;
        let mut positioned_glyphs = Vec::new();

        for &glyph_idx in line_glyph_indices {
            let glyph = &shaped.glyphs[glyph_idx];

            positioned_glyphs.push(PositionedShapedGlyph {
                glyph_id: glyph.glyph_id,
                cluster: glyph.cluster,
                x: pen_x + glyph.x_offset,
                y: baseline_y + glyph.y_offset,
                advance: glyph.x_advance,
            });

            pen_x += glyph.x_advance;

            // Add extra space for justified text at space characters
            if extra_space_per_gap > 0.0 {
                let byte_offset = glyph.cluster;
                if text.as_bytes().get(byte_offset) == Some(&b' ') {
                    pen_x += extra_space_per_gap;
                }
            }
        }

        laid_out_lines.push(LayoutedLine {
            glyphs: positioned_glyphs,
            baseline_y,
            width: line_width,
        });
    }

    let computed_width = match sizing_mode {
        TextSizingMode::AutoWidth => max_line_width,
        _ => available_width.min(max_line_width.max(0.0)),
    };

    let computed_height = match sizing_mode {
        TextSizingMode::Fixed => container_height,
        _ => total_height,
    };

    LayoutedText {
        lines: laid_out_lines,
        computed_width,
        computed_height,
    }
}

fn compute_line_height(
    spec: &LineHeight,
    font_size: f32,
    ascent: f32,
    descent: f32,
) -> f32 {
    match spec {
        LineHeight::Auto => {
            // Typical auto line height: 1.2x the font size, or ascent - descent
            let metrics_height = ascent - descent;
            metrics_height.max(font_size * 1.2)
        }
        LineHeight::Fixed { value } => value.value(),
        LineHeight::Percent { value } => font_size * value.value() / 100.0,
    }
}

/// Find the best break point in the current line based on unicode-linebreak opportunities.
fn find_break_point(
    line_indices: &[usize],
    shaped: &ShapedText,
    break_opportunities: &[(usize, unicode_linebreak::BreakOpportunity)],
) -> Option<usize> {
    // Walk backwards through the line to find an allowed break
    for i in (1..line_indices.len()).rev() {
        let glyph_cluster = shaped.glyphs[line_indices[i]].cluster;
        for &(offset, opp) in break_opportunities {
            if offset == glyph_cluster && matches!(opp, unicode_linebreak::BreakOpportunity::Allowed) {
                return Some(i);
            }
        }
    }
    None
}

fn count_spaces_in_line(
    line_indices: &[usize],
    shaped: &ShapedText,
    text: &str,
) -> usize {
    line_indices.iter()
        .filter(|&&idx| {
            let cluster = shaped.glyphs[idx].cluster;
            text.as_bytes().get(cluster) == Some(&b' ')
        })
        .count()
}
