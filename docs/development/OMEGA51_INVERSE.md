# OMEGA 51 — 逆設計

開始: 2026-11-28 JST / main `c1213c885647c2e708017a63f6b7bdc8f9b66620`。

担当範囲: 依存追加なしの有界逆設計ソルバー、既存の数値入力・グラフ評価への接続、花器の例。PR #100（名前付きパラメータ）、#81（スケッチ拘束）、#114（体積精度）とは独立させる。保存形式・NodeKind・UIの共有構造を変更しない。候補は元のグラフと分離する。

SolidCraft の取り込みは docs/SOLIDCRAFT_CHANGES.md と PR #48 を参照。逆設計の対象はまず既存の NODES の形状生成・測定を再利用する。SolidCraft sketch は main に未導入。位相変更、離散選択は微分可能と扱わない。

検証: この環境に Rust toolchain はなく、直接 git clone も DNS エラー。GitHub connector で開発ブランチへ記録し、CIで確認する。main へはマージしない。

AI: ChatGPT。
