//! Engine-only benchmark. Run with cargo run --release --locked --example mission10_benchmark.
//! No GUI, process-memory, startup, or display-latency measurements are inferred.
use morphlet::engine::Evaluator;
use morphlet::model::{Graph, NodeId, NodeKind, Value, MAX_NODES};
use serde_json::{json, Value as Json};
use std::time::Instant;

fn add(g: &mut Graph, kind: NodeKind) -> NodeId {
    g.add(kind, [0., 0.]).expect("benchmark graph")
}
fn set(g: &mut Graph, id: NodeId, key: &str, val: f64) {
    g.node_mut(id).unwrap().params.insert(key.into(), val);
}
fn series(g: &mut Graph, count: usize) -> NodeId {
    let id = add(g, NodeKind::Series);
    set(g, id, "count", count as f64);
    set(g, id, "step", 1.);
    id
}
fn summarize(mut samples: Vec<f64>) -> Json {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    let median = (samples[(n - 1) / 2] + samples[n / 2]) / 2.;
    let p95 = samples[(n * 95).div_ceil(100) - 1];
    json!({"samples":n, "min_ms":samples[0], "median_ms":median, "p95_ms":p95, "max_ms":samples[n-1]})
}
fn timing(g: &Graph) -> Json {
    let mut cold = Vec::new();
    let mut warm = Vec::new();
    for _ in 0..7 {
        let mut evaluator = Evaluator::default();
        let t = Instant::now();
        let output = evaluator.evaluate(g, &|| false);
        cold.push(t.elapsed().as_secs_f64()*1000.);
        assert_eq!(output.outputs.len(), g.nodes.len());
        assert!(output.outputs.values().all(Result::is_ok), "{:?}", output.outputs.values().find(|r|r.is_err()));
        let t = Instant::now();
        let output = evaluator.evaluate(g, &|| false);
        warm.push(t.elapsed().as_secs_f64()*1000.);
        assert_eq!(output.outputs.len(), g.nodes.len());
        assert!(output.outputs.values().all(Result::is_ok));
    }
    json!({"graph_nodes":g.nodes.len(),"fresh_evaluator":summarize(cold),"same_evaluator":summarize(warm)})
}
fn fanout(count: usize) -> (Graph, NodeId) {
    let mut g = Graph::default();
    let source = add(&mut g, NodeKind::Number);
    for _ in 1..count {
        let n = add(&mut g, NodeKind::Add);
        g.connect(source, n, "a").unwrap();
    }
    (g, source)
}
fn main() {
    let mut cases = Vec::new();
    for size in [10, 100, 1000] {
        let (g, _) = fanout(size);
        cases.push(json!({"case":"fanout","requested_nodes":size,"timing":timing(&g)}));
    }
    // The 10,000-node request is not a successful benchmark if capped by validation.
    let mut capped = Graph::default();
    let t = Instant::now();
    let mut rejection = None;
    for _ in 0..10000 {
        if let Err(err) = capped.add(NodeKind::Number, [0.,0.]) {
            rejection = Some(err);
            break;
        }
    }
    assert_eq!(capped.nodes.len(), MAX_NODES.min(10000));
    cases.push(json!({"case":"node_capacity","requested_nodes":10000,
        "accepted_nodes":capped.nodes.len(),"rejection":rejection,
        "construction_ms":t.elapsed().as_secs_f64()*1000.,
        "evaluation_ms":null}));
    // 100,000 point values across multiple outputs, NOT one unbounded point list.
    let mut points = Graph::default();
    let full = series(&mut points,512);
    let remainder = series(&mut points,160);
    let mut targets = Vec::new();
    for idx in 0..196 {
        let p = add(&mut points,NodeKind::Point);
        points.connect(if idx == 195 {remainder} else {full},p,"x").unwrap();
        targets.push(p);
    }
    let out = Evaluator::default().evaluate(&points,&||false);
    let count:usize = targets.iter().map(|id|{
        let tree = out.outputs[id].as_ref().unwrap();
        assert!(tree.branches.values().flatten().all(|v|matches!(v,Value::Point(_))));
        tree.item_count()
    }).sum();
    assert_eq!(count,100000);
    cases.push(json!({"case":"distributed_points","point_count":count,"single_output":false,"timing":timing(&points)}));
    let mut g = Graph::default();
    let s = series(&mut g,512);
    let graft = add(&mut g,NodeKind::Graft);
    g.connect(s,graft,"geometry").unwrap();
    let branch_series = series(&mut g,16);
    g.connect(graft,branch_series,"start").unwrap();
    let out = Evaluator::default().evaluate(&g,&||false);
    let tree = out.outputs[&branch_series].as_ref().unwrap();
    assert_eq!(tree.branches.len(),512);
    assert_eq!(tree.item_count(),8192);
    cases.push(json!({"case":"data_tree","branches":512,"items":8192,"timing":timing(&g)}));
    let mut mesh = Graph::default();
    let sphere = add(&mut mesh,NodeKind::Sphere);
    set(&mut mesh,sphere,"segments",192.);
    set(&mut mesh,sphere,"rings",96.);
    let out = Evaluator::default().evaluate(&mesh,&||false);
    let tree = out.outputs[&sphere].as_ref().unwrap();
    let (vertices,triangles) = match tree.branches.values().flatten().next().unwrap() {
        Value::Mesh(m)=>(m.vertices.len(),m.triangles.len()),
        _=>panic!("sphere did not emit mesh"),
    };
    cases.push(json!({"case":"sphere_mesh","vertices":vertices,"triangles":triangles,"timing":timing(&mesh)}));
    let (mut sliders,source)=fanout(100);
    let mut evaluator=Evaluator::default();
    let mut changes=Vec::new();
    for i in 0..100 {
        set(&mut sliders,source,"value",i as f64);
        let t=Instant::now();
        let output=evaluator.evaluate(&sliders,&||false);
        changes.push(t.elapsed().as_secs_f64()*1000.);
        assert!(output.outputs.values().all(Result::is_ok));
    }
    cases.push(json!({"case":"slider_changes","graph_nodes":100,"evaluation_only":summarize(changes)}));
    println!("{}",serde_json::to_string_pretty(&json!({
        "schema_version":1,
        "os":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "debug_assertions":cfg!(debug_assertions),
        "scope":"Engine evaluation only, not GUI responsiveness",
        "startup_ms":null,"memory_bytes":null,"frame_ms":null,"input_to_display_ms":null,
        "cases":cases
    })).unwrap());
}
