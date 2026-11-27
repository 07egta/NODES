# Windows の極小角度境界 — 2026-11-27 JST

PR #18 の `aa6906574ba3a89919d80a8a778d71ac65cc3724` に対する
[CI run45917517285](https://github.com/07egta/NODES/actions/runs/45917517285) は
Linux が成功し、Windows の `vector_angle_regressions` で1件失敗した。
`u = f64::from_bits(1)` として、`[1.5, 3u, 0]` と `[1.5, 2u, 0]` の角度は
約 `(2/3)u`。元の回帰は丸め後のbits=1を要求するが、Windowsの出力はbits=0だった。
実際の[失敗ログ抜粋](windows-angle-portability/windows-ci-failure-excerpt.log)と
[対象SHA・job情報](windows-angle-portability/windows-ci-failure.json)を保存した。

## 判断の根拠

[Rustのf64文書](https://doc.rust-lang.org/std/primitive.f64.html)では `hypot` と
`atan2` の精度はプラットフォーム等に依存するとされる。
[MicrosoftのUCRT精度説明](https://learn.microsoft.com/en-us/cpp/c-runtime-library/floating-point-support?view=msvc-170)
も全入力に対する正しい丸めを保証していない。今回、Windows上の中間値は取得していないため、
`hypot`／`atan2` のどちらの段階かや、特定のFTZ設定を原因と断定しない。

期待値を弱めず、VectorAngleに限って次の数値経路を使う。

- 外積の最大絶対成分で割ってから二乗和と `sqrt` を取り、最後に尺度を戻す。
  二乗和は非ゼロなら1〜3になり、既存の方向スケーリングにより結果も有限。
  軸方向のノルムは単一の絶対成分そのものになる。
- 内積が正で、外積ノルム／内積が `2^-27` 以下なら、その比を返す。
  `0 ≤ t − atan(t) ≤ t^3/3` より、ここでの相対補正は `2^-54/3` 以下。
  最小角度をプラットフォームの `atan2` へ渡す必要がない。
- その他の角度は従来の `atan2`。一般のノルム、Unitize、Projectionの処理は変更しない。

これは全入力で正しく丸めた角度を保証する方法ではない。たとえば傾きが正確に
`1.5u` の場合は比の丸めと `atan` の厳密な丸めが異なり得る。今回の契約は
元の反例と追加の独立した軸方向の境界を保持し、通常の浮動小数点の限界を明記すること。

## 検証と再開

元のbits=1検査は変更しない。新しい1テストは `u`、`2u`、`3u`、最小normalの
直前・その値・直後を3軸と入力順の両方向で確認する。期待値に `atan2` を使わない。
実行した局所テストの出力は同じ証拠ディレクトリへ保存する。

CIのdebug/test情報量は、既にローカル検証と統合候補で用いた `DEBUG=0` の
profile設定にそろえる。テスト対象・assertion・release設定は維持する。
新しいWindows CIの成功は、実際に完了したrunを取得するまで未確認として扱う。
この修正より前の486件／統合候補567件のログを、新しいソースの検証とは呼ばない。
