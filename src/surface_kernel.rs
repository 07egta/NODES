//! Mathematical surfaces and a deliberately restricted, planar B-rep.
//! No external kernel. A tessellation is a derived Mesh, not a solid.
use crate::model::{Curve, Mesh};
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

pub type Point3 = [f64; 3];
type R<T> = Result<T, String>;
const MAX_CONTROL: usize = 32;
const MAX_DEGREE: usize = 8;
const MIN_SIZE: f64 = 1e-3;
const MAX_SIZE: f64 = 1e6;

fn fail<T>(msg: &str) -> R<T> { Err(msg.into()) }
fn finite(p: Point3) -> bool { p.iter().all(|x| x.is_finite() && x.abs() <= 1e9) }
fn add(a: Point3, b: Point3) -> Point3 { std::array::from_fn(|i| a[i]+b[i]) }
fn sub(a: Point3, b: Point3) -> Point3 { std::array::from_fn(|i| a[i]-b[i]) }
fn mul(a: Point3, t: f64) -> Point3 { a.map(|x| x*t) }
fn dot(a: Point3,b: Point3)->f64 { a.iter().zip(b).map(|(x,y)| x*y).sum() }
fn cross(a: Point3,b: Point3)->Point3 {
    [a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]
}
fn norm(a: Point3)->f64 { dot(a,a).sqrt() }
fn size(x: f64)->R<f64> {
    if (MIN_SIZE..=MAX_SIZE).contains(&x) { Ok(x) }
    else { fail("寸法は 0.001〜1000000 の有限値にしてください。") }
}
fn uv(t: f64)->R<f64> {
    if (0.0..=1.0).contains(&t) { Ok(t) } else { fail("U/V は 0〜1 の範囲です。") }
}
fn check_cancel(stop: &dyn Fn()->bool)->R<()> {
    if stop() { fail("計算を停止しました。") } else { Ok(()) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample { pub point: Point3, pub du: Point3, pub dv: Point3 }
impl Sample {
    pub fn normal(self)->R<Point3> {
        let n = cross(self.du,self.dv);
        let len=norm(n);
        let scale=norm(self.du)*norm(self.dv);
        if !len.is_finite() || !scale.is_finite() || scale==0. || len<=scale*1e-12 {
            return fail("曲面の接線が特異なため法線を決定できません。");
        }
        Ok(mul(n,1./len))
    }
    fn checked(self)->R<Self> {
        if finite(self.point) && self.du.iter().chain(&self.dv).all(|x| x.is_finite()) { Ok(self) }
        else { fail("曲面評価が有限値になりません。") }
    }
}

/// Clamped nonperiodic tensor-product NURBS. U-major grid: index = i * nv + j.
/// Knot domains are native; public evaluation uses normalized UV [0,1].
/// Positive rational weights only. Inner knot multiplicity <= degree.
#[derive(Clone, Debug, PartialEq)]
pub struct NurbsSurface {
    nu: usize, nv: usize, pu: usize, pv: usize,
    ku: Vec<f64>, kv: Vec<f64>, points: Vec<Point3>, weights: Vec<f64>,
    closed_u: bool,
}
fn check_knots(n: usize, p: usize, k: &[f64])->R<()> {
    if p==0 || p>MAX_DEGREE || n<=p || n>MAX_CONTROL || k.len()!=n+p+1
        || k.iter().any(|x| !x.is_finite() || x.abs()>1e9)
        || k.windows(2).any(|x| x[0]>x[1]) {
        return fail("NURBS の次数・節点・制御点数が無効です。");
    }
    let a=k[p]; let b=k[n];
    if b-a<1e-9 || k[..=p].iter().any(|x| *x!=a)
        || k[n..].iter().any(|x| *x!=b)
        || k[p+1]<=a || k[n-1]>=b {
        return fail("クランプされた非周期節点ベクトルが必要です。");
    }
    let mut run=1;
    for i in p+2..n {
        run=if k[i]==k[i-1] { run+1 } else { 1 };
        if run>p { return fail("内部節点の多重度が次数を超えています。"); }
    }
    Ok(())
}

/// Cox-de Boor basis and analytical first derivative. At the right endpoint use
/// the interval on its left. At an interior knot choose the right-hand derivative.
fn basis(k: &[f64], n: usize, p: usize, t: f64)->(Vec<f64>,Vec<f64>) {
    let mut b=vec![0.;k.len()-1];
    let span=if t==k[n] { n-1 } else {
        (p..n).find(|&i| k[i]<=t && t<k[i+1]).unwrap_or(p)
    };
    b[span]=1.;
    let mut lower=Vec::new();
    for d in 1..=p {
        if d==p { lower=b.clone(); }
        let mut next=vec![0.;b.len()-1];
        for i in 0..next.len() {
            let l=k[i+d]-k[i];
            let r=k[i+d+1]-k[i+1];
            if l>0. { next[i]+=(t-k[i])*b[i]/l; }
            if r>0. { next[i]+=(k[i+d+1]-t)*b[i+1]/r; }
        }
        b=next;
    }
    let d=(0..n).map(|i| {
        let l=k[i+p]-k[i]; let r=k[i+p+1]-k[i+1];
        let a=if l>0. { p as f64*lower[i]/l } else { 0. };
        let z=if r>0. { p as f64*lower[i+1]/r } else { 0. };
        a-z
    }).collect();
    (b,d)
}
impl NurbsSurface {
    #[allow(clippy::too_many_arguments)]
    pub fn new(nu:usize,nv:usize,pu:usize,pv:usize,ku:Vec<f64>,kv:Vec<f64>,
               points:Vec<Point3>,weights:Vec<f64>)->R<Self> {
        check_knots(nu,pu,&ku)?; check_knots(nv,pv,&kv)?;
        if points.len()!=nu*nv || weights.len()!=points.len() || points.iter().any(|p| !finite(*p))
            || weights.iter().any(|w| !(1e-6..=1e6).contains(w)) {
            return fail("NURBS の制御格子または正の重みが無効です。");
        }
        Ok(Self{nu,nv,pu,pv,ku,kv,points,weights,closed_u:false})
    }
    pub fn bilinear(p00:Point3,p01:Point3,p10:Point3,p11:Point3)->R<Self> {
        Self::new(2,2,1,1,vec![0.,0.,1.,1.],vec![0.,0.,1.,1.],
                  vec![p00,p01,p10,p11],vec![1.;4])
    }
    pub fn panel(width:f64,depth:f64,rise:f64)->R<Self> {
        size(width)?;size(depth)?;
        if !rise.is_finite() || rise.abs()>MAX_SIZE { return fail("曲面の高さが無効です。"); }
        let mut p=Vec::new();
        for i in 0..3 { for j in 0..3 {
            p.push([width*(i as f64/2.-0.5),depth*(j as f64/2.-0.5),
                if i==1 && j==1 {rise} else {0.}]);
        }}
        Self::new(3,3,2,2,vec![0.,0.,0.,1.,1.,1.],vec![0.,0.,0.,1.,1.,1.],p,vec![1.;9])
    }
    /// Revolve a quadratic Bezier radius/height profile about Z.
    /// The U circle is exact rational quadratic (four quarter arcs).
    /// The middle radius is a control value, not the radius at mid-height.
    pub fn vase(r0:f64,rm:f64,r1:f64,h:f64)->R<Self>{
        for r in [r0,rm,r1,h]{size(r)?;}
        let ring=[[1.,0.],[1.,1.],[0.,1.],[-1.,1.],[-1.,0.],[-1.,-1.],[0.,-1.],[1.,-1.],[1.,0.]];
        let mut points=Vec::new();let mut weights=Vec::new();
        for (i,[x,y]) in ring.into_iter().enumerate(){
            for (r,z) in [(r0,0.),(rm,h/2.),(r1,h)] {
                points.push([r*x,r*y,z]);
                weights.push(if i%2==0 {1.} else {0.5f64.sqrt()});
            }
        }
        let mut s=Self::new(9,3,2,2,vec![0.,0.,0.,0.25,0.25,0.5,0.5,0.75,0.75,1.,1.,1.],
            vec![0.,0.,0.,1.,1.,1.],points,weights)?;
        s.closed_u=true;
        Ok(s)
    }
    /// Linear interpolation between two matching polyline control sections.
    /// Piecewise linear in U, linear in V. No smooth loft is implied.
    pub fn ruled(a:&Curve,b:&Curve,reverse:bool)->R<Self>{
        let n=a.points.len();
        if a.closed!=b.closed||n!=b.points.len()||n<2||n+usize::from(a.closed)>MAX_CONTROL
            || (a.closed && n<3) {
            return fail("同じ点数・開閉状態の曲線2本が必要です（開2〜32点、閉3〜31点）。");
        }
        let mut right=b.points.clone();
        if reverse {
            if b.closed { right[1..].reverse(); } else { right.reverse(); }
        }
        for points in [&a.points,&right] {
            if points.windows(2).any(|p|norm(sub(p[1],p[0]))<1e-9)
                || (a.closed && norm(sub(points[0],points[n-1]))<1e-9) {
                return fail("断面に重複する隣接点があります。");
            }
        }
        let nu=n+usize::from(a.closed);
        let mut points=Vec::with_capacity(nu*2);
        for i in 0..nu {points.push(a.points[i%n]);points.push(right[i%n]);}
        let mut ku=vec![0.,0.];
        for i in 1..nu-1 {ku.push(i as f64/(nu-1) as f64);}
        ku.extend([1.,1.]);
        let mut s=Self::new(nu,2,1,1,ku,vec![0.,0.,1.,1.],points,vec![1.;nu*2])?;
        s.closed_u=a.closed;
        Ok(s)
    }
    pub fn native_domain(&self)->[[f64;2];2] {
        [[self.ku[self.pu],self.ku[self.nu]],[self.kv[self.pv],self.kv[self.nv]]]
    }
    pub fn evaluate(&self,u:f64,v:f64)->R<Sample> {
        let (u,v)=(uv(u)?,uv(v)?);
        let [[a,b],[c,d]]=self.native_domain();
        let tu=if u==1. {b} else {a+(b-a)*u};
        let tv=if v==1. {d} else {c+(d-c)*v};
        let (bu,du)=basis(&self.ku,self.nu,self.pu,tu);
        let (bv,dv)=basis(&self.kv,self.nv,self.pv,tv);
        let (mut h,mut hu,mut hv)=([0.;4],[0.;4],[0.;4]);
        for i in 0..self.nu { for j in 0..self.nv {
            let k=i*self.nv+j; let w=self.weights[k];
            let p=self.points[k]; let q=[p[0]*w,p[1]*w,p[2]*w,w];
            let x=bu[i]*bv[j];let y=du[i]*bv[j]*(b-a);let z=bu[i]*dv[j]*(d-c);
            for s in 0..4 { h[s]+=x*q[s];hu[s]+=y*q[s];hv[s]+=z*q[s]; }
        }}
        if h[3]<=0. || !h[3].is_finite() { return fail("NURBS の同次座標が無効です。"); }
        let point=std::array::from_fn(|i| h[i]/h[3]);
        let du=std::array::from_fn(|i| (hu[i]-point[i]*hu[3])/h[3]);
        let dv=std::array::from_fn(|i| (hv[i]-point[i]*hv[3])/h[3]);
        Sample{point,du,dv}.checked()
    }
}

/// Rectangular plane, including arbitrary non-collinear axes.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaneSurface { origin:Point3, u:Point3, v:Point3 }
impl PlaneSurface {
    pub fn new(origin:Point3,u:Point3,v:Point3)->R<Self> {
        if !finite(origin)||!finite(u)||!finite(v)||!finite(add(add(origin,u),v))
            || !finite(add(origin,u)) || !finite(add(origin,v))
            || norm(u)<MIN_SIZE || norm(v)<MIN_SIZE
            || norm(cross(u,v))<=norm(u)*norm(v)*1e-12 {
            return fail("平面の原点または方向が無効です。");
        }
        Ok(Self{origin,u,v})
    }
    pub fn xy(w:f64,d:f64)->R<Self> {
        Self::new([-size(w)?/2.,-size(d)?/2.,0.],[w,0.,0.],[0.,d,0.])
    }
    pub fn evaluate(&self,u:f64,v:f64)->R<Sample> {
        Sample{point:add(self.origin,add(mul(self.u,uv(u)?),mul(self.v,uv(v)?))),
            du:self.u,dv:self.v}.checked()
    }
    pub fn area(&self)->f64 { norm(cross(self.u,self.v)) }
    /// Exact constrained least-squares solution for a planar parallelogram.
    pub fn closest(&self,p:Point3)->R<Point3> {
        if !finite(p) {return fail("点の座標が無効です。");}
        let q=sub(p,self.origin); let a=dot(self.u,self.u);let b=dot(self.u,self.v);
        let c=dot(self.v,self.v);let d=dot(q,self.u);let e=dot(q,self.v);
        let det=norm(cross(self.u,self.v)).powi(2);
        let (x,y)=((c*d-b*e)/det,(a*e-b*d)/det);
        let mut candidates=vec![(0.,(e/c).clamp(0.,1.)),(1.,((e-b)/c).clamp(0.,1.)),
            ((d/a).clamp(0.,1.),0.),(((d-b)/a).clamp(0.,1.),1.)];
        if (0.0..=1.0).contains(&x)&&(0.0..=1.0).contains(&y) {candidates.push((x,y));}
        let mut best=None;
        for (u,v) in candidates {
            let q=self.evaluate(u,v)?.point; let dist=norm(sub(q,p));
            if best.is_none_or(|(d,_)|dist<d) {best=Some((dist,q));}
        }
        best.map(|(_,p)|p).ok_or_else(||"最近点を決定できません。".into())
    }
}

#[derive(Clone,Debug,PartialEq)]
enum Geometry { Plane(PlaneSurface), Nurbs(NurbsSurface),
    Frustum{r0:f64,r1:f64,h:f64}, Sphere{r:f64}, Torus{major:f64,minor:f64} }
#[derive(Clone,Debug,PartialEq)]
pub struct Surface(Geometry);
impl Surface {
    pub fn plane(w:f64,d:f64)->R<Self>{Ok(Self(Geometry::Plane(PlaneSurface::xy(w,d)?)))}
    pub fn from_plane(p:PlaneSurface)->Self{Self(Geometry::Plane(p))}
    pub fn nurbs(n:NurbsSurface)->Self{Self(Geometry::Nurbs(n))}
    pub fn frustum(r0:f64,r1:f64,h:f64)->R<Self>{
        Ok(Self(Geometry::Frustum{r0:size(r0)?,r1:size(r1)?,h:size(h)?}))
    }
    pub fn sphere(r:f64)->R<Self>{Ok(Self(Geometry::Sphere{r:size(r)?}))}
    pub fn torus(major:f64,minor:f64)->R<Self>{
        size(major)?;size(minor)?;
        if major<=minor {return fail("大半径は小半径より大きくしてください。");}
        Ok(Self(Geometry::Torus{major,minor}))
    }
    pub fn name(&self)->&'static str {
        match &self.0 {Geometry::Plane(_)=>"Plane",Geometry::Nurbs(_)=>"NURBS",
            Geometry::Frustum{..}=>"Cylinder / Frustum",Geometry::Sphere{..}=>"Sphere",
            Geometry::Torus{..}=>"Torus"}
    }
    pub fn domain(&self)->[[f64;2];2] {[[0.,1.],[0.,1.]]}
    fn periodic(&self)->(bool,bool) {
        match &self.0 {Geometry::Frustum{..}|Geometry::Sphere{..}=>(true,false),
            Geometry::Torus{..}=>(true,true),Geometry::Nurbs(n)=>(n.closed_u,false),_=>(false,false)}
    }
    pub fn evaluate(&self,u:f64,v:f64)->R<Sample>{
        let (u,v)=(uv(u)?,uv(v)?);
        let t=if u==1. {0.} else {TAU*u};
        let (s,c)=t.sin_cos();
        let sample=match &self.0 {
            Geometry::Plane(p)=>return p.evaluate(u,v),
            Geometry::Nurbs(n)=>return n.evaluate(u,v),
            Geometry::Frustum{r0,r1,h}=>{
                let r=r0+(r1-r0)*v;
                Sample{point:[r*c,r*s,h*v],du:[-TAU*r*s,TAU*r*c,0.],
                    dv:[(r1-r0)*c,(r1-r0)*s,*h]}
            }
            Geometry::Sphere{r}=>{
                let (sl,cl)=(PI*(v-0.5)).sin_cos();
                Sample{point:[r*cl*c,r*cl*s,r*sl],du:[-TAU*r*cl*s,TAU*r*cl*c,0.],
                    dv:[-PI*r*sl*c,-PI*r*sl*s,PI*r*cl]}
            }
            Geometry::Torus{major,minor}=>{
                let (sv,cv)=(if v==1. {0.}else{TAU*v}).sin_cos();let r=major+minor*cv;
                Sample{point:[r*c,r*s,minor*sv],du:[-TAU*r*s,TAU*r*c,0.],
                    dv:[-TAU*minor*sv*c,-TAU*minor*sv*s,TAU*minor*cv]}
            }
        };
        sample.checked()
    }
    pub fn normal(&self,u:f64,v:f64)->R<Point3> {
        let s=self.evaluate(u,v)?;
        if let Geometry::Sphere{r}=&self.0 { return Ok(mul(s.point,1./r)); }
        s.normal()
    }
    pub fn closest_plane_point(&self,p:Point3)->R<Point3> {
        match &self.0 {Geometry::Plane(plane)=>plane.closest(p),
            _=>fail("最近点は現在、平面曲面だけに対応しています。")}
    }
    /// Analytic primitives have analytic area. NURBS uses bounded midpoint
    /// integration of |Su x Sv|; this is an estimate, not an error certificate.
    pub fn area(&self,steps:usize,stop:&dyn Fn()->bool)->R<f64>{
        check_cancel(stop)?;
        let area=match &self.0{
            Geometry::Plane(p)=>p.area(),
            Geometry::Frustum{r0,r1,h}=>PI*(r0+r1)*((r1-r0).powi(2)+h*h).sqrt(),
            Geometry::Sphere{r}=>4.*PI*r*r,
            Geometry::Torus{major,minor}=>4.*PI*PI*major*minor,
            Geometry::Nurbs(_)=>{
                if !(4..=128).contains(&steps) {return fail("面積積分の分割数は4〜128です。");}
                let mut area=0.;
                for i in 0..steps {check_cancel(stop)?;for j in 0..steps{
                    let s=self.evaluate((i as f64+0.5)/steps as f64,(j as f64+0.5)/steps as f64)?;
                    area+=norm(cross(s.du,s.dv));
                }}
                area/(steps*steps) as f64
            }
        };
        if area.is_finite()&&area>0. {Ok(area)}else{fail("曲面の面積を評価できません。")}
    }
    /// Explicit display/export approximation. Periodic directions share seam
    /// indices; sphere poles have a single vertex each, never zero-area pole fans.
    pub fn tessellate(&self,nu:usize,nv:usize,stop:&dyn Fn()->bool)->R<Mesh>{
        if !(4..=128).contains(&nu)||!(4..=128).contains(&nv) {
            return fail("曲面の分割数は4〜128です。");
        }
        check_cancel(stop)?;
        let (pu,pv)=self.periodic();
        if let Geometry::Sphere{r}=&self.0 {
            let mut m=Mesh{vertices:vec![[0.,0.,-*r]],triangles:vec![]};
            for j in 1..nv {check_cancel(stop)?;for i in 0..nu {
                m.vertices.push(self.evaluate(i as f64/nu as f64,j as f64/nv as f64)?.point);
            }}
            let top=m.vertices.len() as u32;m.vertices.push([0.,0.,*r]);
            let idx=|i:usize,j:usize| 1+((j-1)*nu+i%nu) as u32;
            for i in 0..nu {
                m.triangles.push([0,idx(i+1,1),idx(i,1)]);
                for j in 1..nv-1 {let a=idx(i,j);let b=idx(i+1,j);let c=idx(i+1,j+1);let d=idx(i,j+1);
                    m.triangles.extend([[a,b,c],[a,c,d]]);}
                m.triangles.push([idx(i,nv-1),idx(i+1,nv-1),top]);
            }
            crate::engine::validate_mesh(&m,stop)?;return Ok(m);
        }
        let (nu,nv)=if matches!(&self.0,Geometry::Plane(_)){(1,1)}else if let Geometry::Nurbs(n)=&self.0 {
            if n.pu==1 && n.pv==1 && n.nu>2 {
                let spans=n.nu-1; (spans*(nu/spans).max(1),nv)
            } else {(nu,nv)}
        }else{(nu,nv)};
        let (uc,vc)=(nu+usize::from(!pu),nv+usize::from(!pv));
        let mut m=Mesh::default();
        for j in 0..vc {check_cancel(stop)?;for i in 0..uc{
            m.vertices.push(self.evaluate(i as f64/nu as f64,j as f64/nv as f64)?.point);
        }}
        let idx=|i:usize,j:usize| ((j%vc)*uc+i%uc) as u32;
        for j in 0..nv {check_cancel(stop)?;for i in 0..nu {
            let a=idx(i,j);let b=idx(i+1,j);let c=idx(i+1,j+1);let d=idx(i,j+1);
            m.triangles.extend([[a,b,c],[a,c,d]]);
        }}
        crate::engine::validate_mesh(&m,stop)?;Ok(m)
    }
    /// Approximate isocurve for the existing polyline Curve type.
    pub fn iso_polyline(&self,constant_u:bool,t:f64,steps:usize,stop:&dyn Fn()->bool)->R<Curve>{
        uv(t)?;if !(4..=512).contains(&steps){return fail("等値曲線の分割数は4〜512です。");}
        let (pu,pv)=self.periodic();let closed=if constant_u {pv}else{pu};
        let mut points=Vec::new();
        for i in 0..(steps+usize::from(!closed)) {
            check_cancel(stop)?;let s=i as f64/steps as f64;
            points.push(if constant_u{self.evaluate(t,s)?}else{self.evaluate(s,t)?}.point);
        }
        if points.windows(2).all(|p|norm(sub(p[0],p[1]))<=1e-12) {
            return fail("特異点で等値曲線が一点に縮退します。");
        }
        Ok(Curve{points,closed})
    }
    fn weight(&self)->usize {
        match &self.0 {Geometry::Nurbs(n)=>n.points.len()*4+n.ku.len()+n.kv.len(),_=>16}
    }
}

/// Worker-produced preview is deliberately separate from the mathematical geometry.
#[derive(Clone,Debug,PartialEq)]
pub struct SurfaceValue { geometry:Surface, preview:Arc<Mesh> }
impl SurfaceValue {
    pub fn new(geometry:Surface,stop:&dyn Fn()->bool)->R<Self>{
        let preview=Arc::new(geometry.tessellate(24,24,stop)?);
        Ok(Self{geometry,preview})
    }
    pub fn geometry(&self)->&Surface{&self.geometry}
    pub fn preview(&self)->&Arc<Mesh>{&self.preview}
    pub fn weight(&self)->usize{
        self.geometry.weight()+self.preview.vertices.len()+3*self.preview.triangles.len()
    }
}

/// Indexed topology. Edges are exact straight segments between shared vertices.
/// A face has an oriented rectangular UV trim and four directed shared edges.
/// This milestone does NOT expose arbitrary unchecked trim construction.
#[derive(Clone,Debug,PartialEq)]
pub struct Vertex { pub position:Point3 }
#[derive(Clone,Debug,PartialEq)]
pub struct Edge { pub vertices:[usize;2] }
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct EdgeUse { pub edge:usize,pub reversed:bool }
#[derive(Clone,Debug,PartialEq)]
pub struct TrimmedSurface { pub support:PlaneSurface,pub uv_loop:[[f64;2];4] }
#[derive(Clone,Debug,PartialEq)]
pub struct Face { pub surface:TrimmedSurface,pub boundary:[EdgeUse;4],pub vertices:[usize;4] }
#[derive(Clone,Debug,PartialEq)]
pub struct Shell { faces:Vec<Face> }
impl Shell { pub fn faces(&self)->&[Face]{&self.faces} }
#[derive(Clone,Debug,PartialEq)]
pub struct Brep { vertices:Vec<Vertex>,edges:Vec<Edge>,shell:Shell }
impl Brep {
    pub fn vertices(&self)->&[Vertex]{&self.vertices}
    pub fn edges(&self)->&[Edge]{&self.edges}
    pub fn shell(&self)->&Shell{&self.shell}
    /// Structural checks plus support/trim endpoint agreement and outward orientation.
    /// This validates the restricted box, not general self-intersections.
    fn validate_box(&self,center:Point3)->R<()>{
        if self.vertices.len()!=8||self.edges.len()!=12||self.shell.faces.len()!=6 {
            return fail("箱のトポロジー数が無効です。");
        }
        let mut incidence=vec![(0u32,0i32);self.edges.len()];
        for f in &self.shell.faces{
            for i in 0..4 {
                let e=f.boundary[i];
                let edge=self.edges.get(e.edge).ok_or("稜線参照が無効です。")?;
                let expected=if e.reversed {[edge.vertices[1],edge.vertices[0]]}else{edge.vertices};
                if expected!=[f.vertices[i],f.vertices[(i+1)%4]] {return fail("面境界が閉じていません。");}
                let vert=self.vertices.get(f.vertices[i]).ok_or("頂点参照が無効です。")?.position;
                let [u,v]=f.surface.uv_loop[i];
                if norm(sub(f.surface.support.evaluate(u,v)?.point,vert))>1e-6 {
                    return fail("曲面トリムと稜線端点が一致しません。");
                }
                incidence[e.edge].0+=1;incidence[e.edge].1+=if e.reversed{-1}else{1};
            }
            let s=f.surface.support.evaluate(0.5,0.5)?;
            if dot(s.normal()?,sub(s.point,center))<=0. {return fail("面の向きが外向きではありません。");}
        }
        if incidence.iter().any(|&(count,sum)|count!=2||sum!=0) {
            return fail("稜線が二つの逆向き面で共有されていません。");
        }
        Ok(())
    }
}
/// Only positive-size, axis-aligned closed boxes are supported; no public unchecked
/// constructor. The AABB is the exact geometric domain, not a mesh bounding guess.
#[derive(Clone,Debug,PartialEq)]
pub struct ClosedSolid { brep:Brep,min:Point3,max:Point3,preview:Arc<Mesh> }
impl ClosedSolid {
    pub fn centered_box(w:f64,d:f64,h:f64)->R<Self>{
        Self::box_bounds([-size(w)?/2.,-size(d)?/2.,0.],[w/2.,d/2.,size(h)?])
    }
    pub fn box_bounds(min:Point3,max:Point3)->R<Self>{
        if !finite(min)||!finite(max){return fail("箱の座標が無効です。");}
        for i in 0..3 {size(max[i]-min[i])?;}
        let [x,y,z]=min;let [a,b,c]=max;
        let positions=[[x,y,z],[a,y,z],[a,b,z],[x,b,z],[x,y,c],[a,y,c],[a,b,c],[x,b,c]];
        let loops=[[0,3,2,1],[4,5,6,7],[0,1,5,4],[1,2,6,5],[2,3,7,6],[3,0,4,7]];
        let mut edges=Vec::<Edge>::new();let mut index=BTreeMap::new();let mut faces=Vec::new();
        let mut triangles=Vec::new();
        for vs in loops {
            let mut boundary=[EdgeUse{edge:0,reversed:false};4];
            for i in 0..4{
                let a=vs[i];let b=vs[(i+1)%4];let key=(a.min(b),a.max(b));
                let edge=*index.entry(key).or_insert_with(||{
                    let id=edges.len();edges.push(Edge{vertices:[key.0,key.1]});id
                });
                boundary[i]=EdgeUse{edge,reversed:a>b};
            }
            let support=PlaneSurface::new(positions[vs[0]],sub(positions[vs[1]],positions[vs[0]]),
                sub(positions[vs[3]],positions[vs[0]]))?;
            faces.push(Face{surface:TrimmedSurface{support,uv_loop:[[0.,0.],[1.,0.],[1.,1.],[0.,1.]]},
                boundary,vertices:vs});
            triangles.extend([[vs[0] as u32,vs[1] as u32,vs[2] as u32],
                              [vs[0] as u32,vs[2] as u32,vs[3] as u32]]);
        }
        let brep=Brep{vertices:positions.into_iter().map(|position|Vertex{position}).collect(),
            edges,shell:Shell{faces}};
        brep.validate_box(mul(add(min,max),0.5))?;
        let mesh=Mesh{vertices:positions.to_vec(),triangles};
        crate::engine::validate_mesh(&mesh,&||false)?;
        Ok(Self{brep,min,max,preview:Arc::new(mesh)})
    }
    pub fn brep(&self)->&Brep{&self.brep}
    pub fn preview(&self)->&Arc<Mesh>{&self.preview}
    pub fn volume(&self)->f64{let [x,y,z]=sub(self.max,self.min);x*y*z}
    pub fn area(&self)->f64{let [x,y,z]=sub(self.max,self.min);2.*(x*y+y*z+z*x)}
    pub fn weight(&self)->usize{128+self.preview.vertices.len()+3*self.preview.triangles.len()}
    /// Only positive-volume intersections between these exact axis-aligned boxes.
    pub fn intersection(&self,other:&Self)->R<Self>{
        let lo=std::array::from_fn(|i|self.min[i].max(other.min[i]));
        let hi=std::array::from_fn(|i|self.max[i].min(other.max[i]));
        if (0..3).any(|i|hi[i]-lo[i]<MIN_SIZE) {
            return fail("体積のある交差がないか、許容最小寸法を下回っています。");
        }
        Self::box_bounds(lo,hi)
    }
    fn contains(&self,other:&Self)->bool{
        (0..3).all(|i|self.min[i]<=other.min[i]&&self.max[i]>=other.max[i])
    }
    /// The union is supported only when it is itself one rectangular box.
    pub fn union_box(&self,other:&Self)->R<Self>{
        if self.contains(other){return Ok(self.clone());}
        if other.contains(self){return Ok(other.clone());}
        for axis in 0..3 {
            if (0..3).filter(|i|*i!=axis).all(|i|self.min[i]==other.min[i]&&self.max[i]==other.max[i])
                && self.max[axis]>=other.min[axis]&&other.max[axis]>=self.min[axis] {
                let lo=std::array::from_fn(|i|self.min[i].min(other.min[i]));
                let hi=std::array::from_fn(|i|self.max[i].max(other.max[i]));
                return Self::box_bounds(lo,hi);
            }
        }
        fail("結合結果が単一の直方体になる場合のみ対応しています。")
    }
    /// Difference is supported only for a single surviving rectangular slab,
    /// including unchanged disjoint input. Cavities and multiple pieces fail.
    pub fn difference_box(&self,other:&Self)->R<Self>{
        if (0..3).any(|i| self.max[i]<=other.min[i]||other.max[i]<=self.min[i]) {
            return Ok(self.clone());
        }
        for axis in 0..3 {
            if !(0..3).filter(|i|*i!=axis).all(|i|other.min[i]<=self.min[i]&&other.max[i]>=self.max[i]) {
                continue;
            }
            let (mut lo,mut hi)=(self.min,self.max);
            if other.min[axis]<=lo[axis]&&other.max[axis]<hi[axis] {
                lo[axis]=other.max[axis];return Self::box_bounds(lo,hi);
            }
            if other.max[axis]>=hi[axis]&&other.min[axis]>lo[axis] {
                hi[axis]=other.min[axis];return Self::box_bounds(lo,hi);
            }
        }
        fail("差が単一の直方体となる場合のみ対応しています。空・穴・分離形状は未対応です。")
    }
    /// Parallel offset of each of the six supporting planes.
    pub fn offset_box(&self,d:f64)->R<Self>{
        if !d.is_finite()||d.abs()>MAX_SIZE{return fail("オフセット距離が無効です。");}
        Self::box_bounds(self.min.map(|x|x-d),self.max.map(|x|x+d))
    }
    pub fn translated(&self,delta:Point3)->R<Self>{
        if !finite(delta){return fail("移動ベクトルが無効です。");}
        Self::box_bounds(add(self.min,delta),add(self.max,delta))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a:f64,b:f64){assert!((a-b).abs()<1e-8*(1.+b.abs()),"{a} vs {b}");}
    fn xyz(a:Point3,b:Point3){for i in 0..3{near(a[i],b[i]);}}
    #[test] fn bilinear_nonunit_knots_derivatives(){
        let n=NurbsSurface::new(2,2,1,1,vec![2.,2.,5.,5.],vec![-3.,-3.,7.,7.],
            vec![[0.,0.,0.],[0.,3.,0.],[2.,0.,0.],[2.,3.,0.]],vec![1.;4]).unwrap();
        for (u,v) in [(0.,0.),(0.2,0.7),(1.,1.)]{
            let s=n.evaluate(u,v).unwrap();xyz(s.point,[2.*u,3.*v,0.]);
            xyz(s.du,[2.,0.,0.]);xyz(s.dv,[0.,3.,0.]);xyz(s.normal().unwrap(),[0.,0.,1.]);
        }
        near(Surface::nurbs(n).area(8,&||false).unwrap(),6.);
    }
    #[test] fn rational_quarter_cylinder(){
        let k=2f64.sqrt()/2.;
        let n=NurbsSurface::new(3,2,2,1,vec![0.,0.,0.,1.,1.,1.],vec![0.,0.,1.,1.],
            vec![[1.,0.,0.],[1.,0.,2.],[1.,1.,0.],[1.,1.,2.],[0.,1.,0.],[0.,1.,2.]],
            vec![1.,1.,k,k,1.,1.]).unwrap();
        let s=n.evaluate(0.5,0.25).unwrap();xyz(s.point,[k,k,0.5]);
        xyz(s.normal().unwrap(),[k,k,0.]);xyz(s.dv,[0.,0.,2.]);
        for i in 0..=20{let p=n.evaluate(i as f64/20.,0.4).unwrap().point;near(p[0]*p[0]+p[1]*p[1],1.);}
    }
    #[test] fn multi_span_endpoint_and_internal_knot(){
        let n=NurbsSurface::new(4,2,2,1,vec![0.,0.,0.,0.5,1.,1.,1.],vec![0.,0.,1.,1.],
            (0..4).flat_map(|i|[[i as f64,0.,0.],[i as f64,2.,0.]]).collect(),vec![1.;8]).unwrap();
        xyz(n.evaluate(0.,0.).unwrap().point,[0.,0.,0.]);
        xyz(n.evaluate(1.,1.).unwrap().point,[3.,2.,0.]);
        xyz(n.evaluate(0.5,0.5).unwrap().point,[1.5,1.,0.]);
        xyz(n.evaluate(0.5,0.5).unwrap().du,[2.,0.,0.]);
    }
    #[test] fn invalid_input_and_cancellation(){
        assert!(NurbsSurface::new(2,2,1,1,vec![0.,0.,1.,1.],vec![0.,0.,1.,1.],
            vec![[0.;3];4],vec![0.;4]).is_err());
        assert!(Surface::torus(1.,1.).is_err());assert!(Surface::sphere(f64::NAN).is_err());
        let s=Surface::plane(2.,3.).unwrap();
        assert!(s.evaluate(-0.1,0.).is_err());assert!(s.tessellate(129,4,&||false).is_err());
        assert!(s.tessellate(4,4,&||true).is_err());
    }
    #[test] fn analytic_areas_normals_and_closest(){
        let c=Surface::frustum(2.,2.,3.).unwrap();
        near(c.area(8,&||false).unwrap(),12.*PI);xyz(c.normal(0.,0.5).unwrap(),[1.,0.,0.]);
        near(Surface::frustum(2.,1.,3.).unwrap().area(8,&||false).unwrap(),3.*PI*10f64.sqrt());
        let s=Surface::sphere(2.).unwrap();near(s.area(8,&||false).unwrap(),16.*PI);
        xyz(s.normal(0.3,0.).unwrap(),[0.,0.,-1.]);
        let t=Surface::torus(5.,2.).unwrap();near(t.area(8,&||false).unwrap(),40.*PI*PI);
        xyz(t.normal(0.,0.).unwrap(),[1.,0.,0.]);
        let plane=Surface::plane(2.,4.).unwrap();
        xyz(plane.closest_plane_point([3.,1.,9.]).unwrap(),[1.,1.,0.]);
        assert!(s.closest_plane_point([0.;3]).is_err());
    }
    #[test] fn sphere_and_torus_meshes_closed_oriented(){
        for s in [Surface::sphere(2.).unwrap(),Surface::torus(5.,1.).unwrap()]{
            let m=s.tessellate(24,16,&||false).unwrap();
            let mut e=BTreeMap::<(u32,u32),(usize,i32)>::new();
            let mut volume=0.;
            for [a,b,c] in m.triangles{
                for (i,j) in [(a,b),(b,c),(c,a)]{
                    let v=e.entry((i.min(j),i.max(j))).or_default();v.0+=1;v.1+=if i<j{1}else{-1};
                }
                volume+=dot(m.vertices[a as usize],cross(m.vertices[b as usize],m.vertices[c as usize]))/6.;
            }
            assert!(e.values().all(|&v|v==(2,0)));assert!(volume>0.);
        }
    }
    #[test] fn exact_revolved_profile_and_ruled_sections(){
        let s=Surface::nurbs(NurbsSurface::vase(2.,6.,4.,8.).unwrap());
        for i in 0..=32{
            let p=s.evaluate(i as f64/32.,0.5).unwrap().point;
            near((p[0]*p[0]+p[1]*p[1]).sqrt(),4.5);
            near(p[2],4.);
        }
        xyz(s.evaluate(0.,0.5).unwrap().point,s.evaluate(1.,0.5).unwrap().point);
        let a=Curve{points:vec![[0.,0.,0.],[2.,0.,0.]],closed:false};
        let b=Curve{points:vec![[0.,0.,4.],[2.,0.,4.]],closed:false};
        let s=Surface::nurbs(NurbsSurface::ruled(&a,&b,false).unwrap());
        xyz(s.evaluate(0.25,0.5).unwrap().point,[0.5,0.,2.]);
        near(s.area(8,&||false).unwrap(),8.);
    }
    #[test] fn box_restricted_booleans_and_offset(){
        let a=ClosedSolid::centered_box(4.,6.,8.).unwrap();
        let b=a.translated([2.,0.,0.]).unwrap();
        near(a.union_box(&b).unwrap().volume(),288.);
        near(a.difference_box(&b).unwrap().volume(),96.);
        near(a.offset_box(1.).unwrap().volume(),480.);
        assert!(a.union_box(&a.translated([1.,1.,0.]).unwrap()).is_err());
        assert!(a.difference_box(&a).is_err());
        assert!(a.offset_box(-2.).is_err());
    }
    #[test] fn closed_box_topology_volume_and_intersection(){
        let a=ClosedSolid::centered_box(4.,6.,8.).unwrap();
        assert_eq!((a.brep.vertices.len(),a.brep.edges.len(),a.brep.shell.faces.len()),(8,12,6));
        near(a.volume(),192.);near(a.area(),208.);
        let b=a.translated([2.,0.,0.]).unwrap();
        near(a.intersection(&b).unwrap().volume(),96.);
        assert!(a.intersection(&a.translated([4.,0.,0.]).unwrap()).is_err());
        assert!(ClosedSolid::centered_box(0.,1.,1.).is_err());
        let mut volume=0.;for &[i,j,k] in &a.preview.triangles {
            volume+=dot(a.preview.vertices[i as usize],cross(a.preview.vertices[j as usize],a.preview.vertices[k as usize]))/6.;
        } near(volume,a.volume());
    }
}
