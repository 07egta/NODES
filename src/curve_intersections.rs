//! Bounded numerical intersections. Sampling is independent of rendering resolution.
//! Polyline intersections and distances are estimates for smooth curves, not certified roots.
use crate::curve_geometry::{self as cg,CurveGeometry,Point3};
type R<T>=Result<T,String>;
type Sample=(f64,Point3);
fn check(cancel:&dyn Fn()->bool)->R<()>{
    if cancel(){Err("計算を中止しました。".into())}else{Ok(())}
}
fn tol_ok(tol:f64)->R<()>{
    if tol.is_finite()&&tol>0.{Ok(())}else{Err("正の有限な許容差が必要です。".into())}
}
fn samples(g:&CurveGeometry,n:usize,cancel:&dyn Fn()->bool)->R<Vec<Sample>>{
    if !(8..=512).contains(&n){return Err("計算分割数は8〜512です。".into());}
    let mut t=g.breaks();t.extend((0..=n).map(|i|i as f64/n as f64));t.sort_by(f64::total_cmp);t.dedup();
    if t.len()>1025{return Err("交差計算の区間数が上限を超えました。".into());}
    t.into_iter().map(|x|{check(cancel)?;Ok((x,g.point(x)?))}).collect()
}
fn lerp(a:Point3,b:Point3,t:f64)->Point3{cg::add(a,cg::scale(cg::sub(b,a),t))}
fn dist(a:Point3,b:Point3)->f64{cg::norm(cg::sub(a,b))}
fn project(p:Point3,a:Point3,b:Point3)->Point3{
    let d=cg::sub(b,a);let len=cg::norm(d);
    if len==0.{a}else{lerp(a,b,(cg::dot(cg::sub(p,a),cg::scale(d,1./len))/len).clamp(0.,1.))}
}
/// Closest pair of finite segments: interior critical point and all four boundaries.
fn closest(a:Point3,b:Point3,c:Point3,d:Point3)->R<(Point3,Point3,f64)>{
    let mut best=(a,c,dist(a,c));
    let mut consider=|p:Point3,q:Point3|{let dd=dist(p,q);if dd<best.2{best=(p,q,dd);}};
    consider(a,project(a,c,d));consider(b,project(b,c,d));
    consider(project(c,a,b),c);consider(project(d,a,b),d);
    let u=cg::sub(b,a);let v=cg::sub(d,c);let lu=cg::norm(u);let lv=cg::norm(v);
    if lu>0.&&lv>0.{
        let u=cg::scale(u,1./lu);let v=cg::scale(v,1./lv);
        let w=cg::sub(c,a);let cross=cg::cross(u,v);let den=cg::dot(cross,cross);
        if den>1e-24{
            let s=cg::dot(cg::cross(w,v),cross)/den;
            let t=cg::dot(cg::cross(w,u),cross)/den;
            if s>=0.&&s<=lu&&t>=0.&&t<=lv{consider(lerp(a,b,s/lu),lerp(c,d,t/lv));}
        }
    }
    if best.2.is_finite(){Ok(best)}else{Err("距離が有限ではありません。".into())}
}
fn overlapping(a:Point3,b:Point3,c:Point3,d:Point3,tol:f64)->bool{
    let u=cg::sub(b,a);let len=cg::norm(u);let v=cg::sub(d,c);let lv=cg::norm(v);
    if len<=tol||lv<=tol{return false;}
    let u=cg::scale(u,1./len);
    if cg::norm(cg::cross(u,cg::scale(v,1./lv)))>1e-8{return false;}
    let p=cg::sub(c,a);let q=cg::sub(d,a);
    if cg::norm(cg::cross(u,p))>tol||cg::norm(cg::cross(u,q))>tol{return false;}
    let x=cg::dot(u,p);let y=cg::dot(u,q);
    x.max(y).min(len)-x.min(y).max(0.)>tol
}
fn push(out:&mut Vec<Point3>,p:Point3,tol:f64)->R<()>{
    if !p.iter().all(|v|v.is_finite()){return Err("交点が有限ではありません。".into());}
    if !out.iter().any(|q|dist(*q,p)<=tol){
        if out.len()>=2048{return Err("交点数が上限を超えました。".into());}
        out.push(p);
    }
    Ok(())
}
fn pairs(a:&[Sample],b:&[Sample],same:bool,closed:bool,tol:f64,cancel:&dyn Fn()->bool)->R<Vec<Point3>>{
    tol_ok(tol)?;let mut out=Vec::new();
    for (i,x) in a.windows(2).enumerate(){
        check(cancel)?;
        for (j,y) in b.windows(2).enumerate(){
            if same&&(j<=i+1||closed&&i==0&&j+2==b.len()){continue;}
            let (p,q,d)=closest(x[0].1,x[1].1,y[0].1,y[1].1)?;
            if d<=tol{
                if overlapping(x[0].1,x[1].1,y[0].1,y[1].1,tol){return Err("線分の重複または許容差内の重複を検出しました。単一交点にできません。".into());}
                push(&mut out,lerp(p,q,0.5),tol)?;
            }
        }
    }
    Ok(out)
}
pub fn intersect(a:&CurveGeometry,b:&CurveGeometry,n:usize,tol:f64,cancel:&dyn Fn()->bool)->R<Vec<Point3>>{
    pairs(&samples(a,n,cancel)?,&samples(b,n,cancel)?,false,false,tol,cancel)
}
pub fn self_intersect(a:&CurveGeometry,n:usize,tol:f64,cancel:&dyn Fn()->bool)->R<Vec<Point3>>{
    let s=samples(a,n,cancel)?;pairs(&s,&s,true,a.closed()?,tol,cancel)
}
pub fn distance(a:&CurveGeometry,b:&CurveGeometry,n:usize,cancel:&dyn Fn()->bool)->R<f64>{
    let a=samples(a,n,cancel)?;let b=samples(b,n,cancel)?;let mut best=f64::INFINITY;
    for x in a.windows(2){check(cancel)?;for y in b.windows(2){
        best=best.min(closest(x[0].1,x[1].1,y[0].1,y[1].1)?.2);
    }}
    if best.is_finite(){Ok(best)}else{Err("距離を計算できません。".into())}
}
/// Signed-distance brackets on the true curve, plus local minimum search for tangency.
pub fn plane(g:&CurveGeometry,o:Point3,normal:Point3,n:usize,tol:f64,cancel:&dyn Fn()->bool)->R<Vec<Point3>>{
    tol_ok(tol)?;let normal=cg::unit(normal)?;let s=samples(g,n,cancel)?;
    let f=|p|cg::dot(cg::sub(p,o),normal);
    let values:Vec<f64>=s.iter().map(|(_,p)|f(*p)).collect();
    if values.iter().any(|v|!v.is_finite()){return Err("平面距離が有限ではありません。".into());}
    if values.iter().all(|d|d.abs()<=tol){return Err("曲線全体が平面と重複するか、許容差内にあります。".into());}
    let mut out=Vec::new();
    for i in 0..s.len(){
        check(cancel)?;
        if values[i].abs()<=tol{push(&mut out,s[i].1,tol)?;}
        if i+1<s.len()&&values[i].is_sign_positive()!=values[i+1].is_sign_positive()&&values[i]!=0.&&values[i+1]!=0.{
            let mut l=s[i].0;let mut r=s[i+1].0;let sign=values[i].is_sign_positive();
            for _ in 0..52{let m=(l+r)/2.;if f(g.point(m)?).is_sign_positive()==sign{l=m;}else{r=m;}}
            let p=g.point((l+r)/2.)?;if f(p).abs()<=tol{push(&mut out,p,tol)?;}
        }
        if i>0&&i+1<s.len()&&values[i-1].is_sign_positive()==values[i+1].is_sign_positive()&&values[i].abs()<values[i-1].abs()&&values[i].abs()<values[i+1].abs(){
            let mut l=s[i-1].0;let mut r=s[i+1].0;
            for _ in 0..48{
                let x=l+(r-l)/3.;let y=r-(r-l)/3.;
                if f(g.point(x)?).abs()<f(g.point(y)?).abs(){r=y;}else{l=x;}
            }
            let p=g.point((l+r)/2.)?;if f(p).abs()<=tol{push(&mut out,p,tol)?;}
        }
    }
    Ok(out)
}
/// Analytic intersection for circles parallel to XY. Tolerance applies to coplanarity,
/// tangency and equality; the result near tangency is one representative point.
pub fn circle_circle_xy(a:Point3,ra:f64,b:Point3,rb:f64,tol:f64)->R<Vec<Point3>>{
    tol_ok(tol)?;
    if !ra.is_finite()||!rb.is_finite()||ra<=0.||rb<=0.||a.iter().chain(b.iter()).any(|v|!v.is_finite()){
        return Err("円の中心・半径が不正です。".into());
    }
    if (a[2]-b[2]).abs()>tol{return Ok(vec![]);}
    let dx=b[0]-a[0];let dy=b[1]-a[1];let d=dx.hypot(dy);
    if d<=tol{return if (ra-rb).abs()<=tol{Err("円が重複しています。".into())}else{Ok(vec![])};}
    if d>ra+rb+tol||d<(ra-rb).abs()-tol{return Ok(vec![]);}
    let x=(d+(ra-rb)*(ra+rb)/d)/2.;let h2=(ra-x)*(ra+x);
    let mid=[a[0]+x*dx/d,a[1]+x*dy/d,(a[2]+b[2])/2.];
    if (d-ra-rb).abs()<=tol||(d-(ra-rb).abs()).abs()<=tol{return Ok(vec![mid]);}
    if h2<0.{return Ok(vec![]);}
    let h=h2.sqrt();let offset=[-h*dy/d,h*dx/d,0.];
    Ok(vec![cg::add(mid,offset),cg::sub(mid,offset)])
}
/// Earliest bracketed chord-distance crossing; narrow excursions can be missed.
pub fn divide_chord(g:&CurveGeometry,step:f64,n:usize,cancel:&dyn Fn()->bool)->R<Vec<Point3>>{
    tol_ok(step)?;let s=samples(g,n,cancel)?;let mut out=vec![s[0].1];let mut t=0.;let mut p=s[0].1;
    let mut idx=1;
    while idx<s.len(){
        check(cancel)?;
        let (r,q)=s[idx];if r<=t||dist(p,q)<step{idx+=1;continue;}
        let mut lo=t.max(s[idx-1].0);let mut hi=r;
        for _ in 0..52{let m=(lo+hi)/2.;if dist(p,g.point(m)?)<step{lo=m;}else{hi=m;}}
        let u=(lo+hi)/2.;
        if u<=t+1e-15{return Err("分割距離が小さすぎます。".into());}
        let q=g.point(u)?;
        if out.len()>=513{return Err("直線距離分割が512区間を超えました。".into());}
        if !(g.closed()?&&u>=1.-1e-12){out.push(q);}
        t=u;p=q;
    }
    Ok(out)
}
