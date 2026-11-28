# SolidCraft から取り込んだコードの変更記録

NODES は厳密 NURBS / B-rep / T-spline のために、SolidCraft（MIT OR Apache-2.0、
https://github.com/tangency-labs/solidcraft）の幾何カーネルを取り込んでいます。
取り込み元は [melt3D の `7f18150`](https://github.com/07egta/melt3D/tree/7f18150bdcc156944fee71617705d66213757dab) の SolidCraft 0.26.0 です。
NODES の [取り込みコミット `1fe143b`](https://github.com/07egta/NODES/commit/1fe143bcf9e745f3d12ade83ea7a790d9ac83a6c)
とファイルのハッシュを照合し、対応する `crates/geom`・`crates/kernel` のソースおよび
下表の vendor ディレクトリが一致することを確認しました。
ライセンス本文は各フォルダに残し、第三者表示は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) にあります。

方針: **そのまま使わず、NODES で使いやすく直す。** 直接の改造は最小限にし（上流の修正を後で
取り込みやすくするため）、使いやすさの改良は NODES 側の `src/kernel_bridge.rs` と
ノード層（既定値・日本語エラー・入力の自動補完）で行います。カーネル本体に手を入れた場合は
必ずこの表に 1 行追加してください。

## 取り込んだもの

| NODES の場所 | 取り込み元 | 備考 |
|---|---|---|
| `crates/sc-geom` | `crates/geom` | NURBS 曲線・曲面、B-spline、T-spline（T-mesh、局所細分、NURBS 抽出）、テッセレーション計画、メッシュ |
| `crates/sc-kernel` | `crates/kernel` | B-rep 立体、押し出し・回転・ロフト・スイープ、ブーリアン、フィレット・面取り、シェル、STEP 入出力、T-spline シート |
| `crates/sc-sketch` | `crates/sketch` | 2D スケッチ、拘束ソルバー、寸法、閉じた領域（プロファイル）の検出。文字の組版とフォントは取り除いた（下の表） |
| `vendor/truck-*`, `vendor/matext4cgmath` | `vendor/` | SolidCraft が修正した truck。変更点は `vendor/README.md`（上流の記録をそのまま保存） |

取り込まなかったもの: `crates/io`（`doc` と cadmpeg に依存するため。STEP は kernel に含まれる）、

UI・描画系（egui 0.36 / wgpu。NODES は egui 0.33 なので一切取り込まない）。

## 変更一覧

| 日付 | ファイル | 変更 | 理由 |
|---|---|---|---|
| 2026-11-27 | `crates/sc-geom/Cargo.toml`, `crates/sc-kernel/Cargo.toml` | `workspace = true` の継承を明示値に置き換え、`publish = false`、`rust-version = "1.88"`。lint 設定は SolidCraft の workspace と同じ内容を各 crate に複写 | NODES の workspace は SolidCraft の workspace 定義を持たないため |
| 2026-11-27 | `crates/sc-kernel/Cargo.toml` | dev-dependencies（proptest、truck-meshalgo の `test-hooks`）を削除し、`crates/kernel/tests/` を取り込まない | NODES の CI はルート package のテストのみ実行し、ロックファイルを小さく保つため。カーネル自身の網羅テストは上流で実行される |
| 2026-11-27 | `crates/sc-*/rustfmt.toml` | SolidCraft の `rustfmt.toml`（max_width 150）を複写 | `cargo fmt --all -- --check` が上流の整形のまま通るように |
| 2026-11-27 | ルート `Cargo.toml` | `[workspace]`（members: `.`, `crates/sc-geom`, `crates/sc-kernel`、`vendor` を除外）と `[patch.crates-io]` で truck を `vendor/` に差し替え。`solidcraft-*` と truck は debug-assertions 無効 | SolidCraft と同じ構成。truck のデバッグ専用検査は正しい形状（シーム辺・閉じた辺）を拒否するため |

## 2026-11-28 の取り込み: `crates/sc-sketch`

| ファイル | 変更 | 理由 |
|---|---|---|
| `crates/sc-sketch/Cargo.toml` | workspace 継承を明示値に。依存から `harfrust`・`ttf-parser`・`unicode-bidi` を外す。example を外す | 文字組版は NODES のスケッチで使わない。依存とフォントファイル（Liberation Sans、OFL）を持ち込まないため |
| `src/text.rs` | 文字の形状を作る処理を削除し、保存形式に必要な `TextStyle` と `TextAlign` だけを残した | 文字リンクのあるスケッチ JSON も読めるように型は残す |
| `src/fonts.rs` | 削除 | 同上 |
| `src/lib.rs` | `fonts` と、文字関係の再公開（`text_geometry` など）を削除。ファイルを分けたテストモジュールの宣言を削除 | 同上。NODES の CI はルートのパッケージだけをテストするため |
| `src/*_tests.rs`, `src/tests.rs`, `tests/` | 削除。ただし `#[path]` で参照される 5 つのファイル（`decompose_tests.rs`、`link_identity_tests.rs`、`link_side_tests.rs`、`offset_topology_tests.rs`、`redundancy_tests.rs`）は残した | 同上 |

## ソースコード本体への変更

なし（2026-11-27 時点）。カーネルの規則（非テストコードで unwrap/expect/panic 禁止、
第三者カーネル呼び出しは `guard` で panic をエラー化）は上流のまま維持されています。
例外: vendor の truck は上流 truck のコードであり、内部に panic を含みます。NODES からは
必ず `solidcraft-kernel` の公開 API（`guard` 済み）経由で呼びます。

## NODES側の統合

- 立体ブーリアンが成功して表示メッシュに開きがある場合、NODES は SolidCraft の
  `faceted_boolean` で表示・STL 用メッシュを作り直します。入力は両方で 8,192 三角形までです。
  閉じた多様体・面方向・体積・外接箱を確認し、幾何演算・体積ノード・STEP には元の B-rep を使います。
- 1 レールの立体スイープでは、レールが断面と平行な平面に収まる入力をカーネルに渡す前に検出します。断面から体積を作れるレールの向きを案内します。
- STEP 書き出しはノードの設定欄の **STEP を保存** から行います。上書き確認と一時ファイル経由の保存には既存の仕組みを使います。
  読み込みは **ファイルを選ぶ** から行い、接続先の計算を更新します。制限付きのグラフ API は STEP ファイルを扱うノードを拒否します。

## 既知の制限

- `crates/sc-kernel/src/step_in/tests.rs` などカーネル単体テストの一部は SolidCraft リポジトリの
  `tests/step-conformance/` を参照するため、NODES 内で `cargo test -p solidcraft-kernel` は通りません。
  NODES 側の検証は `tests/nurbs_kernel.rs` で行います。
