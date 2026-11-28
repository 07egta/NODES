//! Project lifecycle regression tests: real filesystem operations.
use nodes::engine::Evaluator;
use nodes::model::NodeKind;
use nodes::persistence::{self, MAX_PROJECT_BYTES};
use nodes::templates;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        loop {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "nodes-continuity-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&dir) {
                Ok(()) => return Self(dir),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("cannot create test directory: {e}"),
            }
        }
    }

    fn project(&self) -> PathBuf {
        self.0.join("作品.morph")
    }

    fn count(&self) -> usize {
        fs::read_dir(&self.0).unwrap().count()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn first_save_and_multiple_generations_preserve_previous_exact_bytes() {
    let temp = TempDir::new();
    let path = temp.project();
    let first = templates::create(0);
    assert_eq!(persistence::save_with_backup(&path, &first).unwrap(), None);
    let bytes_first = fs::read(&path).unwrap();
    let mut second = first.clone();
    second.name = "second".into();
    let backup_first = persistence::save_with_backup(&path, &second)
        .unwrap()
        .unwrap();
    let bytes_second = fs::read(&path).unwrap();
    let mut third = second.clone();
    third.name = "third".into();
    let backup_second = persistence::save_with_backup(&path, &third)
        .unwrap()
        .unwrap();

    assert_ne!(backup_first, backup_second);
    assert_eq!(backup_first.parent(), Some(temp.0.as_path()));
    assert_eq!(backup_second.extension(), Some(std::ffi::OsStr::new("morph")));
    assert_eq!(fs::read(&backup_first).unwrap(), bytes_first);
    assert_eq!(fs::read(&backup_second).unwrap(), bytes_second);
    assert_eq!(persistence::load(&backup_first).unwrap(), first);
    assert_eq!(persistence::load(&backup_second).unwrap(), second);
    assert_eq!(persistence::load(&path).unwrap(), third);
    assert_eq!(temp.count(), 3);
}

#[test]
fn unknown_schema_unknown_node_and_corruption_remain_byte_exact() {
    let temp = TempDir::new();
    let path = temp.project();
    let project = templates::create(0);
    let mut future = serde_json::to_value(&project).unwrap();
    future["schema_version"] = 42.into();
    future["future_payload"] = serde_json::json!({"data": [1, 2, 3]});
    let mut unknown = serde_json::to_value(&project).unwrap();
    unknown["graph"]["nodes"][0]["kind"] = "FutureNode".into();
    let mut extra = serde_json::to_value(&project).unwrap();
    extra["future_payload"] = serde_json::json!({"data": [1, 2, 3]});
    for bytes in [
        serde_json::to_vec(&future).unwrap(),
        serde_json::to_vec(&unknown).unwrap(),
        serde_json::to_vec(&extra).unwrap(),
        b"{truncated".to_vec(),
    ] {
        fs::write(&path, &bytes).unwrap();
        assert!(persistence::load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        // If the user explicitly chooses to overwrite an existing file, retain
        // every original byte, including fields this version cannot interpret.
        let backup = persistence::save_with_backup(&path, &project)
            .unwrap()
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), bytes);
        assert_eq!(persistence::load(&path).unwrap(), project);
    }
    assert_eq!(temp.count(), 5);
}

#[test]
fn failed_validation_or_oversized_previous_file_preserves_destination() {
    let temp = TempDir::new();
    let path = temp.project();
    let valid = templates::create(0);
    assert_eq!(persistence::save_with_backup(&path, &valid).unwrap(), None);
    let before = fs::read(&path).unwrap();
    let mut invalid = valid.clone();
    invalid.zoom = f32::NAN;
    assert!(persistence::save_with_backup(&path, &invalid).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(temp.count(), 1);

    let oversized = vec![b'x'; MAX_PROJECT_BYTES + 1];
    fs::write(&path, &oversized).unwrap();
    assert!(persistence::save_with_backup(&path, &valid).is_err());
    assert_eq!(fs::read(&path).unwrap(), oversized);
    assert_eq!(temp.count(), 1);
}

#[test]
fn save_reload_and_restore_preserve_tree_results_wires_and_next_id() {
    let temp = TempDir::new();
    let path = temp.project();
    let mut project = templates::create(0);
    project.pan = [-125.25, 62.5];
    project.zoom = 0.75;
    let series = project.graph.add(NodeKind::Series, [0., 0.]).unwrap();
    let graft = project.graph.add(NodeKind::Graft, [200., 0.]).unwrap();
    project.graph.connect(series, graft, "geometry").unwrap();
    let before = Evaluator::default().evaluate(&project.graph, &|| false);
    assert!(before.outputs[&graft].as_ref().unwrap().branches.len() > 1);

    assert_eq!(persistence::save_with_backup(&path, &project).unwrap(), None);
    let mut reopened = persistence::load(&path).unwrap();
    assert_eq!(reopened, project);
    assert!(persistence::same_parameter_bits(&reopened.graph, &project.graph));
    let after = Evaluator::default().evaluate(&reopened.graph, &|| false);
    assert_eq!(before.outputs[&graft], after.outputs[&graft]);

    let next = reopened.graph.add(NodeKind::Number, [0., 200.]).unwrap();
    assert_eq!(next, project.graph.next_id);
    reopened.name = "edited".into();
    let backup = persistence::save_with_backup(&path, &reopened)
        .unwrap()
        .unwrap();
    assert_eq!(persistence::load(&path).unwrap(), reopened);
    let recovered = persistence::load(&backup).unwrap();
    assert_eq!(recovered, project);
    let restored = Evaluator::default().evaluate(&recovered.graph, &|| false);
    assert_eq!(restored.outputs[&graft], before.outputs[&graft]);
}

#[test]
fn ordinary_autosave_does_not_generate_manual_backup_history() {
    let temp = TempDir::new();
    let path = temp.project();
    let first = templates::create(0);
    let mut second = first.clone();
    second.name = "autosaved".into();
    persistence::save(&path, &first).unwrap();
    persistence::save(&path, &second).unwrap();
    assert_eq!(persistence::load(&path).unwrap(), second);
    assert_eq!(temp.count(), 1);
}

#[cfg(unix)]
#[test]
fn symlink_destination_is_not_replaced_or_followed() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new();
    let original = temp.0.join("original.morph");
    let path = temp.project();
    let project = templates::create(0);
    persistence::save(&original, &project).unwrap();
    let original_bytes = fs::read(&original).unwrap();
    symlink(&original, &path).unwrap();

    assert!(persistence::save_with_backup(&path, &project).is_err());
    assert!(fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
    assert_eq!(fs::read(&original).unwrap(), original_bytes);
    assert_eq!(temp.count(), 2);
}

#[cfg(windows)]
#[test]
fn locked_destination_keeps_the_previous_document() {
    use std::os::windows::fs::OpenOptionsExt;

    let temp = TempDir::new();
    let path = temp.project();
    let project = templates::create(0);
    persistence::save(&path, &project).unwrap();
    let before = fs::read(&path).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();

    assert!(persistence::save_with_backup(&path, &project).is_err());
    drop(lock);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(temp.count(), 1);
}
