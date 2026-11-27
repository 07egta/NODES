# 開発への参加

NODESは、日本語を中心に使える独立したノード型モデリングアプリです。小さな修正や改善の提案を歓迎します。crate名と既存GUI実行ファイル名は互換性のため `morphlet` / `Morphlet` を維持しています。

## 実装言語

本体はRustで実装します。GUI、ノード評価、Data Tree、幾何処理、保存、CLIの新規実装・改善もRustで進めてください。他言語は必要な補助用途に限定し、既存の開発用Python/PowerShellと制限付きRhaiユーザースクリプトは維持します。

新しい例外では、用途・Rustで実装しない理由・接続箇所・実行依存と配布への影響をPRに記載してください。共通ルールは [実装言語方針](docs/LANGUAGE_POLICY.md)、エージェント向けの指示は [AGENTS.md](AGENTS.md) を参照してください。

## 変更前に

- 既存の構成と動作を確認し、変更の目的を短く説明してください。
- 画面に表示する新しい文言は、日本語を先に用意してください。英語表示を追加する場合も、既存の表現に合わせてください。
- 依存関係を変えるときは、その理由を説明してください。`Cargo.lock`は意図した依存更新と一緒に変更し、無関係な更新を含めないでください。

## 確認

[CI workflow](.github/workflows/windows.yml) と同じRustバージョンを使い、リポジトリのルートで次を実行してください。

```powershell
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked --bins
```

プルリクエストには、変更内容と実行した確認を簡潔に記載してください。画面や操作に関わる変更では、確認方法や画面例があるとレビューしやすくなります。

現在のLinux/Windows自動チェックは [.github/workflows/windows.yml](.github/workflows/windows.yml) で実行します。文書だけの変更ではリンクと差分を確認し、実行していないビルドやテストを成功扱いしないでください。`Cargo.toml`、`Cargo.lock`、ビルド設定、実行コードを変える場合は、影響する品質ゲートも実行してください。

## PRを受け付ける条件

次の条件を満たすPRを受け付けます。テンプレートは [PULL_REQUEST_TEMPLATE.md](.github/PULL_REQUEST_TEMPLATE.md) を使ってください。

- CI（`linux` と `windows`）がすべて成功している。`cargo clippy --all-targets --locked -- -D warnings` と `cargo test --all-targets --locked` を含む。
- 1つのPRには1つの目的だけを入れる。
- ベースブランチは統合後に `main` とする。統合前は現在の開発ブランチを指定する。
- 非テストコードに `unwrap`、`expect`、`panic` を入れない（レビューで確認）。
- 実装はRustのみ。他言語は [実装言語方針](docs/LANGUAGE_POLICY.md) の例外として理由を書く。
- デスクトップアプリ向けの変更のみ（Web版・モバイル版向けの変更は対象外）。
- GPL / LGPL / AGPL のコードや依存を入れない（[依存ライブラリ・ライセンス確認記録](docs/DEPENDENCIES.md) を参照）。
- 他製品（Rhino / Grasshopper 等）の文言・アイコン・テンプレート・コードをコピーしない。
- 回帰テストは既存のテストファイル（`tests/` 内の該当ファイル）に追加する。バグごとに新しいファイルを作らない。
- 画面や操作に関わる変更は、実際に起動して撮ったスクリーンショットか録画を添付する。
- 作業を始める前に、ドラフトPRで着手を知らせる。同じ内容の作業が重なった場合は、番号の小さい（先に作成された）PRを優先する。

AIツールで作成したPRも同じ条件です。PRテンプレートの「AI生成」欄に使ったツール名を書いてください。
