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

pub fn value_text(value: &Value) -> String {
    match value {
        Value::Number(v) => format!("{v:.4}"),
        Value::Point(p) => format!("({:.4}, {:.4}, {:.4})", p[0], p[1], p[2]),
        Value::Curve(c) => format!(
            "{} · 表示 {} 点 · {}",
            match c.geometry.as_ref() { Some(g) if g.as_nurbs().is_some() => "NURBS", Some(_) => "解析曲線", None => "折れ線（近似を含む）" },
            c.points.len(),
            if c.closed { "閉じた" } else { "開いた" }
        ),
        Value::Mesh(m) => format!(
            "メッシュ · {} 頂点 / {} 三角形",
            m.vertices.len(),
            m.triangles.len()
        ),
    }
}

/// One row of the viewer: branch path, item index (None for an empty branch), text.
pub struct Row {
    pub path: String,
    pub index: Option<usize>,
    pub text: String,
}

/// The first `limit` rows and the number of rows left out.
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
    let (rows, hidden) = rows(tree, MAX_ROWS);
    egui::ScrollArea::vertical()
        .id_salt(("data_view_scroll", salt))
        .max_height(max_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new(("data_view_grid", salt))
                .num_columns(3)
                .striped(true)
                .spacing([14., 4.])
                .show(ui, |ui| {
                    ui.label(theme::small("枝"));
                    ui.label(theme::small("番号"));
                    ui.label(theme::small("値"));
                    ui.end_row();
                    for row in &rows {
                        ui.label(RichText::new(&row.path).size(11.).color(theme::TEAL));
                        ui.label(
                            RichText::new(row.index.map(|i| i.to_string()).unwrap_or_default())
                                .size(11.)
                                .color(theme::MUTED),
                        );
                        ui.label(RichText::new(&row.text).size(11.).monospace());
                        ui.end_row();
                    }
                });
            if hidden > 0 {
                ui.label(theme::small(format!("…ほか {hidden} 件")));
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
    }
}
