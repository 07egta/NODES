# Mission 04 — 独立数学・ベクトルツールキット

Base: `df77c542473a5c637f3353dec48e367dd67db027` (evolve/all). MIT OR Apache-2.0. 既存の .morph / NodeKind 識別名を変更しない。実装は独自であり、GH/GHX 互換は主張しない。

## 公開資料
- McNeel GH1: https://developer.rhino3d.com/guides/grasshopper/gh-algorithms-and-data-structures/data-structures/ — Range は N+1、Series は要素数 N。
- McNeel GH1: https://developer.rhino3d.com/guides/grasshopper/the-why-and-how-of-data-trees/ — item/list mapping。
- Grasshopper Docs (第三者のコンポーネント索引): https://grasshopperdocs.com/addons/grasshopper-maths.html , https://grasshopperdocs.com/addons/grasshopper-vector.html , https://grasshopperdocs.com/completeIndex.html
- GH2 はベータで全 GH1 コンポーネントを備えるわけではない: https://discourse.mcneel.com/t/rhino-beta-feature-grasshopper-2-beta/221336 。GH2 の個々のコンポーネントの実測照合なし。

## 追加登録 (72)

| NodeKind | 表示 | 評価単位 | 出力 |
|---|---|---|---|
| Negate | 符号反転 | item | Number |
| Round | 四捨五入 | item | Number |
| Floor | 切り下げ | item | Number |
| Ceiling | 切り上げ | item | Number |
| Exponential | 指数関数 | item | Number |
| Sinh | 双曲線正弦 | item | Number |
| Cosh | 双曲線余弦 | item | Number |
| Tanh | 双曲線正接 | item | Number |
| Degrees | 度へ変換 | item | Number |
| Radians | ラジアンへ変換 | item | Number |
| Asin | 逆正弦 | item | Number |
| Acos | 逆余弦 | item | Number |
| Atan | 逆正接 | item | Number |
| SinRadians | 正弦 (rad) | item | Number |
| CosRadians | 余弦 (rad) | item | Number |
| TanRadians | 正接 (rad) | item | Number |
| Atan2 | 二引数逆正接 | item | Number |
| Root | n乗根 | item | Number |
| Logarithm | 対数 | item | Number |
| Factorial | 階乗 | item | Number |
| Clamp | 範囲に収める | item | Number |
| Summation | 総和 | list | Number |
| Product | 総積 | list | Number |
| Average | 平均 | list | Number |
| Median | 中央値 | list | Number |
| Variance | 分散 (母集団) | list | Number |
| StdDeviation | 標準偏差 (母集団) | list | Number |
| GhRange | 範囲 (分割数) | item | Number |
| ConstructDomain | 区間を作る | item | Number |
| DomainEndpoint | 区間の端点 | list | Number |
| DomainLength | 区間長 | list | Number |
| EvaluateDomain | 区間を評価 | list | Number |
| DivideDomain | 区間分割 | list | Number |
| DomainIntersection | 区間の共通部分 | list | Number |
| DomainUnion | 区間を包む | list | Number |
| GraphMapper | グラフマッパー | item | Number |
| BoolAnd | 論理 AND | item | Number |
| BoolOr | 論理 OR | item | Number |
| BoolXor | 論理 XOR | item | Number |
| BoolNot | 論理 NOT | item | Number |
| Equals | 等しい | item | Number |
| LargerThan | より大きい | item | Number |
| SmallerThan | より小さい | item | Number |
| SetContains | 集合に含む | list | Number |
| SetUnion | 集合の和 | list | Any |
| SetIntersection | 集合の積 | list | Any |
| SetDifference | 集合の差 | list | Any |
| CreateSet | 集合を作る | list | Any |
| DuplicateDetection | 重複を調べる | list | Number |
| ConstructVector | ベクトル XYZ | item | Point |
| VectorComponent | ベクトル成分 | item | Number |
| UnitX | X 単位ベクトル | item | Point |
| UnitY | Y 単位ベクトル | item | Point |
| UnitZ | Z 単位ベクトル | item | Point |
| VectorLength | ベクトル長 | item | Number |
| Unitize | 単位化 | item | Point |
| ReverseVector | ベクトル反転 | item | Point |
| DotProduct | 内積 | item | Number |
| CrossProduct | 外積 | item | Point |
| VectorAngle | ベクトル角度 | item | Number |
| VectorAddition | ベクトル加算 | item | Point |
| VectorSubtraction | ベクトル減算 | item | Point |
| VectorProjection | ベクトル投影 | item | Point |
| VectorBetweenPoints | 2点間ベクトル | item | Point |
| RotateVector | ベクトル回転 | item | Point |
| ClosestPoint | 最近点 | list | Point |
| PlaneProject | 平面へ投影 | item | Point |
| PlaneCoordinates | 平面座標 | item | Point |
| PlanePoint | 平面上の点 | item | Point |
| PointInPlane | 点が平面上か | item | Number |
| PlaneNormal3Pt | 3点から平面法線 | item | Point |
| RandomNormal | 正規乱数 | item | Number |

## 既存・制約
- Add/Subtract/Multiply/Divide/Power/Abs/Min/Max/Modulo/Series/Range/Random/Bounds/Remap/Expression/Point/Distance/PointComponent は既存。Sine は旧波形関数であり、純粋な正弦は SinRadians を追加。既存 Range は要素数、GhRange は分割数。既存の数値ノードや .morph の意味を変更しない。
- 既存エンジンの同一パス・単枝ブロードキャスト・長いリスト優先の item マッチングを使用。list ノードは枝単位、設定が複数ある場合は最初の値を使う場合がある。集合は空リストを許す。未接続は従来通りエラー。GH1 の完全なマッチングは未検証。
- 現行 Value は Number, Point, Curve, Mesh のみ。ベクトルは Point の XYZ、論理は数値 0/1、区間は枝内の二数値、平面は原点 + 2軸入力による独立計算。厳密な異種 Point/Vector 型区別、区間/平面型、多出力ポートは未実装。複数区間の同一枝での合成は未対応。点と数値の集合判定は型別・完全一致 (±0 は同一)、文字列・曲線・メッシュは未対応。
- 角度は新規ノードでは rad を基本とし、Degrees/Radians を別途提供。旧ノードの角度仕様は維持。
- 区間の合併は包絡区間、交差なしは空。DivideDomain は隣接小区間を平らな端点列で返す。GH1 の完全同等の多出力・型ではない。
- ベクトル出力は既存 Point の座標上限 1e9 に従う。平面は右手系に直交化、平行軸は拒否。最近点探索は候補×検索点 100万以下、同距離は初出。
- Graph Mapper は数値出力 (線形/Bezier/Sin/Cos/Gaussian/指数)、既存 GraphProfile と別ノード。独自の曲線定義。数式パーサは既存 expr.rs。行列、逆行列、線形方程式、フーリエ、最適化、完全な Plane ノード群、型付き集合は後続。
- 総和/平均はスケーリング + 補償和、標準偏差は母集団。総積は途中オーバーフローをエラーとしアンダーフローは f64 の通常挙動。乱数は既存 SplitMix64 を利用。GH の乱数系列の同一性は主張しない。

## 検証
`src/math_nodes.rs` に単体テストを追加（解析解、エラー、座標往復、集合、再現性）。この変更を作成した環境は cargo/rustc が無く、GitHub の直接 clone も不可。コンパイル、cargo test、UI 実機確認、GH 実行比較は未実行。CI とレビューが必要。
