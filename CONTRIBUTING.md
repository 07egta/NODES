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
