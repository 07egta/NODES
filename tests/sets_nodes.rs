use morphlet::data_tree_nodes::run_list;
use morphlet::engine::Evaluator;
use morphlet::graph_api::catalog;
use morphlet::model::{Graph, MAX_ITEMS, NodeId, NodeKind, Tree, Value};
use morphlet::persistence::{Project, load, save};

fn add(graph: &mut Graph, kind: NodeKind, params: &[(&str, f64)]) -> NodeId {
    let id = graph.add(kind, [0., 0.]).unwrap();
    graph
        .node_mut(id)
        .unwrap()
        .params
        .extend(params.iter().map(|&(key, value)| (key.to_owned(), value)));
    id
}

fn series(graph: &mut Graph, start: f64, count: f64) -> NodeId {
    add(
        graph,
        NodeKind::Series,
        &[("start", start), ("step", 1.), ("count", count)],
    )
}

fn numbers(values: &[f64]) -> Vec<Value> {
    values.iter().copied().map(Value::Number).collect()
}

fn branch<'a>(tree: &'a Tree, path: &[u32]) -> &'a [Value] {
    &tree.branches[path]
}

#[test]
fn linked_sequence_set_graph_survives_native_roundtrip() {
    use NodeKind::*;
    let mut graph = Graph::default();
    let sequence = series(&mut graph, 10., 4.);
    let pattern = series(&mut graph, 0., 2.);
    let fib = add(&mut graph, Fibonacci, &[("count", 6.)]);
    let stack = add(&mut graph, StackData, &[]);
    let weave = add(&mut graph, Weave, &[]);
    let graft = add(&mut graph, Graft, &[]);
    let eleven = add(&mut graph, Number, &[("value", 11.)]);
    let symmetric = add(&mut graph, SetSymmetricDifference, &[]);
    let subset = add(&mut graph, SetSubset, &[]);
    let disjoint = add(&mut graph, SetDisjoint, &[]);
    let majority = add(&mut graph, SetMajority, &[]);
    for (source, node, port) in [
        (sequence, stack, "data"),
        (pattern, stack, "stack"),
        (pattern, weave, "pattern"),
        (stack, weave, "stream_0"),
        (fib, weave, "stream_1"),
        (stack, graft, "geometry"),
        (graft, symmetric, "a"),
        (eleven, symmetric, "b"),
        (graft, subset, "a"),
        (eleven, subset, "b"),
        (graft, disjoint, "a"),
        (eleven, disjoint, "b"),
        (graft, majority, "a"),
        (eleven, majority, "b"),
        (symmetric, majority, "c"),
    ] {
        graph.connect(source, node, port).unwrap();
    }
    let project = Project::new("Sets roundtrip", graph);
    let path =
        std::env::temp_dir().join(format!("nodes-sets-roundtrip-{}.morph", std::process::id()));
    save(&path, &project).unwrap();
    let reopened = load(&path);
    std::fs::remove_file(path).unwrap();
    let reopened = reopened.unwrap();
    assert_eq!(reopened, project);

    let eval = Evaluator::default().evaluate(&reopened.graph, &|| false);
    let tree = |id| eval.outputs[&id].as_ref().unwrap();
    assert_eq!(branch(tree(fib), &[0]), numbers(&[0., 1., 1., 2., 3., 5.]));
    assert_eq!(branch(tree(stack), &[0]), numbers(&[11., 13.]));
    assert_eq!(
        branch(tree(weave), &[0]),
        numbers(&[11., 0., 13., 1., 1., 2., 3., 5.])
    );
    assert_eq!(branch(tree(symmetric), &[0, 0]), &[]);
    assert_eq!(branch(tree(symmetric), &[0, 1]), numbers(&[13., 11.]));
    assert_eq!(branch(tree(subset), &[0, 0]), numbers(&[1.]));
    assert_eq!(branch(tree(subset), &[0, 1]), numbers(&[0.]));
    assert_eq!(branch(tree(disjoint), &[0, 0]), numbers(&[0.]));
    assert_eq!(branch(tree(disjoint), &[0, 1]), numbers(&[1.]));
    assert_eq!(branch(tree(majority), &[0, 0]), numbers(&[11.]));
    assert_eq!(branch(tree(majority), &[0, 1]), numbers(&[13., 11.]));

    let public_catalog = catalog();
    let info = |kind| {
        public_catalog
            .nodes
            .iter()
            .find(|n| n.kind == kind)
            .unwrap()
    };
    for kind in [
        StackData,
        Weave,
        SetSymmetricDifference,
        SetSubset,
        SetDisjoint,
        SetMajority,
    ] {
        assert!(info(kind).inputs.iter().all(|p| p.access == "List"));
    }
    assert!(info(Fibonacci).inputs.iter().all(|p| p.access == "Item"));
    assert!(info(Fibonacci).item_matching_configurable);
}

#[test]
fn fibonacci_matches_seed_items_and_errors_on_nonfinite_terms() {
    use NodeKind::*;
    let mut graph = Graph::default();
    let seeds = series(&mut graph, 2., 2.);
    let fib = add(&mut graph, Fibonacci, &[("b", 3.), ("count", 4.)]);
    let overflow = add(&mut graph, Fibonacci, &[("count", 2_000.)]);
    let zero = add(&mut graph, Fibonacci, &[("count", 0.)]);
    let one = add(&mut graph, Fibonacci, &[("a", -2.), ("count", 1.)]);
    graph.connect(seeds, fib, "a").unwrap();
    let eval = Evaluator::default().evaluate(&graph, &|| false);
    assert_eq!(
        branch(eval.outputs[&fib].as_ref().unwrap(), &[0]),
        numbers(&[2., 3., 5., 8., 3., 3., 6., 9.])
    );
    assert!(
        eval.outputs[&overflow]
            .as_ref()
            .unwrap_err()
            .contains("有限")
    );
    assert_eq!(branch(eval.outputs[&zero].as_ref().unwrap(), &[0]), &[]);
    assert_eq!(
        branch(eval.outputs[&one].as_ref().unwrap(), &[0]),
        numbers(&[-2.])
    );
}

#[test]
fn sets_use_unique_presence_per_input_and_preserve_first_appearance() {
    use NodeKind::*;
    let x = Value::Point([1., 2., 3.]);
    let y = Value::Point([2., 3., 4.]);
    let a = [Value::Number(-0.), x.clone(), x.clone()];
    let b = [y.clone(), Value::Number(0.), Value::Number(8.)];
    let c = [y.clone(), y.clone(), Value::Number(4.), Value::Number(4.)];
    let apply =
        |kind, lists: &[&[Value]]| run_list(kind, lists, MAX_ITEMS, MAX_ITEMS, &|| false).unwrap();
    assert_eq!(
        apply(SetMajority, &[&a, &b, &c]),
        vec![Value::Number(0.), y.clone()]
    );
    assert_eq!(
        apply(SetSymmetricDifference, &[&a, &b]),
        vec![x, y, Value::Number(8.)]
    );
    assert_eq!(apply(SetSubset, &[&[], &[]]), numbers(&[1.]));
    assert_eq!(apply(SetSubset, &[&[], &b]), numbers(&[0.]));
    assert_eq!(apply(SetDisjoint, &[&[], &b]), numbers(&[1.]));
    assert_eq!(apply(SetDisjoint, &[&a, &b]), numbers(&[0.]));
}

#[test]
fn list_expansion_validates_patterns_and_output_budgets() {
    use NodeKind::*;
    let data = numbers(&[10., 11., 12., 13., 14.]);
    let repeat = numbers(&[2., 0.]);
    let apply =
        |kind, lists: &[&[Value]], items, weight| run_list(kind, lists, items, weight, &|| false);
    assert_eq!(
        apply(StackData, &[&data, &repeat], 6, 6).unwrap(),
        numbers(&[10., 10., 12., 12., 14., 14.])
    );
    assert!(apply(StackData, &[&data, &repeat], 5, 6).is_err());
    assert!(apply(StackData, &[&data, &repeat], 6, 5).is_err());
    assert!(apply(StackData, &[&data, &numbers(&[1.5])], 30, 30).is_err());
    assert!(apply(StackData, &[&data, &numbers(&[-1.])], 30, 30).is_err());
    assert!(apply(Weave, &[&numbers(&[2.]), &data, &[]], 30, 30).is_err());
    assert_eq!(
        apply(
            Weave,
            &[&numbers(&[0., 0., 1.]), &data, &numbers(&[1., 2.])],
            7,
            7
        )
        .unwrap(),
        numbers(&[10., 11., 1., 12., 13., 2., 14.])
    );
    assert_eq!(
        apply(Weave, &[&numbers(&[1.]), &data, &numbers(&[2.])], 1, 1).unwrap(),
        numbers(&[2.])
    );
    assert_eq!(apply(Weave, &[&[], &data, &data], 0, 0).unwrap(), &[]);
    assert!(apply(Weave, &[&numbers(&[0.]), &data, &data], 4, 5).is_err());
    assert!(apply(SetSymmetricDifference, &[&data, &[]], 4, 5).is_err());
    assert!(run_list(Weave, &[&[], &data, &[]], 100, 100, &|| true).is_err());
}
