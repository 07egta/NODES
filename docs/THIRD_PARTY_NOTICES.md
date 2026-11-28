# NODES 第三者ライセンスと著作権表示

確認日: 2026-11-26  
対象: Windows x64 (`x86_64-pc-windows-gnullvm`) / NODES 0.1.0

この文書の 114 件の監査と MinGW runtime 記録は、その時点の Windows gnullvm 配布物を対象とする。現在の Cargo.lock 全体、Windows MSVC CI、実行環境中断後の回収範囲は [DEPENDENCIES.md](DEPENDENCIES.md) を参照する。Rhai 関連 7 件の本文は記録済み公式コミットから回収し、旧監査とは分けて保存した。

## NODES 本体

NODES は、このプロジェクトのために新しく記述した独立した実装です。ただし 2026-11-27 から、厳密 NURBS / B-rep / T-spline のために SolidCraft（MIT OR Apache-2.0）の `geom` と `kernel` crate、および SolidCraft が修正して同梱する truck（Apache-2.0）を `crates/sc-*` と `vendor/` に取り込んでいます（下の「SolidCraft 由来のコード」と [SOLIDCRAFT_CHANGES.md](SOLIDCRAFT_CHANGES.md)）。Rhino / Grasshopper のコード・アイコン・画像を同梱していません。

NODES 本体のソースコードは **MIT または Apache-2.0** から選択できます。本文は [LICENSE-MIT](../LICENSE-MIT) と [LICENSE-APACHE](../LICENSE-APACHE) です。本体のライセンスは、下記の第三者コードやフォントの著作権・ライセンスを変更しません。

## 確認対象と結果

この監査時点の `Cargo.lock`（SHA-256: `96331253910df5b9537f9b79393a0378a58ea68373df42653f1c6138ae4bd5d5`）と有効な機能設定を使い、`cargo tree` でこのターゲットに有効な通常依存・ビルド依存を抽出し、`cargo metadata` でバージョン・ソース・ライセンスを突き合わせました。開発専用・非選択の optional 依存は除外しています。metadata の resolve グラフには追加の optional 依存も含まれたため、単純な全件集計を採用していません。

```sh
cargo tree --locked --offline --target x86_64-pc-windows-gnullvm --edges normal,build --prefix none
cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-gnullvm
```

- 第三者の Rust パッケージ: **114 件**（異なるバージョンは別件）。
- これらのパッケージの license 宣言に、GPL / LGPL / AGPL の指定はありません。
- ライセンス宣言の欠落: **0 件**。同梱するライセンス本文の未解決: **0 件**。
- `OR` は下表の「採用条件」を選択します。`AND` はすべての条件を維持します。旧形式の `/` は選択式として扱います。
- ビルド時に使うパッケージも含めた保守的な一覧であり、114 件すべての機械語が実行ファイルへ入ることを意味しません。

各フォルダのライセンス本文・著作権表示は原文のまま保存しています。crate に本文がない場合は、crate の `.cargo_vcs_info.json` に記録されたコミットの公式リポジトリから補いました。根拠 URL と crate の checksum は各フォルダの `SOURCE.txt`、全ファイルの SHA-256 は [監査データ](../licenses/windows-dependency-audit.json) にあります。

Cargo の情報形式: [公式 cargo metadata ドキュメント](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html)。依存のバージョンや機能設定を変えた場合は、この一覧を再生成してください。

## 追加済み依存の本文回収

次の7件は上記114件の確認後に追加された既存依存です（理由は [実装構成](ARCHITECTURE.md) の第10章）。2026-11-26 に本文を補完した後、実行環境が失われました。記録済み VCS commit の公式 repository から本文・manifest を再取得し、出典と保存本文の SHA-256 を [追加監査データ](../licenses/additional-dependency-audit.json) と各 package の SOURCE.txt に保存しました。失われた archive 全体との再照合、対象 OS の dependency graph と最終バイナリの再監査は行っていません。

| パッケージ | ライセンス宣言 |
|---|---|
| rhai 1.27.0 / rhai_codegen 3.3.0 | MIT OR Apache-2.0 |
| thin-vec 0.2.21 / const-random 0.1.20 / const-random-macro 0.1.18 | MIT OR Apache-2.0 |
| tiny-keccak 2.0.2 | CC0-1.0 |
| smartstring 1.0.1 | **MPL-2.0+**（ファイル単位の弱いコピーレフト。配布に含めるなら本文とソース入手先の表示が必要） |

## SolidCraft 由来のコード（2026-11-27 追加）

| 場所 | 出典 | ライセンス | 本文 |
|---|---|---|---|
| `crates/sc-geom`（package `solidcraft-geom`） | SolidCraft `crates/geom`（https://github.com/tangency-labs/solidcraft、owner の melt3D fork 経由） | MIT OR Apache-2.0、© SolidCraft contributors, © 07egta | `crates/sc-geom/LICENSE-MIT`, `LICENSE-APACHE` |
| `crates/sc-kernel`（package `solidcraft-kernel`） | SolidCraft `crates/kernel` | MIT OR Apache-2.0、同上 | `crates/sc-kernel/LICENSE-MIT`, `LICENSE-APACHE` |
| `crates/sc-sketch`（package `solidcraft-sketch`） | SolidCraft `crates/sketch`（文字の組版とフォントを除く） | MIT OR Apache-2.0、© SolidCraft contributors, © 07egta | `crates/sc-sketch/LICENSE-MIT`, `LICENSE-APACHE` |
| `vendor/truck-geometry` 0.5.0, `truck-meshalgo` 0.4.0, `truck-shapeops` 0.4.0, `truck-stepio` 0.3.0, `matext4cgmath` 0.1.0 | truck（https://github.com/ricosjp/truck）を SolidCraft が修正したもの。変更点は `vendor/README.md` | Apache-2.0、© RICOS Co. Ltd. | 各フォルダの `LICENSE` |

変更記録: [SOLIDCRAFT_CHANGES.md](SOLIDCRAFT_CHANGES.md)。GPL / LGPL / AGPL のコードは含めず、参照もしていません。

### この取り込みで Cargo.lock に増えた crates.io パッケージ

宣言は `cargo metadata`（2026-11-27）による。本文の同梱（`licenses/crates/`）はまだ行っておらず、次回の依存監査で補う。

| パッケージ | ライセンス宣言 |
|---|---|
| allocator-api2 0.2.22 | MIT OR Apache-2.0 |
| android_system_properties 0.1.6 | MIT OR Apache-2.0 |
| approx 0.4.0 | Apache-2.0 |
| array-macro 2.1.8 | MIT OR Apache-2.0 |
| branches 0.3.0 | MIT |
| cgmath 0.18.0 | Apache-2.0 |
| chrono 0.4.45 | MIT OR Apache-2.0 |
| convert_case 0.4.0 | MIT |
| crossbeam-deque 0.8.8 | MIT OR Apache-2.0 |
| crossbeam-epoch 0.9.21 | MIT OR Apache-2.0 |
| derive_more 0.99.20 | MIT |
| either 1.19.0 | MIT OR Apache-2.0 |
| getrandom 0.2.17 | MIT OR Apache-2.0 |
| iana-time-zone 0.1.65 | MIT OR Apache-2.0 |
| iana-time-zone-haiku 0.1.2 | MIT OR Apache-2.0 |
| itertools 0.13.0 | MIT OR Apache-2.0 |
| katexit 0.1.5 | MIT OR Apache-2.0 |
| matext4cgmath 0.1.0 | Apache-2.0 |
| num-complex 0.4.6 | MIT OR Apache-2.0 |
| ppv-lite86 0.2.21 | MIT OR Apache-2.0 |
| proc-macro-error 1.0.4 | MIT OR Apache-2.0 |
| proc-macro-error-attr 1.0.4 | MIT OR Apache-2.0 |
| rand 0.8.8 | MIT OR Apache-2.0 |
| rand_chacha 0.3.1 | MIT OR Apache-2.0 |
| rand_core 0.6.4 | MIT OR Apache-2.0 |
| rayon 1.12.0 | MIT OR Apache-2.0 |
| rayon-core 1.13.0 | MIT OR Apache-2.0 |
| rclite 0.2.8 | MIT OR Apache-2.0 |
| robust 1.2.0 | MIT OR Apache-2.0 |
| spade 2.15.1 | MIT OR Apache-2.0 |
| syn 1.0.109 | MIT OR Apache-2.0 |
| truck-base 0.5.0 | Apache-2.0 |
| truck-derivers 0.1.0 | Apache-2.0 |
| truck-geometry 0.5.0 | Apache-2.0 |
| truck-geotrait 0.4.0 | Apache-2.0 |
| truck-meshalgo 0.4.0 | Apache-2.0 |
| truck-modeling 0.6.0 | Apache-2.0 |
| truck-polymesh 0.6.0 | Apache-2.0 |
| truck-shapeops 0.4.0 | Apache-2.0 |
| truck-stepio 0.3.0 | Apache-2.0 |
| truck-topology 0.6.0 | Apache-2.0 |
| wasi 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| windows-core 0.62.2 | MIT OR Apache-2.0 |
| windows-implement 0.60.2 | MIT OR Apache-2.0 |
| windows-interface 0.59.3 | MIT OR Apache-2.0 |
| windows-result 0.4.1 | MIT OR Apache-2.0 |
| windows-strings 0.5.1 | MIT OR Apache-2.0 |

## フォント

### NODES JP

`assets/MorphletJP.ttf` は Google Fonts が配布する Noto Sans JP の可変フォントをウェイト 400 に固定し、内部名称を NODES JP へ変更したものです。元フォントの著作権表示は **Copyright 2014-2021 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'** です。フォント自体は **SIL Open Font License 1.1** のまま配布します。

本文: [Noto-Sans-JP-OFL.txt](../licenses/fonts/Noto-Sans-JP-OFL.txt)。アプリ側の同じ表示: [FONT-LICENSE.txt](../assets/FONT-LICENSE.txt)。[Google Fonts の Noto Sans JP 公式ライセンス](https://github.com/google/fonts/blob/main/ofl/notosansjp/OFL.txt)。フォント名の変更は本ソフト向けの変更を示すものであり、Adobe や Noto プロジェクトによる推奨を意味しません。

### egui の同梱フォント

`epaint_default_fonts` のコードは MIT を選択します。フォントは個別の条件を保持しています。

| フォント | 条件 | 原文 |
|---|---|---|
| Hack | MIT、Bitstream Vera、DejaVu の public-domain 表示 | [Hack-Regular.txt](../licenses/crates/epaint_default_fonts-0.33.3/fonts/Hack-Regular.txt) |
| Noto Emoji | OFL-1.1 | [OFL.txt](../licenses/crates/epaint_default_fonts-0.33.3/fonts/OFL.txt) |
| Ubuntu Light | Ubuntu Font Licence 1.0 | [UFL.txt](../licenses/crates/epaint_default_fonts-0.33.3/fonts/UFL.txt) |
| emoji-icon-font | MIT | [emoji-icon-font-mit-license.txt](../licenses/crates/epaint_default_fonts-0.33.3/fonts/emoji-icon-font-mit-license.txt) |

## Windows 実行ファイルのランタイム

Windows バイナリは Rust 1.100.0 と LLVM MinGW 20261117 UCRT、`-C target-feature=+crt-static` でビルドされています。配布にはコンパイラ本体や MinGW の開発ヘッダを含めません。

- [Rust 標準ライブラリの公式包括的著作権表示](../licenses/runtime/Rust-1.100.0-COPYRIGHT-library.html) をそのまま同梱しています。標準ライブラリの基本条件は MIT または Apache-2.0 で、本配布では MIT を選択し、個別の追加表示は保持します。
- [LLVM の Apache-2.0 + LLVM exception 等の原文](../licenses/runtime/LLVM-LICENSE.TXT) を同梱しています。
- 静的ランタイムのため、MinGW がバイナリ配布向けに提供している [COPYING.MinGW-w64-runtime.txt](../licenses/runtime/COPYING.MinGW-w64-runtime.txt) を省略せず同梱しています。[winpthreads](../licenses/runtime/COPYING.winpthreads.txt) と [winstorecompat](../licenses/runtime/COPYING.winstorecompat.txt) の表示も保持しています。

これらの上流包括文書には、他の OS 用の標準ライブラリ部品、任意のライセンス候補、Wine 由来のヘッダ・IDL に関する LGPL の説明なども含まれます。上記「GPL / LGPL / AGPL の指定なし」は **このターゲットの Cargo 依存 114 件の宣言の確認結果**です。包括文書内の説明を消して、実行ファイルの全由来に対する検査結果として読み替えることはしていません。

LLVM MinGW の公式配布: [mstorsjo/llvm-mingw release 20261117](https://github.com/mstorsjo/llvm-mingw/releases/tag/20261117)。使用 archive の SHA-256: `e7a15a49a2ea95e9f3d8a9d0cfabe5052a4b10fb4777cc54de5a8c27b64bb701`。

## Rust パッケージ一覧

| パッケージ | 上流の宣言 | 採用条件 | 本文・著作権表示 |
|---|---|---|---|
| [ab_glyph 0.2.32](https://crates.io/crates/ab_glyph/0.2.32) | `Apache-2.0` | `Apache-2.0` | [LICENSE](../licenses/crates/ab_glyph-0.2.32/LICENSE) |
| [ab_glyph_rasterizer 0.1.10](https://crates.io/crates/ab_glyph_rasterizer/0.1.10) | `Apache-2.0` | `Apache-2.0` | [LICENSE](../licenses/crates/ab_glyph_rasterizer-0.1.10/LICENSE) |
| [adler2 2.0.1](https://crates.io/crates/adler2/2.0.1) | `0BSD OR MIT OR Apache-2.0` | `MIT` | [LICENSE-0BSD](../licenses/crates/adler2-2.0.1/LICENSE-0BSD) · [LICENSE-APACHE](../licenses/crates/adler2-2.0.1/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/adler2-2.0.1/LICENSE-MIT) |
| [ahash 0.8.14](https://crates.io/crates/ahash/0.8.14) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/ahash-0.8.14/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/ahash-0.8.14/LICENSE-MIT) |
| [arboard 3.7.0](https://crates.io/crates/arboard/3.7.0) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE.txt](../licenses/crates/arboard-3.7.0/LICENSE-APACHE.txt) · [LICENSE-MIT.txt](../licenses/crates/arboard-3.7.0/LICENSE-MIT.txt) |
| [autocfg 1.5.1](https://crates.io/crates/autocfg/1.5.1) | `Apache-2.0 OR MIT` | `MIT` | [LICENSE-APACHE](../licenses/crates/autocfg-1.5.1/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/autocfg-1.5.1/LICENSE-MIT) |
| [bitflags 2.13.2](https://crates.io/crates/bitflags/2.13.2) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/bitflags-2.13.2/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/bitflags-2.13.2/LICENSE-MIT) |
| [bytemuck 1.25.2](https://crates.io/crates/bytemuck/1.25.2) | `Zlib OR Apache-2.0 OR MIT` | `MIT` | [LICENSE-APACHE](../licenses/crates/bytemuck-1.25.2/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/bytemuck-1.25.2/LICENSE-MIT) · [LICENSE-ZLIB](../licenses/crates/bytemuck-1.25.2/LICENSE-ZLIB) |
| [bytemuck_derive 1.12.1](https://crates.io/crates/bytemuck_derive/1.12.1) | `Zlib OR Apache-2.0 OR MIT` | `MIT` | [LICENSE-APACHE](../licenses/crates/bytemuck_derive-1.12.1/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/bytemuck_derive-1.12.1/LICENSE-MIT) · [LICENSE-ZLIB](../licenses/crates/bytemuck_derive-1.12.1/LICENSE-ZLIB) |
| [byteorder-lite 0.1.0](https://crates.io/crates/byteorder-lite/0.1.0) | `Unlicense OR MIT` | `MIT` | [LICENSE-MIT](../licenses/crates/byteorder-lite-0.1.0/LICENSE-MIT) · [UNLICENSE](../licenses/crates/byteorder-lite-0.1.0/UNLICENSE) |
| [cfg-if 1.0.5](https://crates.io/crates/cfg-if/1.0.5) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/cfg-if-1.0.5/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/cfg-if-1.0.5/LICENSE-MIT) |
| [cfg_aliases 0.2.2](https://crates.io/crates/cfg_aliases/0.2.2) | `MIT` | `MIT` | [LICENSE](../licenses/crates/cfg_aliases-0.2.2/LICENSE) |
| [clipboard-win 5.4.3](https://crates.io/crates/clipboard-win/5.4.3) | `BSL-1.0` | `BSL-1.0` | [LICENSE](../licenses/crates/clipboard-win-5.4.3/LICENSE) |
| [crc32fast 1.5.6](https://crates.io/crates/crc32fast/1.5.6) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/crc32fast-1.5.6/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/crc32fast-1.5.6/LICENSE-MIT) |
| [cursor-icon 1.2.0](https://crates.io/crates/cursor-icon/1.2.0) | `MIT OR Apache-2.0 OR Zlib` | `MIT` | [LICENSE-APACHE](../licenses/crates/cursor-icon-1.2.0/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/cursor-icon-1.2.0/LICENSE-MIT) · [LICENSE-ZLIB](../licenses/crates/cursor-icon-1.2.0/LICENSE-ZLIB) |
| [displaydoc 0.2.7](https://crates.io/crates/displaydoc/0.2.7) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/displaydoc-0.2.7/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/displaydoc-0.2.7/LICENSE-MIT) |
| [document-features 0.2.12](https://crates.io/crates/document-features/0.2.12) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/document-features-0.2.12/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/document-features-0.2.12/LICENSE-MIT) |
| [dpi 0.1.2](https://crates.io/crates/dpi/0.1.2) | `Apache-2.0 AND MIT` | `Apache-2.0 AND MIT` | [LICENSE](../licenses/crates/dpi-0.1.2/LICENSE) · [LICENSE-LIBM-MIT](../licenses/crates/dpi-0.1.2/LICENSE-LIBM-MIT) |
| [ecolor 0.33.3](https://crates.io/crates/ecolor/0.33.3) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/ecolor-0.33.3/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/ecolor-0.33.3/LICENSE-MIT) |
| [eframe 0.33.3](https://crates.io/crates/eframe/0.33.3) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/eframe-0.33.3/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/eframe-0.33.3/LICENSE-MIT) |
| [egui 0.33.3](https://crates.io/crates/egui/0.33.3) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/egui-0.33.3/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/egui-0.33.3/LICENSE-MIT) |
| [egui-winit 0.33.3](https://crates.io/crates/egui-winit/0.33.3) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/egui-winit-0.33.3/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/egui-winit-0.33.3/LICENSE-MIT) |
| [egui_glow 0.33.3](https://crates.io/crates/egui_glow/0.33.3) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/egui_glow-0.33.3/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/egui_glow-0.33.3/LICENSE-MIT) |
| [emath 0.33.3](https://crates.io/crates/emath/0.33.3) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/emath-0.33.3/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/emath-0.33.3/LICENSE-MIT) |
| [epaint 0.33.3](https://crates.io/crates/epaint/0.33.3) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/epaint-0.33.3/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/epaint-0.33.3/LICENSE-MIT) |
| [epaint_default_fonts 0.33.3](https://crates.io/crates/epaint_default_fonts/0.33.3) | `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` | `MIT AND OFL-1.1 AND Ubuntu-font-1.0` | [LICENSE-APACHE](../licenses/crates/epaint_default_fonts-0.33.3/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/epaint_default_fonts-0.33.3/LICENSE-MIT) · [Hack-Regular.txt](../licenses/crates/epaint_default_fonts-0.33.3/fonts/Hack-Regular.txt) · [OFL.txt](../licenses/crates/epaint_default_fonts-0.33.3/fonts/OFL.txt) · [UFL.txt](../licenses/crates/epaint_default_fonts-0.33.3/fonts/UFL.txt) · [emoji-icon-font-mit-license.txt](../licenses/crates/epaint_default_fonts-0.33.3/fonts/emoji-icon-font-mit-license.txt) |
| [error-code 3.4.0](https://crates.io/crates/error-code/3.4.0) | `BSL-1.0` | `BSL-1.0` | [LICENSE](../licenses/crates/error-code-3.4.0/LICENSE) |
| [fdeflate 0.3.7](https://crates.io/crates/fdeflate/0.3.7) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/fdeflate-0.3.7/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/fdeflate-0.3.7/LICENSE-MIT) |
| [flate2 1.1.11](https://crates.io/crates/flate2/1.1.11) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/flate2-1.1.11/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/flate2-1.1.11/LICENSE-MIT) |
| [form_urlencoded 1.2.3](https://crates.io/crates/form_urlencoded/1.2.3) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/form_urlencoded-1.2.3/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/form_urlencoded-1.2.3/LICENSE-MIT) |
| [gl_generator 0.14.0](https://crates.io/crates/gl_generator/0.14.0) | `Apache-2.0` | `Apache-2.0` | [LICENSE](../licenses/crates/gl_generator-0.14.0/LICENSE) |
| [glow 0.16.0](https://crates.io/crates/glow/0.16.0) | `MIT OR Apache-2.0 OR Zlib` | `MIT` | [LICENSE-APACHE](../licenses/crates/glow-0.16.0/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/glow-0.16.0/LICENSE-MIT) · [LICENSE-ZLIB](../licenses/crates/glow-0.16.0/LICENSE-ZLIB) |
| [glutin 0.32.3](https://crates.io/crates/glutin/0.32.3) | `Apache-2.0` | `Apache-2.0` | [LICENSE](../licenses/crates/glutin-0.32.3/LICENSE) |
| [glutin-winit 0.5.0](https://crates.io/crates/glutin-winit/0.5.0) | `MIT` | `MIT` | [LICENSE](../licenses/crates/glutin-winit-0.5.0/LICENSE) |
| [glutin_egl_sys 0.7.1](https://crates.io/crates/glutin_egl_sys/0.7.1) | `Apache-2.0` | `Apache-2.0` | [LICENSE](../licenses/crates/glutin_egl_sys-0.7.1/LICENSE) |
| [glutin_wgl_sys 0.6.1](https://crates.io/crates/glutin_wgl_sys/0.6.1) | `Apache-2.0` | `Apache-2.0` | [LICENSE](../licenses/crates/glutin_wgl_sys-0.6.1/LICENSE) |
| [icu_collections 2.4.0](https://crates.io/crates/icu_collections/2.4.0) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/icu_collections-2.4.0/LICENSE) |
| [icu_locale_core 2.4.0](https://crates.io/crates/icu_locale_core/2.4.0) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/icu_locale_core-2.4.0/LICENSE) |
| [icu_normalizer 2.4.0](https://crates.io/crates/icu_normalizer/2.4.0) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/icu_normalizer-2.4.0/LICENSE) |
| [icu_normalizer_data 2.4.0](https://crates.io/crates/icu_normalizer_data/2.4.0) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/icu_normalizer_data-2.4.0/LICENSE) |
| [icu_properties 2.4.0](https://crates.io/crates/icu_properties/2.4.0) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/icu_properties-2.4.0/LICENSE) |
| [icu_properties_data 2.4.0](https://crates.io/crates/icu_properties_data/2.4.0) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/icu_properties_data-2.4.0/LICENSE) |
| [icu_provider 2.4.0](https://crates.io/crates/icu_provider/2.4.0) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/icu_provider-2.4.0/LICENSE) |
| [idna 1.1.0](https://crates.io/crates/idna/1.1.0) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/idna-1.1.0/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/idna-1.1.0/LICENSE-MIT) |
| [idna_adapter 1.2.2](https://crates.io/crates/idna_adapter/1.2.2) | `Apache-2.0 OR MIT` | `MIT` | [LICENSE-APACHE](../licenses/crates/idna_adapter-1.2.2/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/idna_adapter-1.2.2/LICENSE-MIT) |
| [image 0.25.10](https://crates.io/crates/image/0.25.10) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/image-0.25.10/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/image-0.25.10/LICENSE-MIT) |
| [itoa 1.0.19](https://crates.io/crates/itoa/1.0.19) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/itoa-1.0.19/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/itoa-1.0.19/LICENSE-MIT) |
| [khronos_api 3.1.0](https://crates.io/crates/khronos_api/3.1.0) | `Apache-2.0` | `Apache-2.0` | [LICENSE](../licenses/crates/khronos_api-3.1.0/LICENSE) · [REGISTRY-NOTICES.txt](../licenses/crates/khronos_api-3.1.0/REGISTRY-NOTICES.txt) |
| [libloading 0.8.9](https://crates.io/crates/libloading/0.8.9) | `ISC` | `ISC` | [LICENSE](../licenses/crates/libloading-0.8.9/LICENSE) |
| [litemap 0.8.3](https://crates.io/crates/litemap/0.8.3) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/litemap-0.8.3/LICENSE) |
| [litrs 1.1.0](https://crates.io/crates/litrs/1.1.0) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/litrs-1.1.0/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/litrs-1.1.0/LICENSE-MIT) |
| [lock_api 0.4.14](https://crates.io/crates/lock_api/0.4.14) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/lock_api-0.4.14/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/lock_api-0.4.14/LICENSE-MIT) |
| [log 0.4.35](https://crates.io/crates/log/0.4.35) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/log-0.4.35/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/log-0.4.35/LICENSE-MIT) |
| [memchr 2.8.3](https://crates.io/crates/memchr/2.8.3) | `Unlicense OR MIT` | `MIT` | [COPYING](../licenses/crates/memchr-2.8.3/COPYING) · [LICENSE-MIT](../licenses/crates/memchr-2.8.3/LICENSE-MIT) · [UNLICENSE](../licenses/crates/memchr-2.8.3/UNLICENSE) |
| [memoffset 0.9.1](https://crates.io/crates/memoffset/0.9.1) | `MIT` | `MIT` | [LICENSE](../licenses/crates/memoffset-0.9.1/LICENSE) |
| [miniz_oxide 0.8.9](https://crates.io/crates/miniz_oxide/0.8.9) | `MIT OR Zlib OR Apache-2.0` | `MIT` | [LICENSE](../licenses/crates/miniz_oxide-0.8.9/LICENSE) · [LICENSE-APACHE.md](../licenses/crates/miniz_oxide-0.8.9/LICENSE-APACHE.md) · [LICENSE-MIT.md](../licenses/crates/miniz_oxide-0.8.9/LICENSE-MIT.md) · [LICENSE-ZLIB.md](../licenses/crates/miniz_oxide-0.8.9/LICENSE-ZLIB.md) |
| [miniz_oxide 0.9.1](https://crates.io/crates/miniz_oxide/0.9.1) | `MIT OR Zlib OR Apache-2.0` | `MIT` | [LICENSE](../licenses/crates/miniz_oxide-0.9.1/LICENSE) · [LICENSE-APACHE.md](../licenses/crates/miniz_oxide-0.9.1/LICENSE-APACHE.md) · [LICENSE-MIT.md](../licenses/crates/miniz_oxide-0.9.1/LICENSE-MIT.md) · [LICENSE-ZLIB.md](../licenses/crates/miniz_oxide-0.9.1/LICENSE-ZLIB.md) |
| [moxcms 0.8.1](https://crates.io/crates/moxcms/0.8.1) | `BSD-3-Clause OR Apache-2.0` | `Apache-2.0` | [LICENSE-APACHE.md](../licenses/crates/moxcms-0.8.1/LICENSE-APACHE.md) · [LICENSE.md](../licenses/crates/moxcms-0.8.1/LICENSE.md) |
| [nohash-hasher 0.2.0](https://crates.io/crates/nohash-hasher/0.2.0) | `Apache-2.0 OR MIT` | `MIT` | [LICENSE-APACHE](../licenses/crates/nohash-hasher-0.2.0/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/nohash-hasher-0.2.0/LICENSE-MIT) |
| [num-traits 0.2.19](https://crates.io/crates/num-traits/0.2.19) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/num-traits-0.2.19/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/num-traits-0.2.19/LICENSE-MIT) |
| [once_cell 1.21.4](https://crates.io/crates/once_cell/1.21.4) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/once_cell-1.21.4/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/once_cell-1.21.4/LICENSE-MIT) |
| [owned_ttf_parser 0.25.1](https://crates.io/crates/owned_ttf_parser/0.25.1) | `Apache-2.0` | `Apache-2.0` | [LICENSE](../licenses/crates/owned_ttf_parser-0.25.1/LICENSE) |
| [parking_lot 0.12.5](https://crates.io/crates/parking_lot/0.12.5) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/parking_lot-0.12.5/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/parking_lot-0.12.5/LICENSE-MIT) |
| [parking_lot_core 0.9.12](https://crates.io/crates/parking_lot_core/0.9.12) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/parking_lot_core-0.9.12/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/parking_lot_core-0.9.12/LICENSE-MIT) |
| [percent-encoding 2.3.2](https://crates.io/crates/percent-encoding/2.3.2) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/percent-encoding-2.3.2/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/percent-encoding-2.3.2/LICENSE-MIT) |
| [pin-project-lite 0.2.18](https://crates.io/crates/pin-project-lite/0.2.18) | `Apache-2.0 OR MIT` | `MIT` | [LICENSE-APACHE](../licenses/crates/pin-project-lite-0.2.18/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/pin-project-lite-0.2.18/LICENSE-MIT) |
| [png 0.18.1](https://crates.io/crates/png/0.18.1) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/png-0.18.1/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/png-0.18.1/LICENSE-MIT) |
| [potential_utf 0.1.6](https://crates.io/crates/potential_utf/0.1.6) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/potential_utf-0.1.6/LICENSE) |
| [proc-macro2 1.0.107](https://crates.io/crates/proc-macro2/1.0.107) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/proc-macro2-1.0.107/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/proc-macro2-1.0.107/LICENSE-MIT) |
| [profiling 1.0.18](https://crates.io/crates/profiling/1.0.18) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/profiling-1.0.18/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/profiling-1.0.18/LICENSE-MIT) |
| [pxfm 0.1.30](https://crates.io/crates/pxfm/0.1.30) | `BSD-3-Clause OR Apache-2.0` | `Apache-2.0` | [LICENSE-APACHE.md](../licenses/crates/pxfm-0.1.30/LICENSE-APACHE.md) · [LICENSE.md](../licenses/crates/pxfm-0.1.30/LICENSE.md) |
| [quote 1.0.47](https://crates.io/crates/quote/1.0.47) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/quote-1.0.47/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/quote-1.0.47/LICENSE-MIT) |
| [raw-window-handle 0.6.2](https://crates.io/crates/raw-window-handle/0.6.2) | `MIT OR Apache-2.0 OR Zlib` | `MIT` | [LICENSE-APACHE.md](../licenses/crates/raw-window-handle-0.6.2/LICENSE-APACHE.md) · [LICENSE-MIT.md](../licenses/crates/raw-window-handle-0.6.2/LICENSE-MIT.md) · [LICENSE-ZLIB.md](../licenses/crates/raw-window-handle-0.6.2/LICENSE-ZLIB.md) |
| [scopeguard 1.2.0](https://crates.io/crates/scopeguard/1.2.0) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/scopeguard-1.2.0/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/scopeguard-1.2.0/LICENSE-MIT) |
| [serde 1.0.229](https://crates.io/crates/serde/1.0.229) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/serde-1.0.229/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/serde-1.0.229/LICENSE-MIT) |
| [serde_core 1.0.229](https://crates.io/crates/serde_core/1.0.229) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/serde_core-1.0.229/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/serde_core-1.0.229/LICENSE-MIT) |
| [serde_derive 1.0.229](https://crates.io/crates/serde_derive/1.0.229) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/serde_derive-1.0.229/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/serde_derive-1.0.229/LICENSE-MIT) |
| [serde_json 1.0.154](https://crates.io/crates/serde_json/1.0.154) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/serde_json-1.0.154/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/serde_json-1.0.154/LICENSE-MIT) |
| [simd-adler32 0.3.10](https://crates.io/crates/simd-adler32/0.3.10) | `MIT` | `MIT` | [LICENSE.md](../licenses/crates/simd-adler32-0.3.10/LICENSE.md) |
| [smallvec 1.16.6](https://crates.io/crates/smallvec/1.16.6) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/smallvec-1.16.6/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/smallvec-1.16.6/LICENSE-MIT) |
| [smol_str 0.2.2](https://crates.io/crates/smol_str/0.2.2) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/smol_str-0.2.2/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/smol_str-0.2.2/LICENSE-MIT) |
| [stable_deref_trait 1.2.1](https://crates.io/crates/stable_deref_trait/1.2.1) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/stable_deref_trait-1.2.1/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/stable_deref_trait-1.2.1/LICENSE-MIT) |
| [static_assertions 1.1.0](https://crates.io/crates/static_assertions/1.1.0) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/static_assertions-1.1.0/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/static_assertions-1.1.0/LICENSE-MIT) |
| [syn 3.0.10](https://crates.io/crates/syn/3.0.10) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/syn-3.0.10/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/syn-3.0.10/LICENSE-MIT) |
| [synstructure 0.14.0](https://crates.io/crates/synstructure/0.14.0) | `MIT` | `MIT` | [LICENSE](../licenses/crates/synstructure-0.14.0/LICENSE) |
| [tinystr 0.8.5](https://crates.io/crates/tinystr/0.8.5) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/tinystr-0.8.5/LICENSE) |
| [tracing 0.1.44](https://crates.io/crates/tracing/0.1.44) | `MIT` | `MIT` | [LICENSE](../licenses/crates/tracing-0.1.44/LICENSE) |
| [tracing-core 0.1.36](https://crates.io/crates/tracing-core/0.1.36) | `MIT` | `MIT` | [LICENSE](../licenses/crates/tracing-core-0.1.36/LICENSE) · [LICENSE](../licenses/crates/tracing-core-0.1.36/src/spin/LICENSE) |
| [ttf-parser 0.25.1](https://crates.io/crates/ttf-parser/0.25.1) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/ttf-parser-0.25.1/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/ttf-parser-0.25.1/LICENSE-MIT) · [colr_1_LICENSE](../licenses/crates/ttf-parser-0.25.1/tests/fonts/colr_1_LICENSE) |
| [unicode-ident 1.0.27](https://crates.io/crates/unicode-ident/1.0.27) | `(MIT OR Apache-2.0) AND Unicode-3.0` | `MIT AND Unicode-3.0` | [LICENSE-APACHE](../licenses/crates/unicode-ident-1.0.27/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/unicode-ident-1.0.27/LICENSE-MIT) · [LICENSE-UNICODE](../licenses/crates/unicode-ident-1.0.27/LICENSE-UNICODE) |
| [unicode-segmentation 1.13.3](https://crates.io/crates/unicode-segmentation/1.13.3) | `MIT OR Apache-2.0` | `MIT` | [COPYRIGHT](../licenses/crates/unicode-segmentation-1.13.3/COPYRIGHT) · [LICENSE-APACHE](../licenses/crates/unicode-segmentation-1.13.3/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/unicode-segmentation-1.13.3/LICENSE-MIT) |
| [url 2.5.9](https://crates.io/crates/url/2.5.9) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/url-2.5.9/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/url-2.5.9/LICENSE-MIT) |
| [utf8_iter 1.1.0](https://crates.io/crates/utf8_iter/1.1.0) | `Apache-2.0 OR MIT` | `MIT` | [COPYRIGHT](../licenses/crates/utf8_iter-1.1.0/COPYRIGHT) · [LICENSE-APACHE](../licenses/crates/utf8_iter-1.1.0/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/utf8_iter-1.1.0/LICENSE-MIT) |
| [utf_types 1.0.0](https://crates.io/crates/utf_types/1.0.0) | `Apache-2.0 OR MIT` | `MIT` | [COPYRIGHT](../licenses/crates/utf_types-1.0.0/COPYRIGHT) · [LICENSE-APACHE](../licenses/crates/utf_types-1.0.0/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/utf_types-1.0.0/LICENSE-MIT) |
| [version_check 0.9.5](https://crates.io/crates/version_check/0.9.5) | `MIT/Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/version_check-0.9.5/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/version_check-0.9.5/LICENSE-MIT) |
| [web-time 1.1.0](https://crates.io/crates/web-time/1.1.0) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/web-time-1.1.0/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/web-time-1.1.0/LICENSE-MIT) |
| [webbrowser 1.2.7](https://crates.io/crates/webbrowser/1.2.7) | `MIT OR Apache-2.0` | `MIT` | [LICENSE-APACHE](../licenses/crates/webbrowser-1.2.7/LICENSE-APACHE) · [LICENSE-MIT](../licenses/crates/webbrowser-1.2.7/LICENSE-MIT) |
| [windows-link 0.2.1](https://crates.io/crates/windows-link/0.2.1) | `MIT OR Apache-2.0` | `MIT` | [license-apache-2.0](../licenses/crates/windows-link-0.2.1/license-apache-2.0) · [license-mit](../licenses/crates/windows-link-0.2.1/license-mit) |
| [windows-sys 0.52.0](https://crates.io/crates/windows-sys/0.52.0) | `MIT OR Apache-2.0` | `MIT` | [license-apache-2.0](../licenses/crates/windows-sys-0.52.0/license-apache-2.0) · [license-mit](../licenses/crates/windows-sys-0.52.0/license-mit) |
| [windows-sys 0.61.2](https://crates.io/crates/windows-sys/0.61.2) | `MIT OR Apache-2.0` | `MIT` | [license-apache-2.0](../licenses/crates/windows-sys-0.61.2/license-apache-2.0) · [license-mit](../licenses/crates/windows-sys-0.61.2/license-mit) |
| [windows-targets 0.52.6](https://crates.io/crates/windows-targets/0.52.6) | `MIT OR Apache-2.0` | `MIT` | [license-apache-2.0](../licenses/crates/windows-targets-0.52.6/license-apache-2.0) · [license-mit](../licenses/crates/windows-targets-0.52.6/license-mit) |
| [windows_x86_64_gnullvm 0.52.6](https://crates.io/crates/windows_x86_64_gnullvm/0.52.6) | `MIT OR Apache-2.0` | `MIT` | [license-apache-2.0](../licenses/crates/windows_x86_64_gnullvm-0.52.6/license-apache-2.0) · [license-mit](../licenses/crates/windows_x86_64_gnullvm-0.52.6/license-mit) |
| [winit 0.30.13](https://crates.io/crates/winit/0.30.13) | `Apache-2.0` | `Apache-2.0` | [LICENSE](../licenses/crates/winit-0.30.13/LICENSE) |
| [writeable 0.6.4](https://crates.io/crates/writeable/0.6.4) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/writeable-0.6.4/LICENSE) |
| [xml-rs 0.8.29](https://crates.io/crates/xml-rs/0.8.29) | `MIT` | `MIT` | [LICENSE](../licenses/crates/xml-rs-0.8.29/LICENSE) |
| [yoke 0.8.4](https://crates.io/crates/yoke/0.8.4) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/yoke-0.8.4/LICENSE) |
| [yoke-derive 0.8.4](https://crates.io/crates/yoke-derive/0.8.4) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/yoke-derive-0.8.4/LICENSE) |
| [zerocopy 0.8.66](https://crates.io/crates/zerocopy/0.8.66) | `BSD-2-Clause OR Apache-2.0 OR MIT` | `MIT` | [LICENSE-APACHE](../licenses/crates/zerocopy-0.8.66/LICENSE-APACHE) · [LICENSE-BSD](../licenses/crates/zerocopy-0.8.66/LICENSE-BSD) · [LICENSE-MIT](../licenses/crates/zerocopy-0.8.66/LICENSE-MIT) |
| [zerofrom 0.1.8](https://crates.io/crates/zerofrom/0.1.8) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/zerofrom-0.1.8/LICENSE) |
| [zerofrom-derive 0.1.8](https://crates.io/crates/zerofrom-derive/0.1.8) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/zerofrom-derive-0.1.8/LICENSE) |
| [zerotrie 0.2.5](https://crates.io/crates/zerotrie/0.2.5) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/zerotrie-0.2.5/LICENSE) |
| [zerovec 0.11.9](https://crates.io/crates/zerovec/0.11.9) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/zerovec-0.11.9/LICENSE) |
| [zerovec-derive 0.11.7](https://crates.io/crates/zerovec-derive/0.11.7) | `Unicode-3.0` | `Unicode-3.0` | [LICENSE](../licenses/crates/zerovec-derive-0.11.7/LICENSE) |
| [zmij 1.0.23](https://crates.io/crates/zmij/1.0.23) | `MIT` | `MIT` | [LICENSE-MIT](../licenses/crates/zmij-1.0.23/LICENSE-MIT) |

Cargo.lock SHA-256: `96331253910df5b9537f9b79393a0378a58ea68373df42653f1c6138ae4bd5d5`
