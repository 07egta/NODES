## 概要 / Summary

<!-- 何を、なぜ変えるかを2〜3行で。 / What changes and why, in 2-3 lines. -->

## 関連Issue / Related issue

<!-- 例: Closes #123。なければ「なし」。 / e.g. Closes #123. Write "none" if there is no issue. -->

## 変更点 / Changes

<!-- 変更内容を箇条書き。1つのPRには1つの目的だけを入れてください。 / Bullet list. Keep one topic per PR. -->

-

## 確認方法 / How to verify

### CI

- [ ] `Check`（linux / windows）がすべて成功している / All `Check` jobs (linux, windows) pass
- [ ] `cargo fmt --all -- --check`、`cargo clippy --all-targets --locked -- -D warnings`、`cargo test --all-targets --locked` がCIで通る / fmt, clippy with `-D warnings`, and tests pass in CI

### 手元で実行したこと（任意） / Local checks (optional)

<!-- 実行したコマンドと結果を書いてください。実行していないものは書かないでください。 / Record only what you actually ran. -->

```text
（例）cargo test --all-targets --locked: 合格 / passed
```

### スクショ・動作確認（画面や操作に関わる変更） / Screenshots (UI or behaviour changes)

<!-- 実際に起動して撮ったスクリーンショットや録画を貼ってください。操作手順も書くと助かります。 / Attach real screenshots or a recording taken from the app, plus the steps. -->

- 操作手順 / Steps:
- 画像 / Images:

## 既定値の変更 / Changes a default

- [ ] 既定値・既定の動作は変えていない / No default value or default behaviour changed
- [ ] 既定値を変えた。旧動作の残し方（設定など）と移行の影響を下に書いた / Default changed. Describe how the old behaviour is kept and the migration impact below.

<!-- 変更した場合の内容 / Details if changed: -->

## ライセンス確認 / License check

- [ ] GPL / LGPL / AGPL のコード・依存を追加していない / No GPL, LGPL or AGPL code or dependency added
- [ ] 他製品（Rhino / Grasshopper 等）の文言・アイコン・テンプレート・コードをコピーしていない / No text, icons, templates or code copied from other products (Rhino / Grasshopper, etc.)
- [ ] 依存を変えた場合、理由を書き、`Cargo.lock` は意図した依存更新だけを含む / If dependencies changed, the reason is given and `Cargo.lock` holds only the intended update
- [ ] 他言語（Python等）を新たに使う場合、理由・Rustで実装しない理由・接続箇所を書いた / If a non-Rust language is added, the reason, why Rust was not used, and the boundary are described

## AI生成 / AI-generated

- [ ] このPRはAIツールで作成・補助した / This PR was created or assisted by an AI tool
- ツール名 / Tool name（AI生成の場合のみ / only if AI-generated）:
- 人が確認したこと / What a human reviewed:

<!-- 注意: 非公開の個人情報、認証情報、個人用のファイルパスを含めないでください。 / Do not include private information, credentials or personal file paths. -->
