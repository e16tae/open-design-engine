use std::process::Command;

fn ode_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ode"))
}

#[test]
fn agent_workflow_new_add_set_build() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("card.ode.json").to_str().unwrap().to_string();
    let png = dir.path().join("card.png").to_str().unwrap().to_string();

    // 1. Create document
    let out = ode_cmd().args(["new", &file, "--width", "400", "--height", "300"]).output().unwrap();
    assert!(out.status.success(), "new failed");

    // 2. Add a colored background rect
    let out = ode_cmd()
        .args(["add", "vector", &file, "--shape", "rect", "--width", "400", "--height", "300", "--fill", "#3B82F6"])
        .output().unwrap();
    assert!(out.status.success(), "add vector failed");

    // 3. Add a text label
    let out = ode_cmd()
        .args(["add", "text", &file, "--content", "Hello ODE", "--font-size", "32"])
        .output().unwrap();
    assert!(out.status.success(), "add text failed");
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text_id = resp["stable_id"].as_str().unwrap().to_string();

    // 4. Set text position
    let out = ode_cmd()
        .args(["set", &file, &text_id, "--x", "50", "--y", "130"])
        .output().unwrap();
    assert!(out.status.success(), "set failed");

    // 5. Build to PNG
    let out = ode_cmd()
        .args(["build", &file, "--output", &png])
        .output().unwrap();
    assert!(out.status.success(), "build failed: {}", String::from_utf8_lossy(&out.stdout));

    // 6. Verify PNG exists and is non-empty
    let metadata = std::fs::metadata(&png).unwrap();
    assert!(metadata.len() > 100, "PNG too small: {} bytes", metadata.len());
}

#[test]
fn agent_workflow_add_set_delete_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd().args(["new", &file, "--width", "800", "--height", "600"]).output().unwrap();

    // Add 3 frames
    let mut ids = Vec::new();
    for i in 0..3 {
        let out = ode_cmd()
            .args(["add", "frame", &file, "--name", &format!("Frame {i}"), "--width", "100", "--height", "100"])
            .output().unwrap();
        let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        ids.push(resp["stable_id"].as_str().unwrap().to_string());
    }

    // Delete the middle one
    let out = ode_cmd().args(["delete", &file, &ids[1]]).output().unwrap();
    assert!(out.status.success());

    // Verify document still valid by building
    let svg = dir.path().join("test.svg").to_str().unwrap().to_string();
    let out = ode_cmd().args(["build", &file, "--output", &svg]).output().unwrap();
    assert!(out.status.success(), "build after delete failed: {}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn agent_workflow_packed_ode_new_add_set_build() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("card.ode").to_str().unwrap().to_string();
    let png = dir.path().join("card.png").to_str().unwrap().to_string();

    // 1. Create packed document
    let out = ode_cmd().args(["new", &file, "--width", "400", "--height", "300"]).output().unwrap();
    assert!(out.status.success(), "new packed failed: {}", String::from_utf8_lossy(&out.stderr));

    // 2. Add a colored background rect
    let out = ode_cmd()
        .args(["add", "vector", &file, "--shape", "rect", "--width", "400", "--height", "300", "--fill", "#3B82F6"])
        .output().unwrap();
    assert!(out.status.success(), "add vector to packed failed: {}", String::from_utf8_lossy(&out.stderr));

    // 3. Add a text label
    let out = ode_cmd()
        .args(["add", "text", &file, "--content", "Packed ODE", "--font-size", "32"])
        .output().unwrap();
    assert!(out.status.success(), "add text to packed failed: {}", String::from_utf8_lossy(&out.stderr));
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text_id = resp["stable_id"].as_str().unwrap().to_string();

    // 4. Set text position
    let out = ode_cmd()
        .args(["set", &file, &text_id, "--x", "50", "--y", "130"])
        .output().unwrap();
    assert!(out.status.success(), "set on packed failed: {}", String::from_utf8_lossy(&out.stderr));

    // 5. Build to PNG
    let out = ode_cmd()
        .args(["build", &file, "--output", &png])
        .output().unwrap();
    assert!(out.status.success(), "build packed failed: {}", String::from_utf8_lossy(&out.stderr));

    // 6. Verify PNG exists and is non-empty
    let metadata = std::fs::metadata(&png).unwrap();
    assert!(metadata.len() > 100, "PNG too small: {} bytes", metadata.len());
}
