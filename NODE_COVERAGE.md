# ノード対応・検証の読み方

更新: 2026-11-27 JST。NODESの登録、ローカル機能試験、Grasshopper互換性を分けて記録する。

## 数え方

| 指標 | 現時点の根拠 |
|---|---|
| ノード選択欄の登録 | 実行したGUIで298種類を確認 |
| Cluster | 選択欄に含めず、定義から生成する1種類 |
| 完全実装済みノード数 | 未集計。登録には限定・近似の演算を含むため、298を完全実装数としない |
| 機能検証 | c44a897aの通常CIでLinux437件・Windows436件、各0 failed / 2 ignored / 35 suites。両OSのGUI/CLI release build、Linux fmt/check/strict Clippyも成功。差1件はUnix限定FIFO回帰。4件の数値UI回帰は両OSで成功。個別ノードの完全検証数とは異なる |
| GH期待値でVERIFIED | 0。Rhino/GHから採取した版付きoracle fixtureとの比較は未実施 |
| GH2公式index | 取得時1419件。build 9.3.26329.6001 / export 2026-11-25T15:23:50Z。詳細未調査はUNKNOWN |

### 機械可読ファイル

- [gh_catalog.json](docs/compatibility/gh_catalog.json): GH1/GH2を区別した調査対象のcurated catalog。標準全件の分母には使わない。
- [REFERENCE_NOTES.md](docs/compatibility/REFERENCE_NOTES.md): 公式資料、枝照合、Simplify、版の相違など。
- `docs/compatibility/nodes_inventory.json`: compiled registryから再生成する出力先。ID、分類、入力/出力型、既定値、範囲、アクセス規則、metadata fingerprintを記録する。生成器は [coverage_inventory.rs](examples/coverage_inventory.rs)。
- `docs/compatibility/gh2_official_inventory.json`: 公式indexの全件出力先。生成器は [update_gh2_inventory.py](scripts/update_gh2_inventory.py)。入力portや数学的意味を名称から推測しない。
- [TEST_RESULTS.md](TEST_RESULTS.md): 実際のコマンドと成功/失敗/未実施を記録する。
- [CI_NUMERIC_VALIDATION.json](docs/development/CI_NUMERIC_VALIDATION.json): c44a897aの通常CIとsource/tree/artifactの記録。後続の証拠文書コミットを検証対象ソースと混同しない。
- [NUMERIC_INPUT_VALIDATION.json](docs/development/NUMERIC_INPUT_VALIDATION.json): 実GUIで発見した数値の0.1ずれ、修正後の既定値・直接入力・保存再起動・STLの検証記録。

生成器の出力にある `NOT_INFERRED` は、登録metadataから実装・互換性の合格を導いていないことを表す。GH catalog の状態とは別の欄である。

## 状態

| 状態 | 意味 |
|---|---|
| VERIFIED | 指定版の独立期待値・公差を用いた比較が記録されている |
| IMPLEMENTED | 実装は存在するが、要求された検証を十分に終えていない |
| PARTIAL | 入力、位相、出力などに具体的な未対応部分がある |
| APPROXIMATE | 折れ線、メッシュ、標本化、数値解等による近似を含む |
| MISSING | 実装がない |
| BLOCKED | 型、複数出力、位相等の前提機能が不足 |
| UNKNOWN | 仕様または対応関係の調査が不足 |

## 実装範囲と主な制限

| 分野 | 現在の機能 | 主要な不足 |
|---|---|---|
| Data Tree / Lists | Path/枝、Flatten/Graft/Simplify、結合、Path Mapper制限構文、転置、リスト操作、Shortest/Longest/Cross | native Null、複数出力、一般混合アクセス、GH全件oracle |
| 数学・ベクトル | 演算、三角/指数/対数、統計、集合、行列、Domainの数値表現、補間、座標系 | 専用Boolean/Vector/Domain型、全成分の極値/誤差証拠 |
| 曲線 | 数学的NURBS/Bezier/B-splineと表示点列を分離、解析curve、評価・導関数・長さ等 | 一般の曲線交差、fillet/offset等の全入力での頑健性 |
| 曲面・ソリッド | NURBS/解析曲面、extrude/ruled/revolve/iso、法線・曲率、箱限定のB-rep演算 | 一般トリム/Boolean/fillet/shell/STEP |
| Mesh / SubD | 生成、診断、面積/体積、法線、溶接、平滑化、細分化、限定修復 | 永続SubD/crease、一般mesh Boolean、完全な交差/肉厚/包含判定 |
| 生成デザイン | グリッド、2D Voronoi/Delaunay/hull、配置、attractor、noise/field、変形 | 3D Voronoi、一般morph/flow/最適化の標準全件 |
| ファイル / API | native JSON、STL/OBJ/3MF、限定GHX archive/変換、安全なJSON編集API | GH binary、広いGHX、STEP/IGES/3DM/glTF、未知nativeノードの編集保持 |
| 表示 / Script | 日本語UI、値/ツリー表示、プレビュー、制限付きRhai、式、クラスター | GH Display/Script完全互換、plugin ABI、OS/GPU/DPI網羅検証 |

## 参照

- [McNeel GH1 algorithms and data structures](https://developer.rhino3d.com/guides/grasshopper/gh-algorithms-and-data-structures/)
- [McNeel GH2 documentation](https://www.rhino3d.com/docs/grasshopper2/)
- [GH2 official component index](https://www.rhino3d.com/docs/grasshopper2/specs/)
- [McNeel NURBS geometry overview](https://developer.rhino3d.com/guides/opennurbs/nurbs-geometry-overview/)

GH2 indexはcomponentとparameterを含む。1419をGH1の標準ノード数やNODESとの直接の互換率の分母へ転用しない。

次の対応基盤は [複数出力・未知nativeノード・Nullの移行計画](docs/development/MULTI_OUTPUT_MIGRATION.md)。現在は設計段階であり、ここに挙げた機能を実装済みへ数えない。
