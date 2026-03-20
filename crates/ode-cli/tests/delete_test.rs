use std::process::Command;

fn ode_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ode"))
}

#[test]
fn delete_node_and_descendants() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args([
            "add", "frame", &file,
            "--name", "Parent",
            "--width", "200",
            "--height", "200",
        ])
        .output()
        .unwrap();
    let parent_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .unwrap()["stable_id"]
        .as_str()
        .unwrap()
        .into();

    let out = ode_cmd()
        .args([
            "add", "text", &file,
            "--content", "Child",
            "--parent", &parent_id,
        ])
        .output()
        .unwrap();
    assert!(out.status.success());

    let out = ode_cmd()
        .args(["delete", &file, &parent_id])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["deleted"].as_array().unwrap().len(), 2);
}

#[test]
fn delete_nonexistent_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args(["delete", &file, "nonexistent"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["code"], "NOT_FOUND");
}

#[test]
fn delete_node_from_packed_ode() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args([
            "add", "frame", &file,
            "--name", "ToDelete",
            "--width", "100",
            "--height", "100",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "add failed: {}", String::from_utf8_lossy(&out.stderr));
    let id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .unwrap()["stable_id"]
        .as_str()
        .unwrap()
        .into();

    let out = ode_cmd()
        .args(["delete", &file, &id])
        .output()
        .unwrap();
    assert!(out.status.success(), "delete from packed failed: {}", String::from_utf8_lossy(&out.stderr));
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["deleted"].as_array().unwrap().len(), 1);
}
