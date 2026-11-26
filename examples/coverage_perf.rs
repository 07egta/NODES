//! Opt-in, headless performance measurement (not a GUI/startup benchmark).
//! cargo run --release --locked --example coverage_perf
use morphlet::engine::Evaluator;
use morphlet::model::{Graph, MAX_NODES, NodeId, NodeKind};
use serde_json::{Value, json};
use std::time::Instant;

fn make_chain(n: usize) -> Result<(Graph, NodeId), String> {
    let mut graph = Graph::default();
    let slider = graph.add(NodeKind::Number, [0., 0.])?;
    let mut previous = slider;
    for _ in 1..n {
        let next = graph.add(NodeKind::Add, [0., 0.])?;
        graph.node_mut(next).unwrap().params.insert("b".into(), 1.);
        // No per-wire full graph scan while constructing the benchmark fixture.
        graph
            .node_mut(next)
            .unwrap()
            .inputs
            .insert("a".into(), previous);
        previous = next;
    }
    graph.validate()?;
    Ok((graph, slider))
}

fn sample(n: usize) -> Value {
    let build_started = Instant::now();
    let (mut graph, slider) = match make_chain(n) {
        Ok(fixture) => fixture,
        Err(e) => return json!({"requested_nodes":n,"status":"REJECTED","reason":e}),
    };
    let fixture_ms = build_started.elapsed().as_secs_f64() * 1000.;
    let last = graph.nodes.last().unwrap().id;
    let mut engine = Evaluator::default();
    let cold = engine.evaluate(&graph, &|| false);
    assert!(!cold.cancelled && cold.outputs.values().all(Result::is_ok));
    let warm = engine.evaluate(&graph, &|| false);
    assert!(!warm.cancelled && warm.outputs.values().all(Result::is_ok));
    assert_eq!(warm.cached, n);
    let mut times = Vec::new();
    for step in 0..50 {
        let number = step as f64 + 2.;
        graph
            .node_mut(slider)
            .unwrap()
            .params
            .insert("value".into(), number);
        let start = Instant::now();
        let result = engine.evaluate(&graph, &|| false);
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        assert!(!result.cancelled && result.outputs.values().all(Result::is_ok));
        let final_value = result.outputs[&last]
            .as_ref()
            .unwrap()
            .branches
            .values()
            .flatten()
            .next()
            .unwrap()
            .number()
            .unwrap();
        assert_eq!(final_value, number + n as f64 - 1.);
        times.push(elapsed);
    }
    times.sort_by(f64::total_cmp);
    json!({"requested_nodes":n,"status":"MEASURED","fixture_ms":fixture_ms,
        "cold_evaluate_ms":cold.elapsed_ms,"warm_evaluate_ms":warm.elapsed_ms,
        "warm_cache_hits":warm.cached,"slider_samples":times.len(),
        "slider_p50_ms":times[24],"slider_p95_ms":times[47],"slider_max_ms":times[49]})
}

fn main() {
    let counts = [10, 100, 1_000, MAX_NODES, 10_000];
    let samples: Vec<_> = counts.into_iter().map(sample).collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "measurement":"headless evaluator only; not frame, GUI, process startup or memory",
            "platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,
            "max_nodes":MAX_NODES,"samples":samples
        }))
        .unwrap()
    );
}
