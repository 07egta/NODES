# 独立自由曲面技術の選択

調査日: 2026-11-28 JST。基準コミット 09a2a546d442fea822d80fb4c756a81f067a8b00。公開資料による予備調査であり、非侵害・有効性・FTO の法律意見ではない。

|方式|局所性|不規則頂点|CAD変換|採用判定|
|---|---|---|---|---|
|Tensor-product NURBS|ノット線全体へ伝播|複数パッチで処理|自然|基本の精密面として採用候補|
|Catmull-Clark (1978)|通常細分は全体、適応評価別|任意価数、連続性は正則部と区別|正則部は双三次、星点は近似/別扱い|既存ベース維持候補|
|ACC (Loop/Schaefer 2008)|面単位抽出|星点近似|双三次パッチ|誤差・G1/G2検証、F11調査|
|Hierarchical B-spline / THB|階層領域の局所細分|基本は矩形パラメータ領域|区分多項式→Bézier|OMEGA 79の第一研究候補（FTO未確認）|
|LR B-spline|部分メッシュ線で局所細分|通常矩形領域|Bézier抽出候補|F08、派生特許を確認して比較|
|PHT-spline|階層T-mesh|通常矩形領域|区分多項式|候補、T-splineと同じとみなさず特許検索|
|Gregory patch / multi-patch NURBS|パッチ局所|接合・星点を別設計|Gregoryの厳密NURBS変換は一般に要検討|連続性・出典・F11を精査|
|T-spline / ASTS|局所細分|実装ごとに限定|有理Bézier抽出可能|既存実装の新規配布判断を専門家に付す|

THBの参考: Giannelli, Jüttler, Speleers (2012), THB-splines: The truncated basis for hierarchical splines, [DOI](https://doi.org/10.1016/j.cagd.2012.03.025)。LR: Dokken, Lyche, Pettersen (2013), Polynomial splines over locally refined box-partitions, [DOI](https://doi.org/10.1016/j.cagd.2013.03.005)。Catmull–Clark、ACC、T-spline研究は [landscape](patent-landscape.md) の文献参照。論文公開だけではライセンス・非侵害を証明しない。

## 研究設計
- 標準NURBSを参照実装とし、階層レベル・有効領域・基底のsupport・truncationを明示。局所細分前後の同一点比較、partition of unity、非負性、線形独立性、境界連続性を検証。形状保存は同次係数更新と数値誤差の両面から確認。
- 正則矩形領域でのTHBから始め、星点・任意トポロジーを別問題として扱う。新しい基底とNURBS変換の誤差評価を分離。細分の局所性は追加制御点数/全制御点数とsupport範囲で測る。
- 星点の近似曲面は厳密と表示しない。G0/G1/G2をパラメトリックC0/C1/C2と分離し、境界・微分残差を評価。意図した折り目は別の連続性ルール。
- UIは backend capabilityを照会し、未対応の局所細分・変換を明示。保存形式は手法タグ、バージョン、誤差、制御網を保持し、未知手法を黙って別手法に変換しない。
- 評価項目: 形状保存誤差、partition of unity、ベンチマーク、B-rep縫合、局所点数増加、キャンセル・上限、ライセンス、国別請求項。数値テスト成功とFTO合格は別ゲート。
