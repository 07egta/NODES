//! Data shape (item / list / tree) and the 値を見る viewer.
//!
//! Everything here reads the latest evaluation the app already holds; nothing evaluates.
use crate::theme;
use eframe::egui::{self, RichText};
use nodes::model::{Tree, Value};

/// At most this many value rows are listed; the rest are summarised as 「…ほか N 件」.
pub const MAX_ROWS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataShape {
    /// No values at all (also the output of a disabled node).
    Empty,
    /// Exactly one value in one branch.
    Item,
    /// One branch holding several values.
    List(usize),
    /// More than one branch.
    Tree { branches: usize, items: usize },
}

pub fn shape_of(tree: &Tree) -> DataShape {
    let items = tree.item_count();
    let branches = tree.branches.len();
    if branches > 1 {
        DataShape::Tree { branches, items }
    } else if items == 0 {
        DataShape::Empty
    } else if items == 1 {
        DataShape::Item
    } else {
        DataShape::List(items)
    }
}

impl DataShape {
    /// Short text for the badge next to an output port: "1", "12", "{3}×12".
    #[cfg(test)]
    pub fn badge(self) -> String {
        match self {
            Self::Empty => "0".into(),
            Self::Item => "1".into(),
            Self::List(n) => n.to_string(),
            Self::Tree { branches, items } => format!("{{{branches}}}×{items}"),
        }
    }
    /// Plain Japanese explanation used in tooltips and the viewer.
    pub fn describe(self) -> String {
        match self {
            Self::Empty => "空（値なし）".into(),
            Self::Item => "1 個の値".into(),
            Self::List(n) => format!("リスト（{n} 個）"),
            Self::Tree { branches, items } => {
                format!("ツリー（{branches} 本の枝 · 合計 {items} 個）")
            }
        }
    }
}

pub fn path_text(path: &[u32]) -> String {
    let inner: Vec<String> = path.iter().map(|x| x.to_string()).collect();
    format!("{{{}}}", inner.join(";"))
}

/// A compact preview; full-precision text is available separately for copying.
fn compact_number(value: f64) -> String {
    let magnitude = value.abs();
    if value.is_finite() && value != 0.0 && !(1e-4..1e6).contains(&magnitude) {
        let preview = format!("{value:.4e}");
        if preview
            .parse::<f64>()
            .is_ok_and(|rounded| rounded.is_finite() && rounded != 0.0)
        {
            preview
        } else {
            // Rounding f64::MAX upward to five significant digits would describe
            // an infinite parsed value. Shortest scientific text stays bounded
            // and retains a finite, nonzero value even at the range endpoints.
            format!("{value:e}")
        }
    } else {
        // Keep ordinary four-place values and the existing sign of zero.
        format!("{value:.4}")
    }
}

/// Shortest scientific text recovers each finite component when parsed again.
/// Unlike default decimal display, very small values do not expand to long rows.
fn full_value_text(value: &Value) -> Option<String> {
    match value {
        Value::Number(value) => Some(format!("{value:e}")),
        Value::Point(p) => Some(format!("({:e}, {:e}, {:e})", p[0], p[1], p[2])),
        _ => None,
    }
}

pub fn value_text(value: &Value) -> String {
    match value {
        Value::Number(v) => compact_number(*v),
        Value::Point(p) => format!(
            "({}, {}, {})",
            compact_number(p[0]),
            compact_number(p[1]),
            compact_number(p[2])
        ),
        Value::Curve(c) => format!(
            "{} · 表示 {} 点 · {}",
            match c.geometry.as_ref() {
                Some(g) if g.as_nurbs().is_some() => "NURBS",
                Some(_) => "解析曲線",
                None => "折れ線（近似を含む）",
            },
            c.points.len(),
            if c.closed { "閉じた" } else { "開いた" }
        ),
        Value::Mesh(m) => format!(
            "メッシュ · {} 頂点 / {} 三角形",
            m.vertices.len(),
            m.triangles.len()
        ),
        Value::Surface(s) => format!("数学曲面 · {} · UV 0–1", s.geometry().name()),
        Value::Brep(b) => format!(
            "厳密 B-rep · {} 面 / {} 稜線 · {}",
            b.face_count(),
            b.edge_count(),
            match b.volume() {
                Some(v) => format!("閉じた立体 · 体積 {}", compact_number(v)),
                None => "開いた面".to_string(),
            }
        ),
        Value::Cage(c) => format!(
            "SubD ケージ · {} 点 / {} 面{}",
            c.vertices().len(),
            c.faces().len(),
            if c.grid().is_some() {
                " · 格子（NURBS 変換可）"
            } else {
                ""
            }
        ),
        Value::Gcode(j) => j.summary(),
        Value::Solid(s) => format!(
            "閉じた B-rep · {} 面 / {} 稜線 · 体積 {}",
            s.brep().shell().faces().len(),
            s.brep().edges().len(),
            compact_number(s.volume())
        ),
    }
}

/// Compact, bounded probe shared by wire and output-port tooltips.
fn probe_lines(tree: &Tree) -> Vec<String> {
    const BRANCH_LIMIT: usize = 4;
    const ITEM_LIMIT: usize = 3;
    let mut lines = vec![format!(
        "{} 枝 / {} 個",
        tree.branches.len(),
        tree.item_count()
    )];
    for (path, items) in tree.branches.iter().take(BRANCH_LIMIT) {
        lines.push(format!("{} · {} 個", path_text(path), items.len()));
        for value in items.iter().take(ITEM_LIMIT) {
            // Keep the tooltip narrow even for a long textual geometry summary.
            let preview: String = value_text(value).chars().take(96).collect();
            lines.push(format!("  {preview}"));
        }
        if items.len() > ITEM_LIMIT {
            lines.push(format!("  … 残り {} 個", items.len() - ITEM_LIMIT));
        }
    }
    if tree.branches.len() > BRANCH_LIMIT {
        lines.push(format!("… 残り {} 枝", tree.branches.len() - BRANCH_LIMIT));
    }
    lines
}

pub fn show_probe(ui: &mut egui::Ui, tree: &Tree) {
    for line in probe_lines(tree) {
        ui.label(theme::small(line));
    }
}

fn show_value(ui: &mut egui::Ui, value: &Value) {
    let label = RichText::new(value_text(value)).size(11.).monospace();
    if !matches!(value, Value::Number(_) | Value::Point(_)) {
        ui.label(label);
        return;
    }
    // Detail formatting is lazy: a large viewer does not allocate full text for
    // every value each frame. The click sense is required for a context menu.
    ui.add(egui::Label::new(label).sense(egui::Sense::click()))
        .on_hover_ui(|ui| {
            ui.label("詳しい値");
            if let Some(text) = full_value_text(value) {
                ui.monospace(text);
            }
            ui.label(theme::small("右クリックで、この精度の値をコピー"));
        })
        .context_menu(|ui| {
            if ui.button("詳しい値をコピー").clicked() {
                if let Some(text) = full_value_text(value) {
                    ui.ctx().copy_text(text);
                }
                ui.close();
            }
        });
}

/// One row of the viewer: branch path, item index (None for an empty branch), text.
#[cfg(test)]
pub struct Row {
    pub path: String,
    pub index: Option<usize>,
    pub text: String,
}

/// The first `limit` rows and the number of rows left out.
#[cfg(test)]
pub fn rows(tree: &Tree, limit: usize) -> (Vec<Row>, usize) {
    let mut out = Vec::new();
    let mut hidden = 0usize;
    for (path, values) in &tree.branches {
        if values.is_empty() {
            if out.len() < limit {
                out.push(Row {
                    path: path_text(path),
                    index: None,
                    text: "（空の枝）".into(),
                });
            } else {
                hidden = hidden.saturating_add(1);
            }
            continue;
        }
        for (index, value) in values.iter().enumerate() {
            if out.len() < limit {
                out.push(Row {
                    path: path_text(path),
                    index: Some(index),
                    text: value_text(value),
                });
            } else {
                hidden = hidden.saturating_add(1);
            }
        }
    }
    (out, hidden)
}

const BRANCH_PAGE: usize = 32;
const ITEM_PAGE: usize = 100;

fn page_bounds(count: usize, requested: usize, size: usize) -> (usize, usize, usize) {
    let last = count.saturating_sub(1) / size;
    let page = requested.min(last);
    let start = page * size;
    (page, start, start.saturating_add(size).min(count))
}

fn page_control(ui: &mut egui::Ui, id: egui::Id, count: usize, size: usize) -> (usize, usize) {
    let requested = ui.data_mut(|d| d.get_temp::<usize>(id).unwrap_or_default());
    let (mut page, _, _) = page_bounds(count, requested, size);
    let last = count.saturating_sub(1) / size;
    if last > 0 {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(page > 0, egui::Button::new("前へ"))
                .clicked()
            {
                page -= 1;
            }
            ui.label(format!("{} / {}", page + 1, last + 1));
            if ui
                .add_enabled(page < last, egui::Button::new("次へ"))
                .clicked()
            {
                page += 1;
            }
            ui.add(
                egui::DragValue::new(&mut page)
                    .range(0..=last)
                    .prefix("頁 (0始まり) "),
            );
        });
    }
    let (page, start, end) = page_bounds(count, page, size);
    ui.data_mut(|d| d.insert_temp(id, page));
    (start, end)
}

/// Scrollable, paginated view of a node's output tree. No values are truncated
/// permanently: branch and item pages can reach every entry, including empty paths.
pub fn show_tree(ui: &mut egui::Ui, tree: &Tree, salt: &str, max_height: f32) {
    let shape = shape_of(tree);
    ui.label(
        RichText::new(format!("形: {}", shape.describe()))
            .size(12.)
            .color(theme::teal()),
    );
    if tree.branches.is_empty() {
        ui.label(theme::small("値はありません。"));
        return;
    }
    ui.label(theme::small(format!(
        "{} 枝 · {} 要素",
        tree.branches.len(),
        tree.item_count()
    )));
    ui.label(theme::small("数値・座標：かざして詳細、右クリックでコピー"));
    let (start, end) = page_control(
        ui,
        ui.make_persistent_id((salt, "branches")),
        tree.branches.len(),
        BRANCH_PAGE,
    );
    egui::ScrollArea::vertical()
        .id_salt(("data_view_scroll", salt))
        .max_height(max_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (path, values) in tree.branches.iter().skip(start).take(end - start) {
                egui::CollapsingHeader::new(format!("{} · {} 件", path_text(path), values.len()))
                    .id_salt((salt, path))
                    .default_open(tree.branches.len() == 1)
                    .show(ui, |ui| {
                        if values.is_empty() {
                            ui.label(theme::small("（空の枝）"));
                            return;
                        }
                        let (first, last) = page_control(
                            ui,
                            ui.make_persistent_id((salt, path, "items")),
                            values.len(),
                            ITEM_PAGE,
                        );
                        for (i, value) in values.iter().enumerate().skip(first).take(last - first) {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("[{i}]"))
                                        .size(11.)
                                        .color(theme::muted()),
                                );
                                show_value(ui, value);
                            });
                        }
                    });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn numbers(branches: &[(&[u32], usize)]) -> Tree {
        Tree {
            branches: branches
                .iter()
                .map(|(p, n)| {
                    (
                        p.to_vec(),
                        (0..*n).map(|i| Value::Number(i as f64)).collect(),
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn pages_cover_every_entry_and_clamp_after_tree_shrinks() {
        for (count, size) in [(0usize, 32usize), (1, 32), (32, 32), (33, 32), (1001, 100)] {
            let mut covered = Vec::new();
            for page in 0..=count.saturating_sub(1) / size {
                let (_, start, end) = page_bounds(count, page, size);
                covered.extend(start..end);
            }
            assert_eq!(covered, (0..count).collect::<Vec<_>>());
        }
        assert_eq!(page_bounds(3, usize::MAX, 32), (0, 0, 3));
        assert_eq!(page_bounds(101, 99, 100), (1, 100, 101));
    }

    #[test]
    fn probe_preserves_empty_branches_and_limits_preview_rows() {
        let tree = numbers(&[(&[0], 5), (&[1], 0), (&[2], 1), (&[3], 1), (&[4], 1)]);
        let lines = probe_lines(&tree);
        assert_eq!(lines[0], "5 枝 / 8 個");
        assert!(lines.iter().any(|line| line == "{1} · 0 個"));
        assert!(lines.iter().any(|line| line == "  … 残り 2 個"));
        assert_eq!(lines.last().map(String::as_str), Some("… 残り 1 枝"));
        assert!(!lines.iter().any(|line| line.starts_with("{4}")));
        assert_eq!(probe_lines(&Tree::default()), vec!["0 枝 / 0 個"]);
    }

    #[test]
    fn shapes_and_badges() {
        assert_eq!(shape_of(&Tree::default()), DataShape::Empty);
        assert_eq!(shape_of(&numbers(&[(&[0], 1)])).badge(), "1");
        assert_eq!(shape_of(&numbers(&[(&[0], 12)])).badge(), "12");
        assert_eq!(
            shape_of(&numbers(&[(&[0], 4), (&[1], 4), (&[2], 4)])).badge(),
            "{3}×12"
        );
    }

    #[test]
    fn rows_are_capped_and_counted() {
        let t = numbers(&[(&[0], 600), (&[1], 0)]);
        let (rows, hidden) = rows(&t, MAX_ROWS);
        assert_eq!(rows.len(), MAX_ROWS);
        assert_eq!(hidden, 101);
        assert_eq!(rows[0].text, "0.0000");
        assert_eq!(rows[0].path, "{0}");
        assert_eq!(rows[0].index, Some(0));
        assert_eq!(rows[MAX_ROWS - 1].index, Some(MAX_ROWS - 1));
    }

    #[test]
    fn numeric_previews_keep_extremes_nonzero_and_ordinary_values_readable() {
        for value in [
            5.00000001e-19,
            -5.00000001e-19,
            f64::from_bits(1),
            -f64::from_bits(1),
            f64::MIN_POSITIVE,
            -f64::MIN_POSITIVE,
            f64::MAX,
            -f64::MAX,
            1e6,
            -1e6,
            -1.2345678901234567e-200,
        ] {
            let preview = value_text(&Value::Number(value));
            assert!(preview.len() <= 24, "{preview}");
            assert!(preview.contains('e'), "{preview}");
            let parsed: f64 = preview.parse().unwrap();
            assert!(parsed.is_finite() && parsed != 0.0, "{preview}");
            assert_eq!(parsed.is_sign_negative(), value.is_sign_negative());
        }
        for (value, expected) in [
            (0.0, "0.0000"),
            (-0.0, "-0.0000"),
            (12.34567, "12.3457"),
            (-2.5, "-2.5000"),
            (1e-4, "0.0001"),
            (99_999.125, "99999.1250"),
        ] {
            assert_eq!(value_text(&Value::Number(value)), expected);
        }
        assert_eq!(
            value_text(&Value::Point([1., -0., -2.5])),
            "(1.0000, -0.0000, -2.5000)"
        );
        let point = value_text(&Value::Point([f64::MAX, f64::from_bits(1), -0.]));
        assert!(point.len() <= 3 * 24 + 6);
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(value_text(&Value::Number(value)).len() <= 24);
        }
    }

    #[test]
    fn full_number_and_coordinate_text_preserves_finite_values_exactly() {
        for value in [
            0.0,
            -0.0,
            f64::from_bits(1),
            -f64::from_bits(1),
            f64::MIN_POSITIVE,
            f64::MAX,
            -f64::MAX,
            5.00000001e-19,
            1.2345678901234567,
            std::f64::consts::PI,
        ] {
            let full = full_value_text(&Value::Number(value)).unwrap();
            assert!(full.len() <= 24, "{full}");
            assert_eq!(full.parse::<f64>().unwrap().to_bits(), value.to_bits());
        }
        let coordinates = [1.2345678901234567, f64::from_bits(1), -0.0];
        let full = full_value_text(&Value::Point(coordinates)).unwrap();
        assert!(full.len() <= 3 * 24 + 6);
        let components: Vec<_> = full
            .strip_prefix('(')
            .unwrap()
            .strip_suffix(')')
            .unwrap()
            .split(", ")
            .collect();
        assert_eq!(components.len(), 3);
        for (text, value) in components.into_iter().zip(coordinates) {
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), value.to_bits());
        }
    }
}
