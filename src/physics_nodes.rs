//! Nodes for mesh physics (`physics`) and mesh editing tools (`mesh_tools`).
//! Grasshopper-plugin style names are search keywords only; labels are Japanese.
//! Independent implementation: no GPL/LGPL code was used or read.
use crate::engine::{as_mesh, cancelled, num};
use crate::mesh_tools as mt;
use crate::model::{DataType as D, Mesh, NodeKind as K, NodeSpec, Value, g, i, n};
use crate::physics::{self, Config};

pub const KINDS: &[K] = &[
    K::PhysSpringRelax,
    K::PhysHang,
    K::PhysSoapFilm,
    K::PhysDrape,
    K::PhysMinimalSurface,
    K::MeshLoop,
    K::MeshSplitFaces,
    K::MeshTaubin,
    K::MeshRemesh,
    K::MeshFillHoles,
    K::MeshThicken,
    K::MeshOffset,
];

pub fn is_kind(kind: K) -> bool {
    KINDS.contains(&kind)
}
/// Physics nodes take whole lists: meshes, anchor points and an optional collider.
pub fn is_list_kind(kind: K) -> bool {
    matches!(
        kind,
        K::PhysSpringRelax | K::PhysHang | K::PhysSoapFilm | K::PhysDrape
    )
}
pub fn is_item_kind(kind: K) -> bool {
    is_kind(kind) && !is_list_kind(kind)
}
/// Whether input `index` of a physics node is a list (not a first-item setting).
pub fn input_is_list(kind: K, index: usize) -> bool {
    match kind {
        K::PhysSpringRelax | K::PhysHang | K::PhysSoapFilm => index <= 1,
        K::PhysDrape => index <= 2,
        _ => false,
    }
}

const MESH: crate::model::InputSpec = g("mesh", "メッシュ", D::Mesh);
const ANCHORS: crate::model::InputSpec = g("anchors", "固定点", D::Point);

macro_rules! spec {
    ($cat:literal,$label:literal,$description:literal,$keywords:literal,$out:ident,[$($input:expr),* $(,)?]) => {{
        static SPEC: NodeSpec = NodeSpec {
            label: $label,
            category: $cat,
            description: $description,
            keywords: $keywords,
            inputs: &[$($input),*],
            output: D::$out,
        };
        &SPEC
    }};
}

pub fn spec(kind: K) -> &'static NodeSpec {
    match kind {
        K::PhysSpringRelax => spec!(
            "物理",
            "ばね緩和",
            "メッシュの辺をばねとして、固定点を残して形を落ち着かせます（ばね質点）。重力を入れると垂れ下がり、自然長の倍率を変えると縮んだり張ったりします。固定点は最も近い頂点に付き、その位置へ動きます。",
            "kangaroo spring relax physics 緩和 ばね 質点 重力 固定点",
            Mesh,
            [
                MESH,
                ANCHORS,
                i("iterations", "反復", 300., 1., 5000.),
                n("stiffness", "ばね剛性", 1., 0.01, 100., 0.1),
                n("rest_scale", "自然長の倍率", 1., 0., 2., 0.05),
                n("gravity", "重力（下向きの力）", 0., -1000., 1000., 0.1),
                i("pin_boundary", "境界を固定（0/1）", 0., 0., 1.),
                i("tension_only", "引張のみ（0/1）", 0., 0., 1.)
            ]
        ),
        K::PhysHang => spec!(
            "物理",
            "逆吊り形状探索",
            "ネットを重力で吊り下げて、張力だけの形（懸垂線・テンション構造）を探します。「上下反転」を1にすると、圧縮だけで立つシェル（ガウディの逆さ吊り）になります。たるみは 荷重÷剛性 に比例します。",
            "hanging chain form finding catenary inverse hang tension 逆吊り 吊り 懸垂線 形状探索",
            Mesh,
            [
                MESH,
                ANCHORS,
                i("iterations", "反復", 800., 1., 5000.),
                n("load", "荷重", 1., 0., 1000., 0.1),
                n("stiffness", "ばね剛性", 1., 0.01, 100., 0.1),
                n("rest_scale", "自然長の倍率", 0., 0., 2., 0.05),
                i("pin_boundary", "境界を固定（0/1）", 1., 0., 1.),
                i("flip", "上下反転（0/1）", 1., 0., 1.)
            ]
        ),
        K::PhysSoapFilm => spec!(
            "物理",
            "ソープフィルム",
            "固定した縁（または固定点）を保ったまま面積が最小になる形に落ち着かせます（石けん膜）。「膨らみ」を正にすると法線方向に膨らむシャボン玉風になります。",
            "soap film minimal area surface bubble 石けん 面積最小 シャボン",
            Mesh,
            [
                MESH,
                ANCHORS,
                i("iterations", "反復", 300., 1., 5000.),
                n("strength", "1回の強さ", 0.5, 0.05, 1., 0.05),
                n("pressure", "膨らみ", 0., -5., 5., 0.01),
                i("pin_boundary", "境界を固定（0/1）", 1., 0., 1.)
            ]
        ),
        K::PhysDrape => spec!(
            "物理",
            "衝突・布かぶせ",
            "布のようなメッシュを重力で落として、静止メッシュや床に被せます。頂点を球として移動経路を連続判定します。面や辺の自己衝突、初期貫通の解消、構造安全性は保証しません。",
            "collision drape cloth collide floor 衝突 布 かぶせ 床",
            Mesh,
            [
                MESH,
                ANCHORS,
                g("collider", "衝突形状", D::Mesh),
                i("iterations", "反復", 400., 1., 5000.),
                n("stiffness", "ばね剛性", 1., 0.01, 100., 0.1),
                n("rest_scale", "自然長の倍率", 1., 0., 2., 0.05),
                n("gravity", "重力（下向きの力）", 0.5, 0., 1000., 0.1),
                n("margin", "すき間", 0.2, 0.001, 100., 0.05),
                i("floor_on", "床を使う（0/1）", 0., 0., 1.),
                n("floor_z", "床の高さ", 0., -1e5, 1e5, 1.),
                i("pin_boundary", "境界を固定（0/1）", 0., 0., 1.)
            ]
        ),
        K::PhysMinimalSurface => spec!(
            "物理",
            "最小曲面",
            "閉じた曲線を縁にして、面積が最小の面（石けん膜の形）を作ります。曲線に張る円盤を作ってからソープフィルムで落ち着かせます。",
            "minimal surface plateau soap boundary 最小曲面 境界曲線 面積最小",
            Mesh,
            [
                g("curve", "境界曲線", D::Curve),
                i("rings", "分割の段数", 12., 2., 64.),
                i("iterations", "反復", 400., 1., 5000.),
                n("strength", "1回の強さ", 0.5, 0.05, 1., 0.05)
            ]
        ),
        K::MeshLoop => spec!(
            "立体",
            "Loop 細分化",
            "三角形メッシュをLoop法でなめらかに細分化します（1面が4面になります）。縁や折り目の辺は形を保ちます。",
            "weaverbird loop subdivision smooth subdivide 細分化 ループ",
            Mesh,
            [MESH, i("levels", "段数", 1., 1., 4.)]
        ),
        K::MeshSplitFaces => spec!(
            "立体",
            "面分割",
            "面を細かく割ります。モード0は各面を重心で3分割、モード1は長い辺を二分割して最大辺長以下にします（T字の継ぎ目は出ません）。形は変えません。",
            "weaverbird split faces centroid refine 面分割 重心 細かく",
            Mesh,
            [
                MESH,
                i("mode", "モード（0=重心 1=長い辺）", 0., 0., 1.),
                i("levels", "段数（モード0）", 1., 1., 4.),
                n("max_len", "最大辺長（モード1）", 10., 0.001, 1e5, 0.5)
            ]
        ),
        K::MeshTaubin => spec!(
            "立体",
            "平滑化（縮まない）",
            "Taubin法の平滑化。ならしと戻しを交互に行うので、普通の平滑化のように縮みません。縁を固定できます。",
            "taubin smooth shrink free 平滑化 スムース 縮まない",
            Mesh,
            [
                MESH,
                i("iterations", "反復", 10., 1., 200.),
                n("strength", "強さ", 0.5, 0.05, 0.9, 0.05),
                i("fix_boundary", "縁を固定（0/1）", 1., 0., 1.)
            ]
        ),
        K::MeshRemesh => spec!(
            "立体",
            "再メッシュ",
            "辺の長さがそろった三角形に作り直します（分割・縮約・辺の入れ替え・平滑化）。縁の頂点は動かさず、元の面に沿わせます。長さ0は今の平均を使います。",
            "remesh isotropic uniform quad remesher 再メッシュ 均一 三角形",
            Mesh,
            [
                MESH,
                n("edge_length", "目標の辺長（0=自動）", 0., 0., 1e5, 0.5),
                i("iterations", "反復", 5., 1., 10.)
            ]
        ),
        K::MeshFillHoles => spec!(
            "立体",
            "穴埋め",
            "メッシュの穴（開いた縁）を三角形でふさぎます。凹んだ形の穴にも対応します。大きすぎる穴は残します。",
            "fill holes cap close boundary 穴埋め 穴 ふさぐ",
            Mesh,
            [MESH, i("max_edges", "穴の最大辺数", 100., 3., 300.)]
        ),
        K::MeshThicken => spec!(
            "立体",
            "厚み付け（シェル）",
            "面に厚みをつけて立体にします。縁には側面が付き、閉じたメッシュは中空のシェルになります。法線が外向きの面を想定し、急な曲がりでは自己交差に注意してください。",
            "thicken shell solidify thickness 厚み シェル 肉厚 中空",
            Mesh,
            [
                MESH,
                n("thickness", "厚み", 2., 0.001, 1000., 0.5),
                i("side", "向き（0=外 1=内 2=両側）", 2., 0., 2.)
            ]
        ),
        K::MeshOffset => spec!(
            "立体",
            "メッシュオフセット",
            "各頂点を法線方向に動かして面をずらします。正で外側、負で内側です。",
            "offset mesh inflate shrink オフセット 膨らませる",
            Mesh,
            [MESH, n("distance", "距離", 1., -1000., 1000., 0.1)]
        ),
        _ => spec!(
            "物理",
            "物理",
            "物理ノードではありません。",
            "physics",
            Mesh,
            [MESH]
        ),
    }
}

fn mesh<'a>(a: &[&'a Value], index: usize) -> Result<&'a Mesh, String> {
    match a.get(index) {
        Some(Value::Mesh(m)) => Ok(m),
        _ => Err("メッシュを接続してください。".into()),
    }
}
fn mesh_out(m: Mesh, stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    as_mesh(m, stop)
}

/// Item-wise nodes (mesh tools and the minimal surface).
pub fn run_item(kind: K, a: &[&Value], stop: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    match kind {
        K::MeshLoop => mesh_out(
            mt::loop_subdivide(mesh(a, 0)?, num(a, 1)? as usize, stop)?,
            stop,
        ),
        K::MeshSplitFaces => {
            let m = mesh(a, 0)?;
            if num(a, 1)? >= 0.5 {
                mesh_out(mt::split_long_edges(m, num(a, 3)?, stop)?, stop)
            } else {
                mesh_out(mt::split_centroid(m, num(a, 2)? as usize, stop)?, stop)
            }
        }
        K::MeshTaubin => mesh_out(
            mt::taubin(
                mesh(a, 0)?,
                num(a, 1)? as usize,
                num(a, 2)?,
                num(a, 3)? != 0.,
                stop,
            )?,
            stop,
        ),
        K::MeshRemesh => mesh_out(
            mt::remesh(mesh(a, 0)?, num(a, 1)?, num(a, 2)? as usize, stop)?,
            stop,
        ),
        K::MeshFillHoles => mesh_out(
            mt::fill_holes(mesh(a, 0)?, num(a, 1)? as usize, stop)?,
            stop,
        ),
        K::MeshThicken => {
            let t = num(a, 1)?;
            let (outer, inner) = match num(a, 2)? as i64 {
                0 => (t, 0.),
                1 => (0., -t),
                _ => (t / 2., -t / 2.),
            };
            mesh_out(mt::thicken(mesh(a, 0)?, outer, inner, stop)?, stop)
        }
        K::MeshOffset => mesh_out(mt::offset(mesh(a, 0)?, num(a, 1)?, stop)?, stop),
        K::PhysMinimalSurface => {
            let Some(Value::Curve(c)) = a.first() else {
                return Err("閉じた曲線を接続してください。".into());
            };
            let disk = physics::disk_from_curve(&c.points, num(a, 1)? as usize, stop)?;
            mesh_out(
                physics::soap_film(&disk, &[], num(a, 2)? as usize, num(a, 3)?, 0., true, stop)?,
                stop,
            )
        }
        _ => Err("このノードはリスト評価が必要です。".into()),
    }
}

fn points(list: &[Value]) -> Vec<mt::P> {
    list.iter()
        .filter_map(|v| match v {
            Value::Point(p) => Some(*p),
            _ => None,
        })
        .collect()
}
fn setting(lists: &[&[Value]], index: usize) -> Result<f64, String> {
    lists
        .get(index)
        .and_then(|l| l.first())
        .ok_or("設定値がありません。")?
        .number()
}

/// List-wise physics nodes: every mesh of the first list is relaxed with the same anchors.
pub fn run_list(
    kind: K,
    lists: &[&[Value]],
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    cancelled(stop)?;
    let meshes = lists.first().copied().unwrap_or(&[]);
    let anchors = points(lists.get(1).copied().unwrap_or(&[]));
    let collider = match lists.get(2).and_then(|l| l.first()) {
        Some(Value::Mesh(m)) if kind == K::PhysDrape => Some(m.as_ref()),
        _ => None,
    };
    let mut out = Vec::new();
    for value in meshes {
        cancelled(stop)?;
        let Value::Mesh(m) = value else {
            return Err("メッシュを接続してください。".into());
        };
        let result = match kind {
            K::PhysSpringRelax => {
                let cfg = Config {
                    iterations: setting(lists, 2)? as usize,
                    stiffness: setting(lists, 3)?,
                    rest_scale: setting(lists, 4)?,
                    gravity: setting(lists, 5)?,
                    tension_only: setting(lists, 7)? != 0.,
                    pin_boundary: setting(lists, 6)? != 0.,
                    floor: None,
                    margin: 0.,
                };
                physics::relax(m, &anchors, &cfg, None, stop)?
            }
            K::PhysHang => {
                let pin = setting(lists, 6)? != 0.;
                if anchors.is_empty() && !pin {
                    return Err("固定点を接続するか「境界を固定」を1にしてください。".into());
                }
                let cfg = Config {
                    iterations: setting(lists, 2)? as usize,
                    stiffness: setting(lists, 4)?,
                    rest_scale: setting(lists, 5)?,
                    gravity: setting(lists, 3)?,
                    tension_only: true,
                    pin_boundary: pin,
                    floor: None,
                    margin: 0.,
                };
                let hung = physics::relax(m, &anchors, &cfg, None, stop)?;
                if setting(lists, 7)? != 0. {
                    let top = physics::support_height(m, &anchors, pin, stop)?;
                    physics::mirror_z(&hung, top)
                } else {
                    hung
                }
            }
            K::PhysSoapFilm => physics::soap_film(
                m,
                &anchors,
                setting(lists, 2)? as usize,
                setting(lists, 3)?,
                setting(lists, 4)?,
                setting(lists, 5)? != 0.,
                stop,
            )?,
            K::PhysDrape => {
                let cfg = Config {
                    iterations: setting(lists, 3)? as usize,
                    stiffness: setting(lists, 4)?,
                    rest_scale: setting(lists, 5)?,
                    gravity: setting(lists, 6)?,
                    tension_only: false,
                    pin_boundary: setting(lists, 10)? != 0.,
                    floor: (setting(lists, 8)? != 0.).then_some(setting(lists, 9)?),
                    margin: setting(lists, 7)?,
                };
                physics::relax(m, &anchors, &cfg, collider, stop)?
            }
            _ => return Err("物理ノードではありません。".into()),
        };
        out.extend(mesh_out(result, stop)?);
    }
    Ok(out)
}
