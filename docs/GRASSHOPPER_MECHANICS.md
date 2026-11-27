# Grasshopper の「仕組み」と NODES の差分

作成日: 2026-11-28。目的は、Grasshopper (GH) と同じ使い心地に近づけることです。

## 守ること

- 公開情報（公式ドキュメント、フォーラム、書籍、講義資料）で**挙動だけ**を理解し、自分の言葉と自前の実装にします。
- GH のコード・文章・アイコンは写しません。GPL/LGPL のコードは読みません。
- 確認できなかった内容は「推測」と書きます。GH1 と GH2 は別物なので混ぜません（このファイルの「GH」は主に GH1 の話です）。
- NODES の状態は 2026-11-28 時点の main（このブランチ）です。

## 出典

| 略称 | URL |
|---|---|
| S1 | https://wiki.mcneel.com/labs/explicithistory/datastreammatchingalgorithms （Longest/Shortest/Cross） |
| S2 | https://developer.rhino3d.com/guides/grasshopper/gh-algorithms-and-data-structures/advanced-data-structures/ （ツリーの照合・Graft・Flatten・Simplify・パス） |
| S3 | https://runxel.xyz/rhino-secrets/gh1-iomodifier/ （入力右クリックの修飾子） |
| S4 | https://developer.rhino3d.com/api/grasshopper/html/T_Grasshopper_Kernel_GH_ParamWireDisplay.htm （ワイヤ表示 3 種） |
| S5 | https://modelab.gitbooks.io/grasshopper-primer/1-foundations/1-2/4_wiring-components.html （配線・Shift/Ctrl・ワイヤ表示） |
| S6 | https://modelab.gitbooks.io/grasshopper-primer/1-foundations/1-1/2_the-grasshopper-ui.html （プレビュー・グループ・検索） |
| S7 | https://defkey.com/grasshopper-rhino-shortcuts （ショートカット一覧。ページの一部が崩れており、マウス操作の修飾キーは推測を含む） |
| S8 | https://255ribeiro.github.io/Algorithmic_Modeling_Special_Training/Grasshopper_Training/Slider/Slider_config.html （スライダー設定） |
| S9 | https://discourse.mcneel.com/t/suggestions-for-improving-clusters/99450 （クラスターの不満と提案） |
| S10 | https://discourse.mcneel.com/t/trigger-block-unstoppable-loop/123795 （Trigger の無限ループ、Data Dam の「漏れ」） |
| S11 | https://discourse.mcneel.com/t/handling-expired-solutions-on-downstream-components/211945 （下流の再計算の回数） |
| S12 | https://interactivetextbooks.tudelft.nl/rhino-grasshopper/Grasshopper_Rhino_course/4_Troubleshooting/Troubleshooting_Grasshopper/Colours%26Errors/%21index.html （灰・橙・赤の意味） |
| S13 | https://www.shapediver.com/blog/optimizing-grasshopper-definitions-explained （遅くなる原因） |
| S14 | https://discourse.mcneel.com/t/data-matching-tree/59746 （枝は順番でそろえる。既存の docs/DATA_TREE_ENGINE.md が引用） |

## 比較表

状態の記号: 済 = 実装済み、一部 = 一部だけ、未 = ない。「今回」は本ブランチ `feat/gh-mechanics` で入れたもの。

| 項目 | Grasshopper の挙動 | NODES の現状 | 差分 | 改善案 |
|---|---|---|---|---|
| データ照合 (Longest / Shortest / Cross) | 長さが違う入力は、Longest が既定。短い方の最後の値を繰り返す。Shortest は短い方で止める。Cross は全組み合わせ（10×5×2=100）[S1]。リストを受ける入力は照合の対象外 [S1] | 済。ノード設定で「既定・Longest・Shortest・Cross」を選べる（`__data_matching`）。docs/DATA_TREE_ENGINE.md に例あり | GH は入力ごとの設定ではなく部品ごと（推測：GH1 の右クリックメニュー相当）。NODES は旧ファイル互換のため「旧互換」が残る | 済のまま。新規ノードは既定を Longest にそろえる（Issue #78 に含む） |
| ツリーの照合 | 枝数が少ない側は最後の枝を繰り返す。対応する枝どうしで長さをそろえる [S2][S14] | 済（枝の順番で対応、最後を繰り返す） | 混在した深さの出力パスは GH1 と同一かどうか未検証（既存 docs に明記） | GH1 実機との比較テスト（人手）。Issue #78 |
| Graft | リストの要素ごとに新しい枝を作る。照合を強制する時にも使う [S2] | 済。入力の設定 `__tree_port:`（今回: 入力の丸を右クリックから選べる） | 空の枝を残す点は NODES 独自 | なし |
| Flatten | 全部の枝を、番号の小さい枝から順に 1 つのリストにまとめる。パスは `{0}` [S2][S3] | 済 | なし | なし |
| Simplify | 全部の枝に共通する余分な番号（先頭の 0 など）や空の枝を除く [S2][S3]。グラフトした出力では `{0;0}` が残ることがある [S3] | 済（共通の列を除く。1 枝だけなら最後の番号を残す） | GH の UI 版と DataTree API 版の違いは既存 docs に記載 | なし |
| Reverse | リストの順番を逆にする。複数パスなら各リストを逆にする [S3]（ツリー全体での動きは推測） | 今回: 済（各枝の中の順番を逆に。パスと枝の順番は変えない）`__tree_port:` の値 4 | 枝の順番を逆にする版は未対応 | 必要なら別の値で追加 |
| 入力の右クリックメニュー | Flatten / Graft / Simplify / Reverse / Expression / Reparameterize / Invert / Degrees / Principal [S3] | 今回: 入力の丸の右クリックで Flatten・Graft・Simplify・Reverse・ワイヤ表示・切断。以前は設定欄の ComboBox のみ（右クリックは切断だけ） | Expression（入力の式）、Reparameterize、Invert（真偽値がない）、Degrees、Principal が未 | Issue #73（Expression/Invert）、Issue #74（Reparameterize） |
| パラメータのアクセス (Item / List / Tree) | 部品の入力は「1 個ずつ」「リスト」「ツリー」のどれで受けるかが決まっていて、リスト入力は照合しない [S1][S2] | 済。`node_access` で入力ごとに持ち、ノード設定に表示 | GH のような専用の Param Viewer（枝・パスの確認）はない | Issue #78 |
| ワイヤ表示 | 入力ごとに Default / Faint / Hidden。Faint は細く薄い。Hidden は選んだ時だけ緑で出る。データは流れ続ける [S4][S5] | 今回: 済（`__wire:<入力キー>` 0 標準・1 うっすら・2 非表示。選択中は通常どおり描く）。表示だけで計算には影響しない | GH の「fancy wires」（型で線種を変える）に近い物は NODES に元からある（形で線種が変わる） | なし |
| プレビュー ON/OFF と Enable | 部品ごとにプレビューを切れる。Enable を切ると出力が空になる。全体設定（ワイヤーフレーム・影つき・オフ）もある [S6] | 済。カード上の目のボタンと設定欄で preview、`Ctrl+E` で無効（出力なし）。今回: `Ctrl+Q` で選択ノードのプレビューを切り替え | 「選択したものだけ表示」モード、全体の描画モード切り替えは一部のみ（`preview_mode`） | Issue #79 |
| Panel / Slider / Toggle / Number Slider | スライダーは実数・整数（偶数・奇数もあるらしいが出典では説明なし）、最小・最大・桁数を編集。値を直接入力できる [S8] | 一部。数値入力にスライダー＋直接入力、範囲と刻みは各入力の定義（`min/max/step/integer`）。独立したスライダー部品、真偽値、テキストパネルはない | 範囲を使う人が編集できない。式つきスライダー（RH8 の話題あり、詳細は未確認）もない | Issue #73 |
| Expression / 式 | 入力の右クリックで `-x/2` のような式を当てる [S3]。式の部品もある | 一部。「式」ノードがある（x y z w、sin 等、長さ・深さの上限あり。`src/expr.rs`） | 入力ごとの式はない | Issue #73 |
| Cluster | 選んだ部品を 1 つにまとめ、中を開いて編集する。入力が重複しやすい、名前が外に反映されない、別ウィンドウで混乱、デバッグしにくい、が不満 [S9] | 一部。クラスター定義は作品の中に保存（`src/cluster.rs`） | 入力の重複・名前の同期は未検証 | Issue #75 |
| グループ | `Ctrl+G` で囲み、名前・色・枠を変える。入れ子もできる [S6] | 済（名前・色・グループ解除）。入れ子はなし | 入れ子、解除のショートカット（GH は `Ctrl+Shift+G`、NODES は吸着切替に使用中） | Issue #79 |
| Bake 相当 | 選んだ結果を Rhino の形にする（`Insert`）[S7] | NODES は単体で完結。STL / STEP / G コードの書き出しが近い物 | 「その場で固定（値を作品に埋め込む）」はない | Issue #76 |
| Remote parameter / 無線 | 名前で別の場所の値を受け取る（GH の Relay・Scribble 等。出典は未取得で、ここは推測） | 未 | 長い配線が避けられない | Issue #72 |
| Data Dam | 流れを一度止め、あとで流す。ファイルを開いた時に 1 回流れてしまう「漏れ」の報告あり [S10] | 未 | 重い計算を手動にできない | Issue #71 |
| Trigger / Timer | 一定間隔で再計算させる。計算より間隔が短いと、終わった瞬間にまた走って UI が固まる [S10] | 未 | 無限ループ防止の方針が先に必要 | Issue #71 |
| 再計算の順序 | 変更した部品の**下流だけ**が期限切れになり、上流から順に解く。ひとつの変更で下流が複数回解かれる例がある（B が 2 回、C が 3 回）[S11]。循環配線は認められない（推測：GH は接続時に拒否） | 済。接続時に循環を確認、トポロジカル順に評価、同じ入力は結果を再利用（キャッシュ。名前・位置・preview はキーに入れない）。GH と違い下流が何度も解かれることはない（1 回） | 無限ループ防止は、グラフを非循環に限ることで満たしている。将来の Timer は別途設計が必要 | Issue #71 |
| ショートカット | `Ctrl+G` グループ、`Ctrl+Q` プレビュー切替、`Ctrl+E` 有効切替、`F3` 検索、`Insert` Bake、`Shift`/`Ctrl` ドラッグで配線の追加・削除 [S5][S7] | 一部。`Ctrl+E`、`Ctrl+G`、`Tab/Ctrl+K/Space` 検索、`Ctrl+C/V/D` など。今回: `Ctrl+Q` | 配線の Shift/Ctrl ドラッグ、上流・下流への選択移動、選択反転が未 | Issue #79 |
| よくある不満（遅い） | 大きい定義は遅い。ブーリアン・ロフト・オフセット・結合が重い [S13]。Data Dam / 無効化で一時停止 | 一部。キャッシュ、キャンセル、サイズ上限あり | 重いノードの手動実行（Data Dam 相当）がない | Issue #71 |
| よくある不満（分かりにくい） | 灰＝正常、橙＝警告、赤＝エラー。吹き出しで文言を見る。「データ変換に失敗」など型の不一致 [S12] | 一部。赤いエラー表示と日本語の文言あり（`diagnostics`）。警告（橙）の段階はない | 警告の段階がない | Issue #77 |

## 今回の実装（このブランチ）

1. 入力の丸の右クリックメニュー（Flatten / Graft / Simplify / Reverse、ワイヤ表示、切断）。
2. Reverse（`__tree_port:<入力>` = 4）。既存の 0〜3 は変更なし。
3. ワイヤ表示（`__wire:<入力>` = 0/1/2）。表示だけの設定だが、パラメータ表に入るため、変更すると再計算が走る可能性がある（キャッシュ鍵がパラメータを含むため。未確認）。
4. `Ctrl+Q` で選択ノードのプレビュー切り替え。

保存の互換性: `.morph` の構造は変えず、既存の数値パラメータ表（キーは `__` で始まる予約名）に値を足しただけです。古い実行ファイルでこれらを含む作品を開くと拒否される場合があります（既存の `__tree_port:` と同じ扱い）。古い作品は今までどおり開けます。

## 後回し（Issue にしたもの）

Issue の一覧は PR の説明を見てください。粒度は「GPT が 1 回で作業できる大きさ」、Rust のみ、main が土台、CI 緑が条件です。
