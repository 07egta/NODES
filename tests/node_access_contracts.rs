//! Access contracts are explicit product behavior, not claims of GH equivalence.
use nodes::data_tree::MATCH_PARAM;
use nodes::engine::Evaluator;
use nodes::model::{Graph, NodeId, NodeKind, Tree, Value};
use nodes::node_access::{
    InputAccess as A, for_input, supports_item_matching, uses_list_access, uses_ordinal_matching,
};

#[test]
fn mathematical_list_ports_and_scalar_settings_are_distinguished() {
    let cases: &[(NodeKind, &[A])] = &[
        (NodeKind::Average, &[A::List]),
        (NodeKind::SetUnion, &[A::List, A::List]),
        (NodeKind::EvaluateDomain, &[A::List, A::List]),
        (NodeKind::DomainEndpoint, &[A::List, A::FirstItem]),
        (
            NodeKind::MatrixMultiply,
            &[A::List, A::List, A::FirstItem, A::FirstItem, A::FirstItem],
        ),
        (NodeKind::LinearSolve, &[A::List, A::List, A::FirstItem]),
        (NodeKind::ClosestPoint, &[A::List, A::List]),
    ];
    for &(kind, expected) in cases {
        assert_eq!(kind.spec().inputs.len(), expected.len(), "{kind:?}");
        assert!(uses_list_access(kind), "{kind:?}");
        assert!(!supports_item_matching(kind), "{kind:?}");
        for (index, access) in expected.iter().copied().enumerate() {
            assert_eq!(for_input(kind, index), Some(access), "{kind:?}/{index}");
        }
    }
}

#[test]
fn curve_and_generation_ports_describe_their_actual_adapters() {
    let cases: &[(NodeKind, &[A])] = &[
        (
            NodeKind::CurveWeightedNurbs,
            &[A::List, A::List, A::List, A::FirstItem, A::FirstItem],
        ),
        (NodeKind::CurvePolyline, &[A::List, A::FirstItem]),
        (NodeKind::CurveShatter, &[A::List, A::List, A::FirstItem]),
        (NodeKind::Voronoi2d, &[A::List, A::FirstItem, A::FirstItem]),
        (
            NodeKind::PowerDiagram2d,
            &[A::List, A::List, A::FirstItem, A::FirstItem],
        ),
        (
            NodeKind::LloydRelax2d,
            &[A::List, A::FirstItem, A::FirstItem, A::FirstItem],
        ),
        (NodeKind::MultiAttractor, &[A::List, A::List]),
        (NodeKind::PointArray, &[A::List, A::SingleItem]),
        (NodeKind::MeshConstruct, &[A::List, A::List]),
    ];
    for &(kind, expected) in cases {
        assert_eq!(kind.spec().inputs.len(), expected.len(), "{kind:?}");
        assert!(uses_list_access(kind), "{kind:?}");
        assert!(!supports_item_matching(kind), "{kind:?}");
        for (index, access) in expected.iter().copied().enumerate() {
            assert_eq!(for_input(kind, index), Some(access), "{kind:?}/{index}");
        }
    }
}

#[test]
fn whole_tree_and_legacy_special_routes_keep_their_contracts() {
    assert_eq!(for_input(NodeKind::PartitionList, 0), Some(A::Tree));
    assert_eq!(for_input(NodeKind::PartitionList, 1), Some(A::FirstItem));
    assert_eq!(for_input(NodeKind::TreeItem, 0), Some(A::Tree));
    assert_eq!(for_input(NodeKind::TreeItem, 1), Some(A::List));
    assert_eq!(for_input(NodeKind::TreeItem, 2), Some(A::FirstItem));
    assert_eq!(for_input(NodeKind::UnflattenTree, 1), Some(A::Tree));
    assert_eq!(for_input(NodeKind::Output, 0), Some(A::Tree));
    assert_eq!(for_input(NodeKind::Loft, 0), Some(A::List));
    assert_eq!(for_input(NodeKind::Loft, 1), Some(A::FirstItem));
    assert_eq!(for_input(NodeKind::ImportMesh, 0), Some(A::FirstItem));
    assert_eq!(for_input(NodeKind::Script, 0), Some(A::Custom));
    for kind in [
        NodeKind::TreeItem,
        NodeKind::PartitionList,
        NodeKind::ImportMesh,
        NodeKind::Script,
        NodeKind::Cluster,
        NodeKind::Loft,
    ] {
        assert!(!supports_item_matching(kind), "{kind:?}");
    }
}

#[test]
fn catalog_ports_are_total_and_nonexistent_ports_are_not_invented() {
    for &kind in NodeKind::all() {
        for index in 0..kind.spec().inputs.len() {
            let access =
                for_input(kind, index).expect("every declared input needs access metadata");
            assert!(!access.label().is_empty());
            assert!(!access.description().is_empty());
        }
        assert_eq!(for_input(kind, kind.spec().inputs.len()), None, "{kind:?}");
        if uses_list_access(kind) {
            assert!(!supports_item_matching(kind), "{kind:?}");
        }
    }
}

#[test]
fn only_new_families_default_to_ordinal_matching() {
    for kind in [
        NodeKind::Add,
        NodeKind::Circle,
        NodeKind::Series,
        NodeKind::Remap,
        NodeKind::Loft,
        NodeKind::Script,
        NodeKind::Cluster,
        NodeKind::ImportMesh,
    ] {
        assert!(!uses_ordinal_matching(kind), "legacy {kind:?}");
    }
    for kind in [
        NodeKind::Lerp,
        NodeKind::SetUnion,
        NodeKind::CurveWeightedNurbs,
        NodeKind::CurveEvaluate,
        NodeKind::Voronoi2d,
        NodeKind::MeshConstruct,
        NodeKind::MeshArea,
        NodeKind::PlaneSurface,
        NodeKind::BrepUnion,
        NodeKind::RepeatData,
    ] {
        assert!(uses_ordinal_matching(kind), "new {kind:?}");
    }
    assert!(supports_item_matching(NodeKind::Add));
    assert!(supports_item_matching(NodeKind::Lerp));
    assert!(supports_item_matching(NodeKind::CurveEvaluate));
    assert!(supports_item_matching(NodeKind::BrepUnion));
}

fn branched_inputs() -> (Graph, NodeId, NodeId) {
    let mut graph = Graph::default();
    let a = graph.add(NodeKind::Series, [0., 0.]).unwrap();
    let b = graph.add(NodeKind::Series, [0., 100.]).unwrap();
    graph.node_mut(a).unwrap().params.extend([
        ("start".into(), 1.),
        ("step".into(), 1.),
        ("count".into(), 2.),
    ]);
    graph.node_mut(b).unwrap().params.extend([
        ("start".into(), 10.),
        ("step".into(), 10.),
        ("count".into(), 2.),
    ]);
    let graft_a = graph.add(NodeKind::Graft, [200., 0.]).unwrap();
    let graft_b = graph.add(NodeKind::Graft, [200., 100.]).unwrap();
    graph.connect(a, graft_a, "geometry").unwrap();
    graph.connect(b, graft_b, "geometry").unwrap();
    let rename_b = graph.add(NodeKind::PathMapper, [400., 100.]).unwrap();
    graph.connect(graft_b, rename_b, "tree").unwrap();
    graph.node_mut(rename_b).unwrap().script = "{A;B}->{9;B}".into();
    (graph, graft_a, rename_b)
}
fn output(graph: &Graph, node: NodeId) -> Tree {
    Evaluator::default()
        .evaluate(graph, &|| false)
        .outputs
        .remove(&node)
        .unwrap()
        .unwrap()
}
fn numbers(v: &[f64]) -> Vec<Value> {
    v.iter().copied().map(Value::Number).collect()
}

#[test]
fn new_list_and_item_nodes_match_disjoint_branch_paths_ordinally() {
    let (mut graph, a, b) = branched_inputs();
    let union = graph.add(NodeKind::SetUnion, [600., 0.]).unwrap();
    graph.connect(a, union, "list").unwrap();
    graph.connect(b, union, "other").unwrap();
    let result = output(&graph, union);
    assert_eq!(
        result.branches,
        [
            (vec![0, 0], numbers(&[1., 10.])),
            (vec![0, 1], numbers(&[2., 20.])),
        ]
        .into()
    );
    let lerp = graph.add(NodeKind::Lerp, [600., 100.]).unwrap();
    graph.connect(a, lerp, "a").unwrap();
    graph.connect(b, lerp, "b").unwrap();
    graph.node_mut(lerp).unwrap().params.insert("t".into(), 0.5);
    assert_eq!(
        output(&graph, lerp).branches,
        [(vec![0, 0], numbers(&[5.5])), (vec![0, 1], numbers(&[11.])),].into()
    );
}

#[test]
fn legacy_item_paths_remain_unchanged_until_matching_is_selected() {
    let (mut graph, a, b) = branched_inputs();
    let sum = graph.add(NodeKind::Add, [600., 0.]).unwrap();
    graph.connect(a, sum, "a").unwrap();
    graph.connect(b, sum, "b").unwrap();
    let legacy = output(&graph, sum);
    assert_eq!(legacy.branches.len(), 4);
    assert_eq!(legacy.item_count(), 0);
    graph
        .node_mut(sum)
        .unwrap()
        .params
        .insert(MATCH_PARAM.into(), 0.);
    assert_eq!(
        output(&graph, sum).branches,
        [(vec![0, 0], numbers(&[11.])), (vec![0, 1], numbers(&[22.])),].into()
    );
}
