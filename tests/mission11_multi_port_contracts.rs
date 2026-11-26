//! Multi-port regression fixtures. Native NODES contracts, not a Grasshopper oracle.
use morphlet::engine::Evaluator;
use morphlet::model::{Graph, NodeId, NodeKind, Value};
use morphlet::multi_port::{PortRef, MAX_WIRES_PER_INPUT};
use morphlet::persistence::{decode_project, History, Project, SCHEMA_VERSION};

fn add(g: &mut Graph, kind: NodeKind) -> NodeId {
    g.add(kind, [0.,0.]).unwrap()
}
fn set(g: &mut Graph, id: NodeId, key: &str, v: f64) {
    g.node_mut(id).unwrap().params.insert(key.into(),v);
}
fn port(node: NodeId, port: u32) -> PortRef { PortRef { node,port } }
fn numbers(e: &morphlet::engine::Evaluation, source: PortRef) -> Vec<f64> {
    e.output(source).unwrap().as_ref().unwrap().branches.values().flatten()
        .map(|v| v.number().unwrap()).collect()
}
fn series(g: &mut Graph, start: f64, step: f64, count: f64) -> NodeId {
    let n = add(g, NodeKind::Series);
    set(g,n,"start",start); set(g,n,"step",step); set(g,n,"count",count);
    n
}

#[test]
fn distinct_xyz_ports_reach_distinct_consumers_and_cache() {
    let mut g = Graph::default();
    let p = add(&mut g,NodeKind::Point);
    for (k,v) in [("x",3.),("y",7.),("z",-2.)] { set(&mut g,p,k,v); }
    let split = add(&mut g,NodeKind::DeconstructPoint);
    g.connect(p,split,"point").unwrap();
    let sum = add(&mut g,NodeKind::Add);
    g.connect_port(port(split,1),sum,"a",false).unwrap();
    g.connect_port(port(split,2),sum,"b",false).unwrap();
    let mut e = Evaluator::default();
    let result = e.evaluate(&g,&||false);
    assert_eq!(numbers(&result,port(split,0)),vec![3.]);
    assert_eq!(numbers(&result,port(split,1)),vec![7.]);
    assert_eq!(numbers(&result,port(split,2)),vec![-2.]);
    assert_eq!(numbers(&result,port(sum,0)),vec![5.]);
    let warm=e.evaluate(&g,&||false);
    assert_eq!((warm.computed,warm.cached),(0,3));
    g.connect_port(port(split,0),sum,"a",false).unwrap();
    let updated=e.evaluate(&g,&||false);
    assert_eq!(numbers(&updated,port(sum,0)),vec![1.]);
    assert_eq!((updated.computed,updated.cached),(1,2));
}

#[test]
fn dispatch_preserves_partition_order_and_can_recombine_multiple_wires() {
    let mut g=Graph::default();
    let list=series(&mut g,10.,1.,5.);
    let pattern=series(&mut g,1.,-1.,2.);
    let dispatch=add(&mut g,NodeKind::Dispatch);
    g.connect(list,dispatch,"list").unwrap();
    g.connect(pattern,dispatch,"pattern").unwrap();
    let merged=add(&mut g,NodeKind::StreamMerge);
    g.connect_port(port(dispatch,1),merged,"in0",false).unwrap();
    g.connect_port(port(dispatch,0),merged,"in0",true).unwrap();
    let e=Evaluator::default().evaluate(&g,&||false);
    assert_eq!(numbers(&e,port(dispatch,0)),vec![10.,12.,14.]);
    assert_eq!(numbers(&e,port(dispatch,1)),vec![11.,13.]);
    assert_eq!(numbers(&e,port(merged,0)),vec![11.,13.,10.,12.,14.]);
    g.disconnect_port(port(dispatch,1),merged,"in0").unwrap();
    assert_eq!(g.node(merged).unwrap().sources("in0"),vec![port(dispatch,0)]);
}

#[test]
fn gate_ids_survive_reorder_save_reload_and_undo_redo() {
    let mut g=Graph::default();
    let n=add(&mut g,NodeKind::Number);
    set(&mut g,n,"value",42.);
    let gate=add(&mut g,NodeKind::StreamGate);
    g.connect(n,gate,"stream").unwrap();
    let id=g.add_variable_output(gate).unwrap();
    assert_eq!(id,2);
    let sink=add(&mut g,NodeKind::Add);
    set(&mut g,sink,"b",0.);
    g.connect_port(port(gate,id),sink,"a",false).unwrap();
    assert!(g.remove_variable_output(gate,id).is_err());
    g.reorder_outputs(gate,vec![id,0,1]).unwrap();
    g.rename_output(gate,id,"選択先".into()).unwrap();
    let mut project=Project::new("ports",g);
    let decoded=decode_project(&serde_json::to_vec(&project).unwrap()).unwrap();
    assert_eq!(decoded,project);
    assert_eq!(numbers(&Evaluator::default().evaluate(&decoded.graph,&||false),port(sink,0)),vec![42.]);
    let mut h=History::default();
    h.checkpoint(&project);
    project.graph.disconnect_port(port(gate,id),sink,"a").unwrap();
    project.graph.remove_variable_output(gate,id).unwrap();
    let later=project.graph.add_variable_output(gate).unwrap();
    assert!(later>id);
    assert!(h.undo(&mut project));
    assert_eq!(project,decoded);
    assert!(h.redo(&mut project));
    assert!(project.graph.node(gate).unwrap().output_port(later).is_some());
    assert!(project.graph.node(gate).unwrap().output_port(id).is_none());
}

#[test]
fn legacy_node_ids_migrate_and_bad_ports_never_redirect_to_primary() {
    let mut g=Graph::default();
    let n=add(&mut g,NodeKind::Number);
    let a=add(&mut g,NodeKind::Add);
    g.connect(n,a,"a").unwrap();
    let mut raw=serde_json::to_value(Project::new("legacy",g.clone())).unwrap();
    raw["schema_version"]=1.into();
    let p=decode_project(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert_eq!(p.schema_version,SCHEMA_VERSION);
    assert_eq!(p.graph.node(a).unwrap().sources("a"),vec![port(n,0)]);
    let before=g.clone();
    assert!(g.connect_port(port(n,999),a,"a",false).is_err());
    assert_eq!(g,before);
    let target=g.node_mut(a).unwrap();
    target.inputs.remove("a");
    target.port_links.insert("a".into(),vec![port(n,999)]);
    assert!(decode_project(&serde_json::to_vec(&Project::new("bad",g)).unwrap()).is_err());
}

#[test]
fn cycles_types_and_wire_limits_are_transactional() {
    let mut g=Graph::default();
    let a=add(&mut g,NodeKind::Add);
    let b=add(&mut g,NodeKind::Add);
    let p=add(&mut g,NodeKind::Point);
    g.connect(a,b,"a").unwrap();
    let old=g.clone();
    assert!(g.connect_port(port(b,0),a,"a",true).is_err());
    assert_eq!(g,old);
    assert!(g.connect_port(port(p,0),a,"a",false).is_err());
    assert_eq!(g,old);
    for _ in 0..MAX_WIRES_PER_INPUT {
        let n=add(&mut g,NodeKind::Number);
        g.connect_port(port(n,0),a,"b",true).unwrap();
    }
    let extra=add(&mut g,NodeKind::Number);
    let old=g.clone();
    assert!(g.connect_port(port(extra,0),a,"b",true).is_err());
    assert_eq!(g,old);
}

#[test]
fn secondary_results_are_empty_when_disabled_and_removed_when_reconfigured() {
    let mut g=Graph::default();
    let n=add(&mut g,NodeKind::Point);
    let split=add(&mut g,NodeKind::DeconstructPoint);
    g.connect(n,split,"point").unwrap();
    let mut engine=Evaluator::default();
    assert!(engine.evaluate(&g,&||true).cancelled);
    assert_eq!(numbers(&engine.evaluate(&g,&||false),port(split,2)),vec![0.]);
    g.node_mut(split).unwrap().off=true;
    let off=engine.evaluate(&g,&||false);
    assert!(off.output(port(split,2)).unwrap().as_ref().unwrap().branches.is_empty());
    g.remove(split);
    assert!(engine.evaluate(&g,&||false).output(port(split,2)).is_none());
    // Ensure the test exercises the concrete geometry value, not a dummy scalar.
    assert!(matches!(engine.evaluate(&g,&||false).outputs[&n].as_ref().unwrap().branches.values().flatten().next(), Some(Value::Point(_))));
}
