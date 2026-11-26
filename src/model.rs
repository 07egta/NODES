//! Serializable, renderer-independent graph model. Geometry is evaluated separately.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

pub type NodeId = u64;
pub const MAX_NODES: usize = 2_048;
pub const MAX_ITEMS: usize = 8_192;
pub const MAX_VERTICES: usize = 250_000;
pub const MAX_TRIANGLES: usize = 500_000;
pub const MAX_TOTAL_WEIGHT: usize = 3_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    Number,
    Point,
    Curve,
    Mesh,
    Geometry,
    Any,
}
impl DataType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Number => "数値",
            Self::Point => "点",
            Self::Curve => "曲線",
            Self::Mesh => "メッシュ",
            Self::Geometry => "形状",
            Self::Any => "データ",
        }
    }
    pub fn accepts(self, source: Self) -> bool {
        self == Self::Any
            || source == Self::Any
            || self == source
            || (self == Self::Geometry && matches!(source, Self::Point | Self::Curve | Self::Mesh))
            || (source == Self::Geometry && matches!(self, Self::Point | Self::Curve | Self::Mesh))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InputSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: DataType,
    pub default: Option<f64>,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub integer: bool,
}
#[derive(Debug)]
pub struct NodeSpec {
    pub label: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub keywords: &'static str,
    pub inputs: &'static [InputSpec],
    pub output: DataType,
}
const fn n(
    key: &'static str,
    label: &'static str,
    default: f64,
    min: f64,
    max: f64,
    step: f64,
) -> InputSpec {
    InputSpec {
        key,
        label,
        kind: DataType::Number,
        default: Some(default),
        min,
        max,
        step,
        integer: false,
    }
}
const fn i(key: &'static str, label: &'static str, default: f64, min: f64, max: f64) -> InputSpec {
    InputSpec {
        key,
        label,
        kind: DataType::Number,
        default: Some(default),
        min,
        max,
        step: 1.,
        integer: true,
    }
}
const fn g(key: &'static str, label: &'static str, kind: DataType) -> InputSpec {
    InputSpec {
        key,
        label,
        kind,
        default: None,
        min: 0.,
        max: 0.,
        step: 0.,
        integer: false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Number,
    Add,
    Multiply,
    Divide,
    Series,
    Point,
    Circle,
    Polygon,
    Star,
    Rectangle,
    Helix,
    Extrude,
    Loft,
    Revolve,
    Vase,
    Sphere,
    Box,
    Move,
    Rotate,
    Scale,
    Twist,
    Wave,
    LinearArray,
    RadialArray,
    Merge,
    Flatten,
    Graft,
    Output,
}
impl NodeKind {
    pub fn all() -> &'static [Self] {
        &[
            Self::Number,
            Self::Add,
            Self::Multiply,
            Self::Divide,
            Self::Series,
            Self::Point,
            Self::Circle,
            Self::Polygon,
            Self::Star,
            Self::Rectangle,
            Self::Helix,
            Self::Extrude,
            Self::Loft,
            Self::Revolve,
            Self::Vase,
            Self::Sphere,
            Self::Box,
            Self::Move,
            Self::Rotate,
            Self::Scale,
            Self::Twist,
            Self::Wave,
            Self::LinearArray,
            Self::RadialArray,
            Self::Merge,
            Self::Flatten,
            Self::Graft,
            Self::Output,
        ]
    }
    pub fn spec(self) -> &'static NodeSpec {
        match self {
            Self::Number => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "数値",
                    category: "数値",
                    description: "数値を他のノードへ渡します。複数の形の寸法を一緒に調整できます。",
                    keywords: "number slider パラメータ スライダー",
                    inputs: &[n("value", "値", 1., -1e6, 1e6, 0.1)],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Add => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "足し算",
                    category: "数値",
                    description: "A + B を計算。リストは順に対応し、短い側の最後の値を繰り返します。",
                    keywords: "add plus sum 和 加算",
                    inputs: &[
                        n("a", "A", 0., -1e6, 1e6, 0.1),
                        n("b", "B", 10., -1e6, 1e6, 0.1),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Multiply => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "掛け算",
                    category: "数値",
                    description: "A × B を計算します。",
                    keywords: "multiply product 積 乗算 倍",
                    inputs: &[
                        n("a", "A", 1., -1e6, 1e6, 0.1),
                        n("b", "B", 2., -1e6, 1e6, 0.1),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Divide => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "割り算",
                    category: "数値",
                    description: "A ÷ B を計算します。ゼロでの割り算はエラーを表示します。",
                    keywords: "divide quotient 除算 比率",
                    inputs: &[
                        n("a", "A", 1., -1e6, 1e6, 0.1),
                        n("b", "B", 2., -1e6, 1e6, 0.1),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Series => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "数列",
                    category: "データ",
                    description: "開始値から等間隔の数値リストを生成。位置や角度をまとめて変えられます。",
                    keywords: "series range list 連番 リスト 配列",
                    inputs: &[
                        n("start", "開始", 0., -1e5, 1e5, 1.),
                        n("step", "間隔", 10., -1e4, 1e4, 0.1),
                        i("count", "個数", 8., 0., 512.),
                    ],
                    output: DataType::Number,
                };
                &SPEC
            }
            Self::Point => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "点",
                    category: "曲線",
                    description: "X・Y・Z 座標から点を作ります。単位は mm。",
                    keywords: "point xyz 座標",
                    inputs: &[
                        n("x", "X", 0., -1e5, 1e5, 1.),
                        n("y", "Y", 0., -1e5, 1e5, 1.),
                        n("z", "Z", 0., -1e5, 1e5, 1.),
                    ],
                    output: DataType::Point,
                };
                &SPEC
            }
            Self::Circle => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "円",
                    category: "曲線",
                    description: "XY 平面に閉じた円の点列を作ります。分割数で滑らかさを調整。",
                    keywords: "circle ring 丸 輪",
                    inputs: &[
                        n("radius", "半径", 30., 0.1, 1000., 0.5),
                        i("segments", "分割数", 64., 8., 256.),
                    ],
                    output: DataType::Curve,
                };
                &SPEC
            }
            Self::Polygon => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "正多角形",
                    category: "曲線",
                    description: "外接円半径と辺数で正多角形を作ります。",
                    keywords: "polygon triangle hexagon 六角形 三角形",
                    inputs: &[
                        n("radius", "半径", 30., 0.1, 1000., 0.5),
                        i("sides", "辺の数", 6., 3., 64.),
                    ],
                    output: DataType::Curve,
                };
                &SPEC
            }
            Self::Star => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "星形",
                    category: "曲線",
                    description: "外半径と内半径を交互に結んだ閉じた断面を作ります。",
                    keywords: "star flower 星 花 断面",
                    inputs: &[
                        n("radius", "外半径", 30., 0.1, 1000., 0.5),
                        n("inner_radius", "内半径", 16., 0.1, 1000., 0.5),
                        i("points", "角の数", 7., 3., 48.),
                    ],
                    output: DataType::Curve,
                };
                &SPEC
            }
            Self::Rectangle => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "長方形",
                    category: "曲線",
                    description: "原点を中心に、XY 平面上の長方形を作ります。",
                    keywords: "rectangle square 四角 四角形",
                    inputs: &[
                        n("width", "幅", 40., 0.1, 2000., 1.),
                        n("depth", "奥行", 30., 0.1, 2000., 1.),
                    ],
                    output: DataType::Curve,
                };
                &SPEC
            }
            Self::Helix => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "らせん",
                    category: "曲線",
                    description: "Z 軸を中心に上昇するらせん状の点列を作ります。",
                    keywords: "helix spiral 螺旋 バネ スパイラル",
                    inputs: &[
                        n("radius", "半径", 25., 0.1, 1000., 0.5),
                        n("height", "高さ", 100., 0.1, 2000., 1.),
                        n("turns", "巻き数", 3., 0.1, 30., 0.1),
                        i("segments", "分割数", 192., 16., 1024.),
                    ],
                    output: DataType::Curve,
                };
                &SPEC
            }
            Self::Extrude => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "押し出し",
                    category: "立体",
                    description: "閉じた平面の断面を Z 方向に押し出します。凹形状も三角形に分割。",
                    keywords: "extrude extrusion 押出 厚み プリズム",
                    inputs: &[
                        g("curve", "断面", DataType::Curve),
                        n("height", "高さ", 40., 0.1, 2000., 1.),
                        i("layers", "縦の分割", 16., 1., 128.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Loft => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "ロフト",
                    category: "立体",
                    description: "同じ点数の曲線を順に結びます。1 ブランチに 2 本以上の曲線を接続。",
                    keywords: "loft surface 曲面 断面 接続",
                    inputs: &[
                        g("curves", "断面リスト", DataType::Curve),
                        i("cap", "端を閉じる", 1., 0., 1.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Revolve => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "回転面",
                    category: "立体",
                    description: "断面を Z 軸の周りに回転。断面は XZ 平面上で半径 X > 0 に配置。端の開いた断面は開いた面になります。",
                    keywords: "revolve lathe 回転体 回転 曲面",
                    inputs: &[
                        g("curve", "断面", DataType::Curve),
                        n("angle", "回転角度", 360., 1., 360., 1.),
                        i("steps", "分割数", 64., 8., 192.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Vase => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "花器",
                    category: "立体",
                    description: "底・内壁・外壁・上縁を備えた中空の花器を生成。肉厚は半径方向の厚さです。",
                    keywords: "vase pot vessel 花瓶 容器 花器 プリント",
                    inputs: &[
                        n("radius", "半径", 32., 3., 500., 0.5),
                        n("height", "高さ", 100., 5., 1000., 1.),
                        n("thickness", "肉厚", 2.4, 0.5, 40., 0.1),
                        i("lobes", "波の数", 7., 0., 24.),
                        n("amplitude", "凹凸", 0.12, 0., 0.4, 0.01),
                        i("segments", "周の分割", 64., 16., 192.),
                        i("layers", "縦の分割", 32., 4., 128.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Sphere => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "球",
                    category: "立体",
                    description: "緯度と経度で分割した閉じた球のメッシュを作ります。",
                    keywords: "sphere ball 球体 丸",
                    inputs: &[
                        n("radius", "半径", 25., 0.1, 1000., 0.5),
                        i("segments", "横の分割", 48., 8., 192.),
                        i("rings", "縦の分割", 24., 4., 96.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Box => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "箱",
                    category: "立体",
                    description: "XY の中心を原点とし、底面 Z=0 の直方体を作ります。",
                    keywords: "box cube cuboid 立方体 直方体 箱",
                    inputs: &[
                        n("width", "幅", 40., 0.1, 2000., 1.),
                        n("depth", "奥行", 40., 0.1, 2000., 1.),
                        n("height", "高さ", 40., 0.1, 2000., 1.),
                    ],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Move => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "移動",
                    category: "変形",
                    description: "点・曲線・メッシュを平行移動します。数列を接続すると複数配置。",
                    keywords: "move translate offset 動かす ずらす 平行移動",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("x", "X", 0., -1e5, 1e5, 1.),
                        n("y", "Y", 0., -1e5, 1e5, 1.),
                        n("z", "Z", 0., -1e5, 1e5, 1.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Rotate => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "回転",
                    category: "変形",
                    description: "原点を通る指定軸の周りに回転します。軸: 0=X、1=Y、2=Z。",
                    keywords: "rotate rotation 回す 角度",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("angle", "角度", 45., -3600., 3600., 1.),
                        i("axis", "軸 (0:X 1:Y 2:Z)", 2., 0., 2.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Scale => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "拡大縮小",
                    category: "変形",
                    description: "原点を基準に形状全体の大きさを変更します。",
                    keywords: "scale resize size サイズ 倍率",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("factor", "倍率", 1.2, 0.01, 100., 0.05),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Twist => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "ねじり",
                    category: "変形",
                    description: "形状の最下部から最上部まで Z 軸周りの回転を徐々に増やします。縦の分割が多いほど滑らかです。",
                    keywords: "twist torsion ねじる 捻る 螺旋",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("angle", "ねじり角", 90., -1080., 1080., 5.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Wave => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "波打つ",
                    category: "変形",
                    description: "X 方向に沿う正弦波で Z 座標を変形します。細分割した形状に使用。",
                    keywords: "wave sine 波 ウェーブ 波形",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        n("amplitude", "振幅", 4., 0., 200., 0.5),
                        n("wavelength", "波長", 40., 0.1, 2000., 1.),
                        n("phase", "位相", 0., -3600., 3600., 5.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::LinearArray => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "直線配列",
                    category: "配列",
                    description: "指定した間隔で形状を複製。個数には元の形状を含みます。",
                    keywords: "linear array repeat duplicate 並べる 複製 反復",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        i("count", "個数", 5., 1., 128.),
                        n("x", "X 間隔", 50., -1e4, 1e4, 1.),
                        n("y", "Y 間隔", 0., -1e4, 1e4, 1.),
                        n("z", "Z 間隔", 0., -1e4, 1e4, 1.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::RadialArray => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "円形配列",
                    category: "配列",
                    description: "原点の Z 軸を中心に複製。先に移動すると円状に並びます。",
                    keywords: "radial polar circular array repeat 円周 回転配列 放射",
                    inputs: &[
                        g("geometry", "形状", DataType::Geometry),
                        i("count", "個数", 8., 1., 128.),
                        n("angle", "全体の角度", 360., 1., 360., 1.),
                    ],
                    output: DataType::Geometry,
                };
                &SPEC
            }
            Self::Merge => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "メッシュをまとめる",
                    category: "データ",
                    description: "同じブランチのメッシュを 1 つにまとめます。交差部分のブーリアン結合は行いません。",
                    keywords: "merge join combine 集合 結合 まとめる",
                    inputs: &[g("geometry", "メッシュ", DataType::Mesh)],
                    output: DataType::Mesh,
                };
                &SPEC
            }
            Self::Flatten => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "フラット化",
                    category: "データ",
                    description: "全ブランチのデータを順に 1 つのリストへまとめます。",
                    keywords: "flatten list tree フラット 階層 リスト",
                    inputs: &[g("geometry", "データ", DataType::Any)],
                    output: DataType::Any,
                };
                &SPEC
            }
            Self::Graft => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "枝を分ける",
                    category: "データ",
                    description: "各要素を個別のブランチへ分けます。空ブランチも保持します。",
                    keywords: "graft tree branch ツリー 枝 階層",
                    inputs: &[g("geometry", "データ", DataType::Any)],
                    output: DataType::Any,
                };
                &SPEC
            }
            Self::Output => {
                static SPEC: NodeSpec = NodeSpec {
                    label: "出力",
                    category: "出力",
                    description: "完成した形状を表示・書き出しの対象にします。",
                    keywords: "output preview export 出力 完成 表示 STL",
                    inputs: &[g("geometry", "形状", DataType::Geometry)],
                    output: DataType::Geometry,
                };
                &SPEC
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    pub position: [f32; 2],
    pub params: BTreeMap<String, f64>,
    pub inputs: BTreeMap<String, NodeId>,
    pub preview: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub next_id: NodeId,
}
impl Graph {
    pub fn add(&mut self, kind: NodeKind, position: [f32; 2]) -> Result<NodeId, String> {
        if self.nodes.len() >= MAX_NODES {
            return Err(format!("ノードは {MAX_NODES} 個までです。"));
        }
        if !valid_position(position) {
            return Err("ノードの位置が無効です。".into());
        }
        let id = self.next_id;
        if self.node(id).is_some() {
            return Err("ノード ID が重複しています。".into());
        }
        let next = id
            .checked_add(1)
            .ok_or("ノード ID をこれ以上追加できません。")?;
        self.nodes.push(Node {
            id,
            kind,
            name: kind.spec().label.into(),
            position,
            params: kind
                .spec()
                .inputs
                .iter()
                .filter_map(|s| s.default.map(|v| (s.key.into(), v)))
                .collect(),
            inputs: BTreeMap::new(),
            preview: true,
        });
        self.next_id = next;
        Ok(id)
    }
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
    pub fn remove(&mut self, id: NodeId) {
        self.nodes.retain(|n| n.id != id);
        for n in &mut self.nodes {
            n.inputs.retain(|_, source| *source != id);
        }
    }
    pub fn connect(&mut self, from: NodeId, to: NodeId, input: &str) -> Result<(), String> {
        let source = self
            .node(from)
            .ok_or("接続元が見つかりません。")?
            .kind
            .spec()
            .output;
        let node = self.node(to).ok_or("接続先が見つかりません。")?;
        let target = node
            .kind
            .spec()
            .inputs
            .iter()
            .find(|s| s.key == input)
            .ok_or("入力が見つかりません。")?;
        if !target.kind.accepts(source) {
            return Err(format!(
                "{} には {} を接続できません。",
                target.kind.label(),
                source.label()
            ));
        }
        let old = self
            .node_mut(to)
            .ok_or("接続先が見つかりません。")?
            .inputs
            .insert(input.into(), from);
        if let Err(e) = self.topological_order() {
            if let Some(n) = self.node_mut(to) {
                match old {
                    Some(v) => {
                        n.inputs.insert(input.into(), v);
                    }
                    None => {
                        n.inputs.remove(input);
                    }
                }
            }
            return Err(e);
        }
        Ok(())
    }
    pub fn disconnect(&mut self, to: NodeId, input: &str) {
        if let Some(n) = self.node_mut(to) {
            n.inputs.remove(input);
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.topological_order().map(|_| ())
    }
    pub fn topological_order(&self) -> Result<Vec<NodeId>, String> {
        if self.nodes.len() > MAX_NODES {
            return Err(format!("ノード数が上限 {MAX_NODES} を超えています。"));
        }
        let mut indegree = BTreeMap::new();
        let mut after: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        let ids: BTreeMap<_, _> = self.nodes.iter().map(|n| (n.id, n)).collect();
        if ids.len() != self.nodes.len() {
            return Err("ノード ID が重複しています。".into());
        }
        for node in &self.nodes {
            if node.id >= self.next_id {
                return Err("次のノード ID が無効です。".into());
            }
            if !valid_position(node.position) || node.name.len() > 512 {
                return Err("ノードの位置または名前が無効です。".into());
            }
            let specs = node.kind.spec().inputs;
            for (key, value) in &node.params {
                if !value.is_finite() || !specs.iter().any(|s| s.key == key && s.default.is_some())
                {
                    return Err(format!("「{}」のパラメータが無効です。", node.name));
                }
            }
            let mut parents = BTreeSet::new();
            for (key, source) in &node.inputs {
                let spec = specs
                    .iter()
                    .find(|s| s.key == key)
                    .ok_or("不明な入力への接続があります。")?;
                let other = ids
                    .get(source)
                    .ok_or("存在しないノードへの接続があります。")?;
                if !spec.kind.accepts(other.kind.spec().output) {
                    return Err(format!(
                        "「{}」の「{}」は接続の型が違います。",
                        node.name, spec.label
                    ));
                }
                parents.insert(*source);
            }
            indegree.insert(node.id, parents.len());
            for p in parents {
                after.entry(p).or_default().push(node.id);
            }
        }
        let mut queue: VecDeque<_> = indegree
            .iter()
            .filter(|(_, v)| **v == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut order = Vec::new();
        while let Some(id) = queue.pop_front() {
            order.push(id);
            for child in after.get(&id).into_iter().flatten() {
                if let Some(d) = indegree.get_mut(child) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        queue.push_back(*child);
                    }
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err("この接続は循環します。途中のノードへ戻る線はつなげません。".into());
        }
        Ok(order)
    }
}
fn valid_position(p: [f32; 2]) -> bool {
    p.into_iter()
        .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Curve {
    pub points: Vec<[f64; 3]>,
    pub closed: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Point([f64; 3]),
    Curve(Arc<Curve>),
    Mesh(Arc<Mesh>),
}
impl Value {
    pub fn kind(&self) -> DataType {
        match self {
            Self::Number(_) => DataType::Number,
            Self::Point(_) => DataType::Point,
            Self::Curve(_) => DataType::Curve,
            Self::Mesh(_) => DataType::Mesh,
        }
    }
    pub fn weight(&self) -> usize {
        match self {
            Self::Number(_) | Self::Point(_) => 1,
            Self::Curve(c) => c.points.len().max(1),
            Self::Mesh(m) => m
                .vertices
                .len()
                .saturating_add(m.triangles.len().saturating_mul(3)),
        }
    }
    pub fn number(&self) -> Result<f64, String> {
        match self {
            Self::Number(v) if v.is_finite() => Ok(*v),
            _ => Err("数値を接続してください。".into()),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tree {
    pub branches: BTreeMap<Vec<u32>, Vec<Value>>,
}
impl Tree {
    pub fn single(value: Value) -> Self {
        Self {
            branches: BTreeMap::from([(vec![0], vec![value])]),
        }
    }
    pub fn item_count(&self) -> usize {
        self.branches.values().map(Vec::len).sum()
    }
    pub fn weight(&self) -> usize {
        self.branches.iter().fold(0usize, |sum, (path, items)| {
            items
                .iter()
                .fold(sum.saturating_add(path.len()).saturating_add(1), |a, v| {
                    a.saturating_add(v.weight())
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_wire_keeps_previous_connection() {
        let mut g = Graph::default();
        let a = g.add(NodeKind::Number, [0., 0.]).unwrap();
        let b = g.add(NodeKind::Add, [1., 0.]).unwrap();
        let c = g.add(NodeKind::Add, [2., 0.]).unwrap();
        g.connect(a, b, "a").unwrap();
        g.connect(b, c, "a").unwrap();
        let before = g.clone();
        assert!(g.connect(c, b, "a").is_err());
        assert_eq!(g, before);
    }
    #[test]
    fn impossible_types_are_rejected() {
        let mut g = Graph::default();
        let a = g.add(NodeKind::Number, [0., 0.]).unwrap();
        let b = g.add(NodeKind::Extrude, [1., 0.]).unwrap();
        assert!(g.connect(a, b, "curve").is_err());
    }
    #[test]
    fn duplicate_ids_and_missing_sources_are_rejected() {
        let mut g = Graph::default();
        let a = g.add(NodeKind::Number, [0., 0.]).unwrap();
        g.nodes.push(g.nodes[0].clone());
        assert!(g.validate().is_err());
        g.nodes.pop();
        g.node_mut(a).unwrap().inputs.insert("value".into(), 999);
        assert!(g.validate().is_err());
    }
    #[test]
    fn metadata_and_defaults_cover_every_node() {
        let mut g = Graph::default();
        for kind in NodeKind::all() {
            let id = g.add(*kind, [0., 0.]).unwrap();
            let node = g.node(id).unwrap();
            for spec in kind.spec().inputs {
                if let Some(default) = spec.default {
                    assert_eq!(node.params.get(spec.key), Some(&default));
                    assert!(default >= spec.min && default <= spec.max);
                }
            }
        }
        assert!(g.validate().is_ok());
    }
}
