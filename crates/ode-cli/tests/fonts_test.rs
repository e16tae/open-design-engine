use std::process::Command;

fn ode_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ode"))
}

#[test]
fn fonts_list_returns_json_array() {
    let out = ode_cmd()
        .args(["fonts", "list"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(resp.is_array(), "expected JSON array, got: {resp}");
    if cfg!(target_os = "macos") {
        assert!(!resp.as_array().unwrap().is_empty());
    }
}

#[test]
fn fonts_check_existing_font() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let out = ode_cmd()
        .args(["fonts", "check", "Arial"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["available"], true);
    assert!(!resp["weights"].as_array().unwrap().is_empty());
}

#[test]
fn fonts_check_missing_font() {
    let out = ode_cmd()
        .args(["fonts", "check", "NonexistentFont12345"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["available"], false);
    assert!(resp["weights"].as_array().unwrap().is_empty());
}

#[test]
fn fonts_audit_document() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd()
        .args(["new", &file, "--width", "800", "--height", "600"])
        .output()
        .unwrap();
    ode_cmd()
        .args(["add", "text", &file, "--content", "Hello", "--font-family", "Inter"])
        .output()
        .unwrap();

    let out = ode_cmd()
        .args(["fonts", "audit", &file])
        .output()
        .unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(resp["used"].as_array().unwrap().iter().any(|v| v == "Inter"));
}
