//! Independent answers for the legacy list aggregates, including branch use.
use nodes::{
    data_tree_nodes,
    engine::Evaluator,
    model::{Graph, MAX_ITEMS, MAX_TOTAL_WEIGHT, NodeKind, Value},
    persistence::{self, Project},
};

fn aggregate(kind: NodeKind, data: &[f64]) -> Result<f64, String> {
    let values: Vec<_> = data.iter().copied().map(Value::Number).collect();
    let result =
        data_tree_nodes::run_list(kind, &[&values], MAX_ITEMS, MAX_TOTAL_WEIGHT, &|| false)?;
    assert_eq!(result.len(), 1);
    result[0].number()
}

#[test]
fn mass_sum_keeps_small_terms_through_cancellation() {
    for data in [[1e16, 1.0, -1e16], [1e16, -1e16, 1.0], [1.0, 1e16, -1e16]] {
        assert_eq!(aggregate(NodeKind::MassAddition, &data).unwrap(), 1.0);
    }
}

#[test]
fn mass_sum_allows_finite_results_after_large_intermediates() {
    assert_eq!(
        aggregate(NodeKind::MassAddition, &[f64::MAX, f64::MAX, -f64::MAX]).unwrap(),
        f64::MAX,
    );
    assert!(aggregate(NodeKind::MassAddition, &[f64::MAX, f64::MAX]).is_err());
}

#[test]
fn mass_product_keeps_an_exact_binary_exponent_until_the_end() {
    let large = 2.0_f64.powi(1000);
    let small = 2.0_f64.powi(-1000);
    for data in [[large, large, small, small], [small, small, large, large]] {
        assert_eq!(aggregate(NodeKind::MassMultiplication, &data).unwrap(), 1.0);
    }
}

#[test]
fn mass_product_handles_zero_sign_subnormals_and_true_range_loss() {
    assert_eq!(
        aggregate(NodeKind::MassMultiplication, &[f64::MAX, f64::MAX, 0.0]).unwrap(),
        0.0
    );
    let tiny = f64::from_bits(1);
    assert_eq!(
        aggregate(NodeKind::MassMultiplication, &[tiny, 2.0])
            .unwrap()
            .to_bits(),
        2
    );
    assert!(
        aggregate(NodeKind::MassMultiplication, &[-0.0, 2.0])
            .unwrap()
            .is_sign_negative()
    );
    assert!(aggregate(NodeKind::MassMultiplication, &[tiny, 0.5]).is_err());
    assert!(aggregate(NodeKind::MassMultiplication, &[f64::MAX, 2.0]).is_err());
}

#[test]
fn aggregate_empty_invalid_and_budget_contracts_remain_explicit() {
    assert_eq!(aggregate(NodeKind::MassAddition, &[]).unwrap(), 0.0);
    assert_eq!(aggregate(NodeKind::MassMultiplication, &[]).unwrap(), 1.0);
    for kind in [NodeKind::MassAddition, NodeKind::MassMultiplication] {
        assert!(aggregate(kind, &[f64::NAN]).is_err());
        assert!(aggregate(kind, &[f64::INFINITY, 0.0]).is_err());
        let values = [Value::Number(2.0)];
        assert!(
            data_tree_nodes::run_list(kind, &[&values], 0, MAX_TOTAL_WEIGHT, &|| false).is_err()
        );
        assert!(data_tree_nodes::run_list(kind, &[&values], MAX_ITEMS, 0, &|| false).is_err());
        assert!(
            data_tree_nodes::run_list(kind, &[&values], MAX_ITEMS, MAX_TOTAL_WEIGHT, &|| true)
                .is_err()
        );
    }
}

#[test]
fn graph_aggregation_preserves_branches_cache_and_serialized_parameters() {
    let mut graph = Graph::default();
    let source = graph.add(NodeKind::Script, [0.0, 0.0]).unwrap();
    graph.node_mut(source).unwrap().script = "[1.0e16, 1.0, -1.0e16, 2.0, 3.0, 4.0]".into();
    let partition = graph.add(NodeKind::PartitionList, [100.0, 0.0]).unwrap();
    let partition_spec = NodeKind::PartitionList.spec();
    graph
        .node_mut(partition)
        .unwrap()
        .params
        .insert(partition_spec.inputs[1].key.into(), 3.0);
    graph
        .connect(source, partition, partition_spec.inputs[0].key)
        .unwrap();
    let sum = graph.add(NodeKind::MassAddition, [200.0, 0.0]).unwrap();
    let key = NodeKind::MassAddition.spec().inputs[0].key;
    graph.connect(partition, sum, key).unwrap();
    let mut evaluator = Evaluator::default();
    let first = evaluator.evaluate(&graph, &|| false);
    let result = first.outputs[&sum].as_ref().unwrap();
    let numbers: Vec<_> = result
        .branches
        .values()
        .flatten()
        .map(|v| v.number().unwrap())
        .collect();
    assert_eq!(numbers, vec![1.0, 9.0]);
    assert_eq!(
        result.branches.keys().cloned().collect::<Vec<_>>(),
        vec![vec![0, 0], vec![0, 1]]
    );
    let project = Project::new("Branch sums", graph);
    let text = serde_json::to_vec(&project).unwrap();
    let loaded = persistence::decode_project(&text).unwrap();
    let second = evaluator.evaluate(&loaded.graph, &|| false);
    assert_eq!(&second.outputs[&sum], &first.outputs[&sum]);
    assert_eq!(second.computed, 0);
}
