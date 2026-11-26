//! egui pieces for the script node editor and for cluster parts (kept out of app.rs).
use crate::theme;
use eframe::egui::{self, RichText};
use morphlet::{
    cluster,
    model::{Graph, NodeId},
    script,
};

pub enum PartAction {
    Add(u64),
    Export(u64),
    Import,
}

const HELP: &str = "入力 a〜d は数値、g はつないだ形状です（複数あるときはリスト）。最後に書いた値が出力になります。\n\
数学: sin cos tan sqrt abs pow min max floor ceil round radians pi() clamp lerp linspace(始め,終わり,個数)\n\
点: point(x,y,z)   メッシュ: mesh_box(幅,奥行,高さ)  mesh_cylinder(半径,高さ,分割)  mesh_move(m,x,y,z)  mesh_rotate(m,度)  mesh_scale(m,倍率)  mesh_merge(m1,m2) または mesh_merge(リスト)\n\
出力できるのは 数値・数値のリスト・点・メッシュ です。ファイルやネットは使えず、実行は 20万回・2秒までです。";

/// Multi-line editor for a Script node. Edits are applied with the button or when the
/// field loses focus, so each keystroke does not create an undo step or a recompute.
pub fn script_editor(ui: &mut egui::Ui, graph: &mut Graph, id: NodeId) {
    let Some(node) = graph.node_mut(id) else {
        return;
    };
    ui.add_space(6.);
    ui.label(RichText::new("スクリプト").strong());
    ui.label(theme::small(HELP));
    let key = egui::Id::new(("script_buffer", id));
    let (mut buffer, mut base): (String, String) = ui
        .data_mut(|d| d.get_temp::<(String, String)>(key))
        .unwrap_or_else(|| (node.script.clone(), node.script.clone()));
    if base != node.script {
        // Changed from outside (undo/redo, example button): drop the unsaved buffer.
        buffer = node.script.clone();
        base = node.script.clone();
    }
    let response = ui.add(
        egui::TextEdit::multiline(&mut buffer)
            .code_editor()
            .desired_rows(12)
            .desired_width(f32::INFINITY)
            .hint_text("例: a * 2"),
    );
    while buffer.len() > script::MAX_SCRIPT_BYTES {
        buffer.pop();
    }
    let mut apply = response.lost_focus();
    ui.horizontal(|ui| {
        if ui
            .button("適用")
            .on_hover_text("スクリプトを実行します")
            .clicked()
        {
            apply = true;
        }
        if ui.button("例1: らせん階段").clicked() {
            buffer = script::EXAMPLE_STAIRS.to_string();
            apply = true;
        }
        if ui.button("例2: 波の数列").clicked() {
            buffer = script::EXAMPLE_WAVE.to_string();
            apply = true;
        }
    });
    ui.label(theme::small(format!(
        "{} / {} バイト{}",
        buffer.len(),
        script::MAX_SCRIPT_BYTES,
        if buffer != base {
            "（未適用）"
        } else {
            ""
        }
    )));
    if apply && buffer != node.script {
        node.script = buffer.clone();
        base = buffer.clone();
    }
    ui.data_mut(|d| d.insert_temp(key, (buffer, base)));
}

/// One-line formula editor for the 式 node. Applied on Enter, focus loss or the button.
pub fn expression_editor(ui: &mut egui::Ui, graph: &mut Graph, id: NodeId) {
    let Some(node) = graph.node_mut(id) else {
        return;
    };
    ui.add_space(6.);
    ui.label(RichText::new("式").strong());
    ui.label(theme::small(
        "x・y・z・w は左の入力です。例: sin(x) * 10 + y、(x - 5) ^ 2、max(x, y)。\n\
         関数: sin cos tan sqrt abs min max floor ceil round ln exp、定数: pi e。角度はラジアンです。",
    ));
    let key = egui::Id::new(("expression_buffer", id));
    let (mut buffer, mut base): (String, String) = ui
        .data_mut(|d| d.get_temp::<(String, String)>(key))
        .unwrap_or_else(|| (node.script.clone(), node.script.clone()));
    if base != node.script {
        buffer = node.script.clone();
        base = node.script.clone();
    }
    let mut apply = false;
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut buffer)
                .code_editor()
                .desired_width(220.)
                .char_limit(morphlet::expr::MAX_EXPR_LEN)
                .hint_text(morphlet::expr::DEFAULT_EXPRESSION),
        );
        apply = response.lost_focus();
        if ui.button("適用").clicked() {
            apply = true;
        }
    });
    if let Err(e) = morphlet::expr::parse(&buffer) {
        ui.label(RichText::new(e).size(11.).color(theme::ERROR));
    }
    if apply && buffer != node.script {
        node.script = buffer.clone();
        base = buffer.clone();
    }
    ui.data_mut(|d| d.insert_temp(key, (buffer, base)));
}

/// Cluster node details. Returns an action for the app to carry out.
pub fn cluster_panel(ui: &mut egui::Ui, graph: &mut Graph, id: NodeId) -> Option<PartAction> {
    let def_id = graph.node(id)?.cluster?;
    let def = graph.clusters.iter_mut().find(|d| d.id == def_id)?;
    ui.add_space(6.);
    ui.label(RichText::new("部品（まとめノード）").strong());
    ui.horizontal(|ui| {
        ui.label("部品名");
        ui.text_edit_singleline(&mut def.name);
    });
    if def.name.len() > 200 {
        def.name = def.name.chars().take(60).collect();
    }
    ui.label(theme::small(format!(
        "内部ノード {} 個 / 公開する入力 {} 個（最大 {}）",
        def.graph.nodes.len(),
        def.inputs.len(),
        cluster::MAX_CLUSTER_INPUTS
    )));
    let mut action = None;
    if ui
        .button("部品として書き出す（.morphpart）")
        .on_hover_text("ほかの作品でも読み込んで使えます")
        .clicked()
    {
        action = Some(PartAction::Export(def_id));
    }
    action
}

/// Library section listing the work's parts. Returns what the user asked for.
pub fn library_parts(ui: &mut egui::Ui, graph: &Graph) -> Option<PartAction> {
    let mut action = None;
    ui.add_space(8.);
    ui.label(theme::small("部品（まとめノード）"));
    if graph.clusters.is_empty() {
        ui.label(theme::small("ノードを複数選び「まとめる」で作れます。"));
    }
    for def in &graph.clusters {
        ui.horizontal(|ui| {
            if ui
                .button(format!("＋ {}", def.name))
                .on_hover_text("この部品を置く")
                .clicked()
            {
                action = Some(PartAction::Add(def.id));
            }
            if ui.small_button("書き出し").clicked() {
                action = Some(PartAction::Export(def.id));
            }
        });
    }
    if ui.small_button("部品を読み込む…").clicked() {
        action = Some(PartAction::Import);
    }
    action
}
