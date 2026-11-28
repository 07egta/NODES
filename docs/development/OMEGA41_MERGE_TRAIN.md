# OMEGA 41 — 統合列と検証証拠

## 着手記録 (2026-11-28 14:06 JST)

- 基準 main: `c1213c885647c2e708017a63f6b7bdc8f9b66620`
- main の Check run: [46110422520](https://github.com/07egta/NODES/actions/runs/46110422520) (調査時点では実行中)。直前の複数の main run は concurrency によって取消済み。古い success は現行 main の証拠ではない。
- 34 open PR (29 draft); 33 main 向け、#53 のみ #52 の head branch 向け。全34件の変更ファイル一覧を調査。
- GitHub の branch 情報では main protected=false、rulesets は空、自動マージは無効。設定変更・キューへの登録・main のマージは MISSION 21 の判断に委ねる。
- AGENTS.md / CONTRIBUTING.md / ROADMAP.md / ARCHITECTURE.md / development/STATUS.md と現行 Check / integration-recovery を確認。通常の PR CI は merge ref を checkout するが、実際に検証した tree・base/head の対応は BUILD-INFO の GITHUB_SHA だけでは不十分。既存 CI を拡張し、別の常駐管理システムは作らない。
- 本作業は既存 [#110](https://github.com/07egta/NODES/pull/110) (OMEGA31 の #102/#106/#103/#108/#104/#107 統合実験) や [#109](https://github.com/07egta/NODES/pull/109) (#81 の修復) の変更を上書きしない。#83 の windows.yml 差分は evaluator timing step 追加だけなので、その機能は保持して後から統合できる。

## 初期分類（提案、マージ許可ではない）

| 対象 | 根拠・確認事項 | 取扱い |
|---|---|---|
| #53 → #52 | #53 の base が feat/bio-math-patterns、#52 の head。同じ src/bio_patterns.rs | #53 の修正を #52 側に反映・再検証してから main 候補 |
| #81 / #109 | 61ファイル級の同一範囲（Cargo.toml/lock、sc-sketch、model、engine 等）。#109 は #81 の修復 | #109 を優先調査。原PRを二重投入しない。履歴と差分確認 |
| #102/#103/#104/#106/#107/#108 / #110 | #110 が6 headを統合済みと記録。#102/#104 は旧基準CI失敗、#106 は旧基準テスト失敗 | #110 の正確な候補SHAで再検証を待つ。個別と統合PRを二重投入しない |
| #88 / #93 | どちらも数学曲面。#93 は現時点の PR ファイル一覧では文書だけ、#88 は実装あり | main に取り込まれた範囲・ノードID・数学を比較、重複なら差分のみ保全 |
| #85/#87/#93/#94/#105 | 調査時点の PR ファイル一覧が文書のみ。#106 は文書+テストのみ | 既に main へ入った実装か差分欠落かを確認。名称で判定しない |
| #52/#55/#88/#89/#91/#98/#99/#101/#84/#109 | src/model.rs / src/engine.rs など共有基盤への編集が重なる | ファイル衝突は依存の証明ではない。基盤方針・ノード登録・評価契約をレビュー後に小さく統合 |
| #96/#91 | src/templates.rs が重なる | #96 の失敗報告修正を先に検証、#91 のテンプレート追加はその上で検証 |
| #57 / #65 | 旧 main ec3b840 で Check 成功（46107389399 / 46107363517） | 現行 main に対して再検証する候補。旧 success の再利用禁止 |
| #83 | 古い基準で実行中、workflow と app/persistence/viewport | このPRの CI 変更と調整。性能追加を削除しない |

初期の順番: 現行 main の品質確立 → OMEGA31/22 の既存実験の確認 → #57/#65 の現行基準検証 → 共有型/登録基盤の設計レビュー → 個別機能を一件ずつ。独立小変更は並行で検証可能だが、main 更新後は未マージ候補の証拠を失効させる。強制 push、無検証マージ、他ブランチの書換えは禁止。

## 実装予定

- 既存 Check に merge_group トリガーと exact checkout の記録・検証を追加する。
- 読み取り専用の PR 監査補助で、base/head、明示された積層依存、共有ファイル、同一変更範囲候補、CI の古さ、実行状況を一覧化する。自動マージは行わない。
- CI と candidate の証拠、失敗時の停止、復旧・再検証手順を明記する。
