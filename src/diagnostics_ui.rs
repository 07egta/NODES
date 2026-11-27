//! Read-only diagnostics from the worker's latest graph/result snapshot.
//!
//! Rendering does not evaluate nodes or rebuild the dependency graph. Navigation
//! uses current node IDs and labels; stale snapshots cannot change the selection.
use crate::{theme, ui_polish};
use eframe::egui::{self, RichText};
use nodes::{
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
    query: String,
    /// Root, blocked, disabled, unevaluated and hotspot pages are independent.
    pages: [usize; 5],
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
            self.pages = [0; 5];
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
                ui.horizontal(|ui| {
                    // Allocate the actual button and spacing first. TextEdit's
                    // desired width excludes its margins: guessing a reservation
                    // could overflow the row and grow this window every frame.
                    let (changed, cleared) = ui
                        .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let cleared =
                                !self.query.is_empty() && ui.small_button("クリア").clicked();
                            if cleared {
                                self.query.clear();
                            }
                            let changed = ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.query)
                                        .id(egui::Id::new("diagnostics_query"))
                                        .hint_text("名前で絞り込む / #123 で ID")
                                        .char_limit(160)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed();
                            (changed, cleared)
                        })
                        .inner;
                    if changed || cleared {
                        self.pages = [0; 5];
                    }
                });
                let filter = NodeFilter::new(&self.query);
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("diagnostics_scroll")
                    .max_height(ui.available_height().max(140.))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.timings {
                            selected = show_timings(
                                ui,
                                report,
                                &nodes,
                                stale,
                                &mut self.show_cached,
                                &filter,
                                &mut self.pages[4],
                            );
                        } else {
                            selected =
                                show_errors(ui, report, &nodes, stale, &filter, &mut self.pages);
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

enum NodeFilter {
    All,
    Id(Option<NodeId>),
    Name(String),
}

impl NodeFilter {
    fn new(query: &str) -> Self {
        let query = ui_polish::normalize(query.trim());
        if query.is_empty() {
            Self::All
        } else if let Some(id) = query.strip_prefix('#') {
            Self::Id(id.trim().parse().ok())
        } else {
            Self::Name(query)
        }
    }

    fn matches(&self, node: &NodeRef, nodes: &IndexedNodes<'_>) -> bool {
        match self {
            Self::All => true,
            Self::Id(id) => *id == Some(node.id),
            Self::Name(query) => {
                let name = nodes
                    .node(node.id)
                    .map_or(node.name.as_str(), |node| node.name.as_str());
                ui_polish::normalize(name).contains(query)
            }
        }
    }
}

fn matching_indices<T>(
    items: &[T],
    node: impl Fn(&T) -> &NodeRef,
    nodes: &IndexedNodes<'_>,
    filter: &NodeFilter,
) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| filter.matches(node(item), nodes).then_some(index))
        .collect()
}

fn page_range(count: usize, page: &mut usize) -> std::ops::Range<usize> {
    *page = (*page).min(count.saturating_sub(1) / MAX_VISIBLE_ROWS);
    let start = *page * MAX_VISIBLE_ROWS;
    start..(start + MAX_VISIBLE_ROWS).min(count)
}

fn page_controls(
    ui: &mut egui::Ui,
    id: &'static str,
    page: &mut usize,
    matched: usize,
    total: usize,
) -> std::ops::Range<usize> {
    let range = page_range(matched, page);
    ui.push_id(id, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::small(format!("該当 {matched} 件 / 全 {total} 件")));
            if matched > MAX_VISIBLE_ROWS {
                if ui
                    .add_enabled(*page > 0, egui::Button::new("前の100件").small())
                    .clicked()
                {
                    *page -= 1;
                }
                if ui
                    .add_enabled(range.end < matched, egui::Button::new("次の100件").small())
                    .clicked()
                {
                    *page += 1;
                }
                let current = page_range(matched, page);
                ui.label(theme::small(format!(
                    "{}–{} 件目",
                    current.start + 1,
                    current.end
                )));
            }
        });
    });
    page_range(matched, page)
}

fn show_timings(
    ui: &mut egui::Ui,
    report: &DiagnosticReport,
    nodes: &IndexedNodes<'_>,
    stale: bool,
    show_cached: &mut bool,
    filter: &NodeFilter,
    page: &mut usize,
) -> Option<NodeId> {
    ui.label(RichText::new("時間のかかったノード").strong());
    ui.label(theme::small(
        "入力準備・計算・出力検査の実測時間です。部品ノードは内部の処理を含みます。",
    ));
    ui.label(theme::small(
        "割合は非表示のノードも含む、測定済み有効ノードの時間合計に対する値です。CPU 使用率ではありません。",
    ));
    if ui
        .checkbox(show_cached, "キャッシュから再利用したノードも表示")
        .changed()
    {
        *page = 0;
    }
    ui.add_space(5.);
    let mut selected = None;
    let matching: Vec<_> = report
        .hotspots
        .iter()
        .enumerate()
        .filter_map(|(index, hotspot)| {
            ((*show_cached || !hotspot.cached) && filter.matches(&hotspot.node, nodes))
                .then_some(index)
        })
        .collect();
    let range = page_controls(
        ui,
        "hotspot_pages",
        page,
        matching.len(),
        report.hotspots.len(),
    );
    for &index in &matching[range] {
        let hotspot = &report.hotspots[index];
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
    if matching.is_empty() {
        ui.label(theme::small(if report.hotspots.is_empty() {
            "表示できるノードの計測結果はありません。"
        } else if !*show_cached && report.hotspots.iter().all(|hotspot| hotspot.cached) {
            "すべて再利用されました。チェックを付けると取得時間を確認できます。"
        } else {
            "絞り込みに一致するノードはありません。"
        }));
    }
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
    filter: &NodeFilter,
    pages: &mut [usize; 5],
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
        let matching = matching_indices(
            &report.root_failures,
            |failure| &failure.node,
            nodes,
            filter,
        );
        let range = page_controls(
            ui,
            "root_pages",
            &mut pages[0],
            matching.len(),
            report.root_failures.len(),
        );
        for &index in &matching[range] {
            let failure = &report.root_failures[index];
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
                        if failure.message_truncated {
                            ui.label(theme::small("長いメッセージの一部を省略しています。"));
                        }
                    });
                ui.add_space(5.);
            });
        }
        if matching.is_empty() {
            ui.label(theme::small("絞り込みに一致する原因候補はありません。"));
        }
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
        let matching = matching_indices(
            &report.blocked_nodes,
            |failure| &failure.node,
            nodes,
            filter,
        );
        egui::CollapsingHeader::new(format!(
            "上流にエラーがあるノード · 該当 {} / 全 {} 件",
            matching.len(),
            report.blocked_nodes.len()
        ))
        .id_salt("diagnostics_blocked")
        .show(ui, |ui| {
            let range = page_controls(
                ui,
                "blocked_pages",
                &mut pages[1],
                matching.len(),
                report.blocked_nodes.len(),
            );
            for &index in &matching[range] {
                let failure = &report.blocked_nodes[index];
                if let Some(id) = blocked_failure(ui, report, failure, nodes, stale) {
                    selected = Some(id);
                }
            }
            if matching.is_empty() {
                ui.label(theme::small("絞り込みに一致するノードはありません。"));
            }
        });
    }

    for (label, group, page) in [
        ("無効なノード", &report.disabled_nodes, 2),
        ("未計算のノード", &report.unevaluated_nodes, 3),
    ] {
        if !group.is_empty() {
            let matching = matching_indices(group, |node| node, nodes, filter);
            egui::CollapsingHeader::new(format!(
                "{label} · 該当 {} / 全 {} 件",
                matching.len(),
                group.len()
            ))
            .id_salt(("diagnostics_state", label))
            .show(ui, |ui| {
                let range = page_controls(ui, label, &mut pages[page], matching.len(), group.len());
                for &index in &matching[range] {
                    let node = &group[index];
                    if node_link(ui, nodes, node, stale) {
                        selected = Some(node.id);
                    }
                }
                if matching.is_empty() {
                    ui.label(theme::small("絞り込みに一致するノードはありません。"));
                }
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
        if failure.message_truncated {
            ui.label(theme::small("長いメッセージの一部を省略しています。"));
        }
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
                "ほか {} 件の上流候補があります。",
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

#[cfg(test)]
mod tests {
    use super::*;
    use nodes::{diagnostics::diagnose, engine::Evaluator, model::NodeKind};

    #[test]
    fn nonempty_filter_does_not_expand_the_window_each_frame() {
        let ctx = egui::Context::default();
        theme::configure(&ctx, true);
        let mut graph = Graph::default();
        let id = graph.add(NodeKind::Divide, [0.; 2]).unwrap();
        graph.node_mut(id).unwrap().params.insert("b".into(), 0.);
        let evaluation = Evaluator::default().evaluate(&graph, &|| false);
        let report = diagnose(&graph, &evaluation);
        let mut window = DiagnosticsWindow {
            open: true,
            query: format!("#{id}"),
            ..Default::default()
        };
        let mut widths = Vec::new();
        for frame in 0..60 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1364., 920.),
                )),
                time: Some(frame as f64 / 60.),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                window.show(ctx, Some(&report), &graph, false, 1);
            });
            widths.push(ctx.memory(|memory| {
                memory
                    .area_rect(egui::Id::new("graph_diagnostics"))
                    .unwrap()
                    .width()
            }));
        }
        assert!(
            widths
                .iter()
                .all(|width| width.is_finite() && *width <= 650.)
        );
        let settled = &widths[10..];
        let min = settled.iter().copied().fold(f32::INFINITY, f32::min);
        let max = settled.iter().copied().fold(0., f32::max);
        assert!(max - min <= 1., "window kept growing: {widths:?}");
    }

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

    #[test]
    fn the_101st_root_is_reachable_by_page_and_exact_id_filter() {
        let mut graph = Graph::default();
        for _ in 0..101 {
            let id = graph.add(NodeKind::Divide, [0.; 2]).unwrap();
            graph.node_mut(id).unwrap().params.insert("b".into(), 0.);
        }
        let evaluation = Evaluator::default().evaluate(&graph, &|| false);
        let report = diagnose(&graph, &evaluation);
        assert_eq!(report.root_failures.len(), 101);
        let indices = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id, index))
            .collect();
        let nodes = IndexedNodes {
            graph: &graph,
            indices: &indices,
        };
        let matching = matching_indices(
            &report.root_failures,
            |failure| &failure.node,
            &nodes,
            &NodeFilter::All,
        );
        let mut page = 1;
        let range = page_range(matching.len(), &mut page);
        assert_eq!(range, 100..101);
        assert_eq!(report.root_failures[matching[range.start]].node.id, 100);
        let filtered = matching_indices(
            &report.root_failures,
            |failure| &failure.node,
            &nodes,
            &NodeFilter::new("＃１００"),
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(report.root_failures[filtered[0]].node.id, 100);
        assert_eq!(page_range(filtered.len(), &mut page), 0..1);
        assert_eq!(page, 0);
        assert_eq!(page_range(0, &mut page), 0..0);
    }

    #[test]
    fn name_filter_uses_live_renames_without_new_diagnosis() {
        let mut graph = Graph::default();
        let id = graph.add(NodeKind::Divide, [0.; 2]).unwrap();
        graph.nodes[0].params.insert("b".into(), 0.);
        graph.nodes[0].name = "Before Rename".into();
        let evaluation = Evaluator::default().evaluate(&graph, &|| false);
        let report = diagnose(&graph, &evaluation);
        let indices = BTreeMap::from([(id, 0)]);
        graph.nodes[0].name = "After カナ".into();
        let nodes = IndexedNodes {
            graph: &graph,
            indices: &indices,
        };
        let find = |query: &str| {
            matching_indices(
                &report.root_failures,
                |failure| &failure.node,
                &nodes,
                &NodeFilter::new(query),
            )
        };
        assert!(find("Before").is_empty());
        assert_eq!(find("AFTER かな"), vec![0]);
        assert!(find("#invalid").is_empty());
    }
}
