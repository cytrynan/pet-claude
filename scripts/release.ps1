# 配布 zip を作る（release ビルド → 同梱物を dist/ に集める → zip → 展開して自己検査）。
#
# 使い方（リポジトリの直下で）:
#   pwsh scripts/release.ps1                      # dist/pet-claude-<版>-windows-x64.zip
#   pwsh scripts/release.ps1 -CopyTo "$env:USERPROFILE\Desktop"   # できた zip をそこへも置く
#   pwsh scripts/release.ps1 -SkipBuild           # ビルド済みの target/release を使う
#
# なぜ: 2026-10-28 K 指示「一旦ここでリリースバージョンとする。配布は zip ファイルで行う」。
# 同梱物の一覧（$Manifest）をここに固定し、zip を展開し直して ①一覧と一致 ②exe が CLI で動く
# ③`--install-hooks` が同梱の pet-hook.exe を指す、の 3 点を毎回機械で確かめる（「できました」でなく証拠）。
#
# 版は Cargo.toml の [workspace.package] version。同梱しないもの: 各ペットの SOURCE.md（元素材の在処は
# 作者の PC のパス）、AivisSpeech（別配布）、音声入力の Python 環境（setup-stt.ps1 が利用者の PC で作る）。

[CmdletBinding()]
param(
    [string]$CopyTo = '',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

$cargo = Get-Content (Join-Path $root 'Cargo.toml') -Raw
if ($cargo -notmatch '(?m)^\[workspace\.package\][^\[]*?^version\s*=\s*"([^"]+)"') { throw 'Cargo.toml の workspace.package.version が読めない' }
$version = $Matches[1]
$name = "pet-claude-$version"
$dist = Join-Path $root 'dist'
$stage = Join-Path $dist $name
$zip = Join-Path $dist "$name-windows-x64.zip"

if (-not $SkipBuild) {
    Write-Host "== cargo build --release ($version)"
    & cargo build --release -p pet-app -p pet-hook
    if ($LASTEXITCODE -ne 0) { throw "cargo build が失敗（exit $LASTEXITCODE）" }
}

# 同梱物: zip の中のパス → リポジトリの元。フォルダは中身を再帰で入れる（SOURCE.md は除く）
$Manifest = [ordered]@{
    'pet-app.exe'                      = 'target\release\pet-app.exe'
    'pet-hook.exe'                     = 'target\release\pet-hook.exe'
    'pet-claude.ico'                   = 'crates\pet-app\icons\icon.ico'
    'SETUP.md'                         = 'docs\release\SETUP.md'
    'README.md'                        = 'docs\release\README.md'
    'LICENSE'                          = 'LICENSE'
    'CHANGELOG.md'                     = 'CHANGELOG.md'
    'docs\usage.md'                    = 'docs\usage.md'
    'docs\pet-claude-manual.pdf'       = 'docs\manual\pet-claude-manual.pdf'
    'scripts\run-hidden.vbs'           = 'scripts\run-hidden.vbs'
    'scripts\setup-stt.ps1'            = 'scripts\setup-stt.ps1'
    'scripts\stt\transcribe.py'        = 'scripts\stt\transcribe.py'
    'pets\'                            = 'assets\pets'
}
$Exclude = @('SOURCE.md')

Write-Host "== stage: $stage"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
if (Test-Path $zip) { Remove-Item $zip -Force }
New-Item -ItemType Directory -Force $stage | Out-Null

foreach ($entry in $Manifest.GetEnumerator()) {
    $src = Join-Path $root $entry.Value
    $dst = Join-Path $stage $entry.Key
    if (-not (Test-Path $src)) { throw "同梱物が無い: $src" }
    if ($entry.Key.EndsWith('\')) {
        New-Item -ItemType Directory -Force $dst | Out-Null
        Get-ChildItem $src -Recurse -File | Where-Object { $Exclude -notcontains $_.Name } | ForEach-Object {
            $rel = $_.FullName.Substring($src.Length).TrimStart('\')
            $to = Join-Path $dst $rel
            New-Item -ItemType Directory -Force (Split-Path $to) | Out-Null
            Copy-Item $_.FullName $to
        }
    } else {
        New-Item -ItemType Directory -Force (Split-Path $dst) | Out-Null
        Copy-Item $src $dst
    }
}

# 期待する一覧（検査で zip の中身と突き合わせる）
$expected = Get-ChildItem $stage -Recurse -File | ForEach-Object { $_.FullName.Substring($stage.Length + 1) } | Sort-Object

Write-Host "== zip: $zip"
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -CompressionLevel Optimal

# ---- 自己検査: 展開し直して一覧・exe・hooks の 3 点 ----
Write-Host '== verify'
$check = Join-Path $dist "$name-check"
if (Test-Path $check) { Remove-Item $check -Recurse -Force }
Expand-Archive -Path $zip -DestinationPath $check
$actual = Get-ChildItem $check -Recurse -File | ForEach-Object { $_.FullName.Substring($check.Length + 1) } | Sort-Object
$diff = Compare-Object $expected $actual
if ($diff) { $diff | Format-Table | Out-String | Write-Host; throw '展開した中身が期待の一覧と違う' }
foreach ($must in @('pet-app.exe', 'pet-hook.exe', 'pet-claude.ico', 'SETUP.md', 'pets\mame-dragon\pet.json', 'scripts\run-hidden.vbs')) {
    if ($actual -notcontains $must) { throw "zip に $must が無い" }
}
if ($actual | Where-Object { $_ -like '*SOURCE.md' }) { throw 'SOURCE.md が混ざっている' }

# exe が CLI で動く（常駐のミューテックスは取らない経路）
$status = & (Join-Path $check 'pet-app.exe') --autostart-status 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) { throw "pet-app.exe --autostart-status が exit ${LASTEXITCODE}: $status" }
Write-Host "   --autostart-status: $($status.Trim())"

# --install-hooks が同梱の pet-hook.exe の絶対パスを書く（作業用の settings に対して）
$settings = Join-Path $check 'settings-check.json'
'{ "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "echo keep" } ] } ] } }' | Set-Content $settings -Encoding utf8
$line = & (Join-Path $check 'pet-app.exe') --install-hooks --settings $settings 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) { throw "--install-hooks が exit ${LASTEXITCODE}: $line" }
$json = Get-Content $settings -Raw | ConvertFrom-Json
$hookExe = Join-Path $check 'pet-hook.exe'
$cmds = @()
foreach ($ev in $json.hooks.PSObject.Properties) { foreach ($g in $ev.Value) { foreach ($h in $g.hooks) { $cmds += $h.command } } }
$mine = @($cmds | Where-Object { $_ -eq $hookExe })
if ($mine.Count -ne 7) { throw "pet-hook.exe の登録が 7 組でない（$($mine.Count)）: $($cmds -join '; ')" }
if ($cmds -notcontains 'echo keep') { throw '印の無い既存の hook が消えた' }
Write-Host "   --install-hooks: pet-hook.exe を 7 組、既存の hook は保持"

Remove-Item $check -Recurse -Force

$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
$sha = (Get-FileHash $zip -Algorithm SHA256).Hash
Write-Host "== OK  $zip  ($size MB)  SHA256 $sha"
Write-Host "   files: $($expected.Count)"

if ($CopyTo) {
    New-Item -ItemType Directory -Force $CopyTo | Out-Null
    Copy-Item $zip (Join-Path $CopyTo (Split-Path $zip -Leaf)) -Force
    Write-Host "   copied to $(Join-Path $CopyTo (Split-Path $zip -Leaf))"
}
