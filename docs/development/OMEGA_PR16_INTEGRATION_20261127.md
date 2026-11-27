# WIP: PR #16 と診断 PR #18 の統合候補 — 2026-11-27 JST

## 最新の固定入力と位置づけ

`wip/omega-pr16-integration-20261127` は隔離した提案ブランチ。
main、PR #16、PR #18への統合や、新しいPRの作成は行っていない。
本追記の対象は、初回候補 `771b4760bf983fb35afa65a2cbf495772172e6d5` に
PR #16 の `46ab578af63193c7dc3d37ce1926e1bc75216486` と
PR #18 の `ca6147a0b639b9429900313147d4e288972373e5` を加えた候補である。
履歴はこの3つの入力を親として保持する。PR #15 由来の変更も引き続き含む。

先行候補の **567件**と新候補の **572件**を区別する。
初回の実画面操作2→1→0とUndo/Redo、release 91.868秒は `771b476` の証拠。
その結果を、今回変わったビューポート／ファイル選択UIのネイティブ試験と読み替えない。
取り込んだPR16のCI・GPU以外のビューポート性能記録も、親ソースについての記録である。

## 更新によって発見した結合不整合

`git merge-tree --write-tree 771b4760bf983fb35afa65a2cbf495772172e6d5 46ab578af63193c7dc3d37ce1926e1bc75216486`
はテキスト競合なく tree `92b7c199cf532499a10cffc5b08e23586a90022a` を生成した。
しかし all-target check は **E0063、exit 101**で失敗した。
新しい `viewport_cache_tests.rs` の2つの `ComputeResult` 初期化に、PR18が導入した
`diagnostics` がなかった。これは自動マージの成功では分からない不整合だった。

fixtureに実際の `diagnostics::diagnose` の結果を設定し、実アプリと同じ評価・診断の
組を使うように修正した。古いrevisionの結果を投入するテストは、形状のArcだけでなく
現在の診断レポート全体が変わらないことも確認する。失敗した原ログと、その時点の
ソースハッシュを [更新時の証拠](pr16-refresh-validation/README.md) に残した。

同時に、PR18のWindows CIが実際に見つけた極小角度の問題を直した `ca6147` を移植。
元のbits=1 assertionは維持する。詳細は [移植性記録](WINDOWS_ANGLE_PORTABILITY.md)。
`math_nodes.rs` のPR18との差分はPR16の補間・合計／平均への委譲2箇所だけで、
`vector_angle_regressions.rs` はPR18とバイト一致する。独立担当が確認した。
PR16の集約・補間・Data Tree・scriptの4ファイルも固定入力46abと一致する。

## 新候補の実測した検証

Linux / Rust 1.100.0、独立したtarget、debug/test情報0、incremental無効、jobs2。
実装を凍結した候補treeは `af7316c51d7a78fd9abcc7841ee711b64d10f538`。
これは文書追記前のindex treeであり、最終の証拠付きcommit treeとは区別する。

| 対象 | 結果 |
|---|---|
| `cargo fmt --all -- --check` | 成功 |
| all-target `cargo check --locked --offline` | 成功 |
| all-target `cargo clippy --locked --offline -- -D warnings` | 成功 |
| all-target `cargo test --locked --offline` | **572 passed / 0 failed / 4 ignored、57スイート** |
| debug desktop / CLI | 成功 |
| release desktop / CLI | 成功、95.754秒。依存再利用のビルド時間で、起動性能ではない |
| 実release CLIの診断fixture3件 | 期待する終了コード・原因数・影響数が一致、入力SHA256不変 |
| 164件の実装・テスト・例・Cargoファイル | 検証前後のSHA256一致 |

ignoredは `benchmark_canvas_pan`、`benchmark_sparse_slider_edit`、`audit_stress`、
新しい `benchmark_pending_preview` の4つ。すべて手動性能／ストレス測定であり、
失敗した機能テストをignoreにしたものではない。親の成功件数を合計していない。

[コマンド、原ログ、バイナリ指紋、release CLI、ソース指紋](pr16-refresh-validation/README.md)
をこの提案と一緒に保存する。実行バイナリ本体はGitへ追加していない。
新候補のWindows CI、Windows GUIは未実施。限定したLinux実画面の追試は次節に記録する。

## 更新候補の実画面追試 — 10:05 JST

保存済み `4ad872028b3f3b8106d7a6ff1a8d8c406a57c446` のdebug実行ファイルを起動した。
初回候補の確認とは別に、同じ `two-roots.morph` で原因候補 **2→1→0**、
原因ノード#00／#01への移動、Bを0→50へ修正、Undoで1件へ戻ることとRedoで0件へ
戻ることを画面で確認。計算3／再利用2／無効1も表示された。

診断を閉じてCtrl+Oを押し、ダークテーマの作品選択画面で `numeric-details.morph` を
1回選択した。ファイル名、選択項目のアクセント色、「開く」の有効表示を確認。
キャンセル後も元の未保存プロジェクトと2つのB=50が保持された。
新しいファイルの読み込み確定や保存は行っていない。

各操作のスクリーンショットを目視確認し、画像ファイルはコミットしていない。
実行したdebugバイナリのSHA256、操作と未実施範囲は
[native-gui.json](pr16-refresh-validation/native-gui.json)に記録した。
担当プロセスだけを終了し、fixtureと164ソースのハッシュ不変を再確認した。
このscalar graphでは、形状のpending保持やピッキングのnative性能を確認したとは
主張しない。それらの境界は今回の自動テストと、親に限定した性能記録を区別する。

## PR18単独の修正後の再検証

別worktreeの **ca6147** でも実際に全体検証を実行し、
**487 passed / 0 failed / 2 ignored、44スイート**、debug/release成功を確認した。
releaseは93.308秒、実release CLIの3fixtureは期待する構造・終了コードで入力不変。
142ソースファイルを再照合した。直前に実行したfmt・strict all-target Clippy・
5件の局所角度テストの原ログは `windows-angle-portability/` に保存されている。
[単独PR18の追加原ログ](windows-angle-full-validation/README.md) は、進行中のCIを
文書だけのpushで中止しないため、この提案ブランチに保存する。

旧aa690のWindows失敗は [run45917517285](https://github.com/07egta/NODES/actions/runs/45917517285)。
修正ca6147の [run45919617590](https://github.com/07egta/NODES/actions/runs/45919617590) は
10:00 JST時点では進行中。ローカルLinux成功をWindows成功とは扱わない。

## 再開時に残る確認

1. 上記固定入力から候補を読み、両方の診断／ビューポート境界を維持する。
2. ca6147のCIの最終結果と、候補自身のWindows検証を区別する。
3. PR20と重複するdiagnoseのJSON／終了コード契約を、無条件マージの前に一本化する。
4. 形状を伴うpendingビューポートの実画面を確認し、親のベンチマークを候補の値に転記しない。
   ファイル選択の今回の確認範囲は選択とキャンセルに限る。
5. 次のPR16更新へ無制限に追従せず、まずこの固定候補を独立してレビューする。

---

# 初回候補771b476の履歴 — 以下は25f981bとの統合記録

このブランチは **統合候補の隔離検証用**。PR #16、PR #18、main に対するマージを
実行したものではなく、新しい競合PRも作成していない。
`wip/omega-pr16-integration-20261127` に再開可能な形で保存する。
初回検証時の PR #18 は `aa6906574ba3a89919d80a8a778d71ac65cc3724` だった。
後続のWindows修正と新しい候補は、本書冒頭の追記に分けて記録する。

## 比較・統合した入力

| 入力 | SHA | 範囲 |
|---|---|---|
| PR #18 | `aa6906574ba3a89919d80a8a778d71ac65cc3724` | 診断、行列／ベクトル精度、数値表示、協調キャンセルと証拠 |
| PR #16 | `25f981b794e3e0f037cd664915ce144347b49f40` | 数値・幾何・出力の修正、符号付きゼロ、ヘルプとガイド |
| #16 が継承する PR #15 | `a1ffa40071c0c3508b890065bd12220c5abb80d9` | 評価器、トポロジー、EditTracker、キャンバス性能など |

#16 の直接差分だけを取り込んだものではない。共通祖先からの変更には
`src/engine.rs`、`src/model.rs`、`src/canvas_perf.rs` も含む。
`git merge-tree --write-tree` の自動統合結果は
`557394840a70a94ae9146d87192445ff7c226908` で、文章の競合は
`docs/USER_GUIDE_JA.md` の1ファイルだった。共有ブランチを変更せず専用 worktree
へ展開して検証した。

## 発見して修正した結合時の不整合

自動統合後の `cargo check --all-targets` は **E0425、終了コード101**で失敗した。
#15/#16 側の EditTracker 導入が `let before = self.project.clone()` を削除した一方、
#18 の同一フレーム内で古い診断から移動することを防ぐ判定に `before.graph` が残った。
これは片側のPRだけでは発生せず、Gitの文字列上の競合にもならなかった。

隔離した候補では、その参照を `self.edits.previous.graph` に変更した。
`begin()` は pan・zoom を更新するが graph の比較基準は保持し、診断表示の後で
`finish()` が比較基準を更新する。したがってフレーム内の計算変更を検出する元の
意図を保てる。Open／Undo／Redo／Recovery は既存のrevision更新と履歴抑制を使う。
修正後の all-target check は成功した。独立担当がこの順序と Worker の公開境界を監査した。

## ガイドの競合解消

#16 の13章構成、新しいメニュー、ヘルプへの導線を維持した。第3章の操作一覧、
第4章の値・診断・処理時間、第11章のCLI、第13章の問題解決に #18 の説明を移した。
科学表記、詳しい値のホバー／コピー、ページ・検索、表示されない測定行を含む時間割合、
診断と評価時間の区別を保った。第1・2・5・6・7・8・9・10・12章は #16 原文と一致する。
別担当が実ソース・相対リンク・各上限・終了コードを読み取り照合した。

親ブランチでの520件と486件のテストは合算しない。ガイド末尾の親の検証記録、
取り込んだ `STATUS.md` の #16 履歴は、以下の統合候補の検証とは別のものとして残す。

## 保存形式と互換性の境界

`.morph` の schema 1、Project／NodeId／ポートの保存表現を保つ。
`.morphpart` は通常ファイルであることの検証を追加し、既存の容量制限・読み込み検証・
失敗時の保持を続ける。通常ファイルへのシンボリックリンクは禁止していない。

`diagnose` の JSON 配置と終了コードは維持する。ただし **CLI全体の出力不変ではない**。
取り込む `export-stl` には `non_manifold_vertices` と `duplicate_faces` が追加され、
`topologically_closed` の判定が厳しくなる。固定したフィールド一覧へ依存する外部利用を
確認してから統合する。OBJ座標は固定小数9桁から、有限のbinary64値を読み戻せる表記へ変わる。

Cargo manifest／lockfile、ライセンス、通常のアプリ実行依存の追加はない。
PythonのFractionによる期待値生成器2件は開発・CIで独立検証するためのもの。
アプリ・CLIの実行時には必要ない。用途を [言語方針](../LANGUAGE_POLICY.md)に記録する。

## この候補に対する実行結果

隔離 target に既存依存のビルド済みファイルをコピーした後、project package を
明示的に clean している。検証済みPR #18のtargetや、以前の測定バイナリは変更しない。
古いproject libraryを再利用して成功と判断することを避けるための手順である。

Linux / Rust 1.100.0、debug/test情報を抑えた同じローカル設定で実行した結果:

| 検査 | 実行結果 |
|---|---|
| 自動統合直後の all-target check | E0425で失敗。原ログ保存 |
| 上記1行修正後の all-target check | 成功。tool出力で確認、別の原ログは未保存 |
| `cargo fmt --all -- --check` | 成功 |
| `cargo clippy --locked --offline --all-targets -- -D warnings` | 成功 |
| `cargo test --locked --offline --all-targets` | **567 passed / 0 failed / 3 ignored、57スイート** |
| debug desktop / CLI | ビルド成功 |
| release desktop / CLI | ビルド成功、依存再利用のビルド時間91.868秒 |
| release CLIの既存診断fixture3件 | 期待する終了コード・原因数・影響数。元ファイルのハッシュ不変 |

ignoredは既存の手動性能・ストレス試験で、成功数に含めない。
この567件は統合候補を実際に走らせた件数であり、親の結果の合算ではない。
[コマンド・原ログ・ソースハッシュ](pr16-integration-validation/README.md)を保存した。
Windowsの候補ビルド、統合候補のCI実行や配布artifact公開は未実施。

### 統合候補の実画面で確認した操作

同じ候補のdebug実行ファイルをLinux/Xfceで起動し、保存済みの
`tests/fixtures/diagnostics/two-roots.morph` を開いた。診断の原因候補2件から
`#00 半径の計算`、続いて `#01 高さの計算` に移動し、それぞれのBをスライダーで
0から50へ変更した。原因数が **2 → 1 → 0**、全体の失敗数が **4 → 3 → 0**へ
更新し、最後は計算3・再利用2・無効1になった。

さらに実際の `Ctrl+Z` で2回目の変更を戻し、B=0と原因1件の復帰を確認。
`Ctrl+Y` でB=50と原因0件に戻った。これによりEditTracker、診断からの選択移動、
再計算・再利用、Undo/Redoの組み合わせを確認した。スクリーンショットは各操作時に
目視確認したが、この記録に画像ファイルとして収録していない。
保存操作はせず、この担当のプロセスだけを停止した。fixtureはソースハッシュと一致する。
Windows、IME、数値のキーボード入力、形状出力・実機印刷の試験とは区別する。

## 再開・統合する前の確認

1. 入力PRの最新headを取り直し、この固定された候補との違いを確認する。
2. PR #20 と重複する `diagnose` の公開契約は、親PR #18と同様に未解決。
   この候補には #20 を取り込んでいない。
3. Windowsビルド・GUI、実形状・印刷、外部CLI利用の確認を区別する。
4. 親のベンチマーク値を、評価器やキャンバスも変わった統合候補の性能と呼ばない。
5. main や他の開発ブランチへ上書きせず、この記録と固定した入力SHAを使って判断する。
