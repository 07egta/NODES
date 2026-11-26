//! Read-only diagnostics from the worker's latest graph/result snapshot.
//!
//! Rendering does not evaluate nodes or rebuild the dependency graph. Navigation
//! uses current node IDs and labels; stale snapshots cannot change the selection.
use crate::{theme, ui_polish};
use eframe::egui::{self, RichText};
use morphlet::{
    diagnostics::{DiagnosticReport, NodeFailure, NodeRef},
    model::{Graph, Node, NodeId},
};
use std::collections::BTreeMap;

const MAX_VISIBLE_ROWS: usize = 100;

#[derive(Default)]
pub struct DiagnosticsWindow {
    pub open: bool,
    timings: bool,
    show_cached: bool,
    node_index: BTreeMap<NodeId, usize>,
    index_revision: Option<u64>,
}

impl DiagnosticsWindow {
    /// Return a node to select and frame on the canvas, without editing the graph.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        report: Option<&DiagnosticReport>,
        graph: &Graph,
        stale: bool,
        revision: u64,
    ) -> Option<NodeId> {
        if !self.open {
            return None;
        }
        if self.index_revision != Some(revision) {
            self.node_index = graph
                .nodes
                .iter()
                .enumerate()
                .map(|(index, node)| (node.id, index))
                .collect();
            self.index_revision = Some(revision);
        }
        let nodes = IndexedNodes {
            graph,
            indices: &self.node_index,
        };
        let mut open = self.open;
        let mut selected = None;
        egui::Window::new("診断と処理時間")
            .id(egui::Id::new("graph_diagnostics"))
            .open(&mut open)
            .default_width(590.)
            .default_height(460.)
            .min_width(300.)
            .resizable(true)
            .show(ctx, |ui| {
                if stale {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("計算中です。更新後の診断をお待ちください。");
                    });
                    if report.is_some() {
                        ui.label(theme::small(
                            "以下は前回の結果です。ノードへの移動は更新後にできます。",
                        ));
                    }
                    ui.add_space(6.);
                }
                let Some(report) = report else {
                    ui.label(theme::small(
                        "計算が終わると、入力の問題とノードごとの処理時間が表示されます。",
                    ));
                    return;
                };
                let summary = &report.summary;
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(status_text(report)).strong().color(
                        if summary.failed_count > 0 || report.graph_error.is_some() {
                            theme::error()
                        } else {
                            theme::ink()
                        },
                    ));
                    ui.label(theme::small(format!(
                        "{} ノード · {:.3} ms",
                        summary.node_count, summary.elapsed_ms
                    )));
                });
                ui.label(theme::small(format!(
                    "計算 {} / 再利用 {} / 無効 {} / 未計算 {}",
                    summary.computed_count,
                    summary.cached_count,
                    summary.disabled_count,
                    summary.unevaluated_count,
                )));
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.timings, false, "入力とエラー");
                    ui.selectable_value(&mut self.timings, true, "処理時間");
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("diagnostics_scroll")
                    .max_height(ui.available_height().max(140.))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.timings {
                            selected =
                                show_timings(ui, report, &nodes, stale, &mut self.show_cached);
                        } else {
                            selected = show_errors(ui, report, &nodes, stale);
                        }
                    });
            });
        self.open = open;
        selected
    }
}

/// IDs remain stable through rename/position edits, which do not queue geometry.
/// Rebuild the index only on graph revisions, then read current labels directly.
struct IndexedNodes<'a> {
    graph: &'a Graph,
    indices: &'a BTreeMap<NodeId, usize>,
}

impl IndexedNodes<'_> {
    fn node(&self, id: NodeId) -> Option<&Node> {
        self.indices
            .get(&id)
            .and_then(|index| self.graph.nodes.get(*index))
            .filter(|node| node.id == id)
    }
}

fn show_timings(
    ui: &mut egui::Ui,
    report: &DiagnosticReport,
    nodes: &IndexedNodes<'_>,
    stale: bool,
    show_cached: &mut bool,
) -> Option<NodeId> {
    ui.label(RichText::new("時間のかかったノード").strong());
    ui.label(theme::small(
        "入力準備・計算・出力検査の実測時間です。部品ノードは内部の処理を含みます。",
    ));
    ui.label(theme::small(
        "割合は測定できた有効ノードの時間合計に対する値です。CPU 使用率ではありません。",
    ));
    ui.checkbox(show_cached, "キャッシュから再利用したノードも表示");
    ui.add_space(5.);
    let mut selected = None;
    let mut matching = 0;
    for hotspot in report
        .hotspots
        .iter()
        .filter(|hotspot| *show_cached || !hotspot.cached)
    {
        matching += 1;
        if matching > MAX_VISIBLE_ROWS {
            continue;
        }
        ui.push_id(("hotspot", hotspot.node.id), |ui| {
            ui.horizontal_wrapped(|ui| {
                if node_link(ui, nodes, &hotspot.node, stale) {
                    selected = Some(hotspot.node.id);
                }
                ui.label(RichText::new(format!("{:.3} ms", hotspot.elapsed_ms)).monospace());
                ui.label(theme::small(format!("{:.1}%", hotspot.share_percent)));
                ui.label(theme::small(if !hotspot.completed {
                    "中断まで"
                } else if hotspot.cached {
                    "再利用"
                } else {
                    "計算"
                }));
            });
        });
    }
    if matching == 0 {
        ui.label(theme::small(if report.hotspots.is_empty() {
            "表示できるノードの計測結果はありません。"
        } else {
            "すべて再利用されました。チェックを付けると取得時間を確認できます。"
        }));
    }
    hidden_rows(ui, matching);
    selected
}

fn status_text(report: &DiagnosticReport) -> &'static str {
    if report.graph_error.is_some() {
        "グラフの接続・構造を確認してください"
    } else if report.summary.cancelled {
        "計算が中断されています"
    } else if !report.summary.complete {
        "まだ計算されていないノードがあります"
    } else if report.summary.failed_count > 0 {
        "入力とエラーの原因候補を確認してください"
    } else {
        "計算エラーはありません"
    }
}

fn show_errors(
    ui: &mut egui::Ui,
    report: &DiagnosticReport,
    nodes: &IndexedNodes<'_>,
    stale: bool,
) -> Option<NodeId> {
    let mut selected = None;
    if let Some(error) = &report.graph_error {
        ui.colored_label(theme::error(), error);
        ui.label(theme::small(
            "グラフ全体を検証できないため、個別ノードへの原因の割り当ては行っていません。",
        ));
    } else if !report.root_failures.is_empty() {
        ui.label(
            RichText::new(format!(
                "最初に確認する原因候補 · {} 件",
                report.root_failures.len()
            ))
            .strong(),
        );
        ui.label(theme::small(
            "接続関係から上流の候補を絞っています。下流ノードにも別の入力問題がある場合があります。",
        ));
        for failure in report.root_failures.iter().take(MAX_VISIBLE_ROWS) {
            ui.push_id(("root_error", failure.node.id), |ui| {
                egui::Frame::new()
                    .fill(theme::error_fill())
                    .corner_radius(6)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if node_link(ui, nodes, &failure.node, stale) {
                                selected = Some(failure.node.id);
                            }
                            ui.label(theme::small(format!(
                                "下流のエラー {} ノード",
                                failure.affected_count
                            )));
                        });
                        ui.label(RichText::new(&failure.message).color(theme::error()));
                    });
                ui.add_space(5.);
            });
        }
        hidden_rows(ui, report.root_failures.len());
    } else if report.summary.failed_count == 0
        && report.summary.complete
        && !report.summary.cancelled
    {
        ui.label("接続されたグラフの計算が完了しています。");
        ui.label(theme::small(
            "形状の正確さや 3D プリント適性は、形状・メッシュの確認機能で確認してください。",
        ));
    }

    if !report.blocked_nodes.is_empty() {
        ui.add_space(8.);
        egui::CollapsingHeader::new(format!(
            "上流にエラーがあるノード · {} 件",
            report.blocked_nodes.len()
        ))
        .id_salt("diagnostics_blocked")
        .show(ui, |ui| {
            for failure in report.blocked_nodes.iter().take(MAX_VISIBLE_ROWS) {
                if let Some(id) = blocked_failure(ui, report, failure, nodes, stale) {
                    selected = Some(id);
                }
            }
            hidden_rows(ui, report.blocked_nodes.len());
        });
    }

    for (label, group) in [
        ("無効なノード", &report.disabled_nodes),
        ("未計算のノード", &report.unevaluated_nodes),
    ] {
        if !group.is_empty() {
            egui::CollapsingHeader::new(format!("{label} · {} 件", group.len()))
                .id_salt(("diagnostics_state", label))
                .show(ui, |ui| {
                    for node in group.iter().take(MAX_VISIBLE_ROWS) {
                        if node_link(ui, nodes, node, stale) {
                            selected = Some(node.id);
                        }
                    }
                    hidden_rows(ui, group.len());
                });
        }
    }
    selected
}

fn blocked_failure(
    ui: &mut egui::Ui,
    report: &DiagnosticReport,
    failure: &NodeFailure,
    nodes: &IndexedNodes<'_>,
    stale: bool,
) -> Option<NodeId> {
    let mut selected = None;
    ui.push_id(("blocked_error", failure.node.id), |ui| {
        if node_link(ui, nodes, &failure.node, stale) {
            selected = Some(failure.node.id);
        }
        ui.label(theme::small(&failure.message));
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::small("上流の原因候補:"));
            for id in failure.upstream_roots.iter().take(8) {
                if let Ok(index) = report
                    .root_failures
                    .binary_search_by_key(id, |root| root.node.id)
                    && node_link(ui, nodes, &report.root_failures[index].node, stale)
                {
                    selected = Some(*id);
                }
            }
        });
        if failure.upstream_root_count > failure.upstream_roots.len().min(8) {
            ui.label(theme::small(format!(
                "ほか {} 件の上流候補があります。候補一覧で確認できます。",
                failure.upstream_root_count - failure.upstream_roots.len().min(8)
            )));
        }
        ui.add_space(6.);
    });
    selected
}

fn node_link(ui: &mut egui::Ui, nodes: &IndexedNodes<'_>, node: &NodeRef, stale: bool) -> bool {
    // Name-only edits deliberately do not rerun geometry, so read the live label.
    let current = nodes.node(node.id);
    let name = current.map_or(node.name.as_str(), |node| node.name.as_str());
    let short = ui_polish::one_line(name, 32);
    ui.add_enabled(
        !stale && current.is_some(),
        egui::Button::new(format!("#{:02} {short}  ↗", node.id)).small(),
    )
    .on_hover_text(format!(
        "{name} · {}\nキャンバスで選択して表示",
        node.kind.spec().label
    ))
    .clicked()
}

fn hidden_rows(ui: &mut egui::Ui, count: usize) {
    if count > MAX_VISIBLE_ROWS {
        ui.label(theme::small(format!(
            "先頭 {MAX_VISIBLE_ROWS} 件を表示（ほか {} 件）",
            count - MAX_VISIBLE_ROWS
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morphlet::{diagnostics::diagnose, engine::Evaluator, model::NodeKind};

    #[test]
    fn incomplete_or_cancelled_results_never_display_success() {
        let mut graph = Graph::default();
        graph.add(NodeKind::Number, [0., 0.]).unwrap();
        let mut evaluation = Evaluator::default().evaluate(&graph, &|| false);
        assert_eq!(
            status_text(&diagnose(&graph, &evaluation)),
            "計算エラーはありません"
        );
        evaluation.outputs.clear();
        assert_eq!(
            status_text(&diagnose(&graph, &evaluation)),
            "まだ計算されていないノードがあります"
        );
        evaluation.cancelled = true;
        assert_eq!(
            status_text(&diagnose(&graph, &evaluation)),
            "計算が中断されています"
        );
    }

    #[test]
    fn cached_node_lookup_keeps_live_names_and_rejects_reordered_indices() {
        let mut graph = Graph::default();
        let first = graph.add(NodeKind::Number, [0., 0.]).unwrap();
        graph.add(NodeKind::Number, [200., 0.]).unwrap();
        let indices = BTreeMap::from([(first, 0)]);
        graph.nodes[0].name = "変更後の名前".into();
        assert_eq!(
            IndexedNodes {
                graph: &graph,
                indices: &indices
            }
            .node(first)
            .unwrap()
            .name,
            "変更後の名前"
        );
        graph.nodes.swap(0, 1);
        assert!(
            IndexedNodes {
                graph: &graph,
                indices: &indices
            }
            .node(first)
            .is_none()
        );
    }
}
