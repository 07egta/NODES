# OMEGA 05 — Mathematical Geometry Laboratory / checkpoint 1

基準 main: ec3b840e81c76aa7e426da5dc47b716ba975a98e。専用ブランチ: `feat/omega05-mathematical-surfaces`。
2026-11-28 JST。AGENTS.md / ROADMAP.md / ARCHITECTURE.md / STATUS.mdを確認。PR #52（数式・生物・フラクタル）と #65（Expression拡張）の重複を避け、未登録の解析曲面6種を優先。SolidCraftは統合済みだが、今回は明示的なパラメトリック式からメッシュを生成するため再利用による精度改善はなく、依存追加もしない。NURBS・B-repに偽装しない。

## 解析式と造形例

角度は内部ラジアン、位置はモデル単位。U,V 分割はセル数。サンプリングは一様で、適応分割や公差保証はまだない。

| ノード | 解析式（パラメータ）・条件 | 造形例 | 出典 |
|---|---|---|---|
| カテノイド | (r cosh(v) cos(u), r cosh(v) sin(u), rv), u in [0,2π), v in [-extent,extent], r>0 | 砂時計状の極小曲面 | [MathWorld](https://mathworld.wolfram.com/Catenoid.html) |
| ヘリコイド | (v cos(u), v sin(u), pitch*u/(2π)), v in [-r,r], u in [0,2π*turns] | 螺旋階段状の曲面 | [MathWorld](https://mathworld.wolfram.com/Helicoid.html) |
| エンネパー | s*(u-u³/3+uv², -v-u²v+v³/3, u²-v²), u,v in [-extent,extent] | 花弁のような鞍形 | [MathWorld](https://mathworld.wolfram.com/EnneperSurface.html) |
| メビウス | ((R+v cos(u/2)) cos(u), (R+v cos(u/2)) sin(u), v sin(u/2)), abs(v)<=half_width<=0.9R | ねじれた一面の彫刻 | [MathWorld](https://mathworld.wolfram.com/MoebiusStrip.html) |
| クライン | s*((a+cos(u/2)sin(v)-sin(u/2)sin(2v))cos(u), (a+cos(u/2)sin(v)-sin(u/2)sin(2v))sin(u), sin(u/2)sin(v)+cos(u/2)sin(2v)), a>=2.5, u,v in [0,2π) | 自己交差する8の字の壺 | [MathWorld](https://mathworld.wolfram.com/KleinBottle.html) |
| ディニ | s*(cos(u)sin(v), sin(u)sin(v), cos(v)+ln(tan(v/2))+twist*u), 0.05<=v_min<v_max<=1.45 rad（0,π/2の特異域を避ける） | 螺旋状の負曲率シェル | [MathWorld](https://mathworld.wolfram.com/DinisSurface.html) |

カテノイド・ヘリコイド・エンネパーの解析面は平均曲率ゼロ。離散メッシュの平均曲率が厳密にゼロという意味ではない。クラインは3D自己交差浸入、メビウスとクラインは非向き付け可能。向きの不整合は数学的性質であり、法線統一や自動修復で正しいソリッドになるとは限らない。厚みや印刷可能性は提供しない。

[Wolfram ParametricPlot3D](https://reference.wolfram.com/language/ref/ParametricPlot3D.html) の初期サンプリング/適応分割の考え方を参考にしたが、今回の実装は固定分割。出典のコードや画像はコピーせず数式を独立実装。

## 数値・資源契約

- U 8〜256、V 2〜256（Klein は8以上）。整数のみ。ヘリコイド/ディニは1回転に12セル以上。入力範囲はNodeSpecとカーネル双方で検査。
- 両端開放では (U+1)(V+1) 頂点、2UV 面。Catenoid は U 周期、Mobius は U 逆接続、Klein は U 逆接続 + V 周期で縫合する。頂点重複の溶接に頼らない。
- 最大66,049頂点、131,072面。エンジンのpreflightに頂点数+3×面数を通知し、複数入力による大量生成も既存上限で拒否。
- 各列でキャンセル確認。既存finite_point / as_mesh で非有限座標、上限、無効インデックス、面積ゼロに近い三角形を拒否。
- 自己交差検査、適応サンプリング、幾何誤差保証は未実装。原理的に非向き付け可能な形状を製造用ソリッドと扱わない。

## 作例と検証

`examples/mathematical/` に各ノードの編集可能な .morph を追加。出力は Mesh。 `tests/mathematical_surfaces.rs` で既定作例のロード・評価、位相境界・Euler標数、点座標、入力条件・分割数・保存表現を検証する。

このチャット環境ではGitHubへの直接cloneとRustツールチェーン実行が利用できず、コンパイル・fmt・clippy・cargo test・実GUIは未確認。PRのCI結果を確認し、失敗があれば修正してからマージする。サブエージェントは利用できない。

## 次の再開地点

1. CIのfmt / clippy / test、6作例のプレビューと高分割時の形状・応答確認。
2. 独立した数学モジュールを維持し、数式曲面（u,v）と特異点を跨がない適応サンプリングへ進む。ExpressionはPR #65を監査し重複を避ける。
3. 高次元は4D点・回転・投影のデータ契約を設計し、W=投影距離の特異点を切り分ける。Costa/Kuen/Boy、超楕円体、調和関数、Mandelbulb、数値ODE/FFT等は未実装として別のPRへ。NURBSへの変換には誤差契約を設け、既存SolidCraft連携を利用する。
