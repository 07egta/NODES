//! Explicit migrations for independently published development graph schemas.
//! A conflicting connection is rejected, never silently replaced.
use crate::model::Graph;

pub fn normalize_legacy_inputs(graph: &mut Graph) -> Result<(), String> {
    normalize(graph, 0)
}

fn normalize(graph: &mut Graph, depth: usize) -> Result<(), String> {
    if depth > crate::cluster::MAX_CLUSTER_DEPTH {
        return Err("まとめノードの階層が深すぎます。".into());
    }
    for node in &mut graph.nodes {
        // Older mission inspectors exposed item matching on list-only adapters.
        // Their item mode was ignored; the selected branch mode is now the
        // shared ordinal default, so retain behavior and remove that stale option.
        if crate::node_access::uses_ordinal_matching(node.kind)
            && crate::node_access::uses_list_access(node.kind)
            && let Some(&value) = node.params.get(crate::data_tree::MATCH_PARAM)
        {
            crate::data_tree::MatchMode::from_number(value)?;
            node.params.remove(crate::data_tree::MATCH_PARAM);
        }
        let aliases: Vec<_> = node
            .inputs
            .keys()
            .filter(|key| node.kind.canonical_input_key(key) != key.as_str())
            .cloned()
            .collect();
        for alias in aliases {
            let canonical = node.kind.canonical_input_key(&alias).to_owned();
            let source = node.inputs[&alias];
            if node
                .inputs
                .get(&canonical)
                .is_some_and(|&existing| existing != source)
            {
                return Err(format!(
                    "ノード {} の旧入力「{}」と入力「{}」の接続が競合しています。元のファイルは変更していません。",
                    node.id, alias, canonical
                ));
            }
            node.inputs.remove(&alias);
            node.inputs.insert(canonical, source);
        }
        let transform_aliases: Vec<_> = node
            .params
            .keys()
            .filter_map(|key| {
                let port = key.strip_prefix(crate::data_tree::PORT_PREFIX)?;
                let canonical = node.kind.canonical_input_key(port);
                (canonical != port).then(|| {
                    (
                        key.clone(),
                        format!("{}{canonical}", crate::data_tree::PORT_PREFIX),
                    )
                })
            })
            .collect();
        for (alias, canonical) in transform_aliases {
            let value = node.params[&alias];
            if node
                .params
                .get(&canonical)
                .is_some_and(|&existing| existing != value)
            {
                return Err(format!(
                    "ノード {} の旧入力と入力のデータツリー設定が競合しています。",
                    node.id
                ));
            }
            node.params.remove(&alias);
            node.params.insert(canonical, value);
        }
    }
    for definition in &mut graph.clusters {
        normalize(&mut definition.graph, depth + 1)?;
        for port in &mut definition.inputs {
            if let Some(node) = definition.graph.node(port.node) {
                port.key = node.kind.canonical_input_key(&port.key).to_owned();
            }
        }
    }
    Ok(())
}
