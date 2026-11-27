//! Curve, tube, mesh-repair, bend/taper/grid and gyroid nodes (docs/GH_COVERAGE.md).
//!
//! Same rules as `gh_nodes.rs`: every input is treated as hostile, every failure is a
//! Japanese message, nothing here panics, and sizes are checked before a large result is
//! built. Not compiled or run in the session that wrote it; CI is the first check.
//! Engine hooks (see `engine.rs`): `run_item`, `run_list`, `is_list_kind`, `estimate`.
use crate::engine::{
    as_mesh, cancelled, checked_size, cross, dot, finite_point, length, map_shape, num,
    orient_outward, shape, sub, validate_mesh,
};
use crate::model::*;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::f64::consts::TAU;
use std::sync::Arc;

/// Longest curve the quadratic self-intersection check will examine.
const MAX_OFFSET_POINTS: usize = 2_000;
/// Copies made by 格子配列 for one item.
const MAX_GRID_COPIES: usize = 512;
/// Gyroid resolution limit, in cells along the longest side.
const MAX_GYROID_RES: usize = 64;

// ------------------------------------------------------------------ specifications

const POINTS: InputSpec = g("points", "点のリスト", DataType::Point);

static ARC_CURVE: NodeSpec = NodeSpec {
    label: "円弧",
    category: "曲線",
    description: "XY 平面の円弧を、開始角度から終了角度まで等分した点列の曲線として作ります。角度は度（0 は +X 方向、反時計回りが正）。",
    keywords: "arc curve circular segment 円弧 弧 曲線 角度 扇",
    inputs: &[
        n("radius", "半径", 30., 0.1, 1000., 0.5),
        n("start", "開始角度", 0., -3600., 3600., 1.),
        n("end", "終了角度", 180., -3600., 3600., 1.),
        i("segments", "分割数", 24., 1., 512.),
    ],
    output: DataType::Curve,
};
static POLYLINE: NodeSpec = NodeSpec {
    label: "点をつなぐ線",
    category: "曲線",
    description: "点のリストを順に直線でつないだ折れ線を作ります。「閉じる」を 1 にすると最後の点から最初の点へもつなぎます。",
    keywords: "polyline polygon points connect path line 折れ線 点 つなぐ 線 パス 多角形",
    inputs: &[POINTS, i("closed", "閉じる (0:開く 1:閉じる)", 0., 0., 1.)],
    output: DataType::Curve,
};
static SMOOTH_CURVE: NodeSpec = NodeSpec {
    label: "なめらかな曲線",
    category: "曲線",
    description: "点のリストを通るなめらかな曲線（Catmull-Rom）を作ります。サンプル数で点の細かさを決めます。通る点は元の点と一致します。",
    keywords: "spline smooth curve catmull rom interpolate 曲線 スプライン なめらか 通る点 補間",
    inputs: &[
        POINTS,
        i("samples", "サンプル数", 64., 2., 1024.),
        i("closed", "閉じる (0:開く 1:閉じる)", 0., 0., 1.),
    ],
    output: DataType::Curve,
};
static OFFSET_CURVE: NodeSpec = NodeSpec {
    label: "曲線のオフセット",
    category: "曲線",
    description: "閉じた平面の曲線（XY）を、距離だけ外側（+）または内側（−）へずらします。角は「角の上限」を超えると面取りします。ずらした結果が自分と交わるときはエラーになります。",
    keywords: "offset curve inset outline 2d 輪郭 オフセット 広げる 縮める 内側 外側 ふち",
    inputs: &[
        g("curve", "閉じた曲線", DataType::Curve),
        n("distance", "距離（+ で外側）", 2., -200., 200., 0.1),
        n("miter", "角の上限（倍）", 4., 1., 10., 0.1),
    ],
    output: DataType::Curve,
};
static POINT_ON_CURVE: NodeSpec = NodeSpec {
    label: "曲線上の点",
    category: "曲線",
    description: "曲線の始点から長さに沿って、位置 0〜1 の点を返します。0 が始点、1 が終点です。",
    keywords: "point on curve parameter t evaluate 曲線上 点 位置 パラメータ 割合 距離",
    inputs: &[
        g("curve", "曲線", DataType::Curve),
        n("t", "位置 (0 〜 1)", 0.5, 0., 1., 0.01),
    ],
    output: DataType::Point,
};
static DIVIDE_CURVE: NodeSpec = NodeSpec {
    label: "曲線を等分割",
    category: "曲線",
    description: "曲線を長さで等しく分け、点のリストを作ります。開いた曲線は両端を含み、閉じた曲線は始点から重ならないように並べます。",
    keywords: "divide curve points equal spacing 等分割 分割 点 等間隔 曲線 割る",
    inputs: &[
        g("curve", "曲線", DataType::Curve),
        i("count", "個数", 8., 2., 512.),
    ],
    output: DataType::Point,
};
static PIPE: NodeSpec = NodeSpec {
    label: "パイプ",
    category: "立体",
    description: "曲線に沿って、断面が円の管を作ります。曲がりの内側が半径より急だと形が潰れるので、太さを小さくしてください。開いた曲線は「端を閉じる」で両端にふたをします。",
    keywords: "pipe tube sweep along curve 管 パイプ チューブ 曲線 沿わせる らせん ランプ",
    inputs: &[
        g("curve", "パスの曲線", DataType::Curve),
        n("radius", "半径", 3., 0.1, 200., 0.1),
        i("sides", "周の分割", 16., 3., 96.),
        i("caps", "端を閉じる (0:開く 1:閉じる)", 1., 0., 1.),
    ],
    output: DataType::Mesh,
};
static SWEEP: NodeSpec = NodeSpec {
    label: "スイープ",
    category: "立体",
    description: "閉じた断面を、パスの曲線に沿って動かした面を作ります。断面は XY 平面の形を使い、パスの各点で向きを引き継ぎます。両端は開いたままです。",
    keywords: "sweep profile along path extrude along curve 断面 沿って スイープ パス 掃引",
    inputs: &[
        g("profile", "断面（閉じた曲線）", DataType::Curve),
        g("path", "パス", DataType::Curve),
    ],
    output: DataType::Mesh,
};
static SMOOTH_MESH: NodeSpec = NodeSpec {
    label: "なめらかにする",
    category: "変形",
    description: "メッシュの頂点を近くの頂点の平均へ寄せて、角をなめらかにします。方式 1（テイコフ）は縮みを抑えます。境界の頂点は動かしません。",
    keywords: "smooth smoothing laplacian taubin relax mesh なめらか 平滑化 スムーズ 角を丸める 縮み",
    inputs: &[
        g("geometry", "メッシュ", DataType::Mesh),
        n("strength", "強さ", 0.5, 0.05, 1., 0.05),
        i("iterations", "回数", 5., 0., 50.),
        i("method", "方式 (0:ラプラシアン 1:テイコフ)", 1., 0., 1.),
    ],
    output: DataType::Mesh,
};
static SUBDIVIDE: NodeSpec = NodeSpec {
    label: "細分化",
    category: "変形",
    description: "各三角形の辺の中点で 4 つに分けます。段階は 1〜3 で、増えすぎる前に上限でエラーになります。",
    keywords: "subdivide subdivision midpoint refine 細分化 分割 細かく 三角形 増やす",
    inputs: &[
        g("geometry", "メッシュ", DataType::Mesh),
        i("levels", "段階", 1., 1., 3.),
    ],
    output: DataType::Mesh,
};
static WELD_MESH: NodeSpec = NodeSpec {
    label: "頂点をくっつける",
    category: "変形",
    description: "許容差より近い頂点を 1 つにまとめ、同じ頂点を 2 回使う面を取り除きます。STL の重なった頂点をつなぐときに使います。",
    keywords: "weld merge vertices tolerance remove duplicate 頂点 くっつける 溶接 結合 許容差 重複",
    inputs: &[
        g("geometry", "メッシュ", DataType::Mesh),
        n("tolerance", "許容差 (mm)", 0.01, 0.0001, 10., 0.001),
    ],
    output: DataType::Mesh,
};
static UNIFY_NORMALS: NodeSpec = NodeSpec {
    label: "面の向きをそろえる",
    category: "変形",
    description: "隣り合う面の向きをそろえ、閉じた形では体積が正になる向き（外向き）にします。向きが逆の面が混ざったメッシュの修復に使います。",
    keywords: "unify normals orient flip faces outward 法線 向き そろえる 外向き 反転 面",
    inputs: &[g("geometry", "メッシュ", DataType::Mesh)],
    output: DataType::Mesh,
};
static MESH_INFO: NodeSpec = NodeSpec {
    label: "メッシュ情報",
    category: "出力",
    description: "[頂点数, 三角形数, 体積 (mm³), 表面積 (mm²)] の 4 つの数値を返します。体積は閉じた形でのみ意味があります。",
    keywords: "mesh info count vertices triangles volume area 情報 頂点数 三角形数 体積 面積 数える",
    inputs: &[g("geometry", "メッシュ", DataType::Mesh)],
    output: DataType::Number,
};
static TAPER: NodeSpec = NodeSpec {
    label: "テーパー",
    category: "変形",
    description: "Z の低いところの倍率（下の倍率）から高いところの倍率（上の倍率）へ、X・Y を Z に沿って少しずつ変えます。高さのある形状が必要です。",
    keywords: "taper cone funnel scale along z 先細り テーパー 上下 倍率 すぼめる 広げる",
    inputs: &[
        g("geometry", "形状", DataType::Geometry),
        n("base", "下の倍率", 1., 0.01, 10., 0.05),
        n("top", "上の倍率", 0.5, 0.01, 10., 0.05),
    ],
    output: DataType::Geometry,
};
static BEND: NodeSpec = NodeSpec {
    label: "曲げ",
    category: "変形",
    description: "指定した軸のまわりに形を曲げます。曲げる前の長さを、そのまま円弧の長さにします。細かく分割した形に使うと、曲がりがなめらかです。曲げ角が大きいと形が折り重なるのでエラーになります。",
    keywords: "bend curve arc bend around axis 曲げ 湾曲 反る 角度 アーチ",
    inputs: &[
        g("geometry", "形状", DataType::Geometry),
        n("angle", "曲げ角度", 90., -330., 330., 1.),
        i("axis", "曲げる軸のまわり (0:X 1:Y 2:Z)", 2., 0., 2.),
    ],
    output: DataType::Geometry,
};
static GRID_ARRAY: NodeSpec = NodeSpec {
    label: "格子配列",
    category: "配列",
    description: "X・Y・Z の個数と間隔で、形状を格子状に複製します。個数の積は 512 までです。",
    keywords: "grid array matrix 3d lattice rectangular copies 格子 配列 三次元 並べる 格子状 複製",
    inputs: &[
        g("geometry", "形状", DataType::Geometry),
        i("nx", "X の個数", 3., 1., 16.),
        i("ny", "Y の個数", 3., 1., 16.),
        i("nz", "Z の個数", 1., 1., 16.),
        n("dx", "X 間隔", 50., -1e4, 1e4, 1.),
        n("dy", "Y 間隔", 50., -1e4, 1e4, 1.),
        n("dz", "Z 間隔", 50., -1e4, 1e4, 1.),
    ],
    output: DataType::Geometry,
};
static GYROID: NodeSpec = NodeSpec {
    label: "ジャイロイド",
    category: "立体",
    description: "箱の中に、ジャイロイド（TPMS）の網目を閉じた立体として作ります。箱は XY の中心が原点、底面 Z=0。単位の大きさは網目の周期、壁の太さは 0〜1 の目安（大きいほど太い）、分解能は箱の長い辺のマス目の数です。",
    keywords: "gyroid tpms lattice infill minimal surface marching 格子 ジャイロイド 内部構造 網目 TPMS 中空 小物入れ 等値面",
    inputs: &[
        n("width", "幅 (X)", 40., 1., 400., 1.),
        n("depth", "奥行 (Y)", 40., 1., 400., 1.),
        n("height", "高さ (Z)", 40., 1., 400., 1.),
        n("cell", "単位の大きさ", 10., 1., 200., 0.5),
        n("thickness", "壁の太さ (0.02〜1)", 0.35, 0.02, 1., 0.01),
        i("resolution", "分解能（最大 64）", 48., 8., 64.),
    ],
    output: DataType::Mesh,
};
static REVOLVE_POINTS: NodeSpec = NodeSpec {
    label: "回転体の輪郭",
    category: "立体",
    description: "点のリストを輪郭として Z 軸のまわりに回転し、器や柱の面を作ります。各点の X・Y の距離を半径、Z を高さにします。輪郭の両端が軸上にあると閉じた立体になります。",
    keywords: "revolve points profile lathe vase spin 回転体 輪郭 点 回す 器 旋盤 回転面",
    inputs: &[
        g("points", "輪郭の点", DataType::Point),
        i("steps", "分割数", 48., 8., 192.),
    ],
    output: DataType::Mesh,
};

/// Specification of a node defined in this module; other kinds fall back to `gh_nodes`.
pub fn spec(kind: NodeKind) -> &'static NodeSpec {
    use NodeKind::*;
    match kind {
        ArcCurve => &ARC_CURVE,
        Polyline => &POLYLINE,
        SmoothCurve => &SMOOTH_CURVE,
        OffsetCurve => &OFFSET_CURVE,
        PointOnCurve => &POINT_ON_CURVE,
        DivideCurve => &DIVIDE_CURVE,
        Pipe => &PIPE,
        Sweep => &SWEEP,
        SmoothMesh => &SMOOTH_MESH,
        Subdivide => &SUBDIVIDE,
        WeldMesh => &WELD_MESH,
        UnifyNormals => &UNIFY_NORMALS,
        MeshInfo => &MESH_INFO,
        TaperScale => &TAPER,
        Bend => &BEND,
        GridArray3d => &GRID_ARRAY,
        Gyroid => &GYROID,
        RevolvePoints => &REVOLVE_POINTS,
        other => crate::gh_nodes::spec(other),
    }
}

/// True for the node kinds implemented in this module.
pub fn is_kind(kind: NodeKind) -> bool {
    use NodeKind::*;
    matches!(
        kind,
        ArcCurve
            | Polyline
            | SmoothCurve
            | OffsetCurve
            | PointOnCurve
            | DivideCurve
            | Pipe
            | Sweep
            | SmoothMesh
            | Subdivide
            | WeldMesh
            | UnifyNormals
            | MeshInfo
            | TaperScale
            | Bend
            | GridArray3d
            | Gyroid
            | RevolvePoints
    )
}

/// Nodes that see the whole list of each branch (points in, one curve or mesh out).
pub fn is_list_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Polyline | NodeKind::SmoothCurve | NodeKind::RevolvePoints
    )
}

/// Preflight estimate `(weight, items)` for the engine's budget, before a per-item run.
/// Kinds without a large output return `(0, 1)`.
pub(crate) fn estimate(kind: NodeKind, args: &[&Value]) -> Result<(usize, usize), String> {
    use NodeKind::*;
    let count = |k: usize| -> usize { num(args, k).map(|v| v.max(0.) as usize).unwrap_or(0) };
    Ok(match kind {
        Pipe => {
            let path = points_len(shape(args)?);
            (path.saturating_mul(count(2)).saturating_mul(2), 1)
        }
        Sweep => {
            let profile = args.get(1).copied().map(points_len).unwrap_or(0);
            let path = points_len(shape(args)?);
            (profile.saturating_mul(path).saturating_mul(2), 1)
        }
        Subdivide => {
            let factor = 4usize.saturating_pow(count(1) as u32);
            (shape(args)?.weight().saturating_mul(factor), 1)
        }
        TaperScale | Bend => (shape(args)?.weight(), 1),
        GridArray3d => {
            let copies = count(1).saturating_mul(count(2)).saturating_mul(count(3));
            (shape(args)?.weight().saturating_mul(copies), copies)
        }
        Gyroid => {
            let r = count(5).saturating_add(2);
            (r.saturating_mul(r).saturating_mul(r), 1)
        }
        DivideCurve => (0, count(1)),
        _ => (0, 1),
    })
}

fn points_len(v: &Value) -> usize {
    value_points(v).map(|p| p.len()).unwrap_or(0)
}

// ------------------------------------------------------------------ list nodes

fn setting(lists: &[&[Value]], index: usize) -> Result<f64, String> {
    lists
        .get(index)
        .and_then(|l| l.first())
        .ok_or("設定の値がありません。")?
        .number()
}
fn too_many(len: usize) -> Result<(), String> {
    if len > MAX_ITEMS {
        Err("リストが長すぎます。要素の数を減らしてください。".into())
    } else {
        Ok(())
    }
}
/// The points of a point list; any other item is refused.
fn points_of(list: &[Value]) -> Result<Vec<[f64; 3]>, String> {
    too_many(list.len())?;
    list.iter()
        .map(|v| match v {
            Value::Point(p) if finite_point(*p) => Ok(*p),
            _ => Err("点のリストを接続してください。".to_string()),
        })
        .collect()
}
fn curve_value(points: Vec<[f64; 3]>, closed: bool) -> Result<Vec<Value>, String> {
    if points.len() > MAX_VERTICES {
        return Err("曲線の点が多すぎます。点の数を減らしてください。".into());
    }
    if points.iter().any(|p| !finite_point(*p)) {
        return Err("曲線の座標が大きすぎます。".into());
    }
    Ok(vec![Value::Curve(Arc::new(Curve { points, closed }))])
}
fn numbers_value(values: Vec<f64>) -> Result<Vec<Value>, String> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err("計算結果が有限の数値ではありません。".into());
    }
    Ok(values.into_iter().map(Value::Number).collect())
}

/// Evaluates one branch of a list node in this module. Other kinds go to `gh_nodes`.
pub fn run_list(
    kind: NodeKind,
    lists: &[&[Value]],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    if !is_list_kind(kind) {
        return crate::gh_nodes::run_list(kind, lists, cancel);
    }
    cancelled(cancel)?;
    let list: &[Value] = lists.first().copied().unwrap_or(&[]);
    if list.is_empty() {
        return Ok(Vec::new());
    }
    let points = points_of(list)?;
    match kind {
        NodeKind::Polyline => {
            let closed = setting(lists, 1)? != 0.;
            if points.len() < 2 {
                return Err("点を 2 つ以上接続してください。".into());
            }
            if closed && points.len() < 3 {
                return Err("閉じた線には 3 点以上が必要です。".into());
            }
            curve_value(points, closed)
        }
        NodeKind::SmoothCurve => {
            let samples = setting(lists, 1)?.round() as usize;
            let closed = setting(lists, 2)? != 0.;
            smooth_curve(&points, samples, closed, cancel)
        }
        NodeKind::RevolvePoints => {
            let steps = setting(lists, 1)?.round() as usize;
            as_mesh(revolve_points(&points, steps, cancel)?, cancel)
        }
        _ => Err("このノードはリストを扱いません。".into()),
    }
}

/// Uniform Catmull-Rom through the points. Open curves pass through both end points.
fn smooth_curve(
    points: &[[f64; 3]],
    samples: usize,
    closed: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let m = points.len();
    let min_points = if closed { 3 } else { 2 };
    if m < min_points {
        return Err(
            "なめらかな曲線には、閉じるときは 3 点以上、開くときは 2 点以上が必要です。".into(),
        );
    }
    if samples < 2 {
        return Err("サンプル数は 2 以上にしてください。".into());
    }
    let spans = if closed { m } else { m - 1 };
    let at = |index: isize| -> [f64; 3] {
        let len = m as isize;
        let q = if closed {
            index.rem_euclid(len)
        } else {
            index.clamp(0, len - 1)
        };
        points[q as usize]
    };
    let mut out = Vec::with_capacity(samples);
    for k in 0..samples {
        if k % 256 == 0 {
            cancelled(cancel)?;
        }
        let u = if closed {
            k as f64 * m as f64 / samples as f64
        } else {
            k as f64 * (spans as f64) / (samples - 1) as f64
        };
        let j = (u.floor() as usize).min(spans.saturating_sub(1));
        let t = (u - j as f64).clamp(0., 1.);
        let base = j as isize;
        let (p0, p1, p2, p3) = (at(base - 1), at(base), at(base + 1), at(base + 2));
        let t2 = t * t;
        let t3 = t2 * t;
        let mut q = [0.; 3];
        for c in 0..3 {
            q[c] = 0.5
                * (2. * p1[c]
                    + (-p0[c] + p2[c]) * t
                    + (2. * p0[c] - 5. * p1[c] + 4. * p2[c] - p3[c]) * t2
                    + (-p0[c] + 3. * p1[c] - 3. * p2[c] + p3[c]) * t3);
        }
        out.push(q);
    }
    curve_value(out, closed)
}

/// Surface of revolution of a profile given as points. A point on the axis (X・Y = 0)
/// becomes a single vertex, so a profile that starts and ends on the axis is closed.
fn revolve_points(
    points: &[[f64; 3]],
    steps: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if points.len() < 2 {
        return Err("回転体の輪郭には 2 点以上が必要です。".into());
    }
    if steps < 3 {
        return Err("分割数は 3 以上にしてください。".into());
    }
    let rings = points.len();
    let radius: Vec<f64> = points
        .iter()
        .map(|p| (p[0] * p[0] + p[1] * p[1]).sqrt())
        .collect();
    let on_axis: Vec<bool> = radius.iter().map(|r| *r < 1e-9).collect();
    let vertex_count: usize = on_axis
        .iter()
        .map(|axis| if *axis { 1 } else { steps })
        .sum();
    checked_size(vertex_count, rings.saturating_mul(steps).saturating_mul(2))?;
    let mut mesh = Mesh::default();
    let mut base = Vec::with_capacity(rings);
    for (i, p) in points.iter().enumerate() {
        cancelled(cancel)?;
        base.push(mesh.vertices.len());
        if on_axis[i] {
            mesh.vertices.push([0., 0., p[2]]);
        } else {
            for j in 0..steps {
                let t = TAU * j as f64 / steps as f64;
                mesh.vertices
                    .push([radius[i] * t.cos(), radius[i] * t.sin(), p[2]]);
            }
        }
    }
    let index = |ring: usize, j: usize| -> u32 {
        let offset = if on_axis[ring] { 0 } else { j };
        (base[ring] + offset) as u32
    };
    for i in 0..rings - 1 {
        cancelled(cancel)?;
        for j in 0..steps {
            let j2 = (j + 1) % steps;
            let a = index(i, j);
            let b = index(i, j2);
            let c = index(i + 1, j);
            let d = index(i + 1, j2);
            for tri in [[a, b, c], [b, d, c]] {
                if tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
                    mesh.triangles.push(tri);
                }
            }
        }
    }
    if on_axis[0] && on_axis[rings - 1] {
        orient_outward(&mut mesh);
    }
    validate_mesh(&mesh, cancel)?;
    Ok(mesh)
}

// ------------------------------------------------------------------ item nodes

/// Evaluates one item of a node defined here. Kinds not defined here are refused.
pub fn run_item(
    kind: NodeKind,
    a: &[&Value],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    cancelled(cancel)?;
    match kind {
        ArcCurve => {
            let radius = num(a, 0)?;
            let start = num(a, 1)?.to_radians();
            let end = num(a, 2)?.to_radians();
            let segments = num(a, 3)?.round() as usize;
            if segments < 1 {
                return Err("分割数は 1 以上にしてください。".into());
            }
            if (end - start).abs() < 1e-9 {
                return Err(
                    "開始角度と終了角度が同じです。円にするには終了角度を 360 度ずらしてください。"
                        .into(),
                );
            }
            let points: Vec<[f64; 3]> = (0..=segments)
                .map(|k| {
                    let t = start + (end - start) * k as f64 / segments as f64;
                    [radius * t.cos(), radius * t.sin(), 0.]
                })
                .collect();
            curve_value(points, false)
        }
        OffsetCurve => {
            let Value::Curve(c) = shape(a)? else {
                return Err("曲線を接続してください。".into());
            };
            offset_closed(c, num(a, 1)?, num(a, 2)?, cancel)
        }
        PointOnCurve => {
            let Value::Curve(c) = shape(a)? else {
                return Err("曲線を接続してください。".into());
            };
            let t = num(a, 1)?.clamp(0., 1.);
            let lengths = arc_lengths(c)?;
            Ok(vec![Value::Point(point_at(c, &lengths, t * lengths.total))])
        }
        DivideCurve => {
            let Value::Curve(c) = shape(a)? else {
                return Err("曲線を接続してください。".into());
            };
            let count = num(a, 1)?.round() as usize;
            if count < 2 {
                return Err("個数は 2 以上にしてください。".into());
            }
            let lengths = arc_lengths(c)?;
            let mut out = Vec::with_capacity(count);
            for k in 0..count {
                cancelled(cancel)?;
                let s = if c.closed {
                    lengths.total * k as f64 / count as f64
                } else {
                    lengths.total * k as f64 / (count - 1) as f64
                };
                out.push(Value::Point(point_at(c, &lengths, s)));
            }
            Ok(out)
        }
        Pipe => {
            let Value::Curve(c) = shape(a)? else {
                return Err("パスの曲線を接続してください。".into());
            };
            let radius = num(a, 1)?;
            let sides = num(a, 2)?.round() as usize;
            let caps = num(a, 3)? != 0.;
            if sides < 3 {
                return Err("周の分割は 3 以上にしてください。".into());
            }
            let path = clean_path(&c.points, c.closed);
            if path.len() < 2 {
                return Err("パイプのパスには 2 点以上が必要です。".into());
            }
            let closed = c.closed && path.len() >= 3;
            let profile: Vec<[f64; 2]> = (0..sides)
                .map(|j| {
                    let t = TAU * j as f64 / sides as f64;
                    [radius * t.cos(), radius * t.sin()]
                })
                .collect();
            as_mesh(tube(&path, closed, &profile, caps, cancel)?, cancel)
        }
        Sweep => {
            let Value::Curve(profile) = shape(a)? else {
                return Err("断面の曲線を接続してください。".into());
            };
            let Value::Curve(path) = a.get(1).copied().ok_or("パスの曲線を接続してください。")?
            else {
                return Err("パスの曲線を接続してください。".into());
            };
            if !profile.closed {
                return Err("断面は閉じた曲線にしてください。".into());
            }
            let section = profile_xy(&profile.points)?;
            let path_points = clean_path(&path.points, path.closed);
            if path_points.len() < 2 {
                return Err("パスには 2 点以上が必要です。".into());
            }
            let closed = path.closed && path_points.len() >= 3;
            as_mesh(tube(&path_points, closed, &section, false, cancel)?, cancel)
        }
        SmoothMesh => {
            let Value::Mesh(m) = shape(a)? else {
                return Err("メッシュを接続してください。".into());
            };
            let strength = num(a, 1)?;
            let iterations = num(a, 2)?.round() as usize;
            let taubin = num(a, 3)? != 0.;
            as_mesh(smooth(m, strength, iterations, taubin, cancel)?, cancel)
        }
        Subdivide => {
            let Value::Mesh(m) = shape(a)? else {
                return Err("メッシュを接続してください。".into());
            };
            let levels = num(a, 1)?.round() as usize;
            as_mesh(subdivide(m, levels, cancel)?, cancel)
        }
        WeldMesh => {
            let Value::Mesh(m) = shape(a)? else {
                return Err("メッシュを接続してください。".into());
            };
            as_mesh(weld(m, num(a, 1)?, cancel)?, cancel)
        }
        UnifyNormals => {
            let Value::Mesh(m) = shape(a)? else {
                return Err("メッシュを接続してください。".into());
            };
            as_mesh(unify(m, cancel)?, cancel)
        }
        MeshInfo => {
            let Value::Mesh(m) = shape(a)? else {
                return Err("メッシュを接続してください。".into());
            };
            let (mut volume, mut area) = (0., 0.);
            for t in &m.triangles {
                let [Some(p), Some(q), Some(r)] = t.map(|v| m.vertices.get(v as usize).copied())
                else {
                    return Err("メッシュの頂点番号が無効です。".into());
                };
                volume += dot(p, cross(q, r)) / 6.;
                area += length(cross(sub(q, p), sub(r, p))) / 2.;
            }
            numbers_value(vec![
                m.vertices.len() as f64,
                m.triangles.len() as f64,
                volume.abs(),
                area,
            ])
        }
        TaperScale => {
            let value = shape(a)?;
            let (lo, hi) = bounds_along(value, 2)?;
            if hi - lo < 1e-9 {
                return Err(
                    "テーパーには高さのある形状が必要です。先に押し出してください。".into(),
                );
            }
            let (base, top) = (num(a, 1)?, num(a, 2)?);
            let shaped = map_shape(
                value,
                |p| {
                    let t = ((p[2] - lo) / (hi - lo)).clamp(0., 1.);
                    let s = base + (top - base) * t;
                    [p[0] * s, p[1] * s, p[2]]
                },
                cancel,
            )?;
            Ok(vec![shaped])
        }
        Bend => {
            let value = shape(a)?;
            let angle = num(a, 1)?.to_radians();
            let around = (num(a, 2)?.round().max(0.) as usize).min(2);
            if angle.abs() < 1e-9 {
                return Ok(vec![value.clone()]);
            }
            // Bending about the `around` axis: the length runs along `along`, and the
            // bend centre is at the middle of the `across` axis.
            let along = (around + 2) % 3;
            let across = (around + 1) % 3;
            let (lo, hi) = bounds_along(value, along)?;
            if hi - lo < 1e-9 {
                return Err("曲げには長さのある形状が必要です。".into());
            }
            let (min_w, max_w) = bounds_along(value, across)?;
            let centre = (min_w + max_w) / 2.;
            let half = (max_w - min_w) / 2.;
            let sign = angle.signum();
            let radius = (hi - lo) / angle.abs();
            if radius <= half + 1e-9 {
                return Err("曲げ角が大きすぎて形が折り重なります。角度を小さくするか、曲げる軸を変えてください。".into());
            }
            let shaped = map_shape(
                value,
                |p| {
                    let u = p[along] - lo;
                    let w = sign * (p[across] - centre);
                    let rho = radius - w;
                    let phi = u / radius;
                    let mut q = p;
                    q[along] = lo + rho * phi.sin();
                    q[across] = centre + sign * (radius - rho * phi.cos());
                    q
                },
                cancel,
            )?;
            Ok(vec![shaped])
        }
        GridArray3d => {
            let value = shape(a)?;
            let (nx, ny, nz) = (
                num(a, 1)?.round() as usize,
                num(a, 2)?.round() as usize,
                num(a, 3)?.round() as usize,
            );
            let (dx, dy, dz) = (num(a, 4)?, num(a, 5)?, num(a, 6)?);
            let count = nx.saturating_mul(ny).saturating_mul(nz);
            if count == 0 || count > MAX_GRID_COPIES {
                return Err(format!(
                    "格子配列の個数は {MAX_GRID_COPIES} 個までです（現在 {count} 個）。"
                ));
            }
            let mut out = Vec::with_capacity(count);
            for k in 0..nz {
                for j in 0..ny {
                    for i in 0..nx {
                        cancelled(cancel)?;
                        let d = [i as f64 * dx, j as f64 * dy, k as f64 * dz];
                        out.push(map_shape(
                            value,
                            |p| [p[0] + d[0], p[1] + d[1], p[2] + d[2]],
                            cancel,
                        )?);
                    }
                }
            }
            Ok(out)
        }
        Gyroid => {
            let size = [num(a, 0)?, num(a, 1)?, num(a, 2)?];
            let cell = num(a, 3)?;
            let thickness = num(a, 4)?;
            let res = num(a, 5)?.round() as usize;
            as_mesh(gyroid(size, cell, thickness, res, cancel)?, cancel)
        }
        _ => Err("このノードは 1 つずつ計算できません。".into()),
    }
}

// ------------------------------------------------------------------ shared helpers

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let l = length(v);
    (l.is_finite() && l > 1e-12).then(|| [v[0] / l, v[1] / l, v[2] / l])
}

/// Points of a geometry value (a point is a one-point list).
fn value_points(v: &Value) -> Result<&[[f64; 3]], String> {
    match v {
        Value::Point(p) => Ok(std::slice::from_ref(p)),
        Value::Curve(c) => Ok(c.points.as_slice()),
        Value::Mesh(m) => Ok(m.vertices.as_slice()),
        Value::Number(_) => Err("形状を接続してください。".into()),
    }
}

/// Smallest and largest coordinate along `axis` (0 = X, 1 = Y, 2 = Z).
fn bounds_along(v: &Value, axis: usize) -> Result<(f64, f64), String> {
    let points = value_points(v)?;
    if points.is_empty() {
        return Err("形状に頂点がありません。".into());
    }
    Ok(points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[axis]), hi.max(p[axis]))
        }))
}

/// Removes repeated consecutive points (and the repeated last point of a closed curve).
fn clean_path(points: &[[f64; 3]], closed: bool) -> Vec<[f64; 3]> {
    let mut out: Vec<[f64; 3]> = Vec::with_capacity(points.len());
    for &p in points {
        let duplicate = out.last().is_some_and(|q| length(sub(p, *q)) <= 1e-9);
        if !duplicate {
            out.push(p);
        }
    }
    if closed {
        while out.len() > 1
            && out
                .first()
                .zip(out.last())
                .is_some_and(|(a, b)| length(sub(*a, *b)) <= 1e-9)
        {
            out.pop();
        }
    }
    out
}

fn signed_area_2d(p: &[[f64; 2]]) -> f64 {
    let n = p.len();
    0.5 * (0..n)
        .map(|i| {
            let a = p[i];
            let b = p[(i + 1) % n];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
}

/// Profile of a sweep in its own XY plane, counter-clockwise, without repeats.
fn profile_xy(points: &[[f64; 3]]) -> Result<Vec<[f64; 2]>, String> {
    let lifted: Vec<[f64; 3]> = points.iter().map(|p| [p[0], p[1], 0.]).collect();
    let mut xy: Vec<[f64; 2]> = clean_path(&lifted, true)
        .iter()
        .map(|p| [p[0], p[1]])
        .collect();
    if xy.len() < 3 {
        return Err("断面には 3 点以上が必要です。".into());
    }
    let area = signed_area_2d(&xy);
    if area.abs() < 1e-9 {
        return Err("断面の面積がありません。".into());
    }
    if area < 0. {
        xy.reverse();
    }
    Ok(xy)
}

fn perpendicular(t: [f64; 3]) -> Result<[f64; 3], String> {
    let helper = if t[0].abs() < 0.9 {
        [1., 0., 0.]
    } else {
        [0., 1., 0.]
    };
    let along = dot(helper, t);
    unit([
        helper[0] - t[0] * along,
        helper[1] - t[1] * along,
        helper[2] - t[2] * along,
    ])
    .ok_or_else(|| "パスの向きを決められません。".to_string())
}

/// Tangent, normal and binormal of one path point.
type Frame = ([f64; 3], [f64; 3], [f64; 3]);

/// Tangent, normal and binormal at each path point. The normal is carried from point to
/// point by projection (a parallel-transport approximation), so a profile does not spin.
/// Requires at least two path points.
fn tube_frames(path: &[[f64; 3]], closed: bool) -> Result<Vec<Frame>, String> {
    let n = path.len();
    let mut tangents = Vec::with_capacity(n);
    for i in 0..n {
        let before = if i > 0 {
            Some(path[i - 1])
        } else if closed {
            Some(path[n - 1])
        } else {
            None
        };
        let after = if i + 1 < n {
            Some(path[i + 1])
        } else if closed {
            Some(path[0])
        } else {
            None
        };
        let incoming = before.and_then(|p| unit(sub(path[i], p)));
        let outgoing = after.and_then(|q| unit(sub(q, path[i])));
        let t = match (incoming, outgoing) {
            (Some(a), Some(b)) => unit([a[0] + b[0], a[1] + b[1], a[2] + b[2]]).unwrap_or(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => return Err("パスの向きを決められません。".into()),
        };
        tangents.push(t);
    }
    let mut out = Vec::with_capacity(n);
    let mut previous: Option<[f64; 3]> = None;
    for t in tangents {
        let carried = previous.and_then(|p| {
            let along = dot(p, t);
            unit([
                p[0] - t[0] * along,
                p[1] - t[1] * along,
                p[2] - t[2] * along,
            ])
        });
        let normal = match carried {
            Some(v) => v,
            None => perpendicular(t)?,
        };
        let binormal = cross(t, normal);
        out.push((t, normal, binormal));
        previous = Some(normal);
    }
    Ok(out)
}

/// Sweeps a counter-clockwise profile (in its own XY) along a path. Each ring is the
/// profile placed on the path frame; the quads face outward. Caps close an open path
/// with a centre point per end, and only apply when the path is open.
fn tube(
    path: &[[f64; 3]],
    closed: bool,
    profile: &[[f64; 2]],
    caps: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    let n = path.len();
    let m = profile.len();
    if n < 2 || m < 3 {
        return Err("パイプ・スイープの形が不足しています。".into());
    }
    let segments = if closed { n } else { n - 1 };
    let capped = caps && !closed;
    let (cap_vertices, cap_triangles) = if capped { (2, 2 * m) } else { (0, 0) };
    checked_size(
        n.saturating_mul(m).saturating_add(cap_vertices),
        segments
            .saturating_mul(m)
            .saturating_mul(2)
            .saturating_add(cap_triangles),
    )?;
    let frames = tube_frames(path, closed)?;
    let mut mesh = Mesh::default();
    for (i, p) in path.iter().enumerate() {
        cancelled(cancel)?;
        let (_, normal, binormal) = frames[i];
        for q in profile {
            mesh.vertices.push([
                p[0] + q[0] * normal[0] + q[1] * binormal[0],
                p[1] + q[0] * normal[1] + q[1] * binormal[1],
                p[2] + q[0] * normal[2] + q[1] * binormal[2],
            ]);
        }
    }
    for i in 0..segments {
        cancelled(cancel)?;
        let i2 = (i + 1) % n;
        for j in 0..m {
            let j2 = (j + 1) % m;
            let a = (i * m + j) as u32;
            let b = (i * m + j2) as u32;
            let c = (i2 * m + j) as u32;
            let d = (i2 * m + j2) as u32;
            mesh.triangles.push([a, b, c]);
            mesh.triangles.push([b, d, c]);
        }
    }
    if capped {
        let start = mesh.vertices.len() as u32;
        mesh.vertices.push(path[0]);
        let end = mesh.vertices.len() as u32;
        mesh.vertices.push(path[n - 1]);
        for j in 0..m {
            let j2 = (j + 1) % m;
            mesh.triangles.push([start, (j2) as u32, j as u32]);
            let last = (n - 1) * m;
            mesh.triangles
                .push([end, (last + j) as u32, (last + j2) as u32]);
        }
    }
    if closed || capped {
        orient_outward(&mut mesh);
    }
    validate_mesh(&mesh, cancel)?;
    Ok(mesh)
}

// ------------------------------------------------------------------ curve helpers

/// Cumulative chord lengths along a curve: `cum[k]` is the length up to point `k`.
struct ArcLengths {
    cum: Vec<f64>,
    total: f64,
}

fn arc_lengths(c: &Curve) -> Result<ArcLengths, String> {
    let n = c.points.len();
    if n < 2 {
        return Err("曲線には 2 点以上が必要です。".into());
    }
    let segments = if c.closed { n } else { n - 1 };
    let mut cum = Vec::with_capacity(segments + 1);
    cum.push(0.);
    let mut total = 0.;
    for s in 0..segments {
        total += length(sub(c.points[(s + 1) % n], c.points[s]));
        cum.push(total);
    }
    if !(total.is_finite() && total > 1e-12) {
        return Err("曲線の長さがありません。".into());
    }
    Ok(ArcLengths { cum, total })
}

/// The point at arc length `distance` (clamped to the curve).
fn point_at(c: &Curve, lengths: &ArcLengths, distance: f64) -> [f64; 3] {
    let n = c.points.len();
    let segments = lengths.cum.len() - 1;
    let d = distance.clamp(0., lengths.total);
    let k = lengths
        .cum
        .partition_point(|&x| x <= d)
        .saturating_sub(1)
        .min(segments.saturating_sub(1));
    let (a, b) = (lengths.cum[k], lengths.cum[k + 1]);
    let f = if b - a > 1e-15 {
        ((d - a) / (b - a)).clamp(0., 1.)
    } else {
        0.
    };
    let p = c.points[k];
    let q = c.points[(k + 1) % n];
    [
        p[0] + (q[0] - p[0]) * f,
        p[1] + (q[1] - p[1]) * f,
        p[2] + (q[2] - p[2]) * f,
    ]
}

fn orient2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn on_segment(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    c[0] >= a[0].min(b[0]) - 1e-12
        && c[0] <= a[0].max(b[0]) + 1e-12
        && c[1] >= a[1].min(b[1]) - 1e-12
        && c[1] <= a[1].max(b[1]) + 1e-12
}
/// Whether segment ab and segment cd cross or touch.
fn segments_touch(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    const EPS: f64 = 1e-9;
    let d1 = orient2(c, d, a);
    let d2 = orient2(c, d, b);
    let d3 = orient2(a, b, c);
    let d4 = orient2(a, b, d);
    let strictly_across = |x: f64, y: f64| (x > EPS && y < -EPS) || (x < -EPS && y > EPS);
    if strictly_across(d1, d2) && strictly_across(d3, d4) {
        return true;
    }
    (d1.abs() <= EPS && on_segment(c, d, a))
        || (d2.abs() <= EPS && on_segment(c, d, b))
        || (d3.abs() <= EPS && on_segment(a, b, c))
        || (d4.abs() <= EPS && on_segment(a, b, d))
}

/// Offsets a closed planar curve (XY) by `distance`. Corners are mitred up to `limit`
/// times the distance and bevelled beyond it. A result that folds onto itself, or that
/// collapses, is refused.
fn offset_closed(
    c: &Curve,
    distance: f64,
    limit: f64,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    if !c.closed {
        return Err("曲線のオフセットには閉じた曲線を接続してください。".into());
    }
    if c.points.len() > MAX_OFFSET_POINTS {
        return Err(format!(
            "曲線の点は {MAX_OFFSET_POINTS} 点までにしてください。"
        ));
    }
    let pts = clean_path(&c.points, true);
    if pts.len() < 3 {
        return Err("閉じた曲線には 3 点以上が必要です。".into());
    }
    let n = pts.len();
    let flat: Vec<[f64; 2]> = pts.iter().map(|p| [p[0], p[1]]).collect();
    let area = signed_area_2d(&flat);
    if area.abs() < 1e-9 {
        return Err("曲線の面積がありません。".into());
    }
    if distance.abs() < 1e-9 {
        return curve_value(pts, true);
    }
    // Outward unit normal of each edge: the right-hand side of a counter-clockwise curve.
    let sign = if area > 0. { 1. } else { -1. };
    let mut normals = Vec::with_capacity(n);
    for i in 0..n {
        let (p, q) = (flat[i], flat[(i + 1) % n]);
        let (ex, ey) = (q[0] - p[0], q[1] - p[1]);
        let len = (ex * ex + ey * ey).sqrt();
        if len < 1e-9 {
            return Err("曲線の点が重なっています。".into());
        }
        normals.push([sign * ey / len, -sign * ex / len]);
    }
    let mut raw: Vec<[f64; 3]> = Vec::with_capacity(n * 2);
    for i in 0..n {
        cancelled(cancel)?;
        let n1 = normals[(i + n - 1) % n];
        let n2 = normals[i];
        let p = flat[i];
        let sum = [n1[0] + n2[0], n1[1] + n2[1]];
        let len = (sum[0] * sum[0] + sum[1] * sum[1]).sqrt();
        // Cosine between the mitre direction and each edge normal.
        let cos_half = if len > 1e-9 {
            (sum[0] * n1[0] + sum[1] * n1[1]) / len
        } else {
            0.
        };
        if cos_half >= 1. / limit {
            let scale = distance / cos_half;
            raw.push([
                p[0] + sum[0] / len * scale,
                p[1] + sum[1] / len * scale,
                pts[i][2],
            ]);
        } else {
            raw.push([p[0] + distance * n1[0], p[1] + distance * n1[1], pts[i][2]]);
            raw.push([p[0] + distance * n2[0], p[1] + distance * n2[1], pts[i][2]]);
        }
    }
    let result = clean_path(&raw, true);
    if result.len() < 3 {
        return Err("オフセットの結果が小さすぎます。距離を小さくしてください。".into());
    }
    let flat_result: Vec<[f64; 2]> = result.iter().map(|p| [p[0], p[1]]).collect();
    let new_area = signed_area_2d(&flat_result);
    if new_area * area <= 0. || new_area.abs() < 1e-9 {
        return Err("オフセットで形が消えてしまいます。距離を小さくしてください。".into());
    }
    let m = flat_result.len();
    for i in 0..m {
        cancelled(cancel)?;
        for j in (i + 2)..m {
            if i == 0 && j == m - 1 {
                continue;
            }
            if segments_touch(
                flat_result[i],
                flat_result[(i + 1) % m],
                flat_result[j],
                flat_result[(j + 1) % m],
            ) {
                return Err("オフセットの結果が自分と交わります。距離を小さくするか、角の上限を大きくしてください。".into());
            }
        }
    }
    curve_value(result, true)
}

// ------------------------------------------------------------------ mesh helpers

/// Midpoint subdivision: each triangle becomes four, sharing edge midpoints.
fn subdivide(m: &Mesh, levels: usize, cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    if levels > 3 {
        return Err("細分化は 3 段階までです。".into());
    }
    let mut current = m.clone();
    for _ in 0..levels {
        cancelled(cancel)?;
        let mut edges: Vec<(u32, u32)> = current
            .triangles
            .iter()
            .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
            .map(|(a, b)| (a.min(b), a.max(b)))
            .collect();
        edges.sort_unstable();
        edges.dedup();
        checked_size(
            current.vertices.len().saturating_add(edges.len()),
            current.triangles.len().saturating_mul(4),
        )?;
        current = subdivide_once(&current, cancel)?;
    }
    Ok(current)
}

fn subdivide_once(m: &Mesh, cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    let mut out = Mesh {
        vertices: m.vertices.clone(),
        triangles: Vec::with_capacity(m.triangles.len().saturating_mul(4)),
    };
    let mut midpoints: HashMap<(u32, u32), u32> = HashMap::new();
    for (k, t) in m.triangles.iter().enumerate() {
        if k % 512 == 0 {
            cancelled(cancel)?;
        }
        let ab = midpoint(&mut out, &mut midpoints, t[0], t[1])?;
        let bc = midpoint(&mut out, &mut midpoints, t[1], t[2])?;
        let ca = midpoint(&mut out, &mut midpoints, t[2], t[0])?;
        out.triangles
            .extend([[t[0], ab, ca], [ab, t[1], bc], [ca, bc, t[2]], [ab, bc, ca]]);
    }
    Ok(out)
}

fn midpoint(
    out: &mut Mesh,
    cache: &mut HashMap<(u32, u32), u32>,
    a: u32,
    b: u32,
) -> Result<u32, String> {
    let key = (a.min(b), a.max(b));
    if let Some(&index) = cache.get(&key) {
        return Ok(index);
    }
    let p = *out
        .vertices
        .get(a as usize)
        .ok_or("メッシュの頂点番号が無効です。")?;
    let q = *out
        .vertices
        .get(b as usize)
        .ok_or("メッシュの頂点番号が無効です。")?;
    let mid = [(p[0] + q[0]) / 2., (p[1] + q[1]) / 2., (p[2] + q[2]) / 2.];
    if !finite_point(mid) {
        return Err("細分化の座標が無効です。".into());
    }
    let index = out.vertices.len() as u32;
    out.vertices.push(mid);
    cache.insert(key, index);
    Ok(index)
}

/// Laplacian or Taubin smoothing. Vertices on an open boundary stay where they are.
fn smooth(
    m: &Mesh,
    strength: f64,
    iterations: usize,
    taubin: bool,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    let n = m.vertices.len();
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut edge_uses: BTreeMap<(usize, usize), u32> = BTreeMap::new();
    for t in &m.triangles {
        let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
        if a >= n || b >= n || c >= n {
            return Err("メッシュの頂点番号が無効です。".into());
        }
        for (u, v) in [(a, b), (b, c), (c, a)] {
            neighbours[u].push(v);
            neighbours[v].push(u);
            *edge_uses.entry((u.min(v), u.max(v))).or_insert(0) += 1;
        }
    }
    let mut fixed = vec![false; n];
    for (&(u, v), &uses) in &edge_uses {
        if uses == 1 {
            fixed[u] = true;
            fixed[v] = true;
        }
    }
    for list in &mut neighbours {
        list.sort_unstable();
        list.dedup();
    }
    let mut steps = vec![strength];
    if taubin {
        steps.push(-strength * 1.05);
    }
    let mut current = m.vertices.clone();
    let mut next = current.clone();
    for _ in 0..iterations {
        for &lambda in &steps {
            cancelled(cancel)?;
            for i in 0..n {
                if fixed[i] || neighbours[i].is_empty() {
                    next[i] = current[i];
                    continue;
                }
                let mut avg = [0.; 3];
                for &j in &neighbours[i] {
                    for c in 0..3 {
                        avg[c] += current[j][c];
                    }
                }
                let count = neighbours[i].len() as f64;
                for c in 0..3 {
                    next[i][c] = current[i][c] + lambda * (avg[c] / count - current[i][c]);
                }
            }
            std::mem::swap(&mut current, &mut next);
        }
    }
    let mesh = Mesh {
        vertices: current,
        triangles: m.triangles.clone(),
    };
    validate_mesh(&mesh, cancel)?;
    Ok(mesh)
}

/// Merges vertices that fall in the same tolerance cell, then drops faces that collapse.
fn weld(m: &Mesh, tolerance: f64, cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    if !(tolerance.is_finite() && tolerance > 0.) {
        return Err("許容差は 0 より大きくしてください。".into());
    }
    let mut out = Mesh::default();
    let mut cells: HashMap<[i64; 3], u32> = HashMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(m.vertices.len());
    for (k, p) in m.vertices.iter().enumerate() {
        if k % 512 == 0 {
            cancelled(cancel)?;
        }
        let key = [
            (p[0] / tolerance).round() as i64,
            (p[1] / tolerance).round() as i64,
            (p[2] / tolerance).round() as i64,
        ];
        let index = match cells.get(&key) {
            Some(&index) => index,
            None => {
                let index = out.vertices.len() as u32;
                out.vertices.push(*p);
                cells.insert(key, index);
                index
            }
        };
        remap.push(index);
    }
    for t in &m.triangles {
        let [Some(a), Some(b), Some(c)] = t.map(|v| remap.get(v as usize).copied()) else {
            return Err("メッシュの頂点番号が無効です。".into());
        };
        if a != b && b != c && a != c {
            out.triangles.push([a, b, c]);
        }
    }
    validate_mesh(&out, cancel)?;
    Ok(out)
}

/// Makes neighbouring faces agree, then flips any connected part whose volume is negative.
fn unify(m: &Mesh, cancel: &dyn Fn() -> bool) -> Result<Mesh, String> {
    let faces = m.triangles.len();
    let mut edge_faces: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (k, t) in m.triangles.iter().enumerate() {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            edge_faces.entry((a.min(b), a.max(b))).or_default().push(k);
        }
    }
    let mut flip = vec![false; faces];
    let mut part = vec![usize::MAX; faces];
    let mut parts = 0usize;
    for start in 0..faces {
        if part[start] != usize::MAX {
            continue;
        }
        cancelled(cancel)?;
        part[start] = parts;
        let mut queue = VecDeque::from([start]);
        while let Some(k) = queue.pop_front() {
            let t = effective(m.triangles[k], flip[k]);
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let Some(others) = edge_faces.get(&(a.min(b), a.max(b))) else {
                    continue;
                };
                for &other in others {
                    if other == k || part[other] != usize::MAX {
                        continue;
                    }
                    // A consistent neighbour uses the shared edge in the opposite direction.
                    flip[other] = has_directed(m.triangles[other], a, b);
                    part[other] = parts;
                    queue.push_back(other);
                }
            }
        }
        parts += 1;
    }
    let mut volume = vec![0f64; parts];
    for k in 0..faces {
        let t = effective(m.triangles[k], flip[k]);
        let [Some(p), Some(q), Some(r)] = t.map(|v| m.vertices.get(v as usize).copied()) else {
            return Err("メッシュの頂点番号が無効です。".into());
        };
        volume[part[k]] += dot(p, cross(q, r));
    }
    for k in 0..faces {
        if volume[part[k]] < 0. {
            flip[k] = !flip[k];
        }
    }
    let triangles: Vec<[u32; 3]> = (0..faces)
        .map(|k| effective(m.triangles[k], flip[k]))
        .collect();
    let out = Mesh {
        vertices: m.vertices.clone(),
        triangles,
    };
    validate_mesh(&out, cancel)?;
    Ok(out)
}

fn effective(t: [u32; 3], flipped: bool) -> [u32; 3] {
    if flipped { [t[0], t[2], t[1]] } else { t }
}
fn has_directed(t: [u32; 3], a: u32, b: u32) -> bool {
    [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])].contains(&(a, b))
}

// ------------------------------------------------------------------ gyroid

/// Regular grid of signed values. Planes sit half a step outside and inside the box
/// faces, so the box faces fall between planes and no grid point lies on them.
struct Lattice {
    origin: [f64; 3],
    step: [f64; 3],
    dims: [usize; 3],
    values: Vec<f64>,
}

impl Lattice {
    fn point_at(&self, i: usize, j: usize, l: usize) -> [f64; 3] {
        [
            self.origin[0] + (i as f64 - 0.5) * self.step[0],
            self.origin[1] + (j as f64 - 0.5) * self.step[1],
            self.origin[2] + (l as f64 - 0.5) * self.step[2],
        ]
    }
    fn point(&self, id: usize) -> [f64; 3] {
        let (nx, ny) = (self.dims[0], self.dims[1]);
        self.point_at(id % nx, (id / nx) % ny, id / (nx * ny))
    }
    fn index(&self, i: usize, j: usize, l: usize) -> usize {
        (l * self.dims[1] + j) * self.dims[0] + i
    }
    fn value(&self, id: usize) -> f64 {
        self.values.get(id).copied().unwrap_or(0.)
    }
}

/// Corner offsets of a cube, and six tetrahedra that share the main diagonal 0-6. Every
/// cube uses the same diagonal, so neighbouring cubes agree on their shared faces.
const CUBE_CORNERS: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [1, 1, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [1, 1, 1],
    [0, 1, 1],
];
const CUBE_TETS: [[usize; 4]; 6] = [
    [0, 1, 2, 6],
    [0, 2, 3, 6],
    [0, 3, 7, 6],
    [0, 7, 4, 6],
    [0, 4, 5, 6],
    [0, 5, 1, 6],
];

/// Gyroid sheets thickened to `thickness`, cut to the box. The field is the larger of
/// "distance from the sheet" and "distance outside the box", so the result is a closed
/// solid whose faces lie on the box walls and on the sheets.
fn gyroid(
    size: [f64; 3],
    cell: f64,
    thickness: f64,
    res: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Mesh, String> {
    if !(8..=MAX_GYROID_RES).contains(&res) {
        return Err("分解能は 8 〜 64 にしてください。".into());
    }
    let k = TAU / cell;
    let largest = size[0].max(size[1]).max(size[2]);
    let target = largest / res as f64;
    let counts: [usize; 3] = size.map(|s| ((s / target).ceil() as usize).clamp(1, res));
    let step: [f64; 3] = std::array::from_fn(|a| size[a] / counts[a] as f64);
    let origin = [-size[0] / 2., -size[1] / 2., 0.];
    let centre = [0., 0., size[2] / 2.];
    let half = [size[0] / 2., size[1] / 2., size[2] / 2.];
    let dims = [counts[0] + 2, counts[1] + 2, counts[2] + 2];
    let field = |p: [f64; 3]| -> f64 {
        let f = (k * p[0]).sin() * (k * p[1]).cos()
            + (k * p[1]).sin() * (k * p[2]).cos()
            + (k * p[2]).sin() * (k * p[0]).cos();
        let outside = (0..3)
            .map(|a| (p[a] - centre[a]).abs() - half[a])
            .fold(f64::NEG_INFINITY, f64::max);
        (f.abs() - thickness).max(outside)
    };
    let mut lattice = Lattice {
        origin,
        step,
        dims,
        values: Vec::with_capacity(dims[0] * dims[1] * dims[2]),
    };
    for l in 0..dims[2] {
        cancelled(cancel)?;
        for j in 0..dims[1] {
            for i in 0..dims[0] {
                let g = field(lattice.point_at(i, j, l));
                // A value of exactly zero would put crossings on grid points.
                lattice.values.push(if g.abs() < 1e-9 { 1e-9 } else { g });
            }
        }
    }
    let mut mesh = Mesh::default();
    let mut ids: HashMap<(usize, usize), u32> = HashMap::new();
    for l in 0..dims[2] - 1 {
        cancelled(cancel)?;
        for j in 0..dims[1] - 1 {
            for i in 0..dims[0] - 1 {
                let corners = CUBE_CORNERS.map(|c| lattice.index(i + c[0], j + c[1], l + c[2]));
                for tet in CUBE_TETS {
                    march_tet(&lattice, &mut mesh, &mut ids, tet.map(|c| corners[c]))?;
                }
            }
            if mesh.triangles.len() > MAX_TRIANGLES || mesh.vertices.len() > MAX_VERTICES {
                return Err(
                    "ジャイロイドの形が大きすぎます。分解能か単位の大きさを変えてください。".into(),
                );
            }
        }
    }
    if mesh.triangles.is_empty() {
        return Err(
            "ジャイロイドの壁が見つかりません。太さや単位の大きさを変えてください。".into(),
        );
    }
    Ok(mesh)
}

/// Marching tetrahedra on one tetrahedron. Inside means a negative value.
fn march_tet(
    lattice: &Lattice,
    mesh: &mut Mesh,
    ids: &mut HashMap<(usize, usize), u32>,
    tet: [usize; 4],
) -> Result<(), String> {
    let mut inside = [0usize; 4];
    let mut outside = [0usize; 4];
    let (mut ni, mut no) = (0usize, 0usize);
    for id in tet {
        if lattice.value(id) < 0. {
            inside[ni] = id;
            ni += 1;
        } else {
            outside[no] = id;
            no += 1;
        }
    }
    if ni == 0 || no == 0 {
        return Ok(());
    }
    // Triangles are turned to face the outside vertices of this tetrahedron.
    let outside_centre = centre_of(lattice, &outside[..no]);
    match ni {
        1 => {
            let a = crossing(lattice, mesh, ids, inside[0], outside[0])?;
            let b = crossing(lattice, mesh, ids, inside[0], outside[1])?;
            let c = crossing(lattice, mesh, ids, inside[0], outside[2])?;
            push_oriented(mesh, [a, b, c], outside_centre);
        }
        3 => {
            let a = crossing(lattice, mesh, ids, inside[0], outside[0])?;
            let b = crossing(lattice, mesh, ids, inside[1], outside[0])?;
            let c = crossing(lattice, mesh, ids, inside[2], outside[0])?;
            push_oriented(mesh, [a, b, c], outside_centre);
        }
        _ => {
            let a = crossing(lattice, mesh, ids, inside[0], outside[0])?;
            let b = crossing(lattice, mesh, ids, inside[0], outside[1])?;
            let c = crossing(lattice, mesh, ids, inside[1], outside[1])?;
            let d = crossing(lattice, mesh, ids, inside[1], outside[0])?;
            push_oriented(mesh, [a, b, c], outside_centre);
            push_oriented(mesh, [a, c, d], outside_centre);
        }
    }
    Ok(())
}

fn centre_of(lattice: &Lattice, ids: &[usize]) -> [f64; 3] {
    let mut sum = [0.; 3];
    for &id in ids {
        let p = lattice.point(id);
        for c in 0..3 {
            sum[c] += p[c];
        }
    }
    let count = ids.len().max(1) as f64;
    [sum[0] / count, sum[1] / count, sum[2] / count]
}

/// Vertex where the surface crosses the grid edge between an inside and an outside
/// point. Shared by every tetrahedron that uses the edge, so the mesh stays closed.
fn crossing(
    lattice: &Lattice,
    mesh: &mut Mesh,
    ids: &mut HashMap<(usize, usize), u32>,
    inside: usize,
    outside: usize,
) -> Result<u32, String> {
    let key = (inside.min(outside), inside.max(outside));
    if let Some(&id) = ids.get(&key) {
        return Ok(id);
    }
    let g_in = lattice.value(inside);
    let g_out = lattice.value(outside);
    let t = g_in / (g_in - g_out);
    let (p, q) = (lattice.point(inside), lattice.point(outside));
    let v = [
        p[0] + (q[0] - p[0]) * t,
        p[1] + (q[1] - p[1]) * t,
        p[2] + (q[2] - p[2]) * t,
    ];
    if !finite_point(v) {
        return Err("ジャイロイドの座標が無効です。".into());
    }
    let id = mesh.vertices.len() as u32;
    mesh.vertices.push(v);
    ids.insert(key, id);
    Ok(id)
}

fn push_oriented(mesh: &mut Mesh, tri: [u32; 3], outside: [f64; 3]) {
    let [a, b, c] = tri;
    let (Some(pa), Some(pb), Some(pc)) = (
        mesh.vertices.get(a as usize).copied(),
        mesh.vertices.get(b as usize).copied(),
        mesh.vertices.get(c as usize).copied(),
    ) else {
        return;
    };
    let normal = cross(sub(pb, pa), sub(pc, pa));
    let centre = [
        (pa[0] + pb[0] + pc[0]) / 3.,
        (pa[1] + pb[1] + pc[1]) / 3.,
        (pa[2] + pb[2] + pc[2]) / 3.,
    ];
    if dot(normal, sub(outside, centre)) < 0. {
        mesh.triangles.push([a, c, b]);
    } else {
        mesh.triangles.push([a, b, c]);
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_kinds_are_not_claimed_here() {
        assert!(!is_list_kind(NodeKind::Add));
        assert!(is_list_kind(NodeKind::Polyline));
        assert_eq!(
            spec(NodeKind::Add).label,
            crate::gh_nodes::spec(NodeKind::Add).label
        );
    }

    #[test]
    fn clean_path_drops_repeats_and_closing_point() {
        let p = [[0., 0., 0.], [0., 0., 0.], [1., 0., 0.], [0., 0., 0.]];
        assert_eq!(clean_path(&p, false).len(), 3);
        assert_eq!(clean_path(&p, true).len(), 2);
    }

    #[test]
    fn segments_touch_detects_a_crossing_and_not_a_parallel_pair() {
        assert!(segments_touch([0., 0.], [2., 2.], [0., 2.], [2., 0.]));
        assert!(!segments_touch([0., 0.], [2., 0.], [0., 1.], [2., 1.]));
    }
}
