# OMEGA 01 陰関数フィールド

2026-11-28 JST。基点 main `ec3b840e81c76aa7e426da5dc47b716ba975a98e`。既存 Gyroid の marching tetrahedra を確認。公開中の PR #55 はメッシュ形状補間・パネル、#83 はプレビュー性能であり、今回は別機能。

担当範囲：解析距離場と周期スカラー場を分離した Rust カーネル、既存メッシュ抽出の共通化、新規ノードと編集可能な作例、回帰テスト。永続 Value 型や GPU、B-rep の変更は別 PR に分ける。

状態：着手。ローカル環境に Rust/Cargo がなく GitHub への直接 clone も不可。CI で検証するまで未確認。ChatGPT による実装。
