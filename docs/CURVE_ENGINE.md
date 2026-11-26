# 精密曲線カーネル / Mission 05

## 対象と状態
基準コミット: df77c542473a5c637f3353dec48e367dd67db027 (evolve/all)。
Rust標準ライブラリのみの独立実装。新規依存なし。コードはMIT OR Apache-2.0。
初回作成時は未コンパイルだったが、統合セッションのRust 1.100環境で実ビルドと曲線の11回帰テストが成功。曲面との連携7テストも実行済み。全体の最新結果は docs/development/STATUS.md を参照。
Rhino/Grasshopperのコード・バイナリ・UI素材は取得・コピーしていない。

## 公開仕様の分類（完全互換ではない）
- GH1: Grasshopper Docs の [Curve index](https://grasshopperdocs.com/addons/grasshopper-curve.html) は2023-12-12版の111コンポーネントを列挙。分類は解析、分割、プリミティブ、スプライン、ユーティリティ。
- GH2: [McNeel公式コンポーネント索引](https://www.rhino3d.com/docs/grasshopper2/specs/) は 2026-11-25出力の版で、Curveを Analysis / Conic / Fragment / Freeform / Line に再分類。交差はIntersectに属する。
- [Grasshopper Primer](https://modelab.gitbooks.io/grasshopper-primer/content/appendix/A-1/0_index-of-components.html) と [Divide Curve](https://grasshopperdocs.com/components/grasshoppercurve/divideCurve.html) も参照。
  GH1のDivide Curveには点・接線・パラメータ出力と角で分割する設定がある。ここでは既存の単一出力に合わせて点のみ返す。GH2のlocus/plane等も完全再現しない。
- 数学: C.-K. Shene の [B-spline/NURBS教育資料](https://pages.mtu.edu/~shene/COURSES/cs3621/NOTES/) および Piegl/Tiller, The NURBS Book。Cox-de Boor、同次座標でのde Boor、微分制御点、内部ノット挿入、弦長補間、Gauss-Legendre積分を独立実装。
- Rust候補 [curvo](https://github.com/mattatz/curvo) はMIT。NURBS曲面も扱うため将来の相互運用候補だが、今回は依存関係・表現差・既存キャッシュを考慮して導入しない。Arclet CADのコードも移植しない。

## 内部表現
- model::Curve の points は描画と既存メッシュ化に使う点列、closed は表示の開閉状態。geometry は任意の数学的基底とパラメータ区間。
- curve_geometry::NurbsCurve は f64、完全ノットベクトル（制御点数+次数+1）、正の重み、次数1〜12、最大8193制御点。重みは共通係数で正規化。
- CurveGeometry はNURBS基底または解析らせん/スパイラルの共有基底と向き付き区間。point/derivatives は0〜1正規化入力、domainは元の区間。部分曲線と反転は点列を再フィットせず基底を保持する。
- 円・円弧・楕円は有理二次曲線（数学的に正確、浮動小数点誤差あり）。ポリライン、長方形、多角形は次数1の曲線。
- ノットの多重度は内側で次数まで。周期一様B-splineは対応するが周期ノット挿入、任意の周期重み入力は未対応。
- 既存Circle/Helixは旧描画分割数と点列を維持。アフィン変形は制御点/軸へ適用し基底を保持。非線形Twist/Wave/Noiseは点列に落とし、古い精密基底を残さない。
- 既存メッシュLoft/Extrude/Revolveは引き続きpointsを使用するため精密B-repではない。別の精密SurfaceExtrude/RuledSurfaceとSurfaceIsoCurveは数学的NURBSを直接処理する（docs/CURVE_SURFACE_INTEGRATION.md参照）。表示用サンプリングはノットを含む均等パラメータ分割で、弦誤差の保証なし。
- 既存保存形式v1は計算結果を保存せずノードとパラメータを保存するため、新フィールドはファイル形式に追加しない。新ノードのファイルを旧アプリが読む前方互換性はない。

## ノード対応表（今回追加40種＋既存曲線ノード）
| 要求 | 状態 / 制限 |
|---|---|
| Line, Line SDL, Polyline, Rectangle, Polygon, Circle, Circle CNR, Circle 3Pt, Arc, Arc 3Pt, Ellipse | 実装。既存Rectangle/Polygon/CircleはXY、円弧/楕円もXY（3点円弧/法線指定円は3D）。GHのPlane型・全既定値互換なし |
| Interpolate, NURBS, Bezier, Helix, Spiral | 実装。補間最大128点。NURBSは制御点・重み・完全ノット指定が可能。 |
| Length, Evaluate Curve, Evaluate Length, Point on Curve, Middle, Tangent, Curvature, End Points, Domain, Closed, Control Points | 実装。端点や制御点などを別単一出力ノードに分ける。長さは数値積分。角の微分は片側評価。 |
| Closest Point | 近似、全局最小保証なし。パラメータ用ノードも用意。 |
| Divide Curve, Divide Length, Shatter, Sub Curve, Reverse | 実装。分割最大512区間。閉曲線の分割点は終点を重複させない。Shatterはパラメータ分割。 |
| Perpendicular Frame | 直交XYZ軸のみ。安定した平行移動フレーム・Plane型・Horizontal Frameは未対応。 |
| Offset | XYに平行な表示点列のマイター近似。自己交差解消・厳密NURBSオフセットなし。 |
| Line-Line / Line-Plane | 有限線分に限定。空交点は空リスト、重複やほぼ平行な線分の非一意解はエラー。許容距離付き。 |
| Curve–Curve / Curve–Plane / Circle–Circle / Self Intersection / 最短距離 | 一般形未対応。 |
| Join / Explode / Polycurve / Extend / Fillet / Chamfer / Rebuild / Fit / Blend / Divide Distance / Curve Plane / Orientation | 未対応。Shatter / Sub Curveは一部のSplit/Trim用途を満たすが交差ベースのトリムではない。 |

## 数値と制約
弧長はノットスパンごとの適応5点Gauss-Legendre（局所推定差1e-9基準、最大20万評価）。弧長逆算は同じ積分表と二分探索。証明された誤差上限ではなく、収束しない入力は拒否。
最近接は128均等パラメータ点＋ノットスパン＋局所三分探索。振動の強い曲線では局所最小を逃し得る。
表示用点列から数学的曲線を再構成して精密と呼ぶことは禁止。geometryなしの入力を解析する場合は、その点列を次数1の曲線として扱う。
ノード入力は既存Treeのpath対応を維持し、制御点/重み/ノットは枝ごとにリスト処理。複数の設定は既存リストノード同様に先頭要素を使う。

## 検証と作例
tests/curve_geometry_tests.rs: 有理1/4円の点・接線・曲率・弧長、ノット挿入による形状不変、周期シーム、弦長補間、パラメータと弧長の相違、解析らせん、逆方向と切出し、交差と平行・重複、異常入力、キャンセル、表示分割数からの独立、アフィン変形、データツリー、作例6ファイルの読み込み・評価・保存再読込。
2026-11-26: ローカルRust 1.100で曲線11・曲面連携7・曲面グラフ5テストが成功。Windows画面操作は未確認。
