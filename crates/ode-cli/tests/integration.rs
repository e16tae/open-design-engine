use std::process::Command;

fn ode_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ode"))
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("Failed to parse JSON: {}\nOutput: {}", e, stdout))
}

// ─── ode new ───

#[test]
fn new_creates_file() {
    let dir = std::env::temp_dir().join("ode_test_new");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("test.ode.json");
    let _ = std::fs::remove_file(&file);

    let output = ode_cmd()
        .args([
            "new",
            file.to_str().unwrap(),
            "--name",
            "Test Doc",
            "--width",
            "100",
            "--height",
            "50",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert!(file.exists());

    // Verify the created file is valid
    let content: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(content["name"], "Test Doc");
    assert_eq!(content["canvas"].as_array().unwrap().len(), 1);

    std::fs::remove_dir_all(&dir).ok();
}

// ─── ode validate ───

#[test]
fn validate_valid_document() {
    let dir = std::env::temp_dir().join("ode_test_validate");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("valid.ode.json");

    // Create a valid document first
    ode_cmd()
        .args(["new", file.to_str().unwrap()])
        .output()
        .unwrap();

    let output = ode_cmd()
        .args(["validate", file.to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["valid"], true);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn validate_invalid_json() {
    let dir = std::env::temp_dir().join("ode_test_invalid");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("bad.ode.json");
    std::fs::write(&file, "not json").unwrap();

    let output = ode_cmd()
        .args(["validate", file.to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let json = parse_json(&output);
    assert_eq!(json["valid"], false);
    assert!(json["errors"][0]["code"].as_str().unwrap() == "PARSE_FAILED");

    std::fs::remove_dir_all(&dir).ok();
}

// ─── ode build ───

#[test]
fn build_creates_png() {
    let dir = std::env::temp_dir().join("ode_test_build");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("design.ode.json");
    let png = dir.join("output.png");

    // Create a document with a colored frame
    ode_cmd()
        .args([
            "new",
            file.to_str().unwrap(),
            "--width",
            "64",
            "--height",
            "64",
        ])
        .output()
        .unwrap();

    let output = ode_cmd()
        .args(["build", file.to_str().unwrap(), "-o", png.to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert!(png.exists());

    // Verify PNG magic bytes
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);

    std::fs::remove_dir_all(&dir).ok();
}

// ─── ode build (SVG) ───

#[test]
fn build_creates_svg_by_extension() {
    let dir = std::env::temp_dir().join("ode_test_build_svg_ext");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("design.ode.json");
    let svg = dir.join("output.svg");

    ode_cmd()
        .args([
            "new",
            file.to_str().unwrap(),
            "--width",
            "64",
            "--height",
            "64",
        ])
        .output()
        .unwrap();

    let output = ode_cmd()
        .args(["build", file.to_str().unwrap(), "-o", svg.to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert!(svg.exists());

    let content = std::fs::read_to_string(&svg).unwrap();
    assert!(
        content.starts_with("<?xml"),
        "Expected XML declaration, got: {}",
        &content[..50.min(content.len())]
    );
    assert!(content.contains("<svg"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn build_creates_svg_by_format_flag() {
    let dir = std::env::temp_dir().join("ode_test_build_svg_flag");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("design.ode.json");
    let out = dir.join("output.dat");

    ode_cmd()
        .args([
            "new",
            file.to_str().unwrap(),
            "--width",
            "32",
            "--height",
            "32",
        ])
        .output()
        .unwrap();

    let output = ode_cmd()
        .args([
            "build",
            file.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--format",
            "svg",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.exists());

    let content = std::fs::read_to_string(&out).unwrap();
    assert!(content.contains("<svg"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn render_creates_svg() {
    let dir = std::env::temp_dir().join("ode_test_render_svg");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("design.ode.json");
    let svg = dir.join("render_out.svg");

    ode_cmd()
        .args([
            "new",
            file.to_str().unwrap(),
            "--width",
            "48",
            "--height",
            "48",
        ])
        .output()
        .unwrap();

    let output = ode_cmd()
        .args([
            "render",
            file.to_str().unwrap(),
            "-o",
            svg.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(svg.exists());

    let content = std::fs::read_to_string(&svg).unwrap();
    assert!(content.contains("<svg"));

    std::fs::remove_dir_all(&dir).ok();
}

// ─── ode inspect ───

#[test]
fn inspect_shows_tree() {
    let dir = std::env::temp_dir().join("ode_test_inspect");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("doc.ode.json");

    ode_cmd()
        .args([
            "new",
            file.to_str().unwrap(),
            "--name",
            "Inspect Me",
            "--width",
            "100",
            "--height",
            "50",
        ])
        .output()
        .unwrap();

    let output = ode_cmd()
        .args(["inspect", file.to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["name"], "Inspect Me");
    assert_eq!(json["node_count"], 1);
    assert!(!json["tree"].as_array().unwrap().is_empty());

    std::fs::remove_dir_all(&dir).ok();
}

// ─── ode schema ───

#[test]
fn schema_outputs_valid_json_schema() {
    let output = ode_cmd().args(["schema"]).output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    // JSON Schema should have a title or $schema field
    assert!(
        json.get("title").is_some() || json.get("$schema").is_some() || json.get("type").is_some(),
        "Expected JSON Schema, got: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );
}

#[test]
fn schema_invalid_topic() {
    let output = ode_cmd().args(["schema", "nonsense"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let json = parse_json(&output);
    assert_eq!(json["status"], "error");
}

// ─── stdin support ───

#[test]
fn validate_stdin() {
    let json = r#"{"format_version":[0,2,0],"name":"Stdin","nodes":[],"canvas":[],"tokens":{"collections":[],"active_modes":{}},"views":[]}"#;

    let output = ode_cmd()
        .args(["validate", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(json.as_bytes())
                .unwrap();
            child.wait_with_output()
        })
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["valid"], true);
}

#[test]
fn inspect_stdin() {
    let json = r#"{"format_version":[0,2,0],"name":"Stdin Inspect","nodes":[{"stable_id":"r","name":"Root","type":"frame","width":50,"height":50,"visual":{},"container":{},"component_def":null}],"canvas":["r"],"tokens":{"collections":[],"active_modes":{}},"views":[]}"#;

    let output = ode_cmd()
        .args(["inspect", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(json.as_bytes())
                .unwrap();
            child.wait_with_output()
        })
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["name"], "Stdin Inspect");
    assert_eq!(result["node_count"], 1);
}

// ─── ode guide ───

#[test]
fn guide_lists_layers() {
    let output = ode_cmd().args(["guide"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    let layers = json["layers"].as_array().unwrap();
    assert!(layers.len() >= 2);
}

#[test]
fn guide_shows_accessibility() {
    let output = ode_cmd().args(["guide", "accessibility"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["format"], "markdown");
    let content = json["content"].as_str().unwrap();
    assert!(
        content.contains("접근성") || content.contains("Accessibility"),
        "Expected guide content to mention accessibility, got: {}",
        &content[..100.min(content.len())]
    );
}

#[test]
fn guide_unknown_layer_returns_error() {
    let output = ode_cmd().args(["guide", "nonexistent"]).output().unwrap();
    assert_ne!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["status"], "error");
}

// ─── ode review ───

#[test]
fn review_validates_document() {
    let dir = std::env::temp_dir().join("ode_review_integ_test");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("test.ode.json");
    let _ = std::fs::remove_file(&file);

    ode_cmd()
        .args([
            "new",
            file.to_str().unwrap(),
            "--width",
            "400",
            "--height",
            "300",
        ])
        .output()
        .unwrap();

    let output = ode_cmd()
        .args(["review", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert!(json["summary"]["total"].as_u64().is_some());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn review_with_context_flag() {
    let dir = std::env::temp_dir().join("ode_review_ctx_test");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("test.ode.json");
    let _ = std::fs::remove_file(&file);

    ode_cmd()
        .args([
            "new",
            file.to_str().unwrap(),
            "--width",
            "400",
            "--height",
            "300",
        ])
        .output()
        .unwrap();

    let output = ode_cmd()
        .args([
            "review",
            file.to_str().unwrap(),
            "--context",
            "print",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    // Context is serialized as an array of strings
    let ctx_arr = json["context"].as_array().unwrap();
    assert!(
        ctx_arr.iter().any(|v| v == "print"),
        "Expected context to contain 'print', got: {:?}",
        ctx_arr
    );

    std::fs::remove_dir_all(&dir).ok();
}
