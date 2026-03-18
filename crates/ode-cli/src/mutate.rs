use crate::output::*;
use ode_format::wire::{
    ContainerPropsWire, DocumentWire, FrameDataWire, GroupDataWire, ImageDataWire, NodeKindWire,
    NodeWire, TextDataWire,
};
use ode_format::{BlendMode, Color, Fill, Paint, SizingMode, StyleValue, VisualProps};
use ode_format::node::{FillRule, Transform, VectorData};
use ode_format::style::ImageSource;
use ode_format::typography::{TextSizingMode, TextStyle};

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
