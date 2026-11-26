//! Direct public adapters must enforce the same contracts as graph evaluation.
use morphlet::data_tree::{self, ItemMatches, MatchMode};
use morphlet::data_tree_nodes::{run_list, run_tree};
use morphlet::model::{Graph, MAX_ITEMS, MAX_TOTAL_WEIGHT, NodeKind, Tree, Value};

fn values(input: &[f64]) -> Vec<Value> {
    input.iter().copied().map(Value::Number).collect()
}

#[test]
fn direct_integer_settings_reject_negative_fractional_and_nonfinite_values() {
    let list = values(&[10., 20.]);
    for invalid in [-1., 0.5, 1.999999999, f64::NAN, f64::INFINITY] {
        let count = values(&[invalid]);
        for kind in [NodeKind::RepeatData, NodeKind::DuplicateData] {
            assert!(
                run_list(kind, &[&list, &count], MAX_ITEMS, MAX_TOTAL_WEIGHT, &|| {
                    false
                })
                .is_err()
            );
            // Empty data must not hide a malformed control value.
            assert!(
                run_list(kind, &[&[], &count], MAX_ITEMS, MAX_TOTAL_WEIGHT, &|| false).is_err()
            );
        }
    }
    let wrap = values(&[1.]);
    let invalid_index = values(&[-0.5]);
    assert!(
        run_list(
            NodeKind::CullIndex,
            &[&list, &invalid_index, &wrap],
            MAX_ITEMS,
            MAX_TOTAL_WEIGHT,
            &|| false
        )
        .is_err()
    );
}

#[test]
fn direct_tree_adapters_validate_paths_and_scalar_settings() {
    let mut graph = Graph::default();
    let id = graph.add(NodeKind::PartitionList, [0., 0.]).unwrap();
    let node = graph.node(id).unwrap();
    for invalid in [-1., 0., 1.5, f64::NAN, f64::INFINITY] {
        let inputs = [
            Tree::single(Value::Number(1.)),
            Tree::single(Value::Number(invalid)),
        ];
        assert!(run_tree(node, &inputs, &|| false).is_err());
    }
    let malformed = Tree {
        branches: [(vec![], values(&[1.]))].into(),
    };
    assert!(
        run_tree(node, &[malformed, Tree::single(Value::Number(1.))], &|| {
            false
        })
        .is_err()
    );
    let wrong_type = Tree::single(Value::Point([1., 0., 0.]));
    assert!(
        run_tree(
            node,
            &[Tree::single(Value::Number(1.)), wrong_type],
            &|| false
        )
        .is_err()
    );
}

#[test]
fn every_list_result_respects_remaining_count_and_weight() {
    let list = values(&[2., 3.]);
    assert!(
        run_list(
            NodeKind::MassAddition,
            &[&list],
            0,
            MAX_TOTAL_WEIGHT,
            &|| false
        )
        .is_err()
    );
    assert!(run_list(NodeKind::MassAddition, &[&list], MAX_ITEMS, 0, &|| false).is_err());
    let index = values(&[1.]);
    let replacement = values(&[4.]);
    let wrap = values(&[0.]);
    assert!(
        run_list(
            NodeKind::ReplaceItems,
            &[&list, &index, &replacement, &wrap],
            1,
            MAX_TOTAL_WEIGHT,
            &|| false
        )
        .is_err()
    );
    assert!(
        run_list(
            NodeKind::InsertItems,
            &[&list, &index, &replacement],
            MAX_ITEMS,
            2,
            &|| false
        )
        .is_err()
    );
    let frequency = values(&[2.]);
    assert!(
        run_list(
            NodeKind::CullNth,
            &[&list, &frequency],
            0,
            MAX_TOTAL_WEIGHT,
            &|| false
        )
        .is_err()
    );
}

#[test]
fn insertion_preserves_order_at_duplicate_positions_and_empty_input() {
    let list = values(&[10., 20.]);
    let indices = values(&[1., 1., 0., 2.]);
    let inserted = values(&[7., 8.]);
    let got = run_list(
        NodeKind::InsertItems,
        &[&list, &indices, &inserted],
        MAX_ITEMS,
        MAX_TOTAL_WEIGHT,
        &|| false,
    )
    .unwrap();
    assert_eq!(got, values(&[8., 10., 7., 8., 20., 8.]));
    assert_eq!(
        run_list(
            NodeKind::InsertItems,
            &[&[], &values(&[0.]), &values(&[42.])],
            MAX_ITEMS,
            MAX_TOTAL_WEIGHT,
            &|| false
        )
        .unwrap(),
        values(&[42.])
    );
}

#[test]
fn direct_aggregation_reports_overflow_and_cancel() {
    let huge = values(&[f64::MAX, f64::MAX]);
    assert!(
        run_list(
            NodeKind::MassAddition,
            &[&huge],
            MAX_ITEMS,
            MAX_TOTAL_WEIGHT,
            &|| false
        )
        .is_err()
    );
    let list = values(&[1., 2., 3.]);
    assert!(
        run_list(
            NodeKind::MassAddition,
            &[&list],
            MAX_ITEMS,
            MAX_TOTAL_WEIGHT,
            &|| true
        )
        .is_err()
    );
    assert_eq!(
        run_list(
            NodeKind::MassAddition,
            &[&[]],
            MAX_ITEMS,
            MAX_TOTAL_WEIGHT,
            &|| false
        )
        .unwrap(),
        values(&[0.])
    );
    assert_eq!(
        run_list(
            NodeKind::MassMultiplication,
            &[&[]],
            MAX_ITEMS,
            MAX_TOTAL_WEIGHT,
            &|| false
        )
        .unwrap(),
        values(&[1.])
    );
}

#[test]
fn mapper_repeated_variables_and_unmatched_paths_are_lossless() {
    let input = Tree {
        branches: [
            (vec![1, 1], values(&[10.])),
            (vec![1, 2], values(&[20.])),
            (vec![7], vec![]),
        ]
        .into(),
    };
    let got = data_tree::path_mapper(&input, "{A;A}->{A}").unwrap();
    assert_eq!(got.branches[&vec![1]], values(&[10.]));
    assert_eq!(got.branches[&vec![1, 2]], values(&[20.]));
    assert_eq!(got.branches[&vec![7]], vec![]);
    assert_eq!(got.item_count(), input.item_count());
}

#[test]
fn item_matching_rejects_overflow_before_enumeration_and_is_empty_if_any_port_empty() {
    let pair = values(&[1., 2.]);
    let many = vec![pair.as_slice(); usize::BITS as usize];
    assert!(ItemMatches::new(&many, MatchMode::Cross, MAX_ITEMS).is_err());
    let mut empty = many;
    empty.push(&[]);
    assert_eq!(
        ItemMatches::new(&empty, MatchMode::Cross, MAX_ITEMS)
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn member_search_budgets_geometry_comparison_work() {
    use morphlet::model::Curve;
    use std::sync::Arc;
    let shape = Value::Curve(Arc::new(Curve {
        points: vec![[1., 2., 3.]; 200],
        closed: false,
    }));
    let list = vec![shape.clone(); 100];
    let members = vec![shape; 100];
    // Only 10,000 pairs, but up to 2,000,000 point comparisons.
    let error = run_list(
        NodeKind::MemberIndex,
        &[&list, &members],
        MAX_ITEMS,
        MAX_TOTAL_WEIGHT,
        &|| false,
    )
    .unwrap_err();
    assert!(error.contains("計算量"));
    let list = values(&[7., 8., 7.]);
    assert_eq!(
        run_list(
            NodeKind::MemberIndex,
            &[&list, &values(&[7.])],
            MAX_ITEMS,
            MAX_TOTAL_WEIGHT,
            &|| false
        )
        .unwrap(),
        values(&[0., 2.])
    );
}

#[test]
fn singleton_simplification_follows_documented_nodes_api_policy() {
    let input = Tree {
        branches: [(vec![4, 2, 9], values(&[1., 2.]))].into(),
    };
    assert_eq!(
        data_tree::simplify(&input).unwrap().branches,
        [(vec![9], values(&[1., 2.]))].into()
    );
}
