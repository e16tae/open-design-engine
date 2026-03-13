use std::path::Path;
use ode_core::{Renderer, Scene};
use ode_export::PngExporter;
use ode_format::Document;
use ode_format::wire::DocumentWire;
use crate::output::*;
use crate::validate::validate_json;

pub fn load_input(file: &str) -> Result<String, (i32, ErrorResponse)> {
    if file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)
            .map_err(|e| (EXIT_IO, ErrorResponse::new("IO_ERROR", "io", &e.to_string())))?;
        Ok(buf)
    } else {
        std::fs::read_to_string(file)
            .map_err(|e| (EXIT_IO, ErrorResponse::new("IO_ERROR", "io",
                &format!("failed to read '{}': {}", file, e))))
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
        Err((code, err)) => { print_json(&err); return code; }
    };

    let result = validate_json(&json);
    let exit = if result.valid { EXIT_OK } else { EXIT_INPUT };
    print_json(&result);
    exit
}

// ─── ode build ───

pub fn cmd_build(file: &str, output: &str) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => { print_json(&err); return code; }
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

    render_and_export(&doc, output, validation.warnings)
}

// ─── ode render ───

pub fn cmd_render(file: &str, output: &str) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => { print_json(&err); return code; }
    };

    let doc: Document = match serde_json::from_str(&json) {
        Ok(d) => d,
        Err(e) => {
            print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
            return EXIT_INPUT;
        }
    };

    render_and_export(&doc, output, vec![])
}

fn render_and_export(doc: &Document, output: &str, warnings: Vec<Warning>) -> i32 {
    let scene = match Scene::from_document(doc) {
        Ok(s) => s,
        Err(e) => {
            print_json(&ErrorResponse::new("RENDER_FAILED", "render", &e.to_string()));
            return EXIT_PROCESS;
        }
    };

    let pixmap = match Renderer::render(&scene) {
        Ok(p) => p,
        Err(e) => {
            print_json(&ErrorResponse::new("RENDER_FAILED", "render", &e.to_string()));
            return EXIT_PROCESS;
        }
    };

    if let Err(e) = PngExporter::export(&pixmap, Path::new(output)) {
        print_json(&ErrorResponse::new("EXPORT_FAILED", "export", &e.to_string()));
        return EXIT_PROCESS;
    }

    let mut resp = OkResponse::with_render(output, pixmap.width(), pixmap.height());
    resp.warnings = warnings;
    print_json(&resp);
    EXIT_OK
}

// ─── ode inspect ───

pub fn cmd_inspect(file: &str, full: bool) -> i32 {
    let json = match load_input(file) {
        Ok(j) => j,
        Err((code, err)) => { print_json(&err); return code; }
    };

    if full {
        let wire: DocumentWire = match serde_json::from_str(&json) {
            Ok(w) => w,
            Err(e) => {
                print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
                return EXIT_INPUT;
            }
        };
        print_json(&wire);
    } else {
        let wire: DocumentWire = match serde_json::from_str(&json) {
            Ok(w) => w,
            Err(e) => {
                print_json(&ErrorResponse::new("PARSE_FAILED", "parse", &e.to_string()));
                return EXIT_INPUT;
            }
        };
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
    let node_map: HashMap<&str, &ode_format::wire::NodeWire> = wire.nodes.iter()
        .map(|n| (n.stable_id.as_str(), n))
        .collect();

    let tree = wire.canvas.iter()
        .filter_map(|id| node_map.get(id.as_str()).map(|n| build_tree_node(n, &node_map)))
        .collect();

    InspectSummary {
        name: wire.name.clone(),
        format_version: format!("{}.{}.{}", wire.format_version.0, wire.format_version.1, wire.format_version.2),
        working_color_space: serde_json::to_value(&wire.working_color_space)
            .ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
        node_count: wire.nodes.len(),
        canvas: wire.canvas.clone(),
        tree,
        tokens: TokensSummary {
            collections: wire.tokens.collections.iter().map(|c| c.name.clone()).collect(),
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
        NodeKindWire::Frame(d) => ("frame", Some([d.width, d.height]),
            d.container.children.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
        NodeKindWire::Group(d) => ("group", None,
            d.children.iter().map(|s| s.as_str()).collect()),
        NodeKindWire::Vector(_) => ("vector", None, vec![]),
        NodeKindWire::BooleanOp(d) => ("boolean-op", None,
            d.children.iter().map(|s| s.as_str()).collect()),
        NodeKindWire::Text(_) => ("text", None, vec![]),
        NodeKindWire::Image(_) => ("image", None, vec![]),
        NodeKindWire::Instance(d) => ("instance", None,
            d.container.children.iter().map(|s| s.as_str()).collect()),
    };

    let children = child_ids.iter()
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
                "INVALID_TOPIC", "schema",
                &format!("unknown schema topic '{}'. Available: document, node, paint, token, color", unknown),
            ));
            return EXIT_INPUT;
        }
    };

    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    EXIT_OK
}
