# MISSION 02 — UI / UX 検証

## 変更の範囲
- 独立型 Rust/egui の既存エンジンを維持。データモデル、.morph 形式、バイナリ名 Morphlet は変更しない。
- 日本語・英語の操作意図検索、全角入力、IME 入力中の確定保護、最近・お気に入り。
- 型によるソケット色と候補フィルター、既存のグラフ検証による接続拒否、右クリックでワイヤー挿入・削除、入力からドラッグして再接続。
- ワイヤー操作は一時グラフで検証してコミットする。キャンセル・失敗時に元の辺を維持する。
- コンパクトなメニュー、パネル幅変更、狭い画面ではライブラリを一時非表示（Ctrl+K で検索）、ライト／ダークテーマ。
- ミニマップ（10ノード以上、十分なキャンバス領域のみ）、クリック・ドラッグで移動。選択へのフィット、整列、均等間隔、同一作品内コピー＆ペースト。低ズームでは簡略カードを描画。
- 見えないカードの clone と画面外ワイヤー描画を減らす。高速化の実測値はまだない。

## 検証の再現

```powershell
cargo test --all-targets --locked
cargo build --locked --bin Morphlet
python scripts/capture_ui.py --binary target/debug/Morphlet.exe --out ui-captures --quick
```

`--quick` を外すと、3 解像度 × 3 UI ズーム × 3 ノード数 × 2 テーマ = 54 枚を実アプリからキャプチャする。
解像度はウィンドウ内寸の指定、UI ズームは egui の倍率であり、Windows OS の DPI 設定自体を変更するものではない。
OS の表示倍率が 100% の環境では `--strict-pixels` で PNG の実寸を検証する。
自動処理では画面の存在と PNG サイズを確認する。見た目・手動操作・OS IME の合否は別に確認する。
Linux CI の仮想ディスプレイによるスクリーンショットは Windows 実機確認の代替にはならない。

### 手動確認項目
- 1280×720、1920×1080、2560×1600、Windows 100/150/200% DPI でウィンドウ操作と文字切れを確認。
- 円を作る、点を並べる、面を作る、花瓶を作る、Construct Point、Number Slider、全角 CIRCLE を検索。
- IME 変換確定 Enter で意図せずノードを追加しないこと。
- 型違いの接続・循環接続を拒否。右クリックで線を削除／ノードを挿入。入力からドラッグして再接続し、不適合先や Esc で元の辺が残ること。
- 10/100/1,000 ノードで選択・パン・ズーム、ミニマップ、コピー、整列、Undo/Redo、フレーム時間を確認。
- 3D の回転・パン・ズーム、正投影/透視、上面/正面/側面、陰影/ワイヤー、選択表示。既存ビューポートの奥行き精度と大規模形状のフォールバックを確認。
- Before は基点 df77c542473a5c637f3353dec48e367dd67db027 の既存 `--screenshot`、After はこのブランチの同一テンプレート。画像が実際に取得できるまで比較済みとしない。

## 残課題
任意の自然言語理解、OS 間／作品間のクリップボード、ショートカット変更、グループ折り畳み、ポートへの複数ソース、ワイヤー上へ既存ノードをドラッグ挿入、経路全体強調、ノードごとの計算時間、完全な英語 UI、初回対話チュートリアルは未実装。3Dエンジンは既存実装のまま。

## 調査と設計判断

操作の参考資料（公開ドキュメント。実装コード、画像、アイコンは流用していない）:

| 製品 | 資料 | NODES への反映 |
|---|---|---|
| Grasshopper | [Grasshopper Primer: The Grasshopper UI](https://grasshopperprimer.com/en/1-foundations/1-1/2_the-grasshopper-ui.html) | 空白ダブルクリック検索、カテゴリ、グループ、選択整列、データ表示を継承 |
| Blender | [Blender 5.0 Manual: Editing Nodes](https://docs.blender.org/manual/en/5.0/interface/controls/nodes/editing.html) | 型の合うソケット、リンク挿入・再接続（このPRでは右クリックで明示） |
| Houdini | [Network navigation](https://www.sidefx.com/docs/houdini/network/navigate.html)、[Wiring nodes](https://www.sidefx.com/docs/houdini/network/wire) | 選択範囲フィット、ナビゲーション概要図、配線からのノード追加 |
| Fusion | [キーボードショートカットの参照](https://help.autodesk.com/cloudhelp/JPN/Fusion-GetStarted/files/GUID-F0491540-0324-470A-B651-2238D0EFAC30.htm) | キーボードからアクセスできる検索・共通編集コマンド |
| Plasticity | [Command Palette](https://plasticity-document-revisions.vercel.app/plasticity-essentials/plasticity-interface/command-palette) | 候補リスト、キーボード確定、お気に入り、コンパクトな操作面 |

このPRの検索は辞書・キーワード・曖昧一致によるもので、AIサービスや他社のコンポーネントカタログを組み込まない。
型候補の絞り込みは既存の静的 DataType::accepts に準拠する。Any/Geometry は広い型であり実データの検証は評価時にも必要。
1000ノードで縮小率の下限に達する場合はミニマップとパンを使用する。完全な表示性能の測定は別途必要。
