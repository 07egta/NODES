# NODES 開発ロードマップ

更新: 2026-11-27 JST。実際の保存地点と直近の検証は [STATUS](docs/development/STATUS.md)、結果は [TEST_RESULTS](TEST_RESULTS.md) を参照。

## 現在地

Rust / egui の独立デスクトップ基盤へ、既存の core・Data Tree・数学・生成デザイン・メッシュ・UI・GHX/API・精密曲線・精密曲面ブランチを統合した。298種類をノード選択欄に登録し、Cluster は定義から作成する。登録数を完成率やGrasshopper互換率として扱わない。

現在の作業は [PR #16](https://github.com/07egta/NODES/pull/16)、`feat/nodes-omega-geometry`。`0aa286a` のLinux520件・Windows518件と両release配布が成功し、UIを追加した `25f981b` はローカル523件・0 failed・3 ignored、fmt/strict Clippy/releaseが成功した。実GUIでキャンバス直接入力と符号付きゼロ、Undo/Redo/保存、頂点接触の診断、2部品STL出力を確認。正確なsource/CI/artifact/未完了事項は [OMEGA_CI_RESULTS.json](docs/development/OMEGA_CI_RESULTS.json)、[OMEGA_NATIVE_GUI.json](docs/development/OMEGA_NATIVE_GUI.json)、[STATUS](docs/development/STATUS.md) に記録する。

新しい問題を再調査し、複数出力の既存PR統合、消費portのエラー伝播、未知nativeノード保持、差分APIの符号付きゼロ、描画cache、配布runtime等の11項目を [OMEGA_NEXT_TASKS.md](docs/development/OMEGA_NEXT_TASKS.md) に優先順位付きで残した。別PRを名称だけで取り込まず、競合と実際のCI失敗を解消する。

| 優先 | 開発項目 | 現状 | 完了条件 |
|---|---|---|---|
| P0 | 成果保全・統合 | PR #16へ意味のある単位で保存、旧PR・復旧ブランチを保全 | 最終SHAと実装・試験・再開手順をSTATUSに一致させる |
| P1 | 品質ゲート | 0aaのLinux520件・Windows518件と両release成功。UI25fのローカル523件とstrict gates成功 | 次の変更も同じソースのCI・実操作を通し、証拠を保存する |
| P2 | データフロー | 型検査、循環検出、キャッシュ、キャンセル、枝照合、List/Tree実装 | 複数出力ポート、Null、一般の混合アクセス、完全な空データ規則 |
| P3 | 精密幾何 | NURBS曲線/曲面、解析primitive、曲率、箱限定ソリッド | トリム曲面、一般B-rep位相、頑健な交差・Boolean、許容誤差の公開 |
| P4 | 実操作品質 | 日本語検索、Light/Dark、キャンバス、プレビュー、Undo/Redo、精度を保持する数値入力をLinux実GUIで確認 | Windows/Intel Arc・高DPI・より小さい画面・大グラフ実操作（Linux 1116×758と1364px幅は確認） |
| P5 | 製造品質 | STL/OBJ/3MF、メッシュ診断、限定修復 | 殻包含、自己交差、肉厚の誤差評価、凹開口/穴/非平面の安全な扱い |
| P6 | 互換性 | GHXの限定読込と未対応archive、独立ノードAPI | 未知nativeノードの無損失保持、GH/GHX公開fixtures、STEP/IGES/3DM等 |
| P7 | 拡張 | 制限付きRhai、式、クラスター、オフラインJSON API/CLI | 版付きplugin ABI、型登録、複数出力・外部連携の能力境界 |
| P8 | 作例と性能 | 37作例の編集/再読込回帰、8作例STL往復、大規模tree/mesh/実I/O計測 | Windows対象機での起動/idle/RAM/frame、複雑な実形状・操作列（Linux単一idle標本は記録） |
| P9 | GH対応調査 | GH1 curated catalog、GH2公式全件indexの再生成器 | 各componentのport/default/数学/照合/版/参照/制限を埋め、独立oracleで検証 |

## 次の具体的な実装

1. [MULTI_OUTPUT_MIGRATION.md](docs/development/MULTI_OUTPUT_MIGRATION.md) に従い、既存PR #14/#17のv1 reader・出力endpoint・v2保存envelopeを監査して安全に統合する。既存37作例・旧port alias・Clusterの値と接続を比較する移行fixtureを先に作る。
2. 評価・キャッシュ・Canvas・Cluster・Graph APIを同じ出力keyへ移行し、Dispatch A/Bを別の下流へ接続する最初の実用グラフで、保存再読込・Undo・キャンセル・容量上限を検証する。
3. 未知nativeノードの元JSON・port・接続を保持し、実行しないplaceholderを導入する。文書保存の可否と評価の可否を分離する。
4. `Value::Null` を追加し、未接続・枝なし・空枝・Null項目・評価エラーを区別する。Boolean/Vector/Domainの専用型はNullの一貫した評価・表示・保存を確認してから別単位で進める。
5. Item/List/Treeの混合アクセスを枝ごとに処理する共通schedulerを導入する。既存FirstItem制限を黙って変えない。
6. 精密kernelの例外・退化・公差を整備し、一般B-repを検討する。メッシュ結合をBooleanと呼ばない。
7. WindowsノートPCでGUI/性能/印刷準備を検証し、実測から目標を定める。各実装単位でCI・実操作の結果を保存し、検証済みソースSHAを更新する。

## 互換性の完了基準

仕様出典があること、実装があること、テストがあること、Grasshopperの期待結果と一致することを分けて扱う。`VERIFIED` には対象バージョン・component ID・入力fixture・期待結果・公差・実行記録が必要。GH1とGH2を混同せず、名前一致だけで対応済みへ昇格しない。

一般ソリッドBoolean、STEP、永続SubD、完全GH互換、実機印刷成功は現時点で宣言しない。
