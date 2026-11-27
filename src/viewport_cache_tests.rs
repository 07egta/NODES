//! Render the actual application panel and viewport across evaluation revisions.
use super::*;
use morphlet::model::{Mesh, Value};

struct PreviewTest {
    context: egui::Context,
    app: MorphletApp,
    evaluator: Evaluator,
    results: LatestSender<ComputeResult>,
    _requests: LatestReceiver<ComputeRequest>,
    number: NodeId,
    subdivide: NodeId,
    output: NodeId,
}

impl PreviewTest {
    fn new(segments: usize) -> Self {
        let mut graph = Graph::default();
        let sphere = graph.add(NodeKind::Sphere, [0., 0.]).unwrap();
        graph
            .node_mut(sphere)
            .unwrap()
            .params
            .insert("segments".into(), segments as f64);
        graph
            .node_mut(sphere)
            .unwrap()
            .params
            .insert("rings".into(), (segments / 2) as f64);
        let subdivide = graph.add(NodeKind::MeshSubdivide, [280., 0.]).unwrap();
        graph.connect(sphere, subdivide, "mesh").unwrap();
        let output = graph.add(NodeKind::Output, [560., 0.]).unwrap();
        graph.connect(subdivide, output, "geometry").unwrap();
        let number = graph.add(NodeKind::Number, [0., 250.]).unwrap();
        let mut evaluator = Evaluator::default();
        let evaluation = evaluator.evaluate(&graph, &|| false);
        assert!(evaluation.outputs.values().all(Result::is_ok));

        let context = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(context.clone());
        // A screenshot argument only suppresses recovery-file loading. We call
        // right_panel, never App::update, so no screenshots or autosaves run.
        let mut app = MorphletApp::new(&cc, None, Some(PathBuf::from("unused-preview-test.png")));
        let (sender, requests) = latest_channel::channel();
        let (results, receiver) = latest_channel::channel();
        // Feed actual completion mailboxes deterministically, without sleeps or
        // a competing evaluator thread during the viewport measurements.
        app.worker = Worker {
            sender,
            receiver,
            latest: Arc::new(AtomicU64::new(0)),
        };
        app.project = Project::new("preview cache", graph);
        app.evaluation = Some(evaluation);
        app.revision = 0;
        app.result_revision = 0;
        app.pending_compute = false;
        app.canvas.selected.clear();
        app.preview_mode = 0;
        let result = Self {
            context,
            app,
            evaluator,
            results,
            _requests: requests,
            number,
            subdivide,
            output,
        };
        // Sphere has 2*S*(R-1) triangles; midpoint subdivision makes four per face.
        assert_eq!(
            result.mesh().triangles.len(),
            8 * segments * (segments / 2 - 1)
        );
        result
    }

    fn mesh(&self) -> Arc<Mesh> {
        mesh_from(self.app.evaluation.as_ref().unwrap(), self.output)
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 800.),
                )),
                events,
                ..Default::default()
            },
            |context| self.app.right_panel(context),
        )
    }

    fn begin_unrelated_edit(&mut self) {
        *self
            .app
            .project
            .graph
            .node_mut(self.number)
            .unwrap()
            .params
            .get_mut("value")
            .unwrap() += 1.;
        self.app.queue_compute();
        // The request has been accepted by a worker, but no result has arrived.
        self.app.pending_compute = false;
        assert!(self.app.stale());
    }

    fn evaluate(&mut self) -> Evaluation {
        self.evaluator.evaluate(&self.app.project.graph, &|| false)
    }

    fn complete(&mut self, evaluation: Evaluation) {
        self.results
            .send(ComputeResult {
                revision: self.app.revision,
                evaluation,
            })
            .unwrap_or_else(|_| panic!("completion mailbox closed"));
        self.app.compute_tick(&self.context);
        assert!(!self.app.stale());
    }
}

fn mesh_from(evaluation: &Evaluation, node: NodeId) -> Arc<Mesh> {
    let tree = evaluation.outputs[&node].as_ref().unwrap();
    let Some(Value::Mesh(mesh)) = tree.branches.values().flatten().next() else {
        panic!("expected a mesh output");
    };
    Arc::clone(mesh)
}

#[test]
fn pending_unrelated_edit_retains_the_actual_viewport_scene_and_diagnostics() {
    let mut test = PreviewTest::new(24);
    test.frame(vec![]);
    let before = test.app.viewport.test_cache_state();
    let original = test.mesh();
    assert!(before.has_bounds && before.triangles > 2_000);
    assert_eq!(before.work.completed_diagnostics, 1);

    test.begin_unrelated_edit();
    let completed = test.evaluate();
    assert_eq!(completed.computed, 1);
    assert!(Arc::ptr_eq(&original, &mesh_from(&completed, test.output)));
    assert!(
        test.app.result_trees().is_err(),
        "pending data must not become exportable"
    );
    // The F/menu fit path must preserve the same scene as normal rendering.
    test.app.fit_preview();
    assert_eq!(test.app.viewport.test_cache_state().work, before.work);
    let pending_output = test.frame(vec![]);
    let pending = test.app.viewport.test_cache_state();
    assert_eq!(
        pending.triangles, before.triangles,
        "pending is not a completed empty result"
    );
    assert_eq!(pending.work, before.work);
    assert!(pending_output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains("前回"))));
    test.complete(completed);
    test.frame(vec![]);
    assert_eq!(test.app.viewport.test_cache_state().work, before.work);
    assert!(Arc::ptr_eq(&original, &test.mesh()));
    assert!(test.app.result_trees().is_ok());
}

#[test]
fn completed_empty_and_error_clear_previous_geometry_even_after_hidden_completion() {
    for error in [false, true] {
        for hidden in [false, true] {
            let mut test = PreviewTest::new(16);
            test.frame(vec![]);
            if error {
                test.app.project.graph.disconnect(test.subdivide, "mesh");
            } else {
                test.app.project.graph.node_mut(test.output).unwrap().off = true;
            }
            test.begin_unrelated_edit();
            let completed = test.evaluate();
            if error {
                assert!(completed.outputs[&test.output].is_err());
            } else {
                assert_eq!(
                    completed.outputs[&test.output]
                        .as_ref()
                        .unwrap()
                        .item_count(),
                    0
                );
            }
            test.complete(completed);
            if hidden {
                // No frame was drawn for this completed empty/error evaluation.
                // Reshowing while the next edit is pending must not resurrect it.
                test.begin_unrelated_edit();
            }
            test.frame(vec![]);
            let scene = test.app.viewport.test_cache_state();
            assert_eq!(scene.triangles, 0, "error={error}, hidden={hidden}");
            assert!(!scene.has_bounds);
            test.app.fit_preview();
            test.frame(vec![]);
            assert_eq!(test.app.viewport.test_cache_state().work, scene.work);
        }
    }
    // Completion is installed even when a temporarily tiny viewport cannot draw;
    // otherwise the caller's revision marker could certify an older scene.
    let mut test = PreviewTest::new(16);
    test.frame(vec![]);
    let _ = test.context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1., 1.),
            )),
            ..Default::default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                test.app.viewport.show(ui, &[]);
            });
        },
    );
    assert!(!test.app.viewport.test_cache_state().has_bounds);
}

#[test]
fn pending_camera_controls_work_and_stale_worker_results_are_not_published() {
    let mut test = PreviewTest::new(16);
    test.frame(vec![]);
    let before = test.app.viewport.test_cache_state();
    let original = test.mesh();
    test.begin_unrelated_edit();
    test.results
        .send(ComputeResult {
            revision: test.app.revision.wrapping_sub(1),
            evaluation: Evaluation::default(),
        })
        .unwrap_or_else(|_| panic!("completion mailbox closed"));
    test.app.compute_tick(&test.context);
    assert!(test.app.stale());
    assert!(Arc::ptr_eq(&original, &test.mesh()));
    test.frame(vec![
        egui::Event::PointerMoved(egui::pos2(800., 200.)),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0., 40.),
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    let pending = test.app.viewport.test_cache_state();
    assert_ne!(
        pending.camera, before.camera,
        "pending must not freeze camera controls"
    );
    assert_eq!(pending.triangles, before.triangles);
    assert_eq!(pending.selected_triangles, before.selected_triangles);
    assert_eq!(pending.work, before.work);
}

#[test]
fn pending_pick_cannot_select_a_reused_node_id_after_undo() {
    let mut test = PreviewTest::new(16);
    let before_add = test.app.project.clone();
    let original = test
        .app
        .project
        .graph
        .add(NodeKind::Sphere, [700., 0.])
        .unwrap();
    test.app.preview_mode = 1;
    test.app.canvas.select_only(original);
    test.app.history.checkpoint_owned(before_add);
    test.app.queue_compute();
    test.app.pending_compute = false;
    let evaluation = test.evaluate();
    test.complete(evaluation);
    test.frame(vec![]);

    // Positive control: a completed current preview still selects live nodes.
    test.app.preview_mode = 2;
    test.app.canvas.selected.clear();
    test.frame(vec![]);
    let current_point = test.app.viewport.test_pick_point().unwrap();
    for pressed in [true, false] {
        test.frame(vec![
            egui::Event::PointerMoved(current_point),
            egui::Event::PointerButton {
                pos: current_point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert!(!test.app.canvas.selected.is_empty());
    assert!(
        test.app
            .canvas
            .selected
            .iter()
            .all(|id| test.app.project.graph.node(*id).is_some())
    );
    test.app.preview_mode = 1;
    test.app.canvas.select_only(original);
    test.frame(vec![]);
    let previous_triangles = test.app.viewport.test_cache_state().triangles;
    assert!(previous_triangles > 0);

    test.app.undo();
    let replacement = test
        .app
        .project
        .graph
        .add(NodeKind::Number, [700., 0.])
        .unwrap();
    assert_eq!(
        replacement, original,
        "Undo restores next_id as part of the graph"
    );
    test.app.queue_compute();
    test.app.pending_compute = false;
    test.frame(vec![]);
    assert!(test.app.stale());
    assert!(test.app.canvas.selected.is_empty());
    assert_eq!(
        test.app.viewport.test_cache_state().triangles,
        previous_triangles
    );
    // Advance only the synthetic input clock, so this is a single click rather
    // than a double-click after the positive control. No wall-clock wait is needed.
    let later = test.context.input(|input| input.time) + 1.;
    let _ = test.context.run(
        egui::RawInput {
            time: Some(later),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 800.),
            )),
            ..Default::default()
        },
        |context| test.app.right_panel(context),
    );
    let point = test.app.viewport.test_pick_point().unwrap();
    for pressed in [true, false] {
        test.frame(vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert!(
        test.app.canvas.selected.is_empty(),
        "the old Sphere must not select the new Number with its reused ID"
    );
}

/// cargo test --locked --bin Morphlet benchmark_pending_preview -- --ignored --nocapture --test-threads=1
#[test]
#[ignore = "manual measurement of actual app/viewport pending and completion frames"]
fn benchmark_pending_preview() {
    for segments in [24, 192] {
        let mut test = PreviewTest::new(segments);
        test.frame(vec![]);
        test.frame(vec![]); // Fonts, layout and the first projection are warm.
        let mesh = test.mesh();
        let before = test.app.viewport.test_cache_state().work;
        let mut pending_ms = Vec::new();
        let mut completed_ms = Vec::new();
        let mut retained = Vec::new();
        for _ in 0..7 {
            test.begin_unrelated_edit();
            let result = test.evaluate();
            assert_eq!(result.computed, 1);
            assert!(Arc::ptr_eq(&mesh, &mesh_from(&result, test.output)));
            let start = Instant::now();
            test.frame(vec![]);
            pending_ms.push(start.elapsed().as_secs_f64() * 1e3);
            retained.push(test.app.viewport.test_cache_state().triangles == mesh.triangles.len());
            test.complete(result);
            let start = Instant::now();
            test.frame(vec![]);
            completed_ms.push(start.elapsed().as_secs_f64() * 1e3);
        }
        let mut p = pending_ms.clone();
        let mut c = completed_ms.clone();
        p.sort_by(f64::total_cmp);
        c.sort_by(f64::total_cmp);
        println!(
            "{}",
            serde_json::json!({
                "scenario": "sphere_midpoint_subdivision_and_unconnected_number",
                "segments": segments,
                "vertices": mesh.vertices.len(),
                "triangles": mesh.triangles.len(),
                "cycles": 7,
                "screen_points": [1000, 800],
                "mode": "completed_outputs",
                "accurate_depth": test.app.viewport.accurate_depth,
                "mesh_arc_identity_verified": true,
                "work_before": before,
                "work_after": test.app.viewport.test_cache_state().work,
                "retained_pending_scene": retained,
                "pending_ms": pending_ms,
                "completed_ms": completed_ms,
                "pending_median_ms": p[3],
                "completed_median_ms": c[3],
                "timed": "egui Context::run -> actual MorphletApp::right_panel -> Viewport::show, excluding evaluation and mailbox delivery"
            })
        );
    }
}
