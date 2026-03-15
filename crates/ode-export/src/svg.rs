use std::fmt::Write;
use std::path::Path;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use kurbo::{BezPath, PathEl};
use ode_core::scene::{
    RenderCommand, ResolvedEffect, ResolvedGradientStop, ResolvedPaint, Scene, StrokeStyle,
};
use ode_format::color::Color;
use ode_format::node::FillRule;
use ode_format::style::{BlendMode, StrokeCap, StrokeJoin, StrokePosition};

use crate::error::ExportError;

pub struct SvgExporter;

impl SvgExporter {
    pub fn export(scene: &Scene, path: &Path) -> Result<(), ExportError> {
        let svg = Self::export_string(scene)?;
        std::fs::write(path, svg)?;
        Ok(())
    }

    pub fn export_string(scene: &Scene) -> Result<String, ExportError> {
        let mut ctx = SvgContext::new(scene.width, scene.height);
        for cmd in &scene.commands {
            ctx.process_command(cmd)?;
        }
        Ok(ctx.finish())
    }

    pub fn export_bytes(scene: &Scene) -> Result<Vec<u8>, ExportError> {
        Self::export_string(scene).map(|s| s.into_bytes())
    }
}

// ─── Context ───

struct LayerState {
    had_effect: bool,
}

struct SvgContext {
    width: f32,
    height: f32,
    defs: String,
    body: String,
    layer_stack: Vec<LayerState>,
    id_counter: u32,
}

impl SvgContext {
    fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            defs: String::new(),
            body: String::new(),
            layer_stack: Vec::new(),
            id_counter: 0,
        }
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.id_counter += 1;
        format!("{prefix}{}", self.id_counter)
    }

    fn finish(self) -> String {
        let mut out = String::with_capacity(self.defs.len() + self.body.len() + 256);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        let _ = write!(
            out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
            self.width, self.height, self.width, self.height
        );
        if !self.defs.is_empty() {
            out.push_str("\n<defs>");
            out.push_str(&self.defs);
            out.push_str("\n</defs>");
        }
        out.push_str(&self.body);
        out.push_str("\n</svg>\n");
        out
    }

    fn process_command(&mut self, cmd: &RenderCommand) -> Result<(), ExportError> {
        match cmd {
            RenderCommand::PushLayer {
                opacity,
                blend_mode,
                clip,
                transform,
            } => {
                let mut attrs = String::new();

                let has_opacity = (*opacity - 1.0).abs() > f32::EPSILON;
                let has_blend = *blend_mode != BlendMode::Normal;

                if has_opacity && has_blend {
                    // Combine both into a single style attribute for correct SVG rendering
                    let _ = write!(
                        attrs,
                        " style=\"opacity:{};mix-blend-mode:{}\"",
                        fmt_f32(*opacity),
                        blend_mode_to_css(blend_mode)
                    );
                } else if has_opacity {
                    let _ = write!(attrs, " opacity=\"{}\"", fmt_f32(*opacity));
                } else if has_blend {
                    let _ = write!(
                        attrs,
                        " style=\"mix-blend-mode:{}\"",
                        blend_mode_to_css(blend_mode)
                    );
                }

                let t = *transform;
                if let Some(clip_path) = clip {
                    let clip_id = self.next_id("clip");
                    let d = bezpath_to_svg_d(clip_path);
                    let clip_transform = transform_to_svg_attr(&t);
                    let _ = write!(
                        self.defs,
                        "\n<clipPath id=\"{clip_id}\"><path d=\"{d}\"{clip_transform}/>"
                    );
                    self.defs.push_str("</clipPath>");
                    let _ = write!(attrs, " clip-path=\"url(#{clip_id})\"");
                }

                let _ = write!(self.body, "\n<g{attrs}>");
                self.layer_stack.push(LayerState { had_effect: false });
            }
            RenderCommand::PopLayer => {
                if let Some(layer) = self.layer_stack.pop() {
                    if layer.had_effect {
                        self.body.push_str("\n</g>"); // close effect wrapper <g>
                    }
                }
                self.body.push_str("\n</g>"); // close layer <g>
            }
            RenderCommand::FillPath {
                path,
                paint,
                fill_rule,
                transform,
            } => {
                let d = bezpath_to_svg_d(path);
                let (fill_str, fill_opacity) = self.resolve_paint(paint)?;
                let rule = fill_rule_to_svg(fill_rule);
                let t = transform_to_svg_attr(transform);

                let mut attrs = format!(" d=\"{d}\" fill=\"{fill_str}\"");
                if !rule.is_empty() {
                    let _ = write!(attrs, " fill-rule=\"{rule}\"");
                }
                if (fill_opacity - 1.0).abs() > f32::EPSILON {
                    let _ = write!(attrs, " fill-opacity=\"{}\"", fmt_f32(fill_opacity));
                }
                attrs.push_str(&t);
                let _ = write!(self.body, "\n<path{attrs}/>");
            }
            RenderCommand::StrokePath {
                path,
                paint,
                stroke,
                transform,
            } => {
                self.write_stroke(path, paint, stroke, transform)?;
            }
            RenderCommand::DrawImage {
                data,
                width,
                height,
                transform,
            } => {
                // Detect image format from magic bytes
                let mime = if data.starts_with(b"\x89PNG") {
                    "image/png"
                } else {
                    "image/jpeg"
                };
                let b64 = BASE64.encode(data);
                let t = transform_to_svg_attr(transform);
                let _ = write!(
                    self.body,
                    "\n<image x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" href=\"data:{};base64,{}\"{t}/>",
                    fmt_f32(*width),
                    fmt_f32(*height),
                    mime,
                    b64
                );
            }
            RenderCommand::ApplyEffect { effect } => {
                let filter_id = self.write_filter_def(effect);
                // Apply filter to the current layer's <g> tag
                // We insert a wrapper <g> around the current body content
                if let Some(layer) = self.layer_stack.last_mut() {
                    if !layer.had_effect {
                        layer.had_effect = true;
                        let _ = write!(self.body, "\n<g filter=\"url(#{filter_id})\">");
                    }
                    // For multiple effects on same layer, close previous and open new
                    else {
                        let _ = write!(self.body, "\n</g>\n<g filter=\"url(#{filter_id})\">");
                    }
                }
            }
        }
        Ok(())
    }

    fn write_stroke(
        &mut self,
        path: &BezPath,
        paint: &ResolvedPaint,
        stroke: &StrokeStyle,
        transform: &tiny_skia::Transform,
    ) -> Result<(), ExportError> {
        let d = bezpath_to_svg_d(path);
        let (stroke_color, stroke_opacity) = self.resolve_paint(paint)?;
        let t = transform_to_svg_attr(transform);
        let cap = stroke_cap_to_svg(&stroke.cap);
        let join = stroke_join_to_svg(&stroke.join);

        match stroke.position {
            StrokePosition::Center => {
                let mut attrs = format!(
                    " d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"",
                    d,
                    stroke_color,
                    fmt_f32(stroke.width)
                );
                if (stroke_opacity - 1.0).abs() > f32::EPSILON {
                    let _ = write!(attrs, " stroke-opacity=\"{}\"", fmt_f32(stroke_opacity));
                }
                let _ = write!(
                    attrs,
                    " stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\""
                );
                if stroke.join == StrokeJoin::Miter
                    && (stroke.miter_limit - 4.0).abs() > f32::EPSILON
                {
                    let _ = write!(
                        attrs,
                        " stroke-miterlimit=\"{}\"",
                        fmt_f32(stroke.miter_limit)
                    );
                }
                if let Some(ref dash) = stroke.dash {
                    let segs: Vec<String> = dash.segments.iter().map(|s| fmt_f32(*s)).collect();
                    let _ = write!(attrs, " stroke-dasharray=\"{}\"", segs.join(","));
                    if dash.offset.abs() > f32::EPSILON {
                        let _ = write!(attrs, " stroke-dashoffset=\"{}\"", fmt_f32(dash.offset));
                    }
                }
                attrs.push_str(&t);
                let _ = write!(self.body, "\n<path{attrs}/>");
            }
            StrokePosition::Inside => {
                // Inside stroke: double the width, clip to path interior
                let clip_id = self.next_id("clip");
                let _ = write!(
                    self.defs,
                    "\n<clipPath id=\"{clip_id}\"><path d=\"{d}\"/></clipPath>"
                );
                let doubled = stroke.width * 2.0;
                let mut attrs = format!(
                    " d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" clip-path=\"url(#{})\"",
                    d,
                    stroke_color,
                    fmt_f32(doubled),
                    clip_id
                );
                if (stroke_opacity - 1.0).abs() > f32::EPSILON {
                    let _ = write!(attrs, " stroke-opacity=\"{}\"", fmt_f32(stroke_opacity));
                }
                let _ = write!(
                    attrs,
                    " stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\""
                );
                if let Some(ref dash) = stroke.dash {
                    let segs: Vec<String> = dash.segments.iter().map(|s| fmt_f32(*s)).collect();
                    let _ = write!(attrs, " stroke-dasharray=\"{}\"", segs.join(","));
                }
                attrs.push_str(&t);
                let _ = write!(self.body, "\n<path{attrs}/>");
            }
            StrokePosition::Outside => {
                // Outside stroke: double the width, clip to path exterior (inverted mask)
                let clip_id = self.next_id("clip");
                let _ = write!(
                    self.defs,
                    "\n<clipPath id=\"{clip_id}\"><path d=\"M0,0 H{} V{} H0 Z {d}\" clip-rule=\"evenodd\"/></clipPath>",
                    fmt_f32(self.width),
                    fmt_f32(self.height),
                );
                let doubled = stroke.width * 2.0;
                let mut attrs = format!(
                    " d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" clip-path=\"url(#{})\"",
                    d,
                    stroke_color,
                    fmt_f32(doubled),
                    clip_id
                );
                if (stroke_opacity - 1.0).abs() > f32::EPSILON {
                    let _ = write!(attrs, " stroke-opacity=\"{}\"", fmt_f32(stroke_opacity));
                }
                let _ = write!(
                    attrs,
                    " stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\""
                );
                if let Some(ref dash) = stroke.dash {
                    let segs: Vec<String> = dash.segments.iter().map(|s| fmt_f32(*s)).collect();
                    let _ = write!(attrs, " stroke-dasharray=\"{}\"", segs.join(","));
                }
                attrs.push_str(&t);
                let _ = write!(self.body, "\n<path{attrs}/>");
            }
        }
        Ok(())
    }

    /// Resolve a paint to a CSS fill/stroke value and opacity.
    /// For gradients, writes a `<defs>` entry and returns `url(#id)`.
    fn resolve_paint(&mut self, paint: &ResolvedPaint) -> Result<(String, f32), ExportError> {
        match paint {
            ResolvedPaint::Solid(color) => {
                let (css, alpha) = color_to_css(color);
                Ok((css, alpha))
            }
            ResolvedPaint::LinearGradient { stops, start, end } => {
                let id = self.next_id("lg");
                let _ = write!(
                    self.defs,
                    "\n<linearGradient id=\"{}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">",
                    id,
                    fmt_f32(start.x as f32),
                    fmt_f32(start.y as f32),
                    fmt_f32(end.x as f32),
                    fmt_f32(end.y as f32)
                );
                for stop in stops {
                    write_gradient_stop(&mut self.defs, stop);
                }
                self.defs.push_str("</linearGradient>");
                Ok((format!("url(#{id})"), 1.0))
            }
            ResolvedPaint::RadialGradient {
                stops,
                center,
                radius,
            } => {
                let id = self.next_id("rg");
                let cx = center.x as f32;
                let cy = center.y as f32;
                let rx = radius.x as f32;
                let ry = radius.y as f32;
                // Use rx as the SVG r, apply scale for elliptical
                let transform = if (rx - ry).abs() > f32::EPSILON && rx > 0.0 {
                    format!(
                        " gradientTransform=\"translate({},{}) scale(1,{}) translate({},{})\"",
                        fmt_f32(cx),
                        fmt_f32(cy),
                        fmt_f32(ry / rx),
                        fmt_f32(-cx),
                        fmt_f32(-cy)
                    )
                } else {
                    String::new()
                };
                let _ = write!(
                    self.defs,
                    "\n<radialGradient id=\"{}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\"{}>",
                    id,
                    fmt_f32(cx),
                    fmt_f32(cy),
                    fmt_f32(rx),
                    transform
                );
                for stop in stops {
                    write_gradient_stop(&mut self.defs, stop);
                }
                self.defs.push_str("</radialGradient>");
                Ok((format!("url(#{id})"), 1.0))
            }
            ResolvedPaint::AngularGradient {
                stops,
                center,
                angle,
            } => {
                // SVG has no native angular gradient — rasterize and embed as base64 PNG
                self.write_raster_gradient_fallback(|w, h| {
                    ode_core::paint::generate_angular_gradient_pixmap(w, h, stops, *center, *angle)
                })
            }
            ResolvedPaint::DiamondGradient {
                stops,
                center,
                radius,
            } => {
                // SVG has no native diamond gradient — rasterize and embed as base64 PNG
                self.write_raster_gradient_fallback(|w, h| {
                    ode_core::paint::generate_diamond_gradient_pixmap(w, h, stops, *center, *radius)
                })
            }
        }
    }

    /// Rasterize a gradient to a pixmap, encode as base64 PNG, define as SVG pattern.
    fn write_raster_gradient_fallback(
        &mut self,
        generate: impl FnOnce(u32, u32) -> Option<tiny_skia::Pixmap>,
    ) -> Result<(String, f32), ExportError> {
        let w = (self.width.ceil() as u32).max(1);
        let h = (self.height.ceil() as u32).max(1);
        let pixmap = generate(w, h).ok_or_else(|| {
            ExportError::SvgGenerationFailed("failed to generate gradient pixmap".into())
        })?;
        let png_bytes = pixmap
            .encode_png()
            .map_err(|e| ExportError::SvgGenerationFailed(e.to_string()))?;
        let b64 = BASE64.encode(&png_bytes);

        let pat_id = self.next_id("pat");
        let img_id = self.next_id("img");
        let _ = write!(
            self.defs,
            "\n<pattern id=\"{pat_id}\" patternUnits=\"userSpaceOnUse\" width=\"{w}\" height=\"{h}\">"
        );
        let _ = write!(
            self.defs,
            "<image id=\"{img_id}\" width=\"{w}\" height=\"{h}\" href=\"data:image/png;base64,{b64}\"/>"
        );
        self.defs.push_str("</pattern>");
        Ok((format!("url(#{pat_id})"), 1.0))
    }

    fn write_filter_def(&mut self, effect: &ResolvedEffect) -> String {
        let id = self.next_id("f");
        match effect {
            ResolvedEffect::DropShadow {
                color,
                offset_x,
                offset_y,
                blur_radius,
                spread,
                ..
            } => {
                let (css_color, alpha) = color_to_css(color);
                let sigma = blur_radius / 2.0;
                let _ = write!(self.defs, "\n<filter id=\"{id}\">");
                let _ = write!(
                    self.defs,
                    "<feFlood flood-color=\"{}\" flood-opacity=\"{}\" result=\"flood\"/>",
                    css_color,
                    fmt_f32(alpha)
                );
                self.defs.push_str("<feComposite in=\"flood\" in2=\"SourceAlpha\" operator=\"in\" result=\"shadow\"/>");
                if spread.abs() > f32::EPSILON {
                    let _ = write!(
                        self.defs,
                        "<feMorphology in=\"shadow\" operator=\"dilate\" radius=\"{}\" result=\"spread\"/>",
                        fmt_f32(*spread)
                    );
                    let _ = write!(
                        self.defs,
                        "<feGaussianBlur in=\"spread\" stdDeviation=\"{}\" result=\"blur\"/>",
                        fmt_f32(sigma)
                    );
                } else {
                    let _ = write!(
                        self.defs,
                        "<feGaussianBlur in=\"shadow\" stdDeviation=\"{}\" result=\"blur\"/>",
                        fmt_f32(sigma)
                    );
                }
                let _ = write!(
                    self.defs,
                    "<feOffset in=\"blur\" dx=\"{}\" dy=\"{}\" result=\"offset\"/>",
                    fmt_f32(*offset_x),
                    fmt_f32(*offset_y)
                );
                self.defs.push_str("<feMerge><feMergeNode in=\"offset\"/><feMergeNode in=\"SourceGraphic\"/></feMerge>");
                self.defs.push_str("</filter>");
            }
            ResolvedEffect::InnerShadow {
                color,
                offset_x,
                offset_y,
                blur_radius,
                spread,
                ..
            } => {
                let (css_color, alpha) = color_to_css(color);
                let sigma = blur_radius / 2.0;
                let _ = write!(self.defs, "\n<filter id=\"{id}\">");
                // Invert alpha of source
                self.defs.push_str("<feComponentTransfer in=\"SourceAlpha\"><feFuncA type=\"table\" tableValues=\"1 0\"/></feComponentTransfer>");
                if spread.abs() > f32::EPSILON {
                    let _ = write!(
                        self.defs,
                        "<feMorphology operator=\"dilate\" radius=\"{}\" result=\"spread\"/>",
                        fmt_f32(*spread)
                    );
                    let _ = write!(
                        self.defs,
                        "<feGaussianBlur in=\"spread\" stdDeviation=\"{}\" result=\"blur\"/>",
                        fmt_f32(sigma)
                    );
                } else {
                    let _ = write!(
                        self.defs,
                        "<feGaussianBlur stdDeviation=\"{}\" result=\"blur\"/>",
                        fmt_f32(sigma)
                    );
                }
                let _ = write!(
                    self.defs,
                    "<feOffset in=\"blur\" dx=\"{}\" dy=\"{}\" result=\"offset\"/>",
                    fmt_f32(*offset_x),
                    fmt_f32(*offset_y)
                );
                let _ = write!(
                    self.defs,
                    "<feFlood flood-color=\"{}\" flood-opacity=\"{}\" result=\"color\"/>",
                    css_color,
                    fmt_f32(alpha)
                );
                self.defs.push_str(
                    "<feComposite in=\"color\" in2=\"offset\" operator=\"in\" result=\"shadow\"/>",
                );
                self.defs.push_str("<feComposite in=\"shadow\" in2=\"SourceAlpha\" operator=\"in\" result=\"clipped\"/>");
                self.defs.push_str("<feMerge><feMergeNode in=\"SourceGraphic\"/><feMergeNode in=\"clipped\"/></feMerge>");
                self.defs.push_str("</filter>");
            }
            ResolvedEffect::LayerBlur { radius } => {
                let sigma = radius / 2.0;
                let _ = write!(
                    self.defs,
                    "\n<filter id=\"{id}\"><feGaussianBlur stdDeviation=\"{}\"/></filter>",
                    fmt_f32(sigma)
                );
            }
            ResolvedEffect::BackgroundBlur { radius } => {
                // BackgroundBlur has limited browser support; approximate with BackgroundImage
                let sigma = radius / 2.0;
                let _ = write!(
                    self.defs,
                    "\n<!-- BackgroundBlur: limited browser support --><filter id=\"{id}\"><feGaussianBlur in=\"BackgroundImage\" stdDeviation=\"{}\"/></filter>",
                    fmt_f32(sigma)
                );
            }
        }
        id
    }
}

// ─── Conversion Helpers ───

fn bezpath_to_svg_d(path: &BezPath) -> String {
    let mut d = String::new();
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => {
                let _ = write!(d, "M{} {}", fmt_f32(p.x as f32), fmt_f32(p.y as f32));
            }
            PathEl::LineTo(p) => {
                let _ = write!(d, "L{} {}", fmt_f32(p.x as f32), fmt_f32(p.y as f32));
            }
            PathEl::QuadTo(p1, p2) => {
                let _ = write!(
                    d,
                    "Q{} {},{} {}",
                    fmt_f32(p1.x as f32),
                    fmt_f32(p1.y as f32),
                    fmt_f32(p2.x as f32),
                    fmt_f32(p2.y as f32)
                );
            }
            PathEl::CurveTo(p1, p2, p3) => {
                let _ = write!(
                    d,
                    "C{} {},{} {},{} {}",
                    fmt_f32(p1.x as f32),
                    fmt_f32(p1.y as f32),
                    fmt_f32(p2.x as f32),
                    fmt_f32(p2.y as f32),
                    fmt_f32(p3.x as f32),
                    fmt_f32(p3.y as f32)
                );
            }
            PathEl::ClosePath => {
                d.push('Z');
            }
        }
    }
    d
}

fn color_to_css(color: &Color) -> (String, f32) {
    match color {
        Color::Srgb { r, g, b, a } => {
            let ri = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
            let gi = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
            let bi = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
            (format!("rgb({ri},{gi},{bi})"), *a)
        }
        Color::DisplayP3 { r, g, b, a } => (
            format!(
                "color(display-p3 {} {} {})",
                fmt_f32(*r),
                fmt_f32(*g),
                fmt_f32(*b)
            ),
            *a,
        ),
        // Fallback: convert to sRGB via to_rgba_u8
        other => {
            let [r, g, b, a] = other.to_rgba_u8();
            (format!("rgb({r},{g},{b})"), a as f32 / 255.0)
        }
    }
}

fn fill_rule_to_svg(rule: &FillRule) -> &'static str {
    match rule {
        FillRule::NonZero => "", // default, no attribute needed
        FillRule::EvenOdd => "evenodd",
    }
}

fn blend_mode_to_css(mode: &BlendMode) -> &'static str {
    match mode {
        BlendMode::Normal => "normal",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::ColorDodge => "color-dodge",
        BlendMode::ColorBurn => "color-burn",
        BlendMode::HardLight => "hard-light",
        BlendMode::SoftLight => "soft-light",
        BlendMode::Difference => "difference",
        BlendMode::Exclusion => "exclusion",
        BlendMode::Hue => "hue",
        BlendMode::Saturation => "saturation",
        BlendMode::Color => "color",
        BlendMode::Luminosity => "luminosity",
    }
}

fn stroke_cap_to_svg(cap: &StrokeCap) -> &'static str {
    match cap {
        StrokeCap::Butt => "butt",
        StrokeCap::Round => "round",
        StrokeCap::Square => "square",
    }
}

fn stroke_join_to_svg(join: &StrokeJoin) -> &'static str {
    match join {
        StrokeJoin::Miter => "miter",
        StrokeJoin::Round => "round",
        StrokeJoin::Bevel => "bevel",
    }
}

fn transform_to_svg_attr(t: &tiny_skia::Transform) -> String {
    if t.is_identity() {
        return String::new();
    }
    format!(
        " transform=\"matrix({},{},{},{},{},{})\"",
        fmt_f32(t.sx),
        fmt_f32(t.ky),
        fmt_f32(t.kx),
        fmt_f32(t.sy),
        fmt_f32(t.tx),
        fmt_f32(t.ty)
    )
}

fn write_gradient_stop(defs: &mut String, stop: &ResolvedGradientStop) {
    let (css, alpha) = color_to_css(&stop.color);
    if (alpha - 1.0).abs() > f32::EPSILON {
        let _ = write!(
            defs,
            "<stop offset=\"{}\" stop-color=\"{}\" stop-opacity=\"{}\"/>",
            fmt_f32(stop.position),
            css,
            fmt_f32(alpha)
        );
    } else {
        let _ = write!(
            defs,
            "<stop offset=\"{}\" stop-color=\"{}\"/>",
            fmt_f32(stop.position),
            css
        );
    }
}

/// Format f32 in a compact way: strip trailing zeros.
fn fmt_f32(v: f32) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    if v == v.floor() && v.abs() < 1e9 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ode_format::color::Color;
    use ode_format::style::BlendMode;

    // ─── bezpath_to_svg_d ───

    #[test]
    fn bezpath_to_svg_d_basic() {
        let mut bp = BezPath::new();
        bp.move_to((10.0, 20.0));
        bp.line_to((30.0, 40.0));
        bp.curve_to((50.0, 60.0), (70.0, 80.0), (90.0, 100.0));
        bp.quad_to((110.0, 120.0), (130.0, 140.0));
        bp.close_path();
        let d = bezpath_to_svg_d(&bp);
        assert!(d.starts_with("M10 20"));
        assert!(d.contains("L30 40"));
        assert!(d.contains("C50 60,70 80,90 100"));
        assert!(d.contains("Q110 120,130 140"));
        assert!(d.ends_with("Z"));
    }

    // ─── color_to_css ───

    #[test]
    fn color_to_css_srgb() {
        let (css, alpha) = color_to_css(&Color::Srgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        });
        assert_eq!(css, "rgb(255,0,0)");
        assert!((alpha - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn color_to_css_alpha() {
        let (css, alpha) = color_to_css(&Color::Srgb {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 0.5,
        });
        assert_eq!(css, "rgb(0,0,255)");
        assert!((alpha - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn color_to_css_display_p3() {
        let (css, _) = color_to_css(&Color::DisplayP3 {
            r: 1.0,
            g: 0.5,
            b: 0.0,
            a: 1.0,
        });
        assert!(css.starts_with("color(display-p3"));
    }

    // ─── Export string tests ───

    fn make_rect_path(w: f32, h: f32) -> BezPath {
        let mut bp = BezPath::new();
        bp.move_to((0.0, 0.0));
        bp.line_to((w as f64, 0.0));
        bp.line_to((w as f64, h as f64));
        bp.line_to((0.0, h as f64));
        bp.close_path();
        bp
    }

    #[test]
    fn export_string_solid_fill() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(100.0, 100.0),
                paint: ResolvedPaint::Solid(Color::Srgb {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("<path"));
        assert!(svg.contains("fill=\"rgb(255,0,0)\""));
        assert!(svg.contains("<?xml"));
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn export_string_linear_gradient() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(100.0, 100.0),
                paint: ResolvedPaint::LinearGradient {
                    stops: vec![
                        ResolvedGradientStop {
                            position: 0.0,
                            color: Color::black(),
                        },
                        ResolvedGradientStop {
                            position: 1.0,
                            color: Color::white(),
                        },
                    ],
                    start: kurbo::Point::new(0.0, 0.0),
                    end: kurbo::Point::new(100.0, 0.0),
                },
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("<linearGradient"));
        assert!(svg.contains("gradientUnits=\"userSpaceOnUse\""));
        assert!(svg.contains("<stop"));
        assert!(svg.contains("url(#lg1)"));
    }

    #[test]
    fn export_string_radial_gradient() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(100.0, 100.0),
                paint: ResolvedPaint::RadialGradient {
                    stops: vec![
                        ResolvedGradientStop {
                            position: 0.0,
                            color: Color::white(),
                        },
                        ResolvedGradientStop {
                            position: 1.0,
                            color: Color::black(),
                        },
                    ],
                    center: kurbo::Point::new(50.0, 50.0),
                    radius: kurbo::Point::new(50.0, 50.0),
                },
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("<radialGradient"));
        assert!(svg.contains("cx=\"50\""));
    }

    #[test]
    fn export_string_angular_fallback() {
        let scene = Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(50.0, 50.0),
                paint: ResolvedPaint::AngularGradient {
                    stops: vec![
                        ResolvedGradientStop {
                            position: 0.0,
                            color: Color::black(),
                        },
                        ResolvedGradientStop {
                            position: 1.0,
                            color: Color::white(),
                        },
                    ],
                    center: kurbo::Point::new(25.0, 25.0),
                    angle: 0.0,
                },
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("<pattern"));
        assert!(svg.contains("data:image/png;base64,"));
        assert!(svg.contains("<image"));
    }

    #[test]
    fn export_string_drop_shadow() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::DropShadow {
                        color: Color::Srgb {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.5,
                        },
                        offset_x: 2.0,
                        offset_y: 4.0,
                        blur_radius: 8.0,
                        spread: 0.0,
                        shape: make_rect_path(100.0, 100.0),
                    },
                },
                RenderCommand::FillPath {
                    path: make_rect_path(100.0, 100.0),
                    paint: ResolvedPaint::Solid(Color::white()),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("<filter"));
        assert!(svg.contains("feGaussianBlur"));
        assert!(svg.contains("feOffset"));
    }

    #[test]
    fn export_string_inner_shadow() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::ApplyEffect {
                    effect: ResolvedEffect::InnerShadow {
                        color: Color::black(),
                        offset_x: 0.0,
                        offset_y: 2.0,
                        blur_radius: 4.0,
                        spread: 0.0,
                        shape: make_rect_path(100.0, 100.0),
                    },
                },
                RenderCommand::FillPath {
                    path: make_rect_path(100.0, 100.0),
                    paint: ResolvedPaint::Solid(Color::white()),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("<filter"));
        assert!(svg.contains("feComponentTransfer"));
    }

    #[test]
    fn export_string_inside_stroke() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![RenderCommand::StrokePath {
                path: make_rect_path(100.0, 100.0),
                paint: ResolvedPaint::Solid(Color::black()),
                stroke: StrokeStyle {
                    width: 4.0,
                    position: StrokePosition::Inside,
                    cap: StrokeCap::Butt,
                    join: StrokeJoin::Miter,
                    miter_limit: 4.0,
                    dash: None,
                },
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("<clipPath"));
        assert!(svg.contains("stroke-width=\"8\"")); // doubled
    }

    #[test]
    fn export_string_outside_stroke() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            commands: vec![RenderCommand::StrokePath {
                path: make_rect_path(100.0, 100.0),
                paint: ResolvedPaint::Solid(Color::black()),
                stroke: StrokeStyle {
                    width: 4.0,
                    position: StrokePosition::Outside,
                    cap: StrokeCap::Butt,
                    join: StrokeJoin::Miter,
                    miter_limit: 4.0,
                    dash: None,
                },
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("<clipPath"));
        assert!(svg.contains("clip-rule=\"evenodd\""));
        assert!(svg.contains("stroke-width=\"8\"")); // doubled
    }

    #[test]
    fn export_string_opacity_layer() {
        let scene = Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 0.5,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::FillPath {
                    path: make_rect_path(50.0, 50.0),
                    paint: ResolvedPaint::Solid(Color::black()),
                    fill_rule: FillRule::NonZero,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("opacity=\"0.5\""));
    }

    #[test]
    fn export_string_blend_mode() {
        let scene = Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 1.0,
                    blend_mode: BlendMode::Multiply,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
            ],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("mix-blend-mode:multiply"));
    }

    #[test]
    fn export_string_nested_layers() {
        let scene = Scene {
            width: 50.0,
            height: 50.0,
            commands: vec![
                RenderCommand::PushLayer {
                    opacity: 0.8,
                    blend_mode: BlendMode::Normal,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PushLayer {
                    opacity: 0.5,
                    blend_mode: BlendMode::Screen,
                    clip: None,
                    transform: tiny_skia::Transform::identity(),
                },
                RenderCommand::PopLayer,
                RenderCommand::PopLayer,
            ],
        };
        let svg = SvgExporter::export_string(&scene).unwrap();
        assert!(svg.contains("opacity=\"0.8\""));
        // opacity=0.5 + blend=screen are combined into a single style attribute
        assert!(svg.contains("opacity:0.5;mix-blend-mode:screen"));
        // Two </g> closings
        assert_eq!(svg.matches("</g>").count(), 2);
    }

    #[test]
    fn export_to_file() {
        let scene = Scene {
            width: 10.0,
            height: 10.0,
            commands: vec![RenderCommand::FillPath {
                path: make_rect_path(10.0, 10.0),
                paint: ResolvedPaint::Solid(Color::black()),
                fill_rule: FillRule::NonZero,
                transform: tiny_skia::Transform::identity(),
            }],
        };
        let path = std::env::temp_dir().join("ode_test_svg_export.svg");
        SvgExporter::export(&scene, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("<?xml"));
        assert!(content.contains("<svg"));
        std::fs::remove_file(&path).ok();
    }
}
