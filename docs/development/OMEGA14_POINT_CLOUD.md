# OMEGA 14 — 点群処理

2026-11-28 JST。main `ec3b840e81c76aa7e426da5dc47b716ba975a98e` を基準に着手。対象は点群前処理と距離評価。既存 Point の枝リストを使い、共有 Value 型・保存形式の変更を避ける。新規依存なし。PLY 出力 #84、性能 #83、NURBS #81 と重複しない範囲に限定する。SolidCraft #48 と既存 kernel_bridge は確認済み。厳密曲面への近似は別段階で扱う。

検証: 着手時点では未実施。ローカル環境に Rust toolchain および GitHub への直接ネットワーク接続がなく、CI で検証する必要がある。
