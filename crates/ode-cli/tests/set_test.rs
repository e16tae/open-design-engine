use std::process::Command;

fn ode_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ode"))
}

fn setup_doc_with_frame() -> (tempfile::TempDir, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("test.ode.json")
        .to_str()
        .unwrap()
        .to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    let out = ode_cmd()
        .args([
            "add", "frame", &file, "--name", "Card", "--width", "320", "--height", "200",
        ])
        .output()
        .unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = resp["stable_id"].as_str().unwrap().to_string();
    (dir, file, id)
}

#[test]
fn set_fill_and_opacity() {
    let (_dir, file, id) = setup_doc_with_frame();
    let out = ode_cmd()
        .args([
            "set", &file, &id, "--fill", "#FF0000", "--opacity", "0.5",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["status"], "ok");
    assert_eq!(resp["stable_id"], id);
    let modified = resp["modified"].as_array().unwrap();
    let mod_strs: Vec<&str> = modified.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(mod_strs.contains(&"fill"), "expected 'fill' in modified");
    assert!(
        mod_strs.contains(&"opacity"),
        "expected 'opacity' in modified"
    );
}

#[test]
fn set_layout_on_non_frame_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("test.ode.json")
        .to_str()
        .unwrap()
        .to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    let out = ode_cmd()
        .args(["add", "text", &file, "--content", "Hello"])
        .output()
        .unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text_id = resp["stable_id"].as_str().unwrap().to_string();

    let out = ode_cmd()
        .args(["set", &file, &text_id, "--layout", "horizontal"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["code"], "INVALID_PROPERTY");
}

#[test]
fn set_nonexistent_node_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir
        .path()
        .join("test.ode.json")
        .to_str()
        .unwrap()
        .to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args(["set", &file, "nonexistent-id", "--name", "Foo"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["code"], "NOT_FOUND");
}
