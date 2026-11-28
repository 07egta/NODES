# OMEGA 07 — 折り紙 / 建築造形

2026-11-28 JST: main を土台とする専用ブランチ。既存の HexPanels / TriPanels は平面セル、NurbsPanel は NURBS パッチ。PR #55 は曲面パネル・morph、物理緩和は PR #82 と main の physics_nodes が担当するため、ここでは変更しない。

## 今回の対象

- ミウラ折りの有限矩形パッチ：剛体面を保つ解析的な 1 パラメータの折り畳み。
- 折り畳みメッシュ、折線の山/谷/外周フィルタ、平面展開図、個別のパネル輪郭。
- 折線データを計算核に保持し、将来の拘束ソルバへ接続する契約を記述する。既存の Value / 保存形式に変更を加えない。
- 日本語のノード名、英語の検索語、ネイティブノード棚の図形アイコン、編集可能な作例、独立幾何テスト。

## 参考（公開仕様・挙動のみ、ソースを流用しない）

- https://origamisimulator.org/ — 折り角と山/谷の区別、FOLD フォーマットの考え方。
- https://grasshopperdocs.com/addons/lunchbox.html — パネル単位と構造線の分離。
- https://amandaghassaei.com/projects/origami_simulator/ — 剛体折り紙の拘束と動的ソルバの区別。

## 検証状況

作業開始時点。コンテナに rustc/cargo がなく、GitHub CI によるビルド・fmt・clippy・test が必要。強度、法規、実機造形、自己衝突不存在は保証しない。

## 保存した実装

- src/origami.rs: 2 ノード、折り率 0〜0.95、各方向最大64セル、有限値・寸法検証、協調キャンセル、生成前の重量見積り。
- NodeKind、engine、ノード棚アイコン、テンプレートに接続。既存保存型は変更していない。
- 折り率 f に対し theta = alpha*f。頂点 (i,j) = (i*a*cos(theta)+(j mod 2)*s, j*h, (i mod 2)*a*sin(theta))、s=b*cos(alpha)/cos(theta)、h=sqrt(b²-s²)。各四辺形は平行四辺形で、両辺長と内積が一定。折り率は実際の二面角ではない。
- 展開図は折線・外周の単一 Curve リスト。山谷分類、パネル個別出力、ソルバ向け折線トポロジー型は未実装。次段階で隣接面 ID と向き付き二面角を定義し、物理担当と共有する。
- 独立の JavaScript 数値計算で alpha=10/55/85 度、折り率0/0.1/0.5/0.95、両方の行列パリティの辺・対角長の最大絶対誤差 7.11e-15 mm（a=13,b=21）。Rust実行ではない。
- Rust 単体テスト2件を追加したが実行していない。fmt/clippy/build/GUI/保存再読込未検証。CI結果を確認してから統合すること。
