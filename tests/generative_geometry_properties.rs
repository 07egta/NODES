//! End-to-end geometric contracts beyond the individual kernel unit tests.
use morphlet::{
    engine::Evaluator,
    model::{Curve, Graph, Mesh, NodeId, NodeKind, Tree, Value},
};
use std::collections::BTreeMap;

fn add(g:&mut Graph, kind:NodeKind, params:&[(&str,f64)])->NodeId {
    let id=g.add(kind,[0.,0.]).unwrap();
    let n=g.node_mut(id).unwrap();
    for &(k,v) in params {n.params.insert(k.into(),v);}
    id
}
fn values(t:&Tree)->impl Iterator<Item=&Value> {
    t.branches.values().flatten()
}
fn xy_area(c:&Curve)->f64 {
    let p=&c.points;
    p.iter().zip(p.iter().cycle().skip(1)).map(|(a,b)|a[0]*b[1]-a[1]*b[0]).sum::<f64>()*0.5
}
fn orient(a:[f64;3],b:[f64;3],p:[f64;3])->f64 {
    (b[0]-a[0])*(p[1]-a[1])-(b[1]-a[1])*(p[0]-a[0])
}
fn dist2(a:[f64;3],b:[f64;3])->f64 {
    (a[0]-b[0]).powi(2)+(a[1]-b[1]).powi(2)
}
fn mesh_area(m:&Mesh)->f64 {
    m.triangles.iter().map(|t|{
        let [a,b,c]=t.map(|i|m.vertices[i as usize]);
        let area=orient(a,b,c)*0.5;
        assert!(area>0.,"triangle must be nondegenerate and counterclockwise");
        area
    }).sum()
}

#[test]
fn seeded_partition_and_surface_sampling_have_geometric_invariants() {
    let mut graph=Graph::default();
    let sites=add(&mut graph,NodeKind::Populate,&[("count",48.),("width",120.),("depth",80.),("height",0.),("seed",11.)]);
    let vor=add(&mut graph,NodeKind::Voronoi2d,&[("width",120.),("height",80.)]);
    let hull=add(&mut graph,NodeKind::ConvexHull2d,&[]);
    let triang=add(&mut graph,NodeKind::Delaunay2d,&[]);
    let scatter=add(&mut graph,NodeKind::PopulateMesh,&[("count",100.),("seed",7.)]);
    for id in [vor,hull,triang] {graph.connect(sites,id,"points").unwrap();}
    graph.connect(triang,scatter,"mesh").unwrap();
    graph.validate().unwrap();

    // Each seed is an independent evaluation. This guards both parameter handling and
    // geometric consistency, without relying on a frozen triangulation diagonal.
    for seed in [11.,23.,47.] {
        graph.node_mut(sites).unwrap().params.insert("seed".into(),seed);
        let mut eval=Evaluator::default();
        let r=eval.evaluate(&graph,&||false);
        let tree=|id|r.outputs[&id].as_ref().unwrap_or_else(|e|panic!("seed {seed}, node {id}: {e}"));
        let p:Vec<_>=values(tree(sites)).map(|v|{let Value::Point(p)=v else{panic!("point")};*p}).collect();
        let cells:Vec<_>=values(tree(vor)).map(|v|{let Value::Curve(c)=v else{panic!("cell")};c}).collect();
        assert_eq!(cells.len(),p.len());
        let mut total=0.;
        for (site,c) in p.iter().zip(cells) {
            assert!(c.closed && c.points.len()>=3);
            let area=xy_area(c);
            assert!(area>0.);
            total+=area;
            for &q in &c.points {
                assert!(q[0].abs()<=60.+1e-7 && q[1].abs()<=40.+1e-7 && q[2]==0.);
                let own=dist2(q,*site);
                for &other in &p {
                    assert!(own<=dist2(q,other)+1e-6,"a Voronoi vertex is assigned to a farther site");
                }
            }
        }
        assert!((total-9600.).abs()<1e-6,"Voronoi partition lost area");
        let Value::Curve(h)=values(tree(hull)).next().unwrap() else{panic!("hull")};
        let Value::Mesh(m)=values(tree(triang)).next().unwrap() else{panic!("mesh")};
        assert!((mesh_area(m)-xy_area(h)).abs()<1e-6,"Delaunay does not cover convex hull");
        assert_eq!(m.vertices.len(),p.len());
        let mut edges=BTreeMap::<(u32,u32),usize>::new();
        for t in &m.triangles {
            for (a,b) in [(t[0],t[1]),(t[1],t[2]),(t[2],t[0])] {
                *edges.entry((a.min(b),a.max(b))).or_default()+=1;
            }
        }
        assert!(edges.values().all(|n|*n==1||*n==2));
        assert_eq!(m.vertices.len()+m.triangles.len(),edges.len()+1);
        assert_eq!(tree(scatter).item_count(),100);
        for v in values(tree(scatter)) {
            let Value::Point(q)=v else{panic!("sample point")};
            assert!(q[2].abs()<1e-12);
            for (a,b) in h.points.iter().zip(h.points.iter().cycle().skip(1)){
                assert!(orient(*a,*b,*q)>=-1e-7,"surface sample outside hull");
            }
        }
        let again=Evaluator::default().evaluate(&graph,&||false);
        for id in [sites,vor,hull,triang,scatter] {
            assert_eq!(r.outputs[&id],again.outputs[&id],"seeded output is not reproducible");
        }
    }
}

#[test]
fn full_hex_panels_share_exactly_five_edges_in_two_by_two_grid(){
    let mut g=Graph::default();
    let id=add(&mut g,NodeKind::HexPanels,&[("nx",2.),("ny",2.),("size",10.),("fill",1.)]);
    let r=Evaluator::default().evaluate(&g,&||false);
    let tree=r.outputs[&id].as_ref().unwrap();
    assert_eq!(tree.item_count(),4);
    let quant=|p:&[f64;3]|p.map(|x|(x*1e6).round() as i64);
    let mut edges=BTreeMap::<([i64;3],[i64;3]),usize>::new();
    for v in values(tree) {
        let Value::Curve(c)=v else{panic!("curve")};
        assert!(c.closed && c.points.len()==6);
        assert!((xy_area(c)-150.*3f64.sqrt()).abs()<1e-9);
        for (a,b) in c.points.iter().zip(c.points.iter().cycle().skip(1)){
            let (a,b)=(quant(a),quant(b));
            *edges.entry((a.min(b),a.max(b))).or_default()+=1;
        }
    }
    assert_eq!(edges.values().filter(|&&n|n==2).count(),5);
    assert_eq!(edges.values().filter(|&&n|n==1).count(),14);
    assert!(edges.values().all(|&n|n<=2));
}
