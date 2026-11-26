# NODES 検証記録

更新: 2026-11-27 JST。成功は実行したコードに限定する。作業環境停止後に復旧・再実装したコードは、追加CIの結果で確定する。

## 最新の確定結果

最新ソース **`c44a897a22072c6694d1dbf2233ae9557955f49e`** の [Check 45857217255](https://github.com/07egta/NODES/actions/runs/45857217255) は Linux・Windows ともに成功した。

| 環境・job | Rust 全ターゲット | その他の成功した検証 |
|---|---|---|
| Linux `140038666343` | **437 passed / 0 failed / 2 ignored、35 suites** | fmt / check / strict Clippy (`-D warnings`)、Python 17件（GH2 3 + exporter 6 + dependency audit 8）、依存監査、`cargo build --release --locked --bins`、artifact upload |
| Windows `140041049976` | **436 passed / 0 failed / 2 ignored、35 suites** | `cargo build --release --locked --bins`、artifact upload |

新規 `numeric_editor_tests` 4件は両OSで成功し、上表の合計に含まれる。LinuxとWindowsの差1件はUnix専用FIFO回帰である。PRで実際に検証した merge revision `92e70be85c4b34311a27036a18d7a97f37f879c1` とsource headは、tree `2285ea77c81898e6c43924bb09ab749355a8b612` が一致することを確認した。Linux artifactは `15137335621`、Windows artifactは `15135989980`。根拠は [CI_NUMERIC_VALIDATION.json](docs/development/CI_NUMERIC_VALIDATION.json) に保存した。

このLinux配布バイナリでは、数値入力の精度保持、新規Sphere作成、保存・終了・再起動・再編集・STL出力まで実操作で確認した。操作列とメッシュの独立測定は後述する。Windows GUI、Intel Arc、実機造形の検証は引き続き未完了である。

## 初期の確定チェックポイント

| コード | 環境 | 実行 | 結果 |
|---|---|---|---|
| `6349577399dd484a90fde5afe77ee173f8f9444a` | Linux x86_64 / Rust 1.100 | `cargo test --all-targets --locked` | 350 passed、0 failed、2 ignored、24 suites |
| 同じSHA | GitHub Actions Windows / Rust 1.100.0 | 同じコマンド | 350 passed、0 failed、2 ignored、24 suites |
| 同じSHA | 同じWindows runner | GUI release build | 成功 |

Windowsの根拠: [run 45822273755](https://github.com/07egta/NODES/actions/runs/45822273755)、job 139917295799 の Test / Build Morphlet。ログ上のtest buildは4m11s、release buildは1m42s。これは起動時間やユーザーPCの性能ではない。

2件のignoredは `benchmark_sparse_slider_edit`（手動release計測）と `audit_stress`（10万頂点・50万三角形のopt-inストレス検証）。失敗した試験を削除して成功にしたものではない。

## 復旧後のCIで確認したこと

[diagnostics run 45836582370](https://github.com/07egta/NODES/actions/runs/45836582370) は、0e1032fc の作業ツリーに formatter / Clippy 修正を適用した上で実行した。

| 対象 | 結果 |
|---|---|
| Rust 全ターゲット | **424 passed / 0 failed / 2 opt-in ignored、33 suites** |
| コンパイル、整形確認 | 成功 |
| 厳格な Clippy | 失敗。型の複雑さ、比較、初期化等を別途修正 |
| ノード登録情報の生成 | 299件（picker 298 + Cluster） |
| GH2公式index生成 | build 9.3.26329.6001、1,419件 |
| releaseヘッドレス評価計測 | 成功。数値は下記および performance.json |

その同一Rustソースを 2080af67 として保全した。369個のログ断片、52ファイルのSHA-256・UTF-8バイト数、元のGit blob IDを照合してから保存している。詳細は [CI_RECOVERY_VALIDATION.json](docs/development/CI_RECOVERY_VALIDATION.json)。

この424件には、37作例の編集・保存再読込・再評価、STL往復、曲率境界、メッシュ修復、多角形分割、ノード時間、10,000ノードの回帰を含む。ノード単位の完全検証数ではない。

厳格なlint修正、コンソールCLI、検索の追加表現、拡張benchmarkと監査scriptを b26d246 に統合した。その後の [diagnostics run 45846736773](https://github.com/07egta/NODES/actions/runs/45846736773)、job 140002253199 で、整形後の候補が次の結果になった。

| 対象 | 結果 |
|---|---|
| Rust 全ターゲット | **431 passed / 0 failed / 2 opt-in ignored、35 suites** |
| check / fmt / strict Clippy (`-D warnings`) | すべて成功 |
| 実プロセスのCLI試験 | 7件成功。作成・編集・保存再読込・厳格なSTL出力・不正入力・日本語パス |
| 登録metadata / GH2生成 | 成功、299登録 / 1,419公式index項目 |
| 依存metadata監査 | 成功、303件のlicense宣言を取得。配布全体の法的承認を意味しない |
| release計測 | スカラー評価、Data Tree、メッシュ、実ファイル保存再読込が成功 |

候補の299個のログ断片、9ファイルのSHA-256・UTF-8バイト数と元のblobを照合した。Rustの変更はformatterによる整形のみ。詳細は [CI_INTEGRATION_VALIDATION.json](docs/development/CI_INTEGRATION_VALIDATION.json)。**この成功はCIの整形済み作業ツリーに対するもの**として、下記のコミット済みソースの通常Checkと区別する。

復帰したローカル環境では、PythonのGH2 parser 3件、checkpoint exporter 6件、依存監査8件、合計 **17件が成功**。Rustは未インストールのため、最新Rust検証はCIへ委ねている。

### コミット済みソースの通常CI

`c19dca185380391ad1c875e412529e22cae05ba6` の [Check 45849521685](https://github.com/07egta/NODES/actions/runs/45849521685)、Linux job `140011951495` は **431 passed / 0 failed / 2 ignored、35 suites**、fmt/check/strict Clippy、Python17件、`cargo build --release --locked --bins`、Linux artifact upload に成功した。PRの実際のcheckout `ca49e6c016c40f680117bac4dff8babe2dc2664a` のtreeがc19dca1と一致することも確認した。

このrunのWindows jobは次のソース更新で中止されたため、Windows成功には数えない。artifact 15132917546 のZIP SHA-256、展開したGUI/CLIのハッシュは [CI_COMMITTED_VALIDATION.json](docs/development/CI_COMMITTED_VALIDATION.json) に保存した。

実CLIでwriterのないFIFOを渡すと2秒のtimeoutで停止を再現した。`eefc9c1` に事前通常ファイル検査と回帰を保存したが、その [Check 45851162593](https://github.com/07egta/NODES/actions/runs/45851162593) はGHXの1箇所のfmt差分で停止した。差分をそのとおり修正し、GUIで発見した行間修正とともに2f94df4へ保存した。失敗したgateを成功へ読み替えていない。

前回ソース `2f94df459533774f83593835ba1e22d4924a3c42` の [Check 45852634502](https://github.com/07egta/NODES/actions/runs/45852634502)、Linux job `140022721574` は **433 passed / 0 failed / 2 ignored、35 suites**。fmt/check/strict Clippy、Python17件、両バイナリのrelease build、artifact uploadも成功。実checkout `19717e70917718af1374e387720b29b619c1c1ed` のtreeとsource headのtree一致をAPIで確認した。Windows job `140024871303` も **432 passed / 0 failed / 2 ignored、35 suites**、両バイナリrelease build・artifact uploadに成功した。Linuxとの差1件はUnix専用FIFO回帰である。

配布用Linux ZIP artifact `15134610988` のSHA-256と展開バイナリを照合した上で、FIFO拒否を4入口で実行した。`nodes-cli validate` / `evaluate`、`Morphlet --build-graph` / `--inspect-ghx` は全てexit 1で通常ファイルを要求し、各1.3〜2.3msで終了、既存出力のbytesを保持した。修正前の2秒timeout、各コマンド・時刻・バイナリの根拠は [FIFO_VALIDATION.json](docs/development/FIFO_VALIDATION.json)。敵対的なファイル差し替え競合の完全防止を主張しない。

### 実CLIの花器ワークフロー

上記2f94df4の配布用CLIで再実行し、`scripts/validate_cli_vase.py` で作成→validate/evaluate→STL→半径変更→validate/evaluate→STLの **8コマンドが成功**。半径以外の全projectフィールドの保持、元ファイルのbytes不変、半径32→36を検査した。

STLをPythonの独立したparserで読み、両方とも5,906個の一意な頂点座標、11,808三角形、590,484 bytes、高さ104 mm、全座標・法線の有限性、全三角形の非ゼロ面積を確認。f32座標から行列式で求めた体積は52,430.418306→59,967.823772 mm³となり、CLI報告との差はそれぞれ0.000751846 / 0.000629314 mm³。相対公差1e-5・絶対公差1e-3 mm³に収まった。生データは [CLI_VASE_VALIDATION.json](docs/development/CLI_VASE_VALIDATION.json)。自己交差や実機造形の独立検証を意味しない。

c44a897のLinux artifactに含まれる `nodes-cli` は、この検証に使った2f94df4版とbyte単位で一致する。SHA-256は `752f8abac0b18bc5930108b961701bbeda2ae598c545e32b8f71bbeff99a4434`。花器ワークフローの記録は元の実行日時・source revisionを保持し、最新artifactとの対応は同一バイナリのハッシュを根拠にする。

## その後のローカル検証

- NodeTiming、10000ノード、曲面曲率の関連3 suite: **16 passed / 0 failed**。
- 空List、SingleItem、API alias重複の回帰: 修正前 **1 passed / 4 failed**、共通評価/APIを修正後に成功。
- Access/API tree/fabrication/sample/mesh repair/STL exportの6 suite: **42 passed / 0 failed / 1 ignored**。
- 独立した平面多角形三角形化module: `rustc --test` で **5 passed / 0 failed**。統合Cargo試験はその時点では未実施。
- 37作例の編集・保存再読込・再編集、および8作例のSTL往復: 停止前は未実行。その後、上記424件のCIで実行に成功。

これらのローカルコミットには `997183b`、`299f510`、`95a070e`、`cf0b6af`、`4d40820`、`7f59add` 等が含まれた。環境停止で一部のexact sourceが参照不能となり、GitHub復旧ブランチに保存した。復旧文書に原文回復と再実装の差を明記している。

## 10000ノードの実測

純数値の直列グラフ、Linuxのoptimized-debug test profileでの一回の実測:

| 操作 | 時間 | 検証した件数 |
|---|---:|---|
| 初回評価 | 48.370 ms | 10000 computed |
| 全件キャッシュ再利用 | 23.725 ms | 10000 cached |
| 中間ノード変更 | 33.476 ms | 下流5000 computed、上流5000 cached |

シリアライズ結果は1,287,793 bytes。構築時間を評価時間へ混ぜず、最終値と再読込の同値性も検査した。GUI frame、Windows/Intel Arc、実形状、RAM、同時負荷の性能保証へ一般化しない。release測定は `examples/coverage_perf.rs` の再実行結果を別途記録する。

## release ヘッドレス評価の実測

Linux x86_64 GitHub Actions runner、Rust 1.100.0、b26d246から整形した上記431件成功のCI候補で測定。`cargo run --release --locked --example coverage_perf`、`debug_assertions=false`。50回のslider更新は先頭入力を変更し、全下流を再計算する。前節のoptimized-debug・中間編集や別CI実行とは測定条件が異なるため、速度改善率を比較しない。

| ノード数 | Cold | Warm | Slider p50 | Slider p95 |
|---:|---:|---:|---:|---:|
| 100 | 0.211 ms | 0.120 ms | 0.143 ms | 0.163 ms |
| 1,000 | 2.218 ms | 1.270 ms | 1.841 ms | 1.884 ms |
| 10,000 | 16.363 ms | 12.604 ms | 23.632 ms | 24.151 ms |
| 16,384 | 41.851 ms | 27.206 ms | 43.826 ms | 47.470 ms |

[performance.json](docs/development/performance.json) に生データを保存。GUI・起動・RAM・Windows/Intel Arcの計測ではない。前回の2080af67候補では10,000ノードのcold 24.616 ms、warm 16.926 msを観測したが、別実行の参考値である。

### 大規模データと保存の実測

追加workloadは各操作1回。fixture構築と独立期待値の検査を操作時間から分けている。

| 対象 | 規模・独立検査 | 時間 |
|---|---|---:|
| Data Tree Flatten | 12,288 item、128×96枝データと空枝8本。全値とpathを照合 | 0.065 ms |
| Data Tree Graft | 12,296枝、空枝を保持 | 2.466 ms |
| Data Tree Transpose | 和75,491,328と各項目・pathを照合 | 0.755 ms |
| Mesh診断 | 16,641頂点 / 32,768三角形、開境界512本 | 11.698 ms |
| 中点細分化 | 66,049頂点 / 131,072三角形へ。面積16,384 mm²・重心(64,64,0)を照合 | 16.687 ms |
| Native保存 | 2,000ノード / 1,999接続、589,908 bytes。検証・一時ファイル・flush/sync・renameを含む | 3.531 ms |
| Native読込 | 同じproject全体の同値性と独立算術期待値を照合 | 2.193 ms |
| 変更後の上書き / 再読込 | パラメータ・metadata変更と出力2016.25→1995.5を照合 | 3.276 / 2.317 ms |

読み込みは書き込み直後でOS cacheを除去していない。ディスクの一般的なコールド読込性能を表さない。全workload所要は4,153.770 msだった。

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

### 復旧後の実GUI検証

ハッシュを照合したc19dca1のLinux artifactを、1364×1024のネイティブデスクトップと専用設定ディレクトリで実行した。

- **32→3（肉厚エラー）→36**を、途中で入力欄を再クリックせず入力し、フォーカス保持と形状復帰を確認。
- キーボードUndoで36→3→32、Redoで32→3→36を確認。テキスト入力の1編集単位への履歴統合は未対応。
- GUIで `.morph` 保存、ライトテーマへ変更、STL出力完了表示を確認。保存した作品は3ノード・半径36、STLは11,808三角形・590,484 bytesだった。
- アプリを閉じて保存済み作品を指定して再起動し、半径36、接続、ライトテーマの保持を確認。
- 再起動後に半径スライダーを36→67へ動かし、Undoで36へ戻して再保存した。

実画面で新たに、Inspectorの最初の入力まで約100pxの余白と、過大な行高を確認した。明示IDのscope内を通常のhorizontal行にする修正を追加した。2f94df4の配布バイナリで、**余白の解消、一定の行間、36→3（エラー）→36の継続入力、全7項目へのスクロール到達を確認した**。1116×758のclient windowで全体表示を使い3ノードを収め、1364×1000へ戻してダークテーマも確認した。

修正前の[実画面](docs/development/screenshots/linux-before-inspector-layout.png)、修正後の[実画面](docs/development/screenshots/linux-after-inspector-layout.png)、GUIで保存・再読込した[作品](docs/development/gui-vase-36.morph)を保存した。PNGは起動直後のclient領域1364×920をアプリから保存したもので、後続の最大化・縮小確認時の寸法とは区別する。出典・観測・ファイルハッシュは [GUI_VALIDATION.json](docs/development/GUI_VALIDATION.json)。Windows GUI、広いDPI・さらに小さい解像度、実機造形は未検証。

### Linux GUIの限定アイドル測定

同じ2f94df4バイナリで3ノード・11,808三角形の花器を表示し、1364×1024仮想デスクトップ上で静止した12.000秒を測った。`/proc/<PID>/stat` のutime+stimeは **0 tick増加**（100Hz、1 tickはこの区間の約0.083% coreに相当）。「CPUを全く使わない」という保証にはしない。`VmRSS` は開始・終了とも **103,888 KiB（101.45 MiB）**、起動から操作後までの `VmHWM` は109,388 KiB（106.82 MiB）。別processのgraphics serverやGPUメモリは含まない。

GUIはフォーカスを保持し、測定コマンドだけを別のbound terminalへ送った。単一の短い標本であり、起動時間・frame時間・Intel Arc・Windows性能へ一般化しない。生データと手法は [GUI_IDLE_VALIDATION.json](docs/development/GUI_IDLE_VALIDATION.json)。

### 新規グラフで発見した数値入力の回帰

2f94df4で空の作品を作り、日本語「円を作りたい」が円1件に絞られること、英語sphere検索、ノード追加・移動、Outputの自動接続、明示切断、手動ワイヤー接続、接続のUndo/Redo、保存を確認した。

一方、Sphereの登録既定値25が表示だけで25.1になり、半径へ30と入力して保存すると30.1になった。保存済みfixtureと操作列は [NUMERIC_INPUT_VALIDATION.json](docs/development/NUMERIC_INPUT_VALIDATION.json)。原因は便宜スライダーの最小値0.1を起点とするstep0.5の丸めと、表示時にもsetterが呼ばれる設定である。入力精度を3桁に限定する経路も併せて修正対象とした。

共通の数値widgetで読み取りを無変更にし、手入力とスライダーの便宜範囲を分離し、本来のmetadata上限・有限値・整数制約を編集時に適用する修正を追加した。全登録既定値の受動描画、30 / 30.123456 / 500の実キーイベント、上下限・NaN・Infinity・整数、編集せずfocusを外した際の精度保持をheadless eguiで検査する4件は、**c44a897のLinux・Windows CIで全件成功**した。

521742bの [Check 45856676411](https://github.com/07egta/NODES/actions/runs/45856676411) はassert! 2箇所のfmt差分で停止し、後続gateは未実行だった。その実diffだけを適用し、試験の意味を変更せずに進めたc44a897のCheckで、冒頭の437 / 436件の成功結果を得た。以前の失敗や433 / 432件の結果を、新しいソースの成功として読み替えていない。

### 数値入力修正後の実GUI・保存・STL検証

ハッシュを照合したc44a897のLinux配布GUIで、新規Sphereの次の操作を確認した。

- 登録既定値 **25** が表示後も25のままで、寸法は50×50×50 mm。
- **30** と入力すると30を保持し、寸法は60×60×60 mm。
- **30.123456** は入力後にfocusを外しても6桁の小数部を保持。
- **500** は便宜スライダーの表示範囲を超えてもmetadataの許容範囲内の値として保持され、寸法は1000×1000×1000 mm。
- Undoで **500→30.123456→30** を正確に復元。
- SphereをOutputへ接続し、GUIから保存してアプリを終了。再起動すると2ノード、接続、半径30、寸法60×60×60 mmとダークテーマを保持。
- 再起動後に31へ編集して寸法62×62×62 mmを確認し、30へ戻して再保存。scale 1、bed Z=0、XY中央配置でバイナリSTLの書き出しが成功。

修正前に保存された半径30.1の作品は、読み込んでも30.1のまま保持した。過去の保存値を推測で変更していない。修正後の編集可能な[作品](docs/development/gui-sphere-exact.morph)と、再起動後の[実画面](docs/development/screenshots/linux-exact-radius-reopened.png)を保存した。PNGはclient領域1364×920である。

保存されたJSONとSTLのbytesをPython標準ライブラリで独立に読み、次を確認した。幾何の測定にNODESのカーネルは使用せず、STLのf32頂点から面積と行列式を計算し、体積は `math.fsum` で集計した。

| 項目 | 独立測定結果 |
|---|---:|
| 保存済み半径 / ノード数 | **30 / 2**（Sphere → Output） |
| 一意な頂点座標 / 三角形数 | **1,106 / 2,208** |
| STLサイズ | **110,484 bytes**（84 + 50×2,208） |
| bounds | **[-30, -30, 0] ～ [30, 30, 60] mm** |
| 座標・法線の有限性 / 退化面 | 全件有限 / **0** |
| STLメッシュの符号付き体積 | **112,292.23168992388 mm³** |
| 半径30の解析球の体積（4πr³/3） | **113,097.33552923256 mm³** |
| 解析球に対する体積の相対差 | **0.0071186808737910634（約0.712%）** |

このSphere出力は三角形で近似したメッシュであり、解析球より約0.712%小さい体積となる。厳密球・B-repの体積検証や実機造形成功の証明には用いない。STLのSHA-256は `9a177aeac2a0c49200f0c9fcd3d58a03ca7be170429611f9463c7f647e377031`。バイナリSTLはGitへ追加せず、生成グラフとファイルハッシュ・件数・寸法・操作列を [NUMERIC_INPUT_VALIDATION.json](docs/development/NUMERIC_INPUT_VALIDATION.json) に記録した。

## 次の必須ゲート

```sh
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked --bins
```

c44a897についてLinuxで上記、Windowsで全ターゲットtestと両バイナリのrelease buildが成功した。数値入力修正のCIとLinux配布GUIの確認は完了している。以後のsource変更でも同じgateを再実行する。fmt/clippyの未解消警告、未実行項目を成功へ読み替えない。GH1/GH2 oracle、Windows GUI、Intel Arc、3Dプリンター実機は未検証。
