# MISSION 06 — 精密曲面・B-rep 第一段階

## 状況

基点: `evolve/all` の `df77c542473a5c637f3353dec48e367dd67db027`。
この変更は独立実装です。Rhino / Grasshopper のバイナリやSDKは使いません。
**コンパイル・Rustテスト・Windows GUI操作はまだ実行していません。** ローカル実行環境にRustがなく、GitHubへの直接git接続も不可。PRの既存Windows CIで検証が必要です。

## 設計

- `surface_kernel.rs`: クランプされたテンソル積有理B-spline。制御点と正の重み、次数、節点ベクトルから同次座標で計算し、一階偏微分は商の微分法を使用。正規化UVを公開し、元の節点区間もAPIで取得可能。内部節点では右側微分、終端では左側微分。上限: 各方向32制御点、次数8、重み1e-6〜1e6。
- 平面、円筒・円錐台、球、トーラスは解析式で評価。円錐台は正の上下半径に限定。球・トーラスが閉曲面であっても、B-repとしては扱わない。
- `NurbsSurface::vase`: 円周方向の4つの有理二次四分円と、高さ方向の二次Bezier半径断面。中央半径は制御値。正の半径、開いた上下端。
- `NurbsSurface::ruled`: 2本の同じ点数・開閉状態の点列曲線を線形補間。開2〜32点、閉3〜31点、閉曲線は開始点を一致させる必要あり。Bの方向反転指定のみで自動継ぎ目整合はしない。一般の滑らかなロフトやNURBS曲線入力ではない。
- `ClosedSolid`: 箱に限定。頂点8・共有稜線12・外向き面6・シェル。UV矩形トリムと有向辺参照を持つ。各辺の2面使用・逆向き、面支持曲面と頂点一致、外向き法線をチェック。一般B-repの妥当性証明ではない。
- `Value::Surface` と `Value::Solid` は別型。表示用メッシュはワーカー上で生成してArcで保持し、表示時に数学的データを捨てない。出力は明示的メッシュ化ノードを経由して既存STL/OBJ等へ。旧メッシュノード・Curve点列表現はそのまま。
- 新しいノード種別は既存列挙の末尾に追加し、既存serde名とschema_version=1を維持。旧アプリは新規ノードを読めない。新アプリで旧ファイルを読む回帰試験は既存CIの対象。データツリーは既存マッチングを利用する。新しい専用型の一般変形・Rhai入出力は未対応で、暗黙メッシュ化しない。
- 1ノード1出力のため、Evaluate Surfaceの複数出力を独立ノードへ分割。接線/法線のXYZ成分は既存の点型で代用し、法線の可視化は別の線出力とする。

## 30ノードの実装範囲

| 分野 | ノード | 制限 |
|---|---|---|
| 曲面作成 (10) | 平面曲面、NURBSパネル、4点から曲面、円筒曲面、円錐台曲面、球曲面、トーラス曲面、NURBS回転花器、線織曲面（2断面）、曲面を押し出す | 開曲面の蓋なし。一般Sweep/Loft未対応 |
| 評価 (7) | 曲面上の点、曲面方向成分、曲面法線の表示、曲面積、最近点（平面）、等値曲線、曲面を点に分割 | 正規化UV。最近点は平面のみ。等値曲線は折れ線。 |
| 明示変換 (2) | 曲面をメッシュ化、B-repをメッシュ化 | 近似曲面をソリッドと呼ばない |
| B-rep (11) | 箱、体積、表面積、共通部分、結合、差、オフセット、移動、面、稜線、頂点 | 軸平行箱のみ。結合・差の出力は単一箱に限定 |

## 数値・失敗の範囲

- 解析面積・箱の体積/面積: 浮動小数点の公式評価。厳密実数演算ではない。
- NURBS面積: 4〜128の各方向分割の中点求積。誤差上界や収束保証なし。
- 表示用メッシュ: 既定24×24。明示変換は4〜128。直線区間の線織では節点を表示格子へ入れるため実分割数を調整。一般の曲面で弦誤差や自己交差を保証しない。
- 球の極・トーラスの周期継ぎ目は共有頂点として扱う。球は解析法線で極も評価。一般NURBS特異点の法線はエラー。
- 箱の最小各辺長0.001、最大1e6。トリム端点一致検査は1e-6座標単位。箱交差で接触・非交差・最小寸法未満はエラー。結合は包含または2軸境界が完全一致し残り1軸で接触/重なりがある場合。差は変化なし、または1方向からの単一箱切断のみ。穴・複数片・空・一般曲面Booleanは拒否。
- 処理上限、キャンセル、既存データツリーの重み制限を維持。NURBSの任意非一様節点に対する適応分割は未対応。

## 作例・テスト

新規例は旧10作品の番号や組み込みテンプレートを変更せず、examplesから開く。
- `surface-vase.morph` — 高さ・上下半径・中央制御半径を編集。開いた表面で印刷用の壁・底ではない。
- `surface-panel.morph` — 二次制御パネルと法線。
- `surface-box-intersection.morph` — 軸平行箱の共通部分から体積・明示メッシュ化。
- `surface-ruled.morph` — 長方形断面の押し出し側面と等値曲線。
- カーネル単体試験: 二次元有理曲面、非単位節点区間、一階微分、四分円柱、多スパン節点、解析面積・法線、周期/極の閉じた表示メッシュ、箱の面・辺・体積、限定Boolean、無効入力とキャンセル。
- 統合試験: 全新規ノードの評価、型接続、再計算とキャッシュ、4作例のロード→編集→保存→再読込→再評価、箱の明示メッシュ出力・境界整合、データツリー、未対応操作の安全な失敗。
- 実行コマンド: `cargo test --all-targets --locked` / `cargo build --release --locked --bin Morphlet`。実行済みとは扱わない。

## Grasshopper / カーネル候補調査

- [Grasshopper 1 Surface component list](https://grasshopperdocs.com/addons/grasshopper-surface.html) は90件（第三者の2023年版一覧）。[Evaluate Surface](https://grasshopperdocs.com/components/grasshoppersurface/evaluateSurface.html) は点・法線・U/V方向・フレームを個別出力。[Surface Closest Point](https://grasshopperdocs.com/components/grasshoppersurface/surfaceClosestPoint.html) は点・UV・距離を出力。本実装はこの全てを再現していない。
- [Grasshopper 2公式コンポーネント一覧](https://www.rhino3d.com/docs/grasshopper2/specs/index.html) は別の仕様。GH1と混同しない。GH2の完全分類・互換確認は未完了。
- [Truck modeling](https://github.com/ricosjp/truck/blob/master/truck-modeling/Cargo.toml): パッケージ宣言Apache-2.0。幾何とトポロジーを統合するRust CADカーネル候補。[Curvo](https://github.com/mattatz/curvo/blob/main/Cargo.toml): パッケージ宣言MIT。NURBS曲面の押し出し・ロフト・スイープ・回転等の候補。どちらも今回のCargo依存には追加しない。採用時は特定バージョンと推移依存のライセンス・MSRV・サイズ・数値精度を確認する。
- [Arclet CAD workspace](https://github.com/07egta/Arclet-CAD/blob/main/Cargo.toml) はMIT OR Apache-2.0、Rust 1.95、Truck 0.6系、形状演算・メッシュ・STEP依存あり。[geom](https://github.com/07egta/Arclet-CAD/tree/main/crates/geom/src) にはbspline、nurbs_curve、brep等、[kernel](https://github.com/07egta/Arclet-CAD/tree/main/crates/kernel/src) にsurface_trim、surface_ruled、loft、sweep、boolean_guard等の候補がある。個別実装・vendorのライセンスと検証結果を監査しきれていないため、今回はコードの移植やコピーを行わない。
- 新規外部依存なし。GPL/LGPL/AGPLを追加していない。

## 未対応 / 次の段階

一般のトリムループ・曲線稜線・複数シェル・曲面立体、滑らかな複数断面Loft、Sweep1/2、Boundary Surfaces、Network/Patch/Blend/Offset Surface、任意軸Revolve、Pipe・Cap・Join・Shell・Thicken、一般Boolean・Fillet・Chamfer、Curve–Surface/Surface–Surface/Brep–Planeの交差、曲率、汎用最近点、STEP、GH/GHX互換、完全な誤差制御、GPU/Windows GUIの実測。椅子、ねじれた塔、穴付き部品、厚み付き曲面の実用グラフも未完了。
