//! Exercise numeric entry through the real canvas, including ports and graph state.
use super::*;
use morphlet::{
    engine::Evaluator,
    model::{Graph, Value},
    persistence::{self, History},
};

struct CanvasTest {
    context: egui::Context,
    canvas: GraphCanvas,
    project: Project,
}

impl CanvasTest {
    fn new(graph: Graph, zoom: f32) -> Self {
        let mut project = Project::new("canvas numeric entry", graph);
        project.pan = [0., 0.];
        project.zoom = zoom;
        Self {
            context: egui::Context::default(),
            canvas: GraphCanvas::default(),
            project,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 800.))),
            events,
            ..Default::default()
        };
        self.context.run(input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                self.canvas.show(ui, &mut self.project, None, false);
            });
        })
    }

    fn click(&mut self, position: Pos2) {
        for pressed in [true, false] {
            self.frame(vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
    }

    fn focus_number(&mut self, node: NodeId, input: usize) -> Id {
        self.frame(vec![]);
        let position = input_pos(
            self.project.graph.node(node).unwrap(),
            input,
            self.canvas.last_rect.unwrap(),
            self.project.pan,
            self.project.zoom,
        ) + egui::vec2(170., 0.) * self.project.zoom;
        self.click(position);
        self.frame(vec![]);
        assert!(
            self.context.wants_keyboard_input(),
            "clicking the canvas number must start direct entry"
        );
        self.context.memory(|memory| memory.focused()).unwrap()
    }

    fn confirm(&mut self) {
        for pressed in [true, false] {
            self.frame(vec![key(egui::Key::Enter, pressed, egui::Modifiers::NONE)]);
        }
        self.frame(vec![]);
        assert!(!self.context.wants_keyboard_input());
    }

    fn enter(&mut self, text: &str) {
        let select_all = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        self.frame(vec![
            key(egui::Key::A, true, select_all),
            key(egui::Key::A, false, select_all),
            egui::Event::Text(text.into()),
        ]);
        self.confirm();
    }
}

fn key(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    }
}

fn scalar(evaluation: &Evaluation, node: NodeId) -> f64 {
    let values = &evaluation.outputs[&node].as_ref().unwrap().branches[&vec![0]];
    let [Value::Number(value)] = values.as_slice() else {
        panic!("expected one number");
    };
    *value
}

#[test]
fn canvas_number_entry_recalculates_signed_zero_and_supports_undo_redo() {
    let mut graph = Graph::default();
    let number = graph.add(NodeKind::Number, [40., 40.]).unwrap();
    graph
        .node_mut(number)
        .unwrap()
        .params
        .insert("value".into(), 0.);
    let angle = graph.add(NodeKind::Atan2, [380., 40.]).unwrap();
    graph
        .node_mut(angle)
        .unwrap()
        .params
        .insert("x".into(), -1.);
    graph.connect(number, angle, "y").unwrap();
    let mut test = CanvasTest::new(graph, 1.);
    let before = test.project.clone();
    let mut evaluator = Evaluator::default();
    assert_eq!(
        scalar(&evaluator.evaluate(&test.project.graph, &|| false), angle),
        std::f64::consts::PI
    );

    test.focus_number(number, 0);
    test.enter("-0");
    assert_eq!(
        test.project.graph.node(number).unwrap().params["value"].to_bits(),
        (-0.0_f64).to_bits()
    );
    // This is the bit-sensitive predicate also used by the application's edit tracker.
    assert!(!persistence::same_parameter_bits(
        &before.graph,
        &test.project.graph
    ));
    let evaluated = evaluator.evaluate(&test.project.graph, &|| false);
    assert_eq!(evaluated.computed, 2);
    assert_eq!(scalar(&evaluated, angle), -std::f64::consts::PI);

    let mut history = History::default();
    history.checkpoint_owned(before);
    assert!(history.undo(&mut test.project));
    test.frame(vec![]);
    assert_eq!(
        scalar(&evaluator.evaluate(&test.project.graph, &|| false), angle),
        std::f64::consts::PI
    );
    assert!(history.redo(&mut test.project));
    test.frame(vec![]);
    assert_eq!(
        scalar(&evaluator.evaluate(&test.project.graph, &|| false), angle),
        -std::f64::consts::PI
    );
    let saved = serde_json::to_string(&test.project).unwrap();
    let loaded: Project = serde_json::from_str(&saved).unwrap();
    assert!(persistence::same_parameter_bits(
        &test.project.graph,
        &loaded.graph
    ));
}

#[test]
fn canvas_precise_numbers_preserve_bits_and_leave_ports_accessible_at_each_zoom() {
    for zoom in [0.7, 1., 1.8] {
        let mut graph = Graph::default();
        let sphere = graph.add(NodeKind::Sphere, [40., 40.]).unwrap();
        let original = 30.123_456_789_012_344_f64;
        graph
            .node_mut(sphere)
            .unwrap()
            .params
            .insert("radius".into(), original);
        let mut test = CanvasTest::new(graph, zoom);
        let focused = test.focus_number(sphere, 0);
        let output = test.frame(vec![]);
        let node = test.project.graph.node(sphere).unwrap();
        let port = output_pos(node, test.canvas.last_rect.unwrap(), test.project.pan, zoom);
        let safe_right = port.x - 12. * zoom;
        let response = test.context.read_response(focused).unwrap();
        assert!(
            response.interact_rect.right() <= safe_right,
            "the editor must not intercept the output port"
        );
        let mut numeric_texts = 0;
        for clipped in output.shapes {
            if let Shape::Text(text) = clipped.shape
                && text.galley.job.text == original.to_string()
            {
                numeric_texts += 1;
                let visible = text.visual_bounding_rect().intersect(clipped.clip_rect);
                assert!(
                    visible.right() <= safe_right,
                    "precise text must stay inside the card"
                );
            }
        }
        assert!(
            numeric_texts > 0,
            "the editor must retain the complete precision string"
        );
        test.confirm();
        assert_eq!(
            test.project.graph.node(sphere).unwrap().params["radius"].to_bits(),
            original.to_bits()
        );
        test.click(port);
        assert_eq!(test.canvas.connecting, Some(sphere));
        assert_eq!(
            test.project.graph.node(sphere).unwrap().params["radius"].to_bits(),
            original.to_bits()
        );
    }
}

#[test]
fn canvas_numeric_focus_follows_node_and_port_when_order_and_slider_assignment_change() {
    let mut graph = Graph::default();
    let helix = graph.add(NodeKind::Helix, [40., 40.]).unwrap();
    let source = graph.add(NodeKind::Number, [400., 40.]).unwrap();
    let mut test = CanvasTest::new(graph, 1.);
    let focused = test.focus_number(helix, 3);
    // Connecting an earlier numeric port promotes this fourth field into the
    // three slider rows. Reordering nodes must not retarget its text buffer.
    test.project.graph.nodes.reverse();
    test.project.graph.connect(source, helix, "radius").unwrap();
    test.frame(vec![]);
    assert_eq!(
        test.context.memory(|memory| memory.focused()),
        Some(focused)
    );
    assert!(test.context.wants_keyboard_input());
    test.enter("96");
    assert_eq!(
        test.project.graph.node(helix).unwrap().params["segments"],
        96.
    );
    assert_eq!(
        test.project.graph.node(helix).unwrap().params["radius"],
        25.
    );
    assert_eq!(test.project.graph.node(source).unwrap().params["value"], 1.);
}
