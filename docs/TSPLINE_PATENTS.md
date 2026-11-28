# T-spline の特許について（調査メモ）

2026-11-27 時点で公開情報から調べた内容です。**法律上の助言ではありません。** 製品として配布・販売する前に、必要なら専門家に確認してください。

## わかったこと

- T-spline の基本特許は **US 7,274,364 B2**「System and method for defining T-spline and T-NURCC surfaces using local refinements」（Sederberg ほか、2007 年に登録）です。T-Splines, Inc.（2004 年設立）が事業化し、同社は 2011 年に Autodesk に買収されました。
  - 出典: [Wikipedia: T-spline](https://en.wikipedia.org/wiki/T-spline)（「US patent 7,274,364 は 2024 年に失効」と記載）
- McNeel（Rhino）のフォーラムでは、この特許が **2024 年 3 月 26 日にパブリックドメインになった** と投稿されています。McNeel は、Rhino が T-spline ではなく SubD（Catmull-Clark）を採用した理由を特許問題だと説明しています。
  - 出典: [McNeel Discourse: "TSplines patent is in the public domain…"](https://discourse.mcneel.com/t/tsplines-patent-is-in-the-public-domain-will-subd-get-local-detail/178748)
- 欧州の対応出願 **EP1606692**（[Google Patents](https://patents.google.com/patent/EP1606692)）があります。各国での状態は確認していません。

## 確認できていないこと

- US 7,274,364 の正確な失効日と、権利延長の有無（Google Patents の詳細ページは今回取得できませんでした）。
- 継続出願、関連出願、Autodesk が後から取得した T-spline 関連特許の有無と状態。対象には analysis-suitable T-spline、T-NURCC の変換、編集 UI などがあります。
- 日本と欧州の対応特許の状態。

## NODES の方針

1. **SubD（Catmull-Clark）→ NURBS の経路を優先します。** Catmull-Clark（1978）と Loop & Schaefer の近似 Catmull-Clark（2008）は古くから公開されている手法です。箱ケージなどの一般ケージはこの経路で B-rep にします（`src/subd.rs`）。
2. 「T-spline」と呼んでいる円柱ケージは、T 字の接合を使わない **テンソル積の B-spline**（通常の NURBS と同じ）です。細分はノット挿入です。T-junction を含む局所細分（上記特許の中心）は、NODES のノードとしてはまだ使っていません。SolidCraft の `sc-geom::tspline` には実装がありますが、NODES は正則メッシュの NURBS 抽出だけを使っています。
3. T-junction の局所細分をノードとして公開する前に、この文書の「確認できていないこと」を見直します。
