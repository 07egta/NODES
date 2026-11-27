# Ω: 次回の統合任務と確認済みの障害

最終調査: **2026-11-27 00:04 UTC**。対象ソースは `feat/nodes-omega-geometry` の **`0aa286a93a8b61f23ce691b1d15a025780ec1fc8`**。この文書の作業では実装、Cargo、commit、push は行っていない。GitHub の PR 情報・Actions の job/step・固定 SHA のソースを調査し、`git merge-tree BASE OURS THEIRS` で作業ツリーを変えずに統合差分を確認した。

現在のソース checkpoint の全 gate は統括担当が **520 passed / 0 failed / 3 ignored、fmt/check/strict Clippy 合格**と記録している。以下の他 PR の検証結果と混同しない。候補 PR の本文にあるローカル試験結果は、この監査で再実行した結果ではない。

## 1. 固定した PR・head・CI

すべて調査時点で **open / draft / unmerged**。GitHub の `mergeable=true` はそれぞれの PR の base に対する値であり、この Ω ブランチへの安全な統合を保証しない。

| PR | head branch / exact SHA | PR base | 確認した Actions |
| --- | --- | --- | --- |
| [#14: multi-port](https://github.com/07egta/NODES/pull/14) | `feat/nodes-multi-port-components` / `35ccdb491fac5da2669b14755b7793910282ef84` | `develop/nodes-ultimate` / `c52e850ed01a82461557fab9182778092151db2d` | [45906728248](https://github.com/07egta/NODES/actions/runs/45906728248): failure。既知の整形 gate 失敗。 |
| [#17: multi-port CI](https://github.com/07egta/NODES/pull/17) | `fix/omega-multiport-ci` / `1668e31decd0dde491adfa359816324cba518583` | PR14 head `35ccdb4` | [45910115735](https://github.com/07egta/NODES/actions/runs/45910115735): **Clippy failure**。fmt・inventory scripts・dependency audit・check は成功、Rust tests と Windows は skipped。 |
| [#18: diagnostics + numerics](https://github.com/07egta/NODES/pull/18) | `feat/omega-graph-diagnostics-20261127` / `bf98da477f3cbe9826d04f12ce885ecaf1380cf3` | `develop/nodes-ultimate-api-edits` / `6b29c1cab3239494766e11c4799b557acc54469a` | [45914084423](https://github.com/07egta/NODES/actions/runs/45914084423): **in progress**。00:04 UTC に再照会。本文のローカル 472/0/2 ignored は別の記録。 |
| [#19: bounded picker](https://github.com/07egta/NODES/pull/19) | `fix/omega-picker-bounded-listing` / `25297ed5893fcbd0584ba2a07fb38f25aef178d1` | `main` / `79f32745923adfa9e3de05ef4683b6e3b5f4948f` | 取得した PR-triggered workflow 一覧に run なし。検証済みとは扱わない。 |
| [#20: alternative diagnostics](https://github.com/07egta/NODES/pull/20) | `feat/omega-graph-diagnostics` / `4ef925c4153b3cc045213147b68d4d16ca61a8e9` | PR15 / `a1ffa40071c0c3508b890065bd12220c5abb80d9` | [45911023329](https://github.com/07egta/NODES/actions/runs/45911023329): desktop build libraries のインストールで failure。Rust gates は未到達。コードが原因と断定しない。 |
| [#21: semantic project diff](https://github.com/07egta/NODES/pull/21) | `feat/omega-semantic-project-diff` / `7478fd12fdcd55195e821fd17291351cc7076b25` | `main` / `79f3274` | 取得した PR-triggered workflow 一覧に run なし。Python の小さい再現例のみ本監査で実行。 |
| [#22: fabrication import](https://github.com/07egta/NODES/pull/22) | `feat/omega-fabrication-import` / `342f7476f1921a66d51b3292691f2c2ad2f949f5` | PR15 / `a1ffa40071c0c3508b890065bd12220c5abb80d9` | [45911030830](https://github.com/07egta/NODES/actions/runs/45911030830): formatting failure。check/test/Clippy は未到達。 |

Actions 照会は connector が返した PR-triggered run の先頭ページに限定される。「一覧にない」は全 CI が存在しないという意味ではない。着手時は exact head を再照会し、他の開発者の追加入力を保全する。

## 2. 基点と競合の整理

| 統合候補 | Ω checkpoint との merge base | 静的な統合判断 |
| --- | --- | --- |
| PR17（PR14 を包含） | `c52e850ed01a82461557fab9182778092151db2d` | Ω 側 17 commits / 相手側 5 commits。merge-tree の conflict marker は `docs/development/STATUS.md`, `src/canvas.rs`, `src/engine.rs`, `src/model.rs`。同時変更ファイルはさらに app/CLI/cluster/data_tree_nodes/graph_api/lib/persistence に及ぶ。 |
| PR18 | `6b29c1cab3239494766e11c4799b557acc54469a` | Ω 側 14 / 相手側 10 commits。text conflict marker なし。ただし app/CLI/lib/math_nodes/STATUS は両側変更。診断契約と数値修正を関数単位でレビューする。 |
| PR19 / PR21 | `79f32745923adfa9e3de05ef4683b6e3b5f4948f` | Ω 側は main 基点から 155 commits 進んでいる。text conflict marker なしでも、PR19 の新テストには現 API と引数型の不整合がある。 |
| PR20 / PR22 | `a1ffa40071c0c3508b890065bd12220c5abb80d9` | Ω 側 11 commits。text conflict marker なし。PR20 は PR18 と同じ CLI コマンド名を追加する。PR22 は独立した import 改善として先行検証できる。 |

PR17 を取り込むと PR14 の履歴も入る。両方を別の実装として重複移植しない。候補ブランチのファイルを丸ごと checkout して競合解決すると、Ω の精密幾何・exact 集計/補間・signed-zero history・mesh 診断・安全な出力準備が巻き戻り得る。現在の checkpoint から独立した統合ブランチを作り、必要な基盤を小さい単位で検証・保存する。

## 3. 現在の single-output 制約

次表は Ω `0aa286a` の実装そのもの。複数の数値を返すことと、別々の型付き出力ポートを持つことを区別する。

| 対象 | コード根拠 | 利用上の制約 / PR14–17 での到達点 |
| --- | --- | --- |
| Graph / evaluation | `src/model.rs`: `NodeSpec.output: DataType`, `Node.inputs: BTreeMap<String, NodeId>`。`src/engine.rs`: `Evaluation.outputs` と `Cache.result` は 1 Tree / node。 | 接続元に出力ポートを指定できない。PR17 は `PortRef { node, port }`, `port_links`, `port_outputs`, grouped cache を加えるが、port ごとの独立 revision ではない。 |
| Surface | `src/surface_nodes.rs::run`: EvaluateSurface は point、SurfaceDirection は axis 指定で normal/du/dv、PrincipalCurvatures は 2 個の Number を同一結果リストへ返す。 | point/normal/derivatives を同時に取り出すには別ノードが必要。主曲率の 2 値は独立 port ではない。PR17 は EvaluateSurface に 4 ports を追加するが、既存 evaluate を合計 4 回呼ぶ adapter。主曲率などはまだ単一出力。 |
| Mesh / fabrication | `src/mesh_fabrication.rs::run_item`: MeshVertices / MeshFaces / MeshNormals は別ノード。MeshFaces は三角形ごとの 3 index を連続した Number にする。MeshAudit は境界辺など 8 個の Number のリスト。 | 診断指標に名前付き port がなく、複数メッシュの結果はリスト/枝の対応を利用側で管理する。PR17 DeconstructMesh は vertices と flattened face indices の 2 ports、メッシュ item ごとに path を追加。normals や audit 8 指標の port 化までは含まない。 |
| Script | `src/script.rs::evaluate_node` は入力 Tree を flatten。`execute` / `run_script` は `{0}` の単一 Tree を返す。Surface/Solid 入力は明示的に拒否。 | 入力枝の保存、名前付き複数出力、精密曲面/B-rep の直接操作は未対応。PR17 `cluster::evaluate_ports` でも Script を primary port 0 の 1 結果へ包む。数値の exact lerp 修正はこの制約を変えない。 |
| Cluster / reusable part | `src/cluster.rs::ClusterDef.output: NodeId`、最大 8 exposed inputs、`run_def` の最終結果は 1 Tree。 | PR17 は内部の副出力接続を扱うが、外部インターフェースは依然 1 output node の primary。副出力を直接公開する複数出力 cluster ではない。 |
| UI / CLI / API | app の values/export/error paths、CLI checked_evaluate、graph_api build_file は現在 `Evaluation.outputs` を読む。 | 現ブランチでは正しい single-output 前提。PR17 の secondary result 導入後は、各 consumer の意味を明示して移行する必要がある。 |

固定した実装参照: [Ω model](https://github.com/07egta/NODES/blob/0aa286a93a8b61f23ce691b1d15a025780ec1fc8/src/model.rs), [Ω surface adapters](https://github.com/07egta/NODES/blob/0aa286a93a8b61f23ce691b1d15a025780ec1fc8/src/surface_nodes.rs), [Ω Script](https://github.com/07egta/NODES/blob/0aa286a93a8b61f23ce691b1d15a025780ec1fc8/src/script.rs), [PR17 multi-port](https://github.com/07egta/NODES/blob/1668e31decd0dde491adfa359816324cba518583/src/multi_port.rs), [PR17 cluster](https://github.com/07egta/NODES/blob/1668e31decd0dde491adfa359816324cba518583/src/cluster.rs)。

## 4. 優先任務

以下のコマンドは **次回実行する検証**。この文書作成時には実行していない。Cargo は省容量設定を維持する。

```sh
export PATH=/home/agent/.cargo/bin:$PATH
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
```

### P0-1: PR17 の gate と矛盾した API version テストを修正

**根拠:** [Linux job 140216166099](https://github.com/07egta/NODES/actions/runs/45910115735/job/140216166099) の実ログは `src/model.rs:1596` の `collapsible_if` と `src/multi_port.rs:651` の `type_complexity` の 2 件を報告する。整形だけの修正では CI は完了していない。

さらに PR17 `src/graph_api.rs:484` の `untrusted_capabilities_and_versions_are_rejected` は `api_version:2` の parse が Err と断言している。同じファイルの `supported_version` は v1/v2 を許可する。この矛盾はソースで確定しており、Clippy の後にテスト gate を塞ぐ。監査では Cargo を実行していないので「失敗テストを実行済み」とは記録しない。

- **編集:** `src/model.rs`, `src/multi_port.rs`, `src/graph_api.rs`, `tests/multi_port_integration.rs`。
- **実装方針:** if を簡潔化し、materialize の結果型に名前を付ける。古い拒否 fixture は unsupported v3 に直し、v2 acceptance / named output routing の正例を追加・維持する。テスト削除や lint 抑制で通さない。
- **検証:** `cargo test --locked --lib graph_api::tests::untrusted_capabilities_and_versions_are_rejected`; `cargo test --locked --test multi_port_integration`; `cargo clippy --all-targets --locked -- -D warnings`。
- **依存:** PR17 最新 head を再確認し、既存担当が修正中なら同じ修復を重複実装しない。

### P0-2: port identity を保って現在の計算・保存基盤へ統合

**根拠:** PR17 の Key は port-aware だが、毎回 params / links / port order を新しい BTreeMap/Vec に収集する。Ω の `Key::matches` は cache hit の照合時に元データを借用している。機能移行時に以前の allocation 削減を失わないようにする。port order のうち表示だけの変更と StreamMerge の評価順は分離して考える。

- **編集:** 主に `src/model.rs`, `src/engine.rs`, `src/node_access.rs`, `src/multi_port.rs`, `src/canvas.rs`, `src/app.rs`, `src/graph_api.rs`, `src/persistence.rs`, `src/cluster.rs`, CLI。`Node` の struct literal を持つ既存テスト/作例も列挙する。
- **必須保持:** signed-zero の parameter bit 比較、cache の weight 上限・cancel latch、曲線/曲面の exact runtime geometry、point/mesh 型の既存検証、入力 alias、atomic save、FIFO/非通常ファイル拒否、既存の Undo/Redo/copy の原子性。
- **検証:** `cargo test --locked --test multi_port_integration --test access_evaluation_regressions --test node_access_contracts --test node_timing_tests --test persistence_tests --test part_io_regressions --test graph_api_tree_tests --test cli_workflows`; app の copy/paste/history テストを含む全 target。
- **性能ゲート:** 現在の `examples/coverage_perf.rs` 等と同一条件で cache hit・一つの slider 変更を比較し、変更前後の source SHA を固定する。port ごとの dirty revision は第一統合の必須条件にせず、計測した node-level invalidation を維持する。
- **依存:** P0-1。現在の Ω checkpoint を基点に PR17 の履歴を統合し、4 個の text conflict を意味に基づいて解消する。

### P0-3: 副出力の失敗・選択・保存の公開契約を完成

**根拠:** PR17 `Evaluation` は primary を `outputs`、secondary を `port_outputs` に保存する。`multi_port::evaluate` は CurveEvaluate の point 成功 / tangent 失敗を許す一方、`graph_api::build_file` の成功判定（同 SHA の 425 行付近）と CLI `checked_evaluate` は primary だけを見る。app の `error_line`, `values_window`, selected export も primary だけを読む。副出力のエラーは実装上あり得るが、全体の成功表示に反映されない経路が残る。

- **編集:** `src/engine.rs` の結果参照 API、`src/graph_api.rs`, `src/bin/nodes-cli.rs`, `src/app.rs`, `src/canvas.rs`、診断モジュールを採用する場合はそこも。
- **契約:** usable primary と失敗した secondary を区別して表示する。v1 の primary 利用を維持しつつ、v2 の使用/要求 port の失敗を検査し、部分成功を完全成功と報告しない。副出力を選ぶ export UI を作るまでは、明示した Output ノードへ配線する経路を案内する。すべての secondary 失敗で正常な point 利用まで無条件に禁止する設計にはしない。
- **fixture:** stationary curve の Evaluate point は成功、tangent は失敗。point consumer / tangent consumer を別々に接続し、再評価・副出力 tooltip・JSON diagnostics・保存の判定を確認。disabled/cancelled でも古い port 結果を表示しない。
- **検証:** `cargo test --locked --test multi_port_integration --test cli_workflows --test curve_geometry_tests --test surface_kernel_tests`。GUI で副出力接続→値 viewer→失敗修正→Undo→save/reopen を操作する。
- **依存:** P0-2。古い PR13 の全ファイルを置換せず、必要な all-port viewer / secondary error 回帰だけを現実装へ適用する。

### P1-4: PR18 / PR20 の診断 API を一本化し port-aware にする

**根拠:** PR18 `diagnostics.rs` と PR20 `graph_diagnostics.rs` は異なる JSON schema で `nodes-cli diagnose` を追加する。PR18 は `checked_evaluate` を通し failure exit 1 と `error.diagnostics` を維持する。PR20 は `graph_api::evaluate` の結果を report にして Ok を返し、node error を含む診断でも CLI success になる。両方をそのまま merge しない。

PR18 は bounded root/affected lists、snapshot revision、実 GUI/性能記録があり、採用基盤として優先する。PR20 の measured dependency path は必要なら独立した追加 field として取り込む。どちらも現在 `node.inputs` / primary `evaluation.outputs` を読むので、multiport 統合後は linked port の失敗・ordered fan-in・動的入力を正しく読むよう修正する。PR18 の「cluster が実際には消費しない spare input を原因にしない」条件を保持する。

- **編集:** `src/diagnostics.rs`, `src/diagnostics_ui.rs`, `src/app.rs`, `src/bin/nodes-cli.rs`, `src/lib.rs`, `src/main.rs`。重複した command arm / schema を残さない。
- **検証:** PR18 の `diagnostics_tests`, `diagnostics_oracle`, `diagnostics_root_ordinals`, `diagnostics_interleaved`, `cli_workflows` と multi-port fixture。正常 primary / 失敗 secondary、unused Cluster input、disabled、途中 cancel、同一上流の複数 wires を比較する。
- **性能:** PR18 の before/after 計測は診断 API のもの。schema probe / frozen library の確認を維持し、whole-app の高速化と混同しない。
- **依存:** 公開契約選択は今できる。最終 port-aware adapter は P0-2/3 と同時に統合する。

### P1-5: PR18 の線形代数・ベクトル修正と Ω の exact arithmetic を併存

**根拠:** PR18 `src/math_nodes.rs` は matrix row normalization の underflow/subnormal 保護、FMA product residual を用いる DotProduct/CrossProduct、微小方向の angle を修正する。同じファイルで Ω は Summation/Average と Lerp を独立 exact helper へ接続済み。text conflict がなくても関数単位で双方の経路が残ることを確認する。

- **編集:** `src/math_nodes.rs`、PR18 数値回帰テスト。`src/number_aggregates.rs`, `src/number_interpolation.rs`, `src/script.rs` の共有は維持し、旧 scaled sum / weighted lerp に戻さない。
- **検証:** `python3 scripts/generate_sum_oracle.py --check`; `python3 scripts/generate_interpolation_oracle.py --check`; `cargo test --locked --test vector_precision_regressions --test vector_angle_regressions --test linear_algebra_regressions --test sum_rounding_regressions --test interpolation_oracle --test script_numeric_regressions --test math_precision_regressions`。
- **限界:** PR18 の補償ベクトル積は一般の任意精度演算ではない。入力座標 bound と underflow の記載を残す。
- **依存:** PR18 exact head の CI を確認。診断 UI と別の小さい integration checkpoint にできる。

### P1-6: 曲面・メッシュの複数結果を型と枝対応を保って公開

**根拠:** §3 の packed list と PR17 の repeated evaluation。今回改善した surface jet / adaptive area / mesh topology audit は kernel として利用できるが、stable な複数出力契約へ露出していない。

- **編集:** `src/multi_port.rs`, `src/surface_nodes.rs`, `src/mesh_fabrication.rs`, `src/node_access.rs`, inventory / node documentation。kernel は必要な共有結果 API の追加だけに留める。
- **方針:** EvaluateSurface は同じ入力 tuple の point/du/dv を一度の jet から生成し、normal の局所失敗は port 単位に保つ。principal curvatures / MeshAudit の既存 primary list を無断で別の値へ変えず、versioned contract または別 component で名前付き結果を増やす。DeconstructMesh の vertices / indices / normals が同じ mesh item path を共有することを定義する。
- **検証:** `cargo test --locked --test surface_numeric_regressions --test surface_support_regressions --test surface_curvature_tests --test fabrication_tests --test export_topology_regressions --test multi_port_integration`。非単位 knot・繰返し knot・zero derivative・空枝・複数 geometry items を含める。
- **性能:** `examples/geometry_numeric_benchmark.rs` の kernel 測定と、実 graph adapter の計測を分ける。4 回 evaluate を 1 回へ変えた効果は実測後に報告する。
- **依存:** P0-2/3。登録名や出力欄数だけで互換性を VERIFIED にしない。

### P1-7: Cluster / .morphpart の複数出力インターフェース

**根拠:** PR17 の cluster 内部は port-aware でも `ClusterDef.output: NodeId` と `Result<Tree, String>` の外部戻り値は単一のまま。`PART_VERSION` は 1 のままで port_links を含む新しい Node 形式を保存するため、古い reader と新しい payload の境界も明示する必要がある。

- **編集:** `src/cluster.rs`, `src/multi_port.rs`, `src/model.rs`, `src/persistence.rs`, cluster UI / catalog。
- **契約:** exposed output は stable endpoint と label/type の一覧にする。旧 output node は primary endpoint へ移行。collapse/explode、外部 wires の再接続、nested cluster fingerprint、part import の id remap、最大 nesting/weight/cancel を保つ。
- **検証:** 新しい cluster contract suite に secondary-only output、2 external consumers、nested part、reordered ports、save/reopen、unknown output rejection を追加。`cargo test --locked --test multi_port_integration --test part_io_regressions --test persistence_tests` と既存 cluster unit tests。
- **依存:** P0-2/3。現在の安全な part file 読み込みを旧 PR17 の無条件 File::open へ戻さない。

### P1-8: Script の tree/port adapter を明示的な新契約として設計

**根拠:** Ω と PR17 とも Script は 1 output。入力 flatten と `{0}` output は既存互換性の一部であり、暗黙変更すると既存作例の意味が変わる。exact Surface/Solid は mesh への明示変換が必要なまま。

- **編集:** `src/script.rs`, `src/script_ui.rs`, `src/multi_port.rs`, `src/cluster.rs`、script contracts の文書。
- **方針:** 既存 script は primary-only として維持。tree paths / named outputs を opt-in の明示した adapter で渡す。戻り値全体で item/byte/geometry/time budgets を計上し、型不一致と一つの port の失敗を分離する。読み込みだけで script を実行する導線やファイル/ネットワーク capability を増やさない。
- **検証:** `cargo test --locked --test script_tests --test script_numeric_regressions --test interpolation_oracle --test multi_port_integration`、空 branch / empty result / timeout / cancel / mixed geometry / huge outputs の新契約 fixture。
- **依存:** P0-2/3、必要なら P1-7。restricted Graph API の Script 禁止を別件として解除しない。

### P1-9: PR21 の signed-zero と native schema 2 対応

**確認済み再現:** PR21 の `scripts/project_diff.py` を固定 SHA から読み込み、メモリ内の有効な最小 v1 project の `Number.params.value` を `-0.0` → `+0.0` に変更すると **`{"changed": false, "change_count": 0, "changes": []}`** を返した。bits は `8000000000000000` / `0000000000000000`。`equal` の通常数値比較が原因。Ω の `persistence::same_parameter_bits` と signed-zero-aware cache / history の意味に一致しない。

また `normalize_project` は schema_version 1 だけを許すため、PR17 が保存する v2 は `only NODES schema_version 1 is supported` で拒否する。port_links の正規化コードが存在することだけでは v2 対応とはならない。

- **編集:** `scripts/project_diff.py`, `tests/test_project_diff.py`, CLI documentation。
- **方針:** params は Rust reader と同じ f64 に正規化して bits を比較。整数の ID 等は整数のまま比較し、JSON の `1` / `1.0` の意味を不用意に変えない。v1/v2 envelope を検査し、legacy primary wire と explicit primary wire の同値、ordered fan-in / non-primary port の差を検査する。unknown payload を消す処理は導入しない。
- **検証:** `python3 -m unittest discover -s tests -p test_project_diff.py`。上記 signed-zero、schema2、port order、重複 JSON key、unsafe/nonregular input、cluster 内部の変更を fixture にする。
- **依存:** signed-zero 修正は独立。v2 の意味は P0-2 の保存契約に合わせる。PR18 が保留した別 diff prototype と重複した active command を作らない。

### P1-10: PR22 の import 改善を既存 export 回帰と合わせて先行検証

**根拠:** 変更は `src/import.rs`, `tests/import_tests.rs` のみ。ASCII STL を facet/loop/vertex/endfacet/solid の state machine にし、BOM、OBJ inline comments、byte size / cancel を扱う。現 Ω との text conflict はないが CI は fmt で止まっている。

- **編集:** この 2 ファイルの整形と必要な回帰、import/export contracts の文書。
- **検証:** `cargo test --locked --test import_tests --test polygon_import_repair --test export_numeric_regressions --test export_topology_regressions --test export_batch_regressions --test stl_export_tests`。正常な binary/ASCII round trip、切れた facet、孤立 vertex、BOM、OBJ comment、cancel、薄い有効三角形を比較する。
- **依存:** multiport と独立して検証・保存できる。input grammar の厳格化と topology/printability の診断を混同しない。

### P2-11: PR19 を FileTask API に合わせて取り込む

**根拠:** PR19 は raw scan 10,000 / visible 2,000 に分離し、bounded selection と tie-break を追加する。ただし main 基点の新テストは `read_directory(path, "morph")` を呼ぶ。現在の Ω は `read_directory(path, task: FileTask)` で、ImportMesh は STL と OBJ の両方を受け入れる。text merge が成功しても新テストの文字列引数は型に一致しない。これは静的確認であり、この監査ではコンパイルしていない。

- **編集:** `src/file_picker.rs` の限定的な統合。現 `FileTask::accepts` / import / part / export filters を保持する。
- **検証:** `cargo test --locked --bin Morphlet directory_listing_tests`; `cargo test --locked --test ux_tests --test import_tests`。mixed extensions、case ties、directory-first、非対象ファイルが 2,000 を超える directory を確認する。
- **性能:** scan cap を増やすので UI 応答の改善は件数別の実測後に判断する。全 directory の完全なソートや全件表示を保証しない。
- **依存:** 独立作業として並列化できる。現在の task filter API を旧 extension-only API に戻さない。

## 5. 次回の開始手順

1. 最新 Ω branch / PR16 / STATUS と上表の PR heads を再照会する。対象 source checkpoint が更新されていれば、その追加差分を先に読み、今回の固定 SHA を current と偽って使わない。
2. PR17 の既存担当と重複しない状態で P0-1 を閉じる。PR18 と PR20 は診断公開契約を一つ選ぶ。PR22 import、PR19 picker、PR21 semantic diff はファイル担当を分離できる。
3. Ω の全 regressions を維持した統合ブランチで P0-2/3 を実施。port-aware fixture を追加してから cluster / Script の契約を拡張する。
4. 意味のある単位で tests → diff review → commit → push → remote SHA 確認。`STATUS.md`, `NODE_COVERAGE.md`, generated inventory と PR 説明を実装・検証状態に合わせる。各 PR の古い STATUS を現状として丸ごと上書きしない。
5. 最終 gate は `cargo fmt --all -- --check`, `cargo check --all-targets --locked`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --all-targets --locked`。実際の GUI で named output 配線、second-port error、Undo/Redo、保存・再読み込み、選択形状の export を確認する。Windows gate は Linux 成功から推測しない。

この文書は次回作業の根拠と順序を保存するもので、multiport・診断・import・semantic diff を現在の Ω ソースへ統合済みと宣言するものではない。
