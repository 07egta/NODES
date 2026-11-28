use nodes::sketch_nodes::{parse, run, session::SketchSession};
use solidcraft_geom::Vec2;
use solidcraft_sketch::ConstraintKind;

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

fn length_id(session: &SketchSession) -> String {
    session
        .sketch()
        .constraints
        .iter()
        .find(|c| matches!(c.kind, ConstraintKind::Length { .. }))
        .expect("length dimension")
        .id
        .clone()
}

#[test]
fn fully_constrained_rectangle_and_dimension_edit() {
    let mut session = SketchSession::from_text(
        "rect r 0 0 12 8\nfix r.p0\nlength r.b 12\nlength r.l 8",
    )
    .unwrap();
    let first = session.solve();
    assert_eq!(first.dof(), Some(0));
    let id = length_id(&session);
    assert_eq!(session.source_line(&id), Some(3));
    let attempt = session.edit_dimensions(&[(&id, 20.0)]).unwrap();
    assert_eq!(attempt.dof(), Some(0));
    let sk = session.sketch();
    let p = sk.point(sk.resolve_point("r.p2").unwrap()).unwrap();
    near(p.x, 20.0);
    near(p.y, 8.0);
}

#[test]
fn atomic_dimension_failure_and_expression_error() {
    let mut session = SketchSession::from_text(
        "point b 10 0\nline l origin b\nhorizontal l\nlength l 10\ndx origin b 10",
    )
    .unwrap();
    assert!(session.solve().committed);
    let id = length_id(&session);
    let before = session.sketch().clone();
    let failed = session.edit_dimensions(&[(&id, 20.0)]).unwrap();
    assert!(!failed.committed);
    assert_eq!(failed.dof(), None);
    assert_eq!(session.sketch(), &before);
    assert!(session.edit_dimensions(&[(&id, f64::NAN)]).is_err());
    assert!(session.edit_dimensions(&[(&id, 10.0), (&id, 12.0)]).is_err());
    assert!(session.edit_expressions(&[(&id, "1/0")], [0.0; 4]).is_err());
    assert_eq!(session.sketch(), &before);
    assert!(session.is_solved());
}

#[test]
fn expressions_and_feasible_drag() {
    let mut session =
        SketchSession::from_text("point b 10 0\nline l origin b\nlength l 10").unwrap();
    assert_eq!(session.solve().dof(), Some(1));
    let id = length_id(&session);
    assert!(
        session
            .edit_expressions(&[(&id, "x*2")], [6.0, 0.0, 0.0, 0.0])
            .unwrap()
            .committed
    );
    let initial = session.sketch().point(1).unwrap();
    near(initial.dist(Vec2::new(0.0, 0.0)), 12.0);
    let target = Vec2::new(initial.x, initial.y + 4.0);
    assert!(session.drag_point("b", target, 1.0).unwrap());
    let moved = session.sketch().point(1).unwrap();
    assert!(moved.dist(target) < initial.dist(target));
    near(moved.dist(Vec2::new(0.0, 0.0)), 12.0);
    assert!(!session.drag_point("origin", target, 1.0).unwrap());
}

#[test]
fn initial_conflict_reports_source_without_deleting_constraint() {
    let text = "point p 3 4\nfix p\ndistance origin p 10";
    let mut session = SketchSession::from_text(text).unwrap();
    let before = session.sketch().clone();
    let attempt = session.solve();
    assert!(!attempt.committed);
    assert_eq!(attempt.dof(), None);
    assert_eq!(session.sketch(), &before);
    assert!(session.failure_message(&attempt).contains("3 行目"));
    assert!(run(text, 1, &|| false).unwrap_err().contains("3 行目"));
    let id = &before.constraints[1].id;
    near(session.diagnostic(id).unwrap().residual.unwrap(), 5.0);
}

#[test]
fn additional_geometry_and_constraints() {
    let text = "point a 0 0\npoint b 10 0\npoint c 10 5\npoint d 0 5\n                line axis a b\nconstruction axis\nblock axis\nellipse e a b 3\n                bezier bez a d c b\nbspline spl 2 a d c b\n                horizontal_points a b\nvertical_points b c\npoint_line_distance c axis 5";
    let sk = parse(text).unwrap();
    assert_eq!(sk.curves.len(), 4);
    assert!(sk.curves[0].construction && sk.curves[0].fixed);
    let out = run(text, 1, &|| false).unwrap();
    assert_eq!(out.branches[&vec![0]].len(), 3);
    assert!(parse("point a 0 0\npoint b 1 0\nellipse e a b 2").is_err());
    assert!(parse("point a 0 0\npoint b 1 0\nbspline s 2 a b").is_err());
}

#[test]
fn pre_cancelled_node_does_not_start_solving() {
    let result = run("not_a_command x", 1, &|| true);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), nodes::kernel_bridge::STOPPED);
}
