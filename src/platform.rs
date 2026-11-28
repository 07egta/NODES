//! Native desktop paths and executable selection without extra dependencies.
//! Existing Windows/Linux directories are intentionally unchanged.
use std::path::{Path, PathBuf};

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn mac_native_or_legacy(home: &Path, legacy: &str) -> PathBuf {
    let old = home.join(legacy);
    if old.is_dir() {
        old
    } else {
        home.join("Library/Application Support/NODES")
    }
}

/// Recovery and UI state share a directory. Retain old macOS installations.
pub fn data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("NODES"));
    }
    if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(base).join("morphlet"));
    }
    home().map(|home| {
        if cfg!(target_os = "macos") {
            mac_native_or_legacy(&home, ".local/share/morphlet")
        } else {
            home.join(".local/share/morphlet")
        }
    })
}

/// Export settings retain their original Windows and Linux locations.
pub fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("NODES"));
    }
    if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(base).join("NODES"));
    }
    home().map(|home| {
        if cfg!(target_os = "macos") {
            mac_native_or_legacy(&home, ".config/NODES")
        } else {
            home.join(".config/NODES")
        }
    })
}

/// Only regular executable files are selectable; never use a shell to launch them.
pub fn is_slicer_executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        path.extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("nodes-platform-{}-{id}", std::process::id()));
            fs::create_dir(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn mac_paths_preserve_existing_directories_without_moving_files() {
        let root = Scratch::new();
        let legacy = ".local/share/morphlet";
        assert_eq!(
            mac_native_or_legacy(&root.0, legacy),
            root.0.join("Library/Application Support/NODES")
        );
        fs::create_dir_all(root.0.join(legacy)).unwrap();
        assert_eq!(mac_native_or_legacy(&root.0, legacy), root.0.join(legacy));
        assert!(!root.0.join("Library").exists());
    }

    #[test]
    fn executable_selection_rejects_missing_paths_and_directories() {
        let root = Scratch::new();
        assert!(!is_slicer_executable(&root.0));
        assert!(!is_slicer_executable(&root.0.join("missing.exe")));
    }

    #[cfg(unix)]
    #[test]
    fn unix_executable_with_japanese_filename_needs_no_exe_suffix() {
        use std::os::unix::fs::PermissionsExt;
        let root = Scratch::new();
        let file = root.0.join("スライサー");
        fs::write(&file, []).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_slicer_executable(&file));
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_slicer_executable(&file));
    }

    #[cfg(windows)]
    #[test]
    fn windows_executable_selection_preserves_case_insensitive_suffix() {
        let root = Scratch::new();
        let file = root.0.join("スライサー.EXE");
        fs::write(&file, []).unwrap();
        assert!(is_slicer_executable(&file));
        let text = root.0.join("notes.txt");
        fs::write(&text, []).unwrap();
        assert!(!is_slicer_executable(&text));
    }
}
