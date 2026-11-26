# Mission 07 — メッシュ・SubD・製造ツール

基準: evolve/all @ df77c542473a5c637f3353dec48e367dd67db027.
evolve/stl-import, evolve/stl-export, evolve/viewport-print は基準に取り込み済み。
既存 .morph schema_version=1、従来の NodeKind 名、Cargo.toml/Cargo.lock を変更しない。
追加26ノードは既存の「立体」カテゴリ、日本語名と英語検索語を持つ。
この文書は Grasshopper 互換性や造形成功を宣言するものではない。

## 実装一覧

| ノード | 意味・制限 |
|---|---|
| MeshConstruct | 枝ごとの点リストと0始まりの面番号（3個ずつ）から三角形メッシュ |
| MeshVertices / MeshFaces | 頂点と面番号。1出力の現行設計に合わせ別ノード |
| MeshNormals | 面の単位法線を XYZ 点として出力 |
| MeshEdges / MeshBoundary / MeshNonManifold | 全辺 / 1面共有 / 3面以上共有の辺 |
| MeshArea / MeshCenter | 表面積・面積加重表面重心（質量重心ではない） |
| MeshVolume / MeshMass | 符号付き体積・正体積に対する100%充填相当重量（mm, g/cm³） |
| MeshDimensions | AABBの幅・奥行・高さ |
| MeshClosest | 点と三角形の全走査による最近点 |
| MeshAudit | 8項目の整数診断 |
| MeshFlip / MeshUnify | 全反転 / 隣接面の向きの整合。内外は推定しない |
| MeshWeldExact / MeshUnweld | 同一XYZ頂点の結合 / 全面独立化。角度指定Weld互換ではない |
| MeshCompact / MeshDeleteFace | 未使用頂点削除 / 1枚削除（穴が開く可能性あり） |
| MeshSmooth | 境界固定のLaplacian平滑化。収縮・交差の可能性あり |
| MeshSubdivide | 共有中点による1面→4面。元の三角形面を維持 |
| MeshCatmullClark | 三角形制御メッシュから四角形細分化、最後に三角形化 |
| MeshOnBed | 各メッシュをXY原点、最小Z=0に移動。複数部品の相対位置保持は既存エクスポート側を使用 |
| MeshSelfContact | 共有頂点を持たない面ペアの近似接触数 |
| MeshThicknessSample | 面中心の内向き法線上の出口距離の最小サンプル値とサンプル数 |

### 診断結果の読み方
MeshAudit は次の順に数値を出力する。複数メッシュを同一枝に入れた場合、8数値ずつ連結される。個別に読むには先にGraftする。
1. 境界辺
2. 非多様体辺
3. 隣接面の方向不一致辺
4. 非多様体頂点（分離した頂点ファンを含む）
5. 完全一致の重複頂点（-0 と +0 は同じ）
6. 同じ3頂点番号からなる重複面
7. 退化面
8. 未使用頂点

インデックスの位相条件は幾何学的な閉じた立体の十分条件ではない。
退化面は既存グラフ評価が上流で拒否するため、この診断で数えるのは主に直接API入力。
完全一致Weldは許容距離・法線角度によるWeldとは異なる。
自己接触候補はAABBの1軸スイープと三角形の分離軸判定（1e-8モデル単位の許容値）。
共有頂点のあるペアを除外するので隣接面の重なりは未検出。
4,000,000候補を超える場合はエラー。ゼロは完全な自己交差なしを意味しない。
肉厚は最大512面の等インデックス間隔サンプリングで内向き法線の出口距離を調べる。
面積加重でなく、斜めの最短距離や未サンプルの局所的な薄肉を見逃す。4,000,000レイ面テストまで。
サンプルに出口がない場合、出口の向きが不正の場合、予算超過の場合はエラー。
体積は閉じた向きのそろったインデックス位相を要求し、符号を保持する。
自己交差・入れ子・重複する別部品を検査しないため物理的な体積保証ではない。

## SubD
Catmull–Clark の面点、辺点、頂点点の標準ルールを独立実装。
境界辺は中点、境界頂点は 3/4 P + 1/8(Pprev+Pnext)。
四角形を内部で保持してから最後に三角形化し、既存プレビューと接続する。
元の制御メッシュは上流ノードで保持される。独立したSubD型、編集UI、内部クリース、対称拘束、数学的極限曲面は未実装。
最大5段。各段の生成前に頂点・三角形・重み上限を確認。

## 3MF
以前の全ボディ単一objectから、入力Meshごとに独立object/resource/build itemへ変更。
同一Mesh内の内外殻は分割しない。負体積・開境界・方向不一致・非多様体のmodel出力を拒否する。
独立したMeshが空洞の内殻を意味する場合は同じMeshにまとめ、外面・内面の向きを適切にする必要がある。
既存の自動修復・ボディ分割は内外殻を完全には判別しないため中空形状には慎重に使用する。
座標は丸め桁数固定ではなくRustの数値表示を使う。色と材料、3MF読み込みは未実装。
自己交差・重複物体・ネスティングの検証、Bambu Studioでの読込み、実機印刷は未確認。
STL/OBJの読み書きは既存コードを使用。

## 例と検証
examples/mesh-fabrication/ に5つの編集可能なグラフと参照OBJ/STL。
丸い箱の参照は別途JavaScriptで同じ数学的ルールを実装し計算したもので、NODES実行結果ではない。
参照: 146頂点、288三角形、432辺、辺の共有数と向き不整合0、符号付き体積約66896.522 mm³。
テストは Rust の単体テスト、5作例の評価と寸法変更、旧10作例のデシリアライズ、STL/OBJ round-trip、3MFの複数objectと不正形状拒否を含む。
負荷テストは100,000・500,000三角形トーラスを生成する opt-in テスト。1,000,000三角形の拒否を通常テストに含む。
現行の1メッシュ上限25万頂点・50万三角形・ノード重み150万に従う。50万三角形は直接API試験でありグラフ評価では重み上限に達する。
面・辺抽出は既存リスト上限8192、各重い処理は可能な範囲で協調キャンセルをチェックする。
この作業環境に cargo/rustc、GUI、スライサー、実機はなく、Rustのコンパイルと実行は未確認。CI結果を別途確認すること。
Windowsで実行:
```powershell
cargo test --all-targets --locked
cargo build --release --locked --bin Morphlet
cargo test --release --locked --test fabrication_tests audit_stress -- --ignored --nocapture
cargo run --release --locked --bin Morphlet -- --validate examples/mesh-fabrication/rounded-box.morph
```

## 保留・非互換
NURBS/B-rep変換、角度Weld、真のQuad Remesh、QEM簡略化、Boolean、頑健な穴埋め・交差修復、
SubD Creaseと制御メッシュ編集、正確な最小肉厚、Voronoi・格子照明の専用グラフ、3MF色/読み込みは今回未完成。
既存Meshは三角形なのでTriangulateは表現上不要だが四角形入力対応を意味しない。
結合は既存Merge、厚みは既存Shell、張り出しは既存OverhangCheckを再利用する。

## 公開仕様（参考）
- Grasshopper Mesh一覧（第三者カタログ、GH1）: https://grasshopperdocs.com/addons/grasshopper-mesh.html
- McNeel RhinoCommon Mesh（公開APIの意味を参照、独立実装）: https://developer.rhino3d.com/api/rhinocommon/type/rhino.geometry.mesh/
- Pixar, Subdivision Exterior Calculus for Geometry Processing, Supplemental Material: https://graphics.pixar.com/library/SEC/supplemental.pdf
- 3MF Consortium Core 4.1: https://github.com/3MFConsortium/spec_core/blob/master/3MF%20Core%20Specification.md
新規外部依存はない。Rhino/Grasshopper実装のソースやバイナリは使用しない。
