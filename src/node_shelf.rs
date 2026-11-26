//! Node shelf: a strip above the canvas with category tabs and icon buttons, a ★ tab for
//! favourites, "おすすめ次のノード" for the selected node, and drag-to-place.
//!
//! Everything is driven by `NodeKind::all()` and `spec().category`, so node kinds and
//! categories added elsewhere appear here without edits. The rules (grouping, the
//! DataType filter for suggestions, the favourites file format) are plain functions with
//! unit tests. The drawing code is at the bottom of the file.

use crate::{node_icons, theme, ui_polish};
use eframe::egui::{self, Align, Align2, FontId, Layout, Pos2, Rect, RichText, Sense};
use morphlet::model::{DataType, Graph, NodeId, NodeKind};

/// Most suggestions shown for the selected node.
pub const SUGGESTION_LIMIT: usize = 6;
/// Settings-file keys for the saved lists.
const FAVORITES_KEY: &str = "favorites";
const RECENT_KEY: &str = "recent";
/// Width kept free on the right of the strip for the suggestions column.
const SUGGESTION_WIDTH: f32 = 380.;
const BUTTON_W: f32 = 74.;
const BUTTON_H: f32 = 66.;
const ICON: f32 = 26.;

/// Nodes grouped by `spec().category`. Categories come in the order they first appear in
/// `kinds`, and nodes keep the order of `kinds` inside each group.
pub fn group_by_category(kinds: &[NodeKind]) -> Vec<(&'static str, Vec<NodeKind>)> {
    let mut groups: Vec<(&'static str, Vec<NodeKind>)> = Vec::new();
    for &kind in kinds {
        let category = kind.spec().category;
        if let Some(group) = groups.iter_mut().find(|(name, _)| *name == category) {
            group.1.push(kind);
        } else {
            groups.push((category, vec![kind]));
        }
    }
    groups
}

/// Category names in library order (first appearance in `NodeKind::all()`).
pub fn categories() -> Vec<&'static str> {
    group_by_category(NodeKind::all())
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// Every node of one category, in library order.
pub fn kinds_in(category: &str) -> Vec<NodeKind> {
    NodeKind::all()
        .iter()
        .copied()
        .filter(|kind| kind.spec().category == category)
        .collect()
}

/// True when the first input of `kind` can take a value of type `source`.
pub fn first_input_accepts(kind: NodeKind, source: DataType) -> bool {
    kind.spec()
        .inputs
        .first()
        .is_some_and(|input| input.kind.accepts(source))
}

/// Next-node candidates after a node whose output is `source`: favourites first, then
/// recently added nodes, then everything else in library order.
pub fn suggestions(
    source: DataType,
    favorites: &[NodeKind],
    recent: &[NodeKind],
    limit: usize,
) -> Vec<NodeKind> {
    let mut picks: Vec<NodeKind> = Vec::new();
    for &kind in favorites.iter().chain(recent).chain(NodeKind::all()) {
        if picks.len() >= limit {
            break;
        }
        if !picks.contains(&kind) && first_input_accepts(kind, source) {
            picks.push(kind);
        }
    }
    picks
}

/// Input and output in words, for the hover text: `入力: A・B → 出力: 数値`.
pub fn describe_io(kind: NodeKind) -> String {
    let spec = kind.spec();
    let inputs: Vec<&str> = spec.inputs.iter().map(|input| input.label).collect();
    let inputs = if inputs.is_empty() {
        "なし".to_owned()
    } else {
        inputs.join("・")
    };
    format!("入力: {inputs} → 出力: {}", spec.output.label())
}

/// Adds `kind` to the favourites, or removes it when it is already there.
pub fn toggle_favorite(favorites: &mut Vec<NodeKind>, kind: NodeKind) {
    match favorites.iter().position(|k| *k == kind) {
        Some(index) => {
            favorites.remove(index);
        }
        None => favorites.push(kind),
    }
}

/// Stable file name of a kind: `LinearArray` becomes `linear_array`.
pub fn kind_key(kind: NodeKind) -> String {
    let mut key = String::new();
    for (index, c) in format!("{kind:?}").chars().enumerate() {
        if c.is_ascii_uppercase() {
            if index > 0 {
                key.push('_');
            }
            key.push(c.to_ascii_lowercase());
        } else {
            key.push(c);
        }
    }
    key
}

/// Comma-separated file names, e.g. `box,linear_array`.
pub fn format_kinds(kinds: &[NodeKind]) -> String {
    kinds
        .iter()
        .map(|kind| kind_key(*kind))
        .collect::<Vec<_>>()
        .join(",")
}

/// Reads a list written by `format_kinds`. Unknown names are skipped and repeats dropped,
/// so a file saved by another version still loads.
pub fn parse_kinds(text: &str) -> Vec<NodeKind> {
    let mut kinds: Vec<NodeKind> = Vec::new();
    for name in text.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(&kind) = NodeKind::all().iter().find(|kind| kind_key(**kind) == name)
            && !kinds.contains(&kind)
        {
            kinds.push(kind);
        }
    }
    kinds
}

/// Favourites and recent nodes from the settings file (both empty when nothing is saved).
pub fn load_lists() -> (Vec<NodeKind>, Vec<NodeKind>) {
    let read = |key: &str| {
        ui_polish::read_setting(key)
            .map(|text| parse_kinds(&text))
            .unwrap_or_default()
    };
    (read(FAVORITES_KEY), read(RECENT_KEY))
}

/// Saves both lists into the settings file, keeping the other settings lines.
pub fn save_lists(favorites: &[NodeKind], recent: &[NodeKind]) -> Result<(), String> {
    ui_polish::save_setting_line(&format!("{FAVORITES_KEY}={}", format_kinds(favorites)))?;
    ui_polish::save_setting_line(&format!("{RECENT_KEY}={}", format_kinds(recent)))
}

/// Rebuilds the recent list from a saved newest-first list.
pub fn recent_from_saved(saved: &[NodeKind]) -> ui_polish::Recent {
    let mut recent = ui_polish::Recent::default();
    // `push` puts an entry at the front, so push the oldest first.
    for kind in saved.iter().rev() {
        recent.push(*kind);
    }
    recent
}

/// Adds `kind` just right of `source` and connects `source` to the first input of `kind`.
pub fn add_suggested(graph: &mut Graph, source: NodeId, kind: NodeKind) -> Result<NodeId, String> {
    let at = graph
        .node(source)
        .map(|n| [n.position[0] + 280., n.position[1] + 40.])
        .unwrap_or([50., 50.]);
    let id = graph.add(kind, at)?;
    if let Some(input) = kind.spec().inputs.first() {
        graph.connect(source, id, input.key)?;
    }
    Ok(id)
}

/// The pointer position when it is inside `rect`.
pub fn pointer_in(ctx: &egui::Context, rect: Option<Rect>) -> Option<Pos2> {
    let pos = ctx.input(|i| i.pointer.hover_pos())?;
    rect.filter(|r| r.contains(pos)).map(|_| pos)
}

/// Tab open on the shelf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Favorites,
    Category(&'static str),
}

/// What the shelf asks the app to do this frame.
pub enum ShelfAction {
    /// Click on a button. The app connects it to the selected node when the types match.
    Add(NodeKind),
    /// Click on a suggestion: added after the source node and connected to it.
    Suggest(NodeId, NodeKind),
    /// Released over the canvas at this screen position.
    Place(NodeKind, Pos2),
}

/// Shelf state: the open tab, favourites, a button being dragged, and Space-tap state.
pub struct Shelf {
    tab: Tab,
    favorites: Vec<NodeKind>,
    drag: Option<NodeKind>,
    /// Space went down over the canvas and no pointer press has happened since.
    space_armed: bool,
}

impl Shelf {
    pub fn new(favorites: Vec<NodeKind>) -> Self {
        let tab = if favorites.is_empty() {
            categories()
                .first()
                .copied()
                .map_or(Tab::Favorites, Tab::Category)
        } else {
            Tab::Favorites
        };
        Self {
            tab,
            favorites,
            drag: None,
            space_armed: false,
        }
    }

    pub fn favorites(&self) -> &[NodeKind] {
        &self.favorites
    }

    pub fn is_favorite(&self, kind: NodeKind) -> bool {
        self.favorites.contains(&kind)
    }

    pub fn toggle(&mut self, kind: NodeKind) {
        toggle_favorite(&mut self.favorites, kind);
    }

    /// True on the frame Space is released when it was pressed over the canvas and the
    /// pointer did not press in between. Space + drag still pans the canvas.
    pub fn space_opens_search(&mut self, ctx: &egui::Context, canvas: Option<Rect>) -> bool {
        let over_canvas = pointer_in(ctx, canvas).is_some();
        let (pressed, released, pointer_pressed) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.key_released(egui::Key::Space),
                i.pointer.any_pressed(),
            )
        });
        if pressed && over_canvas && !ctx.wants_keyboard_input() {
            self.space_armed = true;
        }
        if pointer_pressed {
            self.space_armed = false;
        }
        if released {
            return std::mem::take(&mut self.space_armed);
        }
        false
    }

    /// Draws the strip and returns what the user asked for this frame.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        recent: &[NodeKind],
        selected: Option<(NodeId, DataType)>,
        canvas: Option<Rect>,
    ) -> Option<ShelfAction> {
        let mut action: Option<ShelfAction> = None;
        let mut toggled: Option<NodeKind> = None;
        let tabs: Vec<Tab> = std::iter::once(Tab::Favorites)
            .chain(categories().into_iter().map(Tab::Category))
            .collect();
        egui::TopBottomPanel::top("shelf")
            .frame(
                egui::Frame::new()
                    .fill(theme::toolbar_bg())
                    .inner_margin(egui::Margin::symmetric(14, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for tab in tabs {
                        let text = match tab {
                            Tab::Favorites => "★ お気に入り".to_owned(),
                            Tab::Category(name) => name.to_owned(),
                        };
                        let label = RichText::new(text).size(12.);
                        if ui.selectable_label(self.tab == tab, label).clicked() {
                            self.tab = tab;
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(theme::small("右クリックで★ ・ ドラッグで配置"));
                    });
                });
                ui.add_space(6.);
                let kinds = match self.tab {
                    Tab::Favorites => self.favorites.clone(),
                    Tab::Category(name) => kinds_in(name),
                };
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - SUGGESTION_WIDTH).max(160.);
                    egui::ScrollArea::horizontal()
                        .max_width(width)
                        .show(ui, |ui| {
                            if kinds.is_empty() {
                                ui.label(theme::small("右クリックで★を付けると、ここに並びます"));
                            }
                            self.button_row(ui, &kinds, None, &mut action, &mut toggled);
                        });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(RichText::new("おすすめ次のノード").size(11.5).strong());
                        match selected {
                            None => {
                                ui.label(theme::small(
                                    "ノードを1つ選ぶと、つなげられる候補を出します",
                                ));
                            }
                            Some((source, output)) => {
                                ui.label(theme::small(format!(
                                    "出力「{}」につなげられるノード",
                                    output.label()
                                )));
                                let picks =
                                    suggestions(output, &self.favorites, recent, SUGGESTION_LIMIT);
                                if picks.is_empty() {
                                    ui.label(theme::small(
                                        "この出力につなげられるノードはまだありません",
                                    ));
                                } else {
                                    egui::ScrollArea::horizontal().show(ui, |ui| {
                                        self.button_row(
                                            ui,
                                            &picks,
                                            Some(source),
                                            &mut action,
                                            &mut toggled,
                                        );
                                    });
                                }
                            }
                        }
                    });
                });
            });

        // A button dragged onto the canvas follows the pointer; releasing it places the node.
        if let Some(kind) = self.drag {
            let pointer = ctx.input(|i| i.pointer.latest_pos());
            if ctx.input(|i| i.pointer.primary_down()) {
                if let Some(pos) = pointer {
                    let painter = ctx.layer_painter(egui::LayerId::new(
                        egui::Order::Tooltip,
                        egui::Id::new("shelf_ghost"),
                    ));
                    let category = kind.spec().category;
                    node_icons::paint(
                        &painter,
                        Rect::from_center_size(pos, egui::vec2(40., 40.)),
                        kind,
                        category,
                        theme::category_color(category),
                    );
                }
            } else {
                self.drag = None;
                if action.is_none()
                    && let (Some(pos), Some(rect)) = (pointer, canvas)
                    && rect.contains(pos)
                {
                    action = Some(ShelfAction::Place(kind, pos));
                }
            }
        }
        if let Some(kind) = toggled {
            self.toggle(kind);
        }
        action
    }

    /// One row of icon buttons. `from` is the source node for suggestions.
    fn button_row(
        &mut self,
        ui: &mut egui::Ui,
        kinds: &[NodeKind],
        from: Option<NodeId>,
        action: &mut Option<ShelfAction>,
        toggled: &mut Option<NodeKind>,
    ) {
        ui.horizontal(|ui| {
            for &kind in kinds {
                let response = icon_button(ui, kind, self.is_favorite(kind), toggled);
                if response.drag_started() {
                    self.drag = Some(kind);
                }
                if response.clicked() && action.is_none() {
                    *action = Some(match from {
                        Some(source) => ShelfAction::Suggest(source, kind),
                        None => ShelfAction::Add(kind),
                    });
                }
            }
        });
    }
}

/// One icon button: the icon, the Japanese name under it, a ★ mark when it is a favourite.
/// Hover shows the description and the inputs → output. The right-click menu toggles ★.
fn icon_button(
    ui: &mut egui::Ui,
    kind: NodeKind,
    favorite: bool,
    toggled: &mut Option<NodeKind>,
) -> egui::Response {
    let spec = kind.spec();
    let color = theme::category_color(spec.category);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(BUTTON_W, BUTTON_H), Sense::click_and_drag());
    let fill = if response.hovered() {
        theme::teal().gamma_multiply(0.2)
    } else {
        theme::card_bg()
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 8, fill);
    let icon = Rect::from_center_size(
        rect.center_top() + egui::vec2(0., 17.),
        egui::vec2(ICON, ICON),
    );
    node_icons::paint(painter, icon, kind, spec.category, color);
    let name = painter.layout(
        spec.label.to_owned(),
        FontId::proportional(10.5),
        theme::ink(),
        BUTTON_W - 6.,
    );
    painter.galley(rect.left_top() + egui::vec2(3., 32.), name, theme::ink());
    if favorite {
        painter.text(
            rect.right_top() + egui::vec2(-9., 9.),
            Align2::CENTER_CENTER,
            "★",
            FontId::proportional(10.),
            theme::teal(),
        );
    }
    let response = response.on_hover_text(format!("{}\n{}", spec.description, describe_io(kind)));
    response.context_menu(|ui| {
        let text = if favorite {
            "★ お気に入りから外す"
        } else {
            "★ お気に入りに追加"
        };
        if ui.button(text).clicked() {
            *toggled = Some(kind);
            ui.close();
        }
    });
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn grouping_covers_every_kind_once_under_its_own_category() {
        let groups = group_by_category(NodeKind::all());
        let total: usize = groups.iter().map(|(_, list)| list.len()).sum();
        assert_eq!(total, NodeKind::all().len());
        for kind in NodeKind::all() {
            let homes: Vec<&str> = groups
                .iter()
                .filter(|(_, list)| list.contains(kind))
                .map(|(name, _)| *name)
                .collect();
            assert_eq!(homes, vec![kind.spec().category]);
        }
    }

    #[test]
    fn grouping_is_stable_and_follows_first_appearance() {
        let first = group_by_category(NodeKind::all());
        assert_eq!(first, group_by_category(NodeKind::all()));
        assert_eq!(
            first.first().map(|(name, _)| *name),
            NodeKind::all().first().map(|k| k.spec().category)
        );
        let names = categories();
        let unique: BTreeSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), names.len());
    }

    #[test]
    fn suggestions_only_offer_nodes_whose_first_input_takes_the_type() {
        let numbers = suggestions(DataType::Number, &[], &[], usize::MAX);
        assert!(!numbers.is_empty());
        assert!(
            numbers
                .iter()
                .all(|k| first_input_accepts(*k, DataType::Number))
        );
        assert!(!numbers.contains(&NodeKind::Output));

        let shapes = suggestions(DataType::Geometry, &[], &[], usize::MAX);
        assert!(shapes.contains(&NodeKind::Output));
        assert!(!shapes.contains(&NodeKind::Add));
    }

    #[test]
    fn suggestions_put_favourites_first_then_recent_and_respect_the_limit() {
        let favorites = [NodeKind::Subtract];
        let recent = [NodeKind::Multiply, NodeKind::Subtract];
        let picks = suggestions(DataType::Number, &favorites, &recent, SUGGESTION_LIMIT);
        assert_eq!(picks.len(), SUGGESTION_LIMIT);
        assert_eq!(picks[0], NodeKind::Subtract);
        assert_eq!(picks[1], NodeKind::Multiply);
        let unique: BTreeSet<&NodeKind> = picks.iter().collect();
        assert_eq!(unique.len(), picks.len());
    }

    #[test]
    fn favourites_toggle_on_and_off() {
        let mut list = Vec::new();
        toggle_favorite(&mut list, NodeKind::Box);
        assert_eq!(list, vec![NodeKind::Box]);
        toggle_favorite(&mut list, NodeKind::Box);
        assert!(list.is_empty());
    }

    #[test]
    fn favourites_and_recent_survive_a_text_round_trip() {
        let all = NodeKind::all().to_vec();
        assert_eq!(parse_kinds(&format_kinds(&all)), all);
        assert_eq!(
            parse_kinds("box, nope,box,,scale_xyz"),
            vec![NodeKind::Box, NodeKind::ScaleXyz]
        );
        assert_eq!(
            format_kinds(&[NodeKind::LinearArray, NodeKind::Box]),
            "linear_array,box"
        );
    }

    #[test]
    fn recent_list_restores_newest_first() {
        let recent = recent_from_saved(&[NodeKind::Box, NodeKind::Twist]);
        assert_eq!(
            recent.kinds().to_vec(),
            vec![NodeKind::Box, NodeKind::Twist]
        );
    }

    #[test]
    fn hover_text_lists_inputs_then_output() {
        assert_eq!(describe_io(NodeKind::Add), "入力: A・B → 出力: 数値");
    }
}
