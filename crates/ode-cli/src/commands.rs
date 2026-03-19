use crate::output::*;
use crate::validate::validate_json;
use ode_core::{FontDatabase, Renderer, Scene};
use ode_export::{PdfExporter, PngExporter, SvgExporter};
use ode_format::asset::AssetStore;
use ode_format::container::{ContainerError, OdeContainer, OdeSource};
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

/// Open any supported input as an `OdeContainer`.
///
/// Handles packed `.ode`, unpacked directories, legacy `.ode.json`, and stdin.
#[allow(clippy::result_large_err)]
fn open_container(file: &str) -> Result<OdeContainer, (i32, ErrorResponse)> {
    OdeContainer::open(file).map_err(|e| {
        (
            EXIT_IO,
            ErrorResponse::new("IO_ERROR", "io", &format!("failed to open '{file}': {e}")),
        )
    })
}

/// Load document JSON string from any supported input format.
///
/// For directories and `.ode` files, extracts `document.json`.
/// For legacy `.ode.json` files and stdin, reads the file directly.
#[allow(clippy::result_large_err)]
fn load_document_json(file: &str) -> Result<String, (i32, ErrorResponse)> {
    match OdeSource::detect(file) {
        OdeSource::Stdin | OdeSource::LegacyJson(_) => load_input(file),
        OdeSource::Unpacked(dir) => {
            let doc_path = dir.join("document.json");
            std::fs::read_to_string(&doc_path).map_err(|e| {
                (
                    EXIT_IO,
                    ErrorResponse::new(
                        "IO_ERROR",
                        "io",
                        &format!("failed to read document.json from '{}': {e}", dir.display()),
                    ),
                )
            })
        }
        OdeSource::Packed(path) => {
            let reader = std::fs::File::open(&path).map_err(|e| {
                (
                    EXIT_IO,
                    ErrorResponse::new(
                        "IO_ERROR",
                        "io",
                        &format!("failed to open '{}': {e}", path.display()),
                    ),
                )
            })?;
            let mut archive = zip::ZipArchive::new(reader).map_err(|e| {
                (
                    EXIT_IO,
                    ErrorResponse::new(
                        "IO_ERROR",
                        "io",
                        &format!("failed to read ZIP '{}': {e}", path.display()),
                    ),
                )
            })?;
            let mut entry = archive.by_name("document.json").map_err(|e| {
                (
                    EXIT_IO,
                    ErrorResponse::new(
                        "IO_ERROR",
                        "io",
                        &format!("document.json not found in '{}': {e}", path.display()),
                    ),
                )
            })?;
            let mut buf = String::new();
            use std::io::Read;
            entry.read_to_string(&mut buf).map_err(|e| {
                (
                    EXIT_IO,
                    ErrorResponse::new(
                        "IO_ERROR",
                        "io",
                        &format!("failed to read document.json from '{}': {e}", path.display()),
                    ),
                )
            })?;
            Ok(buf)
        }
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

    let mut container = OdeContainer::from_document(doc, "ode-cli");
    let path = Path::new(file);

    let result = if file.ends_with('/') || path.is_dir() {
        container.save_unpacked(path)
    } else if file.ends_with(".ode") && !file.ends_with(".ode.json") {
        container.save_packed(path)
    } else {
        // Legacy .ode.json or other — write plain JSON
        let json = match serde_json::to_string_pretty(&container.document) {
            Ok(j) => j,
            Err(e) => {
                print_json(&ErrorResponse::new("INTERNAL", "serialize", &e.to_string()));
                return EXIT_INTERNAL;
            }
        };
        std::fs::write(file, json).map_err(ContainerError::Io)
    };

    match result {
        Ok(()) => {
            print_json(&OkResponse::with_path(file));
            EXIT_OK
        }
        Err(e) => {
            print_json(&ErrorResponse::new("IO_ERROR", "io", &e.to_string()));
            EXIT_IO
        }
    }
}

// ─── ode validate ───

pub fn cmd_validate(file: &str) -> i32 {
    let json = match load_document_json(file) {
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

pub fn cmd_build(file: &str, output: &str, format: Option<&str>, resize: Option<&str>) -> i32 {
    let mut container = match open_container(file) {
        Ok(c) => c,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    // Validate the document JSON
    let json = match serde_json::to_string_pretty(&container.document) {
        Ok(j) => j,
        Err(e) => {
            print_json(&ErrorResponse::new("INTERNAL", "serialize", &e.to_string()));
            return EXIT_INTERNAL;
        }
    };
    let validation = validate_json(&json);
    if !validation.valid {
        print_json(&ErrorResponse::validation(validation.errors));
        return EXIT_INPUT;
    }

    // Preload assets before rendering
    if let Err(e) = container.assets.preload_all() {
        print_json(&ErrorResponse::new("IO_ERROR", "io", &e.to_string()));
        return EXIT_IO;
    }

    render_and_export(&container.document, &container.assets, output, format, validation.warnings, resize)
}

// ─── ode render ───

pub fn cmd_render(file: &str, output: &str, format: Option<&str>, resize: Option<&str>) -> i32 {
    let mut container = match open_container(file) {
        Ok(c) => c,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    // Preload assets before rendering
    if let Err(e) = container.assets.preload_all() {
        print_json(&ErrorResponse::new("IO_ERROR", "io", &e.to_string()));
        return EXIT_IO;
    }

    render_and_export(&container.document, &container.assets, output, format, vec![], resize)
}

fn render_and_export(
    doc: &Document,
    assets: &AssetStore,
    output: &str,
    format: Option<&str>,
    warnings: Vec<Warning>,
    resize: Option<&str>,
) -> i32 {
    let font_db = FontDatabase::new_system();

    let scene = if let Some(resize_str) = resize {
        let parts: Vec<&str> = resize_str.split('x').collect();
        if parts.len() != 2 {
            print_json(&ErrorResponse::new(
                "INVALID_RESIZE",
                "parse",
                "resize must be in WxH format (e.g., 1920x1080)",
            ));
            return EXIT_INPUT;
        }
        let w: f32 = match parts[0].parse() {
            Ok(v) => v,
            Err(_) => {
                print_json(&ErrorResponse::new(
                    "INVALID_RESIZE",
                    "parse",
                    "invalid width in resize",
                ));
                return EXIT_INPUT;
            }
        };
        let h: f32 = match parts[1].parse() {
            Ok(v) => v,
            Err(_) => {
                print_json(&ErrorResponse::new(
                    "INVALID_RESIZE",
                    "parse",
                    "invalid height in resize",
                ));
                return EXIT_INPUT;
            }
        };

        let mut resize_map = ode_core::ResizeMap::new();
        if let Some(&root_id) = doc.canvas.first() {
            resize_map.insert(root_id, (w, h));
        }

        match Scene::from_document_with_resize(doc, &font_db, assets, &resize_map) {
            Ok(s) => s,
            Err(e) => {
                print_json(&ErrorResponse::new(
                    "RENDER_FAILED",
                    "render",
                    &e.to_string(),
                ));
                return EXIT_PROCESS;
            }
        }
    } else {
        match Scene::from_document(doc, &font_db, assets) {
            Ok(s) => s,
            Err(e) => {
                print_json(&ErrorResponse::new(
                    "RENDER_FAILED",
                    "render",
                    &e.to_string(),
                ));
                return EXIT_PROCESS;
            }
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
    let json = match load_document_json(file) {
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

    // Save output using OdeContainer
    let mut container = OdeContainer::from_document(result.document, "ode-cli");
    let path = Path::new(output);

    let save_result = if output.ends_with('/') || path.is_dir() {
        container.save_unpacked(path)
    } else if output.ends_with(".ode") && !output.ends_with(".ode.json") {
        container.save_packed(path)
    } else {
        // Legacy .ode.json or other — write plain JSON
        let json = match serde_json::to_string_pretty(&container.document) {
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
        std::fs::write(output, json).map_err(ContainerError::Io)
    };

    match save_result {
        Ok(()) => {
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

// ─── ode tokens list ───

pub fn cmd_tokens_list(file: &str) -> i32 {
    let json = match load_document_json(file) {
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

    let mut collections_out: Vec<TokensListCollection> = Vec::new();

    for coll in &doc.tokens.collections {
        let active_mode = doc.tokens.active_modes.get(&coll.id).copied();
        let modes: Vec<TokensListMode> = coll
            .modes
            .iter()
            .map(|m| TokensListMode {
                id: m.id,
                name: m.name.clone(),
                active: active_mode == Some(m.id),
                is_default: coll.default_mode == m.id,
            })
            .collect();

        let tokens: Vec<TokensListToken> = coll
            .tokens
            .iter()
            .map(|t| {
                let values: std::collections::HashMap<String, String> = t
                    .values
                    .iter()
                    .map(|(mode_id, resolve)| {
                        let mode_name = coll
                            .modes
                            .iter()
                            .find(|m| m.id == *mode_id)
                            .map(|m| m.name.clone())
                            .unwrap_or_else(|| format!("mode-{mode_id}"));
                        let value_str = match resolve {
                            ode_format::tokens::TokenResolve::Direct(tv) => format_token_value(tv),
                            ode_format::tokens::TokenResolve::Alias(tref) => {
                                format!(
                                    "-> collection:{} token:{}",
                                    tref.collection_id, tref.token_id
                                )
                            }
                        };
                        (mode_name, value_str)
                    })
                    .collect();

                TokensListToken {
                    id: t.id,
                    name: t.name.clone(),
                    group: t.group.clone(),
                    values,
                }
            })
            .collect();

        collections_out.push(TokensListCollection {
            id: coll.id,
            name: coll.name.clone(),
            modes,
            tokens,
        });
    }

    let result = TokensListResult {
        status: "ok",
        collections: collections_out,
    };
    print_json(&result);
    EXIT_OK
}

#[derive(serde::Serialize)]
struct TokensListResult {
    status: &'static str,
    collections: Vec<TokensListCollection>,
}

#[derive(serde::Serialize)]
struct TokensListCollection {
    id: u32,
    name: String,
    modes: Vec<TokensListMode>,
    tokens: Vec<TokensListToken>,
}

#[derive(serde::Serialize)]
struct TokensListMode {
    id: u32,
    name: String,
    active: bool,
    is_default: bool,
}

#[derive(serde::Serialize)]
struct TokensListToken {
    id: u32,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    values: std::collections::HashMap<String, String>,
}

fn format_token_value(tv: &ode_format::tokens::TokenValue) -> String {
    match tv {
        ode_format::tokens::TokenValue::Color(c) => match c {
            ode_format::color::Color::Srgb { r, g, b, a } => {
                format!("srgb({r:.3}, {g:.3}, {b:.3}, {a:.3})")
            }
            _ => format!("{c:?}"),
        },
        ode_format::tokens::TokenValue::Number(n) => format!("{n}"),
        ode_format::tokens::TokenValue::Dimension { value, unit } => {
            format!("{value}{unit:?}")
        }
        ode_format::tokens::TokenValue::FontFamily(f) => f.clone(),
        ode_format::tokens::TokenValue::FontWeight(w) => format!("{w}"),
        ode_format::tokens::TokenValue::Duration(d) => format!("{d}ms"),
        ode_format::tokens::TokenValue::CubicBezier(pts) => {
            format!(
                "cubic-bezier({}, {}, {}, {})",
                pts[0], pts[1], pts[2], pts[3]
            )
        }
        ode_format::tokens::TokenValue::String(s) => format!("\"{s}\""),
    }
}

// ─── ode tokens resolve ───

pub fn cmd_tokens_resolve(file: &str, collection: &str, token: &str) -> i32 {
    let json = match load_document_json(file) {
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

    // Find collection by name or ID
    let coll = match find_collection(&doc.tokens, collection) {
        Some(c) => c,
        None => {
            print_json(&ErrorResponse::new(
                "NOT_FOUND",
                "tokens",
                &format!("Collection '{collection}' not found"),
            ));
            return EXIT_INPUT;
        }
    };

    // Find token by name or ID
    let tok = match find_token(coll, token) {
        Some(t) => t,
        None => {
            print_json(&ErrorResponse::new(
                "NOT_FOUND",
                "tokens",
                &format!("Token '{token}' not found in collection '{}'", coll.name),
            ));
            return EXIT_INPUT;
        }
    };

    // Resolve
    match doc.tokens.resolve(coll.id, tok.id) {
        Ok(value) => {
            let result = TokenResolveResult {
                status: "ok",
                collection_name: coll.name.clone(),
                token_name: tok.name.clone(),
                value: format_token_value(&value),
            };
            print_json(&result);
            EXIT_OK
        }
        Err(e) => {
            print_json(&ErrorResponse::new(
                "RESOLVE_FAILED",
                "tokens",
                &format!("Failed to resolve token: {e}"),
            ));
            EXIT_PROCESS
        }
    }
}

#[derive(serde::Serialize)]
struct TokenResolveResult {
    status: &'static str,
    collection_name: String,
    token_name: String,
    value: String,
}

// ─── ode tokens set-mode ───

pub fn cmd_tokens_set_mode(file: &str, collection: &str, mode: &str, output: Option<&str>) -> i32 {
    let json = match load_document_json(file) {
        Ok(j) => j,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    let mut doc: Document = match serde_json::from_str(&json) {
        Ok(d) => d,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
            return EXIT_INPUT;
        }
    };

    // Find collection by name or ID
    let coll = match find_collection(&doc.tokens, collection) {
        Some(c) => c,
        None => {
            print_json(&ErrorResponse::new(
                "NOT_FOUND",
                "tokens",
                &format!("Collection '{collection}' not found"),
            ));
            return EXIT_INPUT;
        }
    };

    // Find mode by name or ID
    let mode_entry = match find_mode(coll, mode) {
        Some(m) => m,
        None => {
            print_json(&ErrorResponse::new(
                "NOT_FOUND",
                "tokens",
                &format!("Mode '{mode}' not found in collection '{}'", coll.name),
            ));
            return EXIT_INPUT;
        }
    };

    let coll_id = coll.id;
    let mode_id = mode_entry.id;
    let coll_name = coll.name.clone();
    let mode_name = mode_entry.name.clone();

    // Set the mode
    doc.tokens.set_active_mode(coll_id, mode_id);

    // Serialize and write
    let out_path = output.unwrap_or(file);
    let json_out = match serde_json::to_string_pretty(&doc) {
        Ok(j) => j,
        Err(e) => {
            print_json(&ErrorResponse::new("INTERNAL", "serialize", &e.to_string()));
            return EXIT_INTERNAL;
        }
    };

    if let Err(e) = std::fs::write(out_path, &json_out) {
        print_json(&ErrorResponse::new("IO_ERROR", "io", &e.to_string()));
        return EXIT_IO;
    }

    let result = SetModeResult {
        status: "ok",
        collection_name: coll_name,
        mode_name,
        path: out_path.to_string(),
    };
    print_json(&result);
    EXIT_OK
}

#[derive(serde::Serialize)]
struct SetModeResult {
    status: &'static str,
    collection_name: String,
    mode_name: String,
    path: String,
}

// ─── Token Lookup Helpers ───

fn find_collection<'a>(
    tokens: &'a ode_format::tokens::DesignTokens,
    name_or_id: &str,
) -> Option<&'a ode_format::tokens::TokenCollection> {
    // Try by name first
    if let Some(c) = tokens.collections.iter().find(|c| c.name == name_or_id) {
        return Some(c);
    }
    // Try by ID
    if let Ok(id) = name_or_id.parse::<u32>() {
        return tokens.collections.iter().find(|c| c.id == id);
    }
    None
}

fn find_token<'a>(
    coll: &'a ode_format::tokens::TokenCollection,
    name_or_id: &str,
) -> Option<&'a ode_format::tokens::Token> {
    // Try by name first
    if let Some(t) = coll.tokens.iter().find(|t| t.name == name_or_id) {
        return Some(t);
    }
    // Try by ID
    if let Ok(id) = name_or_id.parse::<u32>() {
        return coll.tokens.iter().find(|t| t.id == id);
    }
    None
}

fn find_mode<'a>(
    coll: &'a ode_format::tokens::TokenCollection,
    name_or_id: &str,
) -> Option<&'a ode_format::tokens::Mode> {
    // Try by name first
    if let Some(m) = coll.modes.iter().find(|m| m.name == name_or_id) {
        return Some(m);
    }
    // Try by ID
    if let Ok(id) = name_or_id.parse::<u32>() {
        return coll.modes.iter().find(|m| m.id == id);
    }
    None
}

// ─── ode guide ───

/// Index structure for design-knowledge/index.json
#[derive(serde::Deserialize)]
struct KnowledgeIndex {
    layers: Vec<KnowledgeLayer>,
}

#[derive(serde::Deserialize)]
struct KnowledgeLayer {
    id: String,
    name: String,
    #[serde(default)]
    contexts: Vec<String>,
    #[serde(default)]
    guide: Option<String>,
    #[serde(default)]
    rules: Vec<String>,
    #[serde(default)]
    related: Vec<String>,
}

pub fn cmd_guide(
    layer_id: Option<&str>,
    context: Option<&str>,
    section: Option<&str>,
    related: Option<&str>,
) -> i32 {
    let knowledge_dir = match crate::knowledge::find_knowledge_dir() {
        Some(d) => d,
        None => {
            print_json(&ErrorResponse::new(
                "KNOWLEDGE_NOT_FOUND",
                "knowledge",
                "design-knowledge directory not found",
            ));
            return EXIT_IO;
        }
    };

    let index_path = knowledge_dir.join("index.json");
    let index_str = match std::fs::read_to_string(&index_path) {
        Ok(s) => s,
        Err(e) => {
            print_json(&ErrorResponse::new(
                "IO_ERROR",
                "io",
                &format!("failed to read index.json: {e}"),
            ));
            return EXIT_IO;
        }
    };

    let index: KnowledgeIndex = match serde_json::from_str(&index_str) {
        Ok(i) => i,
        Err(e) => {
            print_json(&ErrorResponse::new(
                "PARSE_FAILED",
                "parse",
                &format!("failed to parse index.json: {e}"),
            ));
            return EXIT_INPUT;
        }
    };

    // If --related is given, find guides related to that layer
    if let Some(related_layer) = related {
        let source = index.layers.iter().find(|l| l.id == related_layer);
        let related_ids: Vec<&str> = match source {
            Some(l) => l.related.iter().map(|s| s.as_str()).collect(),
            None => {
                print_json(&ErrorResponse::new(
                    "LAYER_NOT_FOUND",
                    "knowledge",
                    &format!("layer '{related_layer}' not found in index"),
                ));
                return EXIT_INPUT;
            }
        };

        let layers: Vec<GuideLayerInfo> = index
            .layers
            .iter()
            .filter(|l| related_ids.contains(&l.id.as_str()))
            .map(|l| GuideLayerInfo {
                id: l.id.clone(),
                name: l.name.clone(),
                contexts: l.contexts.clone(),
            })
            .collect();

        print_json(&GuideListResponse {
            status: "ok",
            layers,
        });
        return EXIT_OK;
    }

    // If no layer_id: list all layers
    if layer_id.is_none() {
        let mut layers: Vec<GuideLayerInfo> = index
            .layers
            .iter()
            .map(|l| GuideLayerInfo {
                id: l.id.clone(),
                name: l.name.clone(),
                contexts: l.contexts.clone(),
            })
            .collect();

        // Filter by context if provided
        if let Some(ctx) = context {
            layers.retain(|l| l.contexts.is_empty() || l.contexts.iter().any(|c| c == ctx));
        }

        print_json(&GuideListResponse {
            status: "ok",
            layers,
        });
        return EXIT_OK;
    }

    // layer_id given: find the layer and return its guide content
    let layer_id = layer_id.unwrap();
    let layer = match index.layers.iter().find(|l| l.id == layer_id) {
        Some(l) => l,
        None => {
            print_json(&ErrorResponse::new(
                "LAYER_NOT_FOUND",
                "knowledge",
                &format!("layer '{layer_id}' not found in index"),
            ));
            return EXIT_INPUT;
        }
    };

    let guide_path = match &layer.guide {
        Some(g) => {
            // Reject absolute paths and traversals
            if Path::new(g).is_absolute() || g.contains("..") {
                print_json(&ErrorResponse::new(
                    "INVALID_PATH",
                    "knowledge",
                    &format!("guide path escapes knowledge dir: {g}"),
                ));
                return EXIT_INPUT;
            }
            knowledge_dir.join(g)
        }
        None => {
            print_json(&ErrorResponse::new(
                "NO_GUIDE",
                "knowledge",
                &format!("layer '{layer_id}' has no associated guide file"),
            ));
            return EXIT_INPUT;
        }
    };

    let content = match std::fs::read_to_string(&guide_path) {
        Ok(s) => s,
        Err(e) => {
            print_json(&ErrorResponse::new(
                "IO_ERROR",
                "io",
                &format!("failed to read guide file: {e}"),
            ));
            return EXIT_IO;
        }
    };

    let content = if let Some(section_name) = section {
        match extract_section(&content, section_name) {
            Some(s) => s,
            None => {
                print_json(&ErrorResponse::new(
                    "SECTION_NOT_FOUND",
                    "knowledge",
                    &format!("section '{section_name}' not found in guide for '{layer_id}'"),
                ));
                return EXIT_INPUT;
            }
        }
    } else {
        content
    };

    print_json(&GuideContentResponse {
        status: "ok",
        format: "markdown",
        content,
    });
    EXIT_OK
}

/// Extract a section from markdown by heading name.
///
/// Finds `## {section_name}` (case-insensitive) and returns everything
/// from that heading until the next `## ` heading or end of file.
fn extract_section(markdown: &str, section_name: &str) -> Option<String> {
    let section_lower = section_name.to_lowercase();
    let lines: Vec<&str> = markdown.lines().collect();

    let mut start = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            let heading = trimmed.trim_start_matches("## ").trim().to_lowercase();
            if heading == section_lower {
                start = Some(i);
                break;
            }
        }
    }

    let start = start?;
    let mut end = lines.len();
    for i in (start + 1)..lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("## ") {
            end = i;
            break;
        }
    }

    let section: String = lines[start..end].join("\n");
    let trimmed = section.trim_end().to_string();
    Some(trimmed)
}

// ─── ode review ───

pub fn cmd_review(file: &str, context: Option<&str>, layer: Option<&str>) -> i32 {
    let knowledge_dir = match crate::knowledge::find_knowledge_dir() {
        Some(d) => d,
        None => {
            print_json(&ErrorResponse::new(
                "KNOWLEDGE_NOT_FOUND",
                "knowledge",
                "design-knowledge directory not found",
            ));
            return EXIT_IO;
        }
    };

    let json_str = match load_document_json(file) {
        Ok(s) => s,
        Err((code, err)) => {
            print_json(&err);
            return code;
        }
    };

    let doc: Document = match serde_json::from_str(&json_str) {
        Ok(d) => d,
        Err(e) => {
            print_json(&ErrorResponse::new(
                "PARSE_FAILED",
                "parse",
                &e.to_string(),
            ));
            return EXIT_INPUT;
        }
    };

    // Read index.json to find rule paths
    let index_path = knowledge_dir.join("index.json");
    let index_str = match std::fs::read_to_string(&index_path) {
        Ok(s) => s,
        Err(e) => {
            print_json(&ErrorResponse::new(
                "IO_ERROR",
                "io",
                &format!("failed to read index.json: {e}"),
            ));
            return EXIT_IO;
        }
    };

    let index: KnowledgeIndex = match serde_json::from_str(&index_str) {
        Ok(i) => i,
        Err(e) => {
            print_json(&ErrorResponse::new(
                "PARSE_FAILED",
                "parse",
                &format!("failed to parse index.json: {e}"),
            ));
            return EXIT_INPUT;
        }
    };

    // Collect rule file paths, filtered by --layer if given
    let rule_paths: Vec<&str> = index
        .layers
        .iter()
        .filter(|l| match layer {
            Some(filter) => l.id == filter,
            None => true,
        })
        .flat_map(|l| l.rules.iter().map(|s| s.as_str()))
        .collect();

    let rules = if rule_paths.is_empty() {
        vec![]
    } else {
        match ode_review::load_rules_from_paths(&knowledge_dir, &rule_paths) {
            Ok(r) => r,
            Err(e) => {
                print_json(&ErrorResponse::new(
                    "IO_ERROR",
                    "io",
                    &format!("failed to load rules: {e}"),
                ));
                return EXIT_IO;
            }
        }
    };

    let registry = ode_review::checkers::default_registry();
    let result = ode_review::review_document(&doc, &rules, context, &registry);

    let response = ReviewResponse {
        status: "ok",
        context: serde_json::to_value(&result.contexts).unwrap_or_default(),
        summary: result.summary,
        issues: result.issues,
        skipped_rules: result.skipped_rules,
        warnings: vec![],
    };
    print_json(&response);
    EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::extract_section;

    #[test]
    fn extract_existing_section() {
        let md = "# Title\n\n## Section A\nContent A\n\n## Section B\nContent B\n";
        let result = extract_section(md, "Section A").unwrap();
        assert!(result.contains("Content A"));
        assert!(!result.contains("Content B"));
    }

    #[test]
    fn extract_section_case_insensitive() {
        let md = "## My Section\nContent here\n## Next\n";
        let result = extract_section(md, "my section").unwrap();
        assert!(result.contains("Content here"));
    }

    #[test]
    fn extract_nonexistent_section_returns_none() {
        let md = "## Only Section\nContent\n";
        assert!(extract_section(md, "Missing").is_none());
    }
}
