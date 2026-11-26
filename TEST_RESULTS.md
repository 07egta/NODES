# NODES 検証記録

更新: 2026-11-26。成功は実行したコードに限定する。作業環境停止後に復旧・再実装したコードは、追加CIの結果で確定する。

## 確定した全体チェックポイント

| コード | 環境 | 実行 | 結果 |
|---|---|---|---|
| `6349577399dd484a90fde5afe77ee173f8f9444a` | Linux x86_64 / Rust 1.100 | `cargo test --all-targets --locked` | 350 passed、0 failed、2 ignored、24 suites |
| 同じSHA | GitHub Actions Windows / Rust 1.100.0 | 同じコマンド | 350 passed、0 failed、2 ignored、24 suites |
| 同じSHA | 同じWindows runner | `cargo build --release --locked --bin Morphlet` | 成功 |

Windowsの根拠: [run 45822273755](https://github.com/07egta/NODES/actions/runs/45822273755)、job 139917295799 の Test / Build Morphlet。ログ上のtest buildは4m11s、release buildは1m42s。これは起動時間やユーザーPCの性能ではない。

2件のignoredはopt-inの性能/ストレス検証。失敗した試験を削除して成功にしたものではない。

## その後のローカル検証

- NodeTiming、10000ノード、曲面曲率の関連3 suite: **16 passed / 0 failed**。
- 空List、SingleItem、API alias重複の回帰: 修正前 **1 passed / 4 failed**、共通評価/APIを修正後に成功。
- Access/API tree/fabrication/sample/mesh repair/STL exportの6 suite: **42 passed / 0 failed / 1 ignored**。
- 独立した平面多角形三角形化module: `rustc --test` で **5 passed / 0 failed**。統合Cargo試験はその時点では未実施。
- 37作例の編集・保存再読込・再編集、および8作例のSTL往復: テストコードを作成・保全。停止前は未実行。

これらのローカルコミットには `997183b`、`299f510`、`95a070e`、`cf0b6af`、`4d40820`、`7f59add` 等が含まれた。環境停止で一部のexact sourceが参照不能となり、GitHub復旧ブランチに保存した。復旧文書に原文回復と再実装の差を明記している。

## 10000ノードの実測

純数値の直列グラフ、Linuxのoptimized-debug test profileでの一回の実測:

| 操作 | 時間 | 検証した件数 |
|---|---:|---|
| 初回評価 | 48.370 ms | 10000 computed |
| 全件キャッシュ再利用 | 23.725 ms | 10000 cached |
| 中間ノード変更 | 33.476 ms | 下流5000 computed、上流5000 cached |

シリアライズ結果は1,287,793 bytes。構築時間を評価時間へ混ぜず、最終値と再読込の同値性も検査した。GUI frame、Windows/Intel Arc、実形状、RAM、同時負荷の性能保証へ一般化しない。release測定は `examples/coverage_perf.rs` の再実行結果を別途記録する。

## GUIで実際に確認したこと

Linuxネイティブ版を起動し、1364px幅のデスクトップ上で確認した。

- 日本語のメニュー、検索欄、298件のノードライブラリ、3ノードの花器グラフ。
- ねじれた花器の面、グリッド、寸法・体積・質量概算の表示。
- キャンバスの半径スライダー変更、再評価、Ctrl+Z / Ctrl+Yで形状が戻ること。
- パラメータエラーの赤いノードと日本語メッセージ、正常値への復帰。
- ファイル選択画面から `gui-validation-vase.morph` を保存し、保存完了表示とファイル存在を確認。
- 保存後、アプリケーションを閉じた。

### 発見したUI不具合

半径32を36へ入力する途中で3になるとエラーメッセージが追加され、暗黙のwidget IDが変化して入力フォーカスが失われた。エラーが消えたときには別のパラメータ欄へフォーカスが移ることも確認した。ノードID/port keyで一意なwidget IDを与える修正はビルド済み。

**修正後の実操作、保存ファイルのGUI再オープン、GUIでのSTL出力、Light/Darkの両テーマ、DPI・低解像度の検査は未完了。** 実行環境停止後はネイティブ操作が利用不能となった。モックアップを実画面と扱っていない。

## 次の必須ゲート

```sh
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked --bin Morphlet
```

復旧後の最新SHAについて上記をCIで実行し、結果を追記する。fmt/clippyの未解消警告、未実行項目を成功へ読み替えない。GH1/GH2 oracle、Windows GUI、3Dプリンター実機は未検証。
