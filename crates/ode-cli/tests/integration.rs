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

// ─── ode pack / unpack ───

#[test]
fn pack_and_unpack_roundtrip() {
    let dir = std::env::temp_dir().join("ode_test_pack_unpack");
    std::fs::create_dir_all(&dir).ok();
    let json_file = dir.join("design.ode.json");
    let packed_file = dir.join("design.ode");
    let unpacked_dir = dir.join("design_unpacked");

    // 1. Create a new .ode.json document
    let output = ode_cmd()
        .args([
            "new",
            json_file.to_str().unwrap(),
            "--name",
            "Pack Test",
            "--width",
            "200",
            "--height",
            "100",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "new failed");

    // 2. Pack .ode.json -> .ode
    let output = ode_cmd()
        .args([
            "pack",
            json_file.to_str().unwrap(),
            "-o",
            packed_file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "pack failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert!(packed_file.exists(), ".ode file should exist");

    // Verify it's a valid ZIP (PK magic bytes)
    let bytes = std::fs::read(&packed_file).unwrap();
    assert_eq!(&bytes[..2], b"PK", "Expected ZIP magic bytes");

    // 3. Unpack .ode -> directory
    let output = ode_cmd()
        .args([
            "unpack",
            packed_file.to_str().unwrap(),
            "-o",
            unpacked_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "unpack failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");
    assert!(unpacked_dir.exists(), "unpacked dir should exist");
    assert!(
        unpacked_dir.join("document.json").exists(),
        "document.json should exist"
    );
    assert!(
        unpacked_dir.join("meta.json").exists(),
        "meta.json should exist"
    );

    // 4. Verify roundtrip: the unpacked document should match the original
    let unpacked_doc: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(unpacked_dir.join("document.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(unpacked_doc["name"], "Pack Test");

    // 5. Verify the unpacked directory can be validated
    let output = ode_cmd()
        .args(["validate", unpacked_dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "validate unpacked failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn pack_default_output_name() {
    let dir = std::env::temp_dir().join("ode_test_pack_default");
    std::fs::create_dir_all(&dir).ok();
    let json_file = dir.join("mydesign.ode.json");

    // Create a document
    ode_cmd()
        .args(["new", json_file.to_str().unwrap()])
        .output()
        .unwrap();

    // Pack without -o flag: should derive output name
    let output = ode_cmd()
        .args(["pack", json_file.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "pack default failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");

    // The output path should be derived from the input filename
    let out_path = json["path"].as_str().unwrap();
    assert!(
        out_path.ends_with(".ode"),
        "Expected .ode output, got: {out_path}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unpack_default_output_name() {
    let dir = std::env::temp_dir().join("ode_test_unpack_default");
    std::fs::create_dir_all(&dir).ok();
    let json_file = dir.join("test.ode.json");
    let packed_file = dir.join("test.ode");

    // Create and pack
    ode_cmd()
        .args(["new", json_file.to_str().unwrap()])
        .output()
        .unwrap();
    ode_cmd()
        .args([
            "pack",
            json_file.to_str().unwrap(),
            "-o",
            packed_file.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    // Unpack without -o flag
    let output = ode_cmd()
        .args(["unpack", packed_file.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "unpack default failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = parse_json(&output);
    assert_eq!(json["status"], "ok");

    // The output path should be derived: "test.ode" -> "test" directory
    let out_path = json["path"].as_str().unwrap();
    assert!(
        out_path.ends_with("test"),
        "Expected directory named 'test', got: {out_path}"
    );
    assert!(
        std::path::Path::new(out_path).join("document.json").exists(),
        "document.json should exist in unpacked dir"
    );

    std::fs::remove_dir_all(&dir).ok();
}

// ─── Packed .ode format tests ───

#[test]
fn new_creates_packed_ode() {
    let dir = std::env::temp_dir().join("ode_test_new_packed");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("test.ode");
    let _ = std::fs::remove_file(&file);

    let output = ode_cmd()
        .args([
            "new",
            file.to_str().unwrap(),
            "--name",
            "Packed Doc",
            "--width",
            "200",
            "--height",
            "100",
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
    assert!(file.exists());

    // Verify it's a valid ZIP (PK magic bytes)
    let bytes = std::fs::read(&file).unwrap();
    assert_eq!(&bytes[..2], b"PK", "Expected ZIP magic bytes for .ode");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_creates_unpacked_directory() {
    let dir = std::env::temp_dir().join("ode_test_new_unpacked");
    std::fs::create_dir_all(&dir).ok();
    let design_dir = dir.join("mydesign");
    let _ = std::fs::remove_dir_all(&design_dir);

    // Trailing slash triggers unpacked directory mode
    let design_path = format!("{}/", design_dir.to_str().unwrap());

    let output = ode_cmd()
        .args([
            "new",
            &design_path,
            "--name",
            "Unpacked Doc",
            "--width",
            "300",
            "--height",
            "200",
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
    assert!(design_dir.exists());
    assert!(
        design_dir.join("document.json").exists(),
        "document.json should exist"
    );
    assert!(
        design_dir.join("meta.json").exists(),
        "meta.json should exist"
    );

    // Verify document content
    let doc: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(design_dir.join("document.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(doc["name"], "Unpacked Doc");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn build_from_packed_ode() {
    let dir = std::env::temp_dir().join("ode_test_build_packed");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("design.ode");
    let png = dir.join("output.png");

    // Create a packed .ode document
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

#[test]
fn build_from_unpacked_dir() {
    let dir = std::env::temp_dir().join("ode_test_build_unpacked");
    std::fs::create_dir_all(&dir).ok();
    let design_dir = dir.join("design");
    let png = dir.join("output.png");
    let _ = std::fs::remove_dir_all(&design_dir);

    let design_path = format!("{}/", design_dir.to_str().unwrap());

    // Create an unpacked directory
    ode_cmd()
        .args(["new", &design_path, "--width", "64", "--height", "64"])
        .output()
        .unwrap();

    let output = ode_cmd()
        .args([
            "build",
            design_dir.to_str().unwrap(),
            "-o",
            png.to_str().unwrap(),
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
    assert!(png.exists());

    // Verify PNG magic bytes
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn validate_packed_ode() {
    let dir = std::env::temp_dir().join("ode_test_validate_packed");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("valid.ode");

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
fn validate_unpacked_dir() {
    let dir = std::env::temp_dir().join("ode_test_validate_unpacked");
    std::fs::create_dir_all(&dir).ok();
    let design_dir = dir.join("design");
    let _ = std::fs::remove_dir_all(&design_dir);

    let design_path = format!("{}/", design_dir.to_str().unwrap());
    ode_cmd()
        .args(["new", &design_path])
        .output()
        .unwrap();

    let output = ode_cmd()
        .args(["validate", design_dir.to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let json = parse_json(&output);
    assert_eq!(json["valid"], true);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn inspect_packed_ode() {
    let dir = std::env::temp_dir().join("ode_test_inspect_packed");
    std::fs::create_dir_all(&dir).ok();
    let file = dir.join("doc.ode");

    ode_cmd()
        .args([
            "new",
            file.to_str().unwrap(),
            "--name",
            "Packed Inspect",
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
    assert_eq!(json["name"], "Packed Inspect");
    assert_eq!(json["node_count"], 1);

    std::fs::remove_dir_all(&dir).ok();
}

// ─── Full end-to-end workflow: Task 10 ───

#[test]
fn full_workflow_create_edit_pack_unpack_render() {
    let dir = std::env::temp_dir().join("ode_test_full_e2e_workflow");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let design_dir = dir.join("design");
    let design_path = format!("{}/", design_dir.to_str().unwrap());
    let preview1 = dir.join("preview1.png");
    let packed = dir.join("design.ode");
    let preview2 = dir.join("preview2.png");
    let design2_dir = dir.join("design2");

    // 1. Create unpacked: ode new design/ --width 800 --height 600
    let output = ode_cmd()
        .args(["new", &design_path, "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "Step 1 (new unpacked) failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(design_dir.join("document.json").exists());
    assert!(design_dir.join("meta.json").exists());

    // 2. Add a frame: ode add frame design/ --name "Card" --width 400 --height 300 --fill "#336699"
    let output = ode_cmd()
        .args([
            "add",
            "frame",
            design_dir.to_str().unwrap(),
            "--name",
            "Card",
            "--width",
            "400",
            "--height",
            "300",
            "--fill",
            "#336699",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "Step 2 (add frame) failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let add_resp = parse_json(&output);
    assert_eq!(add_resp["status"], "ok");
    assert_eq!(add_resp["name"], "Card");
    let card_id = add_resp["stable_id"].as_str().unwrap().to_string();

    // 3. Render from unpacked: ode build design/ -o preview1.png
    let output = ode_cmd()
        .args([
            "build",
            design_dir.to_str().unwrap(),
            "-o",
            preview1.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "Step 3 (build unpacked) failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(preview1.exists(), "preview1.png should exist");
    let bytes = std::fs::read(&preview1).unwrap();
    assert_eq!(
        &bytes[..4],
        &[0x89, b'P', b'N', b'G'],
        "preview1 should be PNG"
    );

    // 4. Pack: ode pack design/ -o design.ode
    let output = ode_cmd()
        .args([
            "pack",
            design_dir.to_str().unwrap(),
            "-o",
            packed.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "Step 4 (pack) failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(packed.exists(), "design.ode should exist");
    let pk_bytes = std::fs::read(&packed).unwrap();
    assert_eq!(&pk_bytes[..2], b"PK", "packed file should be ZIP");

    // 5. Render from packed: ode build design.ode -o preview2.png
    let output = ode_cmd()
        .args([
            "build",
            packed.to_str().unwrap(),
            "-o",
            preview2.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "Step 5 (build packed) failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(preview2.exists(), "preview2.png should exist");
    let bytes2 = std::fs::read(&preview2).unwrap();
    assert_eq!(
        &bytes2[..4],
        &[0x89, b'P', b'N', b'G'],
        "preview2 should be PNG"
    );

    // 6. Unpack to new location: ode unpack design.ode -o design2/
    let output = ode_cmd()
        .args([
            "unpack",
            packed.to_str().unwrap(),
            "-o",
            design2_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "Step 6 (unpack) failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 7. Verify design2/document.json and design2/meta.json exist
    assert!(
        design2_dir.join("document.json").exists(),
        "design2/document.json should exist"
    );
    assert!(
        design2_dir.join("meta.json").exists(),
        "design2/meta.json should exist"
    );

    // Verify roundtrip: the unpacked document should have the Card frame
    let doc: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(design2_dir.join("document.json")).unwrap(),
    )
    .unwrap();
    let nodes = doc["nodes"].as_array().unwrap();
    let card_node = nodes.iter().find(|n| n["stable_id"] == card_id);
    assert!(
        card_node.is_some(),
        "Card node should exist in unpacked document"
    );
    assert_eq!(card_node.unwrap()["name"], "Card");

    // 8. Validate the unpacked copy
    let output = ode_cmd()
        .args(["validate", design2_dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "Step 8 (validate unpacked copy) failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let valid_resp = parse_json(&output);
    assert_eq!(valid_resp["valid"], true);

    std::fs::remove_dir_all(&dir).ok();
}
