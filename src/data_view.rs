//! Data shape (item / list / tree) and the 値を見る viewer.
//!
//! Everything here reads the latest evaluation the app already holds; nothing evaluates.
use crate::theme;
use eframe::egui::{self, RichText};
use morphlet::model::{Tree, Value};

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
        Value::Solid(s) => format!(
            "閉じた B-rep · {} 面 / {} 稜線 · 体積 {}",
            s.brep().shell().faces().len(),
            s.brep().edges().len(),
            compact_number(s.volume())
        ),
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

/// Scrollable table of a node's output tree.
pub fn show_tree(ui: &mut egui::Ui, tree: &Tree, salt: &str, max_height: f32) {
    let shape = shape_of(tree);
    ui.label(
        RichText::new(format!("形: {}", shape.describe()))
            .size(12.)
            .color(theme::TEAL),
    );
    if matches!(shape, DataShape::Empty) && tree.branches.is_empty() {
        ui.label(theme::small("値はありません。"));
        return;
    }
    ui.label(theme::small(format!(
        "{} 枝 · {} 要素",
        tree.branches.len(),
        tree.item_count()
    )));
    ui.label(theme::small("数値・座標：かざして詳細、右クリックでコピー"));
    egui::ScrollArea::vertical()
        .id_salt(("data_view_scroll", salt))
        .max_height(max_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let mut shown_rows = 0;
            let mut hidden_branches = 0;
            for (p, values) in tree.branches.iter().take(MAX_ROWS) {
                egui::CollapsingHeader::new(format!("{} · {} 件", path_text(p), values.len()))
                    .id_salt((salt, p))
                    .default_open(tree.branches.len() == 1)
                    .show(ui, |ui| {
                        if values.is_empty() {
                            ui.label(theme::small("（空の枝）"));
                        }
                        let limit = MAX_ROWS.saturating_sub(shown_rows).min(values.len());
                        for (i, v) in values.iter().take(limit).enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("[{i}]"))
                                        .size(11.)
                                        .color(theme::MUTED),
                                );
                                show_value(ui, v);
                            });
                        }
                        shown_rows += limit;
                        if limit < values.len() {
                            ui.label(theme::small(format!("…ほか {} 件", values.len() - limit)));
                        }
                    });
            }
            hidden_branches += tree.branches.len().saturating_sub(MAX_ROWS);
            if hidden_branches > 0 {
                ui.label(theme::small(format!("…ほか {hidden_branches} 枝")));
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
