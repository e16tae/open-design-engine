use std::process::Command;

fn ode_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ode"))
}

#[test]
fn move_node_between_parents() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd().args(["new", &file, "--width", "800", "--height", "600"]).output().unwrap();

    let out = ode_cmd().args(["add", "frame", &file, "--name", "A", "--width", "100", "--height", "100"]).output().unwrap();
    let a_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    let out = ode_cmd().args(["add", "frame", &file, "--name", "B", "--width", "100", "--height", "100"]).output().unwrap();
    let b_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    let out = ode_cmd().args(["add", "text", &file, "--content", "Hi", "--parent", &a_id]).output().unwrap();
    let text_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    let out = ode_cmd().args(["move", &file, &text_id, "--parent", &b_id]).output().unwrap();
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["new_parent"], b_id);
}

#[test]
fn move_to_descendant_fails() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.ode.json").to_str().unwrap().to_string();
    ode_cmd().args(["new", &file, "--width", "800", "--height", "600"]).output().unwrap();

    let out = ode_cmd().args(["add", "frame", &file, "--name", "Parent", "--width", "200", "--height", "200"]).output().unwrap();
    let parent_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    let out = ode_cmd().args(["add", "group", &file, "--parent", &parent_id]).output().unwrap();
    let child_id: String = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["stable_id"].as_str().unwrap().into();

    let out = ode_cmd().args(["move", &file, &parent_id, "--parent", &child_id]).output().unwrap();
    assert!(!out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["code"], "CYCLE_DETECTED");
}
