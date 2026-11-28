//! Cluster input fan-out contracts. Run with cargo test --test cluster_fanout.
use nodes::cluster::{self, ClusterPort};
use nodes::engine::Evaluator;
use nodes::model::{Graph, NodeId, NodeKind, Tree};

fn evaluate(graph: &Graph, id: NodeId) -> Result<Tree, String> {
    Evaluator::default()
        .evaluate(graph, &|| false)
        .outputs
        .remove(&id)
        .ok_or_else(|| "missing output".to_owned())?
}

fn shared_chain() -> Result<(Graph, NodeId, Vec<NodeId>), String> {
    let mut graph = Graph::default();
    let source = graph.add(NodeKind::Number, [0., 0.])?;
    let mut selected = Vec::new();
    for index in 0..10 {
        let node = graph.add(NodeKind::Add, [100. * (index + 1) as f32, 0.])?;
        graph.connect(selected.last().copied().unwrap_or(source), node, "a")?;
        graph.connect(source, node, "b")?;
        selected.push(node);
    }
    Ok((graph, source, selected))
}

#[test]
fn repeated_source_uses_one_slot_even_above_old_eight_input_limit() -> Result<(), String> {
    let (mut graph, source, selected) = shared_chain()?;
    let last = selected.last().copied().ok_or("missing selected node")?;
    let before = evaluate(&graph, last)?;
    let cluster_id = cluster::group_nodes(&mut graph, &selected, "Shared")?;
    let def = graph.clusters.first().ok_or("missing definition")?;
    assert_eq!(def.inputs.len(), 1);
    assert_eq!(def.inputs[0].targets().count(), 11);
    assert_eq!(
        graph
            .node(cluster_id)
            .ok_or("missing cluster")?
            .inputs
            .get("in0"),
        Some(&source)
    );
    graph.validate()?;
    assert_eq!(evaluate(&graph, cluster_id)?, before);

    let json = serde_json::to_string(&graph).map_err(|e| e.to_string())?;
    let mut loaded: Graph = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    loaded.validate()?;
    assert_eq!(loaded, graph);
    assert_eq!(evaluate(&loaded, cluster_id)?, before);
    loaded
        .node_mut(source)
        .ok_or("missing source")?
        .params
        .insert("value".into(), 2.);
    assert_ne!(evaluate(&loaded, cluster_id)?, before);

    let part = cluster::export_part(&graph, def.id)?;
    let mut imported = Graph::default();
    let input = imported.add(NodeKind::Number, [0., 0.])?;
    let root = cluster::import_part(&mut imported, part.as_bytes())?;
    let instance = cluster::instantiate(&mut imported, root, [200., 0.])?;
    imported.connect(input, instance, "in0")?;
    assert_eq!(evaluate(&imported, instance)?, before);
    Ok(())
}

#[test]
fn different_sources_remain_distinct_even_when_values_are_equal() -> Result<(), String> {
    let mut graph = Graph::default();
    let a = graph.add(NodeKind::Number, [0., 0.])?;
    let b = graph.add(NodeKind::Number, [0., 100.])?;
    let add = graph.add(NodeKind::Add, [200., 0.])?;
    graph.connect(a, add, "a")?;
    graph.connect(b, add, "b")?;
    let grouped = cluster::group_nodes(&mut graph, &[add], "Distinct")?;
    assert_eq!(graph.clusters[0].inputs.len(), 2);
    assert_eq!(
        graph.node(grouped).ok_or("missing cluster")?.inputs.len(),
        2
    );
    Ok(())
}

#[test]
fn duplicate_inner_target_is_rejected() -> Result<(), String> {
    let (mut graph, _, selected) = shared_chain()?;
    cluster::group_nodes(&mut graph, &selected, "Shared")?;
    let port = &mut graph.clusters[0].inputs[0];
    let duplicate = port.additional_targets[0].clone();
    port.additional_targets.push(duplicate);
    assert!(graph.validate().is_err());
    Ok(())
}

#[test]
fn old_port_json_stays_readable_and_serializes_without_new_field() -> Result<(), String> {
    let json = r#"{"node":1,"key":"a","label":"Input"}"#;
    let port: ClusterPort = serde_json::from_str(json).map_err(|e| e.to_string())?;
    assert_eq!(port.targets().collect::<Vec<_>>(), vec![(1, "a")]);
    assert_eq!(
        serde_json::to_string(&port).map_err(|e| e.to_string())?,
        json
    );
    Ok(())
}
