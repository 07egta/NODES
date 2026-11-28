# OMEGA 78 — T-spline 国際特許ランドスケープ

調査基準日: 2026-11-28 JST（20:13 頃開始）。対象 main: 09a2a546d442fea822d80fb4c756a81f067a8b00。これは公開情報に基づく技術調査であり、弁理士による FTO 意見、登録原簿証明、非侵害保証ではない。Google Patents の表示は公報本文と集約法的状態を区別する。公式登録簿で未確認の事項は未確定とする。

## 目的と結論

- 米国基本特許 F01 US7274364B2 の Google Patents 表示は Expired - Lifetime、満了日 2024-03-26。これだけでは国際 FTO を意味しない。欧州対応は拒絶イベントがあるが、日本対応は登録・経過未確定。
- 後年の改良権利 F02–F07、F11 は別問題。特に trim-free 変換、点群適合、非一様星点処理、SubD/NURBS 双方向編集、GPU 境界処理は要件別に検討する。
- NODES の現在の公開ソースは T-junction 局所細分・局所ノット・有理基底評価・Bézier 分割を含む。単なる名前だけの SubD ではない。実行バイナリや配布先の全監査は未実施。
- この PR は調査文書のみ。既存機能の削除、公開履歴の変更、法的断定、新規アルゴリズム実装はしない。

## 技術史（論文と権利は別）

|年|公開研究・商用化|技術的意味|
|---|---|---|
|1978|Catmull & Clark, Recursively generated B-spline surfaces on arbitrary topological meshes, [論文](https://doi.org/10.1016/0010-4485(78)90110-0)|四角メッシュ細分による極限曲面。通常の正則部と星点を区別。|
|2003|Sederberg, Zheng, Bakenov, Nasri, T-splines and T-NURCCs, [ACM](https://doi.org/10.1145/1201775.882295), [BYU](https://scholarsarchive.byu.edu/facpub/1057/)|T-mesh では制御網の線が途中で終わる。局所ノットとテンソル積基底、T-NURCC の任意トポロジー概念。|
|2004|Sederberg ほか, T-spline simplification and local refinement, [ACM](https://doi.org/10.1145/1015706.1015715)|ブレンディング関数の分割と係数更新で形状を保つ局所細分。|
|2007|US7274364B2 登録 2007-09-25|優先日 2003-03-26、単独発明者 Thomas W. Sederberg。公開論文の共著者数と特許発明者は別。|
|2008|Loop & Schaefer, Approximating Catmull-Clark subdivision surfaces with bicubic patches, [Microsoft Research](https://www.microsoft.com/en-us/research/publication/approximating-catmull-clark-subdivision-surfaces-with-bicubic-patches/)|近似双三次パッチ。星点付近の真の G1 と見かけ上滑らかな法線は別。|
|2011|Autodesk が T-Splines Inc. の特定技術関連資産を取得（2011-12-22 発表）[発表](https://investors.autodesk.com/news-releases/news-release-details/autodesk-acquires-t-splines-modeling-technology-assets)|会社全体を買収したとの断定は避ける。F01 の譲渡記録は別に照合。|
|2012|Scott, Li, Sederberg, Hughes, Local refinement of analysis-suitable T-splines, [論文](https://doi.org/10.1016/j.cma.2011.11.022)|解析適合 T-spline、T-junction extension、基底の独立性・細分条件。一般 T-spline 全てが適合するわけではない。|
|現在|Autodesk Fusion の Form 機能 [公式ヘルプ](https://help.autodesk.com/view/fusion360/ENU/?contextId=FORM-EDIT-FORM-CMD)|製品操作仕様は参考。内部実装、商用ソース、特許ライセンスを取得したことにはならない。|

数学的対象や論文の記載内容、コードの著作権、特許請求項の全構成要件、商標は別の権利。公開されたアルゴリズムでも第三者の存続特許を実施する場合がある。

## 検索方法と再現手順

今回の検索軸: T-spline / T splines / T-NURCC / T-junction / local refinement / local knot vectors / analysis-suitable / extraordinary point / trim-free / subdivision NURBS / watertight patches / point cloud T-mesh / Sederberg / Autodesk / Mitsubishi Electric / NVIDIA / EPFL / locally refined splines。基本公報からファミリー、引用、権利者、関連請求項へ展開した。検索結果に現れたものを選別したスナップショットで、全 IPC/CPC や全引用網を網羅していない。再調査は G06F30/10 等の関連分類を個々の公報から取得し、別表記・権利者旧称・継続/分割・引用先を掛け合わせ、公開後の補正・登録請求項も読む。

根拠レベル: P = 公開公報原文（Google Patents の転載）で請求項を確認、S = 同サイトの集約法的表示/イベント、O = 当局の原簿（今回は個別の確定照合なし）。P/S は O の代わりにならない。未精査候補は C。調査日と国別障害は jurisdiction-status.md。

|ID|代表公報|重点|表示（2026-11-28、S）|請求項調査|
|---|---|---|---|---|
|F01|[US7274364B2](https://patents.google.com/patent/US7274364B2/en)|T-junction、局所ノット、形状不変細分|満了|P: 1,4,9,11,16,18 と関連従属|
|F02|[US9269189B1](https://patents.google.com/patent/US9269189B1/en)|トリムなし T-spline 変換|Active|P: 1–2|
|F03|[US10832471B2](https://patents.google.com/patent/US10832471B2/en), [JP6843237B2](https://patents.google.com/patent/JP6843237B2/en)|点群から T-spline|Active (US/JP)|P: US 1; JP 登録請求項の差分要精査|
|F04|[US11620793B2](https://patents.google.com/patent/US11620793B2/en)|非一様星点、固有多面体|Active; CN/DE 出願|P: US 1|
|F05|[US11928391B2](https://patents.google.com/patent/US11928391B2/en), [US20240184934A1](https://patents.google.com/patent/US20240184934A1/en)|SubD/NURBS 編集履歴・継続出願|Active / Pending|P: 登録 1; 継続の最新補正未確認|
|F06|[US8570324B2](https://patents.google.com/patent/US8570324B2/en)|GPU パッチ境界処理|Active|P: 1|
|F07|[US10339226B2](https://patents.google.com/patent/US10339226B2/en)|H-spline 曲面接続|Active|P: 1|
|F08|[WO2011025385A1](https://patents.google.com/patent/WO2011025385A1/en)|局所細分スプライン代替の調査候補|国際段階 Ceased; US abandoned, EP withdrawn|C: 最新全請求項未精査|
|F09|[US8310481B2](https://patents.google.com/patent/US8310481B2/en)|周辺技術候補|Expired - Fee Related|C: 回復と請求項未確認|
|F10|[US20080162090A1](https://patents.google.com/patent/US20080162090A1/en)|周辺技術候補|Abandoned|C|
|F11|[US11120625B2](https://patents.google.com/patent/US11120625B2/en)|曲率連続 SubD 変換|Active|C: 優先精査|

F01 譲渡（集約イベント）: BYU → T-Splines（2010-02-18 効力、2010-04-09 記録）、T-Splines → Autodesk（2011-12-21 効力、2014-08-07 記録）。NSF confirmatory license (2009-07-31) 記録もあるが法的効果は未評価。US 登録原簿、譲渡証書、訂正公報、特許期間調整、国別登録経過、維持料を最終確認する。

## 成果物
- [ファミリーマトリクス](patent-family-matrix.csv)（24 行、11 ファミリー/候補 ID、同一家族の公報を別行化）
- [請求項対応](claim-mapping.md)
- [国別登録状態](jurisdiction-status.md)
- [機能リスク・担当者ゲート](implementation-risk.md)
- [代替技術](alternative-technologies.md)
- [実装出典](source-provenance.md)

優先順位: F01 の公式原簿・対応国、F02/03/04/05 の独立請求項と継続・分割、F11 の精査、公開ソース/配布物の機能単位ゲート。リスク表で保留のものは、無料配布・研究用途・独立開発を理由に自動解禁しない。
