use nodes::cluster;
use nodes::model::{Graph, NodeKind};
use nodes::node_package::{MAX_PACKAGE_BYTES, Manifest, Package};

fn manifest() -> Manifest {
    Manifest {
        id: "studio.number".into(),
        version: "1.2.3".into(),
        title: "数値部品".into(),
        author: "Example studio".into(),
        description: "A reusable number node.".into(),
        license: "MIT".into(),
        tags: vec!["math".into()],
    }
}

fn part() -> String {
    let mut graph = Graph::default();
    let id = graph.add(NodeKind::Number, [0., 0.]).unwrap();
    let instance = cluster::group_nodes(&mut graph, &[id], "Number part").unwrap();
    let root = graph.node(instance).unwrap().cluster.unwrap();
    cluster::export_part(&graph, root).unwrap()
}

#[test]
fn exact_part_round_trip_and_import() {
    let source = part();
    let package = Package::from_part(manifest(), source.as_bytes()).unwrap();
    let encoded = package.encode().unwrap();
    let decoded = Package::decode(&encoded).unwrap();
    assert_eq!(decoded.part_bytes(), source.as_bytes());
    assert_eq!(decoded.info().manifest, &manifest());
    assert_eq!(decoded.info().definition_count, 1);
    assert_eq!(decoded.info().node_count, 1);
    assert!(!decoded.info().evaluation_performed);
    let mut target = Graph::default();
    let first = decoded.import_into(&mut target).unwrap();
    let second = decoded.import_into(&mut target).unwrap();
    assert_ne!(first, second);
    cluster::instantiate(&mut target, first, [0., 0.]).unwrap();
    target.validate().unwrap();
}

#[test]
fn metadata_limits_and_version_identity() {
    for id in ["../x", "A.name", "a..b", "a/b.c", "a.b-"] {
        let mut bad = manifest();
        bad.id = id.into();
        assert!(bad.validate().is_err());
    }
    for version in ["1.2", "01.2.3", "1.2.3-beta", "1.2.3+foo", "1.+2.3"] {
        let mut bad = manifest();
        bad.version = version.into();
        assert!(bad.validate().is_err());
    }
    let mut bad = manifest();
    bad.license.clear();
    assert!(bad.validate().is_err());
    let mut bad = manifest();
    bad.description = "x".repeat(4097);
    assert!(bad.validate().is_err());
}

#[test]
fn malformed_future_and_oversized_envelopes_fail_closed() {
    let package = Package::from_part(manifest(), part().as_bytes()).unwrap();
    assert!(Package::decode(b"{").is_err());
    assert!(Package::decode(&vec![b' '; MAX_PACKAGE_BYTES + 1]).is_err());
    let mut json: serde_json::Value = serde_json::from_slice(&package.encode().unwrap()).unwrap();
    json["schema_version"] = 2.into();
    assert!(Package::decode(&serde_json::to_vec(&json).unwrap()).is_err());
    json["schema_version"] = 1.into();
    json["native_plugin"] = "untrusted.dll".into();
    assert!(Package::decode(&serde_json::to_vec(&json).unwrap()).is_err());
}

#[test]
fn rejects_unknown_nodes_and_recursive_definitions() {
    let source = part();
    let mut raw: serde_json::Value = serde_json::from_str(&source).unwrap();
    raw["clusters"][0]["graph"]["nodes"][0]["kind"] = "future_unknown_node".into();
    assert!(Package::from_part(manifest(), &serde_json::to_vec(&raw).unwrap()).is_err());

    let mut parsed = cluster::parse_part(source.as_bytes()).unwrap();
    let root = parsed.root;
    let node = &mut parsed.clusters[0].graph.nodes[0];
    node.kind = NodeKind::Cluster;
    node.cluster = Some(root);
    node.params.clear();
    parsed.clusters[0].inputs.clear();
    let bytes = serde_json::to_vec(&parsed).unwrap();
    assert!(cluster::parse_part(&bytes).is_err());
    assert!(Package::from_part(manifest(), &bytes).is_err());
}

#[test]
fn denies_script_even_when_disabled_and_in_unused_definition() {
    let source = part();
    let mut parsed = cluster::parse_part(source.as_bytes()).unwrap();
    let mut unused = parsed.clusters[0].clone();
    unused.id = parsed.root + 1;
    unused.inputs.clear();
    let node = &mut unused.graph.nodes[0];
    node.kind = NodeKind::Script;
    node.params.clear();
    node.off = true;
    node.script = "42.0".into();
    parsed.clusters.push(unused);
    let bytes = serde_json::to_vec(&parsed).unwrap();
    // Prove the policy rejects an otherwise structurally readable part.
    assert!(cluster::parse_part(&bytes).is_ok());
    assert!(Package::from_part(manifest(), &bytes).is_err());
}
