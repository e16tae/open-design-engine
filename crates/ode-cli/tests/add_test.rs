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
