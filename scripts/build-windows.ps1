#Requires -Version 5.1
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# This script uses the installed toolchain. It never installs software, downloads
# setup scripts, or changes PowerShell's execution policy.
if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    throw 'このスクリプトは Windows 上で実行してください。'
}

foreach ($toolName in @('cargo', 'rustc')) {
    if (-not (Get-Command $toolName -ErrorAction SilentlyContinue)) {
        throw "$toolName が見つかりません。Rust 1.100.0 と必要な C++ Build Tools を準備してください。"
    }
}

$projectDirectory = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path -LiteralPath (Join-Path $projectDirectory 'Cargo.toml') -PathType Leaf)) {
    throw 'ソースの Cargo.toml が見つかりません。scripts フォルダーを元の場所へ戻してください。'
}
if (-not (Test-Path -LiteralPath (Join-Path $projectDirectory 'Cargo.lock') -PathType Leaf)) {
    throw 'Cargo.lock がありません。同梱のソース一式を使用してください。'
}

$buildSucceeded = $false
Push-Location -LiteralPath $projectDirectory
try {
    Write-Host 'Morphlet: 使用する Rust を確認しています。'
    & rustc --version
    $versionExitCode = $LASTEXITCODE
    if ($versionExitCode -ne 0) {
        throw "rustc の確認に失敗しました (exit $versionExitCode)。"
    }
    Write-Host '検証済みの推奨ツールチェーン: Rust 1.100.0'

    Write-Host '1/2 テストを実行しています。'
    & cargo test --all-targets --locked
    $testExitCode = $LASTEXITCODE
    if ($testExitCode -ne 0) {
        throw "テストが失敗したため、ビルドを停止しました (exit $testExitCode)。"
    }

    Write-Host '2/2 リリース版をビルドしています。'
    & cargo build --release --locked --bins
    $buildExitCode = $LASTEXITCODE
    if ($buildExitCode -ne 0) {
        throw "リリースビルドに失敗しました (exit $buildExitCode)。"
    }

    $standardExecutable = Join-Path $projectDirectory 'target\release\Morphlet.exe'
    Write-Host 'テストとリリースビルドが完了しました。'
    if (Test-Path -LiteralPath $standardExecutable -PathType Leaf) {
        Write-Host "実行ファイル: $standardExecutable"
    } else {
        Write-Host 'Cargo の独自設定を使用している場合は、設定した生成先の Morphlet.exe を確認してください。'
    }
    $buildSucceeded = $true
}
catch {
    Write-Error -Message $_.Exception.Message -ErrorAction Continue
}
finally {
    Pop-Location
}

if (-not $buildSucceeded) {
    exit 1
}
exit 0
