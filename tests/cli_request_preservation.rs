//! Execute the actual CLI and check input bytes across rejected writes.
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nodes-request-{}-{stamp}-{seq}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli(command: &str, paths: &[&Path], expected_code: i32) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_nodes-cli"))
        .arg(command)
        .args(paths)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn build_rejects_request_as_destination_including_path_alias() {
    let temp = Temp::new();
    let request = temp.0.join("build.json");
    let alias = temp.0.join("sub").join("..").join("build.json");
    fs::create_dir(temp.0.join("sub")).unwrap();
    let bytes = include_bytes!("../examples/interop/ai-add.json");
    fs::write(&request, bytes).unwrap();
    for destination in [&request, &alias] {
        let result = cli("build", &[&request, destination], 1);
        assert_eq!(result["error"]["code"], "invalid_destination");
        assert_eq!(fs::read(&request).unwrap(), bytes.as_slice());
    }
    let project = temp.0.join("output.morph");
    cli("build", &[&request, &project], 0);
    assert_eq!(fs::read(&request).unwrap(), bytes.as_slice());
}

#[test]
fn apply_protects_request_but_allows_in_place_project_save() {
    let temp = Temp::new();
    let build = temp.0.join("build.json");
    let project = temp.0.join("project.morph");
    let edit = temp.0.join("edit.json");
    fs::write(&build, include_bytes!("../examples/interop/ai-add.json")).unwrap();
    cli("build", &[&build, &project], 0);
    let before = fs::read(&project).unwrap();
    let bytes = br#"{"api_version":1,"edits":[{"op":"set","node":"0","input":"value","value":11}]}"#;
    fs::write(&edit, bytes).unwrap();
    let result = cli("apply", &[&project, &edit, &edit], 1);
    assert_eq!(result["error"]["code"], "invalid_destination");
    assert_eq!(fs::read(&edit).unwrap(), bytes.as_slice());
    assert_eq!(fs::read(&project).unwrap(), before);
    cli("apply", &[&project, &edit, &project], 0);
    assert_ne!(fs::read(&project).unwrap(), before);
    assert_eq!(fs::read(&edit).unwrap(), bytes.as_slice());
    cli("validate", &[&project], 0);
}
