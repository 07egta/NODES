//! Independent mathematical nodes. SPDX-License-Identifier: MIT OR Apache-2.0
//! Existing single-output graph transports vectors as Point and domains as two Number values.
use crate::engine::{cancelled, num};
use crate::model::{g, i, n, DataType, NodeKind, NodeSpec, Value, MAX_ITEMS};
use std::collections::BTreeSet;
use std::f64::consts::{PI, TAU};

pub fn spec(kind: NodeKind) -> Option<&'static NodeSpec> {
    use NodeKind::*;
    match kind {
        Negate => {
            static SPEC: NodeSpec = NodeSpec {
                label: "符号反転", category: "数値",
                description: "値の符号を反転。", keywords: "negative negate",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Round => {
            static SPEC: NodeSpec = NodeSpec {
                label: "四捨五入", category: "数値",
                description: "最も近い整数。ちょうど半分はゼロから遠い側へ。", keywords: "round",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Floor => {
            static SPEC: NodeSpec = NodeSpec {
                label: "切り下げ", category: "数値",
                description: "負の無限大方向の整数。", keywords: "floor",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Ceiling => {
            static SPEC: NodeSpec = NodeSpec {
                label: "切り上げ", category: "数値",
                description: "正の無限大方向の整数。", keywords: "ceil ceiling",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Exponential => {
            static SPEC: NodeSpec = NodeSpec {
                label: "指数関数", category: "数値",
                description: "自然指数 e^x。", keywords: "exp exponential",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Sinh => {
            static SPEC: NodeSpec = NodeSpec {
                label: "双曲線正弦", category: "数値",
                description: "双曲線正弦。", keywords: "sinh",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Cosh => {
            static SPEC: NodeSpec = NodeSpec {
                label: "双曲線余弦", category: "数値",
                description: "双曲線余弦。", keywords: "cosh",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Tanh => {
            static SPEC: NodeSpec = NodeSpec {
                label: "双曲線正接", category: "数値",
                description: "双曲線正接。", keywords: "tanh",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Degrees => {
            static SPEC: NodeSpec = NodeSpec {
                label: "度へ変換", category: "数値",
                description: "ラジアンを度に変換。", keywords: "degrees rad",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Radians => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ラジアンへ変換", category: "数値",
                description: "度をラジアンに変換。", keywords: "radians deg",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Asin => {
            static SPEC: NodeSpec = NodeSpec {
                label: "逆正弦", category: "数値",
                description: "[-1,1] からラジアンへ。", keywords: "asin arcsin",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Acos => {
            static SPEC: NodeSpec = NodeSpec {
                label: "逆余弦", category: "数値",
                description: "[-1,1] からラジアンへ。", keywords: "acos arccos",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Atan => {
            static SPEC: NodeSpec = NodeSpec {
                label: "逆正接", category: "数値",
                description: "ラジアンで返します。", keywords: "atan arctan",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        SinRadians => {
            static SPEC: NodeSpec = NodeSpec {
                label: "正弦 (rad)", category: "数値",
                description: "ラジアンの正弦。旧 Sine 波形ノードはそのまま。", keywords: "sin sine radians",
                inputs: &[n("angle", "角度 (rad)", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        CosRadians => {
            static SPEC: NodeSpec = NodeSpec {
                label: "余弦 (rad)", category: "数値",
                description: "ラジアンの余弦。", keywords: "cos cosine radians",
                inputs: &[n("angle", "角度 (rad)", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        TanRadians => {
            static SPEC: NodeSpec = NodeSpec {
                label: "正接 (rad)", category: "数値",
                description: "ラジアンの正接。極ではエラー。", keywords: "tan tangent radians",
                inputs: &[n("angle", "角度 (rad)", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Atan2 => {
            static SPEC: NodeSpec = NodeSpec {
                label: "二引数逆正接", category: "数値",
                description: "Y, X から符号付きラジアンを返す。原点では未定義。", keywords: "atan2 angle",
                inputs: &[n("y", "Y", 0.0, -1e12, 1e12, 0.01), n("x", "X", 1.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Root => {
            static SPEC: NodeSpec = NodeSpec {
                label: "n乗根", category: "数値",
                description: "正の整数 n の根。負数は奇数根のみ。", keywords: "root sqrt nth",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01), i("degree", "次数", 2.0, 1.0, 170.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Logarithm => {
            static SPEC: NodeSpec = NodeSpec {
                label: "対数", category: "数値",
                description: "底 > 0 かつ底 != 1。真数 > 0。", keywords: "log ln logarithm",
                inputs: &[n("x", "真数", 1.0, -1e12, 1e12, 0.01), n("base", "底", 2.718281828459045, 0.000000001, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Factorial => {
            static SPEC: NodeSpec = NodeSpec {
                label: "階乗", category: "数値",
                description: "0! = 1。0〜170 の整数のみ。", keywords: "factorial",
                inputs: &[i("x", "整数", 5.0, 0.0, 170.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Clamp => {
            static SPEC: NodeSpec = NodeSpec {
                label: "範囲に収める", category: "数値",
                description: "最小と最大の間に値を収める。逆順はエラー。", keywords: "clamp limit",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01), n("min", "最小", 0.0, -1e12, 1e12, 0.01), n("max", "最大", 1.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Summation => {
            static SPEC: NodeSpec = NodeSpec {
                label: "総和", category: "数値",
                description: "枝ごとに総和。空枝は 0。", keywords: "summation mass addition sum",
                inputs: &[g("values", "数値リスト", DataType::Number)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Product => {
            static SPEC: NodeSpec = NodeSpec {
                label: "総積", category: "数値",
                description: "枝ごとに総積。空枝は 1。", keywords: "product mass multiplication",
                inputs: &[g("values", "数値リスト", DataType::Number)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Average => {
            static SPEC: NodeSpec = NodeSpec {
                label: "平均", category: "数値",
                description: "枝ごとに算術平均。空枝は空出力。", keywords: "average mean",
                inputs: &[g("values", "数値リスト", DataType::Number)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Median => {
            static SPEC: NodeSpec = NodeSpec {
                label: "中央値", category: "数値",
                description: "枝ごとに中央値。", keywords: "median statistics",
                inputs: &[g("values", "数値リスト", DataType::Number)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Variance => {
            static SPEC: NodeSpec = NodeSpec {
                label: "分散 (母集団)", category: "数値",
                description: "枝ごとに母分散 (N で割る)。", keywords: "variance statistics",
                inputs: &[g("values", "数値リスト", DataType::Number)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        StdDeviation => {
            static SPEC: NodeSpec = NodeSpec {
                label: "標準偏差 (母集団)", category: "数値",
                description: "枝ごとに母標準偏差。", keywords: "standard deviation statistics",
                inputs: &[g("values", "数値リスト", DataType::Number)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        GhRange => {
            static SPEC: NodeSpec = NodeSpec {
                label: "範囲 (分割数)", category: "数値",
                description: "分割数 N に対して N+1 個、両端を含む。旧範囲は変更しない。", keywords: "grasshopper range steps",
                inputs: &[n("start", "始点", 0.0, -1e12, 1e12, 0.01), n("end", "終点", 1.0, -1e12, 1e12, 0.01), i("steps", "分割数", 10.0, 1.0, 512.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        ConstructDomain => {
            static SPEC: NodeSpec = NodeSpec {
                label: "区間を作る", category: "数値",
                description: "区間を始点・終点の 2 数値として返す。専用区間型ではない。", keywords: "construct domain interval",
                inputs: &[n("start", "始点", 0.0, -1e12, 1e12, 0.01), n("end", "終点", 1.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        DomainEndpoint => {
            static SPEC: NodeSpec = NodeSpec {
                label: "区間の端点", category: "数値",
                description: "枝に 2 個の数値が必要。端点を一つ取り出す。", keywords: "deconstruct domain endpoint",
                inputs: &[g("domain", "区間 [始点, 終点]", DataType::Number), i("end", "0:始点 1:終点", 0.0, 0.0, 1.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        DomainLength => {
            static SPEC: NodeSpec = NodeSpec {
                label: "区間長", category: "数値",
                description: "枝ごとの符号付き長さ (終点 − 始点)。", keywords: "domain length",
                inputs: &[g("domain", "区間 [始点, 終点]", DataType::Number)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        EvaluateDomain => {
            static SPEC: NodeSpec = NodeSpec {
                label: "区間を評価", category: "数値",
                description: "2 数値の区間を各 t で線形評価。範囲外は外挿。", keywords: "evaluate domain lerp",
                inputs: &[g("domain", "区間 [始点, 終点]", DataType::Number), n("t", "t", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        DivideDomain => {
            static SPEC: NodeSpec = NodeSpec {
                label: "区間分割", category: "数値",
                description: "N 個の小区間の端点を順に [a,b,b,c,...] で出力。", keywords: "divide domain",
                inputs: &[g("domain", "区間 [始点, 終点]", DataType::Number), i("steps", "分割数", 10.0, 1.0, 512.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        DomainIntersection => {
            static SPEC: NodeSpec = NodeSpec {
                label: "区間の共通部分", category: "数値",
                description: "端点を昇順化し共通部分を返す。交差なしは空。", keywords: "domain intersection",
                inputs: &[g("domain", "区間 [始点, 終点]", DataType::Number), g("other", "区間 B [始点, 終点]", DataType::Number)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        DomainUnion => {
            static SPEC: NodeSpec = NodeSpec {
                label: "区間を包む", category: "数値",
                description: "2 区間の全端点を包む最小区間。離れた区間の隙間も含む。", keywords: "domain union hull",
                inputs: &[g("domain", "区間 [始点, 終点]", DataType::Number), g("other", "区間 B [始点, 終点]", DataType::Number)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        GraphMapper => {
            static SPEC: NodeSpec = NodeSpec {
                label: "グラフマッパー", category: "数値",
                description: "0〜1 入力を 6 種の曲線に変換。ベジェの 2 制御点を設定。", keywords: "graph mapper bezier linear sine cosine gaussian exponential ease",
                inputs: &[n("x", "入力 x", 0.5, 0., 1., 0.001), i("mode", "種類 (0〜5)", 1.0, 0.0, 5.0), n("x1", "制御点1 X", 0.25, 0., 1., 0.01), n("y1", "制御点1 Y", 0.1, 0., 1., 0.01), n("x2", "制御点2 X", 0.75, 0., 1., 0.01), n("y2", "制御点2 Y", 0.9, 0., 1., 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        BoolAnd => {
            static SPEC: NodeSpec = NodeSpec {
                label: "論理 AND", category: "数値",
                description: "両方が 1 なら 1。", keywords: "and logic",
                inputs: &[i("a", "A (0/1)", 0.0, 0.0, 1.0), i("b", "B (0/1)", 1.0, 0.0, 1.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        BoolOr => {
            static SPEC: NodeSpec = NodeSpec {
                label: "論理 OR", category: "数値",
                description: "一方以上が 1 なら 1。", keywords: "or logic",
                inputs: &[i("a", "A (0/1)", 0.0, 0.0, 1.0), i("b", "B (0/1)", 1.0, 0.0, 1.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        BoolXor => {
            static SPEC: NodeSpec = NodeSpec {
                label: "論理 XOR", category: "数値",
                description: "片方だけが 1 なら 1。", keywords: "xor logic",
                inputs: &[i("a", "A (0/1)", 0.0, 0.0, 1.0), i("b", "B (0/1)", 1.0, 0.0, 1.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        BoolNot => {
            static SPEC: NodeSpec = NodeSpec {
                label: "論理 NOT", category: "数値",
                description: "0 と 1 を反転。", keywords: "not logic",
                inputs: &[i("x", "値 (0/1)", 0.0, 0.0, 1.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Equals => {
            static SPEC: NodeSpec = NodeSpec {
                label: "等しい", category: "数値",
                description: "数値または点の厳密な同型比較。異なる型は偽。", keywords: "equality equal",
                inputs: &[g("a", "A", DataType::Any), g("b", "B", DataType::Any)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        LargerThan => {
            static SPEC: NodeSpec = NodeSpec {
                label: "より大きい", category: "数値",
                description: "A > B。", keywords: "larger greater than",
                inputs: &[n("a", "A", 0.0, -1e12, 1e12, 0.01), n("b", "B", 1.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        SmallerThan => {
            static SPEC: NodeSpec = NodeSpec {
                label: "より小さい", category: "数値",
                description: "A < B。", keywords: "smaller less than",
                inputs: &[n("a", "A", 0.0, -1e12, 1e12, 0.01), n("b", "B", 1.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        SetContains => {
            static SPEC: NodeSpec = NodeSpec {
                label: "集合に含む", category: "データ",
                description: "枝 A に B の各値が存在するか、0/1 で返す。数値と点のみ。", keywords: "inclusion contains member",
                inputs: &[g("list", "リスト", DataType::Any), g("other", "リスト B", DataType::Any)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        SetUnion => {
            static SPEC: NodeSpec = NodeSpec {
                label: "集合の和", category: "データ",
                description: "重複を除いて A と B を結合。 数値と点のみ、初出順。", keywords: "set union",
                inputs: &[g("list", "リスト", DataType::Any), g("other", "リスト B", DataType::Any)], output: DataType::Any,
            };
            Some(&SPEC)
        },
        SetIntersection => {
            static SPEC: NodeSpec = NodeSpec {
                label: "集合の積", category: "データ",
                description: "A と B に共通する値。 数値と点のみ、初出順。", keywords: "set intersection",
                inputs: &[g("list", "リスト", DataType::Any), g("other", "リスト B", DataType::Any)], output: DataType::Any,
            };
            Some(&SPEC)
        },
        SetDifference => {
            static SPEC: NodeSpec = NodeSpec {
                label: "集合の差", category: "データ",
                description: "A にあり B にない値。 数値と点のみ、初出順。", keywords: "set difference",
                inputs: &[g("list", "リスト", DataType::Any), g("other", "リスト B", DataType::Any)], output: DataType::Any,
            };
            Some(&SPEC)
        },
        CreateSet => {
            static SPEC: NodeSpec = NodeSpec {
                label: "集合を作る", category: "データ",
                description: "数値と点の重複を除いて初出順に返す。", keywords: "create set unique",
                inputs: &[g("list", "リスト", DataType::Any)], output: DataType::Any,
            };
            Some(&SPEC)
        },
        DuplicateDetection => {
            static SPEC: NodeSpec = NodeSpec {
                label: "重複を調べる", category: "データ",
                description: "数値と点の 2 回目以降の出現に 1 を返す。", keywords: "duplicate detection",
                inputs: &[g("list", "リスト", DataType::Any)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        ConstructVector => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル XYZ", category: "数値",
                description: "現行型の制約により XYZ を点型で輸送。ベクトルとして計算。", keywords: "construct vector xyz",
                inputs: &[n("x", "X", 1.0, -1e12, 1e12, 0.01), n("y", "Y", 0.0, -1e12, 1e12, 0.01), n("z", "Z", 0.0, -1e12, 1e12, 0.01)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        VectorComponent => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル成分", category: "数値",
                description: "X/Y/Z の一つを取り出す。", keywords: "deconstruct vector component",
                inputs: &[g("vector", "ベクトル (XYZ)", DataType::Point), i("axis", "0:X 1:Y 2:Z", 0.0, 0.0, 2.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        UnitX => {
            static SPEC: NodeSpec = NodeSpec {
                label: "X 単位ベクトル", category: "数値",
                description: "指定した軸の単位ベクトルに倍率を掛ける。", keywords: "unit x",
                inputs: &[n("amplitude", "倍率", 1.0, -1e12, 1e12, 0.01)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        UnitY => {
            static SPEC: NodeSpec = NodeSpec {
                label: "Y 単位ベクトル", category: "数値",
                description: "指定した軸の単位ベクトルに倍率を掛ける。", keywords: "unit y",
                inputs: &[n("amplitude", "倍率", 1.0, -1e12, 1e12, 0.01)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        UnitZ => {
            static SPEC: NodeSpec = NodeSpec {
                label: "Z 単位ベクトル", category: "数値",
                description: "指定した軸の単位ベクトルに倍率を掛ける。", keywords: "unit z",
                inputs: &[n("amplitude", "倍率", 1.0, -1e12, 1e12, 0.01)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        VectorLength => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル長", category: "数値",
                description: "XYZ の長さ。", keywords: "vector length magnitude",
                inputs: &[g("vector", "ベクトル (XYZ)", DataType::Point)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        Unitize => {
            static SPEC: NodeSpec = NodeSpec {
                label: "単位化", category: "数値",
                description: "長さ 1 に正規化。ゼロはエラー。", keywords: "unitize normalize vector",
                inputs: &[g("vector", "ベクトル (XYZ)", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        ReverseVector => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル反転", category: "数値",
                description: "XYZ の各符号を反転。", keywords: "reverse vector",
                inputs: &[g("vector", "ベクトル (XYZ)", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        DotProduct => {
            static SPEC: NodeSpec = NodeSpec {
                label: "内積", category: "数値",
                description: "A · B。", keywords: "dot product",
                inputs: &[g("a", "A (XYZ)", DataType::Point), g("b", "B (XYZ)", DataType::Point)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        CrossProduct => {
            static SPEC: NodeSpec = NodeSpec {
                label: "外積", category: "数値",
                description: "右手系の A × B。", keywords: "cross product",
                inputs: &[g("a", "A (XYZ)", DataType::Point), g("b", "B (XYZ)", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        VectorAngle => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル角度", category: "数値",
                description: "2 ベクトルのなす角をラジアンで返す。ゼロはエラー。", keywords: "vector angle",
                inputs: &[g("a", "A (XYZ)", DataType::Point), g("b", "B (XYZ)", DataType::Point)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        VectorAddition => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル加算", category: "数値",
                description: "A + B。", keywords: "vector addition",
                inputs: &[g("a", "A (XYZ)", DataType::Point), g("b", "B (XYZ)", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        VectorSubtraction => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル減算", category: "数値",
                description: "A − B。", keywords: "vector subtraction",
                inputs: &[g("a", "A (XYZ)", DataType::Point), g("b", "B (XYZ)", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        VectorProjection => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル投影", category: "数値",
                description: "A を B の方向へ直交投影。B がゼロならエラー。", keywords: "vector projection",
                inputs: &[g("a", "A (XYZ)", DataType::Point), g("b", "B (XYZ)", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        VectorBetweenPoints => {
            static SPEC: NodeSpec = NodeSpec {
                label: "2点間ベクトル", category: "数値",
                description: "点 A から点 B への差分 B − A。", keywords: "vector between points",
                inputs: &[g("a", "点 A", DataType::Point), g("b", "点 B", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        RotateVector => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル回転", category: "数値",
                description: "右手系。軸の周りにラジアンで回転。", keywords: "rotate vector axis",
                inputs: &[g("vector", "ベクトル (XYZ)", DataType::Point), g("axis", "回転軸 (XYZ)", DataType::Point), n("angle", "角度 (rad)", 0.0, -1e12, 1e12, 0.01)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        ClosestPoint => {
            static SPEC: NodeSpec = NodeSpec {
                label: "最近点", category: "データ",
                description: "枝内の候補点から各検索点に最も近い点を返す。同距離は初出。", keywords: "closest point nearest",
                inputs: &[g("candidates", "候補点リスト", DataType::Point), g("queries", "検索点リスト", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        PlaneProject => {
            static SPEC: NodeSpec = NodeSpec {
                label: "平面へ投影", category: "数値",
                description: "原点・X方向・Y方向で定義する平面に点を直交投影。", keywords: "point projection plane",
                inputs: &[g("origin", "原点", DataType::Point), g("axis_x", "X 方向 (XYZ)", DataType::Point), g("axis_y", "Y 方向 (XYZ)", DataType::Point), g("point", "点", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        PlaneCoordinates => {
            static SPEC: NodeSpec = NodeSpec {
                label: "平面座標", category: "数値",
                description: "原点と直交化した X/Y 方向から点の局所座標 (u,v,w) を返す。", keywords: "plane coordinates change basis",
                inputs: &[g("origin", "原点", DataType::Point), g("axis_x", "X 方向 (XYZ)", DataType::Point), g("axis_y", "Y 方向 (XYZ)", DataType::Point), g("point", "点", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        PlanePoint => {
            static SPEC: NodeSpec = NodeSpec {
                label: "平面上の点", category: "数値",
                description: "局所座標を世界座標へ変換。", keywords: "plane oriented point change basis",
                inputs: &[g("origin", "原点", DataType::Point), g("axis_x", "X 方向 (XYZ)", DataType::Point), g("axis_y", "Y 方向 (XYZ)", DataType::Point), g("point", "点", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        PointInPlane => {
            static SPEC: NodeSpec = NodeSpec {
                label: "点が平面上か", category: "数値",
                description: "平面への符号付き距離の絶対値が許容差以下なら 1。", keywords: "point in plane",
                inputs: &[g("origin", "原点", DataType::Point), g("axis_x", "X 方向 (XYZ)", DataType::Point), g("axis_y", "Y 方向 (XYZ)", DataType::Point), g("point", "点", DataType::Point), n("tolerance", "許容差", 0.000001, 0., 1e6, 0.000001)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        PlaneNormal3Pt => {
            static SPEC: NodeSpec = NodeSpec {
                label: "3点から平面法線", category: "数値",
                description: "3点を通る平面の右手系単位法線。共線点はエラー。", keywords: "plane three points normal",
                inputs: &[g("a", "原点 A", DataType::Point), g("b", "点 B", DataType::Point), g("c", "点 C", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        RandomNormal => {
            static SPEC: NodeSpec = NodeSpec {
                label: "正規乱数", category: "数値",
                description: "Box-Muller 法。指定シードで再現。同一実装内の再現性。", keywords: "normal distribution random gaussian seed",
                inputs: &[i("count", "個数", 10.0, 0.0, 512.0), n("mean", "平均", 0.0, -1e12, 1e12, 0.01), n("sigma", "標準偏差", 1., 0., 1e12, 0.01), i("seed", "シード", 1.0, 0.0, 100000.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        VectorMultiply => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル倍率", category: "数値",
                description: "ベクトルに倍率を掛ける。", keywords: "vector multiply scale",
                inputs: &[g("vector", "ベクトル (XYZ)", DataType::Point), n("scale", "倍率", 1.0, -1e12, 1e12, 0.01)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        VectorDivide => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル除算", category: "数値",
                description: "ベクトルを非ゼロの倍率で割る。", keywords: "vector divide",
                inputs: &[g("vector", "ベクトル (XYZ)", DataType::Point), n("scale", "除数", 1.0, -1e12, 1e12, 0.01)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        VectorAmplitude => {
            static SPEC: NodeSpec = NodeSpec {
                label: "ベクトル長を設定", category: "数値",
                description: "方向を保ち、指定した非負の長さにする。", keywords: "vector amplitude",
                inputs: &[g("vector", "ベクトル (XYZ)", DataType::Point), n("length","長さ",1.0,0.0,1e9,0.1)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        Lerp => {
            static SPEC: NodeSpec = NodeSpec {
                label: "線形補間", category: "数値",
                description: "A から B へ t で補間。範囲外は外挿。", keywords: "lerp interpolate",
                inputs: &[n("a", "A", 0.0, -1e12, 1e12, 0.01), n("b", "B", 1.0, -1e12, 1e12, 0.01), n("t", "t", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        SmoothStep => {
            static SPEC: NodeSpec = NodeSpec {
                label: "滑らかな補間", category: "数値",
                description: "0〜1 で 3t² − 2t³。外側は端点に固定。", keywords: "smoothstep ease",
                inputs: &[n("t", "t", 0.0, -1e12, 1e12, 0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        BezierValue => {
            static SPEC: NodeSpec = NodeSpec {
                label: "3次ベジェ補間", category: "数値",
                description: "4つの数値制御点を de Casteljau 法で評価。", keywords: "bezier cubic interpolate",
                inputs: &[n("a", "P0", 0.0, -1e12, 1e12, 0.01), n("b", "P1", 1.0, -1e12, 1e12, 0.01), n("c", "P2", 1.0, -1e12, 1e12, 0.01), n("d", "P3", 0.0, -1e12, 1e12, 0.01), n("t","t",0.5,0.0,1.0,0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        NormalPdf => {
            static SPEC: NodeSpec = NodeSpec {
                label: "正規分布密度", category: "数値",
                description: "平均と正の標準偏差から確率密度を評価。", keywords: "normal distribution pdf gaussian",
                inputs: &[n("x", "値", 0.0, -1e12, 1e12, 0.01), n("mean", "平均", 0.0, -1e12, 1e12, 0.01), n("sigma","標準偏差",1.0,1e-12,1e12,0.01)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        MatrixTranspose => {
            static SPEC: NodeSpec = NodeSpec {
                label: "行列転置", category: "数値",
                description: "枝に行優先の数値列、行数・列数を指定。転置した平坦な列。", keywords: "matrix transpose",
                inputs: &[g("matrix", "行優先の数値列", DataType::Number), i("rows", "行数", 2.0, 1.0, 64.0), i("cols", "列数", 2.0, 1.0, 64.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        MatrixMultiply => {
            static SPEC: NodeSpec = NodeSpec {
                label: "行列積", category: "数値",
                description: "A(m×k) と B(k×n) の積。行優先の数値列で入出力。", keywords: "matrix multiply product",
                inputs: &[g("a", "行列 A", DataType::Number), g("b", "行列 B", DataType::Number), i("rows", "m", 2.0, 1.0, 64.0), i("inner", "k", 2.0, 1.0, 64.0), i("cols", "n", 2.0, 1.0, 64.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        MatrixInverse => {
            static SPEC: NodeSpec = NodeSpec {
                label: "逆行列", category: "数値",
                description: "行優先の正方行列。ピボット付きガウス・ジョルダン法。", keywords: "matrix inverse invert",
                inputs: &[g("matrix", "行優先の数値列", DataType::Number), i("size", "次数", 2.0, 1.0, 32.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        LinearSolve => {
            static SPEC: NodeSpec = NodeSpec {
                label: "連立一次方程式", category: "数値",
                description: "Ax=b をピボット付き消去で解く。A は行優先の正方行列。", keywords: "linear solve equation matrix",
                inputs: &[g("matrix", "行優先の数値列", DataType::Number), g("rhs", "右辺 b", DataType::Number), i("size", "次数", 2.0, 1.0, 32.0)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        FourierMagnitude => {
            static SPEC: NodeSpec = NodeSpec {
                label: "離散フーリエ振幅", category: "数値",
                description: "枝の実数列から DFT の絶対値を返す。512 点以下。正規化なし。", keywords: "dft fourier spectrum magnitude",
                inputs: &[g("values", "実数のリスト", DataType::Number)], output: DataType::Number,
            };
            Some(&SPEC)
        },
        PlaneIntersectionPoint => {
            static SPEC: NodeSpec = NodeSpec {
                label: "平面交線上の点", category: "数値",
                description: "2つの平面の交線で原点に最も近い点。各平面は点と法線で指定。平行・一致はエラー。", keywords: "plane intersection point",
                inputs: &[g("a", "平面 A の点", DataType::Point), g("na", "平面 A の法線", DataType::Point), g("b", "平面 B の点", DataType::Point), g("nb", "平面 B の法線", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        PlaneIntersectionDirection => {
            static SPEC: NodeSpec = NodeSpec {
                label: "平面交線方向", category: "数値",
                description: "2平面の法線の外積を単位化。平行・一致はエラー。", keywords: "plane intersection direction",
                inputs: &[g("na", "平面 A の法線", DataType::Point), g("nb", "平面 B の法線", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        PlaneRotateAxis => {
            static SPEC: NodeSpec = NodeSpec {
                label: "平面の回転軸成分", category: "数値",
                description: "原点とX/Y方向で平面を作り、Z周りに回転したXまたはY軸を返す。", keywords: "rotate plane axis",
                inputs: &[g("origin", "原点", DataType::Point), g("axis_x", "X 方向", DataType::Point), g("axis_y", "Y 方向", DataType::Point), n("angle", "角度 (rad)", 0.0, -1e12, 1e12, 0.01), i("axis", "0:X 1:Y", 0.0, 0.0, 1.0)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        PlaneAlignAxis => {
            static SPEC: NodeSpec = NodeSpec {
                label: "平面の軸合わせ", category: "数値",
                description: "ガイドを平面内へ投影し単位化した新X軸。垂直ガイドはエラー。", keywords: "align plane vector",
                inputs: &[g("origin", "原点", DataType::Point), g("axis_x", "X 方向", DataType::Point), g("axis_y", "Y 方向", DataType::Point), g("guide", "ガイド方向", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        WorldPlaneAxis => {
            static SPEC: NodeSpec = NodeSpec {
                label: "世界平面の軸", category: "数値",
                description: "XY/XZ/YZ の右手系 X/Y/Z 軸の一つを返す。", keywords: "xy xz yz plane axis",
                inputs: &[i("plane", "0:XY 1:XZ 2:YZ", 0.0, 0.0, 2.0), i("axis", "0:X 1:Y 2:Z", 0.0, 0.0, 2.0)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        ChangeBasis => {
            static SPEC: NodeSpec = NodeSpec {
                label: "基底変換", category: "数値",
                description: "A平面の局所座標を B平面の局所座標へ変換。", keywords: "change basis coordinate transform",
                inputs: &[g("origin_a", "A原点", DataType::Point), g("x_a", "A X方向", DataType::Point), g("y_a", "A Y方向", DataType::Point), g("origin_b", "B原点", DataType::Point), g("x_b", "B X方向", DataType::Point), g("y_b", "B Y方向", DataType::Point), g("point", "点", DataType::Point)], output: DataType::Point,
            };
            Some(&SPEC)
        },
        _ => None,
    }
}

pub fn is_list_kind(kind: NodeKind) -> bool {
    use NodeKind::*;
    matches!(kind, Summation | Product | Average | Median | Variance | StdDeviation | DomainEndpoint | DomainLength | EvaluateDomain | DivideDomain | DomainIntersection | DomainUnion | SetContains | SetUnion | SetIntersection | SetDifference | CreateSet | DuplicateDetection | ClosestPoint | MatrixTranspose | MatrixMultiply | MatrixInverse | LinearSolve | FourierMagnitude)
}

pub fn accepts_empty_second(kind: NodeKind) -> bool {
    matches!(kind, NodeKind::SetUnion | NodeKind::SetIntersection | NodeKind::SetDifference | NodeKind::SetContains)
}

fn scalar(x: f64) -> Result<Vec<Value>, String> {
    if x.is_finite() { Ok(vec![Value::Number(x)]) } else { Err("計算結果が有限ではありません。".into()) }
}
fn point(p: [f64; 3]) -> Result<Vec<Value>, String> {
    if crate::engine::finite_point(p) { Ok(vec![Value::Point(p)]) } else { Err("座標が有限または許容範囲内ではありません。".into()) }
}
fn xyz(a: &[&Value], index: usize) -> Result<[f64; 3], String> {
    match a.get(index).copied() {
        Some(Value::Point(p)) if crate::engine::finite_point(*p) => Ok(*p),
        _ => Err("有効な XYZ を接続してください。".into()),
    }
}
fn plus(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(v: [f64; 3], k: f64) -> [f64; 3] { v.map(|x| x*k) }
use crate::engine::{cross, dot, sub};
fn norm(v: [f64; 3]) -> f64 { v[0].hypot(v[1]).hypot(v[2]) }
fn unit(v: [f64; 3]) -> Result<[f64; 3], String> {
    // Scaling first prevents a reciprocal overflow for subnormal inputs.
    let m = v.iter().fold(0.0_f64, |a, b| a.max(b.abs()));
    if m == 0.0 || !m.is_finite() { return Err("ゼロまたは無効なベクトルです。".into()); }
    let w = v.map(|x| x / m);
    Ok(w.map(|x| x / norm(w)))
}
fn rotate(v: [f64; 3], axis: [f64; 3], radians: f64) -> Result<[f64; 3], String> {
    let u = unit(axis)?;
    let (s, c) = radians.sin_cos();
    Ok(plus(plus(scale(v,c),scale(cross(u,v),s)),scale(u,dot(u,v)*(1.0-c))))
}
#[derive(Clone, Copy)]
struct Frame { origin: [f64;3], x: [f64;3], y: [f64;3], z: [f64;3] }
impl Frame {
    fn new(origin: [f64;3], x: [f64;3], y: [f64;3]) -> Result<Self,String> {
        let x = unit(x)?;
        let guide = unit(y)?;
        let normal = cross(x, guide);
        if norm(normal) <= 1e-12 { return Err("平面の軸が平行です。".into()); }
        let z = unit(normal)?;
        let y = unit(cross(z, x))?;
        Ok(Self {origin,x,y,z})
    }
    fn local(self, p: [f64;3]) -> [f64;3] {
        let d = sub(p,self.origin);
        [dot(d,self.x),dot(d,self.y),dot(d,self.z)]
    }
    fn world(self, p: [f64;3]) -> [f64;3] {
        plus(self.origin,plus(plus(scale(self.x,p[0]),scale(self.y,p[1])),scale(self.z,p[2])))
    }
}
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    if t == 0.0 { a } else if t == 1.0 { b } else { (1.0-t)*a+t*b }
}
fn cubic(t: f64, a: f64, b: f64) -> f64 {
    let s=1.0-t; 3.0*s*s*t*a + 3.0*s*t*t*b + t*t*t
}
/// Normalized independent mapper; cubic Bezier solves x(t)=x, not y(x) via naive t=x.
pub fn mapper(x: f64, mode: usize, controls: [f64;4]) -> Result<f64,String> {
    let [x1,y1,x2,y2] = controls;
    if !(0.0..=1.0).contains(&x) || !controls.iter().all(|p|(0.0..=1.0).contains(p)) {
        return Err("マッパーの入力と制御点は 0〜1 にしてください。".into());
    }
    match mode {
        0 => Ok(x),
        1 => {
            if x1 > x2 { return Err("ベジェ制御点 X1 は X2 以下にしてください。".into()); }
            if x == 0.0 || x == 1.0 { return Ok(x); }
            let (mut lo,mut hi)=(0.0,1.0);
            for _ in 0..48 {
                let mid=(lo+hi)*0.5;
                if cubic(mid,x1,x2)<x {lo=mid;} else {hi=mid;}
            }
            Ok(cubic((lo+hi)*0.5,y1,y2))
        }
        2 => Ok((PI*x).sin()),
        3 => Ok((1.0-(PI*x).cos())*0.5),
        4 => Ok((-0.5*((x-0.5)/0.18).powi(2)).exp()),
        5 => Ok((4.0*x).exp_m1()/4.0_f64.exp_m1()),
        _ => Err("未対応のグラフです。".into())
    }
}
fn flag(x: bool) -> f64 { if x {1.0} else {0.0} }
fn bool_value(x: f64) -> Result<bool,String> {
    if x == 0.0 {Ok(false)} else if x == 1.0 {Ok(true)} else {Err("論理値は 0 または 1 です。".into())}
}
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum Key { Number(u64), Point([u64;3]) }
fn bits(x: f64) -> Result<u64,String> {
    if !x.is_finite() {Err("有限値のみ比較できます。".into())}
    else {Ok(if x == 0.0 {0} else {x.to_bits()})}
}
fn key(value: &Value) -> Result<Key,String> {
    match value {
        Value::Number(x) => Ok(Key::Number(bits(*x)?)),
        Value::Point(p) => Ok(Key::Point([bits(p[0])?,bits(p[1])?,bits(p[2])?])),
        _ => Err("比較・集合は数値と点のみ対応します。".into())
    }
}
fn values(a: Vec<f64>) -> Result<Vec<Value>,String> {
    if a.len()>MAX_ITEMS || a.iter().any(|v| !v.is_finite()) {
        return Err("生成した数値が有限でないか、個数が上限を超えています。".into());
    }
    Ok(a.into_iter().map(Value::Number).collect())
}
fn domain(a: &[Value]) -> Result<(f64,f64),String> {
    if let [a,b]=a {Ok((a.number()?,b.number()?))} else {Err("区間は枝ごとにちょうど 2 数値が必要です。".into())}
}
fn stable_sum(a: &[f64]) -> f64 {
    let (mut s,mut c)=(0.0_f64,0.0_f64);
    for &x in a {
        let t=s+x;
        c+=if s.abs()>=x.abs() {(s-t)+x} else {(x-t)+s};
        s=t;
    }
    s+c
}

// A matrix is transported as a row-major numeric list; dimensions are explicit.
fn dim(lists: &[&[Value]], index: usize, limit: usize) -> Result<usize,String> {
    let n=lists.get(index).and_then(|l|l.first()).ok_or("行列の次元がありません。")?.number()?;
    if n.fract()!=0.0 || n<1.0 || n>limit as f64 {return Err("行列の次元が範囲外です。".into());}
    Ok(n as usize)
}
fn numeric_list(list:&[Value])->Result<Vec<f64>,String> { list.iter().map(Value::number).collect() }
fn matrix_shape(data:&[f64],rows:usize,cols:usize)->Result<(),String>{
    if data.len()!=rows*cols {Err("行列の要素数と次元が一致しません。".into())} else {Ok(())}
}
/// Scaled partial-pivot Gauss-Jordan elimination on an augmented system.
/// Returns the right block in row-major order. Rejects numerically tiny pivots.
fn eliminate(a:&[f64],rhs:&[f64],size:usize,cols:usize,stop:&dyn Fn()->bool)->Result<Vec<f64>,String>{
    matrix_shape(a,size,size)?;matrix_shape(rhs,size,cols)?;
    let w=size+cols;
    let mut aug=vec![0.0;size*w];
    let mut scales=vec![0.0_f64;size];
    for i in 0..size {
        cancelled(stop)?;
        for j in 0..size {
            let v=a[i*size+j];aug[i*w+j]=v;scales[i]=scales[i].max(v.abs());
        }
        if scales[i]==0.0 {return Err("行列が特異です。".into());}
        aug[i*w+size..(i+1)*w].copy_from_slice(&rhs[i*cols..(i+1)*cols]);
    }
    for k in 0..size {
        cancelled(stop)?;
        let mut pivot=k;
        for i in k+1..size {
            if aug[i*w+k].abs()/scales[i]>aug[pivot*w+k].abs()/scales[pivot] {pivot=i;}
        }
        if aug[pivot*w+k].abs()/scales[pivot]<=1e-12 {return Err("行列が特異、または数値的に不安定です。".into());}
        if pivot!=k {
            for j in 0..w {aug.swap(k*w+j,pivot*w+j);}
            scales.swap(k,pivot);
        }
        let d=aug[k*w+k];
        for j in 0..w {aug[k*w+j]/=d;}
        for i in 0..size {
            if i==k {continue;}
            let factor=aug[i*w+k];
            for j in 0..w {
                let q=aug[k*w+j];
                aug[i*w+j]-=factor*q;
            }
        }
        if aug.iter().any(|x|!x.is_finite()){return Err("行列計算で桁あふれしました。".into());}
    }
    let mut out=Vec::with_capacity(size*cols);
    for i in 0..size {out.extend_from_slice(&aug[i*w+size..(i+1)*w]);}
    Ok(out)
}

pub fn run_list(kind: NodeKind, lists: &[&[Value]], stop: &dyn Fn()->bool) -> Result<Vec<Value>,String> {
    use NodeKind::*;
    cancelled(stop)?;
    let a=lists.first().copied().unwrap_or(&[]);
    let b=lists.get(1).copied().unwrap_or(&[]);
    match kind {
        SetContains | SetUnion | SetIntersection | SetDifference | CreateSet | DuplicateDetection => {
            let mut aa=BTreeSet::new();
            let mut bb=BTreeSet::new();
            for val in b {cancelled(stop)?;bb.insert(key(val)?);}
            let mut out=Vec::new();
            for val in a {
                cancelled(stop)?;
                let k=key(val)?;
                let fresh=aa.insert(k);
                match kind {
                    CreateSet | SetUnion if fresh => out.push(val.clone()),
                    SetIntersection if fresh && bb.contains(&k) => out.push(val.clone()),
                    SetDifference if fresh && !bb.contains(&k) => out.push(val.clone()),
                    DuplicateDetection => out.push(Value::Number(flag(!fresh))),
                    _=>{}
                }
            }
            if kind==SetContains {
                for val in b {cancelled(stop)?;out.push(Value::Number(flag(aa.contains(&key(val)?))));}
            } else if kind==SetUnion {
                for val in b {cancelled(stop)?;if aa.insert(key(val)?) {out.push(val.clone());}}
            }
            if out.len()>MAX_ITEMS {return Err("集合の個数が上限を超えました。".into());}
            Ok(out)
        }
        DomainEndpoint | DomainLength | EvaluateDomain | DivideDomain | DomainIntersection | DomainUnion => {
            if a.is_empty() {return Ok(vec![]);}
            let (s,e)=domain(a)?;
            match kind {
                DomainLength=>scalar(e-s),
                DomainEndpoint=>{let end=b.first().ok_or("端点指定なし")?.number()?;scalar(if end==0.0{s}else{e})},
                EvaluateDomain=>values(b.iter().map(|t| t.number().map(|t|lerp(s,e,t))).collect::<Result<Vec<_>,_>>()?),
                DivideDomain=>{
                    let count=b.first().ok_or("分割数なし")?.number()? as usize;
                    if !(1..=512).contains(&count){return Err("分割数は 1〜512。".into());}
                    let mut out=Vec::with_capacity(count*2);
                    for j in 0..count {cancelled(stop)?;out.extend([lerp(s,e,j as f64/count as f64),lerp(s,e,(j+1) as f64/count as f64)]);}
                    values(out)
                }
                DomainIntersection|DomainUnion=>{
                    let (u,v)=domain(b)?;
                    let (al,ah,bl,bh)=(s.min(e),s.max(e),u.min(v),u.max(v));
                    let (lo,hi)=if kind==DomainIntersection {(al.max(bl),ah.min(bh))} else {(al.min(bl),ah.max(bh))};
                    if lo>hi {Ok(vec![])} else {values(vec![lo,hi])}
                }
                _=>unreachable!()
            }
        }
        ClosestPoint=>{
            if a.is_empty() {return Ok(vec![]);}
            let mut out=Vec::new();
            // Bound work independently of output size.
            if a.len().saturating_mul(b.len())>1_000_000 {return Err("最近点探索が大きすぎます。候補か検索点を減らしてください。".into());}
            for q in b {
                cancelled(stop)?;
                let q=xyz(&[q],0)?;
                let mut best=None;
                for (index,p) in a.iter().enumerate() {
                    if index%256==0 {cancelled(stop)?;}
                    let p=xyz(&[p],0)?;
                    let d=norm(sub(q,p));
                    if best.as_ref().is_none_or(|(old,_)|d<*old) {best=Some((d,p));}
                }
                if let Some((_,p))=best {out.push(Value::Point(p));}
            }
            Ok(out)
        }
        MatrixTranspose=>{
            let r=dim(lists,1,64)?;let c=dim(lists,2,64)?;let a=numeric_list(a)?;
            matrix_shape(&a,r,c)?;
            let mut out=vec![0.0;r*c];
            for i in 0..r {cancelled(stop)?;for j in 0..c {out[j*r+i]=a[i*c+j];}}
            values(out)
        }
        MatrixMultiply=>{
            let r=dim(lists,2,64)?;let k=dim(lists,3,64)?;let c=dim(lists,4,64)?;
            let a=numeric_list(a)?;let b=numeric_list(b)?;
            matrix_shape(&a,r,k)?;matrix_shape(&b,k,c)?;
            let mut out=vec![0.0;r*c];let mut terms=vec![0.0;k];
            for i in 0..r {cancelled(stop)?;for j in 0..c {
                for h in 0..k {terms[h]=a[i*k+h]*b[h*c+j];}
                out[i*c+j]=stable_sum(&terms);
            }}
            values(out)
        }
        MatrixInverse|LinearSolve=>{
            let a=numeric_list(a)?;
            let size=dim(lists,if kind==MatrixInverse {1}else{2},32)?;
            if kind==MatrixInverse {
                let mut identity=vec![0.0;size*size];
                for i in 0..size {identity[i*size+i]=1.0;}
                values(eliminate(&a,&identity,size,size,stop)?)
            }else{
                values(eliminate(&a,&numeric_list(b)?,size,1,stop)?)
            }
        }
        FourierMagnitude=>{
            if a.len()>512 {return Err("DFT は 512 点までです。".into());}
            let a=numeric_list(a)?;let len=a.len();
            let mut out=Vec::with_capacity(len);
            let mut re=vec![0.0;len];let mut im=vec![0.0;len];
            for k in 0..len {
                cancelled(stop)?;
                for j in 0..len {
                    let phase=-TAU*((k*j)%len) as f64/len as f64;
                    let(s,c)=phase.sin_cos();re[j]=a[j]*c;im[j]=a[j]*s;
                }
                out.push(stable_sum(&re).hypot(stable_sum(&im)));
            }
            values(out)
        }
        Summation|Product|Average|Median|Variance|StdDeviation=>{
            let mut data=Vec::with_capacity(a.len());
            for v in a {cancelled(stop)?;data.push(v.number()?);}
            if kind==Product {
                let mut result=1.0;
                if data.contains(&0.0) {return scalar(0.0);}
                for x in data {cancelled(stop)?;result*=x;if !result.is_finite(){return Err("総積が桁あふれしました。".into());}}
                return scalar(result);
            }
            if data.is_empty() {return if kind==Summation {scalar(0.0)} else {Ok(vec![])};}
            if kind==Median {
                data.sort_by(f64::total_cmp);
                let m=data.len()/2;
                return scalar(if data.len()%2==1 {data[m]} else {lerp(data[m-1],data[m],0.5)});
            }
            let len=data.len() as f64;
            let m=data.iter().fold(0.0_f64,|x,y|x.max(y.abs()));
            if m==0.0 {return scalar(0.0);}
            for x in &mut data {*x/=m;}
            let sum=stable_sum(&data);
            if kind==Summation {return scalar(sum*m);}
            let mean=sum/len;
            if kind==Average {return scalar(mean*m);}
            for x in &mut data {*x=(*x-mean).powi(2);}
            let sd=(stable_sum(&data)/len).sqrt()*m;
            scalar(if kind==Variance {sd*sd} else {sd})
        }
        _=>Err("未対応の数学リストノード。".into())
    }
}

pub fn run_item(kind: NodeKind, a: &[&Value], stop: &dyn Fn()->bool) -> Result<Vec<Value>,String> {
    use NodeKind::*;
    cancelled(stop)?;
    let x=|| num(a,0);
    let y=|| num(a,1);
    let v=|| xyz(a,0);
    let w=|| xyz(a,1);
    let result=match kind {
        Negate=>-x()?, Round=>x()?.round(), Floor=>x()?.floor(), Ceiling=>x()?.ceil(),
        Exponential=>x()?.exp(), Sinh=>x()?.sinh(), Cosh=>x()?.cosh(), Tanh=>x()?.tanh(),
        Degrees=>x()?.to_degrees(), Radians=>x()?.to_radians(),
        SinRadians=>x()?.sin(), CosRadians=>x()?.cos(),
        TanRadians=>{let t=x()?;if t.cos().abs()<=4.0*f64::EPSILON{return Err("正接の極では未定義です。".into());}t.tan()},
        Asin=>x()?.asin(), Acos=>x()?.acos(), Atan=>x()?.atan(),
        Atan2=>{let (a,b)=(x()?,y()?);if a==0.0 && b==0.0{return Err("原点の角度は未定義です。".into());}a.atan2(b)},
        Root=>{let (a,b)=(x()?,y()?);if b<1.0 || b.fract()!=0.0{return Err("次数は正整数です。".into());}if a<0.0 {if b%2.0!=1.0{return Err("負の数の偶数根は実数ではありません。".into());}-(-a).powf(1.0/b)}else{a.powf(1.0/b)}},
        Logarithm=>{let(a,b)=(x()?,y()?);if a<=0.0||b<=0.0||b==1.0{return Err("対数の定義域外です。".into());}a.ln()/b.ln()},
        Factorial=>{let a=x()?;if !(0.0..=170.0).contains(&a)||a.fract()!=0.0{return Err("階乗は 0〜170 の整数です。".into());}(1..=a as u32).fold(1.0,|p,n|p*n as f64)},
        Clamp=>{let(lo,hi)=(y()?,num(a,2)?);if lo>hi{return Err("最小が最大を超えています。".into());}x()?.clamp(lo,hi)},
        GhRange=>{let(s,e)=(x()?,y()?);let steps=num(a,2)? as usize;if !(1..=512).contains(&steps){return Err("分割数は 1〜512。".into());}let mut out=Vec::with_capacity(steps+1);for j in 0..=steps {cancelled(stop)?;out.push(lerp(s,e,j as f64/steps as f64));}return values(out)},
        ConstructDomain=>return values(vec![x()?,y()?]),
        GraphMapper=>mapper(x()?,y()? as usize,[num(a,2)?,num(a,3)?,num(a,4)?,num(a,5)?])?,
        BoolAnd=>flag(bool_value(x()?)? & bool_value(y()?)?), BoolOr=>flag(bool_value(x()?)? | bool_value(y()?)?), BoolXor=>flag(bool_value(x()?)? ^ bool_value(y()?)?), BoolNot=>flag(!bool_value(x()?)?),
        Equals=>flag(key(a.first().copied().ok_or("入力 A なし")?)? == key(a.get(1).copied().ok_or("入力 B なし")?)?),
        LargerThan=>flag(x()?>y()?), SmallerThan=>flag(x()?<y()?),
        ConstructVector=>return point([x()?,y()?,num(a,2)?]),
        VectorComponent=>{let index=y()? as usize;*v()?.get(index).ok_or("成分の範囲外です。")?},
        UnitX=>return point([x()?,0.0,0.0]), UnitY=>return point([0.0,x()?,0.0]), UnitZ=>return point([0.0,0.0,x()?]),
        VectorLength=>norm(v()?),
        Unitize=>return point(unit(v()?)?), ReverseVector=>return point(scale(v()?,-1.0)),
        DotProduct=>dot(v()?,w()?),
        CrossProduct=>return point(cross(v()?,w()?)),
        VectorAngle=>{let(u,v)=(unit(v()?)?,unit(w()?)?);norm(cross(u,v)).atan2(dot(u,v))},
        VectorAddition=>return point(plus(v()?,w()?)),
        VectorSubtraction=>return point(sub(v()?,w()?)),
        VectorBetweenPoints=>return point(sub(w()?,v()?)),
        VectorProjection=>{let u=unit(w()?)?;return point(scale(u,dot(v()?,u)))},
        RotateVector=>return point(rotate(v()?,w()?,num(a,2)?)?),
        PlaneProject|PlaneCoordinates|PlanePoint|PointInPlane=>{
            let f=Frame::new(v()?,w()?,xyz(a,2)?)?;
            let p=xyz(a,3)?;
            let local=f.local(p);
            match kind {
                PlaneProject=>return point(f.world([local[0],local[1],0.0])),
                PlaneCoordinates=>return point(local),
                PlanePoint=>return point(f.world(p)),
                _=>flag(local[2].abs()<=num(a,4)?)
            }
        }
        PlaneNormal3Pt=>{let p=v()?;let f=Frame::new(p,sub(w()?,p),sub(xyz(a,2)?,p))?;return point(f.z)},
        VectorMultiply=>return point(scale(v()?,y()?)),
        VectorDivide=>{let d=y()?;if d==0.0{return Err("ゼロでは割れません。".into());}return point(v()?.map(|x|x/d));},
        VectorAmplitude=>{let amp=y()?;if amp<0.0{return Err("長さは非負です。".into());}return point(scale(unit(v()?)?,amp));},
        Lerp=>lerp(x()?,y()?,num(a,2)?),
        SmoothStep=>{let t=x()?.clamp(0.0,1.0);t*t*(3.0-2.0*t)},
        BezierValue=>{
            let t=num(a,4)?;
            let (p,q,r)=(lerp(x()?,y()?,t),lerp(y()?,num(a,2)?,t),lerp(num(a,2)?,num(a,3)?,t));
            lerp(lerp(p,q,t),lerp(q,r,t),t)
        }
        NormalPdf=>{let sigma=num(a,2)?;if sigma<=0.0{return Err("標準偏差は正です。".into());}let z=(x()?-y()?)/sigma;(-0.5*z*z).exp()/(TAU.sqrt()*sigma)}
        PlaneIntersectionDirection=>{
            let d=cross(unit(v()?)?,unit(w()?)?);
            if norm(d)<=1e-12 {return Err("平面が平行または一致しています。".into());}
            return point(unit(d)?)
        }
        PlaneIntersectionPoint=>{
            let (p,n,q,m)=(v()?,unit(w()?)?,xyz(a,2)?,unit(xyz(a,3)?)?);
            let d=cross(n,m);let mag=norm(d);
            if mag<=1e-12{return Err("平面が平行または一致しています。".into());}
            return point(scale(plus(scale(cross(m,d),dot(n,p)),scale(cross(d,n),dot(m,q))),1.0/(mag*mag)));
        }
        PlaneRotateAxis=>{
            let f=Frame::new(v()?,w()?,xyz(a,2)?)?;
            let axis=if num(a,4)?==0.0{f.x}else{f.y};
            return point(rotate(axis,f.z,num(a,3)?)?)
        }
        PlaneAlignAxis=>{
            let f=Frame::new(v()?,w()?,xyz(a,2)?)?;
            let guide=unit(xyz(a,3)?)?;
            let d=sub(guide,scale(f.z,dot(guide,f.z)));
            if norm(d)<=1e-12 {return Err("ガイドが平面に垂直です。".into());}
            return point(unit(d)?)
        }
        WorldPlaneAxis=>{
            let (ex,ey,ez)=([1.0,0.0,0.0],[0.0,1.0,0.0],[0.0,0.0,1.0]);
            let (ux,uy)=match x()? as usize {0=>(ex,ey),1=>(ex,ez),2=>(ey,ez),_=>return Err("世界平面の種類が無効です。".into())};
            let axes=[ux,uy,cross(ux,uy)];
            return point(*axes.get(y()? as usize).ok_or("平面の軸が無効です。")?)
        }
        ChangeBasis=>{
            let from=Frame::new(v()?,w()?,xyz(a,2)?)?;
            let to=Frame::new(xyz(a,3)?,xyz(a,4)?,xyz(a,5)?)?;
            return point(to.local(from.world(xyz(a,6)?)))
        }
        RandomNormal=>{
            let count=x()? as usize;let mean=y()?;let sigma=num(a,2)?;let seed=num(a,3)? as u64;
            if count>512||sigma<0.0{return Err("乱数パラメータが範囲外です。".into());}
            let mut rng=crate::gh_nodes::Rng::new(seed);
            let mut out=Vec::with_capacity(count);
            for _ in 0..count {
                cancelled(stop)?;
                let u=(1.0-rng.next_f64()).max(f64::MIN_POSITIVE);
                let v=rng.next_f64();
                out.push(mean+sigma*(-2.0*u.ln()).sqrt()*(TAU*v).cos());
            }
            return values(out)
        }
        _=>return Err("未対応の数学ノード。".into())
    };
    scalar(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use NodeKind::*;
    fn run(k:NodeKind, input:&[Value])->Result<Vec<Value>,String>{
        run_item(k,&input.iter().collect::<Vec<_>>(),&||false)
    }
    fn close(a:f64,b:f64){assert!((a-b).abs()<1e-10, "{a} != {b}");}
    #[test]
    fn scalar_domains_and_errors(){
        assert_eq!(run(Factorial,&[Value::Number(0.0)]).unwrap(),vec![Value::Number(1.0)]);
        assert_eq!(run(Root,&[Value::Number(-27.0),Value::Number(3.0)]).unwrap()[0].number().unwrap().round(),-3.0);
        assert!(run(Root,&[Value::Number(-1.0),Value::Number(2.0)]).is_err());
        assert!(run(Logarithm,&[Value::Number(2.0),Value::Number(1.0)]).is_err());
        assert!(run(Asin,&[Value::Number(2.0)]).is_err());
        assert!(run(Exponential,&[Value::Number(1000.0)]).is_err());
        assert!(run(TanRadians,&[Value::Number(PI/2.0)]).is_err());
    }
    #[test]
    fn range_includes_both_ends_and_reverses(){
        let a=run(GhRange,&[Value::Number(2.0),Value::Number(-2.0),Value::Number(4.0)]).unwrap();
        assert_eq!(a,vec![2.0,1.0,0.0,-1.0,-2.0].into_iter().map(Value::Number).collect::<Vec<_>>());
    }
    #[test]
    fn orthogonal_and_rotation(){
        let x=[1.0,0.0,0.0];let y=[0.0,1.0,0.0];let z=[0.0,0.0,1.0];
        assert_eq!(dot(x,y),0.0);assert_eq!(cross(x,y),z);
        let out=rotate(x,z,PI/2.0).unwrap();close(norm(out),1.0);close(out[1],1.0);
        assert!(unit([0.0;3]).is_err());
        close(norm(unit([f64::MIN_POSITIVE,0.0,0.0]).unwrap()),1.0);
        close(run(VectorAngle,&[Value::Point(x),Value::Point(y)]).unwrap()[0].number().unwrap(),PI/2.0);
    }
    #[test]
    fn frame_round_trip(){
        let f=Frame::new([3.0,4.0,5.0],[1.0,1.0,0.0],[1.0,0.0,1.0]).unwrap();
        close(dot(f.x,f.y),0.0);close(dot(f.x,f.z),0.0);close(dot(cross(f.x,f.y),f.z),1.0);
        let p=[4.0,-2.0,1.0];let q=f.local(f.world(p));for j in 0..3{close(p[j],q[j]);}
        assert!(Frame::new([0.0;3],[1.0,0.0,0.0],[2.0,0.0,0.0]).is_err());
    }
    #[test]
    fn statistics_and_sets(){
        let a=[1.0,2.0,3.0,4.0].map(Value::Number);
        for (k,expected) in [(Summation,10.0),(Product,24.0),(Average,2.5),(Median,2.5),(Variance,1.25)]{
            close(run_list(k,&[&a],&||false).unwrap()[0].number().unwrap(),expected);
        }
        let b=[Value::Number(0.0),Value::Number(-0.0),Value::Point([0.0;3])];
        assert_eq!(run_list(CreateSet,&[&b],&||false).unwrap().len(),2);
        assert_eq!(run_list(SetUnion,&[&b,&[]],&||false).unwrap().len(),2);
        assert_eq!(run_list(SetIntersection,&[&b,&[]],&||false).unwrap().len(),0);
        assert!(run_list(Average,&[&a],&||true).is_err());
    }
    #[test]
    fn linear_algebra_and_fourier(){
        let a=[4.0,7.0,2.0,6.0];let i=[1.0,0.0,0.0,1.0];
        let inv=eliminate(&a,&i,2,2,&||false).unwrap();
        for (x,y) in inv.into_iter().zip([0.6,-0.7,-0.2,0.4]) {close(x,y);}
        let x=eliminate(&[0.0,2.0,1.0,3.0],&[4.0,7.0],2,1,&||false).unwrap();
        close(x[0],1.0);close(x[1],2.0);
        assert!(eliminate(&[1.0,2.0,2.0,4.0],&i,2,2,&||false).is_err());
        let a=[1.0,1.0,1.0,1.0].map(Value::Number);
        let f=run_list(FourierMagnitude,&[&a],&||false).unwrap();
        close(f[0].number().unwrap(),4.0);for v in &f[1..]{close(v.number().unwrap(),0.0);}
    }
    #[test]
    fn intersection_and_basis(){
        let p=Value::Point([1.0,0.0,0.0]);let n=Value::Point([1.0,0.0,0.0]);
        let q=Value::Point([0.0,2.0,0.0]);let m=Value::Point([0.0,1.0,0.0]);
        assert_eq!(run(PlaneIntersectionPoint,&[p,n,q,m]).unwrap(),vec![Value::Point([1.0,2.0,0.0])]);
        assert_eq!(run(WorldPlaneAxis,&[Value::Number(1.0),Value::Number(2.0)]).unwrap(),vec![Value::Point([0.0,-1.0,0.0])]);
    }
    #[test]
    fn mapper_and_seed(){
        let c=[0.25,0.25,0.75,0.75];
        for x in [0.0,0.1,0.5,0.9,1.0]{close(mapper(x,1,c).unwrap(),x);}
        assert!(mapper(0.5,1,[0.9,0.0,0.1,1.0]).is_err());
        let a=[10.0,0.0,1.0,8.0].map(Value::Number);
        assert_eq!(run(RandomNormal,&a).unwrap(),run(RandomNormal,&a).unwrap());
    }
}
