use std::process::Command;

fn ode_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ode"))
}

#[test]
fn add_frame_to_new_document() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");
    let out = ode_cmd()
        .args(["new", file.to_str().unwrap(), "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let out = ode_cmd()
        .args([
            "add", "frame",
            file.to_str().unwrap(),
            "--name", "Card",
            "--width", "320",
            "--height", "200",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["status"], "ok");
    assert_eq!(resp["kind"], "frame");
    assert_eq!(resp["name"], "Card");
    assert!(!resp["stable_id"].as_str().unwrap().is_empty());
}

#[test]
fn add_text_with_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");
    ode_cmd()
        .args(["new", file.to_str().unwrap(), "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    let out = ode_cmd()
        .args(["add", "text", file.to_str().unwrap(), "--content", "Hello World"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["name"], "Text");
}

#[test]
fn add_vector_rect_with_fill() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");
    ode_cmd()
        .args(["new", file.to_str().unwrap(), "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    let out = ode_cmd()
        .args([
            "add", "vector",
            file.to_str().unwrap(),
            "--shape", "rect",
            "--width", "48",
            "--height", "48",
            "--fill", "#3B82F6",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["name"], "Rectangle");
}

#[test]
fn add_to_empty_canvas_creates_root() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");
    ode_cmd()
        .args(["new", file.to_str().unwrap()])
        .output()
        .unwrap();
    let out = ode_cmd()
        .args([
            "add", "frame",
            file.to_str().unwrap(),
            "--name", "Root",
            "--width", "800",
            "--height", "600",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["parent"], "root");
}

#[test]
fn add_to_non_container_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");
    ode_cmd()
        .args(["new", file.to_str().unwrap(), "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    let out = ode_cmd()
        .args(["add", "text", file.to_str().unwrap(), "--content", "Hi"])
        .output()
        .unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text_id = resp["stable_id"].as_str().unwrap().to_string();

    let out = ode_cmd()
        .args([
            "add", "frame",
            file.to_str().unwrap(),
            "--name", "Bad",
            "--width", "10",
            "--height", "10",
            "--parent", &text_id,
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["code"], "NOT_CONTAINER");
}

#[test]
fn add_group() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json");
    ode_cmd()
        .args(["new", file.to_str().unwrap(), "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    let out = ode_cmd()
        .args(["add", "group", file.to_str().unwrap(), "--name", "My Group"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["kind"], "group");
    assert_eq!(resp["name"], "My Group");
}

#[test]
fn add_frame_to_packed_ode() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode");
    let out = ode_cmd()
        .args(["new", file.to_str().unwrap(), "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    assert!(out.status.success(), "new packed failed: {}", String::from_utf8_lossy(&out.stderr));

    let out = ode_cmd()
        .args([
            "add", "frame",
            file.to_str().unwrap(),
            "--name", "Packed Card",
            "--width", "320",
            "--height", "200",
            "--fill", "#FF6600",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "add to packed failed: {}", String::from_utf8_lossy(&out.stderr));
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["status"], "ok");
    assert_eq!(resp["kind"], "frame");
    assert_eq!(resp["name"], "Packed Card");

    // Verify the packed file is still a valid ZIP
    let bytes = std::fs::read(&file).unwrap();
    assert_eq!(&bytes[..2], b"PK", "Should remain valid ZIP after add");
}

#[test]
fn add_text_to_unpacked_dir() {
    let dir = tempfile::tempdir().unwrap();
    let design_dir = dir.path().join("design");
    let design_path = format!("{}/", design_dir.to_str().unwrap());

    ode_cmd()
        .args(["new", &design_path, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args([
            "add", "text",
            design_dir.to_str().unwrap(),
            "--content", "Hello Unpacked",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "add to unpacked failed: {}", String::from_utf8_lossy(&out.stderr));
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["status"], "ok");
    assert_eq!(resp["name"], "Text");

    // Verify document.json was updated
    let doc: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(design_dir.join("document.json")).unwrap(),
    )
    .unwrap();
    let nodes = doc["nodes"].as_array().unwrap();
    assert!(nodes.len() >= 2, "Should have root frame + text node");
}

#[test]
fn add_text_default_sizing_mode_is_auto_height() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args(["add", "text", &file, "--content", "Hello world"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let text_node = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["type"] == "text")
        .expect("text node not found");
    assert_eq!(text_node["sizing_mode"], "auto-height");
}

#[test]
fn add_frame_with_linear_gradient_fill() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args([
            "add", "frame", &file,
            "--width", "400", "--height", "300",
            "--fill", "linear-gradient(90deg, #FF0000, #0000FF)",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let frame = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["type"] == "frame" && n["name"] == "Frame")
        .unwrap();
    let fill_type = frame["visual"]["fills"][0]["paint"]["type"].as_str().unwrap();
    assert_eq!(fill_type, "linear-gradient");
}

#[test]
fn add_text_with_explicit_sizing_mode() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args([
            "add", "text", &file,
            "--content", "Hello",
            "--text-sizing", "fixed",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());

    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let text_node = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["type"] == "text")
        .expect("text node not found");
    assert_eq!(text_node["sizing_mode"], "fixed");
}
