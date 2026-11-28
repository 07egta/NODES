//! Independent procedural design kernels. No CAD kernel or extra dependency.
//! Curves are sampled polylines, not NURBS. All coordinates are in model units.
mod panels;

use crate::engine::{cancelled, cross, dot, finite_point, length, map_shape, num, shape, sub};
use crate::model::*;
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

mod ornament;

const GEOMETRY: InputSpec = g("geometry", "形状", DataType::Geometry);
const POINTS: InputSpec = g("points", "点リスト", DataType::Point);
const GRID: &[InputSpec] = &[
    i("nx", "列", 8., 1., 64.),
    i("ny", "行", 6., 1., 64.),
    n("size", "間隔 / 半径", 10., 0.01, 1000., 0.5),
];
const MAX_SITES: usize = 256;
const MAX_PAIR_TESTS: usize = 1_000_000;

static RECTGRID: NodeSpec = NodeSpec {
    label: "長方形グリッド",
    category: "配列",
    description: "XY 平面に行優先で等間隔の点を生成。X/Y の個数と間隔を設定します。",
    keywords: "rectangular square grid lattice 格子 四角",
    inputs: &[
        i("nx", "X 個数", 8., 1., 64.),
        i("ny", "Y 個数", 6., 1., 64.),
        n("dx", "X 間隔", 10., 0.01, 1000., 0.5),
        n("dy", "Y 間隔", 10., 0.01, 1000., 0.5),
    ],
    output: DataType::Point,
};
static TRIANGULARGRID: NodeSpec = NodeSpec {
    label: "三角グリッド",
    category: "配列",
    description: "正三角形の格子点を生成。奇数行を半間隔ずらします。",
    keywords: "triangular grid staggered 三角 格子",
    inputs: GRID,
    output: DataType::Point,
};
static HEXGRID: NodeSpec = NodeSpec {
    label: "六角グリッド",
    category: "配列",
    description: "頂点が上を向く正六角形の中心点。半径を指定します。",
    keywords: "hexagonal grid honeycomb 六角 ハニカム",
    inputs: GRID,
    output: DataType::Point,
};
static RADIALGRID: NodeSpec = NodeSpec {
    label: "放射グリッド",
    category: "配列",
    description: "原点と同心円上の点。円周に点を均等に配置します。",
    keywords: "radial grid polar rings 放射 同心円",
    inputs: &[
        i("rings", "円の数", 4., 1., 32.),
        i("spokes", "円周の点数", 12., 3., 128.),
        n("step", "半径間隔", 12., 0.01, 1000., 0.5),
    ],
    output: DataType::Point,
};
static HEXPANELS: NodeSpec = NodeSpec {
    label: "六角パネル",
    category: "曲線",
    description: "六角格子の各セルを閉じた折れ線にします。倍率を 1 未満にすると隙間ができます。",
    keywords: "hexagonal panels honeycomb 六角 ハニカム",
    inputs: &[
        i("nx", "列", 8., 1., 64.),
        i("ny", "行", 6., 1., 64.),
        n("size", "外接円半径", 10., 0.01, 1000., 0.5),
        n("fill", "セル倍率", 0.9, 0.05, 1., 0.05),
    ],
    output: DataType::Curve,
};
static TRIPANELS: NodeSpec = NodeSpec {
    label: "三角パネル",
    category: "曲線",
    description: "斜めにずらした格子の各平行四辺形を正三角形 2 枚に分割します。",
    keywords: "triangular panels tessellation 三角 分割",
    inputs: &[
        i("nx", "列", 8., 1., 32.),
        i("ny", "行", 6., 1., 32.),
        n("size", "辺の長さ", 10., 0.01, 1000., 0.5),
        n("fill", "セル倍率", 0.9, 0.05, 1., 0.05),
    ],
    output: DataType::Curve,
};
static VORONOI2D: NodeSpec = NodeSpec {
    label: "ボロノイ 2D",
    category: "曲線",
    description: "各点に最も近い領域を長方形の中で分割します。XY の点を使用（Z 無視）。同位置の点は統合します。最大 256 点。",
    keywords: "voronoi nearest region partition ボロノイ 空間分割",
    inputs: &[
        POINTS,
        n("width", "境界幅", 100., 0.01, 100000., 1.),
        n("height", "境界高さ", 100., 0.01, 100000., 1.),
    ],
    output: DataType::Curve,
};
static CONVEXHULL2D: NodeSpec = NodeSpec {
    label: "凸包 2D",
    category: "曲線",
    description: "XY 投影の点群を包む最小の凸多角形を作ります。非共線の 3 点以上が必要です。",
    keywords: "convex hull envelope 凸包 外周",
    inputs: &[POINTS],
    output: DataType::Curve,
};
static CLOSESTPOINT: NodeSpec = NodeSpec {
    label: "最近点",
    category: "曲線",
    description: "各検索点について、候補点リストから最も近い 1 点を返します。同距離なら最初の候補。",
    keywords: "closest point nearest neighbor 最近点 近接",
    inputs: &[POINTS, g("targets", "検索点", DataType::Point)],
    output: DataType::Point,
};
static MULTIATTRACTOR: NodeSpec = NodeSpec {
    label: "複数アトラクター",
    category: "数値",
    description: "各点から最も近い引力点までの距離を返します。再マップで半径や高さを変化させられます。",
    keywords: "multiple point attractor distance 引力点 距離 場",
    inputs: &[POINTS, g("targets", "引力点", DataType::Point)],
    output: DataType::Number,
};
static CURVEATTRACTOR: NodeSpec = NodeSpec {
    label: "曲線アトラクター",
    category: "数値",
    description: "点から折れ線の各線分への最短距離を計算します。閉曲線の最後の辺も含みます。",
    keywords: "curve attractor closest distance 曲線 引力 距離",
    inputs: &[
        g("point", "点", DataType::Point),
        g("curve", "曲線", DataType::Curve),
    ],
    output: DataType::Number,
};
static GRIDARRAY: NodeSpec = NodeSpec {
    label: "長方形配列",
    category: "配列",
    description: "形状を X/Y 方向に複製。元の形状を含みます。1 回の生成は最大 1024 個。",
    keywords: "rectangular grid array repeat 配列 複製",
    inputs: &[
        GEOMETRY,
        i("nx", "X 個数", 5., 1., 32.),
        i("ny", "Y 個数", 4., 1., 32.),
        n("dx", "X 間隔", 20., -10000., 10000., 1.),
        n("dy", "Y 間隔", 20., -10000., 10000., 1.),
    ],
    output: DataType::Geometry,
};
static POINTARRAY: NodeSpec = NodeSpec {
    label: "点配置",
    category: "配列",
    description: "各ブランチで 1 個の元形状を点の座標だけ移動して複製します。元形状は原点付近で作ります。",
    keywords: "point based array distribute copy 点 配置 複製",
    inputs: &[POINTS, GEOMETRY],
    output: DataType::Geometry,
};
static ROTATE3D: NodeSpec = NodeSpec {
    label: "任意軸回転 3D",
    category: "変形",
    description: "指定された中心点と軸ベクトルの周りを右手系で回転します。角度は度。ゼロ軸は不可。",
    keywords: "rotate 3d axis angle rodrigues 任意軸 回転",
    inputs: &[
        GEOMETRY,
        g("center", "回転中心", DataType::Point),
        n("ax", "軸 X", 0., -1000., 1000., 0.1),
        n("ay", "軸 Y", 0., -1000., 1000., 0.1),
        n("az", "軸 Z", 1., -1000., 1000., 0.1),
        n("angle", "角度 (度)", 45., -3600., 3600., 1.),
    ],
    output: DataType::Geometry,
};
static SHEAR: NodeSpec = NodeSpec {
    label: "せん断変形",
    category: "変形",
    description: "Z に比例して X/Y を移動します。原点基準のアフィン変換です。",
    keywords: "shear affine skew せん断 傾斜",
    inputs: &[
        GEOMETRY,
        n("xz", "X/Z", 0.25, -10., 10., 0.05),
        n("yz", "Y/Z", 0., -10., 10., 0.05),
    ],
    output: DataType::Geometry,
};
static TAPER: NodeSpec = NodeSpec {
    label: "先細り変形",
    category: "変形",
    description: "形状の最低 Z で倍率 1、最高 Z で指定倍率になるよう XY を変形します。高さのあるメッシュ・折れ線用。",
    keywords: "taper deform scale height 先細り 高さ",
    inputs: &[GEOMETRY, n("factor", "上端の倍率", 0.5, 0.05, 5., 0.05)],
    output: DataType::Geometry,
};
static FRACTALTREE: NodeSpec = NodeSpec {
    label: "フラクタル樹木",
    category: "曲線",
    description: "XZ 平面で枝を再帰的に 2 分割します。深さ 10 まで。幹の長さ、角度、縮小率を変更できます。",
    keywords: "fractal recursive binary tree branch 再帰 フラクタル 樹木",
    inputs: &[
        n("length", "幹の長さ", 30., 0.01, 1000., 1.),
        n("angle", "枝角度 (度)", 28., 1., 89., 1.),
        n("ratio", "枝の縮小率", 0.72, 0.05, 0.95, 0.01),
        i("depth", "深さ", 7., 1., 10.),
    ],
    output: DataType::Curve,
};
static CYLINDERWRAP: NodeSpec = NodeSpec {
    label: "円筒に巻き付け",
    category: "変形",
    description: "平面折れ線の X を円周方向、Y を高さ、Z を半径方向の変位として円筒に巻きます。長い辺は細分化します。",
    keywords: "cylinder wrap cylindrical morph lattice 円筒 曲面 巻き付け",
    inputs: &[
        g("curve", "曲線", DataType::Curve),
        n("radius", "円筒半径", 30., 0.1, 1000., 1.),
        n("width", "一周の X 長さ", 100., 0.01, 100000., 1.),
    ],
    output: DataType::Curve,
};

static DELAUNAY: NodeSpec = NodeSpec {
    label: "ドロネー三角形分割 2D",
    category: "立体",
    description: "XY 点群をドロネー三角形分割し、平らな開いたメッシュを生成します。最大 256 点、重複点統合。ほぼ共線な点群は不可。",
    keywords: "delaunay triangulation mesh ドロネー 三角 点群",
    inputs: &[POINTS],
    output: DataType::Mesh,
};
static POPULATE_MESH: NodeSpec = NodeSpec {
    label: "メッシュ表面に点を配置",
    category: "曲線",
    description: "メッシュの三角形面積に比例した確率で表面上に点をばらまきます。内部の体積には配置しません。同じシードで再現できます。",
    keywords: "populate geometry mesh surface random points seed 表面 点 ランダム",
    inputs: &[
        g("mesh", "メッシュ", DataType::Mesh),
        i("count", "個数", 50., 1., 2048.),
        i("seed", "シード", 1., 0., 100000.),
    ],
    output: DataType::Point,
};
static CURVE_ARRAY: NodeSpec = NodeSpec {
    label: "曲線に沿った配列",
    category: "配列",
    description: "折れ線の弧長に沿って等間隔に形状を平行移動します。閉曲線の終点重複はありません。方向は変えません。",
    keywords: "curve array length arc distribute 配列 曲線 等間隔",
    inputs: &[
        GEOMETRY,
        g("curve", "配置する曲線", DataType::Curve),
        i("count", "個数", 12., 1., 128.),
    ],
    output: DataType::Geometry,
};
static NOISE_FIELD: NodeSpec = NodeSpec {
    label: "ノイズ方向場",
    category: "曲線",
    description: "点ごとに決定的な Perlin 系ノイズを角度へ変換し、XY 平面の短い方向線を生成します。流線や物理的な力ではありません。",
    keywords: "noise field vector direction perlin ノイズ 方向 場",
    inputs: &[
        g("point", "点", DataType::Point),
        n("size", "模様の大きさ", 30., 0.1, 10000., 1.),
        n("length", "方向線の長さ", 6., 0.01, 1000., 0.5),
        i("seed", "シード", 1., 0., 100000.),
    ],
    output: DataType::Curve,
};
static LIFE_GRID: NodeSpec = NodeSpec {
    label: "セルオートマトン（Life）",
    category: "曲線",
    description: "決まったシードで初期化した格子を Conway の Life 規則で反復し、生き残ったセルの中心点を返します。境界外は死セル。",
    keywords: "cellular automaton conway game of life iteration セル オートマトン 反復",
    inputs: &[
        i("nx", "列", 32., 1., 64.),
        i("ny", "行", 32., 1., 64.),
        n("size", "格子間隔", 4., 0.01, 1000., 0.5),
        n("density", "初期密度", 0.35, 0., 1., 0.05),
        i("steps", "反復回数", 12., 0., 100.),
        i("seed", "シード", 1., 0., 100000.),
    ],
    output: DataType::Point,
};

pub(crate) fn spec(kind: NodeKind) -> &'static NodeSpec {
    use NodeKind::*;
    match kind {
        RectGrid => &RECTGRID,
        TriangularGrid => &TRIANGULARGRID,
        HexGrid => &HEXGRID,
        RadialGrid => &RADIALGRID,
        HexPanels => &HEXPANELS,
        TriPanels => &TRIPANELS,
        Voronoi2d => &VORONOI2D,
        ConvexHull2d => &CONVEXHULL2D,
        ClosestPoint => &CLOSESTPOINT,
        MultiAttractor => &MULTIATTRACTOR,
        CurveAttractor => &CURVEATTRACTOR,
        GridArray => &GRIDARRAY,
        PointArray => &POINTARRAY,
        Rotate3d => &ROTATE3D,
        Shear => &SHEAR,
        Taper => &TAPER,
        FractalTree => &FRACTALTREE,
        CylinderWrap => &CYLINDERWRAP,
        Delaunay2d => &DELAUNAY,
        PopulateMesh => &POPULATE_MESH,
        CurveArray => &CURVE_ARRAY,
        NoiseField => &NOISE_FIELD,
        LifeGrid => &LIFE_GRID,
        ShapeMorph => &panels::MORPH,
        InsetCell => &panels::INSET,
        MeshFacePanels => &panels::FACE_PANELS,
        k if ornament::is_kind(k) => ornament::spec(k),
        _ => unreachable!("not a generative node"),
    }
}
pub(crate) fn is_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::RectGrid
            | NodeKind::TriangularGrid
            | NodeKind::HexGrid
            | NodeKind::RadialGrid
            | NodeKind::HexPanels
            | NodeKind::TriPanels
            | NodeKind::Voronoi2d
            | NodeKind::ConvexHull2d
            | NodeKind::ClosestPoint
            | NodeKind::MultiAttractor
            | NodeKind::CurveAttractor
            | NodeKind::GridArray
            | NodeKind::PointArray
            | NodeKind::Rotate3d
            | NodeKind::Shear
            | NodeKind::Taper
            | NodeKind::FractalTree
            | NodeKind::CylinderWrap
            | NodeKind::Delaunay2d
            | NodeKind::PopulateMesh
            | NodeKind::CurveArray
            | NodeKind::NoiseField
            | NodeKind::LifeGrid
            | NodeKind::ShapeMorph
            | NodeKind::InsetCell
            | NodeKind::MeshFacePanels
    ) || ornament::is_kind(kind)
}
pub(crate) fn is_list(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Voronoi2d
            | NodeKind::ConvexHull2d
            | NodeKind::ClosestPoint
            | NodeKind::MultiAttractor
            | NodeKind::PointArray
            | NodeKind::Delaunay2d
            | NodeKind::PowerDiagram2d
            | NodeKind::LloydRelax2d
    )
}

fn count(a: &[&Value], k: usize, limit: usize) -> Result<usize, String> {
    let v = num(a, k)?;
    if v < 1. || v > limit as f64 || v.fract() != 0. {
        return Err("個数が範囲外です。".into());
    }
    Ok(v as usize)
}
fn point(v: &Value) -> Result<[f64; 3], String> {
    match v {
        Value::Point(p) if finite_point(*p) => Ok(*p),
        _ => Err("有効な点が必要です。".into()),
    }
}
fn points(v: &[Value]) -> Result<Vec<[f64; 3]>, String> {
    if v.len() > MAX_ITEMS {
        return Err("点が多すぎます。".into());
    }
    v.iter().map(point).collect()
}
fn list<'a>(lists: &[&'a [Value]], k: usize) -> Result<&'a [Value], String> {
    lists
        .get(k)
        .copied()
        .ok_or_else(|| "入力リストがありません。".into())
}
fn setting(lists: &[&[Value]], k: usize) -> Result<f64, String> {
    list(lists, k)?
        .first()
        .ok_or("設定値がありません。")?
        .number()
}
fn curve(p: Vec<[f64; 3]>, closed: bool) -> Result<Value, String> {
    if p.len() < (if closed { 3 } else { 2 })
        || p.len() > MAX_VERTICES
        || p.iter().any(|q| !finite_point(*q))
    {
        return Err("曲線の座標または点数が無効です。".into());
    }
    Ok(Value::Curve(Arc::new(Curve {
        points: p,
        closed,
        geometry: None,
    })))
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: [f64; 3], k: f64) -> [f64; 3] {
    a.map(|x| x * k)
}
fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = sub(a, b);
    dot(d, d)
}

/// Predict the expensive item outputs before the engine allocates them.
pub(crate) fn estimate(kind: NodeKind, a: &[&Value]) -> Result<(usize, usize), String> {
    use NodeKind::*;
    if ornament::is_kind(kind) {
        return ornament::estimate(kind, a);
    }
    let n = match kind {
        RectGrid | TriangularGrid | HexGrid | HexPanels | GridArray => count(
            a,
            if kind == GridArray { 1 } else { 0 },
            64,
        )?
        .saturating_mul(count(a, if kind == GridArray { 2 } else { 1 }, 64)?),
        TriPanels => 2 * count(a, 0, 32)? * count(a, 1, 32)?,
        RadialGrid => 1 + count(a, 0, 32)? * count(a, 1, 128)?,
        FractalTree => (1usize << count(a, 3, 10)?) - 1,
        PopulateMesh => count(a, 1, 2048)?,
        CurveArray => count(a, 2, 128)?,
        LifeGrid => count(a, 0, 64)? * count(a, 1, 64)?,
        MeshFacePanels => match shape(a)? {
            Value::Mesh(m) => m.triangles.len(),
            _ => return Err("メッシュが必要です。".into()),
        },
        _ => 1,
    };
    let weight = match kind {
        HexPanels => 6 * n,
        TriPanels => 3 * n,
        FractalTree => 2 * n,
        GridArray | CurveArray => shape(a)?.weight().saturating_mul(n),
        NoiseField => 2,
        Rotate3d | Shear | Taper | InsetCell => shape(a)?.weight(),
        ShapeMorph => shape(a)?
            .weight()
            .max(a.get(1).ok_or("終了形状が必要です。")?.weight()),
        MeshFacePanels => n.saturating_mul(3),
        _ => n,
    };
    Ok((weight, n))
}
fn translated(v: &Value, p: [f64; 3], cancel: &dyn Fn() -> bool) -> Result<Value, String> {
    if p == [0.; 3] {
        return Ok(v.clone());
    }
    crate::engine::map_affine_shape(v, |q| add(q, p), cancel)
}
fn array_budget(v: &Value, n: usize) -> Result<(), String> {
    if n > MAX_ITEMS || v.weight().saturating_mul(n) > MAX_TOTAL_WEIGHT / 2 {
        return Err("配列が大きすぎます。個数または形状の分割数を減らしてください。".into());
    }
    Ok(())
}
fn cell_center(kind: NodeKind, x: usize, y: usize, size: f64) -> [f64; 3] {
    let (x, y) = (x as f64, y as f64);
    let offset = if (y as usize).is_multiple_of(2) {
        0.
    } else {
        0.5
    };
    match kind {
        NodeKind::HexGrid | NodeKind::HexPanels => {
            [(x + offset) * 3f64.sqrt() * size, y * 1.5 * size, 0.]
        }
        _ => [(x + offset) * size, y * 3f64.sqrt() * 0.5 * size, 0.],
    }
}
fn shrink_triangle(p: [[f64; 3]; 3], fill: f64) -> Vec<[f64; 3]> {
    let c = mul(add(add(p[0], p[1]), p[2]), 1. / 3.);
    p.into_iter()
        .map(|v| add(c, mul(sub(v, c), fill)))
        .collect()
}
pub(crate) fn run_item(
    kind: NodeKind,
    a: &[&Value],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    cancelled(cancel)?;
    if ornament::is_kind(kind) {
        return ornament::run_item(kind, a, cancel);
    }
    match kind {
        ShapeMorph | InsetCell | MeshFacePanels => panels::run(kind, a, cancel),
        RectGrid | TriangularGrid | HexGrid | HexPanels | TriPanels => {
            let nx = count(a, 0, 64)?;
            let ny = count(a, 1, 64)?;
            let size = num(a, 2)?;
            let mut out = Vec::new();
            for y in 0..ny {
                cancelled(cancel)?;
                for x in 0..nx {
                    match kind {
                        RectGrid => {
                            out.push(Value::Point([x as f64 * size, y as f64 * num(a, 3)?, 0.]))
                        }
                        TriangularGrid | HexGrid => {
                            out.push(Value::Point(cell_center(kind, x, y, size)))
                        }
                        HexPanels => {
                            let c = cell_center(kind, x, y, size);
                            let r = size * num(a, 3)?;
                            let p = (0..6)
                                .map(|k| {
                                    let t = PI / 6. + TAU * k as f64 / 6.;
                                    add(c, [r * t.cos(), r * t.sin(), 0.])
                                })
                                .collect();
                            out.push(curve(p, true)?);
                        }
                        TriPanels => {
                            let h = size * 3f64.sqrt() * 0.5;
                            let p = [(x as f64 + 0.5 * y as f64) * size, y as f64 * h, 0.];
                            let q = add(p, [size, 0., 0.]);
                            let r = add(p, [size * 0.5, h, 0.]);
                            let s = add(q, [size * 0.5, h, 0.]);
                            let fill = num(a, 3)?;
                            out.push(curve(shrink_triangle([p, q, r], fill), true)?);
                            out.push(curve(shrink_triangle([q, s, r], fill), true)?);
                        }
                        _ => unreachable!(),
                    }
                }
            }
            Ok(out)
        }
        RadialGrid => {
            let rings = count(a, 0, 32)?;
            let spokes = count(a, 1, 128)?;
            let step = num(a, 2)?;
            let mut out = vec![Value::Point([0.; 3])];
            for r in 1..=rings {
                cancelled(cancel)?;
                for k in 0..spokes {
                    let t = TAU * k as f64 / spokes as f64;
                    out.push(Value::Point([
                        step * r as f64 * t.cos(),
                        step * r as f64 * t.sin(),
                        0.,
                    ]));
                }
            }
            Ok(out)
        }
        GridArray => {
            let v = shape(a)?;
            let nx = count(a, 1, 32)?;
            let ny = count(a, 2, 32)?;
            let (dx, dy) = (num(a, 3)?, num(a, 4)?);
            array_budget(v, nx * ny)?;
            let mut out = Vec::with_capacity(nx * ny);
            for y in 0..ny {
                for x in 0..nx {
                    cancelled(cancel)?;
                    out.push(translated(v, [x as f64 * dx, y as f64 * dy, 0.], cancel)?);
                }
            }
            Ok(out)
        }
        Rotate3d => {
            let v = shape(a)?;
            let c = point(a.get(1).ok_or("回転中心が必要です。")?)?;
            let axis = [num(a, 2)?, num(a, 3)?, num(a, 4)?];
            let len = length(axis);
            if len < 1e-12 {
                return Err("回転軸の長さがゼロです。".into());
            }
            let u = mul(axis, 1. / len);
            let (s, k) = num(a, 5)?.to_radians().sin_cos();
            Ok(vec![crate::engine::map_affine_shape(
                v,
                |p| {
                    let d = sub(p, c);
                    add(
                        c,
                        add(
                            add(mul(d, k), mul(cross(u, d), s)),
                            mul(u, dot(u, d) * (1. - k)),
                        ),
                    )
                },
                cancel,
            )?])
        }
        Shear => {
            let (x, y) = (num(a, 1)?, num(a, 2)?);
            Ok(vec![crate::engine::map_affine_shape(
                shape(a)?,
                |p| [p[0] + x * p[2], p[1] + y * p[2], p[2]],
                cancel,
            )?])
        }
        Taper => {
            let v = shape(a)?;
            let p = match v {
                Value::Curve(c) => c.points.as_slice(),
                Value::Mesh(m) => m.vertices.as_slice(),
                _ => return Err("高さのある曲線またはメッシュが必要です。".into()),
            };
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for chunk in p.chunks(256) {
                cancelled(cancel)?;
                for q in chunk {
                    lo = lo.min(q[2]);
                    hi = hi.max(q[2]);
                }
            }
            if !lo.is_finite() || hi - lo < 1e-9 {
                return Err("形状の高さがありません。".into());
            }
            let f = num(a, 1)?;
            Ok(vec![map_shape(
                v,
                |p| {
                    let k = 1. + (f - 1.) * (p[2] - lo) / (hi - lo);
                    [p[0] * k, p[1] * k, p[2]]
                },
                cancel,
            )?])
        }
        CurveAttractor => {
            let p = point(shape(a)?)?;
            let Some(Value::Curve(c)) = a.get(1) else {
                return Err("曲線が必要です。".into());
            };
            if c.points.is_empty() {
                return Err("曲線に点がありません。".into());
            }
            let mut best = dist2(p, c.points[0]);
            let edges = c.points.len().saturating_sub(1) + (if c.closed { 1 } else { 0 });
            for k in 0..edges {
                if k % 128 == 0 {
                    cancelled(cancel)?;
                }
                let (x, y) = (c.points[k], c.points[(k + 1) % c.points.len()]);
                let d = sub(y, x);
                let l = dot(d, d);
                let t = if l > 0. {
                    (dot(sub(p, x), d) / l).clamp(0., 1.)
                } else {
                    0.
                };
                best = best.min(dist2(p, add(x, mul(d, t))));
            }
            Ok(vec![Value::Number(best.sqrt())])
        }
        FractalTree => {
            let len = num(a, 0)?;
            let angle = num(a, 1)?.to_radians();
            let ratio = num(a, 2)?;
            let depth = count(a, 3, 10)?;
            let mut stack = vec![([0.; 3], 0.0_f64, len, depth)];
            let mut out = Vec::with_capacity((1usize << depth) - 1);
            while let Some((p, t, l, d)) = stack.pop() {
                cancelled(cancel)?;
                let q = add(p, [l * t.sin(), 0., l * t.cos()]);
                out.push(curve(vec![p, q], false)?);
                if d > 1 {
                    stack.push((q, t + angle, l * ratio, d - 1));
                    stack.push((q, t - angle, l * ratio, d - 1));
                }
            }
            Ok(out)
        }
        CylinderWrap => {
            let Value::Curve(c) = shape(a)? else {
                return Err("曲線が必要です。".into());
            };
            let r = num(a, 1)?;
            let width = num(a, 2)?;
            if c.points.len() < 2 {
                return Err("曲線に点が足りません。".into());
            }
            let wrap = |p: [f64; 3]| {
                let t = TAU * p[0] / width;
                [(r + p[2]) * t.cos(), (r + p[2]) * t.sin(), p[1]]
            };
            let edges = c.points.len() - 1 + (if c.closed { 1 } else { 0 });
            let mut out = Vec::new();
            for k in 0..edges {
                cancelled(cancel)?;
                let (p, q) = (c.points[k], c.points[(k + 1) % c.points.len()]);
                if r + p[2] <= 0. || r + q[2] <= 0. {
                    return Err("半径がゼロ以下になります。".into());
                }
                let n = ((q[0] - p[0]).abs() / width * 36.).ceil().max(1.) as usize;
                if out.len().saturating_add(n) > MAX_VERTICES.min(8192) - 1 {
                    return Err("巻き付けの分割点が多すぎます。".into());
                }
                for j in 0..n {
                    out.push(wrap(add(p, mul(sub(q, p), j as f64 / n as f64))));
                }
            }
            if !c.closed {
                out.push(wrap(*c.points.last().ok_or("点がありません。")?));
            }
            Ok(vec![curve(out, c.closed)?])
        }
        PopulateMesh => {
            let Value::Mesh(mesh) = shape(a)? else {
                return Err("メッシュが必要です。".into());
            };
            populate_mesh(mesh, count(a, 1, 2048)?, num(a, 2)? as u64, cancel)
        }
        CurveArray => {
            let source = shape(a)?;
            let Some(Value::Curve(c)) = a.get(1) else {
                return Err("配置する曲線が必要です。".into());
            };
            curve_array(source, c, count(a, 2, 128)?, cancel)
        }
        NoiseField => {
            let p = point(shape(a)?)?;
            let size = num(a, 1)?;
            let len = num(a, 2)?;
            let t = PI * crate::gh_nodes::perlin(p.map(|v| v / size), num(a, 3)? as u64)?;
            Ok(vec![curve(
                vec![p, add(p, [len * t.cos(), len * t.sin(), 0.])],
                false,
            )?])
        }
        LifeGrid => {
            let nx = count(a, 0, 64)?;
            let ny = count(a, 1, 64)?;
            let size = num(a, 2)?;
            let density = num(a, 3)?;
            let steps = num(a, 4)? as usize;
            let mut rng = crate::gh_nodes::Rng::new(num(a, 5)? as u64);
            let mut cells = (0..nx * ny)
                .map(|_| rng.next_f64() < density)
                .collect::<Vec<_>>();
            for _ in 0..steps {
                cancelled(cancel)?;
                cells = life_step(&cells, nx, ny);
            }
            Ok(cells
                .into_iter()
                .enumerate()
                .filter(|(_, alive)| *alive)
                .map(|(k, _)| Value::Point([(k % nx) as f64 * size, (k / nx) as f64 * size, 0.]))
                .collect())
        }
        _ => Err("このノードにはリスト評価が必要です。".into()),
    }
}

pub(crate) fn run_list(
    kind: NodeKind,
    lists: &[&[Value]],
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    use NodeKind::*;
    cancelled(cancel)?;
    let input = points(list(lists, 0)?)?;
    if ornament::is_kind(kind) {
        return ornament::run_list(kind, &input, lists, cancel);
    }
    if kind == PointArray && list(lists, 1)?.len() != 1 {
        return Err("点配置にはブランチごとに元形状が 1 個必要です。".into());
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }
    match kind {
        Voronoi2d => voronoi(&input, setting(lists, 1)?, setting(lists, 2)?, cancel),
        ConvexHull2d => hull(&input, cancel),
        Delaunay2d => delaunay(&input, cancel),
        ClosestPoint | MultiAttractor => {
            let targets = points(list(lists, 1)?)?;
            // ClosestPoint: candidates first, queries second. MultiAttractor: queries first.
            let (queries, candidates) = if kind == ClosestPoint {
                (&targets, &input)
            } else {
                (&input, &targets)
            };
            if candidates.is_empty() {
                return Err("候補点がありません。".into());
            }
            if queries.len().saturating_mul(candidates.len()) > MAX_PAIR_TESTS {
                return Err("近接点の比較回数が上限を超えます。".into());
            }
            let mut out = Vec::with_capacity(queries.len());
            for p in queries {
                cancelled(cancel)?;
                let (mut best, mut d) = (candidates[0], dist2(*p, candidates[0]));
                for (k, q) in candidates.iter().enumerate() {
                    if k % 256 == 0 {
                        cancelled(cancel)?;
                    }
                    let e = dist2(*p, *q);
                    if e < d {
                        best = *q;
                        d = e;
                    }
                }
                out.push(if kind == ClosestPoint {
                    Value::Point(best)
                } else {
                    Value::Number(d.sqrt())
                });
            }
            Ok(out)
        }
        PointArray => {
            let shapes = list(lists, 1)?;
            array_budget(&shapes[0], input.len())?;
            input
                .into_iter()
                .map(|p| {
                    cancelled(cancel)?;
                    translated(&shapes[0], p, cancel)
                })
                .collect()
        }
        _ => Err("未対応のリストノードです。".into()),
    }
}
type P2 = [f64; 2];
fn side(a: P2, b: P2, p: P2) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
fn d2(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}
fn area(p: &[P2]) -> f64 {
    p.iter()
        .zip(p.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
        .sum::<f64>()
        * 0.5
}
fn clean(mut p: Vec<P2>) -> Vec<P2> {
    p.dedup_by(|a, b| d2(*a, *b) < 1e-24);
    if p.len() > 1 && d2(p[0], p[p.len() - 1]) < 1e-24 {
        p.pop();
    }
    // Eliminate redundant collinear vertices (including at the closing edge).
    loop {
        if p.len() < 3 {
            break;
        }
        let n = p.len();
        let bad = (0..n).find(|&i| side(p[(i + n - 1) % n], p[i], p[(i + 1) % n]).abs() < 1e-14);
        if let Some(i) = bad {
            p.remove(i);
        } else {
            break;
        }
    }
    p
}
fn voronoi(
    input: &[[f64; 3]],
    w: f64,
    h: f64,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    if input.len() > MAX_SITES {
        return Err("ボロノイは 256 点までです。".into());
    }
    if !w.is_finite() || !h.is_finite() || w <= 0. || h <= 0. {
        return Err("境界寸法が無効です。".into());
    }
    let scale = w.max(h);
    let (wx, hy) = (w / scale * 0.5, h / scale * 0.5);
    let mut sites: Vec<P2> = Vec::new();
    for p in input {
        cancelled(cancel)?;
        let q = [p[0] / scale, p[1] / scale];
        if q[0].abs() > wx || q[1].abs() > hy {
            return Err("点を境界の長方形内に配置してください。".into());
        }
        if !sites.iter().any(|s| d2(*s, q) <= 1e-18) {
            sites.push(q);
        }
    }
    let mut out = Vec::with_capacity(sites.len());
    for (i, &a) in sites.iter().enumerate() {
        cancelled(cancel)?;
        let mut poly = vec![[-wx, -hy], [wx, -hy], [wx, hy], [-wx, hy]];
        for (j, &b) in sites.iter().enumerate() {
            if j % 16 == 0 {
                cancelled(cancel)?;
            }
            if i == j || poly.is_empty() {
                continue;
            }
            let normal = [b[0] - a[0], b[1] - a[1]];
            let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
            let f = |p: P2| (p[0] - mid[0]) * normal[0] + (p[1] - mid[1]) * normal[1];
            let mut clipped = Vec::with_capacity(poly.len() + 1);
            for (&p, &q) in poly.iter().zip(poly.iter().cycle().skip(1)) {
                let (fp, fq) = (f(p), f(q));
                if fp <= 0. {
                    clipped.push(p);
                }
                if (fp <= 0.) != (fq <= 0.) {
                    let t = (fp / (fp - fq)).clamp(0., 1.);
                    clipped.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
                }
            }
            poly = clipped;
        }
        let poly = clean(poly);
        if poly.len() < 3 || area(&poly) <= 1e-16 {
            return Err("極小ボロノイセルができました。点の間隔を広げてください。".into());
        }
        out.push(curve(
            poly.into_iter()
                .map(|p| [p[0] * scale, p[1] * scale, 0.])
                .collect(),
            true,
        )?);
    }
    Ok(out)
}
fn hull(input: &[[f64; 3]], cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    let mut p: Vec<P2> = input.iter().map(|p| [p[0], p[1]]).collect();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return Err("凸包には異なる 3 点以上が必要です。".into());
    }
    let span = (p[p.len() - 1][0] - p[0][0]).max(
        p.iter().map(|q| q[1]).fold(f64::NEG_INFINITY, f64::max)
            - p.iter().map(|q| q[1]).fold(f64::INFINITY, f64::min),
    );
    let eps = span * span * 1e-12;
    let mut lo: Vec<P2> = Vec::new();
    let mut hi: Vec<P2> = Vec::new();
    for (k, &q) in p.iter().enumerate() {
        if k % 128 == 0 {
            cancelled(cancel)?;
        }
        while lo.len() >= 2 && side(lo[lo.len() - 2], lo[lo.len() - 1], q) <= eps {
            lo.pop();
        }
        lo.push(q);
    }
    for (k, &q) in p.iter().rev().enumerate() {
        if k % 128 == 0 {
            cancelled(cancel)?;
        }
        while hi.len() >= 2 && side(hi[hi.len() - 2], hi[hi.len() - 1], q) <= eps {
            hi.pop();
        }
        hi.push(q);
    }
    lo.pop();
    hi.pop();
    lo.extend(hi);
    if lo.len() < 3 {
        return Err("点がほぼ同一直線上にあります。".into());
    }
    Ok(vec![curve(
        lo.into_iter().map(|p| [p[0], p[1], 0.]).collect(),
        true,
    )?])
}

fn life_step(cells: &[bool], nx: usize, ny: usize) -> Vec<bool> {
    let mut next = vec![false; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            let mut neighbors = 0;
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (u, v) = (x as isize + dx, y as isize + dy);
                    if u >= 0
                        && v >= 0
                        && u < nx as isize
                        && v < ny as isize
                        && cells[v as usize * nx + u as usize]
                    {
                        neighbors += 1;
                    }
                }
            }
            next[y * nx + x] = neighbors == 3 || (cells[y * nx + x] && neighbors == 2);
        }
    }
    next
}
fn populate_mesh(
    mesh: &Mesh,
    n: usize,
    seed: u64,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let mut weights = Vec::with_capacity(mesh.triangles.len());
    let mut total = 0.;
    for (i, t) in mesh.triangles.iter().enumerate() {
        if i % 256 == 0 {
            cancelled(cancel)?;
        }
        let [Some(a), Some(b), Some(c)] = t.map(|j| mesh.vertices.get(j as usize)) else {
            return Err("無効なメッシュの頂点番号です。".into());
        };
        total += length(cross(sub(*b, *a), sub(*c, *a))) * 0.5;
        weights.push(total);
    }
    if !total.is_finite() || total <= 0. {
        return Err("面積のあるメッシュが必要です。".into());
    }
    let mut rng = crate::gh_nodes::Rng::new(seed);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        cancelled(cancel)?;
        let pick = rng.next_f64() * total;
        let index = weights
            .partition_point(|w| *w <= pick)
            .min(weights.len() - 1);
        let t = mesh.triangles[index];
        let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
        // sqrt correction gives uniform area density, not uniform barycentric coordinates.
        let r = rng.next_f64().sqrt();
        let s = rng.next_f64();
        out.push(Value::Point(add(
            add(mul(a, 1. - r), mul(b, r * (1. - s))),
            mul(c, r * s),
        )));
    }
    Ok(out)
}
fn curve_array(
    source: &Value,
    c: &Curve,
    n: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    array_budget(source, n)?;
    if c.points.len() < 2 {
        return Err("曲線に点が足りません。".into());
    }
    let edges = c.points.len() - 1 + (if c.closed { 1 } else { 0 });
    let mut end = Vec::with_capacity(edges);
    let mut total = 0.;
    for k in 0..edges {
        if k % 256 == 0 {
            cancelled(cancel)?;
        }
        total += length(sub(c.points[(k + 1) % c.points.len()], c.points[k]));
        end.push(total);
    }
    if total <= 1e-12 || !total.is_finite() {
        return Err("曲線の長さがありません。".into());
    }
    let divisor = if c.closed {
        n
    } else {
        n.saturating_sub(1).max(1)
    };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        cancelled(cancel)?;
        let d = total * i as f64 / divisor as f64;
        let k = end.partition_point(|x| *x <= d).min(edges - 1);
        let start = if k == 0 { 0. } else { end[k - 1] };
        let t = if end[k] > start {
            ((d - start) / (end[k] - start)).clamp(0., 1.)
        } else {
            0.
        };
        let p = add(
            c.points[k],
            mul(sub(c.points[(k + 1) % c.points.len()], c.points[k]), t),
        );
        out.push(translated(source, p, cancel)?);
    }
    Ok(out)
}
fn incircle(a: P2, b: P2, c: P2, p: P2) -> f64 {
    let (ax, ay) = (a[0] - p[0], a[1] - p[1]);
    let (bx, by) = (b[0] - p[0], b[1] - p[1]);
    let (cx, cy) = (c[0] - p[0], c[1] - p[1]);
    (ax * ax + ay * ay) * (bx * cy - by * cx) - (bx * bx + by * by) * (ax * cy - ay * cx)
        + (cx * cx + cy * cy) * (ax * by - ay * bx)
}
/// Incremental Bowyer-Watson on normalized planar coordinates. Reject ambiguous
/// degeneracies rather than return a mesh with missing sites or invalid triangles.
fn delaunay(input: &[[f64; 3]], cancel: &dyn Fn() -> bool) -> Result<Vec<Value>, String> {
    // A finite supertriangle can suppress a true hull triangle near collinearity.
    // Retry a bounded number of larger enclosures only when the hull audit fails.
    let mut last = String::new();
    for extent in [32., 128., 512., 2048.] {
        match delaunay_enclosed(input, extent, cancel) {
            Ok(mesh) => return Ok(mesh),
            Err(error) if error.starts_with("凸包境界") => last = error,
            Err(error) => return Err(error),
        }
    }
    Err(last)
}
fn delaunay_enclosed(
    input: &[[f64; 3]],
    extent: f64,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    if input.len() > MAX_SITES {
        return Err("ドロネー分割は 256 点までです。".into());
    }
    if input.len() < 3 {
        return Err("異なる 3 点以上が必要です。".into());
    }
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in input {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let scale = (hi[0] - lo[0]).max(hi[1] - lo[1]);
    if scale <= 0. || !scale.is_finite() {
        return Err("点が重なっています。".into());
    }
    let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    let mut p: Vec<P2> = Vec::new();
    let mut vertices = Vec::new();
    // Stable insertion order also makes co-circular tie selection deterministic.
    for q in input {
        cancelled(cancel)?;
        let r = [(q[0] - center[0]) / scale, (q[1] - center[1]) / scale];
        if !p.iter().any(|s| d2(*s, r) <= 1e-18) {
            p.push(r);
            vertices.push([q[0], q[1], 0.]);
        }
    }
    let n = p.len();
    if n < 3 {
        return Err("異なる 3 点以上が必要です。".into());
    }
    p.extend([
        [-extent, -extent * 0.5],
        [extent, -extent * 0.5],
        [0., extent],
    ]);
    let mut tris = vec![[n, n + 1, n + 2]];
    for i in 0..n {
        cancelled(cancel)?;
        let mut keep = Vec::new();
        let mut boundary = std::collections::BTreeMap::<(usize, usize), usize>::new();
        for t in tris {
            let det = incircle(p[t[0]], p[t[1]], p[t[2]], p[i]);
            if det > 0. {
                for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    *boundary.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            } else {
                keep.push(t);
            }
        }
        if boundary.is_empty() {
            return Err("数値的に不安定な点配置です。".into());
        }
        for ((a, b), count) in boundary {
            if count > 2 {
                return Err("分割の境界が不正です。".into());
            }
            if count != 1 {
                continue;
            }
            let sign = side(p[a], p[b], p[i]);
            if sign > 1e-14 {
                keep.push([a, b, i]);
            } else if sign < -1e-14 {
                keep.push([b, a, i]);
            }
        }
        if keep.len() > MAX_SITES * 8 {
            return Err("三角形数が上限を超えました。".into());
        }
        tris = keep;
    }
    tris.retain(|t| t.iter().all(|&i| i < n));
    if tris.is_empty() {
        return Err("点がほぼ同一直線上にあります。".into());
    }
    let mut used = vec![false; n];
    let mut edges = std::collections::BTreeMap::<(usize, usize), usize>::new();
    let mut unique = std::collections::BTreeSet::new();
    for t in &tris {
        cancelled(cancel)?;
        let mut key = *t;
        key.sort_unstable();
        if !unique.insert(key) {
            return Err("重複した三角形ができました。".into());
        }
        for &i in t {
            used[i] = true;
        }
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
        // Check empty circumcircle on normalized positions, with a small roundoff allowance.
        for (k, &q) in p[..n].iter().enumerate() {
            if !t.contains(&k) && incircle(p[t[0]], p[t[1]], p[t[2]], q) > 1e-12 {
                return Err("ドロネー条件を満たせません。点の間隔を調整してください。".into());
            }
        }
    }
    if used.iter().any(|v| !*v)
        || edges.values().any(|&v| v > 2)
        || n + tris.len() != edges.len() + 1
    {
        return Err("退化した分割です。点の間隔を広げてください。".into());
    }
    // Euler and empty circumcircles do not prove that the convex hull is covered.
    // Every exterior directed edge of a CCW triangulation must support the hull:
    // all sites lie on its left. This rejects concave notches left by the enclosure.
    for t in &tris {
        cancelled(cancel)?;
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            if edges[&(a.min(b), a.max(b))] == 1
                && p[..n].iter().any(|&q| side(p[a], p[b], q) < -1e-12)
            {
                return Err("凸包境界を完全に分割できません。点の間隔を広げてください。".into());
            }
        }
    }
    crate::engine::as_mesh(
        Mesh {
            vertices,
            triangles: tris.into_iter().map(|t| t.map(|i| i as u32)).collect(),
        },
        cancel,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(v: &Value) -> &Curve {
        let Value::Curve(c) = v else {
            panic!("curve expected")
        };
        c
    }
    fn area_xy(v: &Value) -> f64 {
        area(&c(v).points.iter().map(|p| [p[0], p[1]]).collect::<Vec<_>>())
    }
    #[test]
    fn two_voronoi_cells_cover_boundary_and_duplicates_are_merged() {
        let sites = [[-2., 0., 0.], [2., 0., 0.], [-2., 0., 9.]];
        let cells = voronoi(&sites, 10., 8., &|| false).unwrap();
        assert_eq!(cells.len(), 2);
        for v in &cells {
            assert!(c(v).closed);
            assert!((area_xy(v) - 40.).abs() < 1e-8);
        }
        assert!(c(&cells[0]).points.iter().all(|p| p[0] <= 1e-9));
        assert!(c(&cells[1]).points.iter().all(|p| p[0] >= -1e-9));
    }
    #[test]
    fn voronoi_collinear_sites_partition_rectangle() {
        let sites = [[-3., 0., 0.], [0., 1e-11, 0.], [3., 0., 0.]];
        let cells = voronoi(&sites, 12., 8., &|| false).unwrap();
        assert_eq!(cells.len(), 3);
        assert!((cells.iter().map(area_xy).sum::<f64>() - 96.).abs() < 1e-7);
    }
    #[test]
    fn rejects_outside_and_excess_sites_and_cancels() {
        assert!(voronoi(&[[6., 0., 0.]], 10., 10., &|| false).is_err());
        assert!(voronoi(&vec![[0.; 3]; 257], 10., 10., &|| false).is_err());
        assert!(voronoi(&[[0.; 3]], 10., 10., &|| true).is_err());
    }
    #[test]
    fn hull_removes_interior_duplicates_and_collinear_points() {
        let p = [
            [0., 0., 0.],
            [2., 0., 0.],
            [2., 2., 0.],
            [0., 2., 0.],
            [1., 1., 0.],
            [1., 0., 0.],
            [0., 0., 0.],
        ];
        let v = hull(&p, &|| false).unwrap();
        assert_eq!(c(&v[0]).points.len(), 4);
        assert!((area_xy(&v[0]) - 4.).abs() < 1e-10);
        assert!(hull(&[[0.; 3], [1., 0., 0.], [2., 0., 0.]], &|| false).is_err());
    }
    #[test]
    fn nearest_ties_are_stable_and_attractor_distances_match() {
        let candidates = [Value::Point([-1., 0., 0.]), Value::Point([1., 0., 0.])];
        let q = [Value::Point([0.; 3])];
        assert_eq!(
            run_list(NodeKind::ClosestPoint, &[&candidates, &q], &|| false).unwrap(),
            vec![candidates[0].clone()]
        );
        assert_eq!(
            run_list(NodeKind::MultiAttractor, &[&q, &candidates], &|| false).unwrap(),
            vec![Value::Number(1.)]
        );
    }
    #[test]
    fn grid_and_recursive_tree_have_expected_counts() {
        let grid = [
            Value::Number(3.),
            Value::Number(2.),
            Value::Number(10.),
            Value::Number(20.),
        ];
        let a = grid.iter().collect::<Vec<_>>();
        let result = run_item(NodeKind::RectGrid, &a, &|| false).unwrap();
        assert_eq!(result.len(), 6);
        assert_eq!(result[5], Value::Point([20., 20., 0.]));
        let args = [
            Value::Number(30.),
            Value::Number(25.),
            Value::Number(0.7),
            Value::Number(5.),
        ];
        let a = args.iter().collect::<Vec<_>>();
        assert_eq!(
            run_item(NodeKind::FractalTree, &a, &|| false)
                .unwrap()
                .len(),
            31
        );
    }
    #[test]
    fn arbitrary_axis_rotation_preserves_distance_to_center() {
        let args = [
            Value::Point([3., 1., 0.]),
            Value::Point([2., 1., 0.]),
            Value::Number(0.),
            Value::Number(0.),
            Value::Number(2.),
            Value::Number(90.),
        ];
        let a = args.iter().collect::<Vec<_>>();
        let v = run_item(NodeKind::Rotate3d, &a, &|| false).unwrap();
        let p = point(&v[0]).unwrap();
        assert!(dist2(p, [2., 2., 0.]) < 1e-20);
    }
    #[test]
    fn wrapping_preserves_closed_flag_and_subdivides() {
        let src = curve(
            vec![[0., 0., 0.], [50., 0., 0.], [50., 10., 0.], [0., 10., 0.]],
            true,
        )
        .unwrap();
        let a = [src, Value::Number(20.), Value::Number(100.)];
        let refs = a.iter().collect::<Vec<_>>();
        let v = run_item(NodeKind::CylinderWrap, &refs, &|| false).unwrap();
        assert!(c(&v[0]).closed);
        assert!(c(&v[0]).points.len() > 4);
        for p in &c(&v[0]).points {
            assert!((p[0].hypot(p[1]) - 20.).abs() < 1e-10);
        }
    }
    #[test]
    fn point_array_rejects_multiple_sources_and_shares_original() {
        let points = [Value::Point([0.; 3]), Value::Point([3., 0., 0.])];
        let s = curve(vec![[0.; 3], [0., 1., 0.]], false).unwrap();
        let out = run_list(
            NodeKind::PointArray,
            &[&points, std::slice::from_ref(&s)],
            &|| false,
        )
        .unwrap();
        let (Value::Curve(a), Value::Curve(b)) = (&s, &out[0]) else {
            panic!("curve")
        };
        assert!(Arc::ptr_eq(a, b));
        assert_eq!(c(&out[1]).points[0], [3., 0., 0.]);
        assert!(run_list(NodeKind::PointArray, &[&points, &[s.clone(), s]], &|| false).is_err());
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;
    #[test]
    fn delaunay_square_duplicate_and_interior_point() {
        let p = [
            [0., 0., 0.],
            [4., 0., 0.],
            [4., 4., 0.],
            [0., 4., 0.],
            [2., 2., 0.],
            [0., 0., 5.],
        ];
        let v = delaunay(&p, &|| false).unwrap();
        let Value::Mesh(m) = &v[0] else {
            panic!("mesh");
        };
        assert_eq!(m.vertices.len(), 5);
        assert_eq!(m.triangles.len(), 4);
        let total = m
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| m.vertices[i as usize]);
                length(cross(sub(b, a), sub(c, a))) * 0.5
            })
            .sum::<f64>();
        assert!((total - 16.).abs() < 1e-9);
        assert!(delaunay(&[[0.; 3], [1., 0., 0.], [2., 0., 0.]], &|| false).is_err());
    }
    #[test]
    fn mesh_population_reproducible_and_inside_triangle() {
        let m = Mesh {
            vertices: vec![[0.; 3], [2., 0., 0.], [0., 2., 0.]],
            triangles: vec![[0, 1, 2]],
        };
        let a = populate_mesh(&m, 100, 12, &|| false).unwrap();
        assert_eq!(a, populate_mesh(&m, 100, 12, &|| false).unwrap());
        for v in a {
            let p = point(&v).unwrap();
            assert!(p[0] >= 0. && p[1] >= 0. && p[0] + p[1] <= 2. + 1e-12 && p[2] == 0.);
        }
    }
    #[test]
    fn curve_array_spacing_uses_arc_length_and_not_vertex_indices() {
        let c = Curve {
            points: vec![[0.; 3], [1., 0., 0.], [5., 0., 0.]],
            closed: false,
            geometry: None,
        };
        let a = curve_array(&Value::Point([0.; 3]), &c, 3, &|| false).unwrap();
        assert_eq!(
            a,
            vec![
                Value::Point([0.; 3]),
                Value::Point([2.5, 0., 0.]),
                Value::Point([5., 0., 0.])
            ]
        );
    }
    #[test]
    fn life_blinker_returns_after_two_steps() {
        let mut cells = vec![false; 25];
        cells[11] = true;
        cells[12] = true;
        cells[13] = true;
        let one = life_step(&cells, 5, 5);
        assert!(one[7] && one[12] && one[17]);
        assert_eq!(one.iter().filter(|v| **v).count(), 3);
        assert_eq!(life_step(&one, 5, 5), cells);
    }
}

#[cfg(test)]
mod hull_coverage_regressions {
    use super::*;
    #[test]
    fn near_collinear_interior_site_cannot_leave_a_hole_in_the_convex_hull() {
        let points = [
            [-0.1, -0.003, 0.],
            [0.5, 0., 0.],
            [-0.4, -0.005, 0.],
            [0., 0.5, 0.],
        ];
        let values = delaunay(&points, &|| false).unwrap();
        let Value::Mesh(mesh) = &values[0] else {
            panic!("mesh required")
        };
        assert_eq!(mesh.triangles.len(), 3);
        let area: f64 = mesh
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
                length(cross(sub(b, a), sub(c, a))) * 0.5
            })
            .sum();
        assert!((area - 0.22625).abs() < 1e-14, "{area}");
    }
}
