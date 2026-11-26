//! Export the compiled registry without implying evaluator/GH verification.
//! cargo run --locked --example coverage_inventory -- --source-revision <git-sha>
//! Regenerate after integration: counts and defaults are never copied by hand.
use morphlet::model::NodeKind;
use serde_json::{Value, json};

/// A deterministic change detector for metadata, not a security hash or a hash
/// of the evaluator's implementation. Git source_revision identifies the code.
fn fingerprint(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("fnv1a64:{hash:016x}")
}

fn inventory(source_revision: Option<&str>) -> Value {
    let rows: Vec<Value> = NodeKind::all()
        .iter()
        .copied()
        .chain([NodeKind::Cluster])
        .map(|kind| {
            let spec = kind.spec();
            let inputs: Vec<Value> = spec
                .inputs
                .iter()
                .enumerate()
                .map(|(index, p)| {
                    json!({
                        "index": index,
                        "key": p.key,
                        "label": p.label,
                        "type": p.kind,
                        "default": p.default,
                        "required_connection": p.default.is_none(),
                        "min": p.default.map(|_| p.min),
                        "max": p.default.map(|_| p.max),
                        "step": p.default.map(|_| p.step),
                        "integer": p.integer,
                        // NodeSpec on the baseline has no access declaration.
                        // A type or a default does not establish Item/List/Tree.
                        "access": null,
                        "access_status": "NOT_DECLARED_IN_NODE_SPEC"
                    })
                })
                .collect();
            let mut row = json!({
                "nodes_id": kind,
                "label": spec.label,
                "category": spec.category,
                "description": spec.description,
                "search_keywords": spec.keywords,
                "inputs": inputs,
                "output": spec.output,
                "outputs": [{
                    "index": 0,
                    "key": null,
                    "type": spec.output,
                    "connection_encoding": "source_node_id"
                }],
                "registration": if kind == NodeKind::Cluster { "definition_only" } else { "picker" },
                "evaluation_status": "NOT_INFERRED",
                "compatibility": "NOT_INFERRED"
            });
            row["spec_fingerprint"] = json!(fingerprint(row.to_string().as_bytes()));
            row
        })
        .collect();
    json!({
        "schema_version": 2,
        "source_revision": source_revision,
        "source_revision_provenance": "caller_supplied; does not assert a clean worktree",
        "generator": "examples/coverage_inventory.rs",
        "fingerprint_scope": "compiled registration metadata only; not evaluator behavior or GH compatibility",
        "picker_count": NodeKind::all().len(),
        "registered_count": rows.len(),
        "verified_count": null,
        "verification_note": "Registry enumeration cannot establish any verified-node count.",
        "nodes": rows
    })
}

fn revision_from_args(args: &[String]) -> Result<Option<&str>, String> {
    if args.is_empty() {
        return Ok(None);
    }
    if args.len() == 2 && args[0] == "--source-revision" {
        let revision = &args[1];
        if matches!(revision.len(), 40 | 64)
            && revision.as_bytes().iter().all(u8::is_ascii_hexdigit)
        {
            return Ok(Some(revision));
        }
        return Err("--source-revision requires a full 40- or 64-digit hexadecimal Git SHA".into());
    }
    Err("Usage: coverage_inventory [--source-revision <full-git-sha>]".into())
}

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let revision = revision_from_args(&args)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&inventory(revision)).unwrap()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn generated_inventory_has_unique_ids_and_no_implicit_verification() {
        let report = inventory(None);
        assert_eq!(report["picker_count"], NodeKind::all().len());
        assert_eq!(report["registered_count"], NodeKind::all().len() + 1);
        assert!(report["verified_count"].is_null());
        assert!(report["source_revision"].is_null());
        let mut ids = BTreeSet::new();
        for row in report["nodes"].as_array().unwrap() {
            assert!(ids.insert(row["nodes_id"].as_str().unwrap()));
            assert_eq!(row["compatibility"], "NOT_INFERRED");
            assert_eq!(row["evaluation_status"], "NOT_INFERRED");
            assert!(!row["description"].as_str().unwrap().is_empty());
            assert!(
                row["spec_fingerprint"]
                    .as_str()
                    .unwrap()
                    .starts_with("fnv1a64:")
            );
        }
    }

    #[test]
    fn registration_signature_is_stable_and_records_numeric_defaults() {
        assert_eq!(inventory(None), inventory(None));
        let report = inventory(None);
        let add = report["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["nodes_id"] == "add")
            .unwrap();
        let specs = NodeKind::Add.spec().inputs;
        for (port, spec) in add["inputs"].as_array().unwrap().iter().zip(specs) {
            assert_eq!(port["key"], spec.key);
            assert_eq!(port["default"], json!(spec.default));
            assert!(port["access"].is_null());
        }
        assert_ne!(fingerprint(b"default=1"), fingerprint(b"default=2"));
    }

    #[test]
    fn revision_is_explicit_and_invalid_arguments_are_rejected() {
        assert_eq!(revision_from_args(&[]).unwrap(), None);
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let args = vec!["--source-revision".into(), sha.into()];
        assert_eq!(revision_from_args(&args).unwrap(), Some(sha));
        assert_eq!(inventory(Some(sha))["source_revision"], sha);
        assert!(revision_from_args(&["--source-revision".into(), "main".into()]).is_err());
        assert!(revision_from_args(&["--unknown".into()]).is_err());
    }
}
