<#
.SYNOPSIS
  .steering/<YYYYMMDD>-<タスク名>/tasklist.md から GitHub issue（親1件＋子N件）を一括起票し、
  tasklist.md に issue 番号を書き戻す。

.DESCRIPTION
  - 親issue: "[tracking] <unit>" / ラベル tracking + task/<unit> / 本文に子issueのチェックリスト
  - 子issue: タスク名そのまま / ラベル task/<unit> / 本文に受入条件・対象・Part of #<親>
  - 書き戻し: 先頭に "tracking: #<親>"、各タスク行末に " (#<子>)"
  - 既に "(#NN)" が付いた行・"~~" でスキップ済みの行・コードフェンス内は対象外（再実行しても二重起票しない）

.EXAMPLE
  # 何が起票されるかだけ見る（gh の変更操作は一切しない）
  .\new-issues-from-tasklist.ps1 -TasklistPath .steering\20260728-在庫CSV取込\tasklist.md -DryRun

.EXAMPLE
  # 実際に起票（カレントが対象リポジトリでない場合は -Repo を指定）
  .\new-issues-from-tasklist.ps1 -TasklistPath .steering\20260728-在庫CSV取込\tasklist.md -Repo 6dorg/inventory_management_system
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$TasklistPath,
    [string]$Repo,
    [string]$Label,
    [switch]$DryRun,
    [int]$MaxTasks = 10,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

# ---------- 共通 ----------

function Write-Utf8NoBom([string]$Path, [string]$Text) {
    [System.IO.File]::WriteAllText($Path, $Text, (New-Object System.Text.UTF8Encoding($false)))
}

function Invoke-Gh([string[]]$GhArgs) {
    $full = @()
    if ($Repo) { $full += @('--repo', $Repo) }
    $full += $GhArgs
    $out = & gh @full 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "gh $($full -join ' ') が失敗しました: $out"
    }
    return (($out | ForEach-Object { $_.ToString() }) -join "`n").Trim()
}

function Get-IssueNumberFromUrl([string]$Text) {
    $m = [regex]::Match($Text, '/issues/(\d+)\s*$', 'Multiline')
    if (-not $m.Success) { $m = [regex]::Match($Text, '/issues/(\d+)') }
    if (-not $m.Success) { throw "gh の出力から issue 番号を取れませんでした: $Text" }
    return [int]$m.Groups[1].Value
}

# ---------- 前提チェック ----------

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw 'gh CLI が見つかりません。起票をスキップし、tasklist.md のみで進めてください。'
}
if (-not (Test-Path -LiteralPath $TasklistPath)) {
    throw "tasklist が見つかりません: $TasklistPath"
}

$tasklistFull = (Resolve-Path -LiteralPath $TasklistPath).Path
$unitDir = Split-Path -Parent $tasklistFull
$unit = Split-Path -Leaf $unitDir
if ($unit -notmatch '^\d{8}-') {
    Write-Warning "作業単位名が '<YYYYMMDD>-<タスク名>' 形式ではありません: $unit"
}
# GitHub のラベル名は 50 文字が上限。超えると label create が HTTP 422 で落ちる。
# -Label で明示指定されていればそれを使い、無ければ作業単位名から作って必要なら詰める。
if ($Label) {
    $label = $Label
} else {
    $label = "task/$unit"
}
if ($label.Length -gt 50) {
    $shortened = $label.Substring(0, 50).TrimEnd('-', '.', '/')
    Write-Warning "ラベル名が 50 文字を超えるため短縮します: $label -> $shortened"
    $label = $shortened
}

# リモートの存在確認（通らなければ起票しない）
# 注: gh repo view はリポジトリを位置引数で取る（--repo は無い）ため Invoke-Gh は使わない
Push-Location (Split-Path -Parent $unitDir)
try {
    if ($Repo) { $repoInfo = (& gh repo view $Repo --json nameWithOwner --jq .nameWithOwner) }
    else { $repoInfo = (& gh repo view --json nameWithOwner --jq .nameWithOwner) }
    if ($LASTEXITCODE -ne 0) { throw 'gh repo view が失敗しました' }
    $repoInfo = ($repoInfo | Out-String).Trim()
}
catch {
    throw "GitHub リモートを解決できません（$($_.Exception.Message)）。起票をスキップし、その旨を報告してください。"
}
finally {
    Pop-Location
}
Write-Host "対象リポジトリ: $repoInfo" -ForegroundColor Cyan
Write-Host "作業単位      : $unit" -ForegroundColor Cyan

# ---------- tasklist.md の解析 ----------

$lines = [System.IO.File]::ReadAllLines($tasklistFull, [System.Text.Encoding]::UTF8)

$existingParent = $null
$inFence = $false
$currentPhase = ''
$tasks = @()   # @{ Index; Title; Phase; Subtasks }

for ($i = 0; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]

    if ($line -match '^\s*```') { $inFence = -not $inFence; continue }
    if ($inFence) { continue }

    if ($line -match '^tracking:\s*#(\d+)') { $existingParent = [int]$Matches[1]; continue }
    # 計画エージェントが置くプレースホルダ "tracking: #TBD" は、起票後に親番号で置き換える（2 行目を増やさない）
    if ($line -match '^tracking:\s*#TBD\s*$') { $trackingPlaceholderIndex = $i; continue }
    if ($line -match '^##\s+(.+?)\s*$') { $currentPhase = $Matches[1]; continue }

    # トップレベルのタスク行（インデント無し）
    if ($line -match '^-\s\[( |~)\]\s+(.+?)\s*$') {
        $title = $Matches[2]
        # 行末の "(#TBD)"（計画段階のプレースホルダ）は issue タイトルに入れない。書き戻しでも実番号に置き換える
        $title = $title -replace '\s*\(#TBD\)\s*$', ''
        if ($title -match '~~') { continue }                 # スキップ済み
        # 既に起票済み: 行末の "(#NN)"、または "(#NN) — <メモ>" のように後ろに追記がある形
        # （行末だけを見ると、追記のある未完了行を再起票して二重になる。check-tasklist-issues.ps1 と同じ式）
        if ($title -match '\(#\d+\)(?=\s*$|\s+[—–-]\s)') { continue }
        if ($title -match '^\{.*\}$') { continue }            # テンプレのプレースホルダのみ

        # 直後の子行（インデント付きチェックボックス）を受入条件として集める
        $subs = @()
        for ($j = $i + 1; $j -lt $lines.Count; $j++) {
            if ($lines[$j] -match '^\s+-\s\[[ ~x]\]\s+(.+?)\s*$') { $subs += $Matches[1]; continue }
            if ($lines[$j] -match '^\s*$') { continue }
            break
        }

        $tasks += [pscustomobject]@{
            Index    = $i
            Title    = $title
            Phase    = $currentPhase
            Subtasks = $subs
        }
    }
}

if ($tasks.Count -eq 0) {
    Write-Host '起票対象のタスクがありません（すべて起票済み、またはタスク行が見つかりません）。' -ForegroundColor Yellow
    return
}

Write-Host "起票対象: $($tasks.Count) 件" -ForegroundColor Cyan
foreach ($t in $tasks) { Write-Host ("  - [{0}] {1}" -f $t.Phase, $t.Title) }

if ($tasks.Count -gt $MaxTasks -and -not $Force) {
    throw "起票対象が $($tasks.Count) 件で上限 $MaxTasks を超えています。タスクを階層化して親タスク単位にまとめるか、-Force を付けてください（細切れの大量起票は issue 一覧を読めなくします）。"
}

# ---------- DryRun ----------

if ($DryRun) {
    Write-Host ''
    Write-Host '[DryRun] 以下を実行します（変更はしません）' -ForegroundColor Yellow
    if (-not $existingParent) { Write-Host "  親issue: [tracking] $unit  (labels: tracking, $label)" }
    else { Write-Host "  親issue: 既存 #$existingParent を再利用" }
    foreach ($t in $tasks) { Write-Host "  子issue: $($t.Title)  (label: $label)" }
    Write-Host "  ラベル: $label / tracking / in-progress が無ければ作成"
    Write-Host "  書き戻し: $tasklistFull"
    return
}

# ---------- ラベル整備（存在しないと --label が失敗する） ----------

$existingLabels = (Invoke-Gh @('label', 'list', '--limit', '200', '--json', 'name', '--jq', '.[].name')) -split "`n" | ForEach-Object { $_.Trim() }
$wanted = @(
    @{ Name = $label;         Color = 'BFD4F2'; Desc = "作業単位 $unit のタスク" },
    @{ Name = 'tracking';     Color = '0E8A16'; Desc = '作業単位のトラッキングissue' },
    @{ Name = 'in-progress';  Color = 'FBCA04'; Desc = '着手中' }
)
foreach ($w in $wanted) {
    if ($existingLabels -notcontains $w.Name) {
        Invoke-Gh @('label', 'create', $w.Name, '--color', $w.Color, '--description', $w.Desc) | Out-Null
        Write-Host "ラベル作成: $($w.Name)" -ForegroundColor DarkGray
    }
}

# ---------- 親issue ----------

$tmpDir = Join-Path $env:TEMP ("tasklist-issues-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $tmpDir | Out-Null

try {
    $parent = $existingParent
    if (-not $parent) {
        $purpose = '（requirements.md を参照）'
        $reqPath = Join-Path $unitDir 'requirements.md'
        if (Test-Path -LiteralPath $reqPath) {
            $reqLine = (Get-Content -LiteralPath $reqPath -Encoding UTF8 | Where-Object { $_ -match '\S' -and $_ -notmatch '^#' } | Select-Object -First 1)
            if ($reqLine) { $purpose = $reqLine.Trim() }
        }
        $parentBody = @"
## 目的

$purpose

## ドキュメント（設計の正本）

- ``.steering/$unit/requirements.md``
- ``.steering/$unit/design.md``
- ``.steering/$unit/tasklist.md``

## タスク

（子issue起票後に更新されます）

---
このissueは ``tasklist.md`` から自動起票されました。状態の正は tasklist.md と本issue群を同期させて保ちます。
"@
        $parentBodyFile = Join-Path $tmpDir 'parent.md'
        Write-Utf8NoBom $parentBodyFile $parentBody
        $res = Invoke-Gh @('issue', 'create', '--title', "[tracking] $unit", '--label', 'tracking', '--label', $label, '--body-file', $parentBodyFile)
        $parent = Get-IssueNumberFromUrl $res
        Write-Host "親issue作成: #$parent" -ForegroundColor Green
    }
    else {
        Write-Host "親issue再利用: #$parent" -ForegroundColor Green
    }

    # ---------- 子issue ----------

    $created = @()
    foreach ($t in $tasks) {
        if ($t.Subtasks.Count -gt 0) {
            $accept = ($t.Subtasks | ForEach-Object { "- $_" }) -join "`n"
        }
        else {
            $accept = "- ``tasklist.md`` のタスク「$($t.Title)」が完了していること"
        }
        $childBody = @"
## 受入条件

$accept

## 対象

- フェーズ: $($t.Phase)
- 設計: ``.steering/$unit/design.md``
- タスクリスト: ``.steering/$unit/tasklist.md``

> issue本文は要約です。着手時は必ず ``tasklist.md`` と ``design.md`` を読んでください。

Part of #$parent
"@
        $childBodyFile = Join-Path $tmpDir ("child-{0}.md" -f $t.Index)
        Write-Utf8NoBom $childBodyFile $childBody
        $res = Invoke-Gh @('issue', 'create', '--title', $t.Title, '--label', $label, '--body-file', $childBodyFile)
        $num = Get-IssueNumberFromUrl $res
        $created += [pscustomobject]@{ Index = $t.Index; Number = $num; Title = $t.Title }
        Write-Host "  子issue作成: #$num $($t.Title)" -ForegroundColor Green
    }

    # ---------- tasklist.md へ書き戻し ----------

    $newLines = New-Object System.Collections.Generic.List[string]
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $hit = $created | Where-Object { $_.Index -eq $i }
        if ($hit) { $newLines.Add((($lines[$i] -replace '\s*\(#TBD\)\s*$', '').TrimEnd() + " (#$($hit.Number))")) }
        elseif ($null -ne $trackingPlaceholderIndex -and $i -eq $trackingPlaceholderIndex) { $newLines.Add("tracking: #$parent") }
        else { $newLines.Add($lines[$i]) }
    }
    if (-not $existingParent -and $null -eq $trackingPlaceholderIndex) {
        # タイトル行（先頭の # 行）の直後に tracking 行を差し込む
        $insertAt = 0
        for ($i = 0; $i -lt $newLines.Count; $i++) {
            if ($newLines[$i] -match '^#\s') { $insertAt = $i + 1; break }
        }
        $newLines.Insert($insertAt, '')
        $newLines.Insert($insertAt + 1, "tracking: #$parent")
    }
    Write-Utf8NoBom $tasklistFull (($newLines -join "`r`n") + "`r`n")
    Write-Host "tasklist.md を更新しました（tracking と issue 番号を反映）" -ForegroundColor Green

    # ---------- 親issue本文にチェックリストを反映 ----------

    $checklist = ($created | ForEach-Object { "- [ ] #$($_.Number) $($_.Title)" }) -join "`n"
    $parentNow = Invoke-Gh @('issue', 'view', "$parent", '--json', 'body', '--jq', '.body')
    # チェックリストと後続の "---" の間に空行を残す（詰めると最終行が見出し扱いされ得る）
    $parentNew = $parentNow -replace '(?s)## タスク\r?\n\r?\n.*?(\r?\n---)', "## タスク`n`n$checklist`n`$1"
    if ($parentNew -eq $parentNow) { $parentNew = $parentNow + "`n`n## タスク`n`n$checklist" }
    $parentBodyFile2 = Join-Path $tmpDir 'parent-update.md'
    Write-Utf8NoBom $parentBodyFile2 $parentNew
    Invoke-Gh @('issue', 'edit', "$parent", '--body-file', $parentBodyFile2) | Out-Null
    Write-Host "親issue #$parent の本文を更新しました" -ForegroundColor Green

    Write-Host ''
    Write-Host "完了: 親 #$parent / 子 $($created.Count) 件" -ForegroundColor Cyan
    $created | Format-Table Number, Title -AutoSize
}
finally {
    Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
}
