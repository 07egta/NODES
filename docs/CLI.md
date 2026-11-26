# NODES オフライン CLI

`nodes-cli` は Windows のコンソール、PowerShell、バッチ、外部ツールから使う専用実行ファイルです。GUI の `Morphlet.exe` と独立しており、標準出力と終了コードを使用できます。基本操作に Rhino、Grasshopper、ネットワーク接続は必要ありません。

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

`tests/cli_workflows.rs` は `CARGO_BIN_EXE_nodes-cli` を使って実際の実行ファイルを起動し、JSON と終了コード、作成→編集→保存→再読込→再評価、作品情報の保持、禁止機能、要求サイズ上限、失敗時の既存保存先の保持、STL の書き出しと再取込を検証します。初回作成時はローカル実行環境が利用できず、実行結果は CI で確認する必要があります。

Cargo の一次資料: [バイナリ自動検出](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#target-auto-discovery)、[実行ファイルを使う統合テスト](https://doc.rust-lang.org/cargo/commands/cargo-test.html)。
