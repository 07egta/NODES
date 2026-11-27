# Windows 版の実行時ランタイム

Windows MSVC 版の開発用 ZIP に含まれる `NODES.exe` と `nodes-cli.exe` は、Microsoft の `VCRUNTIME140.dll` を参照します。ZIP にこの DLL は同梱していません。対応するランタイムが入っていない PC では、起動前に **Microsoft Visual C++ v14 再頒布可能パッケージ（x64）** が必要です。

## 起動に必要な場合の手順

1. [Microsoft 公式の x64 パッケージ](https://aka.ms/vc14/vc_redist.x64.exe)をダウンロードします。配布先と対応条件は [Microsoft の公式説明](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)でも確認できます。
2. Microsoft のインストーラーの案内に従って導入します。再起動を求められた場合は、その案内に従ってください。
3. `NODES.exe` または `nodes-cli.exe` を開き直します。

`VCRUNTIME140.dll` が見つからないというエラーが出た場合も、上記の公式パッケージを利用してください。アプリを作成した MSVC Build Tools と同じか、それより新しいランタイムが必要です。公式の v14 x64 配布先は更新されるため、古い DLL を個別にコピーする方法は案内していません。

## 実行とビルドの違い

| 用途 | 確認するもの |
|---|---|
| 配布された exe を使う | 上記の実行時ランタイム。Rust や C++ Build Tools の導入は不要です。 |
| ソースからビルドする | Rust、Visual Studio C++ Build Tools、Windows SDK。[README のビルド手順](../README.md)を参照してください。 |

この ZIP にはランタイムの自動インストーラーを含めていません。

## 確認できた範囲

Windows Server 2022 の CI でビルドと自動テストを行い、ダウンロードした MSVC x64 配布物の PE ヘッダーと DLL 参照を確認しました。Windows 11 の新規環境での初回起動、Windows の実 GUI、Intel Arc GPU での表示は未検証です。DLL の参照を調べる検査だけでは、実行時に追加ロードされる依存関係のすべては確認できません。

対象コミット、配布物の hash、検証結果は [CI 実行記録](development/OMEGA_CI_RESULTS.json)に保存しています。

参考：Microsoft [必要な DLL の確認方法](https://learn.microsoft.com/en-us/cpp/windows/determining-which-dlls-to-redistribute)、[最新の Visual C++ 再頒布可能パッケージ](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)。
