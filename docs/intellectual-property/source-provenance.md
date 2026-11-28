# ソース来歴・実装監査

調査日: 2026-11-28 JST。基準コミット 09a2a546d442fea822d80fb4c756a81f067a8b00。公開資料による予備調査であり、非侵害・有効性・FTO の法律意見ではない。

## 固定参照
基準 [main] (SHA 09a2a546d442fea822d80fb4c756a81f067a8b00): https://github.com/07egta/NODES/commit/09a2a546d442fea822d80fb4c756a81f067a8b00 。以下リンクはこのSHAに固定。blob SHAはGitHubが返した値。

|ファイル|blob SHA|観察|
|---|---|---|
|[src/subd.rs](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/src/subd.rs)|9c4c3c07239e46a5868af25a94ec41d25a86d642|格子NURBS、一般CC/ACC、実T-splineの3経路。1195–1241で局所refine。|
|[src/cage_edit.rs](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/src/cage_edit.rs)|c712a558562c0fd65bbf7942e09f97cb89216bf8|選択、ドラッグ、編集リスト、476–480で局所細分UI。|
|[crates/sc-geom/src/tspline/mod.rs](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/crates/sc-geom/src/tspline/mod.rs)|531f13b5f89cf88f8758baaff4555bcbf4878f8b|局所ノットから有理基底、奇数次数、矩形index domain。論文2003/2004/2012を明示。|
|[mesh.rs](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/crates/sc-geom/src/tspline/mesh.rs)|85ff816d7d5bb9beffb57f9ca41552dab9f28272|T-mesh、rayによるノット、extension、解析適合判定。|
|[refine.rs](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/crates/sc-geom/src/tspline/refine.rs)|75f3af85837d0bc690251df0eb787072e932da2f|2004論文に基づくviolation補正、同次係数の分割/統合、形状保存。|
|[extract.rs](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/crates/sc-geom/src/tspline/extract.rs)|9a11aa744e5548d852b3eba83e14449ba4dc4e41|regular NURBS変換・Bézier抽出。|
|[crates/sc-kernel/src/tspline.rs](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/crates/sc-kernel/src/tspline.rs)|7688c72082f3ed210ba3cf5dc49b703622fe98e3|regularは1面、非regularはBézier要素を共有辺・頂点でopen shell化。|
|[src/nurbs_nodes.rs](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/src/nurbs_nodes.rs)|3fa49cd5067cc93c2e46c07bda949d62b442c65c|SubdTsplinePatch登録69/570/800行。箱/円柱も検索語tspline。|
|[src/kernel_bridge.rs](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/src/kernel_bridge.rs)|6c324e38c29590ea16fae40cc88b4b91bdd016c4|SolidCraft bridge、エラー/公差。|
|[tests/nurbs_kernel.rs](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/tests/nurbs_kernel.rs)|9a02a8784daea33eee9331b4fdbc3f90685ef46b|376–403にT-junctionと1e-9未満のサンプル形状保存assert。実行は未実施。|
|[docs/TSPLINE_PATENTS.md](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/docs/TSPLINE_PATENTS.md)|8e15dcda0df106776d19907adc1507d7f329be32|11/28追記で局所細分を公開した記録あり。旧段落の未使用記述を現状と誤解しない。|
|[docs/SOLIDCRAFT_CHANGES.md](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/docs/SOLIDCRAFT_CHANGES.md)|da9deafffda84646074a2d401ff6c9e1551adb85|取り込み元melt3D 7f18150、NODES 1fe143b。既存のハッシュ照合記録。今回全ファイル再照合なし。|
|[Cargo.toml](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/Cargo.toml)|a626fab286eed6bca22991bcf8ff01b253f8956d|MIT OR Apache-2.0、vendored SolidCraft geom/kernel/sketch、truck patches。|
|[docs/THIRD_PARTY_NOTICES.md](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/docs/THIRD_PARTY_NOTICES.md)|b9d63620e3e3a35ea466420ac93faeb590b6272d|SolidCraftとtruckの帰属。過去の監査と現在の依存監査の範囲差を記載。|

関連PR [#48](https://github.com/07egta/NODES/pull/48)、[#67](https://github.com/07egta/NODES/pull/67)、[#81](https://github.com/07egta/NODES/pull/81)。[#139](https://github.com/07egta/NODES/pull/139) は調査時open、NURBS3作業中。コード検索tspline/T-splineは0件だったが、既知パスとPRから実ファイルを取得した。検索0件は不在の証拠ではない。全履歴・全ブランチ・全依存・ビルド成果物の監査は未実施。

## ライセンス・商標
NODESとSolidCraftはmanifest上MIT OR Apache-2.0。Apache-2.0第3条の特許許諾は各Contributorが許諾可能で、Contribution単独または提出先Workとの組合せにより必然的に侵害される請求項に限る。第三者Autodesk等の権利一般の許諾ではない。[NODES LICENSE-APACHE](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/LICENSE-APACHE)、[LICENSE-MIT](https://github.com/07egta/NODES/blob/09a2a546d442fea822d80fb4c756a81f067a8b00/LICENSE-MIT)。MITの著作権許諾と特許の解釈は分ける。NOTICE、ライセンス本文、変更記録を維持する。上流ライセンス本文と全依存の最終配布物照合は未実施。

T-spline/TSplinesの名称が数学的用語であってもブランドとしての使用は別途確認。日本J-PlatPat商標、[USPTO trademark search](https://tmsearch.uspto.gov/)、[EUIPO](https://www.euipo.europa.eu/)等の現行登録を確認するまで、公式互換・提携を示唆する製品名称を避け、数学的な記述用途に限定する。登録商標の具体的な状態は未確認。

## 再現と出典記録
上流は https://github.com/tangency-labs/solidcraft 、既存取り込み元は https://github.com/07egta/melt3D/tree/7f18150bdcc156944fee71617705d66213757dab 。導入時ハッシュ照合については既存文書の報告であり、今回の独立照合ではない。次回は全関連ファイルのgit log --follow / blame、上流同一SHA、差分、第三者帰属、Cargo.lockからの実際の依存グラフ、配布アーカイブのライセンス同梱を記録する。論文は DOI と版、特許は番号・請求項・対象国・調査日・原簿状態を記録。非公開商用コードは入手・利用していない。根拠のないクリーンルーム実装との断定はしない。
