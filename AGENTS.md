# NODES development instructions

## Implementation language

The owner's decision, recorded on 2026-11-27 JST, is **Rust first, with a small
amount of other languages where needed**. Use [docs/LANGUAGE_POLICY.md](docs/LANGUAGE_POLICY.md)
as the common implementation-language policy for this repository.

- Implement the application, native UI, graph and Data Tree engine, standard
  nodes, geometry kernels, persistence, file I/O and CLI in Rust.
- Keep other languages limited to a specific supporting role. Existing Python
  developer utilities, PowerShell build helpers, CI commands and bounded Rhai
  user scripts are permitted. Keep the normal application usable without an
  external Python, Node.js or .NET runtime.
- Explain the need, Rust alternative, boundary and dependency impact of any new
  non-Rust implementation in the change. A small adapter is not authorization
  to move a whole core subsystem to another language.
- Preserve existing working functionality. Do not remove user scripting, rewrite
  useful tooling or alter saved projects simply to increase a language percentage.

## Project constraints

- Keep the desktop application independent of Rhino/Grasshopper, Windows first,
  lightweight and Japanese first. Continue the native Rust/eframe/egui UI.
- Preserve existing `.morph` / `.morphpart` files, serialized node IDs and port
  keys. Make necessary migrations explicit and verify their compatibility.
- Preserve the MIT OR Apache-2.0 project license and the policy against adopting
  GPL/LGPL/AGPL terms. Retain upstream notices and the exact license choices
  recorded in [docs/DEPENDENCIES.md](docs/DEPENDENCIES.md).
- Inspect the current branch, [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
  [docs/development/STATUS.md](docs/development/STATUS.md) before changing shared
  architecture. Coordinate changes to the graph model and persistence.
- Use focused development branches and pull requests. Do not push directly to
  `main`, force-push shared history or overwrite another developer's work.
- Record checks actually run and distinguish current results from historical
  evidence. Do not merge unverified changes or report unmeasured performance
  gains, untested compatibility or unfinished features as complete.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the current validation commands.

## GitHubへの作業記録

開発者、Codex、サブエージェント、ほかの自動化はこの手順に従います。

- 着手時に、対象ブランチと既存PRを確認します。進行中のPRまたは開発ブランチ内の作業記録に、目的、方針、対象範囲を書きます。サブエージェントを使う場合は、担当範囲と共有ファイルの調整役も記録します。
- 実装が一段落した時点や方針を変えた時点で、変更を開発ブランチに小さなコミットとして随時保存します。PR本文か作業記録にコミット、決定理由、実行済みの検証と結果、未確認事項、再開する人が次に行う作業を追記します。
- CIが進行中の場合や途中で作業を手放す場合も、最新の作業と状態をGitHubへ保存します。統合担当はサブエージェントの成果を検証し、公開用のブランチと作業記録に反映します。
- 次の担当は、関連PRと開発ブランチの最新コミット・検証記録を読み、既存作業を引き継ぎます。

## SolidCraftからの内部実装の再利用

SolidCraftの幾何処理など、NODESに必要な内部実装を再利用できます。
[既存のカーネル統合PR #48](https://github.com/07egta/NODES/pull/48) と、対象ブランチにある橋渡し実装を先に確認し、同じ機能の重複を避けます。取り込む際は、再利用した範囲、上流リビジョン、ライセンス、変更点、NODESで実行した検証をPRまたは開発資料に残します。

## オーナーからの常設指示（随時追記。指示を受けたらここに書く）

オーナーの指示はチャットだけに残さず、この節に日付つきで追記します。エージェントは作業前に読んでください。

- 2026-11-27: アプリ名は **NODES**（旧Morphlet）。目標はGrasshopper上位互換で、軽量・使いやすさ・日本語優先。NODES単体で設計から出力まで完結させる。
- 2026-11-27: Rustのみで実装する。デスクトップ専用（Web/HTMLにしない）。ライセンスは寛容なもののみ（GPL/LGPL/AGPLは採用せず、読まない）。
- 2026-11-27: 3Dプリンター向けGコード生成ノードをGrasshopperのGコード手法に倣って用意する。主な書き出しはSTLで、Gコードは補助機能。
- 2026-11-27: UIはmelt3D風の落ち着いたダークテーマ（色はテーマトークン経由）。アイコンは機能そのものを表す絵にし、文字で済ませない。
- 2026-11-27: NURBS/B-rep/T-splineはRhino並みを目指し、SolidCraftを使いやすく改造して使う（そのまま使わない）。速く、高精度で、機能を多く。T-splineは特許状況をdocsに記録し、SubD→NURBS経路を優先する。
- 2026-11-27: PRはmain土台で1テーマ1PR。CI（fmt・clippy -D warnings・test）が緑になったものからマージする。古い土台や重複するPRは理由を書いて閉じる。
- 2026-11-27: 進捗の報告は簡単な日本語で。前置きは最小限にし、作れるものから先に作る。
- 2026-11-27: Arclet CADは対象外（中止）。
- 2026-11-28: 操作や仕組みは「Grasshopperと同じ仕組みに似せる」。データ照合・ツリー・入力の右クリック・ワイヤ表示・ショートカットなどはGrasshopperの挙動に近づける（挙動だけを公開情報で調べ、コード・文章・アイコンは写さない）。調査表は [docs/GRASSHOPPER_MECHANICS.md](docs/GRASSHOPPER_MECHANICS.md)。
