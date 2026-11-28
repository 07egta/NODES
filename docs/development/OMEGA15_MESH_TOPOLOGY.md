# OMEGA 15 — メッシュ位相ラボ（第1段階）

基点: `ec3b840e81c76aa7e426da5dc47b716ba975a98e` (main, 2026-11-28 JST)。
専用ブランチ: `feat/omega15-mesh-topology-20261128`。既存PR #48（SolidCraft統合済み）、#55（形状モーフ・パネル）、#83（性能）、#86（色彩）を確認。共通型や属性の大規模移行は他のPRと調整が必要なため行わない。

## 実装方針

- 既存の `mesh_fabrication` の監査・辺使用表・union-findを再利用。従来の `MeshAudit`（8数値）、`MeshBoundary`（線分）、保存ID、ポートの意味を変更しない。
- `mesh_boundary_loops` は面の向きに沿った閉じたポリラインを返す。最小頂点番号から巡回し、最小番号順に出力。先頭点は重複させない。空境界は空リスト。非多様体頂点、非多様体辺、逆向き面、重複・退化面はエラー。
- `mesh_islands` は辺を共有する面を成分に分け、最初の元面番号順に出力。座標と面の巻き方向を保持、各島のローカル番号に振り直す。頂点だけの接触は別の島。孤立頂点は除去。3面以上が共有する辺は接続したまま。修復・Booleanではない。
- `mesh_topology_stats` は [V, E, F, 孤立頂点, 辺連結の面成分, 境界辺, 全頂点のχ, 面複体のχ]。χ=V−E+F。重複・退化面は拒否する。非多様体にもインデックス複体の計数として使えるが、種数・自己交差・内外を証明しない。
- `mesh_fill_holes` は危険なファン分割へのフォールバックを廃止し、共有の検証付き平面多角形三角形化を利用。非平面・不正境界は明示的エラー。上限より大きいループは残す。キャップの追加で位相が悪化した場合もエラー。キャップ同士や元面との幾何交差・入れ子や意図した開口の判断は未実装。
- 永続的なHalf Edgeはまだ導入しない。Half Edgeは頂点fan、隣接face、twin/nextの巡回を簡潔にする一方、各辺に複数の面が接する入力には別のincidence構造が要る。Winged Edgeは辺の両側と前後をまとめる方式だが、三角形中心の既存型から移行するには属性・非多様体・メモリ費用の比較が必要。現在は監査と既存辺表を優先し、後続で圧縮配列/CSRと比較する。
- 追加依存なし。外部コードは取り込まず公開機能仕様・論文概要のみを参照。

## 公開仕様の参考

- Weaverbird: https://grasshopperdocs.com/addons/weaverbird.html （位相・境界・細分化・造形）
- MeshEdit: https://grasshopperdocs.com/addons/meshedit.html （面分解・未使用頂点・境界・法線）
- Blender manual: https://docs.blender.org/manual/en/3.6/modeling/modifiers/generate/subdivision-surface.html （クリース・辺ループ・法線）
- MeshLab filter documentation: https://pymeshlab.readthedocs.io/en/latest/filter_list.html （Taubinなど平滑化の仕様）
- Jakob et al. 2015, Instant Field-Aligned Meshes: https://rgl.epfl.ch/publications/Jakob2015Instant （方向場と位置場による三角・quad-dominantリメッシュ）
- Quad Remesher: https://exoside.com/quadremesher/quadremesher-doc/ （商用SDKは導入しない）
- Kangarooについては既存 `physics_nodes` を再利用する方針。追加の公開仕様調査は継続課題。

## 検証

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --test omega15_mesh_topology
cargo test --locked --lib mesh_tools
cargo test --locked --all-targets
cargo run --locked --release --example mesh_topology_benchmark
```

テストは閉じた四面体（球と同相、χ=2）、トーラス（χ=0）、開いた面、孤立頂点、複数殻、頂点接触、3面共有辺、向き不一致、重複/退化/範囲外面、キャンセル、制限、保存・再評価を扱う。編集可能な `examples/mesh-fabrication/topology-laboratory.morph` を追加。
ベンチマークは4096、65536、250000頂点の平面メッシュを測る。現行上限は250000頂点・500000三角形・ノード出力weight制限あり。100万頂点は現行上限外。上限を緩めて速くなったとは主張しない。
**作成環境にRust/CargoとローカルGitHub checkoutがなく、Rustコンパイル、rustfmt、Clippy、テスト、実時間測定は未実行。CIで確認し、緑になるまでマージしない。**

## 継続課題

Half Edge/CSRの比較実測、属性付き頂点・辺・面と再マッピング（現行Meshは座標と三角形のみ）、多角形・quadの永続化、crease、双対・stellate・bevel等の造形、cotangent/HC/bilateral smoothing、曲率適応細分化、QEM、UV展開、フィールド誘導quad remesh、自己交差を含む厳密判定、頑健なmesh Boolean（単純結合と区別）、100万頂点規模の全体メモリ・時間予算。後続担当はPRとCIを確認し、まず共有基盤を検証する。
