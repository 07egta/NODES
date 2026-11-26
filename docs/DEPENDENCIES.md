# 依存ライブラリ・ライセンス確認記録

確認日: **2026-11-26 UTC**

対象 Cargo.lock SHA-256: `39580f7254769a1ad65c48d05472dee4db82709713e9ba1dd8d633290f03f6ca`

本体の [MIT](../LICENSE-MIT) / [Apache-2.0](../LICENSE-APACHE) 方針は維持している。今回の CI・ライセンス作業では Cargo.toml、Cargo.lock、依存の機能フラグを変更していない。

## 実行環境中断後の回収状態

ローカル実行環境が unavailable となり、作成済みの完全な inventory JSON と Cargo cache を読み出せなくなった。この文書と [dependency-inventory.json](development/dependency-inventory.json) は、GitHub に保存されていた監査、会話に記録された実際の検査結果、記録済みコミットから再取得した公式ファイルを使った**回収版**である。失われた JSON と同一のファイルであるとは主張しない。

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

回収できた 126 件の宣言には GPL / LGPL / AGPL はなかった。残る 176 件、OS ランタイム、最終実行ファイル全体について、その不在を保証する記録ではない。既存依存 `smartstring 1.0.1` の `MPL-2.0+` は明示的に保持している。

この一覧は **lock 全体**の記録であり、optional・別 OS 用の依存も含む。現在の特定ターゲットで使われる dependency graph や、全項目が実行ファイルに含まれるかを示すものではない。

### 中断前に実行して報告した検査

ローカル検査では 206 件の宣言を確認した。内訳は archive SHA-256 と manifest bytes を照合した 198 件、既存監査の同一 checksum を照合した 8 件だった。未取得は 96 件であり、既存 114 package の同梱本文 212 ファイルは監査 SHA-256 と一致した。

これらは中断前の実測結果である。今回の回収で 198 archive を再検証したという意味ではない。126 件以外の宣言を推測で埋めたり、中断前の件数を現在の回収済み件数に読み替えたりしていない。

## 機械可読 inventory の読み方

全 302 件は [dependency-inventory.json](development/dependency-inventory.json) にある。`declared_license: null` は詳細未回収または未調査を表し、使用許可・拒否の判定ではない。

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

### フォントとランタイム

`epaint_default_fonts 0.33.3` の宣言は `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`。Morphlet JP と egui 同梱フォントの個別表示は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md#フォント) と `licenses/` に残す。

既存の Windows 監査と MinGW runtime 通知は **x86_64-pc-windows-gnullvm の過去の配布物**を対象とする。新しい CI artifact は Windows MSVC を使うため、古い監査をその実行ファイル全体の確認結果として扱わない。MSVC runtime の配布条件と、たとえば `windows_x86_64_msvc 0.52.6` の metadata は未確認である。

## CI と Rust

[実行用 workflow](../.github/workflows/windows.yml) は既存 Windows job を再利用し、Linux gate と Windows development artifact を 1 本にまとめる。回収時点では、この新 workflow 自体の実行結果はない。

| Job | 環境・制限 | 内容 |
|---|---|---|
| `linux` | Ubuntu 24.04、Rust 1.100.0、2 build jobs、35 分上限 | fmt / check / clippy（warnings を error）/ all-targets test |
| `windows` | Linux 通過後、Windows Server 2022 x64 / MSVC、Rust 1.100.0、45 分上限 | all-targets test、release build、開発用 artifact（14 日保持） |

PR、main / evolve / develop ブランチの push、手動実行に対応。使用 Actions は具体的 commit SHA に固定し、権限は `contents: read`。古い同一ブランチ実行をキャンセルし、公開 release は自動作成しない。

本体の最低 Rust 宣言は **1.88**。中断前に確認した 198 manifest の最大宣言値も 1.88 / 1.88.0 だったが、Rust 1.88 での実ビルド検証はしていない。CI は [公式 Rust 1.100.0](https://github.com/rust-lang/rust/releases/tag/1.100.0) を明示する。`rust-toolchain.toml` は追加していない。

中断前には workflow の YAML 構造、action SHA、入力、diff を確認した。復元時には GitHub へ保存したテキストを読み戻し、送信した内容と一致することを確認した。復元後の Cargo build / test は実行できていない。Windows CI の成功は Windows 11 GUI 操作、Intel Arc 描画、3D プリント実機の検証を代替しない。

## 再開する作業

1. 利用可能になった環境で exact-version crate archive を取得し、SHA-256 と Cargo.lock を照合する。
2. 80 件の未回収詳細と 96 件の未取得 metadata を補完する。全 302 件の名前・version・checksum は回収済み JSON を使う。
3. Windows MSVC と Linux の実際の dependency graph を保存し、必要な本文・著作権表示を抽出する。
4. 最終 artifact の MSVC runtime、フォント、選択機能の通知を確認する。
5. Cargo.lock が変わったら一覧を再生成し、古い件数を流用しない。

次は実行予定コマンドであり、回収中には実行していない。

```sh
cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc
cargo tree --locked --target x86_64-pc-windows-msvc --edges normal,build --prefix none
cargo tree --locked --target x86_64-unknown-linux-gnu --edges normal,build --prefix none
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```
