//! Point-list and polyline nodes. Arc-length operations act on stored segments, not NURBS.
use super::{cancelled, checked_tree, finite_point};
use crate::model::{Curve, MAX_CURVE_DIVISIONS, MAX_ITEMS, MAX_VERTICES, Tree, Value};
use std::collections::BTreeSet;
use std::sync::Arc;

type Point = [f64; 3];

pub(super) fn two_point(a: &Value, b: &Value) -> Result<Value, String> {
    let a = point(a)?;
    let b = point(b)?;
    if a == b {
        return Err("直線の始点と終点は異なる座標にしてください。".into());
    }
    Ok(Value::Curve(Arc::new(Curve {
        points: vec![a, b],
        closed: false,
    })))
}

fn point(input: &Value) -> Result<Point, String> {
    match input {
        Value::Point(p) if finite_point(*p) => Ok(*p),
        _ => Err("有限の座標を持つ点を接続してください。".into()),
    }
}

/// Point paths are authoritative. A sole closure-settings branch can be shared.
pub(super) fn polyline_tree(
    points: &Tree,
    closures: &Tree,
    cancel: &dyn Fn() -> bool,
) -> Result<Tree, String> {
    let mut out = Tree::default();
    for (path, items) in &points.branches {
        cancelled(cancel)?;
        let result = out.branches.entry(path.clone()).or_default();
        if items.is_empty() {
            continue; // A genuinely empty input branch is not a failed polyline.
        }
        let options = closures.branches.get(path).or_else(|| {
            (closures.branches.len() == 1)
                .then(|| closures.branches.values().next())
                .flatten()
        });
        let Some(options) = options.filter(|v| !v.is_empty()) else {
            continue;
        };
        let labels = || {
            format!(
                "折れ線の枝 {{{}}}",
                path.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(";")
            )
        };
        let [option] = options.as_slice() else {
            return Err(format!(
                "{}: 開閉の設定は枝ごとに 1 つにしてください。",
                labels()
            ));
        };
        let closed = option.number()?.round() == 1.;
        let c = make_polyline(items, closed, cancel).map_err(|e| format!("{}: {e}", labels()))?;
        result.push(Value::Curve(Arc::new(c)));
    }
    checked_tree(out, cancel)
}

fn make_polyline(
    items: &[Value],
    closed: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Curve, String> {
    if items.len() > MAX_ITEMS {
        return Err("点の個数が上限を超えています。".into());
    }
    let mut points = Vec::with_capacity(items.len());
    for (i, value) in items.iter().enumerate() {
        if i % 256 == 0 {
            cancelled(cancel)?;
        }
        let p = point(value)?;
        if points.last() == Some(&p) {
            return Err("続けて同じ点を接続できません。".into());
        }
        points.push(p);
    }
    if closed && points.len() > 1 && points.first() == points.last() {
        points.pop(); // Canonical curve storage uses the closed flag, not a seam duplicate.
    }
    if points.len() < if closed { 3 } else { 2 } {
        return Err(if closed {
            "閉じた折れ線には異なる点が 3 個以上必要です。"
        } else {
            "開いた折れ線には異なる点が 2 個以上必要です。"
        }
        .into());
    }
    if closed {
        let mut seen = BTreeSet::new();
        for (i, p) in points.iter().enumerate() {
            if i % 256 == 0 {
                cancelled(cancel)?;
            }
            let key = p.map(|v| if v == 0. { 0_u64 } else { v.to_bits() });
            if !seen.insert(key) {
                return Err("閉じた折れ線には同じ頂点を繰り返せません。".into());
            }
        }
    }
    Ok(Curve { points, closed })
}

pub(super) fn division_count(n: f64) -> Result<usize, String> {
    let rounded = n.round();
    if !n.is_finite()
        || (n - rounded).abs() > 1e-8
        || rounded < 1.
        || rounded > MAX_CURVE_DIVISIONS as f64
    {
        return Err(format!(
            "分割数は 1〜{MAX_CURVE_DIVISIONS} の整数にしてください。"
        ));
    }
    Ok(rounded as usize)
}

pub(super) fn length(curve: &Curve, cancel: &dyn Fn() -> bool) -> Result<f64, String> {
    segment_lengths(curve, cancel, |_, _| {})
}

fn segment_lengths(
    curve: &Curve,
    cancel: &dyn Fn() -> bool,
    mut report: impl FnMut(f64, f64),
) -> Result<f64, String> {
    let n = curve.points.len();
    if n > MAX_VERTICES || n < if curve.closed { 3 } else { 2 } {
        return Err("曲線の点数が無効です。".into());
    }
    let edge_count = if curve.closed { n } else { n - 1 };
    let mut total = 0.;
    for i in 0..edge_count {
        if i % 256 == 0 {
            cancelled(cancel)?;
        }
        let a = curve.points[i];
        let b = curve.points[(i + 1) % n];
        if !finite_point(a) || !finite_point(b) {
            return Err("曲線に無効な座標があります。".into());
        }
        let span = (b[0] - a[0]).hypot(b[1] - a[1]).hypot(b[2] - a[2]);
        if !(span > 0. && span.is_finite()) {
            return Err("曲線に長さゼロの区間があります。".into());
        }
        total += span;
        report(total, span);
    }
    Ok(total)
}

pub(super) fn divide(
    curve: &Curve,
    n: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let mut lengths = Vec::new();
    let total = segment_lengths(curve, cancel, |end, span| lengths.push((end, span)))?;
    let mut positions = Vec::with_capacity(n + usize::from(!curve.closed));
    positions.push(Value::Point(curve.points[0]));
    let mut edge = 0;
    for i in 1..n {
        if i % 256 == 0 {
            cancelled(cancel)?;
        }
        let at = total * (i as f64 / n as f64);
        while edge + 1 < lengths.len() && at > lengths[edge].0 {
            edge += 1;
            if edge % 256 == 0 {
                cancelled(cancel)?;
            }
        }
        let (end, span) = lengths[edge];
        // Measuring back from the edge end retains an exact vertex when at == end.
        let t = (1. - (end - at) / span).clamp(0., 1.);
        let a = curve.points[edge];
        let b = curve.points[(edge + 1) % curve.points.len()];
        positions.push(Value::Point(match t {
            0. => a,
            1. => b,
            _ => std::array::from_fn(|axis| a[axis] + t * (b[axis] - a[axis])),
        }));
    }
    if !curve.closed
        && let Some(&end) = curve.points.last()
    {
        positions.push(Value::Point(end));
    }
    Ok(positions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Evaluator, evaluate_node};
    use crate::model::{Graph, NodeKind};
    use crate::persistence::Project;
    use std::collections::BTreeMap;

    fn evaluate_inputs(kind: NodeKind, inputs: &[Tree]) -> Result<Tree, String> {
        let mut graph = Graph::default();
        let node = graph.add(kind, [0., 0.]).unwrap();
        let node = graph.node_mut(node).unwrap();
        let mut upstream = BTreeMap::new();
        for (i, (spec, tree)) in kind.spec().inputs.iter().zip(inputs).enumerate() {
            let id = 20 + i as u64;
            node.inputs.insert(spec.key.into(), id);
            upstream.insert(id, Ok(tree.clone()));
        }
        evaluate_node(node, &upstream, &|| false)
    }

    fn points(values: &[[f64; 3]]) -> Vec<Value> {
        values.iter().copied().map(Value::Point).collect()
    }

    fn number(tree: &Tree) -> f64 {
        tree.branches[&vec![0]][0].number().unwrap()
    }

    fn curve_at<'a>(tree: &'a Tree, path: &[u32]) -> &'a Arc<Curve> {
        let Value::Curve(c) = &tree.branches[path][0] else {
            panic!("expected a curve")
        };
        c
    }

    #[test]
    fn full_graph_creates_measures_and_divides_line_then_serializes() {
        let mut graph = Graph::default();
        let start = graph.add(NodeKind::Point, [0., 0.]).unwrap();
        let end = graph.add(NodeKind::Point, [0., 120.]).unwrap();
        for (k, v) in [("x", 0.), ("y", 3.), ("z", 4.)] {
            graph.node_mut(end).unwrap().params.insert(k.into(), v);
        }
        let line = graph.add(NodeKind::Line, [200., 0.]).unwrap();
        let length = graph.add(NodeKind::CurveLength, [400., 0.]).unwrap();
        let divide = graph.add(NodeKind::DivideCurve, [400., 120.]).unwrap();
        graph
            .node_mut(divide)
            .unwrap()
            .params
            .insert("count".into(), 2.);
        for (from, to, key) in [
            (start, line, "start"),
            (end, line, "end"),
            (line, length, "curve"),
            (line, divide, "curve"),
        ] {
            graph.connect(from, to, key).unwrap();
        }
        let project = Project::new("点から直線、長さ、分割", graph);
        let bytes = serde_json::to_vec(&project).unwrap();
        let reopened: Project = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reopened, project);
        reopened.validate().unwrap();

        let mut engine = Evaluator::default();
        let evaluated = engine.evaluate(&reopened.graph, &|| false);
        assert!(evaluated.outputs.values().all(Result::is_ok));
        assert_eq!(
            curve_at(evaluated.outputs[&line].as_ref().unwrap(), &[0]).points,
            vec![[0., 0., 0.], [0., 3., 4.]]
        );
        assert_eq!(number(evaluated.outputs[&length].as_ref().unwrap()), 5.);
        assert_eq!(
            evaluated.outputs[&divide].as_ref().unwrap().branches[&vec![0]],
            points(&[[0., 0., 0.], [0., 1.5, 2.], [0., 3., 4.]])
        );
        assert_eq!(engine.evaluate(&reopened.graph, &|| false).cached, 5);

        let mut edited = reopened.graph;
        edited.node_mut(end).unwrap().params.insert("y".into(), 0.);
        assert_eq!(
            number(
                engine.evaluate(&edited, &|| false).outputs[&length]
                    .as_ref()
                    .unwrap()
            ),
            4.
        );
        edited.node_mut(end).unwrap().params.insert("z".into(), 0.);
        assert!(
            engine.evaluate(&edited, &|| false).outputs[&line]
                .as_ref()
                .unwrap_err()
                .contains("異なる座標")
        );
    }

    #[test]
    fn branch_list_and_equal_arc_length_use_the_l_oracle_not_vertex_spacing() {
        let source = Tree {
            branches: BTreeMap::from([
                (vec![0], points(&[[0., 0., 0.], [3., 0., 0.], [3., 4., 0.]])),
                (vec![1], Vec::new()),
                (vec![2, 3], points(&[[0., 0., 0.], [0., 4., 0.]])),
            ]),
        };
        let poly = evaluate_inputs(
            NodeKind::Polyline,
            &[source, Tree::single(Value::Number(0.))],
        )
        .unwrap();
        assert_eq!(curve_at(&poly, &[0]).points.len(), 3);
        assert!(poly.branches[&vec![1]].is_empty());
        assert_eq!(curve_at(&poly, &[2, 3]).points.len(), 2);
        let len = evaluate_inputs(NodeKind::CurveLength, std::slice::from_ref(&poly)).unwrap();
        assert_eq!(number(&len), 7.);
        assert_eq!(len.branches[&vec![2, 3]], vec![Value::Number(4.)]);
        let divided = evaluate_inputs(
            NodeKind::DivideCurve,
            &[poly, Tree::single(Value::Number(4.))],
        )
        .unwrap();
        assert!(divided.branches[&vec![1]].is_empty());
        assert_eq!(divided.branches[&vec![2, 3]].len(), 5);
        let expected = [
            [0., 0., 0.],
            [1.75, 0., 0.],
            [3., 0.5, 0.],
            [3., 2.25, 0.],
            [3., 4., 0.],
        ];
        for (actual, expected) in divided.branches[&vec![0]].iter().zip(expected) {
            let Value::Point(p) = actual else {
                panic!("expected a point")
            };
            assert!(p.iter().zip(expected).all(|(x, y)| (x - y).abs() < 1e-12));
        }
    }

    #[test]
    fn evolve_concat_list_can_drive_a_nonuniform_polyline_end_to_end() {
        let mut graph = Graph::default();
        let mut point = |x: f64, y: f64| {
            let id = graph.add(NodeKind::Point, [0., 0.]).unwrap();
            let params = &mut graph.node_mut(id).unwrap().params;
            params.insert("x".into(), x);
            params.insert("y".into(), y);
            id
        };
        let a = point(0., 0.);
        let b = point(3., 0.);
        let c = point(3., 4.);
        let ab = graph.add(NodeKind::ConcatList, [100., 0.]).unwrap();
        let abc = graph.add(NodeKind::ConcatList, [200., 0.]).unwrap();
        let poly = graph.add(NodeKind::Polyline, [300., 0.]).unwrap();
        let len = graph.add(NodeKind::CurveLength, [400., 0.]).unwrap();
        let div = graph.add(NodeKind::DivideCurve, [500., 0.]).unwrap();
        graph
            .node_mut(div)
            .unwrap()
            .params
            .insert("count".into(), 4.);
        for (source, target, input) in [
            (a, ab, "a"),
            (b, ab, "b"),
            (ab, abc, "a"),
            (c, abc, "b"),
            (abc, poly, "points"),
            (poly, len, "curve"),
            (poly, div, "curve"),
        ] {
            graph.connect(source, target, input).unwrap();
        }
        let out = Evaluator::default().evaluate(&graph, &|| false);
        assert!(out.outputs.values().all(Result::is_ok), "{:?}", out.outputs);
        assert_eq!(number(out.outputs[&len].as_ref().unwrap()), 7.);
        let sample = &out.outputs[&div].as_ref().unwrap().branches[&vec![0]];
        for (actual, expected) in sample.iter().zip(points(&[
            [0., 0., 0.],
            [1.75, 0., 0.],
            [3., 0.5, 0.],
            [3., 2.25, 0.],
            [3., 4., 0.],
        ])) {
            let (Value::Point(a), Value::Point(b)) = (actual, expected) else {
                panic!("expected points")
            };
            assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-12));
        }
        assert_eq!(sample.len(), 5);
    }

    #[test]
    fn explicit_closure_canonicalizes_one_seam_and_counts_no_duplicate() {
        let source = Tree {
            branches: BTreeMap::from([(
                vec![5],
                points(&[
                    [0., 0., 0.],
                    [2., 0., 0.],
                    [2., 2., 0.],
                    [0., 2., 0.],
                    [-0., 0., 0.],
                ]),
            )]),
        };
        let closed = evaluate_inputs(
            NodeKind::Polyline,
            &[source, Tree::single(Value::Number(1.))],
        )
        .unwrap();
        assert!(curve_at(&closed, &[5]).closed);
        assert_eq!(curve_at(&closed, &[5]).points.len(), 4);
        let lengths =
            evaluate_inputs(NodeKind::CurveLength, std::slice::from_ref(&closed)).unwrap();
        assert_eq!(lengths.branches[&vec![5]][0], Value::Number(8.));
        let samples = evaluate_inputs(
            NodeKind::DivideCurve,
            &[closed, Tree::single(Value::Number(4.))],
        )
        .unwrap();
        assert_eq!(
            samples.branches[&vec![5]],
            points(&[[0., 0., 0.], [2., 0., 0.], [2., 2., 0.], [0., 2., 0.]])
        );
    }

    #[test]
    fn bad_closure_settings_and_invalid_geometry_fail_with_helpful_error() {
        let poly = |values: &[[f64; 3]], closures: Vec<Value>| {
            evaluate_inputs(
                NodeKind::Polyline,
                &[
                    Tree {
                        branches: BTreeMap::from([(vec![3, 2], points(values))]),
                    },
                    Tree {
                        branches: BTreeMap::from([(vec![0], closures)]),
                    },
                ],
            )
        };
        let err = poly(&[[0., 0., 0.], [2., 0., 0.]], vec![Value::Number(1.)]).unwrap_err();
        assert!(err.contains("{3;2}") && err.contains("3 個以上"), "{err}");
        assert!(
            poly(&[[0., 0., 0.], [0., 0., 0.]], vec![Value::Number(0.)])
                .unwrap_err()
                .contains("同じ点")
        );
        assert!(
            poly(
                &[[0., 0., 0.], [2., 0., 0.], [2., 2., 0.], [2., 0., 0.]],
                vec![Value::Number(1.)]
            )
            .unwrap_err()
            .contains("同じ頂点")
        );
        assert!(
            poly(
                &[[0., 0., 0.], [1., 0., 0.]],
                vec![Value::Number(0.), Value::Number(1.)]
            )
            .unwrap_err()
            .contains("枝ごとに 1 つ")
        );
        assert!(poly(&[[0., 0., 0.], [1., 0., 0.]], vec![Value::Number(2.)]).is_err());
    }

    #[test]
    fn divide_preflight_bounds_total_points_and_valid_handles_reversed_and_tiny_curves() {
        let curve = Curve {
            points: vec![[1e-200, 0., 0.], [0., 0., 0.]],
            closed: false,
        };
        assert_eq!(length(&curve, &|| false).unwrap(), 1e-200);
        let repeated = Tree {
            branches: BTreeMap::from([(vec![0], vec![Value::Curve(Arc::new(curve)); 9])]),
        };
        let err = evaluate_inputs(
            NodeKind::DivideCurve,
            &[repeated, Tree::single(Value::Number(1024.))],
        )
        .unwrap_err();
        assert!(err.contains("生成前に停止"), "{err}");
        assert!(division_count(1025.).is_err() && division_count(2.5).is_err());
        assert_eq!(division_count(1.999999999).unwrap(), 2);
        assert!(
            divide(
                &Curve {
                    points: vec![[1., 1., 0.], [1., 1., 0.]],
                    closed: false
                },
                4,
                &|| false
            )
            .is_err()
        );
    }

    #[test]
    fn cancellation_reaches_long_curve_segments() {
        use std::cell::Cell;
        let curve = Curve {
            points: (0..1000).map(|i| [f64::from(i), 0., 0.]).collect(),
            closed: false,
        };
        let checks = Cell::new(0);
        let stop = || {
            checks.set(checks.get() + 1);
            checks.get() == 3
        };
        assert!(divide(&curve, 4, &stop).unwrap_err().contains("停止"));
    }
}
