//! Native curve nodes. Single-output adapters for Morphlet's existing graph and data trees.
//! The geometry module is independent of the renderer. Only explicitly marked preview operations
//! use sampled polylines. Node labels and semantics are original, not GH file compatibility.
use crate::curve_geometry::{self as cg, CurveGeometry, NurbsCurve, Point3};
use crate::engine::{cancelled, num};
use crate::model::*;
use std::{f64::consts::TAU, sync::Arc};
type R<T> = Result<T,String>;
const C:InputSpec=g("curve","曲線",DataType::Curve);
const P:InputSpec=g("points","点のリスト",DataType::Point);
const A:InputSpec=g("a","始点 A",DataType::Point);
const B:InputSpec=g("b","終点 B",DataType::Point);
const D:InputSpec=g("direction","方向 (XYZ)",DataType::Point);
const T:InputSpec=n("t","パラメータ (0–1)",0.5,0.,1.,0.01);
const DEG:InputSpec=i("degree","次数",3.,1.,12.);
const SEG:InputSpec=i("segments","表示分割",96.,4.,1024.);
const RAD:InputSpec=n("radius","半径",30.,0.000001,1000000.,0.5);
const LEN:InputSpec=n("length","長さ",10.,0.,1000000000.,0.1);
const TOL:InputSpec=n("tolerance","許容距離",0.000001,0.000000001,1.,0.0001);
macro_rules! spec {
    ($label:literal,$keys:literal,$desc:literal,$out:ident,[$($input:expr),* $(,)?]) => {{
        static S:NodeSpec=NodeSpec {label:$label,category:"曲線",keywords:$keys,description:$desc,
            inputs:&[$($input),*],output:DataType::$out};
        &S
    }}
}
pub fn is_curve_kind(k:NodeKind)->bool {
    use NodeKind::*;
    matches!(k, CurveLine | CurveLineSdl | CurvePolyline | CurveCircleCnr | CurveCircleThree | CurveArc | CurveArcThree | CurveEllipse | CurveSpiral | CurveBezier | CurveNurbs | CurveWeightedNurbs | CurveInterpolate | CurveLength | CurveEvaluate | CurveEvaluateLength | CurveClosestParameter | CurveClosestPoint | CurveStart | CurveEnd | CurveMiddle | CurveTangent | CurveCurvature | CurveDomain | CurveClosed | CurveDivide | CurveDivideLength | CurveSubcurve | CurveShatter | CurveReverse | CurveControls | CurveWeights | CurveKnots | CurveDegree | CurveInsertKnot | CurveOffsetPreview | CurveFrameX | CurveFrameY | CurveLineLine | CurveLinePlane)
}
pub fn is_list_kind(k:NodeKind)->bool {
    use NodeKind::*;
    matches!(k,CurvePolyline|CurveBezier|CurveNurbs|CurveWeightedNurbs|CurveInterpolate|CurveShatter)
}
pub fn spec(k:NodeKind)->&'static NodeSpec {
    use NodeKind::*;
    match k {
        CurveLine=>spec!("直線","line start end","2点を結ぶ正確な直線。",Curve,[A,B]),
        CurveLineSdl=>spec!("直線・方向・長さ","line sdl","始点、方向（点のXYZをベクトルとして扱う）、長さから直線を作る。",Curve,[A,D,LEN]),
        CurvePolyline=>spec!("ポリライン","polyline","枝内の点を順に結ぶ。閉じる場合は1。",Curve,[P,i("closed","閉じる",0.,0.,1.)]),
        CurveCircleCnr=>spec!("円・中心・法線","circle cnr","中心、法線（XYZ）、半径で正確な有理円を作る。",Curve,[g("center","中心",DataType::Point),g("normal","法線 (XYZ)",DataType::Point),RAD,SEG]),
        CurveCircleThree=>spec!("3点円","circle 3pt","3点を通る円。ほぼ一直線の点は拒否。",Curve,[A,B,g("c","点 C",DataType::Point),SEG]),
        CurveArc=>spec!("円弧 XY","arc","XY平面、中心、開始角と掃引角（度）。正確な有理円弧。",Curve,[g("center","中心",DataType::Point),RAD,n("start","開始角 (度)",0.,-360.,360.,1.),n("sweep","掃引角 (度)",90.,-360.,360.,1.),SEG]),
        CurveArcThree=>spec!("3点円弧","arc 3pt","AからBを経由してCへ向かう円弧。",Curve,[A,B,g("c","終点 C",DataType::Point),SEG]),
        CurveEllipse=>spec!("楕円 XY","ellipse","XY平面の正確な有理楕円。",Curve,[g("center","中心",DataType::Point),n("rx","X 半径",30.,0.000001,1000000.,0.5),n("ry","Y 半径",15.,0.000001,1000000.,0.5),SEG]),
        CurveSpiral=>spec!("平面スパイラル","spiral archimedean","半径が線形に変わる解析的らせん（XY）。",Curve,[g("center","中心",DataType::Point),RAD,n("change","半径変化率",2.,-1.,100.,0.1),n("turns","巻数",3.,-30.,30.,0.1),SEG]),
        CurveBezier=>spec!("Bezier 曲線","bezier control","枝内の制御点からBezier曲線（次数1〜12）。",Curve,[P,SEG]),
        CurveNurbs=>spec!("NURBS 曲線","nurbs b-spline bspline periodic","枝内の制御点、次数。重み1、開いた一様ノットまたは周期一様ノット。",Curve,[P,DEG,i("periodic","周期",0.,0.,1.),SEG]),
        CurveWeightedNurbs=>spec!("有理 NURBS","rational nurbs weighted knots","制御点、正の重み、完全ノット列を枝ごとに入力。ノット数=点数+次数+1。",Curve,[P,g("weights","重み",DataType::Number),g("knots","完全ノット列",DataType::Number),DEG,SEG]),
        CurveInterpolate=>spec!("補間曲線","interpolate curve","枝内の点を通過する弦長パラメータB-spline（最大128点）。",Curve,[P,DEG,SEG]),
        CurveLength=>spec!("曲線長","curve length arc length","導関数の数値積分。折れ線入力は線分長の合計。",Number,[C]),
        CurveEvaluate=>spec!("曲線を評価","evaluate curve point on curve","正規化パラメータ0〜1の点。弧長の割合ではない。",Point,[C,T]),
        CurveEvaluateLength=>spec!("弧長で評価","evaluate length","始点からの弧長（実寸）で点を求める。",Point,[C,LEN]),
        CurveClosestParameter=>spec!("曲線の最近接パラメータ","curve closest parameter","サンプリングと局所最小化による近似値。",Number,[C,g("point","検索点",DataType::Point)]),
        CurveClosestPoint=>spec!("曲線の最近接点","curve closest point","サンプリングと局所最小化による近似点。",Point,[C,g("point","検索点",DataType::Point)]),
        CurveStart=>spec!("曲線の始点","start point end points","曲線の始点。",Point,[C]),
        CurveEnd=>spec!("曲線の終点","end point","曲線の終点。",Point,[C]),
        CurveMiddle=>spec!("曲線の中央・弧長","curve middle midpoint","弧長の半分にある点。",Point,[C]),
        CurveTangent=>spec!("曲線の単位接線","curve tangent","指定パラメータの単位接線をXYZとして返す。",Point,[C,T]),
        CurveCurvature=>spec!("曲率","curve curvature","解析導関数から曲率の大きさを求める。角は片側微分。",Number,[C,T]),
        CurveDomain=>spec!("曲線の元の区間","curve domain","基底の向き付きパラメータ区間の2端を返す。評価ノードの入力は正規化0〜1。",Number,[C]),
        CurveClosed=>spec!("閉曲線判定","closed curve","両端の距離が1e-8以下なら1、それ以外は0。",Number,[C]),
        CurveDivide=>spec!("曲線を等弧長分割","divide curve","弧長で等分し点を返す。閉曲線の終端は重複させない。",Point,[C,i("count","分割数",10.,1.,512.)]),
        CurveDivideLength=>spec!("曲線を長さで分割","divide length","指定弧長ごとの点と開曲線の終点。最大512区間。",Point,[C,n("step","間隔",10.,0.000001,1000000000.,0.1)]),
        CurveSubcurve=>spec!("部分曲線","sub curve trim split","正規化区間を切り出す。基底形状を保持する。",Curve,[C,n("start","開始",0.,0.,1.,0.01),n("end","終了",0.5,0.,1.,0.01),SEG]),
        CurveShatter=>spec!("曲線を分割","shatter split curve","枝内のパラメータで曲線を分割。重複・端点は除く。",Curve,[C,g("parameters","分割パラメータ 0–1",DataType::Number),SEG]),
        CurveReverse=>spec!("曲線の向きを反転","reverse curve","精密曲線の向きを反転。表示の順序も反転する。",Curve,[C]),
        CurveControls=>spec!("曲線の制御点","control points","NURBSの元の制御点列（切り出し前の基底）。",Point,[C]),
        CurveWeights=>spec!("曲線の重み","nurbs weights","基底の正規化された有理重み。",Number,[C]),
        CurveKnots=>spec!("曲線のノット","knot vector","基底の完全ノット列。",Number,[C]),
        CurveDegree=>spec!("曲線の次数","degree","NURBSの次数。解析らせんは対象外。",Number,[C]),
        CurveInsertKnot=>spec!("ノット挿入","insert knot","形状を変えずに内部ノットを挿入。周期/切り出し済み基底は非対応。",Curve,[C,T,SEG]),
        CurveOffsetPreview=>spec!("XY 折れ線オフセット（近似）","offset curve preview polyline","表示点列からXY平面でマイター結合を作る。一般NURBSの厳密Offsetではない。自己交差未検査。",Curve,[C,n("distance","左向き距離",2.,-10000.,10000.,0.1)]),
        CurveFrameX=>spec!("垂直断面 X 軸","perpendicular frame x","接線に直交する局所単位X軸（XYZ）。独立した断面軸で平行移動フレームではない。",Point,[C,T]),
        CurveFrameY=>spec!("垂直断面 Y 軸","perpendicular frame y","接線とX軸に直交する単位Y軸（XYZ）。",Point,[C,T]),
        CurveLineLine=>spec!("線分・線分交点","line line intersection","2本の有限直線の交点。平行・重複は明示エラー。",Point,[g("a","線分 A",DataType::Curve),g("b","線分 B",DataType::Curve),TOL]),
        CurveLinePlane=>spec!("線分・平面交点","line plane intersection","有限直線と原点・法線で定義した平面。重複は明示エラー。",Point,[C,g("origin","平面上の点",DataType::Point),g("normal","平面法線 XYZ",DataType::Point),TOL]),
        _=>unreachable!("curve node dispatch"),
    }
}
fn point(v:&Value)->R<Point3>{match v {Value::Point(p)=>Ok(*p),_=>Err("点を接続してください。".into())}}
fn arg(a:&[&Value],k:usize)->R<Point3>{point(a.get(k).copied().ok_or("点の入力がありません。")?)}
fn cv(v:&Value)->R<&Curve>{match v{Value::Curve(c)=>Ok(c.as_ref()),_=>Err("曲線を接続してください。".into())}}
/// Legacy, mesh-derived, or nonlinearly transformed curves evaluate as piecewise linear curves,
/// never as a recovered smooth NURBS. Use this API for future surface nodes.
pub fn geometry(c:&Curve)->R<CurveGeometry>{
    match &c.geometry {Some(g)=>Ok(g.clone()),None=>Ok(CurveGeometry::nurbs(NurbsCurve::polyline(c.points.clone(),c.closed)?))}
}
fn curve(a:&[&Value],k:usize)->R<&Curve>{
    cv(a.get(k).copied().ok_or("曲線の入力がありません。")?)
}
fn preview(g:CurveGeometry,seg:usize,cancel:&dyn Fn()->bool)->R<Value>{
    let closed=g.closed()?;
    let points=g.preview(seg,cancel)?;
    if points.len()<2 {return Err("曲線の表示点が不足しています。".into());}
    Ok(Value::Curve(Arc::new(Curve{points,closed,geometry:Some(g)})))
}
fn polyline(points:Vec<Point3>,closed:bool)->R<Value>{
    let geom=CurveGeometry::nurbs(NurbsCurve::polyline(points.clone(),closed)?);
    let mut points=points;
    if closed && points.first()==points.last(){points.pop();}
    if points.len()<2{return Err("曲線には異なる2点以上が必要です。".into());}
    Ok(Value::Curve(Arc::new(Curve{points,closed,geometry:Some(geom)})))
}
fn setting(l:&[&[Value]],k:usize)->R<f64>{l.get(k).and_then(|l|l.first()).ok_or("設定がありません。")?.number()}
fn numbers(l:&[Value])->R<Vec<f64>>{l.iter().map(Value::number).collect()}
fn pts(l:&[Value])->R<Vec<Point3>>{l.iter().map(point).collect()}
fn guard(out:&[Value])->R<()>{
    if out.len()>MAX_ITEMS || out.iter().map(Value::weight).fold(0usize,usize::saturating_add)>MAX_TOTAL_WEIGHT/2 {
        Err("曲線の結果が大きすぎます。分割数を減らしてください。".into())
    }else{Ok(())}
}
pub fn run_list(k:NodeKind,l:&[&[Value]],cancel:&dyn Fn()->bool)->R<Vec<Value>>{
    use NodeKind::*;
    cancelled(cancel)?;
    let first=l.first().copied().ok_or("リスト入力がありません。")?;
    let out=match k {
        CurvePolyline=>vec![polyline(pts(first)?,setting(l,1)?!=0.)?],
        CurveBezier=>vec![preview(CurveGeometry::nurbs(NurbsCurve::bezier(pts(first)?)?),setting(l,1)? as usize,cancel)?],
        CurveNurbs=>vec![preview(CurveGeometry::nurbs(NurbsCurve::uniform(pts(first)?,setting(l,1)? as usize,setting(l,2)?!=0.)?),setting(l,3)? as usize,cancel)?],
        CurveWeightedNurbs=>vec![preview(CurveGeometry::nurbs(NurbsCurve::new(setting(l,3)? as usize,pts(first)?,numbers(l[1])?,numbers(l[2])?)?),setting(l,4)? as usize,cancel)?],
        CurveInterpolate=>vec![preview(CurveGeometry::nurbs(NurbsCurve::interpolate(&pts(first)?,setting(l,1)? as usize,cancel)?),setting(l,2)? as usize,cancel)?],
        CurveShatter=>{
            let mut t=numbers(l[1])?;
            if t.iter().any(|v|*v<0.||*v>1.) {return Err("分割パラメータは0〜1にしてください。".into());}
            t.extend([0.,1.]); t.sort_by(f64::total_cmp);t.dedup_by(|a,b|(*a-*b).abs()<1e-12);
            if t.len()>513 || first.len().saturating_mul(t.len()-1)>MAX_ITEMS {return Err("分割結果が多すぎます。".into());}
            let seg=setting(l,2)? as usize;
            let mut out=Vec::new();let mut weight=0usize;
            for v in first{
                let g=geometry(cv(v)?)?;
                for w in t.windows(2){
                    cancelled(cancel)?;
                    let result=preview(g.subcurve(w[0],w[1])?,seg,cancel)?;
                    weight=weight.saturating_add(result.weight());
                    if weight>MAX_TOTAL_WEIGHT/2{return Err("分割結果が大きすぎます。".into());}
                    out.push(result);
                }
            }
            out
        }
        _=>return Err("曲線リストノードが無効です。".into()),
    };
    guard(&out)?;Ok(out)
}
fn straight(c:&Curve)->R<(Point3,Point3)>{
    if c.closed{return Err("閉曲線は線分ではありません。".into());}
    let g=geometry(c)?;
    let n=g.as_nurbs().ok_or("直線だけに対応しています。")?;
    if n.degree()!=1 || n.control_points().len()!=2 {return Err("直線だけに対応しています。".into());}
    let a=g.point(0.)?;let b=g.point(1.)?;
    if cg::norm(cg::sub(b,a))==0.{return Err("長さゼロの直線です。".into());}
    Ok((a,b))
}
fn line_line(a:&Curve,b:&Curve,tol:f64)->R<Vec<Value>>{
    let (p,p1)=straight(a)?;let (q,q1)=straight(b)?;
    let u=cg::sub(p1,p);let v=cg::sub(q1,q);let w=cg::sub(q,p);
    let lu=cg::norm(u);let lv=cg::norm(v);
    let u=cg::unit(u)?;let v=cg::unit(v)?;
    let n=cg::cross(u,v);let den=cg::dot(n,n);
    if den<=1e-24{return Err("線分は平行または重複しています。単一交点を返せません。".into());}
    let s=cg::dot(cg::cross(w,v),n)/den;let t=cg::dot(cg::cross(w,u),n)/den;
    if s< -tol||s>lu+tol||t< -tol||t>lv+tol{return Ok(vec![]);}
    let x=cg::add(p,cg::scale(u,s.clamp(0.,lu)));let y=cg::add(q,cg::scale(v,t.clamp(0.,lv)));
    if cg::norm(cg::sub(x,y))>tol{return Ok(vec![]);}
    Ok(vec![Value::Point(cg::scale(cg::add(x,y),0.5))])
}
fn line_plane(c:&Curve,origin:Point3,normal:Point3,tol:f64)->R<Vec<Value>>{
    let (a,b)=straight(c)?;let u=cg::sub(b,a);let len=cg::norm(u);let u=cg::unit(u)?;
    let n=cg::unit(normal)?;let den=cg::dot(n,u);let d=cg::dot(n,cg::sub(origin,a));
    if den.abs()<1e-12 {
        return if d.abs()<=tol{Err("線分が平面に重複しています。".into())}else{Ok(vec![])};
    }
    let s=d/den;
    if s< -tol||s>len+tol{return Ok(vec![]);}
    Ok(vec![Value::Point(cg::add(a,cg::scale(u,s.clamp(0.,len))))])
}
fn offset(c:&Curve,d:f64,cancel:&dyn Fn()->bool)->R<Value>{
    let p=&c.points;
    if p.len()<2 || c.closed&&p.len()<3 || p.iter().any(|q|(q[2]-p[0][2]).abs()>1e-8){
        return Err("XYに平行な平面内の折れ線だけに対応しています。".into());
    }
    let normal=|a:Point3,b:Point3|->R<Point3>{
        let u=cg::unit(cg::sub(b,a))?;Ok([-u[1],u[0],0.])
    };
    let n=p.len();
    let mut out=Vec::with_capacity(n);
    for i in 0..n{
        cancelled(cancel)?;
        let shift=if !c.closed&&i==0{cg::scale(normal(p[0],p[1])?,d)}
        else if !c.closed&&i+1==n{cg::scale(normal(p[n-2],p[n-1])?,d)}
        else{
            let a=normal(p[(i+n-1)%n],p[i])?;let b=normal(p[i],p[(i+1)%n])?;
            let den=1.+cg::dot(a,b);
            if den<0.02{return Err("折り返し角でオフセットできません。".into());}
            cg::scale(cg::add(a,b),d/den)
        };
        out.push(cg::add(p[i],shift));
    }
    // Intentionally no smooth geometry: the result is the offset of the sampled polyline,
    // not an exact offset of its source NURBS. The viewer marks the result as a polyline.
    Ok(Value::Curve(Arc::new(Curve{points:out,closed:c.closed,geometry:None})))
}
pub fn run_item(k:NodeKind,a:&[&Value],cancel:&dyn Fn()->bool)->R<Vec<Value>>{
    use NodeKind::*;
    cancelled(cancel)?;
    let one=match k {
        CurveLine=>polyline(vec![arg(a,0)?,arg(a,1)?],false)?,
        CurveLineSdl=>{
            let p=arg(a,0)?;let q=cg::add(p,cg::scale(cg::unit(arg(a,1)?)?,num(a,2)?));
            polyline(vec![p,q],false)?
        }
        CurveCircleCnr=>preview(CurveGeometry::circle(arg(a,0)?,arg(a,1)?,num(a,2)?)?,num(a,3)? as usize,cancel)?,
        CurveCircleThree|CurveArcThree=>preview(CurveGeometry::through_three(arg(a,0)?,arg(a,1)?,arg(a,2)?,k==CurveCircleThree)?,num(a,3)? as usize,cancel)?,
        CurveArc=>{
            let r=num(a,1)?;
            preview(CurveGeometry::arc(arg(a,0)?,[r,0.,0.],[0.,r,0.],num(a,2)?.to_radians(),num(a,3)?.to_radians())?,num(a,4)? as usize,cancel)?
        }
        CurveEllipse=>preview(CurveGeometry::arc(arg(a,0)?,[num(a,1)?,0.,0.],[0.,num(a,2)?,0.],0.,TAU)?,num(a,3)? as usize,cancel)?,
        CurveSpiral=>{
            let r=num(a,1)?;
            preview(CurveGeometry::helix(arg(a,0)?,[r,0.,0.],[0.,r,0.],[0.;3],num(a,3)?,num(a,2)?)?,num(a,4)? as usize,cancel)?
        }
        CurveLineLine=>return line_line(curve(a,0)?,curve(a,1)?,num(a,2)?),
        CurveLinePlane=>return line_plane(curve(a,0)?,arg(a,1)?,arg(a,2)?,num(a,3)?),
        CurveOffsetPreview=>offset(curve(a,0)?,num(a,1)?,cancel)?,
        CurveReverse=>{
            let c=curve(a,0)?;let mut pts=c.points.clone();
            if c.closed && pts.len()>1 {pts[1..].reverse();}else{pts.reverse();}
            Value::Curve(Arc::new(Curve{points:pts,closed:c.closed,geometry:c.geometry.as_ref().map(CurveGeometry::reversed)}))
        }
        _=>{
            let c=curve(a,0)?;let g=geometry(c)?;
            match k{
                CurveLength=>Value::Number(g.length_table(cancel)?.total()),
                CurveEvaluate=>Value::Point(g.point(num(a,1)?)?),
                CurveEvaluateLength|CurveMiddle=>{
                    let tab=g.length_table(cancel)?;
                    let len=if k==CurveMiddle{tab.total()/2.}else{num(a,1)?};
                    Value::Point(g.point(tab.parameter(&g,len,cancel)?)?)
                }
                CurveClosestParameter|CurveClosestPoint=>{
                    let t=g.closest_parameter(arg(a,1)?,cancel)?;
                    if k==CurveClosestParameter{Value::Number(t)}else{Value::Point(g.point(t)?)}
                }
                CurveStart=>Value::Point(g.point(0.)?),
                CurveEnd=>Value::Point(g.point(1.)?),
                CurveTangent=>Value::Point(g.tangent(num(a,1)?)?),
                CurveCurvature=>Value::Number(g.curvature(num(a,1)?)?),
                CurveDomain=>return Ok(g.domain().into_iter().map(Value::Number).collect()),
                CurveClosed=>Value::Number(if g.closed()?{1.}else{0.}),
                CurveDivide|CurveDivideLength=>{
                    let tab=g.length_table(cancel)?;let len=tab.total();
                    if len<=0.{return Err("長さゼロの曲線を分割できません。".into());}
                    let v=num(a,1)?;
                    let count=if k==CurveDivide{v as usize}else{(len/v).ceil() as usize};
                    if count==0||count>512{return Err("分割数を1〜512にしてください。".into());}
                    let closed=g.closed()?;
                    let mut out=Vec::with_capacity(count+1);
                    for i in 0..=count{
                        cancelled(cancel)?;
                        if closed&&i==count{break;}
                        let s=if i==count{len}else if k==CurveDivide{len*i as f64/count as f64}else{(v*i as f64).min(len)};
                        out.push(Value::Point(g.point(tab.parameter(&g,s,cancel)?)?));
                    }
                    return Ok(out);
                }
                CurveSubcurve=>preview(g.subcurve(num(a,1)?,num(a,2)?)?,num(a,3)? as usize,cancel)?,
                CurveControls|CurveWeights|CurveKnots|CurveDegree|CurveInsertKnot=>{
                    let n=g.as_nurbs().ok_or("NURBS基底を持つ曲線だけに対応しています。")?;
                    match k{
                        CurveControls=>return Ok(n.control_points().into_iter().map(Value::Point).collect()),
                        CurveWeights=>return Ok(n.weights().into_iter().map(Value::Number).collect()),
                        CurveKnots=>return Ok(n.knots().iter().copied().map(Value::Number).collect()),
                        CurveDegree=>Value::Number(n.degree() as f64),
                        _=>{
                            if g.domain()!=n.domain(){return Err("部分曲線または反転曲線へのノット挿入は未対応です。".into());}
                            let [lo,hi]=n.domain();
                            let t=num(a,1)?;
                            preview(CurveGeometry::nurbs(n.insert_knot(lo+(hi-lo)*t)?),num(a,2)? as usize,cancel)?
                        }
                    }
                }
                CurveFrameX|CurveFrameY=>{
                    let z=g.tangent(num(a,1)?)?;
                    let helper=if z[2].abs()<0.9{[0.,0.,1.]}else{[1.,0.,0.]};
                    let x=cg::unit(cg::cross(helper,z))?;
                    Value::Point(if k==CurveFrameX{x}else{cg::cross(z,x)})
                }
                _=>return Err("リスト評価が必要な曲線ノードです。".into()),
            }
        }
    };
    Ok(vec![one])
}
