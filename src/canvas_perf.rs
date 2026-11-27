use super::{GraphCanvas, NodeId, NodeKind, Project};
use eframe::egui;
use morphlet::{
    engine::Evaluation,
    model::{Graph, Tree},
};
use std::{hint::black_box, time::Instant};

fn canvas_frame(
    ctx: &egui::Context,
    canvas: &mut GraphCanvas,
    project: &mut Project,
    evaluation: Option<&Evaluation>,
) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280., 800.),
        )),
        ..Default::default()
    };
    ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            canvas.show(ui, project, evaluation, false);
        });
    })
}

fn grid(count: usize, columns: usize) -> Graph {
    let mut graph = Graph::default();
    graph.add(NodeKind::Number, [0., 0.]).unwrap();
    graph.add(NodeKind::Add, [0., 0.]).unwrap();
    let number = graph.nodes[0].clone();
    let add = graph.nodes[1].clone();
    graph.nodes = (0..count)
        .map(|i| {
            let mut node = if i % columns == 0 {
                number.clone()
            } else {
                let mut node = add.clone();
                node.inputs.insert("a".into(), (i - 1) as NodeId);
                node
            };
            node.id = i as NodeId;
            node.position = [(i % columns) as f32 * 300., (i / columns) as f32 * 165.];
            node
        })
        .collect();
    graph.next_id = count as NodeId;
    graph
}

fn long_empty_wires() -> (Graph, Evaluation) {
    let mut graph = Graph::default();
    let mut evaluation = Evaluation::default();
    for row in 0..12 {
        let y = row as f32 * 58.;
        let source = graph.add(NodeKind::Number, [-50_000., y]).unwrap();
        let destination = graph.add(NodeKind::Add, [50_000., y]).unwrap();
        graph.connect(source, destination, "a").unwrap();
        evaluation.outputs.insert(source, Ok(Tree::default()));
    }
    (graph, evaluation)
}

/// Run with cargo test --release --locked --bin Morphlet benchmark_canvas_pan -- --ignored --nocapture
#[test]
#[ignore = "manual release benchmark of egui canvas drawing and tessellation"]
fn benchmark_canvas_pan() {
    let (long_graph, evaluation) = long_empty_wires();
    let scenarios = [
        ("small-grid", grid(60, 12), None),
        ("large-grid", grid(12_000, 120), None),
        ("long-empty-wires", long_graph, Some(&evaluation)),
    ];
    for (name, graph, evaluation) in scenarios {
        let ctx = egui::Context::default();
        let mut canvas = GraphCanvas::default();
        let mut project = Project::new(name, graph);
        project.zoom = 1.;
        let mut elapsed = Vec::new();
        let mut shape_count = 0;
        for frame in 0..90 {
            project.pan = [26. - (frame % 30) as f32 * 29., 27.];
            let started = Instant::now();
            let output = canvas_frame(&ctx, &mut canvas, &mut project, evaluation);
            let shapes = output.shapes.len();
            black_box(ctx.tessellate(output.shapes, output.pixels_per_point));
            if frame >= 10 {
                elapsed.push(started.elapsed().as_secs_f64() * 1_000.);
                shape_count += shapes;
            }
        }
        elapsed.sort_unstable_by(f64::total_cmp);
        let median = elapsed[elapsed.len() / 2];
        let p95 = elapsed[(elapsed.len() * 95).div_ceil(100) - 1];
        println!(
            "{name} nodes={} frames={} p50_ms={median:.3} p95_ms={p95:.3} shapes_per_frame={:.1}",
            project.graph.nodes.len(),
            elapsed.len(),
            shape_count as f64 / elapsed.len() as f64
        );
    }
}
