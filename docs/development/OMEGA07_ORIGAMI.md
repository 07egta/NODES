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
