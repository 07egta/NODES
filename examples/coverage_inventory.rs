//! Generate the actual NODES inventory from its compiled registry.
//! cargo run --locked --example coverage_inventory > nodes_inventory.json
//! This inventories registrations; it does not certify evaluator or GH compatibility.
use morphlet::model::NodeKind;
use serde_json::{json, Value};

fn main() {
    let rows: Vec<Value> = NodeKind::all().iter().copied()
        .chain([NodeKind::Cluster])
        .map(|kind| {
            let spec = kind.spec();
            let inputs: Vec<Value> = spec.inputs.iter().map(|p| json!({
                "key": p.key,
                "label": p.label,
                "type": p.kind,
                "default": p.default,
                "min": p.default.map(|_| p.min),
                "max": p.default.map(|_| p.max),
                "step": p.default.map(|_| p.step),
                "integer": p.integer
            })).collect();
            json!({
                "nodes_id": kind,
                "label": spec.label,
                "category": spec.category,
                "inputs": inputs,
                "output": spec.output,
                "registration": if kind == NodeKind::Cluster { "definition_only" } else { "picker" },
                "compatibility": "NOT_INFERRED"
            })
        }).collect();
    println!("{}", serde_json::to_string_pretty(&json!({
        "schema_version": 1,
        "picker_count": NodeKind::all().len(),
        "registered_count": rows.len(),
        "nodes": rows
    })).unwrap());
}
