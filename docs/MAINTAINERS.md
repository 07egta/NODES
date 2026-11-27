# メンテナー向け設定チェックリスト

リポジトリ `07egta/NODES` の推奨設定です。この文書は手順の記録であり、設定は自動では変更されません。各項目は GitHub の設定画面で手動で行ってください。

## ブランチ保護（`main`）

Settings → Branches → Add branch protection rule（対象: `main`）

- [ ] Require a pull request before merging（直接 push を禁止）
- [ ] Require status checks to pass before merging
  - 必須にするチェック: `linux`、`windows`（`.github/workflows/windows.yml` のジョブ名）
  - Require branches to be up to date before merging（推奨）
- [ ] Require conversation resolution before merging（推奨）
- [ ] Do not allow force pushes（強制 push を禁止）
- [ ] Do not allow deletions（`main` の削除を禁止）
- [ ] Include administrators（管理者にも適用するかは運用で判断）

## マージ方法

Settings → General → Pull Requests

- [ ] Allow squash merging を既定にする（Default commit message は PR タイトル）
- [ ] Allow merge commits と Allow rebase merging は必要に応じて無効化
- [ ] Automatically delete head branches を有効にする

## 権限・セキュリティ

- [ ] `.github/CODEOWNERS` の `* @07egta` が効いていることを PR 画面で確認する
- [ ] Settings → Code security で Dependabot alerts と Dependabot security updates を有効にする
- [ ] `.github/dependabot.yml`（cargo と github-actions、毎週）の PR が届くことを確認する
- [ ] Actions の権限は読み取り専用（`permissions: contents: read`）を既定にする

## ラベル

- [ ] `docs/LABELS.md` の一覧をリポジトリに作成する（色・説明は任意）
- [ ] 既定ラベルの `good first issue`、`help wanted` も存在することを確認する

## 公開情報

- [ ] 公開される名前・メールに本名や個人の連絡先を使わない（`07egta` 名義で運用）
- [ ] Issue テンプレートの注意書き（非公開の .morph や個人情報を貼らない）が守られているか時々確認する
