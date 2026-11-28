//! A small filesystem picker that needs no desktop portal or native dialog.
//! This module only reads directories and returns a chosen path; the caller writes.

use eframe::egui::{self, RichText};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

// Bound visible rows separately from the raw directory scan. Unrelated files
// must not exhaust the display budget before the extension filter runs.
const DIRECTORY_LIMIT: usize = 2_000;
const DIRECTORY_SCAN_LIMIT: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileTask {
    Open,
    Save,
    ExportStl,
    ExportObj,
    ExportPly,
    ExportGcode,
    ExportThreeMf,
    ExportStep,
    PickSlicer,
    SavePart,
    LoadPart,
    /// Mesh file for the ファイル読み込み node: STL or OBJ.
    ImportMesh,
    ImportStep,
}

impl FileTask {
    /// Tasks that pick an existing file (the others name a file to write).
    fn picks_existing(self) -> bool {
        matches!(
            self,
            Self::Open | Self::LoadPart | Self::ImportMesh | Self::ImportStep | Self::PickSlicer
        )
    }
    fn title(self) -> &'static str {
        match self {
            Self::Open => "作品を開く",
            Self::Save => "作品を保存",
            Self::ExportStl => "STL に書き出す",
            Self::ExportObj => "OBJ に書き出す",
            Self::ExportPly => "PLY に書き出す",
            Self::ExportGcode => "G-code に書き出す",
            Self::ExportThreeMf => "3MF に書き出す",
            Self::ExportStep => "STEP を保存",
            Self::PickSlicer => "スライサーを選ぶ",
            Self::SavePart => "部品を書き出す",
            Self::LoadPart => "部品を読み込む",
            Self::ImportMesh => "ファイルを読み込む",
            Self::ImportStep => "STEP を読み込む",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Open | Self::Save => "morph",
            Self::ExportStl | Self::ImportMesh => "stl",
            Self::ExportObj => "obj",
            Self::ExportPly => "ply",
            Self::ExportGcode => "gcode",
            Self::ExportThreeMf => "3mf",
            Self::ExportStep | Self::ImportStep => "step",
            Self::PickSlicer => "exe",
            Self::SavePart | Self::LoadPart => "morphpart",
        }
    }

    /// Whether a file in the listing belongs to this task.
    fn accepts(self, path: &Path) -> bool {
        match self {
            Self::ImportMesh => has_extension(path, "stl") || has_extension(path, "obj"),
            Self::ExportStep | Self::ImportStep => {
                has_extension(path, "step")
                    || has_extension(path, "stp")
                    || has_extension(path, "iges")
                    || has_extension(path, "igs")
            }
            other => has_extension(path, other.extension()),
        }
    }

    /// Extension text for the footer line.
    fn filter_hint(self) -> &'static str {
        match self {
            Self::Open | Self::Save => ".morph",
            Self::ExportStl => ".stl",
            Self::ExportObj => ".obj",
            Self::ExportPly => ".ply",
            Self::SavePart | Self::LoadPart => ".morphpart",
            Self::ImportMesh => ".stl / .obj",
            Self::ExportStep | Self::ImportStep => ".step / .stp / .iges / .igs",
            Self::ExportGcode => ".gcode",
            Self::ExportThreeMf => ".3mf",
            Self::PickSlicer => ".exe",
        }
    }

    fn action_label(self) -> &'static str {
        match self {
            Self::Open => "開く",
            Self::Save => "保存",
            Self::LoadPart | Self::ImportMesh | Self::ImportStep => "読み込む",
            Self::PickSlicer => "選ぶ",
            Self::ExportStl
            | Self::ExportObj
            | Self::ExportPly
            | Self::ExportGcode
            | Self::ExportThreeMf
            | Self::ExportStep
            | Self::SavePart => "書き出す",
        }
    }
}

#[derive(Debug)]
pub enum PickerEvent {
    Cancelled,
    Chosen(PathBuf),
}

struct Entry {
    path: PathBuf,
    name: String,
    sort_key: String,
    is_dir: bool,
}

pub struct FilePicker {
    pub task: FileTask,
    directory: PathBuf,
    location: String,
    filename: String,
    // The path, including any non-UTF-8 filename, is kept intact until the user
    // actually edits its lossy display string.
    selected_file: Option<PathBuf>,
    selected_display: String,
    entries: Vec<Entry>,
    shortcuts: Vec<(String, PathBuf)>,
    error: Option<String>,
    truncated: bool,
    overwrite: Option<PathBuf>,
    focus_filename: bool,
    ime_composing: bool,
}

impl FilePicker {
    pub fn new(task: FileTask, directory: PathBuf, filename: String) -> Self {
        let directory = if directory.is_dir() {
            directory
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };
        let mut picker = Self {
            task,
            location: directory.to_string_lossy().into_owned(),
            directory,
            filename,
            selected_file: None,
            selected_display: String::new(),
            entries: Vec::new(),
            shortcuts: shortcuts(),
            error: None,
            truncated: false,
            overwrite: None,
            focus_filename: true,
            ime_composing: false,
        };
        picker.refresh();
        picker
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<PickerEvent> {
        let was_composing = self.ime_composing;
        ctx.input(|input| {
            for event in &input.events {
                match event {
                    egui::Event::Ime(egui::ImeEvent::Preedit(text)) => {
                        self.ime_composing = !text.is_empty();
                    }
                    egui::Event::Ime(egui::ImeEvent::Commit(_) | egui::ImeEvent::Disabled) => {
                        self.ime_composing = false;
                    }
                    _ => {}
                }
            }
        });
        let width = (ctx.content_rect().width() - 64.0).clamp(280.0, 608.0);
        let list_height = (ctx.content_rect().height() - 285.0).clamp(110.0, 240.0);
        let mut event = None;
        let mut navigate = None;
        let mut selection = None;
        let mut activate_file = false;
        let mut go_to_location = false;
        let mut submit = false;
        let response = egui::Modal::new(egui::Id::new("morphlet_file_picker"))
            .frame(
                egui::Frame::popup(&ctx.style())
                    .inner_margin(16)
                    .corner_radius(12),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.heading(self.task.title());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("閉じる").clicked() {
                            event = Some(PickerEvent::Cancelled);
                        }
                    });
                });
                ui.add_space(4.0);

                if let Some(path) = self.overwrite.clone() {
                    ui.label("同じ名前のファイルがすでにあります。");
                    ui.add(
                        egui::Label::new(RichText::new(display_filename(&path)).strong())
                            .truncate(),
                    )
                    .on_hover_text(path.to_string_lossy());
                    ui.label("このファイルの内容を置き換えますか？");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("名前を変える・戻る").clicked() {
                            self.overwrite = None;
                            self.focus_filename = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(
                                    RichText::new("上書きする")
                                        .color(crate::theme::teal())
                                        .strong(),
                                )
                                .clicked()
                            {
                                event = Some(PickerEvent::Chosen(path));
                            }
                        });
                    });
                    return;
                }

                ui.horizontal(|ui| {
                    let parent = self.directory.parent().map(Path::to_path_buf);
                    if ui
                        .add_enabled(parent.is_some(), egui::Button::new("↑"))
                        .on_hover_text("ひとつ上のフォルダー")
                        .clicked()
                    {
                        navigate = parent;
                    }
                    let available = (ui.available_width() - 60.0).max(100.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.location)
                            .desired_width(available)
                            .hint_text("フォルダーの場所"),
                    );
                    if ui.button("移動").clicked() {
                        go_to_location = true;
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    for (label, directory) in &self.shortcuts {
                        if ui.small_button(label).clicked() {
                            navigate = Some(directory.clone());
                        }
                    }
                    if ui.small_button("更新").clicked() {
                        navigate = Some(self.directory.clone());
                    }
                });

                egui::Frame::new()
                    .fill(ui.visuals().extreme_bg_color)
                    .corner_radius(6)
                    .inner_margin(5)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("morphlet_picker_entries")
                            .auto_shrink([false, false])
                            .max_height(list_height)
                            .min_scrolled_height(list_height)
                            .show(ui, |ui| {
                                ui.set_min_height(list_height);
                                if self.entries.is_empty() {
                                    ui.add_space(16.0);
                                    ui.label(
                                        RichText::new("この場所に対応ファイルはありません").weak(),
                                    );
                                    if !self.task.picks_existing() {
                                        ui.label(
                                            RichText::new(
                                                "下の欄にファイル名を入力して保存できます。",
                                            )
                                            .small()
                                            .weak(),
                                        );
                                    }
                                }
                                for entry in &self.entries {
                                    let selected = self.selected_file.as_ref() == Some(&entry.path);
                                    let label = if entry.is_dir {
                                        RichText::new(format!("▸  {}", entry.name))
                                            .color(crate::theme::teal())
                                    } else {
                                        RichText::new(format!("    {}", entry.name))
                                    };
                                    let row = ui.add_sized(
                                        [ui.available_width(), 27.0],
                                        egui::Button::new(label)
                                            .selected(selected)
                                            .frame(false)
                                            .truncate(),
                                    );
                                    let clicked = row.clicked();
                                    let double_clicked = row.double_clicked();
                                    row.on_hover_text(if entry.is_dir {
                                        format!("フォルダーを開く: {}", entry.name)
                                    } else {
                                        entry.path.to_string_lossy().into_owned()
                                    });
                                    if entry.is_dir {
                                        if clicked || double_clicked {
                                            navigate = Some(entry.path.clone());
                                        }
                                    } else if clicked || double_clicked {
                                        selection = Some(entry.path.clone());
                                        activate_file = double_clicked;
                                    }
                                }
                            });
                    });

                if let Some(error) = &self.error {
                    ui.add(
                        egui::Label::new(
                            RichText::new(error)
                                .small()
                                .color(ui.visuals().error_fg_color),
                        )
                        .wrap(),
                    );
                } else if self.truncated {
                    ui.label(
                        RichText::new(
                            "一覧を一部だけ表示しています（最大 2,000 件）。場所欄へ直接入力できます。",
                        )
                        .small()
                        .weak(),
                    );
                } else {
                    ui.label(
                        RichText::new(format!(
                            "フォルダーはクリックで開く  ·  {} ファイル",
                            self.task.filter_hint()
                        ))
                        .small()
                        .weak(),
                    );
                }

                ui.horizontal(|ui| {
                    ui.label("ファイル名");
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.filename)
                            .desired_width(ui.available_width())
                            .hint_text(format!("作品.{}", self.task.extension())),
                    );
                    if self.focus_filename {
                        field.request_focus();
                        self.focus_filename = false;
                    }
                    if field.changed() {
                        self.selected_file = None;
                        self.error = None;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("キャンセル").clicked() {
                        event = Some(PickerEvent::Cancelled);
                    }
                    ui.label(RichText::new("拡張子がない場合は自動で補います").small().weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                !self.filename.trim().is_empty(),
                                egui::Button::new(
                                    RichText::new(self.task.action_label())
                                        .strong()
                                        .color(crate::theme::teal()),
                                ),
                            )
                            .clicked()
                        {
                            submit = true;
                        }
                    });
                });
            });

        if event.is_some() {
            return event;
        }
        // Escape first belongs to Japanese IME candidate cancellation. It must
        // not discard the picker on the frame that ends a composition.
        let should_close = if was_composing || self.ime_composing {
            response.backdrop_response.clicked() || response.response.should_close()
        } else {
            response.should_close()
        };
        if should_close {
            return Some(PickerEvent::Cancelled);
        }
        if go_to_location {
            self.move_to_location();
        } else if let Some(directory) = navigate {
            self.navigate(directory);
        }
        if let Some(path) = selection {
            self.select(path);
        }
        if submit || activate_file {
            return self.choose();
        }
        None
    }

    fn select(&mut self, path: PathBuf) {
        self.filename = display_filename(&path);
        self.selected_display = self.filename.clone();
        self.selected_file = Some(path);
        self.error = None;
    }

    fn refresh(&mut self) {
        match read_directory(&self.directory, self.task) {
            Ok((entries, truncated)) => {
                self.entries = entries;
                self.truncated = truncated;
                self.error = None;
            }
            Err(error) => {
                self.entries.clear();
                self.truncated = false;
                self.error = Some(error);
            }
        }
    }

    fn navigate(&mut self, directory: PathBuf) {
        match read_directory(&directory, self.task) {
            Ok((entries, truncated)) => {
                self.directory = directory;
                self.location = self.directory.to_string_lossy().into_owned();
                self.entries = entries;
                self.truncated = truncated;
                self.selected_file = None;
                self.error = None;
                self.overwrite = None;
                if self.task.picks_existing() {
                    self.filename.clear();
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn move_to_location(&mut self) {
        let path = if self.location == self.directory.to_string_lossy() {
            self.directory.clone()
        } else {
            self.resolve_text(&self.location)
        };
        if path.is_dir() {
            self.navigate(path);
        } else if path.is_file() {
            if let Some(parent) = path.parent() {
                self.navigate(parent.to_path_buf());
                self.select(path);
            }
        } else {
            self.error =
                Some("指定した場所が見つかりません。フォルダーの場所を確認してください。".into());
        }
    }

    fn resolve_text(&self, text: &str) -> PathBuf {
        if let Some(home) = home_dir() {
            if text == "~" {
                return home;
            }
            if text.starts_with("~/") || text.starts_with("~\\") {
                return home.join(&text[2..]);
            }
        }
        let path = PathBuf::from(text);
        if path.is_absolute() {
            path
        } else {
            self.directory.join(path)
        }
    }

    fn choose(&mut self) -> Option<PickerEvent> {
        if self.filename.trim().is_empty() || self.filename.contains('\0') {
            self.error = Some("ファイル名を入力してください。".into());
            return None;
        }
        let mut path = if self.selected_display == self.filename {
            self.selected_file
                .clone()
                .unwrap_or_else(|| self.resolve_text(&self.filename))
        } else {
            self.resolve_text(&self.filename)
        };
        if path.is_dir() {
            self.navigate(path);
            return None;
        }
        let extension = self.task.extension();
        if !self.task.accepts(&path) {
            if path.extension().is_some() {
                self.error = Some(format!(
                    "対応する拡張子は {} です。ファイル名を確認してください。",
                    self.task.filter_hint()
                ));
                return None;
            }
            path.set_extension(extension);
        }
        if path.file_name().is_none() {
            self.error = Some("ファイル名を指定してください。".into());
            return None;
        }
        if self.task.picks_existing() {
            if !path.is_file() {
                self.error =
                    Some("この作品ファイルが見つかりません。名前と場所を確認してください。".into());
                return None;
            }
            return Some(PickerEvent::Chosen(path));
        }
        let parent_exists = path.parent().is_some_and(Path::is_dir);
        if !parent_exists {
            self.error =
                Some("保存先のフォルダーが見つかりません。場所を選び直してください。".into());
            return None;
        }
        match path.try_exists() {
            Ok(true) if !path.is_file() => {
                self.error = Some(
                    "この名前はフォルダーなどに使われています。別のファイル名を指定してください。"
                        .into(),
                );
                None
            }
            Ok(true) => {
                self.overwrite = Some(path);
                self.error = None;
                None
            }
            Ok(false) => Some(PickerEvent::Chosen(path)),
            Err(error) => {
                self.error = Some(format!("保存先を確認できませんでした: {error}"));
                None
            }
        }
    }
}

fn display_filename(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
}

fn home_dir() -> Option<PathBuf> {
    let home = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    };
    home.map(PathBuf::from).filter(|path| path.is_dir())
}

fn shortcuts() -> Vec<(String, PathBuf)> {
    let mut result = Vec::new();
    if let Some(home) = home_dir() {
        result.push(("ホーム".into(), home.clone()));
        for (label, candidates) in [
            ("ドキュメント", ["Documents", "ドキュメント"]),
            ("デスクトップ", ["Desktop", "デスクトップ"]),
        ] {
            if let Some(path) = candidates
                .iter()
                .map(|name| home.join(name))
                .find(|p| p.is_dir())
            {
                result.push((label.into(), path));
            } else if let Some(one_drive) = std::env::var_os("OneDrive") {
                let one_drive = PathBuf::from(one_drive);
                if let Some(path) = candidates
                    .iter()
                    .map(|name| one_drive.join(name))
                    .find(|p| p.is_dir())
                {
                    result.push((label.into(), path));
                }
            }
        }
    }
    let mut examples = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        examples.push(parent.join("examples"));
    }
    if let Ok(directory) = std::env::current_dir() {
        examples.push(directory.join("examples"));
    }
    if let Some(path) = examples.into_iter().find(|p| p.is_dir()) {
        result.push(("作品例".into(), path));
    }
    result
}

fn read_directory(path: &Path, task: FileTask) -> Result<(Vec<Entry>, bool), String> {
    let directory = fs::read_dir(path).map_err(|e| format!("フォルダーを開けませんでした: {e}"))?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for (index, entry) in directory.enumerate() {
        if index >= DIRECTORY_SCAN_LIMIT {
            truncated = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let is_dir = file_type.is_dir() || (file_type.is_symlink() && path.is_dir());
        let is_file = file_type.is_file() || (file_type.is_symlink() && path.is_file());
        if !is_dir && (!is_file || !task.accepts(&path)) {
            continue;
        }
        entries.push(Entry {
            sort_key: name.to_lowercase(),
            name,
            path,
            is_dir,
        });
        // Periodically keep only the best visible entries. Peak storage is
        // bounded by twice the display limit, even for dense directories.
        if entries.len() >= 2 * DIRECTORY_LIMIT {
            keep_best_entries(&mut entries);
            truncated = true;
        }
    }
    if entries.len() > DIRECTORY_LIMIT {
        keep_best_entries(&mut entries);
        truncated = true;
    }
    entries.sort_unstable_by(compare_entries);
    Ok((entries, truncated))
}

fn compare_entries(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    b.is_dir
        .cmp(&a.is_dir)
        .then_with(|| a.sort_key.cmp(&b.sort_key))
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.path.cmp(&b.path))
}

fn keep_best_entries(entries: &mut Vec<Entry>) {
    if entries.len() > DIRECTORY_LIMIT {
        entries.select_nth_unstable_by(DIRECTORY_LIMIT, compare_entries);
        entries.truncate(DIRECTORY_LIMIT);
    }
}

#[cfg(test)]
mod directory_listing_tests {
    use super::*;

    fn entry(name: String, is_dir: bool) -> Entry {
        Entry {
            path: PathBuf::from(&name),
            sort_key: name.to_lowercase(),
            name,
            is_dir,
        }
    }

    #[test]
    fn bounded_selection_matches_full_sort_in_either_input_order() {
        let make_entries = || {
            (0..4_501)
                .map(|i| entry(format!("item{i:05}.morph"), i % 7 == 0))
                .collect::<Vec<_>>()
        };
        let mut expected = make_entries();
        expected.sort_unstable_by(compare_entries);
        expected.truncate(DIRECTORY_LIMIT);
        let expected: Vec<_> = expected.into_iter().map(|e| e.path).collect();

        for reverse in [false, true] {
            let mut input = make_entries();
            if reverse {
                input.reverse();
            }
            let mut retained = Vec::new();
            for item in input {
                retained.push(item);
                if retained.len() >= 2 * DIRECTORY_LIMIT {
                    keep_best_entries(&mut retained);
                }
            }
            keep_best_entries(&mut retained);
            retained.sort_unstable_by(compare_entries);
            let paths: Vec<_> = retained.into_iter().map(|e| e.path).collect();
            assert_eq!(paths, expected);
        }
    }

    #[test]
    fn sort_is_directory_first_and_breaks_case_ties() {
        let mut entries = [
            entry("a.morph".into(), false),
            entry("A.morph".into(), false),
            entry("zzz".into(), true),
        ];
        entries.sort_unstable_by(compare_entries);
        assert_eq!(
            entries.map(|e| e.name),
            [
                "zzz".to_string(),
                "A.morph".to_string(),
                "a.morph".to_string()
            ]
        );
    }

    struct TempDirectory(PathBuf);

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn step_picker_accepts_both_extensions_and_prompts_for_overwrite() {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("nodes-picker-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let directory = TempDirectory(path);
        let existing = directory.0.join("part.STP");
        fs::File::create(&existing).unwrap();

        let mut picker =
            FilePicker::new(FileTask::ImportStep, directory.0.clone(), "part.STP".into());
        assert!(matches!(picker.choose(), Some(PickerEvent::Chosen(path)) if path == existing));

        let mut picker =
            FilePicker::new(FileTask::ExportStep, directory.0.clone(), "part.STP".into());
        assert!(picker.choose().is_none());
        assert_eq!(picker.overwrite, Some(existing));

        let mut picker = FilePicker::new(FileTask::ExportStep, directory.0.clone(), "new".into());
        assert!(
            matches!(picker.choose(), Some(PickerEvent::Chosen(path)) if path == directory.0.join("new.step"))
        );
    }

    #[test]
    fn explicit_wrong_extension_never_redirects_save_to_another_file() {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("nodes-picker-ext-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let directory = TempDirectory(path);
        for (task, wrong, bare, expected) in [
            (FileTask::Save, "model.stl", "model", "model.morph"),
            (FileTask::ExportStl, "model.obj", "model", "model.stl"),
            (FileTask::ExportStep, "model.txt", "model", "model.step"),
        ] {
            let mut picker = FilePicker::new(task, directory.0.clone(), wrong.into());
            assert!(picker.choose().is_none());
            assert!(picker.error.is_some());
            assert!(picker.overwrite.is_none());
            let mut picker = FilePicker::new(task, directory.0.clone(), bare.into());
            assert!(
                matches!(picker.choose(), Some(PickerEvent::Chosen(path)) if path == directory.0.join(expected))
            );
        }
        let mut picker =
            FilePicker::new(FileTask::ExportStep, directory.0.clone(), "model.IGS".into());
        assert!(
            matches!(picker.choose(), Some(PickerEvent::Chosen(path)) if path == directory.0.join("model.IGS"))
        );
    }

    #[test]
    fn unrelated_files_do_not_consume_the_display_limit() {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("nodes-picker-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let directory = TempDirectory(path);
        for i in 0..=DIRECTORY_LIMIT {
            fs::File::create(directory.0.join(format!("ignored{i:05}.txt"))).unwrap();
        }
        fs::File::create(directory.0.join("project.MORPH")).unwrap();
        fs::create_dir(directory.0.join("subdirectory")).unwrap();

        let (entries, truncated) = read_directory(&directory.0, FileTask::Open).unwrap();
        assert!(!truncated);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].name, "project.MORPH");
    }
}
