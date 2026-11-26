//! Data shape (item / list / tree) and the 値を見る viewer.
//!
//! Everything here reads the latest evaluation the app already holds; nothing evaluates.
use crate::theme;
use eframe::egui::{self, RichText};
use morphlet::model::{Tree, Value};

/// Bound visible widget construction, not the inspected output.
pub const ROWS_PER_PAGE: usize = 32;

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
            "曲線 · {} 点 · {}",
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

/// Number of display rows including one marker for each empty branch.
fn row_count(tree: &Tree, branch: Option<usize>) -> usize {
    if let Some(ordinal) = branch {
        return tree
            .branches
            .values()
            .nth(ordinal)
            .map_or(0, |items| items.len().max(1));
    }
    tree.branches.values().map(|items| items.len().max(1)).sum()
}

fn row_window(tree: &Tree, branch: Option<usize>, start: usize, limit: usize) -> Vec<Row> {
    let mut rendered = Vec::new();
    let mut skip = start;
    for (ordinal, (path, values)) in tree.branches.iter().enumerate() {
        if branch.is_some_and(|target| ordinal != target) {
            continue;
        }
        if rendered.len() >= limit {
            break;
        }
        let count = values.len().max(1);
        if skip >= count {
            skip -= count;
            continue;
        }
        let path = path_text(path);
        if values.is_empty() {
            rendered.push(Row {
                path,
                index: None,
                text: "（空の枝）".into(),
            });
        } else {
            for (index, value) in values
                .iter()
                .enumerate()
                .skip(skip)
                .take(limit - rendered.len())
            {
                rendered.push(Row {
                    path: path.clone(),
                    index: Some(index),
                    text: value_text(value),
                });
            }
        }
        skip = 0;
        if branch.is_some() {
            break;
        }
    }
    rendered
}

#[derive(Clone, Copy, Default)]
struct Browse {
    one_branch: bool,
    branch: usize,
    page: usize,
}

fn go(ui: &mut egui::Ui, current: &mut usize, total: usize, title: &str, salt: (&str, u64)) {
    if total < 2 {
        return;
    }
    ui.horizontal(|ui| {
        ui.label(theme::small(title));
        let last = ui.add_enabled(*current > 0, egui::Button::new("‹").small());
        if last.clicked() {
            *current -= 1;
        }
        let mut display = *current + 1;
        if ui
            .add(egui::DragValue::new(&mut display).range(1..=total).speed(1))
            .changed()
        {
            *current = display - 1;
        }
        ui.label(theme::small(format!("/ {total}")));
        let next = ui.add_enabled(*current + 1 < total, egui::Button::new("›").small());
        if next.clicked() {
            *current += 1;
        }
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new(("data_view_next", salt, title)), next.rect));
        let _ = salt;
    });
}

/// Scrollable, paged table of a node's entire output tree.
pub fn show_tree(ui: &mut egui::Ui, tree: &Tree, salt: &str, node: u64, max_height: f32) {
    let shape = shape_of(tree);
    ui.label(
        RichText::new(format!("形: {}", shape.describe()))
            .size(12.)
            .color(theme::TEAL),
    );
    if tree.branches.is_empty() {
        ui.label(theme::small("値はありません。"));
        return;
    }
    let id = ui.id().with(("data_view", salt, node));
    let mut view = ui
        .ctx()
        .data_mut(|d| d.get_temp::<Browse>(id))
        .unwrap_or_default();
    view.branch = view.branch.min(tree.branches.len() - 1);
    let old_filter = (view.one_branch, view.branch);
    if tree.branches.len() > 1 {
        ui.checkbox(&mut view.one_branch, "枝を 1 本だけ表示");
        if view.one_branch {
            go(
                ui,
                &mut view.branch,
                tree.branches.len(),
                "枝の順番",
                (salt, node),
            );
        }
    } else {
        view.one_branch = false;
    }
    if old_filter != (view.one_branch, view.branch) {
        view.page = 0;
    }
    let branch = view.one_branch.then_some(view.branch);
    let total = row_count(tree, branch);
    let pages = total.div_ceil(ROWS_PER_PAGE).max(1);
    view.page = view.page.min(pages - 1);
    go(ui, &mut view.page, pages, "ページ", (salt, node));
    let start = view.page * ROWS_PER_PAGE;
    let rows = row_window(tree, branch, start, ROWS_PER_PAGE);
    if total > ROWS_PER_PAGE {
        ui.label(theme::small(format!(
            "{}–{} 行 / {total}",
            start + 1,
            start + rows.len()
        )));
    }
    egui::ScrollArea::both()
        .id_salt(("data_view_scroll", salt, node, branch, view.page))
        .max_height(max_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new(("data_view_grid", salt, node))
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
        });
    ui.ctx().data_mut(|d| d.insert_temp(id, view));
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
    fn real_pager_clicks_can_reach_beyond_the_old_row_cutoff() {
        let ctx = egui::Context::default();
        let tree = numbers(&[(&[0], 600), (&[1], 0), (&[2, 0], 12)]);
        let mut time = 0.;
        let mut step = |events| {
            time += 1. / 60.;
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700., 640.),
                )),
                events,
                time: Some(time),
                ..Default::default()
            };
            ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show_tree(ui, &tree, "test", 5, 450.));
            })
        };
        step(Vec::new());
        for _ in 0..16 {
            let at = ctx
                .data_mut(|d| {
                    d.get_temp::<egui::Rect>(egui::Id::new((
                        "data_view_next",
                        ("test", 5u64),
                        "ページ",
                    )))
                })
                .unwrap()
                .center();
            for pressed in [true, false] {
                step(vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            step(Vec::new());
        }
        fn has(shape: &egui::epaint::Shape, expected: &str) -> bool {
            match shape {
                egui::epaint::Shape::Text(t) => t.galley.job.text == expected,
                egui::epaint::Shape::Vec(children) => children.iter().any(|s| has(s, expected)),
                _ => false,
            }
        }
        let last = step(Vec::new());
        assert!(
            last.shapes
                .iter()
                .any(|s| has(&s.shape, "513–544 行 / 613"))
        );
        assert!(last.shapes.iter().any(|s| has(&s.shape, "512.0000")));
    }

    #[test]
    fn every_row_is_reachable_and_empty_paths_count_as_one_row() {
        let t = numbers(&[(&[0], 600), (&[1], 0), (&[2, 0], 45)]);
        assert_eq!(row_count(&t, None), 646);
        let last_root = row_window(&t, None, 599, ROWS_PER_PAGE);
        assert_eq!(last_root.len(), ROWS_PER_PAGE);
        assert_eq!(
            (last_root[0].path.as_str(), last_root[0].index),
            ("{0}", Some(599))
        );
        assert_eq!(
            (last_root[1].path.as_str(), last_root[1].index),
            ("{1}", None)
        );
        assert_eq!(
            (last_root[2].path.as_str(), last_root[2].index),
            ("{2;0}", Some(0))
        );
        let last = row_window(&t, Some(2), 32, ROWS_PER_PAGE);
        assert_eq!(last.len(), 13);
        assert_eq!(
            (last[12].path.as_str(), last[12].index),
            ("{2;0}", Some(44))
        );
    }
}
