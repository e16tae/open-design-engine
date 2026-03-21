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

#[test]
fn set_on_packed_ode() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode");
    let file_str = file.to_str().unwrap().to_string();

    // Create a packed .ode document with a frame
    ode_cmd()
        .args(["new", &file_str, "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    let out = ode_cmd()
        .args([
            "add", "frame", &file_str, "--name", "Card", "--width", "320", "--height", "200",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "add failed: {}", String::from_utf8_lossy(&out.stderr));
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = resp["stable_id"].as_str().unwrap().to_string();

    // Set properties on the packed file
    let out = ode_cmd()
        .args(["set", &file_str, &id, "--name", "Renamed Card", "--fill", "#00FF00"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "set on packed failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["status"], "ok");
    let modified = resp["modified"].as_array().unwrap();
    let mod_strs: Vec<&str> = modified.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(mod_strs.contains(&"name"));
    assert!(mod_strs.contains(&"fill"));
}

#[test]
fn set_on_unpacked_dir() {
    let dir = tempfile::tempdir().unwrap();
    let design_dir = dir.path().join("design");
    let design_path = format!("{}/", design_dir.to_str().unwrap());

    // Create an unpacked directory with a frame
    ode_cmd()
        .args(["new", &design_path, "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    let design_str = design_dir.to_str().unwrap().to_string();
    let out = ode_cmd()
        .args([
            "add", "frame", &design_str, "--name", "Box", "--width", "100", "--height", "100",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "add failed: {}", String::from_utf8_lossy(&out.stderr));
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = resp["stable_id"].as_str().unwrap().to_string();

    // Set properties on the unpacked directory
    let out = ode_cmd()
        .args(["set", &design_str, &id, "--opacity", "0.8"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "set on unpacked failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["status"], "ok");
    let modified = resp["modified"].as_array().unwrap();
    let mod_strs: Vec<&str> = modified.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(mod_strs.contains(&"opacity"));
}

#[test]
fn set_negative_coordinates() {
    let (_dir, file, id) = setup_doc_with_frame();
    let out = ode_cmd()
        .args(["set", &file, &id, "--x", "-100", "--y", "-50"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["status"], "ok");
    let modified = resp["modified"].as_array().unwrap();
    let mod_strs: Vec<&str> = modified.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(mod_strs.contains(&"x"));
    assert!(mod_strs.contains(&"y"));
}

#[test]
fn set_text_sizing_mode() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
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
        .args(["set", &file, &text_id, "--text-sizing", "auto-width"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let modified = resp["modified"].as_array().unwrap();
    let mod_strs: Vec<&str> = modified.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(mod_strs.contains(&"text-sizing"));

    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let text_node = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["stable_id"].as_str() == Some(text_id.as_str()))
        .unwrap();
    assert_eq!(text_node["sizing_mode"], "auto-width");
}

#[test]
fn set_radial_gradient_fill() {
    let (_dir, file, id) = setup_doc_with_frame();
    let out = ode_cmd()
        .args([
            "set", &file, &id,
            "--fill", "radial-gradient(#16C1F3, #0A1628)",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let node = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["stable_id"].as_str() == Some(&id))
        .unwrap();
    let fill_type = node["visual"]["fills"][0]["paint"]["type"].as_str().unwrap();
    assert_eq!(fill_type, "radial-gradient");
}

#[test]
fn set_solid_replaces_gradient() {
    let (_dir, file, id) = setup_doc_with_frame();
    ode_cmd()
        .args(["set", &file, &id, "--fill", "linear-gradient(90deg, #FF0000, #0000FF)"])
        .output()
        .unwrap();
    let out = ode_cmd()
        .args(["set", &file, &id, "--fill", "#00FF00"])
        .output()
        .unwrap();
    assert!(out.status.success());

    let doc_json = std::fs::read_to_string(&file).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&doc_json).unwrap();
    let node = doc["nodes"].as_array().unwrap()
        .iter()
        .find(|n| n["stable_id"].as_str() == Some(&id))
        .unwrap();
    let fill_type = node["visual"]["fills"][0]["paint"]["type"].as_str().unwrap();
    assert_eq!(fill_type, "solid");
}

#[test]
fn set_invalid_gradient_fails() {
    let (_dir, file, id) = setup_doc_with_frame();
    let out = ode_cmd()
        .args(["set", &file, &id, "--fill", "linear-gradient(abc, #FF0000)"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn set_text_sizing_on_non_text_fails() {
    let (_dir, file, frame_id) = setup_doc_with_frame();
    let out = ode_cmd()
        .args(["set", &file, &frame_id, "--text-sizing", "auto-height"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}
