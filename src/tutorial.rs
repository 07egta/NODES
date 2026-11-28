//! Learn-by-doing first-run guide: add a node, connect it, change a number.
//! Progress is detected from the graph itself, so nothing needs to be clicked to continue.

use crate::{theme, ui_polish};
use eframe::egui::{self, RichText};
use nodes::model::{Graph, NodeId};
use std::collections::BTreeSet;

/// Settings-file key that remembers the guide was finished or skipped.
const SETTING_KEY: &str = "tutorial";

pub const STEPS: [(&str, &str); 3] = [
    (
        "ノードを 1 つ足そう",
        "何もない所をダブルクリック、または Tab キーで検索が開きます。「円」「花瓶」「らせん」など日本語で探して選びます。",
    ),
    (
        "線でつなごう",
        "ノードの右の丸から、別のノードの左の丸へドラッグします。つながると右側の形が変わります。",
    ),
    (
        "数字を変えてみよう",
        "ノードを選び、右の設定でスライダーを動かすか、数値を入力します。まちがえても Ctrl+Z で戻せます。",
    ),
];

/// What the guide can see: node count, exact wire identities, and a fingerprint of numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stats {
    pub nodes: usize,
    pub wires: BTreeSet<(NodeId, String, NodeId)>,
    pub values: u64,
}

impl Stats {
    pub fn of(graph: &Graph) -> Self {
        let mut values: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |x: u64| {
            values = (values ^ x).wrapping_mul(0x0100_0000_01b3);
        };
        let mut wires = BTreeSet::new();
        for node in &graph.nodes {
            wires.extend(
                node.inputs
                    .iter()
                    .map(|(port, &source)| (node.id, port.clone(), source)),
            );
            for (key, value) in &node.params {
                for byte in key.bytes() {
                    mix(u64::from(byte));
                }
                mix(value.to_bits());
            }
        }
        Self {
            nodes: graph.nodes.len(),
            wires,
            values,
        }
    }
}

pub enum Action {
    Examples,
}

#[derive(Default)]
pub struct Tutorial {
    active: bool,
    step: usize,
    base: Option<Stats>,
}

impl Tutorial {
    /// Active on the first launch only (and never when a file or screenshot was requested).
    pub fn first_run(allowed: bool) -> Self {
        let done = ui_polish::read_setting(SETTING_KEY).is_some_and(|v| v == "done");
        Self {
            active: allowed && !done,
            step: 0,
            base: None,
        }
    }

    pub fn start(&mut self) {
        self.active = true;
        self.step = 0;
        self.base = None;
    }

    /// Moves to the next step when the graph shows the current one was done.
    /// Returns true when the step changed.
    pub fn observe(&mut self, now: Stats) -> bool {
        if !self.active || self.step >= STEPS.len() {
            return false;
        }
        let Some(base) = self.base.as_ref() else {
            self.base = Some(now);
            return false;
        };
        let done = match self.step {
            0 => now.nodes > base.nodes,
            // Replacing an occupied input creates a new wire without increasing the count.
            // Disconnecting an existing wire alone is not a successful connection.
            1 => !now.wires.is_subset(&base.wires),
            _ => now.values != base.values,
        };
        if done {
            self.step += 1;
            self.base = Some(now);
        }
        done
    }

    fn finish(&mut self) {
        self.active = false;
        // A failed write only means the guide may show again next time.
        let _ = ui_polish::save_setting_line(&format!("{SETTING_KEY}=done"));
    }

    pub fn show(&mut self, ctx: &egui::Context, graph: &Graph) -> Option<Action> {
        if !self.active {
            return None;
        }
        // Do not scan or allocate wire identities after the guide has been dismissed.
        self.observe(Stats::of(graph));
        let mut action = None;
        let mut close = false;
        let finished = self.step >= STEPS.len();
        egui::Area::new(egui::Id::new("tutorial_card"))
            .anchor(egui::Align2::LEFT_BOTTOM, [16., -48.])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::panel_bg())
                    .stroke(egui::Stroke::new(1_f32, theme::card_border()))
                    .corner_radius(10)
                    .inner_margin(14)
                    .show(ui, |ui| {
                        ui.set_max_width(300.);
                        ui.horizontal(|ui| {
                            for index in 0..STEPS.len() {
                                let (rect, _) = ui
                                    .allocate_exact_size(egui::vec2(14., 14.), egui::Sense::hover());
                                let color = if index < self.step {
                                    theme::teal()
                                } else if index == self.step {
                                    theme::ink()
                                } else {
                                    theme::line()
                                };
                                if index < self.step {
                                    ui.painter().circle_filled(rect.center(), 5., color);
                                } else {
                                    ui.painter().circle_stroke(
                                        rect.center(),
                                        5.,
                                        egui::Stroke::new(1.5_f32, color),
                                    );
                                }
                            }
                            ui.label(theme::small(format!(
                                "はじめの 3 ステップ · {}/3",
                                self.step.min(STEPS.len())
                            )));
                        });
                        ui.add_space(6.);
                        if finished {
                            ui.label(RichText::new("できました。").strong().color(theme::teal()));
                            ui.label("ノードを足し、つなぎ、数字を変える。これが基本のすべてです。作品例を開いて、自由に変えてみましょう。");
                            ui.add_space(8.);
                            ui.horizontal(|ui| {
                                if ui.button("作品例を見る").clicked() {
                                    action = Some(Action::Examples);
                                    close = true;
                                }
                                if ui.button("閉じる").clicked() {
                                    close = true;
                                }
                            });
                        } else {
                            let (title, body) = STEPS[self.step];
                            ui.label(RichText::new(title).strong().size(14.));
                            ui.label(RichText::new(body).size(12.));
                            ui.add_space(8.);
                            if ui.small_button("あとで（もう出さない）").clicked() {
                                close = true;
                            }
                        }
                    });
            });
        if close {
            self.finish();
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodes::model::NodeKind;

    fn guide() -> Tutorial {
        Tutorial {
            active: true,
            step: 0,
            base: None,
        }
    }

    #[test]
    fn three_steps_advance_on_add_connect_and_change() {
        let mut graph = Graph::default();
        let a = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let mut t = guide();
        assert!(!t.observe(Stats::of(&graph)));
        let b = graph.add(NodeKind::Add, [300., 0.]).unwrap();
        assert!(t.observe(Stats::of(&graph)));
        assert_eq!(t.step, 1);
        graph.connect(a, b, "a").unwrap();
        assert!(t.observe(Stats::of(&graph)));
        assert_eq!(t.step, 2);
        graph
            .node_mut(a)
            .unwrap()
            .params
            .insert("value".into(), 42.);
        assert!(t.observe(Stats::of(&graph)));
        assert_eq!(t.step, 3);
        assert!(!t.observe(Stats::of(&graph)));
    }

    #[test]
    fn replacing_an_occupied_input_advances_connection_step() {
        let mut graph = Graph::default();
        let a = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let output = graph.add(NodeKind::Add, [300., 0.]).unwrap();
        graph.connect(a, output, "a").unwrap();
        let mut t = guide();
        assert!(!t.observe(Stats::of(&graph)));
        let b = graph.add(NodeKind::Number, [0., 100.]).unwrap();
        assert!(t.observe(Stats::of(&graph)));
        assert_eq!(t.step, 1);
        let before = Stats::of(&graph);
        graph.connect(b, output, "a").unwrap();
        let after = Stats::of(&graph);
        assert_eq!(before.wires.len(), after.wires.len());
        assert!(t.observe(after));
        assert_eq!(t.step, 2);
    }

    #[test]
    fn disconnecting_or_repeating_an_existing_wire_does_not_advance() {
        let mut graph = Graph::default();
        let a = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let b = graph.add(NodeKind::Number, [0., 100.]).unwrap();
        let output = graph.add(NodeKind::Add, [300., 0.]).unwrap();
        graph.connect(a, output, "a").unwrap();
        graph.connect(b, output, "b").unwrap();
        let mut t = guide();
        t.step = 1;
        assert!(!t.observe(Stats::of(&graph)));
        graph.connect(a, output, "a").unwrap();
        assert!(!t.observe(Stats::of(&graph)));
        graph.disconnect(output, "b");
        assert!(!t.observe(Stats::of(&graph)));
        graph.connect(b, output, "a").unwrap();
        assert!(t.observe(Stats::of(&graph)));
        assert_eq!(t.step, 2);
    }

    #[test]
    fn moving_a_node_does_not_count_as_changing_a_number() {
        let mut graph = Graph::default();
        let a = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let mut t = guide();
        t.step = 2;
        t.observe(Stats::of(&graph));
        graph.node_mut(a).unwrap().position = [50., 50.];
        assert!(!t.observe(Stats::of(&graph)));
    }

    #[test]
    fn an_inactive_guide_never_moves() {
        let mut t = Tutorial::default();
        assert!(!t.observe(Stats {
            nodes: 1,
            wires: BTreeSet::new(),
            values: 0
        }));
    }
}
