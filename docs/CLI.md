# NODES オフライン CLI

`nodes-cli` は Windows のコンソール、PowerShell、バッチ、外部ツールから使う専用実行ファイルです。GUI の `NODES.exe` と独立しており、標準出力と終了コードを使用できます。基本操作に Rhino、Grasshopper、ネットワーク接続は必要ありません。

## ビルドと起動

```sh
cargo build --release --locked --bin nodes-cli
cargo run --locked --bin nodes-cli -- --help
```

Windows では `target/release/nodes-cli.exe`、Linux/macOS では `target/release/nodes-cli` を実行します。Cargo のバイナリ自動検出を使用し、依存ライブラリは追加していません。既存 GUI のコマンドライン引数は変更しません。

## コマンド

| コマンド | 動作 |
|---|---|
| `nodes-cli catalog` | 制限付き Graph API が許可するノードとポートの一覧を JSON で返す |
| `nodes-cli validate PROJECT.morph` | 保存形式、グラフ構造、型、数値設定、許可する機能を検査する。ノード評価は行わない |
| `nodes-cli evaluate PROJECT.morph` | 安全性を検査して全ノードを評価し、件数・小さなプレビュー・計算時間を返す |
| `nodes-cli diagnose PROJECT.morph` | 全ノードを一度評価し、エラーの原因候補・影響先・実測した処理時間を返す |
| `nodes-cli build REQUEST.json OUTPUT.morph` | API v1 の編集要求から新しいグラフを作り、全ノードの評価成功後に保存する |
| `nodes-cli apply PROJECT.morph REQUEST.json OUTPUT.morph` | 既存作品に編集要求を適用し、評価成功後に保存する |
| `nodes-cli export-stl PROJECT.morph OUTPUT.stl` | 明示的な Output ノードのメッシュを、ミリメートル単位のバイナリ STL として保存する |

`--help`、`-h`、`help` または引数なしではヘルプを表示します。それ以外のコマンドでは成功・失敗とも標準出力に JSON 文書を 1 件返します。パスに空白がある場合はシェル側で引用してください。日本語のファイル名にも対応するため、パスは OS ネイティブ文字列のまま扱います。

## JSON 応答と終了コード

```json
{
  "cli_version": 1,
  "api_version": 1,
  "command": "validate",
  "ok": true,
  "result": {
    "project": {"name": "API graph", "schema_version": 1, "node_count": 3},
    "evaluation_performed": false
  }
}
```

終了コードは成功が `0`、作品・評価・入出力の失敗が `1`、不明なコマンドや引数の数の誤りが `2` です。失敗応答には `error.code`、`error.message`、`error.nodes` を含めます。評価エラーでは `error.nodes` に `node_id` と原因を記録します。ファイルを読めない場合など、ノードを特定できないエラーでは配列は空です。

`evaluate`、`build`、`apply` の `result.evaluation.nodes` はノード ID、種類、枝数、項目数、値の種類別件数を含みます。各ノードのプレビューは先頭 8 項目までです。メッシュは頂点数・三角形数、曲線は表示点数と数学曲線データの有無だけを返し、全頂点や全三角形は標準出力へ展開しません。数値と点のプレビューは実値を返します。`preview_truncated` は省略の有無を示します。

各 `evaluate` 呼び出しは新しい評価器を使います。`elapsed_ms` は当該プロセスの実測値であり、GUI フレーム時間や Windows 全環境での性能を表すものではありません。

## エラーの原因候補を調べる

```sh
nodes-cli diagnose examples/01.morph
```

成功すると `result.diagnostics` に診断レポートを返します。評価エラーがある場合は終了コード `1` と `error.code: "evaluation_failed"` を維持し、`error.diagnostics` に同じ形式のレポートを添えます。`evaluate`、`build`、`apply`、`export-stl` の評価エラーにも診断を添えます。保存形式の破損や許可されない機能など、評価開始前の失敗は従来のエラー形式で返し、評価したかのようなレポートは作りません。`diagnose` は作品を保存・変更しません。

レポートの `schema_version` は `1` です。

| フィールド | 内容 |
|---|---|
| `summary` | 成功・失敗・原因候補・下流エラー・無効・未評価の件数、完了状態、計算・キャッシュ利用の件数 |
| `root_failures` | 失敗した入力元を持たないエラーノード。元のメッセージと、影響を受けた失敗ノードの ID・件数 |
| `blocked_nodes` | 上流にも失敗があるノード。元のメッセージ、たどり着く原因候補の ID・件数 |
| `disabled_nodes` / `unevaluated_nodes` | 無効化されたノードと、キャンセルなどで結果のないノード |
| `graph_error` | グラフ全体の構造エラー。通常の CLI 読込では評価前の検証で拒否される |
| `hotspots` | 実測時間の長い順のノード。キャッシュ使用・途中中断の区別を含む |

`causality: "dependency_inference"` は、原因候補を**接続と評価結果から推定**したことを示します。下流に別の問題がある可能性は残るため、原因候補を修正して再評価してください。エンジンのエラー文を解析して断定する仕組みではありません。

`summary.complete` は評価が最後まで終わったことを表し、全ノードの成功を意味しません。失敗の有無は `failed_count` と `graph_error` も確認してください。無効ノード、未評価ノード、実際に空の値を正常出力したノードを混同しません。

大量のエラーでも出力を抑えるため、各ノードの原因 ID は最大 64 個、各原因候補の影響先 ID は最大 256 個です。総数は `upstream_root_count` / `affected_count`、省略の有無は `upstream_roots_truncated` / `affected_nodes_truncated` で確認できます。形状の頂点配列やデータツリー全体は診断に含めません。

長い評価メッセージは UTF-8 の文字境界を保って最大 8,192 バイトに抑え、`message_truncated` で省略の有無を示します。通常のエラー文はそのまま保持します。

各診断コマンドは新しい評価器を使うので、通常 `cached_count` は `0` です。`hotspots[].elapsed_ms` は入力準備・キャッシュ処理・形状計算などを含むノード処理の経過時間、`share_percent` は対象ノードの計測時間合計に対する比率です。CPU 使用率、GUI のフレーム時間、最適化後の予測時間としては扱わないでください。

## 編集例

リポジトリの `examples/interop/ai-add.json` は `7 + 5` の編集可能なグラフです。

```sh
nodes-cli build examples/interop/ai-add.json addition.morph
nodes-cli evaluate addition.morph
```

次の内容を `change.json` に保存します。既存ノードは保存ファイルの ID を十進文字列で指定します。新規ノードの alias はその編集要求内で使用します。

```json
{
  "api_version": 1,
  "edits": [
    {"op": "set", "node": "0", "input": "value", "value": 11}
  ]
}
```

```sh
nodes-cli apply addition.morph change.json edited.morph
nodes-cli validate edited.morph
nodes-cli evaluate edited.morph
```

加算の結果は `16` になります。`apply` は作品名、キャンバスのパン・ズーム、既存ノードの名前・位置・プレビュー設定などを保持します。出力先と入力作品を同じパスにして保存することもできます。編集要求や評価に失敗した場合は保存を開始しません。

## ねじれた花器を作り、半径を変更する

`examples/interop/ai-vase.json` は `examples/01.morph` と同じ形状パラメータで、`vase → twist → output` の 3 ノードを作成します。基準半径は 32 mm、高さは 104 mm、半径方向の肉厚は 2.4 mm、ねじり角は 110 度です。周方向 72、縦方向 40 の分割数と波の設定も要求に明記しています。接続には `geometry` 入力を使い、花器の Mesh を Twist の Geometry 入力へ、変形後の Mesh を Output の DisplayGeometry 入力へ渡します。

リポジトリのルートで次のコマンドを実行します。`evaluate` と `export-stl` は、`build` が保存した作品をそれぞれ読み直します。

```sh
nodes-cli build examples/interop/ai-vase.json vase.morph
nodes-cli evaluate vase.morph
nodes-cli export-stl vase.morph vase.stl
```

この要求では花器が ID `0`、ねじりが ID `1`、出力が ID `2` です。基準半径を 36 mm に変更するには、次の内容を `vase-radius-36.json` に保存します。`vase` などの alias は最初の要求内だけで使うため、保存済み作品の編集には ID の文字列を指定します。

```json
{
  "api_version": 1,
  "edits": [
    {"op": "set", "node": "0", "input": "radius", "value": 36}
  ]
}
```

```sh
nodes-cli apply vase.morph vase-radius-36.json vase-36.morph
nodes-cli evaluate vase-36.morph
nodes-cli export-stl vase-36.morph vase-36.stl
```

この分割数から期待される出力は、半径変更の前後とも 5,906 頂点・11,808 三角形の Mesh 1 個で、高さは 104 mm です。三角形数から算出したバイナリ STL の期待サイズは `84 + 50 × 11,808 = 590,484` バイトです。実行時の JSON 応答と保存ファイルをこれらの期待値に照合してください。幅と体積は形状の評価結果で確認します。

## STL の選択規則

STL は少なくとも 1 個の明示的な `output` ノードを必要とします。すべての Output ノードが有効で、少なくとも 1 個の空でない Mesh を持ち、全項目が Mesh である必要があります。無効な Output、空の出力、曲線、点、数値、数学曲面、B-rep は保存前にエラーにします。曲面や立体はグラフ内で明示的にメッシュ化してください。

Output に指定されていない途中の形状は書き出しません。複数の Output を指定すると、その各メッシュをまとめて保存します。同じ形状を複数の Output に接続した場合も、その指定どおり複数回含まれます。全グラフの評価エラーが 1 件でもあれば出力しません。

```sh
nodes-cli export-stl printable.morph model.stl
```

応答には選択した Output の ID、三角形数、寸法の境界、符号付き体積、開いた辺や非多様体の診断を含めます。STL 書き出し自体では、自動修復、座標移動、単位変更、曲面の自動分割は行いません。STL の f32 座標への変換で面が潰れる場合などは保存前に拒否します。閉じたメッシュであることだけでは実機造形の成功を保証できないため、`printability_verified` は常に `false` です。出力先を入力作品と同じ実ファイルにする指定は拒否します。

## 入力と保存の制限

CLI は `graph_api::load_safe`、`build`、`apply`、`evaluate` を再利用します。任意スクリプト、Expression、Path Mapper の式、Import Mesh、クラスター、外部プラグインは実行しません。許可するノードは `catalog` で確認できます。

編集要求は最大 1 MiB、作品は最大 8 MiB です。Graph API と評価器のノード数、項目数、計算量の制限も適用されます。出力先の親フォルダーは事前に作成してください。作品と STL は既存の永続化処理で、出力先と同じフォルダーの一時ファイルを検証・書き込み・同期してから置換します。検査や書き込みが失敗した場合は既存の保存先を保持します。成功時は指定された保存先を置換します。

## 検証

```sh
cargo test --locked --test cli_workflows
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
```

`tests/cli_workflows.rs` は `CARGO_BIN_EXE_nodes-cli` を使って実際の実行ファイルを起動し、JSON と終了コード、作成→編集→保存→再読込→再評価、作品情報の保持、禁止機能、要求サイズ上限、失敗時の既存保存先の保持、STL の書き出しと再取込を検証します。検証済みソース `c44a897a22072c6694d1dbf2233ae9557955f49e` の [Check 45857217255](https://github.com/07egta/NODES/actions/runs/45857217255) で、LinuxはUnix FIFO拒否回帰を含む8件、Windowsは共通の7件が成功しました。全体ではLinux437件・Windows436件が成功し、両OSのGUI/CLI release buildも通過しています。source/tree/artifactの記録は [CI_NUMERIC_VALIDATION.json](development/CI_NUMERIC_VALIDATION.json) を参照してください。

validate/evaluate、GUIバイナリのbuild-graph/inspect-ghx入口がFIFOを即座に拒否し、保存先を保持することは2f94df4の配布バイナリでも実測しています。詳細は [FIFO_VALIDATION.json](development/FIFO_VALIDATION.json)。

リポジトリに含まれる実用サンプルはPythonの標準ライブラリだけで再検証できます。出力ディレクトリは新規または空にしてください。

```sh
python3 -B scripts/validate_cli_vase.py \
  --cli target/release/nodes-cli \
  --request examples/interop/ai-vase.json \
  --output-dir /tmp/nodes-vase-validation
```

この花器ワークフローの実行記録は2f94df4のLinux配布用CLIによるものです。8コマンドと半径以外の保存内容の保持、STLの各面・寸法・独立体積計算を確認し、`cli-vase-report.json` に実行したバイナリと入力のハッシュ、公差、結果を記録します。[CLI_VASE_VALIDATION.json](development/CLI_VASE_VALIDATION.json) に保存した証拠を参照できます。c44a897aのLinux artifact内のCLIも、実測したCLIとSHA-256 `752f8abac0b18bc5930108b961701bbeda2ae598c545e32b8f71bbeff99a4434` が一致しています。WindowsでもPythonから利用できますが、このワークフロー実測はLinuxです。実機造形の保証には使いません。

Cargo の一次資料: [バイナリ自動検出](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#target-auto-discovery)、[実行ファイルを使う統合テスト](https://doc.rust-lang.org/cargo/commands/cargo-test.html)。
