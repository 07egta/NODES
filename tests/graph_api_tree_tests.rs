use nodes::{
    data_tree::{MATCH_PARAM, PORT_PREFIX},
    graph_api::{self, Edit, Request},
    model::{Graph, NodeKind as K, Value},
    persistence::{self, Project},
};

#[test]
fn structured_api_configures_cross_matching_and_input_grafting() {
    let request = graph_api::parse_request(
        br#"{"api_version":1,"edits":[
      {"op":"add","alias":"left","kind":"series","position":[0,0]},
      {"op":"set","node":"left","input":"start","value":1},
      {"op":"set","node":"left","input":"step","value":1},
      {"op":"set","node":"left","input":"count","value":2},
      {"op":"add","alias":"right","kind":"series","position":[0,100]},
      {"op":"set","node":"right","input":"start","value":10},
      {"op":"set","node":"right","input":"step","value":10},
      {"op":"set","node":"right","input":"count","value":3},
      {"op":"add","alias":"sum","kind":"add","position":[200,0]},
      {"op":"connect","from":"left","to":"sum","input":"a"},
      {"op":"connect","from":"right","to":"sum","input":"b"},
      {"op":"set","node":"sum","input":"__data_matching","value":2},
      {"op":"set","node":"sum","input":"__tree_port:a","value":2}
    ]}"#,
    )
    .unwrap();
    let project = graph_api::build(&request).unwrap();
    let evaluation = graph_api::evaluate(&project.graph, &|| false).unwrap();
    let expected = [
        (
            vec![0, 0],
            vec![Value::Number(11.), Value::Number(21.), Value::Number(31.)],
        ),
        (
            vec![0, 1],
            vec![Value::Number(12.), Value::Number(22.), Value::Number(32.)],
        ),
    ]
    .into();
    assert_eq!(evaluation.outputs[&2].as_ref().unwrap().branches, expected);
    let reloaded = persistence::decode_project(&serde_json::to_vec(&project).unwrap()).unwrap();
    graph_api::validate_safe(&reloaded.graph).unwrap();
    assert_eq!(reloaded, project);
}

#[test]
fn invalid_reserved_tree_settings_are_rejected_atomically() {
    let mut graph = Graph::default();
    let id = graph.add(K::Add, [0.; 2]).unwrap();
    let before = graph.clone();
    for (key, value) in [
        (MATCH_PARAM, 3.),
        (MATCH_PARAM, 0.5),
        (MATCH_PARAM, f64::NAN),
        ("__tree_port:a", 5.),
        ("__wire:a", 3.),
        ("__wire:missing", 1.),
        ("__tree_port:a", f64::INFINITY),
        ("__tree_port:missing", 1.),
    ] {
        let request = Request {
            api_version: 1,
            edits: vec![Edit::Set {
                node: id.to_string(),
                input: key.into(),
                value,
            }],
        };
        assert!(
            graph_api::apply(&mut graph, &request).is_err(),
            "{key}={value}"
        );
        assert_eq!(graph, before);
    }
    let list = graph.add(K::Summation, [0.; 2]).unwrap();
    let before = graph.clone();
    let request = Request {
        api_version: 1,
        edits: vec![Edit::Set {
            node: list.to_string(),
            input: MATCH_PARAM.into(),
            value: 2.,
        }],
    };
    assert!(graph_api::apply(&mut graph, &request).is_err());
    assert_eq!(graph, before);
}

#[test]
fn legacy_port_transforms_migrate_and_conflicts_remain_explicit() {
    let mut graph = Graph::default();
    let id = graph.add(K::ClosestPoint, [0.; 2]).unwrap();
    let alias = format!("{PORT_PREFIX}points");
    let canonical = format!("{PORT_PREFIX}candidates");
    graph.node_mut(id).unwrap().params.insert(alias.clone(), 2.);
    let project = Project::new("old port transform", graph.clone());
    let decoded = persistence::decode_project(&serde_json::to_vec(&project).unwrap()).unwrap();
    assert_eq!(
        decoded.graph.node(id).unwrap().params.get(&canonical),
        Some(&2.)
    );
    assert!(!decoded.graph.node(id).unwrap().params.contains_key(&alias));
    graph.node_mut(id).unwrap().params.insert(canonical, 1.);
    let conflicting = Project::new("conflicting transforms", graph);
    assert!(
        persistence::decode_project(&serde_json::to_vec(&conflicting).unwrap())
            .unwrap_err()
            .contains("競合")
    );
}
