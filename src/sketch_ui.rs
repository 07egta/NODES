//! Inspector editor for the 拘束スケッチ node: the sketch text kept in `Node::script`.
use crate::theme;
use eframe::egui::{self, RichText};
use nodes::model::{Graph, NodeId};

const EXAMPLE: &str = "# 例: 60 × 40 の板に半径 8 の穴\n長方形 r 0 0 60 40\n固定 r.p0\n長さ r.b 60\n長さ r.l 40\n円 c 30 20 8\n半径 c 8\n";

pub fn editor(ui: &mut egui::Ui, graph: &mut Graph, id: NodeId) {
    let Some(node) = graph.node_mut(id) else {
        return;
    };
    ui.add_space(6.);
    ui.label(RichText::new("スケッチ").strong());
    ui.label(theme::small(
        "1 行に 1 命令。点・線・円・弧・長方形で形を描き、水平・垂直・一致・平行・直交・接する・等しい・固定で拘束、距離・長さ・半径・直径・角度で寸法を付けます。点は l1.start / c.center のようにも書けます。",
    ));
    let key = egui::Id::new(("sketch_buffer", id));
    let (mut buffer, mut base): (String, String) = ui
        .data_mut(|d| d.get_temp::<(String, String)>(key))
        .unwrap_or_else(|| (node.script.clone(), node.script.clone()));
    if base != node.script {
        buffer = node.script.clone();
        base = node.script.clone();
    }
    let response = ui.add(
        egui::TextEdit::multiline(&mut buffer)
            .code_editor()
            .desired_rows(8)
            .desired_width(f32::INFINITY)
            .char_limit(nodes::sketch_nodes::MAX_SKETCH_TEXT),
    );
    let mut apply = response.lost_focus();
    ui.horizontal(|ui| {
        if ui.button("適用").clicked() {
            apply = true;
        }
        if buffer.trim().is_empty() && ui.button("例を入れる").clicked() {
            buffer = EXAMPLE.to_string();
            apply = true;
        }
    });
    if apply && buffer != node.script {
        node.script = buffer.clone();
        base = buffer.clone();
    }
    ui.data_mut(|d| d.insert_temp(key, (buffer, base)));
}
