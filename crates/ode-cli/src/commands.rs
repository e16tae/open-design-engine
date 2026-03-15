use crate::output::*;
use crate::validate::validate_json;
use ode_core::{FontDatabase, Renderer, Scene};
use ode_export::{PdfExporter, PngExporter, SvgExporter};
use ode_format::Document;
use ode_format::wire::DocumentWire;
use std::path::Path;

enum ExportFormat {
    Png,
    Svg,
    Pdf,
}

fn detect_format(output: &str, format_flag: Option<&str>) -> ExportFormat {
    if let Some(f) = format_flag {
        return match f.to_lowercase().as_str() {
            "svg" => ExportFormat::Svg,
            "pdf" => ExportFormat::Pdf,
            _ => ExportFormat::Png,
        };
    }
    if output.ends_with(".svg") {
        ExportFormat::Svg
    } else if output.ends_with(".pdf") {
        ExportFormat::Pdf
    } else {
        ExportFormat::Png
    }
}

#[allow(clippy::result_large_err)]
pub fn load_input(file: &str) -> Result<String, (i32, ErrorResponse)> {
    if file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf).map_err(|e| {
            (
                EXIT_IO,
                ErrorResponse::new("IO_ERROR", "io", &e.to_string()),
            )
        })?;
        Ok(buf)
    } else {
        std::fs::read_to_string(file).map_err(|e| {
            (
                EXIT_IO,
                ErrorResponse::new("IO_ERROR", "io", &format!("failed to read '{file}': {e}")),
            )
        })
    }
}

// ─── ode new ───

pub fn cmd_new(file: &str, name: Option<&str>, width: Option<f32>, height: Option<f32>) -> i32 {
    let mut doc = Document::new(name.unwrap_or("Untitled"));

    if let (Some(w), Some(h)) = (width, height) {
        let frame = ode_format::node::Node::new_frame("Root", w, h);
        let id = doc.nodes.insert(frame);
        doc.canvas.push(id);
    }

    let json = match serde_json::to_string_pretty(&doc) {
        Ok(j) => j,
        Err(e) => {
            print_json(&ErrorResponse::new("INTERNAL", "serialize", &e.to_string()));
            return EXIT_INTERNAL;
        }
    };

    if let Err(e) = std::fs::write(file, &json) {
        print_json(&ErrorResponse::new("IO_ERROR", "io", &e.to_string()));
        return EXIT_IO;
    }

    print_json(&OkResponse::with_path(file));
    EXIT_OK
}

// ─── ode validate ───

pub fn cmd_validate(file: &str) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    let result = validate_json(&json);
    let exit = if result.valid { EXIT_OK } else { EXIT_INPUT };
    print_json(&result);
    exit
}

// ─── ode build ───

pub fn cmd_build(file: &str, output: &str, format: Option<&str>) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    let validation = validate_json(&json);
    if !validation.valid {
        print_json(&ErrorResponse::validation(validation.errors));
        return EXIT_INPUT;
    }

    let doc: Document = match serde_json::from_str(&json) {
        Ok(d) => d,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
            return EXIT_INPUT;
        }
    };

    render_and_export(&doc, output, format, validation.warnings)
}

// ─── ode render ───

pub fn cmd_render(file: &str, output: &str, format: Option<&str>) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    let doc: Document = match serde_json::from_str(&json) {
        Ok(d) => d,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
            return EXIT_INPUT;
        }
    };

    render_and_export(&doc, output, format, vec![])
}

fn render_and_export(
    doc: &Document,
    output: &str,
    format: Option<&str>,
    warnings: Vec<Warning>,
) -> i32 {
    let font_db = FontDatabase::new_system();
    let scene = match Scene::from_document(doc, &font_db) {
        Ok(s) => s,
        Err(e) => {
            print_json(&ErrorResponse::new(
                "RENDER_FAILED",
                "render",
                &e.to_string(),
            ));
            return EXIT_PROCESS;
        }
    };

    match detect_format(output, format) {
        ExportFormat::Svg => {
            // SVG: Scene IR → SVG directly (skip rasterization)
            if let Err(e) = SvgExporter::export(&scene, Path::new(output)) {
                print_json(&ErrorResponse::new(
                    "EXPORT_FAILED",
                    "export",
                    &e.to_string(),
                ));
                return EXIT_PROCESS;
            }
            let mut resp = OkResponse::with_render(output, scene.width as u32, scene.height as u32);
            resp.warnings = warnings;
            print_json(&resp);
            EXIT_OK
        }
        ExportFormat::Pdf => {
            // PDF: Scene IR → PDF directly (skip rasterization)
            if let Err(e) = PdfExporter::export(&scene, Path::new(output)) {
                print_json(&ErrorResponse::new(
                    "EXPORT_FAILED",
                    "export",
                    &e.to_string(),
                ));
                return EXIT_PROCESS;
            }
            let mut resp = OkResponse::with_render(output, scene.width as u32, scene.height as u32);
            resp.warnings = warnings;
            print_json(&resp);
            EXIT_OK
        }
        ExportFormat::Png => {
            // PNG: Scene IR → Renderer → Pixmap → PNG
            let pixmap = match Renderer::render(&scene) {
                Ok(p) => p,
                Err(e) => {
                    print_json(&ErrorResponse::new(
                        "RENDER_FAILED",
                        "render",
                        &e.to_string(),
                    ));
                    return EXIT_PROCESS;
                }
            };
            if let Err(e) = PngExporter::export(&pixmap, Path::new(output)) {
                print_json(&ErrorResponse::new(
                    "EXPORT_FAILED",
                    "export",
                    &e.to_string(),
                ));
                return EXIT_PROCESS;
            }
            let mut resp = OkResponse::with_render(output, pixmap.width(), pixmap.height());
            resp.warnings = warnings;
            print_json(&resp);
            EXIT_OK
        }
    }
}

// ─── ode inspect ───

pub fn cmd_inspect(file: &str, full: bool) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    let wire: DocumentWire = match serde_json::from_str(&json) {
        Ok(w) => w,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
            return EXIT_INPUT;
        }
    };

    if full {
        print_json(&wire);
    } else {
        let summary = build_inspect_summary(&wire);
        print_json(&summary);
    }
    EXIT_OK
}

#[derive(serde::Serialize)]
struct InspectSummary {
    name: String,
    format_version: String,
    working_color_space: String,
    node_count: usize,
    canvas: Vec<String>,
    tree: Vec<InspectNode>,
    tokens: TokensSummary,
}

#[derive(serde::Serialize)]
struct InspectNode {
    stable_id: String,
    name: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<InspectNode>,
}

#[derive(serde::Serialize)]
struct TokensSummary {
    collections: Vec<String>,
    total_tokens: usize,
}

fn build_inspect_summary(wire: &DocumentWire) -> InspectSummary {
    use std::collections::HashMap;
    let node_map: HashMap<&str, &ode_format::wire::NodeWire> = wire
        .nodes
        .iter()
        .map(|n| (n.stable_id.as_str(), n))
        .collect();

    let tree = wire
        .canvas
        .iter()
        .filter_map(|id| {
            node_map
                .get(id.as_str())
                .map(|n| build_tree_node(n, &node_map))
        })
        .collect();

    InspectSummary {
        name: wire.name.clone(),
        format_version: format!(
            "{}.{}.{}",
            wire.format_version.0, wire.format_version.1, wire.format_version.2
        ),
        working_color_space: serde_json::to_value(wire.working_color_space)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default(),
        node_count: wire.nodes.len(),
        canvas: wire.canvas.clone(),
        tree,
        tokens: TokensSummary {
            collections: wire
                .tokens
                .collections
                .iter()
                .map(|c| c.name.clone())
                .collect(),
            total_tokens: wire.tokens.collections.iter().map(|c| c.tokens.len()).sum(),
        },
    }
}

fn build_tree_node(
    node: &ode_format::wire::NodeWire,
    node_map: &std::collections::HashMap<&str, &ode_format::wire::NodeWire>,
) -> InspectNode {
    use ode_format::wire::NodeKindWire;
    let (kind, size, child_ids) = match &node.kind {
        NodeKindWire::Frame(d) => (
            "frame",
            Some([d.width, d.height]),
            d.container
                .children
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        ),
        NodeKindWire::Group(d) => (
            "group",
            None,
            d.children.iter().map(|s| s.as_str()).collect(),
        ),
        NodeKindWire::Vector(_) => ("vector", None, vec![]),
        NodeKindWire::BooleanOp(d) => (
            "boolean-op",
            None,
            d.children.iter().map(|s| s.as_str()).collect(),
        ),
        NodeKindWire::Text(_) => ("text", None, vec![]),
        NodeKindWire::Image(_) => ("image", None, vec![]),
        NodeKindWire::Instance(d) => (
            "instance",
            None,
            d.container.children.iter().map(|s| s.as_str()).collect(),
        ),
    };

    let children = child_ids
        .iter()
        .filter_map(|id| node_map.get(id).map(|n| build_tree_node(n, node_map)))
        .collect();

    InspectNode {
        stable_id: node.stable_id.clone(),
        name: node.name.clone(),
        kind: kind.to_string(),
        size,
        children,
    }
}

// ─── ode import figma ───

pub fn cmd_import_figma(
    token: Option<String>,
    file_key: Option<String>,
    input: Option<String>,
    output: &str,
    with_variables: bool,
    _skip_images: bool,
) -> i32 {
    use ode_import::figma::convert::FigmaConverter;
    use ode_import::figma::types::{FigmaFileResponse, FigmaVariablesResponse};
    use std::collections::HashMap;

    // Load Figma file data
    let (file_response, variables): (FigmaFileResponse, Option<FigmaVariablesResponse>) =
        if let Some(input_path) = input {
            // Local JSON file mode
            let json_str = match std::fs::read_to_string(&input_path) {
                Ok(s) => s,
                Err(e) => {
                    print_json(&ErrorResponse::new(
                        "IO_ERROR",
                        "io",
                        &format!("Failed to read input file: {e}"),
                    ));
                    return EXIT_IO;
                }
            };
            let file: FigmaFileResponse = match serde_json::from_str(&json_str) {
                Ok(f) => f,
                Err(e) => {
                    print_json(&ErrorResponse::new(
                        "PARSE_FAILED",
                        "parse",
                        &format!("Failed to parse Figma JSON: {e}"),
                    ));
                    return EXIT_INPUT;
                }
            };
            // Variables from local file: not supported (need separate API call)
            (file, None)
        } else if let (Some(token), Some(file_key)) = (token, file_key) {
            // API mode
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    print_json(&ErrorResponse::new(
                        "INTERNAL",
                        "runtime",
                        &format!("Failed to create async runtime: {e}"),
                    ));
                    return EXIT_INTERNAL;
                }
            };
            let client = ode_import::figma::client::FigmaClient::new(token);
            let file = match rt.block_on(client.get_file(&file_key)) {
                Ok(f) => f,
                Err(e) => {
                    print_json(&ErrorResponse::new(
                        "API_ERROR",
                        "api",
                        &format!("Failed to fetch Figma file: {e}"),
                    ));
                    return EXIT_IO;
                }
            };
            let variables = if with_variables {
                match rt.block_on(client.get_variables(&file_key)) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        eprintln!("Warning: Failed to fetch variables: {e}");
                        None
                    }
                }
            } else {
                None
            };
            (file, variables)
        } else {
            print_json(&ErrorResponse::new(
                "INVALID_ARGS",
                "args",
                "Either --input or both --token and --file-key are required",
            ));
            return EXIT_INPUT;
        };

    // Convert
    let result = match FigmaConverter::convert(file_response, variables, HashMap::new()) {
        Ok(r) => r,
        Err(e) => {
            print_json(&ErrorResponse::new(
                "CONVERT_FAILED",
                "convert",
                &format!("Conversion failed: {e}"),
            ));
            return EXIT_PROCESS;
        }
    };

    // Collect warnings
    let warnings: Vec<Warning> = result
        .warnings
        .iter()
        .map(|w| Warning {
            path: w.node_id.clone(),
            code: "IMPORT_WARNING".to_string(),
            message: format!("{}: {}", w.node_name, w.message),
        })
        .collect();

    // Serialize and write output
    let json = match serde_json::to_string_pretty(&result.document) {
        Ok(j) => j,
        Err(e) => {
            print_json(&ErrorResponse::new(
                "INTERNAL",
                "serialize",
                &format!("Failed to serialize document: {e}"),
            ));
            return EXIT_INTERNAL;
        }
    };

    match std::fs::write(output, &json) {
        Ok(_) => {
            let mut resp = OkResponse::with_path(output);
            resp.warnings = warnings;
            print_json(&resp);
            EXIT_OK
        }
        Err(e) => {
            print_json(&ErrorResponse::new(
                "IO_ERROR",
                "io",
                &format!("Failed to write output: {e}"),
            ));
            EXIT_IO
        }
    }
}

// ─── ode schema ───

pub fn cmd_schema(topic: Option<&str>) -> i32 {
    let schema = match topic {
        None | Some("document") => schemars::schema_for!(DocumentWire),
        Some("node") => schemars::schema_for!(ode_format::wire::NodeWire),
        Some("paint") => schemars::schema_for!(ode_format::style::Paint),
        Some("token") => schemars::schema_for!(ode_format::tokens::DesignTokens),
        Some("color") => schemars::schema_for!(ode_format::color::Color),
        Some(unknown) => {
            print_json(&ErrorResponse::new(
                "INVALID_TOPIC",
                "schema",
                &format!(
                    "unknown schema topic '{unknown}'. Available: document, node, paint, token, color"
                ),
            ));
            return EXIT_INPUT;
        }
    };

    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    EXIT_OK
}
