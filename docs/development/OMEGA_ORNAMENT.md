# OMEGA 11 — 装飾パターン開発記録

- 起点: main `ec3b840e81c76aa7e426da5dc47b716ba975a98e` (2026-11-28 JST)
- 専用ブランチ: `feat/omega-ornament-20261128`。main 直接変更なし。
- AGENTS.md / ROADMAP / ARCHITECTURE / STATUS、open PR 9件を確認。既存の Voronoi2d、Delaunay2d、HexPanels、TriPanels、CylinderWrap を再利用。PR #55 の形状モーフ・メッシュパネル、#52/#53 の生物数式パターンは対象外。
- 追加依存・SolidCraftの追加取り込みなし。MIT OR Apache-2.0 方針を維持。プラグインの実装・アイコン・文言は複製せず、公開されている数学的定義と利用例から独立実装。

## Checkpoint 1
5ノード: TruchetPattern、AsanohaPattern、ShippoPattern、PowerDiagram2d、LloydRelax2d。日本語の登録・検索・入力説明、既存評価器・Data Tree・出力上限と接続。Truchetのシードは座標ハッシュでグリッド拡張時にも既存タイルを保持。麻の葉の格子共有辺は一度だけ生成。七宝は円の折れ線近似。Power は中心原点の長方形内の power distance (二乗距離 - 重み) による半平面クリップで、重み1個または点と同数。ほぼ同一点（正規化距離1e-9以内）は大きい重みを優先。同重みは最初を保持。空・微小セルを省くため出力インデックスは入力に一対一ではない。Lloydは一様密度のセル面積重心へ同時更新し、重複点・退化セルを明示エラーにする。最大128点、12反復。正規化による数値精度改善はあるが厳密演算ではない。

作例: examples/ornament/ の5つの編集可能な.morph。円筒Truchetは線のスタディで、造形可能な閉じたソリッドを保証しない。既存のPipe等と組み合わせる場合も接合部・肉厚・自己交差を別途検証する。一般曲面の測地線・面積補正は未実装。

## 参考資料
- LunchBox surface panels: https://grasshopperdocs.com/addons/lunchbox.html
- Parakeet tilings: https://www.grasshopper3d.com/group/parakeet
- PanelingTools: https://wiki.mcneel.com/labs/panelingcommands
- Truchet arcs: https://mathworld.wolfram.com/TruchetTiling.html
- Bridges 2023 Hitomezashi/Truchet: https://archive.bridgesmathart.org/2023/bridges2023-457.html
- 組子の実作品・技法: https://k-ino.jp/en/kumiko/kumiko-technique/
- 七宝: https://www.blossommerz.com/patterns/shippo/
- Power diagrams: https://doi.org/10.1137/0216006
- Surface sampling (density/minimum distance): https://docs.blender.org/manual/ja/3.2/modeling/geometry_nodes/point/distribute_points_on_faces.html
- Houdini Labs tiling: https://www.sidefx.com/docs/houdini/nodes/sop/labs--mesh_tiler-1.0.html

## 検証・再開
追加したRustテスト: タイル端点・麻の葉辺の非重複・重み付きセルの面積/空セル/キャンセル・重心/重複点、入力アクセス規則、作例パラメータ編集→保存→再読込→再評価。現環境にcargo/rustcがなく、git cloneもDNS失敗。ローカルのRustビルド・fmt・clippy・テスト・実GUIは未実行。CI確認後に修正し、未検証のままマージしない。
次: CIの失敗修正、曲線の境界クリッピング（穴を持つ輪郭を含む）、青海波の遮蔽処理、Wang/Penrose、平面の17壁紙群、曲面距離・面積補正、レリーフ/織物/製造可能な交差接合を別チェックポイントで実装。
