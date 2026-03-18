use crate::output::{
    AddResponse, DeleteResponse, ErrorResponse, MoveResponse, SetResponse, Warning,
    EXIT_INPUT, EXIT_INTERNAL, EXIT_IO, EXIT_OK, print_json,
};
use ode_format::wire::{
    ContainerPropsWire, DocumentWire, FrameDataWire, GroupDataWire, ImageDataWire, NodeKindWire,
    NodeWire, TextDataWire, ViewKindWire,
};
use ode_format::{BlendMode, Color, Fill, LayoutDirection, LayoutPadding, Paint, SizingMode, Stroke, StyleValue, VisualProps};
use ode_format::node::{FillRule, Transform, VectorData};
use ode_format::style::{ImageSource, StrokeCap, StrokeJoin, StrokePosition};
use ode_format::typography::{LineHeight, TextAlign, TextSizingMode, TextStyle};

// ─── Shared load/save ───

fn load_wire(file: &str) -> Result<(String, DocumentWire), (i32, ErrorResponse)> {
    let json = crate::commands::load_input(file)?;
    let wire: DocumentWire = serde_json::from_str(&json).map_err(|e| {
        (
            EXIT_INPUT,
            ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()),
        )
    })?;
    Ok((file.to_string(), wire))
}

fn save_wire(file: &str, wire: &DocumentWire) -> Result<(), (i32, ErrorResponse)> {
    let json = serde_json::to_string_pretty(wire).map_err(|e| {
        (
            EXIT_INTERNAL,
            ErrorResponse::new("INTERNAL", "serialize", &e.to_string()),
        )
    })?;
    std::fs::write(file, &json).map_err(|e| {
        (
            EXIT_IO,
            ErrorResponse::new("IO_ERROR", "io", &e.to_string()),
        )
    })?;
    Ok(())
}

fn parse_color(s: &str) -> Result<Color, String> {
    Color::from_hex(s).ok_or_else(|| format!("invalid color: {s}"))
}

fn make_solid_fill(color: Color) -> Fill {
    Fill {
        paint: Paint::Solid {
            color: StyleValue::Raw(color),
        },
        opacity: StyleValue::Raw(1.0),
        blend_mode: BlendMode::Normal,
        visible: true,
    }
}

fn parse_corner_radius(s: &str) -> [f32; 4] {
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match parts.len() {
        1 => [parts[0]; 4],
        4 => [parts[0], parts[1], parts[2], parts[3]],
        _ => [0.0; 4],
    }
}

/// Shape name to default node name mapping.
fn shape_default_name(shape: &str) -> &str {
    match shape {
        "rect" => "Rectangle",
        "ellipse" => "Ellipse",
        "line" => "Line",
        "star" => "Star",
        "polygon" => "Polygon",
        _ => "Vector",
    }
}

// ─── ode add ───

#[allow(clippy::too_many_arguments)]
pub fn cmd_add(
    kind: &str,
    file: &str,
    name: Option<&str>,
    parent: Option<&str>,
    index: Option<usize>,
    width: Option<f32>,
    height: Option<f32>,
    fill: Option<&str>,
    corner_radius: Option<&str>,
    clips_content: Option<bool>,
    content: Option<&str>,
    font_size: Option<f32>,
    font_family: Option<&str>,
    shape: Option<&str>,
    sides: Option<u32>,
    src: Option<&str>,
) -> i32 {
    let (file_path, mut wire) = match load_wire(file) {
        Ok(v) => v,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    let stable_id = nanoid::nanoid!();

    // Parse optional fill color
    let fill_color = if let Some(fill_str) = fill {
        match parse_color(fill_str) {
            Ok(c) => Some(c),
            Err(msg) => {
                print_json(&ErrorResponse::new("INVALID_COLOR", "parse", &msg));
                return EXIT_INPUT;
            }
        }
    } else {
        None
    };

    // Build the node kind and determine the default name
    let (node_kind, default_name) = match kind {
        "frame" => {
            let w = match width {
                Some(v) => v,
                None => {
                    print_json(&ErrorResponse::new(
                        "MISSING_ARG",
                        "validate",
                        "frame requires --width and --height",
                    ));
                    return EXIT_INPUT;
                }
            };
            let h = match height {
                Some(v) => v,
                None => {
                    print_json(&ErrorResponse::new(
                        "MISSING_ARG",
                        "validate",
                        "frame requires --width and --height",
                    ));
                    return EXIT_INPUT;
                }
            };
            let cr = corner_radius.map(parse_corner_radius).unwrap_or([0.0; 4]);
            let clip = clips_content.unwrap_or(true);
            let mut visual = VisualProps::default();
            if let Some(color) = fill_color.clone() {
                visual.fills.push(make_solid_fill(color));
            }
            (
                NodeKindWire::Frame(FrameDataWire {
                    width: w,
                    height: h,
                    width_sizing: SizingMode::Fixed,
                    height_sizing: SizingMode::Fixed,
                    corner_radius: cr,
                    clips_content: clip,
                    visual,
                    container: ContainerPropsWire::default(),
                    component_def: None,
                }),
                "Frame",
            )
        }
        "group" => (
            NodeKindWire::Group(GroupDataWire {
                children: vec![],
            }),
            "Group",
        ),
        "text" => {
            let text_content = match content {
                Some(c) => c,
                None => {
                    print_json(&ErrorResponse::new(
                        "MISSING_ARG",
                        "validate",
                        "text requires --content",
                    ));
                    return EXIT_INPUT;
                }
            };
            let w = width.unwrap_or(100.0);
            let h = height.unwrap_or(100.0);
            let mut style = TextStyle::default();
            if let Some(fs) = font_size {
                style.font_size = StyleValue::Raw(fs);
            }
            if let Some(ff) = font_family {
                style.font_family = StyleValue::Raw(ff.to_string());
            }
            let mut visual = VisualProps::default();
            if let Some(color) = fill_color.clone() {
                visual.fills.push(make_solid_fill(color));
            }
            (
                NodeKindWire::Text(TextDataWire {
                    visual,
                    content: text_content.to_string(),
                    runs: vec![],
                    default_style: style,
                    width: w,
                    height: h,
                    sizing_mode: TextSizingMode::Fixed,
                }),
                "Text",
            )
        }
        "vector" => {
            let shape_name = match shape {
                Some(s) => s,
                None => {
                    print_json(&ErrorResponse::new(
                        "MISSING_ARG",
                        "validate",
                        "vector requires --shape (rect, ellipse, line, star, polygon)",
                    ));
                    return EXIT_INPUT;
                }
            };
            let w = width.unwrap_or(100.0);
            let h = height.unwrap_or(100.0);
            let cr = corner_radius.map(parse_corner_radius).unwrap_or([0.0; 4]);
            let path = match shape_name {
                "rect" => {
                    if cr.iter().any(|&r| r > 0.0) {
                        ode_format::shapes::rounded_rect(w, h, cr)
                    } else {
                        ode_format::shapes::rect(w, h)
                    }
                }
                "ellipse" => ode_format::shapes::ellipse(w, h),
                "line" => ode_format::shapes::line(w),
                "star" => ode_format::shapes::star(w.min(h)),
                "polygon" => ode_format::shapes::polygon(sides.unwrap_or(5), w.min(h)),
                other => {
                    print_json(&ErrorResponse::new(
                        "INVALID_SHAPE",
                        "validate",
                        &format!("unknown shape: {other}. Use: rect, ellipse, line, star, polygon"),
                    ));
                    return EXIT_INPUT;
                }
            };
            let mut visual = VisualProps::default();
            if let Some(color) = fill_color.clone() {
                visual.fills.push(make_solid_fill(color));
            }
            let default_n = shape_default_name(shape_name);
            (
                NodeKindWire::Vector(Box::new(VectorData {
                    visual,
                    path,
                    fill_rule: FillRule::default(),
                })),
                default_n,
            )
        }
        "image" => {
            let w = match width {
                Some(v) => v,
                None => {
                    print_json(&ErrorResponse::new(
                        "MISSING_ARG",
                        "validate",
                        "image requires --width and --height",
                    ));
                    return EXIT_INPUT;
                }
            };
            let h = match height {
                Some(v) => v,
                None => {
                    print_json(&ErrorResponse::new(
                        "MISSING_ARG",
                        "validate",
                        "image requires --width and --height",
                    ));
                    return EXIT_INPUT;
                }
            };
            let source = src.map(|p| ImageSource::Linked {
                path: p.to_string(),
            });
            let mut visual = VisualProps::default();
            if let Some(color) = fill_color.clone() {
                visual.fills.push(make_solid_fill(color));
            }
            (
                NodeKindWire::Image(ImageDataWire {
                    visual,
                    source,
                    width: w,
                    height: h,
                }),
                "Image",
            )
        }
        other => {
            print_json(&ErrorResponse::new(
                "INVALID_KIND",
                "validate",
                &format!(
                    "unknown node kind: {other}. Use: frame, group, text, vector, image"
                ),
            ));
            return EXIT_INPUT;
        }
    };

    let node_name = name.unwrap_or(default_name).to_string();

    let node = NodeWire {
        stable_id: stable_id.clone(),
        name: node_name.clone(),
        transform: Transform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        visible: true,
        constraints: None,
        layout_sizing: None,
        kind: node_kind,
    };

    wire.nodes.push(node);

    // Determine parent and insert into children
    let parent_label = match parent {
        Some("root") => {
            // Explicitly add to canvas root
            let pos = index.unwrap_or(wire.canvas.len());
            let pos = pos.min(wire.canvas.len());
            wire.canvas.insert(pos, stable_id.clone());
            "root".to_string()
        }
        Some(parent_id) => {
            // Find the specified parent node
            let parent_node = wire.find_node(parent_id);
            if parent_node.is_none() {
                print_json(&ErrorResponse::new(
                    "NOT_FOUND",
                    "validate",
                    &format!("parent node '{parent_id}' not found"),
                ));
                return EXIT_INPUT;
            }
            let parent_node = parent_node.unwrap();
            if !DocumentWire::is_container(&parent_node.kind) {
                print_json(&ErrorResponse::new(
                    "NOT_CONTAINER",
                    "validate",
                    &format!("node '{parent_id}' is not a container"),
                ));
                return EXIT_INPUT;
            }
            let parent_id_owned = parent_id.to_string();
            let parent_mut = wire.find_node_mut(&parent_id_owned).unwrap();
            let children = DocumentWire::children_of_kind_mut(&mut parent_mut.kind).unwrap();
            let pos = index.unwrap_or(children.len());
            let pos = pos.min(children.len());
            children.insert(pos, stable_id.clone());
            parent_id_owned
        }
        None => {
            if wire.canvas.is_empty() {
                // Empty canvas: add to canvas root
                wire.canvas.push(stable_id.clone());
                "root".to_string()
            } else {
                // Non-empty canvas: add as child of first canvas root
                let first_root_id = wire.canvas[0].clone();
                let first_root = wire.find_node(&first_root_id);
                if first_root.is_none()
                    || !DocumentWire::is_container(&first_root.unwrap().kind)
                {
                    // First canvas root is not a container, add to canvas
                    let pos = index.unwrap_or(wire.canvas.len());
                    let pos = pos.min(wire.canvas.len());
                    wire.canvas.insert(pos, stable_id.clone());
                    "root".to_string()
                } else {
                    let parent_mut = wire.find_node_mut(&first_root_id).unwrap();
                    let children =
                        DocumentWire::children_of_kind_mut(&mut parent_mut.kind).unwrap();
                    let pos = index.unwrap_or(children.len());
                    let pos = pos.min(children.len());
                    children.insert(pos, stable_id.clone());
                    first_root_id
                }
            }
        }
    };

    // Save
    if let Err((code, err)) = save_wire(&file_path, &wire) {
        print_json(&err);
        return code;
    }

    print_json(&AddResponse {
        status: "ok",
        stable_id,
        name: node_name,
        kind: kind.to_string(),
        parent: parent_label,
    });
    EXIT_OK
}

// ─── Parse helpers for ode set ───

fn parse_blend_mode(s: &str) -> Result<BlendMode, String> {
    match s {
        "normal" => Ok(BlendMode::Normal),
        "multiply" => Ok(BlendMode::Multiply),
        "screen" => Ok(BlendMode::Screen),
        "overlay" => Ok(BlendMode::Overlay),
        "darken" => Ok(BlendMode::Darken),
        "lighten" => Ok(BlendMode::Lighten),
        "color-dodge" => Ok(BlendMode::ColorDodge),
        "color-burn" => Ok(BlendMode::ColorBurn),
        "hard-light" => Ok(BlendMode::HardLight),
        "soft-light" => Ok(BlendMode::SoftLight),
        "difference" => Ok(BlendMode::Difference),
        "exclusion" => Ok(BlendMode::Exclusion),
        "hue" => Ok(BlendMode::Hue),
        "saturation" => Ok(BlendMode::Saturation),
        "color" => Ok(BlendMode::Color),
        "luminosity" => Ok(BlendMode::Luminosity),
        other => Err(format!("invalid blend mode: {other}")),
    }
}

fn parse_stroke_position(s: &str) -> Result<StrokePosition, String> {
    match s {
        "center" => Ok(StrokePosition::Center),
        "inside" => Ok(StrokePosition::Inside),
        "outside" => Ok(StrokePosition::Outside),
        other => Err(format!("invalid stroke position: {other}")),
    }
}

fn parse_padding(s: &str) -> Result<LayoutPadding, String> {
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match parts.len() {
        1 => Ok(LayoutPadding {
            top: parts[0],
            right: parts[0],
            bottom: parts[0],
            left: parts[0],
        }),
        4 => Ok(LayoutPadding {
            top: parts[0],
            right: parts[1],
            bottom: parts[2],
            left: parts[3],
        }),
        _ => Err(format!("invalid padding: expected 1 or 4 values, got '{s}'")),
    }
}

fn parse_text_align(s: &str) -> Result<TextAlign, String> {
    match s {
        "left" => Ok(TextAlign::Left),
        "center" => Ok(TextAlign::Center),
        "right" => Ok(TextAlign::Right),
        "justify" => Ok(TextAlign::Justify),
        other => Err(format!("invalid text-align: {other}")),
    }
}

fn parse_line_height(s: &str) -> Result<LineHeight, String> {
    if s == "auto" {
        return Ok(LineHeight::Auto);
    }
    match s.parse::<f32>() {
        Ok(v) => Ok(LineHeight::Percent {
            value: StyleValue::Raw(v),
        }),
        Err(_) => Err(format!("invalid line-height: {s}")),
    }
}

fn make_default_stroke(color: Color) -> Stroke {
    Stroke {
        paint: Paint::Solid {
            color: StyleValue::Raw(color),
        },
        width: StyleValue::Raw(1.0),
        position: StrokePosition::Center,
        cap: StrokeCap::Butt,
        join: StrokeJoin::Miter,
        miter_limit: 4.0,
        dash: None,
        opacity: StyleValue::Raw(1.0),
        blend_mode: BlendMode::Normal,
        visible: true,
    }
}

// ─── ode set ───

#[allow(clippy::too_many_arguments)]
pub fn cmd_set(
    file: &str,
    stable_id: &str,
    name: Option<&str>,
    visible: Option<bool>,
    opacity: Option<f32>,
    blend_mode: Option<&str>,
    x: Option<f32>,
    y: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
    fill: Option<&str>,
    fill_opacity: Option<f32>,
    stroke: Option<&str>,
    stroke_width: Option<f32>,
    stroke_position: Option<&str>,
    corner_radius: Option<&str>,
    clips_content: Option<bool>,
    layout: Option<&str>,
    padding: Option<&str>,
    gap: Option<f32>,
    content: Option<&str>,
    font_size: Option<f32>,
    font_family: Option<&str>,
    font_weight: Option<u16>,
    text_align: Option<&str>,
    line_height: Option<&str>,
) -> i32 {
    // Check that at least one property is specified
    let has_any = name.is_some()
        || visible.is_some()
        || opacity.is_some()
        || blend_mode.is_some()
        || x.is_some()
        || y.is_some()
        || width.is_some()
        || height.is_some()
        || fill.is_some()
        || fill_opacity.is_some()
        || stroke.is_some()
        || stroke_width.is_some()
        || stroke_position.is_some()
        || corner_radius.is_some()
        || clips_content.is_some()
        || layout.is_some()
        || padding.is_some()
        || gap.is_some()
        || content.is_some()
        || font_size.is_some()
        || font_family.is_some()
        || font_weight.is_some()
        || text_align.is_some()
        || line_height.is_some();

    if !has_any {
        print_json(&ErrorResponse::new(
            "NO_CHANGES",
            "validate",
            "no properties specified",
        ));
        return EXIT_INPUT;
    }

    let (file_path, mut wire) = match load_wire(file) {
        Ok(v) => v,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    // Find the node
    let node = match wire.find_node_mut(stable_id) {
        Some(n) => n,
        None => {
            print_json(&ErrorResponse::new(
                "NOT_FOUND",
                "validate",
                &format!("node '{stable_id}' not found"),
            ));
            return EXIT_INPUT;
        }
    };

    let mut modified: Vec<String> = Vec::new();

    // ── Common properties (all nodes) ──

    if let Some(n) = name {
        node.name = n.to_string();
        modified.push("name".to_string());
    }

    if let Some(v) = visible {
        node.visible = v;
        modified.push("visible".to_string());
    }

    if let Some(o) = opacity {
        node.opacity = o;
        modified.push("opacity".to_string());
    }

    if let Some(bm_str) = blend_mode {
        match parse_blend_mode(bm_str) {
            Ok(bm) => {
                node.blend_mode = bm;
                modified.push("blend-mode".to_string());
            }
            Err(msg) => {
                print_json(&ErrorResponse::new("INVALID_VALUE", "validate", &msg));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(xv) = x {
        node.transform.tx = xv;
        modified.push("x".to_string());
    }

    if let Some(yv) = y {
        node.transform.ty = yv;
        modified.push("y".to_string());
    }

    // ── Size properties (frame, text, image) ──

    if let Some(w) = width {
        match &mut node.kind {
            NodeKindWire::Frame(d) => {
                d.width = w;
                modified.push("width".to_string());
            }
            NodeKindWire::Text(d) => {
                d.width = w;
                modified.push("width".to_string());
            }
            NodeKindWire::Image(d) => {
                d.width = w;
                modified.push("width".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "width is only valid for frame, text, or image nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(h) = height {
        match &mut node.kind {
            NodeKindWire::Frame(d) => {
                d.height = h;
                modified.push("height".to_string());
            }
            NodeKindWire::Text(d) => {
                d.height = h;
                modified.push("height".to_string());
            }
            NodeKindWire::Image(d) => {
                d.height = h;
                modified.push("height".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "height is only valid for frame, text, or image nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    // ── Visual properties (frame, vector, text, image, boolean-op — NOT group, NOT instance) ──

    if let Some(fill_str) = fill {
        let color = match parse_color(fill_str) {
            Ok(c) => c,
            Err(msg) => {
                print_json(&ErrorResponse::new("INVALID_VALUE", "validate", &msg));
                return EXIT_INPUT;
            }
        };
        match DocumentWire::visual_props_mut(&mut node.kind) {
            Some(visual) => {
                let new_fill = make_solid_fill(color);
                if visual.fills.is_empty() {
                    visual.fills.push(new_fill);
                } else {
                    visual.fills[0] = new_fill;
                }
                modified.push("fill".to_string());
            }
            None => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "fill is not valid for this node type",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(fo) = fill_opacity {
        match DocumentWire::visual_props_mut(&mut node.kind) {
            Some(visual) => {
                if let Some(first_fill) = visual.fills.first_mut() {
                    first_fill.opacity = StyleValue::Raw(fo);
                }
                modified.push("fill-opacity".to_string());
            }
            None => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "fill-opacity is not valid for this node type",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(stroke_str) = stroke {
        let color = match parse_color(stroke_str) {
            Ok(c) => c,
            Err(msg) => {
                print_json(&ErrorResponse::new("INVALID_VALUE", "validate", &msg));
                return EXIT_INPUT;
            }
        };
        match DocumentWire::visual_props_mut(&mut node.kind) {
            Some(visual) => {
                if visual.strokes.is_empty() {
                    visual.strokes.push(make_default_stroke(color));
                } else {
                    visual.strokes[0].paint = Paint::Solid {
                        color: StyleValue::Raw(color),
                    };
                }
                modified.push("stroke".to_string());
            }
            None => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "stroke is not valid for this node type",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(sw) = stroke_width {
        match DocumentWire::visual_props_mut(&mut node.kind) {
            Some(visual) => {
                if let Some(first_stroke) = visual.strokes.first_mut() {
                    first_stroke.width = StyleValue::Raw(sw);
                }
                modified.push("stroke-width".to_string());
            }
            None => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "stroke-width is not valid for this node type",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(sp_str) = stroke_position {
        let sp = match parse_stroke_position(sp_str) {
            Ok(v) => v,
            Err(msg) => {
                print_json(&ErrorResponse::new("INVALID_VALUE", "validate", &msg));
                return EXIT_INPUT;
            }
        };
        match DocumentWire::visual_props_mut(&mut node.kind) {
            Some(visual) => {
                if let Some(first_stroke) = visual.strokes.first_mut() {
                    first_stroke.position = sp;
                }
                modified.push("stroke-position".to_string());
            }
            None => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "stroke-position is not valid for this node type",
                ));
                return EXIT_INPUT;
            }
        }
    }

    // ── Frame-specific properties ──

    if let Some(cr_str) = corner_radius {
        match &mut node.kind {
            NodeKindWire::Frame(d) => {
                d.corner_radius = parse_corner_radius(cr_str);
                modified.push("corner-radius".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "corner-radius is only valid for frame nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(cc) = clips_content {
        match &mut node.kind {
            NodeKindWire::Frame(d) => {
                d.clips_content = cc;
                modified.push("clips-content".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "clips-content is only valid for frame nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(layout_str) = layout {
        match &mut node.kind {
            NodeKindWire::Frame(d) => {
                let direction = match layout_str {
                    "horizontal" => LayoutDirection::Horizontal,
                    "vertical" => LayoutDirection::Vertical,
                    other => {
                        print_json(&ErrorResponse::new(
                            "INVALID_VALUE",
                            "validate",
                            &format!("invalid layout direction: {other}"),
                        ));
                        return EXIT_INPUT;
                    }
                };
                let mut config = d.container.layout.take().unwrap_or_default();
                config.direction = direction;
                d.container.layout = Some(config);
                modified.push("layout".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "layout is only valid for frame nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(pad_str) = padding {
        match &mut node.kind {
            NodeKindWire::Frame(d) => {
                let pad = match parse_padding(pad_str) {
                    Ok(p) => p,
                    Err(msg) => {
                        print_json(&ErrorResponse::new("INVALID_VALUE", "validate", &msg));
                        return EXIT_INPUT;
                    }
                };
                let mut config = d.container.layout.take().unwrap_or_default();
                config.padding = pad;
                d.container.layout = Some(config);
                modified.push("padding".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "padding is only valid for frame nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(g) = gap {
        match &mut node.kind {
            NodeKindWire::Frame(d) => {
                let mut config = d.container.layout.take().unwrap_or_default();
                config.item_spacing = g;
                d.container.layout = Some(config);
                modified.push("gap".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "gap is only valid for frame nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    // ── Text-specific properties ──

    if let Some(c) = content {
        match &mut node.kind {
            NodeKindWire::Text(d) => {
                d.content = c.to_string();
                modified.push("content".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "content is only valid for text nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(fs) = font_size {
        match &mut node.kind {
            NodeKindWire::Text(d) => {
                d.default_style.font_size = StyleValue::Raw(fs);
                modified.push("font-size".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "font-size is only valid for text nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(ff) = font_family {
        match &mut node.kind {
            NodeKindWire::Text(d) => {
                d.default_style.font_family = StyleValue::Raw(ff.to_string());
                modified.push("font-family".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "font-family is only valid for text nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(fw) = font_weight {
        match &mut node.kind {
            NodeKindWire::Text(d) => {
                d.default_style.font_weight = StyleValue::Raw(fw);
                modified.push("font-weight".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "font-weight is only valid for text nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(ta_str) = text_align {
        match &mut node.kind {
            NodeKindWire::Text(d) => {
                let ta = match parse_text_align(ta_str) {
                    Ok(v) => v,
                    Err(msg) => {
                        print_json(&ErrorResponse::new("INVALID_VALUE", "validate", &msg));
                        return EXIT_INPUT;
                    }
                };
                d.default_style.text_align = ta;
                modified.push("text-align".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "text-align is only valid for text nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    if let Some(lh_str) = line_height {
        match &mut node.kind {
            NodeKindWire::Text(d) => {
                let lh = match parse_line_height(lh_str) {
                    Ok(v) => v,
                    Err(msg) => {
                        print_json(&ErrorResponse::new("INVALID_VALUE", "validate", &msg));
                        return EXIT_INPUT;
                    }
                };
                d.default_style.line_height = lh;
                modified.push("line-height".to_string());
            }
            _ => {
                print_json(&ErrorResponse::new(
                    "INVALID_PROPERTY",
                    "validate",
                    "line-height is only valid for text nodes",
                ));
                return EXIT_INPUT;
            }
        }
    }

    // Save
    if let Err((code, err)) = save_wire(&file_path, &wire) {
        print_json(&err);
        return code;
    }

    print_json(&SetResponse {
        status: "ok",
        stable_id: stable_id.to_string(),
        modified,
    });
    EXIT_OK
}

// ─── ode move ───

pub fn cmd_move(file: &str, stable_id: &str, parent: &str, index: Option<usize>) -> i32 {
    let (file_path, mut wire) = match load_wire(file) {
        Ok(v) => v,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    // Step 1: verify source node exists
    if wire.find_node(stable_id).is_none() {
        print_json(&ErrorResponse::new(
            "NOT_FOUND",
            "validate",
            &format!("node '{stable_id}' not found"),
        ));
        return EXIT_INPUT;
    }

    // Step 2: cycle detection (skip if target is "root")
    if parent != "root" {
        // Also verify target node exists
        if wire.find_node(parent).is_none() {
            print_json(&ErrorResponse::new(
                "NOT_FOUND",
                "validate",
                &format!("target node '{parent}' not found"),
            ));
            return EXIT_INPUT;
        }

        // Check that parent is NOT a descendant of source (and is not source itself)
        if parent == stable_id {
            print_json(&ErrorResponse::new(
                "CYCLE_DETECTED",
                "validate",
                "cannot move a node into itself",
            ));
            return EXIT_INPUT;
        }
        let descendants = wire.collect_descendants(stable_id);
        if descendants.iter().any(|d| d == parent) {
            print_json(&ErrorResponse::new(
                "CYCLE_DETECTED",
                "validate",
                &format!("cannot move node '{stable_id}' into its own descendant '{parent}'"),
            ));
            return EXIT_INPUT;
        }

        // Verify target is a container
        let target_is_container = wire
            .find_node(parent)
            .map(|n| DocumentWire::is_container(&n.kind))
            .unwrap_or(false);
        if !target_is_container {
            print_json(&ErrorResponse::new(
                "NOT_CONTAINER",
                "validate",
                &format!("target node '{parent}' is not a container"),
            ));
            return EXIT_INPUT;
        }
    }

    // Step 3: remove source from old parent
    wire.remove_child_from_parent(stable_id);
    wire.canvas.retain(|c| c != stable_id);

    // Step 4: insert into new parent
    let final_index = if parent == "root" {
        let pos = index.unwrap_or(wire.canvas.len()).min(wire.canvas.len());
        wire.canvas.insert(pos, stable_id.to_string());
        pos
    } else {
        let parent_id_owned = parent.to_string();
        let parent_mut = wire.find_node_mut(&parent_id_owned).unwrap();
        let children = DocumentWire::children_of_kind_mut(&mut parent_mut.kind).unwrap();
        let pos = index.unwrap_or(children.len()).min(children.len());
        children.insert(pos, stable_id.to_string());
        pos
    };

    // Save
    if let Err((code, err)) = save_wire(&file_path, &wire) {
        print_json(&err);
        return code;
    }

    print_json(&MoveResponse {
        status: "ok",
        stable_id: stable_id.to_string(),
        new_parent: parent.to_string(),
        index: final_index,
    });
    EXIT_OK
}

// ─── ode delete ───

pub fn cmd_delete(file: &str, stable_id: &str) -> i32 {
    let (file_path, mut wire) = match load_wire(file) {
        Ok(v) => v,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    // Step 1: verify node exists
    if wire.find_node(stable_id).is_none() {
        print_json(&ErrorResponse::new(
            "NOT_FOUND",
            "validate",
            &format!("node '{stable_id}' not found"),
        ));
        return EXIT_INPUT;
    }

    // Step 2: collect all descendant stable_ids
    let descendants = wire.collect_descendants(stable_id);

    // Step 3: build to_delete list
    let mut to_delete: Vec<String> = Vec::with_capacity(1 + descendants.len());
    to_delete.push(stable_id.to_string());
    to_delete.extend(descendants);

    let to_delete_set: std::collections::HashSet<&str> =
        to_delete.iter().map(|s| s.as_str()).collect();

    let mut warnings: Vec<Warning> = Vec::new();

    // Step 4: remove stable_id from parent's children
    wire.remove_child_from_parent(stable_id);

    // Step 5: remove from canvas if it's a root
    wire.canvas.retain(|c| c != stable_id);

    // Step 6: clean up view references
    wire.views.retain_mut(|view| {
        match &mut view.kind {
            ViewKindWire::Print { pages } => {
                let before = pages.len();
                pages.retain(|p| !to_delete_set.contains(p.as_str()));
                if pages.len() < before {
                    warnings.push(Warning {
                        path: format!("views/{}", view.id.0),
                        code: "VIEW_PAGES_PRUNED".to_string(),
                        message: format!(
                            "Print view '{}' had {} page(s) removed because their nodes were deleted",
                            view.name,
                            before - pages.len()
                        ),
                    });
                }
                true // keep the view even if empty
            }
            ViewKindWire::Web { root } => {
                if to_delete_set.contains(root.as_str()) {
                    warnings.push(Warning {
                        path: format!("views/{}", view.id.0),
                        code: "VIEW_ROOT_DELETED".to_string(),
                        message: format!(
                            "Web view '{}' removed because its root node was deleted",
                            view.name
                        ),
                    });
                    false // remove the entire view
                } else {
                    true
                }
            }
            ViewKindWire::Presentation { slides } => {
                let before = slides.len();
                slides.retain(|s| !to_delete_set.contains(s.as_str()));
                if slides.len() < before {
                    warnings.push(Warning {
                        path: format!("views/{}", view.id.0),
                        code: "VIEW_SLIDES_PRUNED".to_string(),
                        message: format!(
                            "Presentation view '{}' had {} slide(s) removed because their nodes were deleted",
                            view.name,
                            before - slides.len()
                        ),
                    });
                }
                true
            }
            ViewKindWire::Export { .. } => true, // no node references to clean up
        }
    });

    // Step 7: check for dangling instance references
    for node in &wire.nodes {
        if to_delete_set.contains(node.stable_id.as_str()) {
            continue; // will be removed anyway
        }
        if let NodeKindWire::Instance(inst) = &node.kind {
            if to_delete_set.contains(inst.source_component.as_str()) {
                warnings.push(Warning {
                    path: format!("nodes/{}", node.stable_id),
                    code: "DANGLING_INSTANCE".to_string(),
                    message: format!(
                        "Instance node '{}' references deleted component '{}'",
                        node.stable_id, inst.source_component
                    ),
                });
            }
        }
    }

    // Step 8: remove all nodes in to_delete
    wire.nodes.retain(|n| !to_delete_set.contains(n.stable_id.as_str()));

    // Step 9: save and print
    if let Err((code, err)) = save_wire(&file_path, &wire) {
        print_json(&err);
        return code;
    }

    print_json(&DeleteResponse {
        status: "ok",
        deleted: to_delete,
        warnings,
    });
    EXIT_OK
}
