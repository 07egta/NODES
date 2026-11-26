# NODES architecture

更新: 2026-11-27 JST。現在の実装を記す。目標機能は [ROADMAP](../ROADMAP.md)、検証範囲は [TEST_RESULTS](../TEST_RESULTS.md) に分ける。crate名と実行ファイル名は既存互換のため `morphlet` / `Morphlet`、画面上の製品名はNODES。

## 構成

Rustのライブラリがデータモデルと計算を提供し、同じpackageのeframe/eguiバイナリがUIを提供する。Rhino/Grasshopperのruntimeや商用SDKをロードしない。基本操作・計算・保存はオフラインで行える。

| 責務 | 主なmodule |
|---|---|
| 登録・型・グラフ・上限 | `model`、カテゴリごとのNodeSpec |
| 共通アクセス規則 | `node_access`、`data_tree`、`data_tree_nodes` |
| 評価・キャッシュ・時間計測 | `engine`、`latest_channel`、`cluster` |
| 数学・式・従来ノード | `math_nodes`、`expr`、`gh_nodes` |
| 精密曲線 | `curve_geometry`、`curve_nodes` |
| 精密曲面・限定ソリッド | `surface_kernel`、`surface_curvature`、`surface_nodes` |
| メッシュ・生成 | `mesh_fabrication`、`mesh_geometry_checks`、`generative` |
| 保存・移行・入出力 | `persistence`、`compatibility`、`import`、`stl_export`、`ghx` |
| 安全な外部編集 | `graph_api`、CLI |
| 制限付きスクリプト | `script`（Rhai）、`expr` |
| 編集画面 | `app`、`canvas`、`ui_polish`、`theme`、`data_view`、`help` |
| 描画・造形情報 | `viewport`、`raster`、`print_info` |

新しいカテゴリ処理を巨大なengineへまとめない。stable snake_caseのserialized ID、NodeSpec、共通アクセス、kernel、独立期待値テスト、graph/save/reloadテストを一単位とする。登録は現在compile-time enumであり、dynamic plugin registryは未実装。

## Graph / Type / Tree

`NodeId=u64`。入力はport keyからsource NodeIdへの対応で、現在は**各ノード1出力**。複数出力には保存形式とwireの移行が必要。型検査を接続時と評価時に行い、未知port、循環、重複ID、不正座標・非有限数などを拒否する。

`Value` はNumber、Point、Curve、Mesh、Surface、Solidを保持する。形状payloadはArc共有。VectorはPoint、Boolean/Domain等は数値表現のものがあり、専用型との互換性を意味しない。runtimeのNull値はまだ無い。`Slot<T>` のMissing/Nullの区別は将来の基盤であり、ValueのNull実装ではない。

`Tree = BTreeMap<Vec<u32>, Vec<Value>>` によりパス順を決定的にする。空の枝と枝そのものが無い状態を区別する。Flatten/Graft/Simplify、枝操作、制限構文のPath Mapper等は共通moduleを使う。

### 入力アクセス

- Item: 対応要素ごとに評価。Longest、Shortest、Crossを選択できるノードをmetadataで明示する。
- List: 枝内リスト全体。空のindexやcutリストも意味のある入力としてkernelへ渡す。
- Tree: 全枝とパスをまとめて渡す。
- FirstItem: 設定値は現在、その入力範囲の先頭のみを読む。一般の混合schedulerの代替ではない。
- SingleItem: matched branchごとにちょうど1個を要求する。
- Custom: Script/Cluster固有の規則。

新しいkernelは原則としてordinalの枝照合。元のevolve/allノードは既存graphの結果を変えないため、明示的なmatching設定が無い場合に従来のsame-path/broadcast規則を維持する。枝の選択方針はNODESの仕様であり、GH1/GH2全般の一致を保証しない。

## 評価と非同期UI

検証付きトポロジカル走査はnodeへの直接参照を返す。初期rootと各child集合をIDで整列し、ready queueをFIFOで処理するため、保存順に依存しない決定的な順序になる。各kernelは協調キャンセルを確認し、キャンセルを通常の失敗結果としてcacheしない。

cache keyはkind、数値のIEEE bit値、入力元IDと評価revision、scriptやcluster等のsemantic state。名称・座標・previewの変更は計算を無効化しない。下流だけが再計算され、エラー結果もcacheする。保持重みとordered evictionを使い、削除されたnodeの結果を除去する。

UIと計算workerはlatest-only mailboxを使用し、未受信snapshotを各方向1個に制限する。リビジョンが古い要求・結果を破棄し、編集中のUIを長いkernelから分離する。**現在はworker1本でkernelは逐次評価**。複数coreへの並列schedulerは未実装。

NodeTimingは入力/cache key準備からkernelまたはcache取得、資源検査、出力格納までの壁時計時間。Cluster内部は外側へ含める。validation前の中止には偽のnode時間を作らず、途中中止はpartial timingだけ残す。表示は計測値であり、CPU使用率やframe時間ではない。

## 幾何の表現と精度

Curveは表示点列と数学的基底を別に保持する。NURBSでは次数、完全なknot vector、制御点、重みを扱う。affine変換や対応した曲面生成は精密基底を保持し、非線形の点列変形で基底を失った場合は精密NURBSと名乗らない。長さ、最近接点、交差等の数値解は個別の公差・制限を持つ。

Surfaceは解析primitiveまたはNURBSを保持し、表示用Meshへ明示的に離散化する。1階/2階微分、第一/第二基本形式から曲率を求める。法線はSu×Sv、第二基本形式はn・Sijなので外向き球のHは-1/r、Kは1/r²。極や退化接線、非C2内部knot近傍は明示的に扱う。

Solid/B-repは限定的で、一般のトリム曲面・位相修復・fillet・Booleanではない。箱専用演算と一般CAD機能を区別する。メッシュの連結はBooleanではない。永続SubD位相やcreaseも未実装。

## メッシュ・印刷

頂点はf64、三角形indexはu32。面積/符号付き体積、境界・非多様体辺・向き・退化面などを検査する。これらだけで自己交差の不存在や造形成功を証明しない。

修復では内殻を無条件に外向きへ反転しない。複数殻の包含は未判定と記録し、単一閉殻の向き補正では局所原点と補償和を用いる。凹多角形のOBJ/穴埋めは検証付き三角形化へ移行し、非平面・自己交差・曖昧な複数境界を推測で埋めない。

STLはf32へ丸めた後の退化も検査する。OBJ/3MFもmesh出力であり、NURBS/B-rep交換ではない。材料質量は体積と密度からの概算で、slicerのinfill/support/実測重量とは異なる。

## 保存・互換性・外部能力

native v1 JSONは検証付きで読み、原子的な一時file置換で保存する。8MiB書込上限とflush/syncを使用する。未知のnative node/fieldは原fileを変更せず拒否する。編集可能なopaque未知nodeの保存は未実装。

project/API request/GHXの入力はopen前に通常ファイルか確認し、open後のdescriptorでも確認する。FIFO等を通常入力として待ち続けないための対策であり、敵対的なファイル置換競合を完全に排除する保証ではない。読込byte上限も維持する。

独立missionで食い違ったClosestPoint port名とtree-transform keyは明示移行し、競合を上書きしない。GHXは公開情報に基づく限定的な独立import/archiveであり、未知内容と接続を保持するが、binary GHや全component変換は未対応。

JSON Graph APIはversion付き、transactional編集、型/循環/範囲検証を通る。Script、ImportMesh、Cluster、Expression、PathMapperをAPIの実行対象へ含めない。純粋なtree設定はnative graphと同じvalidatorを使う。任意のshell/file/network操作を提供しない。

Rhaiは既存の制限付きscript nodeで、操作数200000、call depth16、script8KiB、文字列4096、時間2秒などの上限を持つ。module/time/custom-syntaxを無効にする。制限された組込関数の追加にも、割当・実行時間・キャンセル・能力の監査が必要。

## 上限

| 対象 | 制限 |
|---|---|
| Graph nodes | 16384 |
| Items / branches | 16384、他の重量予算も適用 |
| Path depth | 32 |
| Mesh | 250000頂点 / 500000三角形 |
| 結果・cacheの論理重量 | 3000000（bytesではない） |
| native project | 8MiB |
| untrusted API request | 1MiB / 8192 edits |
| GHX | 8MiB、XML depth/element/wire数制限 |

上限は快適な動作や全組合せでのメモリ量を保証する値ではない。計測値は別文書でsource SHAと環境に結び付ける。

## 描画とUI

eframe/egui 0.33.3、Glow/OpenGL。現在の3D previewにはsoftware depth bufferとtexture uploadを使う経路があり、独自GPU CAD rendererとは異なる。図形・raster・画面出力のcacheを分け、静止中の不要な再描画を抑える。

canvasにはzoom/pan、可視nodeの描画抑制、wireの索引、minimap、検索、整列、group/note、Undo/Redo等を置く。日本語/英語検索、適切なport説明とエラー修正を優先する。数千nodeの快適性はGUI実測が必要。

Inspectorの入力行はnode IDとport keyに基づく明示UI scopeを持ち、その内部に通常のhorizontal行を置く。上に評価エラーが追加されても入力focusを保ち、残り縦領域を行の高さとして消費しない。ドラッグは1回の履歴にまとまるが、複数フレームの文字入力は各変更をUndoする。

## ライセンスと拡張

projectのMIT OR Apache-2.0方針を維持する。GPL/LGPL/AGPL依存を新規採用しない。依存宣言と実際の配布本文・対象platformの照合は [DEPENDENCIES](DEPENDENCIES.md) で範囲と未調査を記す。既存の他ライセンス宣言を隠さない。

複数出力、Null、一般mixed-access、動的plugin API、並列評価、精密B-rep、CAD exchangeは今後の独立した設計/移行課題。正常な既存graphを壊す一括書換えを避け、比較fixtureを先に用意する。
