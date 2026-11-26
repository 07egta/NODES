# NODES

**ノードをつないで、形を作る。** 日本語を中心にした、独立型デスクトップのパラメトリックCADです。Rhino・Grasshopperのインストールやログインは不要で、基本のモデリング、保存、書き出しはオフラインで利用できます。

現在の統合開発は [develop/nodes-ultimate](https://github.com/07egta/NODES/tree/develop/nodes-ultimate) と [Draft PR #11](https://github.com/07egta/NODES/pull/11) で進めています。**開発版であり、Grasshopperの完全互換版ではありません。** 最新の実行結果、既知の制限、再開手順は [TEST_RESULTS.md](TEST_RESULTS.md) と [開発チェックポイント](docs/development/STATUS.md) を参照してください。

[使い方](docs/USER_GUIDE_JA.md) · [ノード対応表](NODE_COVERAGE.md) · [設計](docs/ARCHITECTURE.md) · [ロードマップ](ROADMAP.md) · [CI](https://github.com/07egta/NODES/actions)

## 入手する

- **開発版:** 成功した `Check` のWindowsジョブから、コミットSHA付きの `NODES-windows-x64-…` アーティファクトを取得できます。CI成果物はGUI・実機造形を保証する安定リリースではありません。
- **旧公開版:** [Morphlet 0.1.0 Windows ZIP](https://github.com/07egta/NODES/releases/download/v0.1.0/Morphlet-0.1.0-Windows-x64.zip)。この旧版には現在の開発ブランチで追加した機能は含まれません。
- **ソースから:** 下記のビルド手順を使います。実行ファイル名は互換性のため `Morphlet.exe` を維持しています。

## 最初のモデルを編集する

1. ZIPを展開し、`Morphlet.exe` を起動します。
2. 「ねじれの花器」の花器ノードを選び、高さ・半径・凹凸を変更します。ねじりノードでねじり角を調整できます。
3. 左の検索で「円」「点を並べたい」「ねじり」や `circle`、`array`、`twist` を探し、必要なノードを追加します。
4. 保存操作から編集可能な `.morph` グラフを残します。配布サンプルとは別名で保存すると元の作例を維持できます。
5. メッシュをSTLで書き出し、外部スライサーで寸法・向き・造形条件を確認します。

3D画面はドラッグで回転、ホイールでズームします。全体表示は `F`、操作を戻すには `Ctrl+Z`、やり直すには `Ctrl+Y`。ノードごとの表示切り替え、値・データツリーの確認、エラー表示、辞典も利用できます。詳細は [日本語操作ガイド](docs/USER_GUIDE_JA.md) を参照してください。

![旧公開版0.1.0で撮影したWindowsの花器画面](docs/windows-vase.png)

上の画像は旧公開版の記録です。今回のLinux実操作では、花器の3D表示、数値変更、Undo/Redo、保存を確認しました。数値入力のフォーカス不具合を発見して修正しましたが、修正後のGUI再検証とWindowsでの最新画面検証は残っています。

## 現在の実装範囲

選択欄に **298種類**、定義から作るClusterを含めて **299種類の登録**があります。登録数は完全実装数・Grasshopper互換性の検証数ではありません。機能ごとの範囲は [ノード対応表](NODE_COVERAGE.md) に記載しています。

| 分野 | 実装した機能 | 主な制限 |
|---|---|---|
| グラフ評価 | 型付き入力、循環検出、決定的な順序、増分再計算、キャッシュ、キャンセル、古い結果の破棄、ノード別時間 | 単一出力モデル。計算ワーカーは1本で、ノードの並列実行は未対応 |
| Data Tree | Path・枝、Flatten/Graft/Simplify、リスト演算、Path Mapper制限構文、Shortest/Longest/Cross | native Null、一般の混合アクセス、GH全件の期待値比較が未完了 |
| 数学・ベクトル | 四則、三角/指数/対数、統計、集合、補間、行列、点と座標変換 | Boolean・Vector・Domainは専用の値型へ移行予定 |
| 曲線 | 解析曲線、Bezier/B-spline/NURBS、評価、導関数、長さ、分割、変換 | 数学的な曲線と表示点列を分離。一般交差・オフセット・フィレットの対応は限定 |
| 曲面・ソリッド | NURBS・解析曲面、押し出し、ルールド面、回転面、Iso Curve、法線、曲率 | B-rep演算は箱などの限定ケース。一般のトリム・Boolean・Fillet・Shellは未完成 |
| Mesh / SubD | 構築、面積・体積、境界/非多様体診断、溶接、平滑化、細分化、限定修復 | 永続SubD/Crease、一般メッシュBoolean、完全な自己交差・肉厚判定は未完成 |
| 生成デザイン | グリッド、2D Voronoi/Delaunay/Convex Hull、配置、場、ノイズ、変形 | 3D Voronoi、一般Morph/Flow/最適化は未完成 |
| 編集・表示 | 日本語/英語検索、最近使ったノード、Undo/Redo、複数選択、整列、グループ、注釈、ツリー表示、ライト/ダーク | 大規模画面のフレーム時間、各DPI、Intel Arc、最新Windows GUIの計測は未実施 |
| ファイル | native JSON、STL/OBJ、試験的3MF、限定GHX解析/変換 | GH binary、STEP/IGES/3DM/glTFは未対応。未知nativeノードは読み込み拒否で元ファイルを保護 |
| 拡張・自動化 | 制限付きRhai、数式、Cluster、安全なJSONグラフAPI | 外部プラグインABI、GH Script互換、一般MCPサーバーは未完成 |

精密なNURBSデータ、限定B-rep、近似メッシュは別の型として扱います。「メッシュをまとめる」はBoolean Unionではありません。STL出力とメッシュ診断は、実機での造形成功を保証するものではありません。G-codeの花瓶モードは実機未検証の補助機能です。

## 編集できる作例

`examples/` に **37種類のグラフ**があります。花器、ハニカム、Voronoi、ラティス、建築外装、らせん階段、曲線・曲面、NURBS、曲率比較などを含みます。各グラフのパラメータ変更、保存・再読み込み、再評価を確認する統合テストを追加しています。実際に成功したテストの範囲は [TEST_RESULTS.md](TEST_RESULTS.md) を参照してください。

数値を変えると影響先だけを再計算します。10,000ノードの検証では、中間ノードの編集で下流5,000ノードだけを再計算する契約を確認します。ヘッドレス計算の測定と、GUIの体感速度・メモリ使用量は別に評価しています。

## ソースからビルドする

- **Rust 1.100.0** をCIで使用します。`Cargo.toml` の宣言下限は1.88ですが、固定された依存関係全体の1.88でのビルドは未確認です。
- Windows MSVCではVisual Studio C++ Build ToolsとWindows SDKが必要です。[Rust公式の手順](https://rust-lang.github.io/rustup/installation/windows.html)
- Linuxのデスクトップ用開発パッケージは [CI workflow](.github/workflows/windows.yml) を参照してください。
- 初回の依存取得にはネット接続が必要な場合があります。実行時の基本機能にクラウドサービスは必要ありません。

```powershell
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked --bins
```

Windowsの生成先は `target\release\Morphlet.exe` です。`--locked` は同梱の `Cargo.lock` を維持します。ビルドを簡略化する [scripts/build-windows.ps1](scripts/build-windows.ps1) もあります。実行したコマンドと環境は [検証結果](TEST_RESULTS.md) に記録しています。

## オフラインCLI

独立したコンソール版 nodes-cli は、JSONの入出力で catalog・validate・evaluate・build・apply・export-stl を実行します。Windowsでは nodes-cli.exe をPowerShellから使えます。GUIとは別の実行ファイルなので標準出力を取得できます。

スクリプト・外部ファイル読み込み・プラグインを実行しない制限付きGraph APIを使用します。変更したグラフは評価に成功してから保存し、作品名や表示設定を保持します。STLは明示的なOutputだけを選択し、空・無効・非メッシュの出力を黙って省略しません。使い方とJSONの契約は [CLI.md](docs/CLI.md) を参照してください。

## ドキュメントとライセンス

- [開発を再開する](docs/development/STATUS.md)
- [実装構成](docs/ARCHITECTURE.md) / [ロードマップ](ROADMAP.md)
- [対応・検証の数え方](NODE_COVERAGE.md) / [公式資料の調査記録](docs/compatibility/REFERENCE_NOTES.md)
- [日本語操作ガイド](docs/USER_GUIDE_JA.md) / [保存・出力の仕様](docs/IO_NOTES.md)
- [依存関係の監査範囲と未確認事項](docs/DEPENDENCIES.md)
- [第三者ライセンス・出典](docs/THIRD_PARTY_NOTICES.md)

本アプリのソースはMITまたはApache-2.0の選択式ライセンスです。フォントと依存ライブラリにはそれぞれのライセンスが適用されます。新しい依存関係を追加する際は、ライセンスと再配布条件を確認してください。NODESは公開資料と数学を根拠にした独立実装であり、Rhino/Grasshopperの本体や非公開SDKを同梱しません。
