//! Independent mathematical curves. No Rhino runtime, third-party geometry code, or mesh
//! approximation is involved in point/derivative evaluation. All coordinates are f64.
//! A CurveGeometry shares its immutable basis and an oriented parameter interval; its
//! normalized parameter is distinct from its arc length. See docs/CURVE_ENGINE.md.
use std::{f64::consts::{PI, TAU}, sync::Arc};

pub type Point3 = [f64; 3];
type H = [f64; 4];
type R<T> = Result<T, String>;
pub const MAX_DEGREE: usize = 12;
pub const MAX_CONTROLS: usize = 8193;
pub const MAX_PREVIEW: usize = 4096;
const MAX_QUAD_WORK: usize = 200_000;

pub fn add(a: Point3, b: Point3) -> Point3 { std::array::from_fn(|i| a[i]+b[i]) }
pub fn sub(a: Point3, b: Point3) -> Point3 { std::array::from_fn(|i| a[i]-b[i]) }
pub fn scale(a: Point3, s: f64) -> Point3 { a.map(|v| v*s) }
pub fn dot(a: Point3, b: Point3) -> f64 { a.iter().zip(b).map(|(x,y)| x*y).sum() }
pub fn cross(a: Point3,b: Point3) -> Point3 {
    [a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]
}
pub fn norm(a: Point3) -> f64 { a[0].hypot(a[1]).hypot(a[2]) }
pub fn unit(a: Point3) -> R<Point3> {
    let d=norm(a);
    if d<=0. || !d.is_finite() { return Err("接線または方向が定義できません。".into()); }
    Ok(a.map(|x| x/d))
}
fn finite(p: Point3) -> bool { p.iter().all(|x| x.is_finite()) }
fn checked(p: Point3) -> R<Point3> {
    if finite(p) { Ok(p) } else { Err("曲線の計算結果が有限ではありません。".into()) }
}
fn check_t(t: f64, a: f64, b: f64) -> R<()> {
    if t.is_finite() && t>=a && t<=b { Ok(()) } else { Err("曲線パラメータが区間外です。".into()) }
}
fn stopped(cancel: &dyn Fn()->bool) -> R<()> {
    if cancel() { Err("計算を停止しました。".into()) } else { Ok(()) }
}
fn xyz(v:H)->Point3 { [v[0],v[1],v[2]] }
fn mix(a:H,b:H,t:f64)->H { std::array::from_fn(|i| (1.-t)*a[i]+t*b[i]) }
fn span(p:usize, count:usize, knots:&[f64], t:f64)->usize {
    if t==knots[count] { count-1 }
    else { (knots.partition_point(|x| *x<=t)-1).clamp(p,count-1) }
}
// Locally evaluate a homogeneous B-spline. Repeated knots are handled by taking the
// right-hand span, except at the final endpoint. The triangular recurrence is de Boor.
fn deboor(p:usize,count:usize,k:&[f64],t:f64, control:impl Fn(usize)->H)->H {
    let s=span(p,count,k,t);
    let mut d=[[0.;4];MAX_DEGREE+1];
    for (j,v) in d.iter_mut().take(p+1).enumerate() { *v=control(s-p+j); }
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i=s-p+j;
            let den=k[i+p-r+1]-k[i];
            let alpha=if den==0. {0.} else {(t-k[i])/den};
            d[j]=mix(d[j-1],d[j],alpha);
        }
    }
    d[p]
}
fn clamped_knots(count:usize,p:usize)->Vec<f64> {
    (0..count+p+1).map(|j| {
        if j<=p {0.} else if j>=count {1.} else {(j-p) as f64/(count-p) as f64}
    }).collect()
}

/// Non-uniform rational B-spline with a complete knot vector (count + degree + 1).
/// Homogeneous coordinates are retained internally; exported weights are normalized by a
/// common factor (which does not change the locus). Only positive weights are supported.
#[derive(Clone,Debug,PartialEq)]
pub struct NurbsCurve {
    degree:usize,
    knots:Vec<f64>,
    h:Vec<H>,
    periodic:bool,
}
impl NurbsCurve {
    pub fn new(degree:usize, points:Vec<Point3>, weights:Vec<f64>, knots:Vec<f64>)->R<Self> {
        let n=points.len();
        if degree==0 || degree>MAX_DEGREE || n<=degree || n>MAX_CONTROLS ||
            weights.len()!=n || knots.len()!=n+degree+1 ||
            points.iter().any(|p| !finite(*p)) ||
            weights.iter().any(|w| !w.is_finite() || *w<=0.) ||
            knots.iter().any(|u| !u.is_finite()) ||
            knots.windows(2).any(|w| w[0]>w[1]) ||
            !((knots[n]-knots[degree]).is_finite() && knots[n]>knots[degree]) ||
            knots[degree]>=knots[degree+1] || knots[n-1]>=knots[n] {
            return Err("NURBS の次数・制御点・重み・ノット区間が無効です。".into());
        }
        let mut i=0;
        while i<knots.len() {
            let mut j=i+1;
            while j<knots.len() && knots[j]==knots[i] { j+=1; }
            let inner=knots[i]>knots[degree] && knots[i]<knots[n];
            if j-i>degree+usize::from(!inner) {
                return Err("ノットの多重度が次数の上限を超えています。".into());
            }
            i=j;
        }
        let max=weights.iter().copied().fold(0.,f64::max);
        let mut h=Vec::with_capacity(n);
        for (p,w) in points.into_iter().zip(weights) {
            let w=w/max;
            if w<1e-12 { return Err("NURBS の重みの比が大きすぎます。".into()); }
            h.push([p[0]*w,p[1]*w,p[2]*w,w]);
        }
        Ok(Self {degree,knots,h,periodic:false})
    }
    pub fn uniform(points:Vec<Point3>, degree:usize, periodic:bool)->R<Self> {
        let n=points.len();
        if degree==0 || degree>MAX_DEGREE || n<=degree || n>MAX_CONTROLS ||
            (periodic && n+degree>MAX_CONTROLS) {
            return Err("制御点数は次数より多くしてください。".into());
        }
        if periodic {
            let mut p=points;
            p.extend_from_within(..degree);
            let count=p.len();
            let k=(0..count+degree+1).map(|i|(i as f64-degree as f64)/n as f64).collect();
            let mut c=Self::new(degree,p,vec![1.;count],k)?;
            c.periodic=true;
            Ok(c)
        } else {
            Self::new(degree,points,vec![1.;n],clamped_knots(n,degree))
        }
    }
    pub fn bezier(points:Vec<Point3>)->R<Self> {
        let p=points.len().checked_sub(1).ok_or("制御点がありません。")?;
        Self::uniform(points,p,false)
    }
    pub fn polyline(mut points:Vec<Point3>, closed:bool)->R<Self> {
        if closed && points.first()!=points.last() {
            if let Some(p)=points.first().copied() { points.push(p); }
        }
        Self::uniform(points,1,false)
    }
    pub fn degree(&self)->usize { self.degree }
    pub fn knots(&self)->&[f64] { &self.knots }
    pub fn weights(&self)->Vec<f64> { self.h.iter().map(|h| h[3]).collect() }
    pub fn control_points(&self)->Vec<Point3> {
        self.h.iter().map(|h| xyz(*h).map(|x|x/h[3])).collect()
    }
    pub fn periodic(&self)->bool { self.periodic }
    pub fn domain(&self)->[f64;2] { [self.knots[self.degree],self.knots[self.h.len()]] }
    fn homogeneous(&self,t:f64)->H {
        deboor(self.degree,self.h.len(),&self.knots,t,|j|self.h[j])
    }
    fn first_control(&self,j:usize)->H {
        let p=self.degree;
        let d=self.knots[j+p+1]-self.knots[j+1];
        if d==0. { [0.;4] }
        else { std::array::from_fn(|i| (self.h[j+1][i]-self.h[j][i])*(p as f64/d)) }
    }
    fn second_control(&self,j:usize)->H {
        let p=self.degree;
        let d=self.knots[j+p+1]-self.knots[j+2];
        if d==0. { [0.;4] }
        else {
            let a=self.first_control(j);
            let b=self.first_control(j+1);
            std::array::from_fn(|i| (b[i]-a[i])*((p-1) as f64/d))
        }
    }
    pub fn point(&self,t:f64)->R<Point3> {
        let [a,b]=self.domain(); check_t(t,a,b)?;
        let h=self.homogeneous(t);
        checked(xyz(h).map(|x|x/h[3]))
    }
    /// Returns C, C', C'' with respect to the native knot parameter (one-sided at knots).
    pub fn derivatives(&self,t:f64)->R<[Point3;3]> {
        let [a,b]=self.domain(); check_t(t,a,b)?;
        let p=self.degree; let n=self.h.len(); let k=&self.knots;
        let h=self.homogeneous(t);
        let dh=deboor(p-1,n-1,&k[1..k.len()-1],t,|j|self.first_control(j));
        let ddh=if p>=2 {deboor(p-2,n-2,&k[2..k.len()-2],t,|j|self.second_control(j))}
            else {[0.;4]};
        let c=checked(xyz(h).map(|x|x/h[3]))?;
        let d=checked(sub(xyz(dh),scale(c,dh[3])).map(|x|x/h[3]))?;
        let dd=checked(sub(sub(xyz(ddh),scale(d,2.*dh[3])),scale(c,ddh[3])).map(|x|x/h[3]))?;
        Ok([c,d,dd])
    }
    /// Single interior knot insertion in homogeneous coordinates, preserving the locus.
    /// Periodic knot insertion needs a separate periodic representation and is rejected.
    pub fn insert_knot(&self,t:f64)->R<Self> {
        let [a,b]=self.domain(); let p=self.degree; let n=self.h.len();
        if self.periodic || !t.is_finite() || t<=a || t>=b || n==MAX_CONTROLS {
            return Err("内部ノットのみ挿入できます（周期曲線は未対応）。".into());
        }
        let s=self.knots.iter().filter(|u| **u==t).count();
        if s>=p { return Err("ノットの多重度が上限です。".into()); }
        let k=span(p,n,&self.knots,t);
        let mut h=Vec::with_capacity(n+1);
        for i in 0..=n {
            h.push(if i<=k-p {self.h[i]} else if i>k-s {self.h[i-1]} else {
                let alpha=(t-self.knots[i])/(self.knots[i+p]-self.knots[i]);
                mix(self.h[i-1],self.h[i],alpha)
            });
        }
        let mut knots=self.knots.clone(); knots.insert(k+1,t);
        Ok(Self {degree:p,knots,h,periodic:false})
    }
    /// Global chord-length interpolation, using averaged clamped knots and partial pivoting.
    /// Bounded dense solve; no implicit degree reduction.
    pub fn interpolate(points:&[Point3],degree:usize,cancel:&dyn Fn()->bool)->R<Self> {
        let n=points.len();
        if n>128 || degree==0 || degree>MAX_DEGREE || n<=degree ||
            points.iter().any(|p|!finite(*p)) {
            return Err("補間は 2〜128 点、点数未満の次数に対応します。".into());
        }
        let mut u=vec![0.;n];
        for i in 1..n {
            let d=norm(sub(points[i],points[i-1]));
            if d==0. || !d.is_finite() {return Err("隣り合う補間点が同じか無効です。".into());}
            u[i]=u[i-1]+d;
        }
        let len=u[n-1];
        if !len.is_finite() { return Err("補間点間の距離が大きすぎます。".into()); }
        for v in &mut u {*v/=len;}
        let p=degree;
        let mut k=clamped_knots(n,p);
        for j in 1..n-p { k[j+p]=u[j..j+p].iter().sum::<f64>()/p as f64; }
        let mut m=vec![vec![0.;n+3];n];
        for i in 0..n {
            let t=u[i]; let s=span(p,n,&k,t);
            // Local nonzero basis functions via the Cox-de Boor recurrence.
            let mut basis=[0.;MAX_DEGREE+1];
            let mut left=[0.;MAX_DEGREE+1]; let mut right=left;
            basis[0]=1.;
            for j in 1..=p {
                left[j]=t-k[s+1-j]; right[j]=k[s+j]-t;
                let mut saved=0.;
                for r in 0..j {
                    let den=right[r+1]+left[j-r];
                    let tmp=if den==0. {0.} else {basis[r]/den};
                    basis[r]=saved+right[r+1]*tmp;
                    saved=left[j-r]*tmp;
                }
                basis[j]=saved;
            }
            m[i][s-p..=s].copy_from_slice(&basis[..=p]);
            m[i][n..n+3].copy_from_slice(&points[i]);
        }
        for j in 0..n {
            stopped(cancel)?;
            let pivot=(j..n).max_by(|&a,&b|m[a][j].abs().total_cmp(&m[b][j].abs())).unwrap();
            m.swap(j,pivot);
            let d=m[j][j];
            if d.abs()<1e-13 || !d.is_finite() {return Err("補間方程式が特異または悪条件です。".into());}
            for v in &mut m[j][j..] {*v/=d;}
            let row=m[j].clone();
            for (i,r) in m.iter_mut().enumerate() {
                if i==j {continue;}
                let f=r[j];
                for k in j..n+3 {r[k]-=f*row[k];}
            }
        }
        let c=Self::new(p,m.into_iter().map(|r|[r[n],r[n+1],r[n+2]]).collect(),vec![1.;n],k)?;
        // Reject ill-conditioned systems rather than outputting a visibly wrong interpolant.
        let scale=points.iter().map(|p|norm(*p)).fold(1.,f64::max);
        for (&t,&pt) in u.iter().zip(points) {
            if norm(sub(c.point(t)?,pt))>1e-9*scale {return Err("補間精度を満たせません。".into());}
        }
        Ok(c)
    }
    /// End-point constrained least-squares approximation with uniform clamped knots
    /// and chord-length parameters. Solve by Householder QR, not normal equations.
    /// The residual is sampled, not a certified Hausdorff bound.
    pub fn fit(points:&[Point3],controls:usize,degree:usize,cancel:&dyn Fn()->bool)->R<Self>{
        let n=points.len();let p=degree;
        if !(2..=256).contains(&n)||controls<2||controls>64||controls>n||p==0||p>MAX_DEGREE||p>=controls||
            points.iter().any(|v|!finite(*v)){
            return Err("近似は2〜256点、2〜64制御点、制御点数未満の次数に対応します。".into());
        }
        let mut u=vec![0.;n];
        for i in 1..n{
            let d=norm(sub(points[i],points[i-1]));
            if d<=0.||!d.is_finite(){return Err("連続した近似点の重複または無効な距離。".into());}
            u[i]=u[i-1]+d;
        }
        let total=u[n-1];if !total.is_finite(){return Err("距離が大きすぎます。".into());}
        for x in &mut u{*x/=total;}
        let k=clamped_knots(controls,p);
        let cols=controls-2;let rows=n-2;
        let mut a=vec![vec![0.;cols+3];rows];
        for i in 0..rows{
            stopped(cancel)?;
            let t=u[i+1];
            let basis=|j|deboor(p,controls,&k,t,|h|[if h==j{1.}else{0.},0.,0.,0.])[0];
            let rhs=sub(sub(points[i+1],scale(points[0],basis(0))),scale(points[n-1],basis(controls-1)));
            for j in 0..cols{a[i][j]=basis(j+1);}
            a[i][cols..cols+3].copy_from_slice(&rhs);
        }
        for j in 0..cols{
            stopped(cancel)?;
            let len=(j..rows).fold(0_f64,|s,i|s.hypot(a[i][j]));
            if !len.is_finite()||len<1e-12{return Err("近似方程式が階数不足です。".into());}
            let mut v:Vec<f64>=(j..rows).map(|i|a[i][j]).collect();
            v[0]+=len.copysign(v[0]);
            let vlen=v.iter().fold(0_f64,|s,x|s.hypot(*x));
            if vlen==0.||!vlen.is_finite(){return Err("近似方程式を解けません。".into());}
            for x in &mut v{*x/=vlen;}
            for c in j..cols+3{
                let proj=2.*v.iter().enumerate().map(|(i,x)|x*a[i+j][c]).sum::<f64>();
                for (i,x) in v.iter().enumerate(){a[i+j][c]-=proj*x;}
            }
        }
        let mut cp=vec![[0.;3];controls];cp[0]=points[0];cp[controls-1]=points[n-1];
        for j in (0..cols).rev(){
            let diag=a[j][j];
            if diag.abs()<1e-12||!diag.is_finite(){return Err("近似方程式の条件が悪すぎます。".into());}
            let mut rhs=[a[j][cols],a[j][cols+1],a[j][cols+2]];
            for c in j+1..cols{rhs=sub(rhs,scale(cp[c+1],a[j][c]));}
            cp[j+1]=checked(scale(rhs,1./diag))?;
        }
        Self::new(p,cp,vec![1.;controls],k)
    }
    fn affine(&self,f:&dyn Fn(Point3)->Point3)->R<Self> {
        let points=self.control_points().into_iter().map(f).collect();
        let mut c=Self::new(self.degree,points,self.weights(),self.knots.clone())?;
        c.periodic=self.periodic;
        Ok(c)
    }
}

/// Analytic spiral/helix data; x and y are radial axes, rise is the total axial displacement.
/// The linear radius factor is 1 + radial_change*t, not a polyline approximation.
#[derive(Clone,Debug,PartialEq)]
enum Basis {
    Nurbs(NurbsCurve),
    Helix { origin:Point3,x:Point3,y:Point3,rise:Point3,turns:f64,radial_change:f64 },
    Composite { parts: Vec<CurveGeometry>, depth: usize },
}
#[derive(Clone,Debug,PartialEq)]
pub struct CurveGeometry {
    basis:Arc<Basis>,
    interval:[f64;2],
    approximate:bool,
}
impl CurveGeometry {
    pub fn nurbs(nurbs:NurbsCurve)->Self {
        let interval=nurbs.domain();
        Self {basis:Arc::new(Basis::Nurbs(nurbs)),interval,approximate:false}
    }
    pub fn as_nurbs(&self)->Option<&NurbsCurve> {
        match self.basis.as_ref() {Basis::Nurbs(c)=>Some(c),_=>None}
    }
    pub fn domain(&self)->[f64;2] { self.interval }
    /// True when the curve was produced by an explicitly approximate operation (e.g. rebuild).
    /// This is distinct from the floating-point or display accuracy of the mathematical basis.
    pub fn is_approximate(&self)->bool { self.approximate }
    pub fn marked_approximate(mut self)->Self {self.approximate=true;self}
    pub fn is_composite(&self)->bool {matches!(self.basis.as_ref(),Basis::Composite{..})}
    fn depth(&self)->usize{
        match self.basis.as_ref(){Basis::Composite{depth,..}=>*depth,_=>0}
    }
    /// Join in the supplied order, with a linear connector for any nonzero gap within
    /// tolerance. This preserves both source curves instead of silently moving endpoints.
    /// The resulting polycurve uses equal parameter intervals per component.
    pub fn join_ordered(curves:&[Self],tol:f64,cancel:&dyn Fn()->bool)->R<Self>{
        if curves.is_empty()||curves.len()>128||!tol.is_finite()||tol<0. {
            return Err("連結は1〜128本の曲線と非負の許容差に対応します。".into());
        }
        if curves.len()==1{return Ok(curves[0].clone());}
        let mut parts:Vec<Self>=Vec::new();
        let mut approx=false;let mut depth=1;let mut weight=0usize;
        for c in curves{
            stopped(cancel)?;
            if let Some(last)=parts.last(){
                let a=last.point(1.)?;let b=c.point(0.)?;
                let gap=norm(sub(a,b));
                if !gap.is_finite()||gap>tol{return Err("連結する曲線の端点が離れすぎています。順序と向きを確認してください。".into());}
                if gap>0.{
                    let bridge=Self::nurbs(NurbsCurve::polyline(vec![a,b],false)?);
                    weight=weight.saturating_add(bridge.weight());
                    parts.push(bridge);approx=true;
                }
            }
            approx|=c.approximate;
            depth=depth.max(c.depth()+1);
            weight=weight.saturating_add(c.weight());
            if depth>8||weight>131072{return Err("複合曲線の深さまたはサイズが上限を超えました。".into());}
            parts.push(c.clone());
        }
        Ok(Self{basis:Arc::new(Basis::Composite{parts,depth}),interval:[0.,1.],approximate:approx})
    }
    fn composite_parameter(parts:&[Self],u:f64)->(&Self,f64){
        let q=u*parts.len() as f64;
        let index=(q.floor() as usize).min(parts.len()-1);
        (&parts[index],(q-index as f64).clamp(0.,1.))
    }
    /// Explode an oriented active interval into component curves (or line segments for
    /// degree-one NURBS). Smooth higher-degree NURBS and analytic helices remain intact.
    pub fn explode(&self)->R<Vec<Self>>{
        if let Basis::Composite{parts,..}=self.basis.as_ref(){
            let [a,b]=self.interval;let count=parts.len() as f64;
            let mut out=Vec::new();
            for (i,part) in parts.iter().enumerate(){
                let l=a.min(b).max(i as f64/count);
                let h=a.max(b).min((i+1) as f64/count);
                if l>=h{continue;}
                let start=(l*count-i as f64).clamp(0.,1.);
                let end=(h*count-i as f64).clamp(0.,1.);
                if start==end{continue;}
                let mut sub=if a<b{part.subcurve(start,end)?}else{part.subcurve(end,start)?};
                sub.approximate|=self.approximate;
                out.push(sub);
            }
            if a>b{out.reverse();}
            return Ok(out);
        }
        if self.as_nurbs().is_some_and(|n|n.degree()==1){
            return self.breaks().windows(2).map(|w|self.subcurve(w[0],w[1])).collect();
        }
        Ok(vec![self.clone()])
    }
    pub fn periodic(&self)->bool {
        self.as_nurbs().is_some_and(|c|c.periodic() &&
            (self.interval==c.domain() || self.interval==[c.domain()[1],c.domain()[0]]))
    }
    /// Exact rational conic arc from center and two radial axes; sweep radians, <= one turn.
    pub fn arc(center:Point3,x:Point3,y:Point3,start:f64,sweep:f64)->R<Self> {
        if !finite(center)||!finite(x)||!finite(y)||norm(cross(x,y))==0. ||
            !start.is_finite()||!sweep.is_finite()||sweep==0.||sweep.abs()>TAU {
            return Err("円弧・楕円の軸または角度が無効です。".into());
        }
        let count=(sweep.abs()/(PI/2.)).ceil() as usize;
        let step=sweep/count as f64;
        let mut points=Vec::with_capacity(2*count+1);
        let mut weights=Vec::with_capacity(2*count+1);
        let at=|t:f64,r:f64|add(center,scale(add(scale(x,t.cos()),scale(y,t.sin())),r));
        points.push(at(start,1.)); weights.push(1.);
        for j in 0..count {
            let mid=start+(j as f64+0.5)*step;
            let w=(step/2.).cos();
            points.push(at(mid,1./w)); weights.push(w);
            points.push(at(start+(j+1) as f64*step,1.)); weights.push(1.);
        }
        if sweep.abs()==TAU {let last=points.len()-1;points[last]=points[0];}
        let mut knots=vec![0.;3];
        for j in 1..count {knots.extend([j as f64/count as f64;2]);}
        knots.extend([1.;3]);
        Ok(Self::nurbs(NurbsCurve::new(2,points,weights,knots)?))
    }
    pub fn circle(center:Point3,normal:Point3,radius:f64)->R<Self> {
        if !radius.is_finite()||radius<=0. {return Err("半径は正の値にしてください。".into());}
        let z=unit(normal)?;
        let helper=if z[0].abs()<0.9 {[1.,0.,0.]} else {[0.,1.,0.]};
        let x=unit(sub(helper,scale(z,dot(helper,z))))?;
        Self::arc(center,scale(x,radius),scale(cross(z,x),radius),0.,TAU)
    }
    /// Exact circle/arc through three ordered, non-collinear points.
    pub fn through_three(a:Point3,b:Point3,c:Point3,closed:bool)->R<Self> {
        let u=sub(b,a); let v=sub(c,a); let w=cross(u,v);
        let area=norm(w);
        if !area.is_finite()||area<=1e-12*norm(u)*norm(v)||norm(u)==0.||norm(v)==0. {
            return Err("3 点が重複またはほぼ一直線です。".into());
        }
        let z=unit(w)?;
        // Solve center in a local orthonormal frame, avoiding squared world coordinates.
        let ex=unit(u)?; let ey=cross(z,ex);
        let bx=norm(u); let cx=dot(v,ex); let cy=dot(v,ey);
        let ox=bx/2.; let oy=(cx*(cx-bx)+cy*cy)/(2.*cy);
        let center=checked(add(a,add(scale(ex,ox),scale(ey,oy))))?;
        let x=sub(a,center); let y=cross(z,x);
        let angle=|pt:Point3| {
            let d=sub(pt,center);
            dot(d,y).atan2(dot(d,x)).rem_euclid(TAU)
        };
        let end=angle(c); let middle=angle(b);
        let sweep=if closed {TAU} else if middle<end {end} else {end-TAU};
        Self::arc(center,x,y,0.,sweep)
    }
    pub fn helix(origin:Point3,x:Point3,y:Point3,rise:Point3,turns:f64,radial_change:f64)->R<Self> {
        if [origin,x,y,rise].iter().any(|p|!finite(*p)) ||
            !turns.is_finite()||turns.abs()>100.||!radial_change.is_finite()||
            radial_change< -1.||radial_change>100. {
            return Err("らせんのパラメータが無効です。".into());
        }
        Ok(Self {basis:Arc::new(Basis::Helix {origin,x,y,rise,turns,radial_change}),interval:[0.,1.],approximate:false})
    }
    fn native(&self,t:f64)->R<f64> {
        check_t(t,0.,1.)?;
        Ok(if t==0. {self.interval[0]} else if t==1. {self.interval[1]}
            else {(1.-t)*self.interval[0]+t*self.interval[1]})
    }
    pub fn point(&self,t:f64)->R<Point3> {
        let u=self.native(t)?;
        match self.basis.as_ref() {
            Basis::Nurbs(c)=>c.point(u),
            Basis::Composite{parts,..}=>{let (part,t)=Self::composite_parameter(parts,u);part.point(t)},
            Basis::Helix {origin,x,y,rise,turns,radial_change}=>{
                let (s,c)=(TAU*turns*u).sin_cos();
                checked(add(add(*origin,scale(*rise,u)),scale(add(scale(*x,c),scale(*y,s)),1.+radial_change*u)))
            }
        }
    }
    /// Derivatives with respect to normalized parameter, including reversed intervals.
    pub fn derivatives(&self,t:f64)->R<[Point3;3]> {
        let u=self.native(t)?;
        let [p,d,dd]=match self.basis.as_ref() {
            Basis::Nurbs(c)=>c.derivatives(u)?,
            Basis::Composite{parts,..}=>{
                let (part,t)=Self::composite_parameter(parts,u);
                let [p,d,dd]=part.derivatives(t)?;
                let n=parts.len() as f64;
                [p,scale(d,n),scale(dd,n*n)]
            },
            Basis::Helix {origin,x,y,rise,turns,radial_change}=>{
                let w=TAU*turns; let (s,c)=(w*u).sin_cos();
                let r=1.+radial_change*u;
                let q=add(scale(*x,c),scale(*y,s));
                let q1=scale(add(scale(*x,-s),scale(*y,c)),w);
                [add(add(*origin,scale(*rise,u)),scale(q,r)),
                 add(*rise,add(scale(q,*radial_change),scale(q1,r))),
                 add(scale(q1,2.*radial_change),scale(q,-r*w*w))]
            }
        };
        let d_u=self.interval[1]-self.interval[0];
        Ok([checked(p)?,checked(scale(d,d_u))?,checked(scale(dd,d_u*d_u))?])
    }
    pub fn tangent(&self,t:f64)->R<Point3> {unit(self.derivatives(t)?[1])}
    pub fn curvature(&self,t:f64)->R<f64> {
        let [_,d,dd]=self.derivatives(t)?; let speed=norm(d);
        if speed==0.||!speed.is_finite() {return Err("停留点の曲率は定義できません。".into());}
        let k=norm(cross(unit(d)?,scale(dd,1./speed)))/speed;
        if k.is_finite(){Ok(k)}else{Err("曲率が有限ではありません。".into())}
    }
    pub fn closed(&self)->R<bool> {
        let a=self.point(0.)?;let b=self.point(1.)?;
        if a==b {return Ok(true);}
        let scale=norm(self.derivatives(0.)?[1]).max(norm(self.derivatives(1.)?[1]));
        Ok(scale.is_finite() && scale>0. && norm(sub(a,b))<=1e-12*scale)
    }
    pub fn subcurve(&self,start:f64,end:f64)->R<Self> {
        let a=self.native(start)?; let b=self.native(end)?;
        if a==b {return Err("切り出し区間の長さがゼロです。".into());}
        Ok(Self {basis:self.basis.clone(),interval:[a,b],approximate:self.approximate})
    }
    pub fn reversed(&self)->Self {
        Self {basis:self.basis.clone(),interval:[self.interval[1],self.interval[0]],approximate:self.approximate}
    }
    pub fn affine(&self,f:impl Fn(Point3)->Point3)->R<Self> {self.affine_impl(&f)}
    fn affine_impl(&self,f:&dyn Fn(Point3)->Point3)->R<Self>{
        let basis=match self.basis.as_ref() {
            Basis::Nurbs(n)=>Basis::Nurbs(n.affine(f)?),
            Basis::Composite{parts,depth}=>Basis::Composite{parts:parts.iter().map(|p|p.affine_impl(f)).collect::<R<Vec<_>>>()?,depth:*depth},
            Basis::Helix {origin,x,y,rise,turns,radial_change}=>{
                let o=checked(f(*origin))?;
                Basis::Helix {origin:o,x:checked(sub(f(add(*origin,*x)),o))?,
                    y:checked(sub(f(add(*origin,*y)),o))?,rise:checked(sub(f(add(*origin,*rise)),o))?,
                    turns:*turns,radial_change:*radial_change}
            }
        };
        Ok(Self {basis:Arc::new(basis),interval:self.interval,approximate:self.approximate})
    }
    pub fn weight(&self)->usize {
        match self.basis.as_ref() {
            Basis::Nurbs(n)=>n.h.len()*5+n.knots.len(),
            Basis::Composite{parts,..}=>parts.iter().map(Self::weight).fold(parts.len(),usize::saturating_add),
            _=>16,
        }
    }
    /// Mandatory sampling breakpoints, including all active knot spans and sufficiently short
    /// helix intervals. The returned numbers are in normalized, increasing traversal order.
    pub fn breaks(&self)->Vec<f64> {
        let [a,b]=self.interval;
        let mut out=vec![0.,1.];
        match self.basis.as_ref() {
            Basis::Nurbs(c)=>for &k in c.knots() {
                if k>a.min(b)&&k<a.max(b) {out.push((k-a)/(b-a));}
            },
            Basis::Composite{parts,..}=>{
                let count=parts.len() as f64;
                for (i,p) in parts.iter().enumerate(){
                    for t in p.breaks(){
                        let k=(i as f64+t)/count;
                        if k>a.min(b)&&k<a.max(b){out.push(((k-a)/(b-a)).clamp(0.,1.));}
                    }
                }
            },
            Basis::Helix {turns,..}=>{
                let n=(turns.abs()*(b-a).abs()*8.).ceil().max(1.) as usize;
                out.extend((1..n).map(|i|i as f64/n as f64));
            }
        }
        out.sort_by(f64::total_cmp); out.dedup();
        out
    }
    /// Bounded display sampling, not a certified chord-error bound. Knots are included so
    /// degree-one corners are never rounded off. Closed output omits the duplicate seam.
    pub fn preview(&self,segments:usize,cancel:&dyn Fn()->bool)->R<Vec<Point3>> {
        if !(1..=MAX_PREVIEW).contains(&segments) {return Err("表示分割数が範囲外です。".into());}
        let mut t=self.breaks();
        t.extend((0..=segments).map(|i|i as f64/segments as f64));
        t.sort_by(f64::total_cmp); t.dedup();
        if t.len()>MAX_CONTROLS+MAX_PREVIEW {return Err("表示点が多すぎます。".into());}
        if self.closed()? {t.pop();}
        t.into_iter().map(|u|{stopped(cancel)?;self.point(u)}).collect()
    }
    pub fn length_table(&self,cancel:&dyn Fn()->bool)->R<LengthTable> {
        let mut table=LengthTable {leaves:vec![],total:0.};
        let mut work=0;
        let breaks=self.breaks();
        if breaks.len()>MAX_CONTROLS+MAX_PREVIEW{return Err("弧長の区間数が多すぎます。".into());}
        for w in breaks.windows(2) {
            stopped(cancel)?;
            let whole=self.gauss(w[0],w[1],&mut work)?;
            self.refine(w[0],w[1],whole,0,&mut work,&mut table,cancel)?;
        }
        Ok(table)
    }
    fn gauss(&self,a:f64,b:f64,work:&mut usize)->R<f64> {
        *work+=5;
        if *work>MAX_QUAD_WORK {return Err("弧長計算の予算を超えました。".into());}
        // Five point Gauss-Legendre quadrature; never evaluates exactly at a kink.
        let mid=(a+b)/2.; let half=(b-a)/2.;
        let mut result=0.;
        for (x,w) in [(0.,0.5688888888888889),(-0.5384693101056831,0.4786286704993665),
            (0.5384693101056831,0.4786286704993665),(-0.906179845938664,0.2369268850561891),
            (0.906179845938664,0.2369268850561891)] {
            result+=w*norm(self.derivatives(mid+half*x)?[1]);
        }
        let result=result*half;
        if result.is_finite(){Ok(result)}else{Err("弧長が有限ではありません。".into())}
    }
    fn refine(&self,a:f64,b:f64,whole:f64,depth:usize,work:&mut usize,
        table:&mut LengthTable,cancel:&dyn Fn()->bool)->R<()> {
        stopped(cancel)?;
        let m=(a+b)/2.;
        let left=self.gauss(a,m,work)?; let right=self.gauss(m,b,work)?;
        let sum=left+right;
        if (sum-whole).abs()<=1e-9*(b-a)+1e-9*sum {
            for (lo,hi,len) in [(a,m,left),(m,b,right)] {
                table.total+=len;
                if !table.total.is_finite(){return Err("弧長が大きすぎます。".into());}
                table.leaves.push(LengthLeaf {lo,hi,len,end:table.total});
            }
            Ok(())
        } else if depth==12 {
            Err("弧長計算が収束しません。曲線を分割してください。".into())
        } else {
            self.refine(a,m,left,depth+1,work,table,cancel)?;
            self.refine(m,b,right,depth+1,work,table,cancel)
        }
    }
    /// Approximate nearest parameter: sample all spans, refine local minima in each sampled
    /// bracket. Not a certified global minimizer for arbitrarily oscillatory curves.
    pub fn closest_parameter(&self,p:Point3,cancel:&dyn Fn()->bool)->R<f64> {
        if !finite(p){return Err("点の座標が無効です。".into());}
        let mut ts=self.breaks();
        if ts.len()>MAX_CONTROLS+MAX_PREVIEW{return Err("最近接点の区間数が多すぎます。".into());}
        ts.extend((0..=128).map(|i|i as f64/128.));
        ts.sort_by(f64::total_cmp);ts.dedup();
        let dist=|t|->R<f64>{Ok(norm(sub(self.point(t)?,p)))};
        let ds=ts.iter().map(|&t|{stopped(cancel)?;dist(t)}).collect::<R<Vec<_>>>()?;
        let mut best=(ts[0],ds[0]);
        for (i,&d) in ds.iter().enumerate() {
            if d<best.1 {best=(ts[i],d);}
            if i==0||i+1==ts.len()||d>ds[i-1]||d>ds[i+1]||d==ds[i-1]&&d==ds[i+1] {continue;}
            let (mut a,mut b)=(ts[i-1],ts[i+1]);
            for _ in 0..36 {
                stopped(cancel)?;
                let x=a+(b-a)/3.;let y=b-(b-a)/3.;
                if dist(x)?<dist(y)? {b=y;} else {a=x;}
            }
            let t=(a+b)/2.; let d=dist(t)?;
            if d<best.1 {best=(t,d);}
        }
        Ok(best.0)
    }
}

#[derive(Clone,Debug)]
struct LengthLeaf {lo:f64,hi:f64,len:f64,end:f64}
#[derive(Clone,Debug)]
pub struct LengthTable {leaves:Vec<LengthLeaf>,total:f64}
impl LengthTable {
    pub fn total(&self)->f64 {self.total}
    /// Invert arc length using quadrature on the corresponding adaptive interval.
    /// Call with the same geometry used to create this table.
    pub fn parameter(&self,curve:&CurveGeometry,s:f64,cancel:&dyn Fn()->bool)->R<f64> {
        check_t(s,0.,self.total)?;
        if s==0. {return Ok(0.);}
        if s==self.total {return Ok(1.);}
        let i=self.leaves.partition_point(|l| l.end<s);
        let leaf=self.leaves.get(i).ok_or("弧長の区間がありません。")?;
        let target=s-(leaf.end-leaf.len);
        let mut lo=leaf.lo;let mut hi=leaf.hi;let mut work=0;
        for _ in 0..42 {
            stopped(cancel)?;
            let mid=(lo+hi)/2.;
            if curve.gauss(leaf.lo,mid,&mut work)?<target {lo=mid;} else {hi=mid;}
        }
        Ok((lo+hi)/2.)
    }
}
