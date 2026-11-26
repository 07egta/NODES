# 依存ライブラリ・ライセンス確認記録

確認日: **2026-11-26 UTC**

対象 Cargo.lock SHA-256: `39580f7254769a1ad65c48d05472dee4db82709713e9ba1dd8d633290f03f6ca`

本体の [MIT](../LICENSE-MIT) / [Apache-2.0](../LICENSE-APACHE) 方針は維持している。今回の依存監査・notice 追補では Cargo.toml、Cargo.lock、依存の機能フラグを変更していない。

## 最新の CI による宣言・同梱本文の確認

[通常CI run 45852634502](https://github.com/07egta/NODES/actions/runs/45852634502) のLinux依存監査は成功した。`cargo metadata --locked --format-version 1` の実データを [audit_dependencies.py](../scripts/audit_dependencies.py) で処理し、配布artifactに入った出力を [dependency-manifests.json](development/dependency-manifests.json) へそのまま保存した。対象sourceは `2f94df459533774f83593835ba1e22d4924a3c42`、実checkoutは `19717e70917718af1374e387720b29b619c1c1ed`。両者のtree一致とartifact ZIP SHA-256を確認している。出典は [CI_COMMITTED_VALIDATION.json](development/CI_COMMITTED_VALIDATION.json)。

| 項目 | この CI で確認した値 |
|---|---:|
| metadata 内のパッケージ | **303**（本体 1、第三者 302） |
| license 宣言あり | **303** |
| license 宣言欠落 | **0** |
| Cargo.lock に存在し、metadata に含まれなかったパッケージ | **0** |
| 同梱本文の SHA-256 が記録と一致 | **226 ファイル** |
| 上記の本文が対応するパッケージ | **122 件** |
| 構文を自動解釈しなかった license 宣言 | **14 件** |
| 一つ以上の review reason が付いたパッケージ | **184 件** |
| GNU 系トークンを含む宣言 | **1 件**（`r-efi 6.0.0`、MIT の選択肢あり） |
| MPL 宣言 | **1 件**（`smartstring 1.0.1`） |

14 件は `MIT/Apache-2.0`、`Unlicense/MIT` などの slash を使う宣言である。スクリプトはこれらを推測で `OR` に変換せず、`ambiguous` として原文を保持する。184 件の review reason には同梱本文の対応記録がないことや、この構文上の限定も含む。これは 184 件が使用不可という判定ではない。件数は本体を含む metadata 全体に対する値であり、ライセンスの自動承認を意味しない。

226 ファイルの照合対象は、既存の Windows 台帳 114 package と Rhai 関連7件・r-efi 1件を含む追加台帳8 packageが指す Git 追跡済み本文である。名前・バージョン・registry checksum で対応を確かめ、実際の本文を SHA-256 と比較した。**registry archive 全体の再ハッシュ、各ターゲットへのリンク状況、最終配布物のランタイム監査はこの処理に含まれない。**

`r-efi` の MIT notice は追補コミットを統合した上で、今回の通常CIにより再照合された。以前の診断run 45846736773は225ファイル / 121 packageであり、現在の226 / 122は手作業の加算ではなく再生成された実測値である。

## 実行環境中断後の回収状態（履歴）

ローカル実行環境が unavailable となり、作成済みの完全な inventory JSON と Cargo cache を読み出せなくなった時点では、GitHub に保存されていた監査、実際の検査結果、記録済みコミットから再取得した公式ファイルを使って**回収版**を作成した。[dependency-inventory.json](development/dependency-inventory.json) はその履歴を保持するファイルであり、今回の実測 JSON で上書きしていない。失われた JSON と同一のファイルであるとは主張しない。

| 項目 | 回収版の状態 |
|---|---:|
| Cargo.lock 全パッケージ | 303（本体 1、第三者 302） |
| 名前・バージョン・registry checksum を保存 | 第三者 302 件すべて |
| 同一 checksum の既存監査からライセンス宣言を回収 | 114 件 |
| 記録済み公式コミットから manifest を再取得 | Rhai 関連 7 件 |
| 中断前の実際のツール出力から宣言を回収 | 5 件 |
| 回収ファイルに宣言を保存できた合計 | **126 件** |
| 中断前には調査したが詳細な manifest 記録を回収できない | 80 件 |
| 中断前から正確なバージョンの metadata が未取得 | 96 件 |

当時回収できた 126 件の宣言には GPL / LGPL / AGPL はなかった。これは当時の部分集合についての記録であり、現在の全宣言に GNU 系トークンがないという意味ではない。最新の CI では残る宣言も取得し、`r-efi` の選択肢に `LGPL-2.1-or-later` が含まれることを確認している。既存依存 `smartstring 1.0.1` の `MPL-2.0+` も引き続き明示する。

この一覧は **lock 全体**の記録であり、optional・別 OS 用の依存も含む。現在の特定ターゲットで使われる dependency graph や、全項目が実行ファイルに含まれるかを示すものではない。

### 中断前に実行して報告した検査

ローカル検査では 206 件の宣言を確認した。内訳は archive SHA-256 と manifest bytes を照合した 198 件、既存監査の同一 checksum を照合した 8 件だった。未取得は 96 件であり、既存 114 package の同梱本文 212 ファイルは監査 SHA-256 と一致した。

これらは中断前の実測結果である。今回の回収で 198 archive を再検証したという意味ではない。126 件以外の宣言を推測で埋めたり、中断前の件数を現在の回収済み件数に読み替えたりしていない。

## 機械可読 inventory の読み方

| ファイル | 役割 |
|---|---|
| [dependency-manifests.json](development/dependency-manifests.json) | 2f94df4の通常Linux CIで取得した303 packageの宣言と226本文の照合結果。`r-efi` notice追補を含む |
| [dependency-inventory.json](development/dependency-inventory.json) | 中断後の回収履歴。第三者 302 件の名前・version・checksum と、その時点で回収できた宣言を保持 |
| [additional-dependency-audit.json](../licenses/additional-dependency-audit.json) | Rhai 関連の取得記録と、CI 後に保存した `r-efi` の MIT 選択・notice の追補 |

新しい一覧は宣言の原文、演算子構造の限定的な解析、license-file の宣言、同梱本文の照合結果を分けて記録する。SPDX 識別子全体や例外の適用可能性を検証するものではない。構文上の `OR` 選択肢の存在と法的な使用可否を同一視しない。再生成方法と制限は [DEPENDENCY_METADATA.md](development/DEPENDENCY_METADATA.md) にある。

以下は旧回収 JSON の区分である。旧ファイルの `declared_license: null` は**その時点の**詳細未回収または未調査を表し、現在の metadata に宣言がないという意味ではない。

| metadata_status | 件数 | 意味 |
|---|---:|---|
| `prior_audit_matching_checksum` | 114 | GitHub に残っている既存監査と name / version / checksum が一致 |
| `upstream_commit_recovered` | 7 | 中断前に記録した VCS commit の公式 manifest と本文を再取得 |
| `pre_outage_tool_output_recovered` | 5 | 中断前の出力に正確な package / version / license 宣言が残っている |
| `pre_outage_manifest_detail_unrecovered` | 80 | 以前の検査対象だが詳細記録が現在読めない。再取得が必要 |
| `previously_unavailable` | 96 | 中断前から exact-version metadata が未取得 |

5 件の履歴由来の宣言は `hexf-parse 0.2.1`、`linux-raw-sys 0.4.15 / 0.12.1`、`rustix 0.38.44 / 1.1.6`。これらも fresh manifest read として扱わない。

Cargo manifest の license フィールドは上流作者の宣言であり、本文、著作権表示、使用機能、コンパイラ・OS ランタイムの検査を代替しない。仕様は [Cargo の license / license-file](https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields) と [cargo metadata](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html) を参照する。

## 直接依存

バージョンと機能フラグは [Cargo.lock](../Cargo.lock) / [Cargo.toml](../Cargo.toml) に基づく。Rust 最低要件は中断前に読んだ正確なバージョンの manifest の記録であり、最低バージョンで実ビルドした結果ではない。

| パッケージ | 用途・明示機能 | license 宣言 | manifest の Rust 要件 |
|---|---|---|---|
| [eframe 0.33.3](https://crates.io/crates/eframe/0.33.3) | GUI。default features 無効、`default_fonts, glow, x11` | `MIT OR Apache-2.0` | 1.88 |
| [image 0.25.10](https://crates.io/crates/image/0.25.10) | 画像。default features 無効、`png` | `MIT OR Apache-2.0` | 1.88.0 |
| [rhai 1.27.0](https://crates.io/crates/rhai/1.27.0) | スクリプト。default features 無効、`std, no_module, no_time, no_custom_syntax` | `MIT OR Apache-2.0` | 1.66.0 |
| [serde 1.0.229](https://crates.io/crates/serde/1.0.229) | 保存データ。`derive, rc` | `MIT OR Apache-2.0` | 1.56 |
| [serde_json 1.0.154](https://crates.io/crates/serde_json/1.0.154) | JSON 保存形式 | `MIT OR Apache-2.0` | 1.71 |
| [xml-rs 0.8.29](https://crates.io/crates/xml-rs/0.8.29) | 公開 XML に基づく GHX 読み書き | `MIT` | 1.70 |

スクリプトの機能フラグだけで安全性を証明したとは扱わない。実行制限と API 登録方針は [ARCHITECTURE.md](ARCHITECTURE.md) と実装で管理する。

## Rhai 関連 7 件の本文・出典

7 package の 13 本文・著作権ファイルと SOURCE.txt を保存した。出典 URL、上流 Git blob SHA、保存したテキストの SHA-256 は [additional-dependency-audit.json](../licenses/additional-dependency-audit.json) にある。

| パッケージ | 宣言 | 本文・ソースの記録 |
|---|---|---|
| `rhai 1.27.0` | MIT OR Apache-2.0 | [SOURCE.txt](../licenses/crates/rhai-1.27.0/SOURCE.txt) |
| `rhai_codegen 3.3.0` | MIT OR Apache-2.0 | [SOURCE.txt](../licenses/crates/rhai_codegen-3.3.0/SOURCE.txt) |
| `thin-vec 0.2.21` | MIT OR Apache-2.0 | [SOURCE.txt](../licenses/crates/thin-vec-0.2.21/SOURCE.txt) |
| `const-random 0.1.20` | MIT OR Apache-2.0 | [SOURCE.txt](../licenses/crates/const-random-0.1.20/SOURCE.txt) |
| `const-random-macro 0.1.18` | MIT OR Apache-2.0 | [SOURCE.txt](../licenses/crates/const-random-macro-0.1.18/SOURCE.txt) |
| `tiny-keccak 2.0.2` | CC0-1.0 | [SOURCE.txt](../licenses/crates/tiny-keccak-2.0.2/SOURCE.txt) |
| `smartstring 1.0.1` | MPL-2.0+ | [SOURCE.txt](../licenses/crates/smartstring-1.0.1/SOURCE.txt) |

今回の回収では公式 repository の固定コミットから本文を取得した。crate archive 自体は再取得していないため、すべての本文が失われたローカルファイルと byte 単位で同じだとは扱わない。Rhai の MIT / Apache 本文は、中断前の LF 版 SHA-256 と一致した。

`rhai_codegen` の crate VCS metadata は中断前に `dirty: true` を報告していた。その事実を追加監査に残した。記録されたコミットの root license 本文を補っているが、公開 crate の全 source とその commit の完全一致を意味しない。

### smartstring の MPL

既存依存 smartstring の宣言は `MPL-2.0+`、同梱本文は MPL 2.0 である。NODES は今回そのソースを変更していない。完全なソースは [registry archive](https://static.crates.io/crates/smartstring/smartstring-1.0.1.crate) と [記録された公式コミット](https://github.com/bodil/smartstring/tree/e407ca23c747257a812d2d2e70bf336412718c3a) から入手でき、Cargo.lock の checksum を SOURCE.txt に保存した。

[MPL 2.0 第 3.2 節](https://www.mozilla.org/en-US/MPL/2.0/#distribution-of-executable-form) は実行形式を配布する場合のソース提供と入手方法の通知を規定している。配布担当者は、そのバージョンと変更内容に対応したソース・通知を維持する。MPL を本体の MIT / Apache-2.0 に読み替えるものではない。

### r-efi 6.0.0 の MIT 選択（追補を通常CIで照合済み）

`r-efi 6.0.0` の宣言は `MIT OR Apache-2.0 OR LGPL-2.1-or-later`。この既存依存について **MIT の選択肢を明示的に選ぶ**。GNU 系トークンの存在だけで LGPL の採用や必須条件と判断せず、元の宣言もそのまま記録する。

[追補コミット 3ed93188](https://github.com/07egta/NODES/commit/3ed93188c6eb56f0bb5c8f9f1e5246e7a6fb1a6f) は公式 `v6.0.0` タグと、その対象コミット `7e1b0322d31d625f81a5656096330934f9cd835d` の [Cargo.toml](https://github.com/r-efi/r-efi/blob/7e1b0322d31d625f81a5656096330934f9cd835d/Cargo.toml) を照合した。MIT 本文と著作権表示を含む [公式 AUTHORS](https://github.com/r-efi/r-efi/blob/7e1b0322d31d625f81a5656096330934f9cd835d/AUTHORS) をそのまま保存し、追加台帳の既存 7 件を変更せず 1 件を追記した。

- 保存先: `licenses/crates/r-efi-6.0.0/AUTHORS`、出典: `SOURCE.txt`。
- 本文サイズ: 3,733 UTF-8 bytes。
- Git blob: `79c8b7c4a1dabcfbc718b7fe6d43b1892f9850cd`（公式ソースと一致）。
- SHA-256: `d027e91dbc9cdbb2f1190068e498bd6b61cff022b6a032b191021ba658d96111`。

この追補は旧run 45846736773の `225 ファイル / 121 package` に含まれず、最新の通常run 45852634502の `226 / 122` で照合された。公式タグ・manifest・notice の照合を行ったもので、registry archive の再取得や SHA-256 再計算は実施していない。全依存・配布物の監査完了を意味しない。

### フォントとランタイム

`epaint_default_fonts 0.33.3` の宣言は `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`。Morphlet JP と egui 同梱フォントの個別表示は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md#フォント) と `licenses/` に残す。

既存の Windows 監査と MinGW runtime 通知は **x86_64-pc-windows-gnullvm の過去の配布物**を対象とする。新しい CI artifact は Windows MSVC を使うため、古い監査をその実行ファイル全体の確認結果として扱わない。今回の全 package の宣言取得は、Windows MSVC / Linux の実際のリンク対象、OS・コンパイラの runtime、各 artifact の配布条件・同梱義務の監査を代替しない。

## CI と Rust

[Integration recovery diagnostics](../.github/workflows/integration-recovery.yml) の run 45846736773 は、Linux 上で formatter / Clippy による修正を適用した作業ツリーを検証し、Rust テスト **431 passed / 0 failed / 2 ignored**、`cargo check --all-targets --locked`、fmt、strict Clippy（`-D warnings`）に成功した。依存監査・registry 生成・性能測定の step も成功し、その出力を回収した。2 件の ignored test は成功件数に含めていない。

この診断runは修正済み作業ツリーに対する歴史的結果である。その後、2f94df4の未変更ソースは [通常の Check workflow](../.github/workflows/windows.yml) のLinuxで433 Rust tests、17 Python tests、fmt/check/strict Clippy、GUI/CLI release buildを通過した。Windowsの完了結果はTEST_RESULTS.mdで別途記録する。

| Job | 環境・制限 | 内容 |
|---|---|---|
| `linux` | Ubuntu 24.04、Rust 1.100.0、2 build jobs、35 分上限 | fmt / check / strict clippy / all-targets test、監査スクリプト・依存宣言の検証、GUI と CLI の release build、開発用 artifact |
| `windows` | Linux 通過後、Windows Server 2022 x64 / MSVC、Rust 1.100.0、45 分上限 | all-targets test、GUI と CLI の release build、開発用 artifact（14 日保持） |

PR、main / evolve / develop ブランチの push、手動実行に対応。使用 Actions は具体的 commit SHA に固定し、権限は `contents: read`。古い同一ブランチ実行をキャンセルし、公開 release は自動作成しない。

本体の最低 Rust 宣言は **1.88**。中断前に確認した 198 manifest の最大宣言値も 1.88 / 1.88.0 だったが、Rust 1.88 での実ビルド検証はしていない。CI は [公式 Rust 1.100.0](https://github.com/rust-lang/rust/releases/tag/1.100.0) を明示する。`rust-toolchain.toml` は追加していない。

workflow の YAML 構造と必須 step の失敗・skip が成功扱いされないことを確認している。成功 CI の記録には回収した出力の SHA-256 と検証範囲を残す。通常CIの結果は診断runと分けて記録する。Windows CI が成功しても、Windows 11 GUI 操作、Intel Arc 描画、3D プリント実機の検証を代替しない。

## 再開する作業

1. Cargo.lock・notice・台帳が変わった際はmanifest inventoryを再生成し、source SHAとartifactの出典を更新する。
2. 14 件の構文上の未解釈宣言と、本文記録がない package を exact-version の公式資料で確認する。184 件の review reason を個別の意味に沿って処理する。
3. 必要な exact-version crate archive を取得し、archive bytes の SHA-256 と Cargo.lock を独立に照合する。
4. Windows MSVC と Linux の実際の dependency graph、最終 artifact の runtime、フォント、選択機能と必要な通知を確認する。
5. source変更時には通常CIを継続する。ターゲットごとの検証結果を保存し、古い件数を流用しない。

再生成には **`--filter-platform` を付けない** metadata を使う。監査スクリプトは packages と resolution nodes が一致する完全な入力を要求する。ターゲット別の graph は別途保存・確認する。

```sh
python3 -B scripts/audit_dependencies.py --self-test
cargo metadata --locked --format-version 1 > /tmp/nodes-cargo-metadata.json
python3 -B scripts/audit_dependencies.py \
  --metadata /tmp/nodes-cargo-metadata.json \
  --output docs/development/dependency-manifests.json
cargo tree --locked --target x86_64-pc-windows-msvc --edges normal,build --prefix none
cargo tree --locked --target x86_64-unknown-linux-gnu --edges normal,build --prefix none
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```
