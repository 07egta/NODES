# Claim mapping — 技術要件と NODES の対応（非法律判断）

調査基準日: 2026-11-28 JST（20:13 頃開始）。対象 main: 09a2a546d442fea822d80fb4c756a81f067a8b00。これは公開情報に基づく技術調査であり、弁理士による FTO 意見、登録原簿証明、非侵害保証ではない。Google Patents の表示は公報本文と集約法的状態を区別する。公式登録簿で未確認の事項は未確定とする。

公報原文を参照して独立請求項を要素別に要約する。逐語翻訳・法的クレーム解釈ではない。権利行使には当該国で存続する請求項、全構成要件（均等論等を含む）を専門家が比較する必要がある。「類似」「未確認」「相違」はコード観察であり非侵害判定ではない。以下は選定した請求項のみ、未調査従属請求項・審査履歴・訂正・無効審判を含まない。

## F01 基本特許 [US7274364B2](https://patents.google.com/patent/US7274364B2/en)

|独立請求項|構成要件の要旨（順序は説明用）|対象コード・証拠|対応・限界|
|---|---|---|---|
|1|矩形制御構造、両方向の T-junction を任意に近接させ得る構成、各点のテンソル積 B-spline 基底を導出、表示可能な曲面を計算|sc-geom/src/tspline/mesh.rs:242–315,138–158; mod.rs:69–112,175–215; src/subd.rs:1113–1178|構造と評価は類似。任意近接・許容条件の完全一致未確認。|
|4|T-junction を挿入、基底から新点および周辺点の座標を求め、形状を保存|refine.rs:1–18,70–135,299以降; src/subd.rs:1195–1241; tests/nurbs_kernel.rs:376–403|強い技術的対応。形状保存は既存テストのサンプルを読むのみ。|
|9|ノット間隔、局所座標系と局所ノットベクトル、単一点追加、両方向 T-junction|mesh.rs:138–158,369–377; mod.rs:69–112; refine.rs:70以降|類似。ただし UI は面ごとに2方向カット、単一点追加の法的構成は未確認。|
|11|任意トポロジー、局所ノット座標・エッジ間隔、両方向 T-junction、局所基底導出|mod.rs:11–18,69–112; mesh.rs:1–10; src/subd.rs:38–45|本実装は矩形 index domain・奇数次数に限定。任意トポロジー全体への一致は未立証。|
|16|非整列ノットを持つ二つの B-spline、T-junction をオフセット挿入、結合|sc-kernel/src/tspline.rs:19–27,57以降; src/subd.rs:509–527|B-rep sewing/分割面の共有は確認したが、2つの B-spline の当該 T-junction 結合手順は確認していない。|
|18|二曲面の C2 接合を構成する T-spline 技術|同上、docs/NURBS_ROADMAP.md|既存コードの sewing は C2 保証とは別。照合未完。|

従属5: 欠けたノットを補う基底分割、余分なノットで点を追加する規則が refine.rs 冒頭の violation 1/2/3 と近い。従属6/7: ゼロ/小間隔による crease、一般ケージの Catmull-Clark crease と T-spline のノット操作を混同しない。従属8: 共通境界の整列による結合を個別照合。米国基本権利は集約表示で満了している一方、日本の対応、他の改良権利、補正等は独立に扱う。

## F02 [US9269189B1](https://patents.google.com/patent/US9269189B1/en) — trim-free T-spline
独立1: (a) B-rep 面と機能的トリム曲線、(b) NURBS グリッド、(c) 曲線に整列する trim-free T-spline への構成、(d) 隣接するトリムなし双三次可変ノットパッチ、対辺の間隔合計が異なる構成。従属2 は G1 条件。
現状: sc-geom/src/tspline/extract.rs:18–59 は regular NURBS→T-spline、T-spline→Bézier 要素、sc-kernel/src/tspline.rs:19–27 は T-spline→B-rep。トリム境界に合わせた逆変換の要件を確認していない。方向が逆というだけで非侵害とは言えない。新規 trim-free 再パラメータ化を保留。US Active 表示、公式未確認。

## F03 [US10832471B2](https://patents.google.com/patent/US10832471B2/en) / [JP6843237B2](https://patents.google.com/patent/JP6843237B2/en) — 点群フィッティング
US 独立1: (a) 3D センサーの単一フレーム深度画像から点群、(b) 点群の領域パラメータ化、(c) パッチを再帰分割し、最大フィット RMSE の割合を基準に閾値まで選択、(d) T-mesh を構成し曲面フィット。
現状: ユーザーが面を選ぶ src/subd.rs:1195–1241 の中点カットとは入力・判断・処理系列が異なる。点群機能の全 repo 監査は未完。JP 登録公報の文言・補正・権利状態は弁理士に原文確認依頼。点群→T-spline 自動適合を保留。

## F04 [US11620793B2](https://patents.google.com/patent/US11620793B2/en) — 非一様 extraordinary point
US 独立1: (a) 星点のパラメータ空間に固有多面体を構成、(b) 頂点・辺・面の位置調整規則、(c) 第1・第2近隣のノット間隔差を計算、等間隔/非等間隔に対応。
現状: src/subd.rs:259–418 の通常 Catmull-Clark、706–895 の ACC は星点で近似。調査範囲に当該固有多面体・間隔差方式の実装証拠なし。単に G1/G2 を目指すことは要件全体と等価でない。CN115130253A、DE102022107038A1 の係属・登録・補正を再調査。非一様星点の改良を保留。

## F05 [US11928391B2](https://patents.google.com/patent/US11928391B2/en) — 双方向編集
独立1: (a) subdivision mesh と保存 NURBS の部分間リンクを追跡、(b) 直接 NURBS 編集とリンクへの関連付け、(c) 後の制御網編集、(d) 極限面を置換し以前の編集を再適用、(e) 物理デバイス出力。
現状: src/cage_edit.rs と src/subd.rs:897–998 はケージ編集履歴を再生し、一方向に B-rep 化する。NURBS 直接編集の対応関係と再適用は確認していない。物理出力も組み合わせとして評価する。継続 [US20240184934A1](https://patents.google.com/patent/US20240184934A1/en) 明細書に親 US16/837618 を参照、最新補正・登録未確認。双方向の永続対応付け・編集再適用を保留。

## F06 [US8570324B2](https://patents.google.com/patent/US8570324B2/en) — GPU 境界
独立1: (a) パラメトリックパッチ境界のエッジ所有フラグ、(b) パッチ制御情報と隣接情報計算、(c) GPU 処理部へ渡しフラグを用い評価。
現状: sc-kernel/src/tspline.rs:57以降 の CPU 共有辺、src/subd.rs ACC の境界共有とは区別。GPU renderer/シェーダー全体は未監査。GPU パッチ割当・境界処理導入前にレビュー。

## F07 [US10339226B2](https://patents.google.com/patent/US10339226B2/en) — H-spline
独立1: (a) 空間を隔てる H-spline 二面、(b) 補間点と導関数の制御を利用する一致領域、(c) 接線・曲率を変更し水密結合を生成。
現状: T-spline/B-spline の係数制御とは別の構成。H-spline 代替を採用する場合は請求項レビュー。単なる水密性達成と同一視しない。

## F08–F11
F08 LR-spline 公報 [WO2011025385A1](https://patents.google.com/patent/WO2011025385A1/en) と US/EP の出願状態は代替調査候補。F09 [US8310481B2](https://patents.google.com/patent/US8310481B2/en)、F10 [US20080162090A1](https://patents.google.com/patent/US20080162090A1/en)、F11 [US11120625B2](https://patents.google.com/patent/US11120625B2/en) は候補検出段階。F11 は曲率連続 SubD→曲面の新規実装前に全文独立請求項・外国対応・経過を優先精査。これらに対する対応無しとの判定は行わない。

## 再レビュー条件
新ノード、演算方向変更、星点・非矩形トポロジー、GPU 実装、点群入力、NURBS 再編集、配布対象国・依存の変更時は請求項の国別テキストとコード SHA/行範囲を更新する。全要件の比較、法律上の均等論・間接侵害・先使用等の検討は専門家に委ねる。
