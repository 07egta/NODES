//! Emit a reviewable inventory without launching the GUI or linking Rhino.
//! cargo run --locked --example mission10_inventory > mission10-inventory.json
use morphlet::gh_nodes::is_list_kind;
use morphlet::model::NodeKind;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const CATALOG: &str = include_str!("../docs/compatibility/mission10_catalog.json");
const WORKFLOWS: &str = include_str!("../docs/compatibility/mission10_workflows.json");

fn native_nodes() -> Vec<Value> {
    NodeKind::all().iter().copied()
        .chain(std::iter::once(NodeKind::Cluster))
        .map(|kind| {
            let spec = kind.spec();
            let eval_mode = match kind {
                NodeKind::Graft | NodeKind::Flatten | NodeKind::Output => "whole_tree",
                NodeKind::Script => "script_flattened_inputs",
                NodeKind::Cluster => "definition_specific",
                NodeKind::ImportMesh => "file_import_first_settings",
                NodeKind::Merge | NodeKind::Loft => "branch_list",
                k if is_list_kind(k) => "branch_list",
                _ => "item_matching",
            };
            json!({
                "id": serde_json::to_value(kind).unwrap(),
                "label": spec.label,
                "category": spec.category,
                "registered_in_picker": kind != NodeKind::Cluster,
                "native_eval_mode": eval_mode,
                "inputs": spec.inputs.iter().map(|p| json!({
                    "key": p.key,
                    "label": p.label,
                    "type": p.kind,
                    "default": p.default,
                    "min": p.default.map(|_| p.min),
                    "max": p.default.map(|_| p.max),
                    "step": p.default.map(|_| p.step),
                    "integer": p.integer,
                })).collect::<Vec<_>>(),
                "outputs": [{"type": spec.output}],
                "compatibility_evidence": "not inferred from registration"
            })
        })
        .collect()
}

fn count_status(rows: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for name in ["VERIFIED", "PARTIAL", "APPROXIMATE", "IMPLEMENTED_UNVERIFIED",
                 "MISSING", "BLOCKED", "UNKNOWN"] {
        counts.insert(name.to_string(), 0usize);
    }
    for row in rows {
        *counts.get_mut(row["status"].as_str().expect("status string")).expect("known status") += 1;
    }
    counts
}

fn validate(catalog: &Value, workflows: &Value, native: &[Value]) {
    assert_eq!(catalog["schema_version"].as_u64(), Some(1));
    assert_eq!(workflows["schema_version"].as_u64(), Some(1));
    let ids: BTreeSet<_> = native.iter().map(|n| n["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), native.len(), "duplicate native IDs");
    for version in ["gh1", "gh2"] {
        let mut names = BTreeSet::new();
        let mut public_ids = BTreeSet::new();
        let rows = catalog[version].as_array().expect("version-separated rows");
        count_status(rows);
        for row in rows {
            assert_eq!(row["version"].as_str().unwrap().to_ascii_lowercase(), version);
            let name = row["name"].as_str().expect("name");
            let category = row["category"].as_str().unwrap_or("");
            let sub = row["subcategory"].as_str().unwrap_or("");
            assert!(names.insert((category, sub, name)), "duplicate {version} row: {name}");
            assert!(!row["limits"].as_str().unwrap().is_empty(), "missing limitations: {name}");
            for node in row["nodes_ids"].as_array().expect("native mapping list") {
                assert!(ids.contains(node.as_str().unwrap()), "unknown native node for {name}: {node}");
            }
            if let Some(id) = row["public_id"].as_str() {
                assert!(public_ids.insert(id), "duplicate reference ID within {version}: {id}");
            }
            if row["status"] == "VERIFIED" {
                assert!(row["public_id"].as_str().is_some(), "no reference identity: {name}");
                assert!(!row["nodes_ids"].as_array().unwrap().is_empty(), "no implementation: {name}");
                assert_eq!(row["test_status"], "PASSED", "no passing test: {name}");
                assert!(row["oracle_evidence"].as_str().is_some_and(|s| !s.is_empty()),
                        "no GH expected-result evidence: {name}");
                assert!(row["official_reference_url"].as_str().is_some(),
                        "no component-specific reference: {name}");
            }
            if row["status"] == "MISSING" || row["status"] == "BLOCKED" {
                assert!(row["nodes_ids"].as_array().unwrap().is_empty(), "unexpected mapping: {name}");
            }
        }
    }
    let cases = workflows["cases"].as_array().expect("workflow cases");
    assert_eq!(cases.len(), 20);
    let mut case_ids = BTreeSet::new();
    for (i, case) in cases.iter().enumerate() {
        assert_eq!(case["number"].as_u64(), Some(i as u64 + 1));
        assert!(case_ids.insert(case["id"].as_str().unwrap()));
        assert!(matches!(case["execution"].as_str(), Some("NOT_RUN" | "PASSED" | "FAILED")));
        if case["execution"] == "PASSED" {
            assert!(case["execution_evidence"].as_str().is_some_and(|s| !s.is_empty()));
        }
        if case["gh_oracle"] == "PASSED" {
            assert!(case["oracle_evidence"].as_str().is_some_and(|s| !s.is_empty()));
        }
    }
}

fn main() {
    let catalog: Value = serde_json::from_str(CATALOG).expect("valid catalog JSON");
    let workflows: Value = serde_json::from_str(WORKFLOWS).expect("valid workflow JSON");
    let native = native_nodes();
    validate(&catalog, &workflows, &native);
    let listed: u64 = catalog["categories"].as_array().unwrap().iter()
        .map(|c| c["listed_count"].as_u64().unwrap()).sum();
    let report = json!({
        "schema_version": 1,
        "scope": "Native registry and partial GH crosswalk; not test execution or parity certification",
        "catalog_baseline": catalog["baseline"],
        "native_registry_count": native.iter().filter(|n| n["registered_in_picker"] == true).count(),
        "native_special_definition_kinds": 1,
        "native_unique_kind_count": native.len(),
        "third_party_index_listed_count": listed,
        "authoritative_gh1_standard_component_count": null,
        "authoritative_gh2_standard_component_count": null,
        "gh1_row_count": catalog["gh1"].as_array().unwrap().len(),
        "gh2_row_count": catalog["gh2"].as_array().unwrap().len(),
        "gh1_status": count_status(catalog["gh1"].as_array().unwrap()),
        "gh2_status": count_status(catalog["gh2"].as_array().unwrap()),
        "native_nodes": native,
        "catalog": catalog,
        "workflows": workflows
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_references_real_ids_and_keeps_evidence_separate() {
        let catalog: Value = serde_json::from_str(CATALOG).unwrap();
        let workflows: Value = serde_json::from_str(WORKFLOWS).unwrap();
        validate(&catalog, &workflows, &native_nodes());
    }

    #[test]
    #[should_panic(expected = "no reference identity")]
    fn catalog_rejects_promotion_based_on_name_alone() {
        let mut catalog: Value = serde_json::from_str(CATALOG).unwrap();
        let workflows: Value = serde_json::from_str(WORKFLOWS).unwrap();
        catalog["gh1"][0]["status"] = json!("VERIFIED");
        validate(&catalog, &workflows, &native_nodes());
    }

    #[test]
    #[should_panic(expected = "unknown native node")]
    fn catalog_rejects_fictitious_node_id() {
        let mut catalog: Value = serde_json::from_str(CATALOG).unwrap();
        let workflows: Value = serde_json::from_str(WORKFLOWS).unwrap();
        catalog["gh1"][0]["nodes_ids"] = json!(["not_a_native_node"]);
        validate(&catalog, &workflows, &native_nodes());
    }
}
