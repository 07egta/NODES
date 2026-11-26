# NODES 開発ロードマップ

更新: 2026-11-27 JST。実際の保存地点と直近の検証は [STATUS](docs/development/STATUS.md)、結果は [TEST_RESULTS](TEST_RESULTS.md) を参照。

## 現在地

Rust / egui の独立デスクトップ基盤へ、既存の core・Data Tree・数学・生成デザイン・メッシュ・UI・GHX/API・精密曲線・精密曲面ブランチを統合した。298種類をノード選択欄に登録し、Cluster は定義から作成する。登録数を完成率やGrasshopper互換率として扱わない。

`6349577` はLinux/Windowsそれぞれ350テストを通過。その後の2f94df4は通常Linux CIで433テスト・厳格Clippy・GUI/CLI release buildを通過した。実GUIの編集/保存/再起動/STL、Inspector配置とエラー入力の修正、実CLIの花器作成/編集/書出し、4入口のFIFO拒否を確認した。最新Windowsの結果は別途記録する。作業環境停止に備え、専用ブランチへ意味のある単位で保存する運用を継続する。

| 優先 | 開発項目 | 現状 | 完了条件 |
|---|---|---|---|
| P0 | 成果保全・統合 | 開発ブランチとDraft PR #11を作成、逐次保存、復旧ブランチを保全 | 最終SHAと実装・試験・再開手順をSTATUSに一致させる |
| P1 | 品質ゲート | Linux/Windows回帰基盤あり。全復旧コードの検証中 | fmt/check/clippy/test/releaseの同じコードに対する成功 |
| P2 | データフロー | 型検査、循環検出、キャッシュ、キャンセル、枝照合、List/Tree実装 | 複数出力ポート、Null、一般の混合アクセス、完全な空データ規則 |
| P3 | 精密幾何 | NURBS曲線/曲面、解析primitive、曲率、箱限定ソリッド | トリム曲面、一般B-rep位相、頑健な交差・Boolean、許容誤差の公開 |
| P4 | 実操作品質 | 日本語検索、Light/Dark、キャンバス、プレビュー、Undo/Redo | Windows/Intel Arc・高DPI・より小さい画面・大グラフ実操作（Linux 1116×758と1364px幅は確認） |
| P5 | 製造品質 | STL/OBJ/3MF、メッシュ診断、限定修復 | 殻包含、自己交差、肉厚の誤差評価、凹開口/穴/非平面の安全な扱い |
| P6 | 互換性 | GHXの限定読込と未対応archive、独立ノードAPI | 未知nativeノードの無損失保持、GH/GHX公開fixtures、STEP/IGES/3DM等 |
| P7 | 拡張 | 制限付きRhai、式、クラスター、オフラインJSON API/CLI | 版付きplugin ABI、型登録、複数出力・外部連携の能力境界 |
| P8 | 作例と性能 | 37作例の編集/再読込回帰、8作例STL往復、大規模tree/mesh/実I/O計測 | Windows対象機での起動/idle/RAM/frame、複雑な実形状・操作列（Linux単一idle標本は記録） |
| P9 | GH対応調査 | GH1 curated catalog、GH2公式全件indexの再生成器 | 各componentのport/default/数学/照合/版/参照/制限を埋め、独立oracleで検証 |

## 次の具体的な実装

1. 復旧済みコードのCIを通し、対応表・測定値・配布artifactの根拠を揃える。
2. v1 readerを維持した複数出力ポートのスキーマ設計と移行fixtureを先に作る。
3. `Value` のNull・Boolean・Vector・Domain等を導入し、既存Number/Point表現との明示変換を定める。
4. Item/List/Treeの混合アクセスを枝ごとに処理する共通schedulerを導入する。既存FirstItem制限を黙って変えない。
5. 未知のnativeノードを元JSON・port・接続ごと保存する無損失モデルを導入する。
6. 精密kernelの例外・退化・公差を整備し、一般B-repを検討する。メッシュ結合をBooleanと呼ばない。
7. WindowsノートPCでGUI/性能/印刷準備を検証し、実測から目標を定める。

## 互換性の完了基準

仕様出典があること、実装があること、テストがあること、Grasshopperの期待結果と一致することを分けて扱う。`VERIFIED` には対象バージョン・component ID・入力fixture・期待結果・公差・実行記録が必要。GH1とGH2を混同せず、名前一致だけで対応済みへ昇格しない。

一般ソリッドBoolean、STEP、永続SubD、完全GH互換、実機印刷成功は現時点で宣言しない。
