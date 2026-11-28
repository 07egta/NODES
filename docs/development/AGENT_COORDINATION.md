# OMEGA 開発調整プロトコル v1

この手順は並行開発の調整に使う。GitHubのIssue、Draft PR、ブランチ、
コミット、CIを記録の原本とし、中央の手編集一覧は作らない。
共有MarkdownやPR本文は排他的ロックではない。mainへの最終統合は
MISSION 21が担当する。ほかのMISSIONは独自にmainへマージしない。

## 着手、競合確認、引き継ぎ

1. 最新mainのSHA、AGENTS.md、ROADMAP、開発中PR（Draftを含む）、
   Issue、関連コミットを確認する。過去のSTATUSやREADMEは履歴として扱い、
   現在のGitHub状態やCI結果を上書きしない。
2. 未担当の課題を選ぶ。優先度（P0=データ消失・ビルド不能等、P1=重大な
   正しさ・統合、P2=通常改善、P3=探索）、依存関係、変更規模と検証可能性を
   比較する。Issueが開いているだけでは未実装・未担当の証明にならない。
   既存PRの実差分・履歴・関連Issueを確認する。
3. 最新mainのSHAから固有のブランチを作り、小さな実作業または個別の
   引き継ぎ記録を保存してDraft PRを作る。PR本文の以下のJSONブロックで担当を宣言。
   同じIssue/MISSION/対象を他のPRが扱っていたら、先に調整する。
4. 宣言直後、範囲拡張前、レビュー前にOpen PRと実差分を再取得する。
   同時宣言の競合では原則として先に作られた（番号が小さい）PRを優先する。
   既存の作業が明確な場合や統合PRは依存関係と合意を優先する。
   後発側は重複範囲を外すか依存を明記して停止・引き継ぐ。
   他PRを勝手に閉じたり、編集・force-push・マージしたりしない。
5. 作業途中も小さくコミットし、PR本文の自分の宣言・検証SHA・進捗・
   未完了部分・次の手順を更新する。終了時は中断であっても保存する。
   後任は既存のPR・最新コミット・実際のCIを読み、調整して再開する。
6. CI成功だけではマージ許可ではない。MISSION 21は最新mainとの統合差分、
   必須チェック、対象SHA、依存関係、回帰テストを確認する。
   CIの別SHAの成功、古い記録、未実行チェックを成功として扱わない。

## PR本文の機械可読宣言

以下は例。実際の担当内容に置き換える。1 PRにつき1ブロック。
本文更新時は他者の記録を消さず、自分のブロックだけを更新する。
```nodes-coordination
{
  "version": 1,
  "mission": "OMEGA 49",
  "task": "multi-agent-coordination",
  "base_sha": "0000000000000000000000000000000000000000",
  "priority": "P1",
  "state": "active",
  "issues": [],
  "depends_on": [],
  "scope": ["scripts/omega_coordination.py", "docs/development/AGENT_COORDINATION.md"],
  "checked_at": "2026-11-28T05:07:00Z",
  "verified_sha": null,
  "handoff": "次に行う作業と未検証事項"
}
```

- mission/task: 作業識別子。taskは具体的な作業単位。同一MISSIONでも分割は可能。
- base_sha: 作業基準にしたmainの完全なSHA。PRのbase refの現在値とは別。
- state: active / blocked / review / handoff。Open PRの範囲はstateによらず尊重する。
- issues / depends_on: 関連Issue番号 / 先に統合が必要なPR番号。番号は正の整数。
- scope: リポジトリ相対のファイル名、または末尾 / のディレクトリ。
  ワイルドカードは使わない。実差分が宣言外なら範囲更新・競合確認を行う。
- checked_at: 最後に本人が確認したUTC時刻。24時間を超えたら要確認。
  時間経過だけで作業権を奪わない。
- verified_sha: 必須検証を記録したコミットのSHA。未検証はnull。
- handoff: 進捗、未完了、再現方法、次に行うこと。詳細はPRの通常本文に残す。
  PR番号、専用ブランチ、最新head、実差分、CIはGitHubから取得し二重管理しない。

## 自動監査（読み取り専用）

開発補助スクリプト `scripts/omega_coordination.py` を参照。
GitHub CLI (`gh`) を使う。常駐サービス、アプリ実行時依存は追加しない。
開発補助にPython標準ライブラリを使う理由は、既存の補助ツールと同じ環境で
JSON/GitHub APIを扱え、RustアプリのCargo依存や配布物を変えないため。
監査結果はスナップショットであり、ロック・マージ権限ではない。
古い宣言、欠落、取得失敗、未確認のCIを明示し、候補を「安全」と断定しない。
