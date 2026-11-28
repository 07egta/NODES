# OMEGA 18: オフライン Cluster パッケージ v1

## 目的・作業範囲
既存の .morphpart をそのまま包み、作者・安定版番号・ライセンス・検索用タグを配布前に確認できるようにする。
main ec3b840e81c76aa7e426da5dc47b716ba975a98e を起点とし、既存の Cluster ID、ノード登録 enum、.morph/.morphpart、Rhai、Graph API v1 の評価権限は変更しない。
同時進行 PR #52/#53/#55/#57/#65/#80/#81/#83/#84 を確認。新規モジュールと CLI の追加に限定する。

## 使用方法

manifest.json の例:

```json
{
  "id": "studio.vase",
  "version": "1.0.0",
  "title": "花器の部品",
  "author": "Example studio",
  "description": "編集可能な花器部品",
  "license": "MIT",
  "tags": ["vase", "geometry"]
}
```

```text
nodes-cli pack-part manifest.json vase.morphpart vase.nodespkg
nodes-cli inspect-package vase.nodespkg
nodes-cli unpack-part vase.nodespkg restored.morphpart
```

既存の GUI の部品読み込みで restored.morphpart を使える。Rust API は Package::from_part、decode、encode、info、import_into、load、save、pack_files、unpack_part。import_into は既存の transactional importer で定義 ID を再採番し、インスタンス配置は従来の instantiate を使う。CLI は評価を行わない。

## 仕様と能力境界

- 単一の UTF-8 JSON envelope: format=nodes-cluster-package、schema_version=1、manifest、part_json（元の UTF-8 JSON 文字列）。ZIP 展開・実行可能ファイル・外部パス指定を持たない。
- ID は英小文字のドット区切り名前空間。version は stable SemVer の major.minor.patch のみ（pre-release/build/range は未対応）。
- ファイル上限: manifest 16 KiB、part 1 MiB、envelope 4 MiB。文字列、タグ数も制限。既存 Cluster の定義数、ノード数、入れ子深さ、循環検査を再利用する。
- 全定義（無効・未参照を含む）で Cluster 以外は Graph API の許可リストに含まれ、かつ script が空でなければならない。任意スクリプト、式、ファイル import、外部ネイティブコードをこの入口で許可しない。これは実行環境のサンドボックスや完全な安全性証明ではない。将来 Graph API の許可範囲を増やす際はこの境界も監査する。
- 読み込み・検査・取り出しでは評価やプラグインのロードをしない。通常ファイルを確認してから上限付きで読み込む（敵対的なローカルファイル差し替え競争までは防げない）。保存は既存の atomic_write。未知の形式・ノードは拒否し元ファイルを書き換えない。未知ノードの編集・再保存機能ではない。
- part_json の元バイト列を保つ。依存 Cluster は既存の .morphpart に同梱される。外部パッケージの依存解決、インストール DB、バージョン選択 UI は未実装。
- ライセンスは作者の申告であり、真正性や権利を保証しない。ハッシュ/署名/信頼ストアは未実装。既存 lockfile に SHA-256 crate がなく、未検証の暗号実装や手書き lockfile を避ける。
- GUI package picker、ギャラリー、サムネイル、SDK、WASM、権限付き Rhai、複数出力、未知ノードの無損失編集は今回の範囲外。グラフへ取り込むと package ID/version の追跡は残らない。

## 公開仕様の参考
- Rhino Yak manifest: https://developer.rhino3d.com/en/guides/yak/the-package-manifest/
- Rhino Yak local install: https://developer.rhino3d.com/guides/yak/yak-cli-reference/
- Houdini asset version and namespace: https://www.sidefx.com/docs/houdini/assets/namespaces.html
公開仕様の設計概念のみを参考にし、コードを流用していない。

## 検証・再開
- tests/node_package_tests.rs: バイト単位の往復、import ID 衝突回避、メタデータ、過大/破損/将来形式、未知ノード、循環、無効かつ未参照スクリプト拒否。
- ローカル環境に rustc/cargo がなく、GitHub への直接ネットワーク接続も不可。GitHub connector でコミットする。テストは追加したが実行済みとは主張しない。
- CI で cargo fmt --all --check、cargo clippy --all-targets --locked -- -D warnings、cargo test --all-targets --locked を確認し、失敗を修正する。CI 緑までマージしない。
- 次は GUI への package inspect/import、署名/ハッシュの依存監査、完全な SemVer/依存解決、ローカルライブラリを独立 PR に分けて実装する。
