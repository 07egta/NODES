//! Headless regression coverage for the actual numeric widgets used by both editors.
use super::{InputSpec, NodeKind, parameter_drag_value, parameter_slider};
use eframe::egui;

#[derive(Clone, Copy, Debug)]
enum Editor {
    Inspector,
    CanvasTrack,
    CanvasNumber,
}

fn frame(
    context: &egui::Context,
    editor: Editor,
    spec: &InputSpec,
    value: &mut f64,
    events: Vec<egui::Event>,
) -> egui::Response {
    let mut response = None;
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    let _ = context.run(input, |context| {
        egui::CentralPanel::default().show(context, |ui| {
            ui.spacing_mut().slider_width = 120.0;
            ui.spacing_mut().item_spacing.x = 8.0;
            response = Some(match editor {
                Editor::Inspector => ui.add(parameter_slider(value, spec)),
                Editor::CanvasTrack => ui.add(parameter_slider(value, spec).show_value(false)),
                Editor::CanvasNumber => ui.add(parameter_drag_value(value, spec)),
            });
        });
    });
    response.expect("the numeric widget was rendered")
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

fn focus(context: &egui::Context, editor: Editor, spec: &InputSpec, value: &mut f64) {
    let response = frame(context, editor, spec, value, Vec::new());
    let position = match editor {
        // The value button follows the 120-point track and 8-point item gap.
        Editor::Inspector => egui::pos2(response.rect.left() + 133.0, response.rect.center().y),
        Editor::CanvasNumber => response.rect.center(),
        Editor::CanvasTrack => panic!("the track has no text editor"),
    };
    assert!(response.rect.contains(position));
    for pressed in [true, false] {
        frame(
            context,
            editor,
            spec,
            value,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    frame(context, editor, spec, value, Vec::new());
    assert!(
        context.wants_keyboard_input(),
        "{editor:?} did not gain text focus"
    );
}

fn confirm(context: &egui::Context, editor: Editor, spec: &InputSpec, value: &mut f64) {
    for pressed in [true, false] {
        frame(
            context,
            editor,
            spec,
            value,
            vec![key(egui::Key::Enter, pressed, egui::Modifiers::NONE)],
        );
    }
    // Exercise focus-loss parsing and subsequent passive rendering as well.
    frame(context, editor, spec, value, Vec::new());
    assert!(
        !context.wants_keyboard_input(),
        "Enter did not end text editing"
    );
}

fn type_value(editor: Editor, spec: &InputSpec, initial: f64, text: &str) -> f64 {
    let context = egui::Context::default();
    let mut value = initial;
    focus(&context, editor, spec, &mut value);
    let select_all = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame(
        &context,
        editor,
        spec,
        &mut value,
        vec![
            key(egui::Key::A, true, select_all),
            key(egui::Key::A, false, select_all),
            egui::Event::Text(text.into()),
        ],
    );
    confirm(&context, editor, spec, &mut value);
    value
}

#[test]
fn passive_numeric_widgets_preserve_every_registered_default() {
    let context = egui::Context::default();
    let kinds = NodeKind::all()
        .iter()
        .copied()
        .chain(std::iter::once(NodeKind::Cluster));
    for kind in kinds {
        for spec in kind.spec().inputs {
            let Some(default) = spec.default else {
                continue;
            };
            for editor in [Editor::Inspector, Editor::CanvasTrack, Editor::CanvasNumber] {
                let mut value = default;
                for _ in 0..2 {
                    let response = frame(&context, editor, spec, &mut value, Vec::new());
                    assert_eq!(
                        value.to_bits(),
                        default.to_bits(),
                        "{kind:?}.{} changed without input in {editor:?}",
                        spec.key,
                    );
                    assert!(!response.changed(), "passive rendering reported an edit");
                }
            }
        }
    }
}

#[test]
fn typed_numbers_keep_precision_and_can_exceed_the_convenience_track_range() {
    let radius = &NodeKind::Sphere.spec().inputs[0];
    for editor in [Editor::Inspector, Editor::CanvasNumber] {
        for (text, expected) in [("30", 30.0_f64), ("30.123456", 30.123456), ("500", 500.0)] {
            let value = type_value(editor, radius, 25.0, text);
            assert_eq!(value.to_bits(), expected.to_bits(), "{editor:?}: {text}");
        }
    }
}

#[test]
fn numeric_edits_enforce_metadata_bounds_finite_values_and_integers() {
    let radius = &NodeKind::Sphere.spec().inputs[0];
    let segments = &NodeKind::Sphere.spec().inputs[1];
    for editor in [Editor::Inspector, Editor::CanvasNumber] {
        assert_eq!(type_value(editor, radius, 25.0, "2000"), 1000.0);
        assert_eq!(type_value(editor, radius, 25.0, "-5"), 0.1);
        assert_eq!(type_value(editor, radius, 25.0, "NaN"), 25.0);
        assert_eq!(type_value(editor, radius, 25.0, "inf"), 25.0);
        assert_eq!(type_value(editor, radius, 25.0, "Infinity"), 25.0);
        assert_eq!(type_value(editor, segments, 48.0, "49.7"), 50.0);
        assert_eq!(type_value(editor, segments, 48.0, "500"), 192.0);
    }
}

#[test]
fn focus_and_enter_without_editing_preserve_loaded_precision() {
    let radius = &NodeKind::Sphere.spec().inputs[0];
    for editor in [Editor::Inspector, Editor::CanvasNumber] {
        for original in [30.123_456_789_012_344_f64, 500.123_456_789] {
            let context = egui::Context::default();
            let mut value = original;
            focus(&context, editor, radius, &mut value);
            confirm(&context, editor, radius, &mut value);
            assert_eq!(value.to_bits(), original.to_bits(), "{editor:?}");
        }
    }
}
