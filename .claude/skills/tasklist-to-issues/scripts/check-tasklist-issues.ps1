<#
.SYNOPSIS
  tasklist.md と GitHub issue の食い違いを検出して報告する（読み取り専用・変更しない）。

.DESCRIPTION
  セッション開始時・ユーザーから指示を受けた時に実行し、issue 側の変更を tasklist.md に取り込む判断材料にする。
  検出する食い違い:
    - ISSUE_ONLY   : issue にあって tasklist に無い     → ユーザーが追加したタスク。tasklist に追記する
    - CLOSED_OPEN  : issue が closed / tasklist が未完了 → tasklist を [x] にする（再実施しない）
    - OPEN_DONE    : issue が open  / tasklist が [x]    → 差し戻しの可能性。ユーザーに確認する（自動で直さない）
    - NO_ISSUE     : tasklist にあって issue 番号が無い  → new-issues-from-tasklist.ps1 で起票する

.EXAMPLE
  .\check-tasklist-issues.ps1 -TasklistPath .steering\20260728-在庫CSV取込\tasklist.md
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$TasklistPath,
    [string]$Repo,
    [string]$Label
)

$ErrorActionPreference = 'Stop'

function Invoke-Gh([string[]]$GhArgs) {
    $full = @()
    if ($Repo) { $full += @('--repo', $Repo) }
    $full += $GhArgs
    $out = & gh @full 2>&1
    if ($LASTEXITCODE -ne 0) { throw "gh $($full -join ' ') が失敗しました: $out" }
    return (($out | ForEach-Object { $_.ToString() }) -join "`n").Trim()
}

if (-not (Test-Path -LiteralPath $TasklistPath)) { throw "tasklist が見つかりません: $TasklistPath" }
$tasklistFull = (Resolve-Path -LiteralPath $TasklistPath).Path
$unitDir = Split-Path -Parent $tasklistFull
$unit = Split-Path -Leaf $unitDir
# new-issues-from-tasklist.ps1 と同じ規則でラベル名を決める
# （GitHub のラベル名上限 50 文字。超える場合は起票側が詰めているのでここでも同じ詰め方をする）
if ($Label) { $label = $Label } else { $label = "task/$unit" }
if ($label.Length -gt 50) { $label = $label.Substring(0, 50).TrimEnd('-', '.', '/') }

# --- tasklist 側 ---
$lines = [System.IO.File]::ReadAllLines($tasklistFull, [System.Text.Encoding]::UTF8)
$inFence = $false
$parent = $null
$local = @{}   # issue番号 -> @{ State; Title }
$noIssue = @()

# タスク行の issue 番号: 行末の "(#NN)"、または "(#NN)" の後に " — <完了ハッシュ・メモ>" が続く形
# （完了時に " — 2e7f95d" のように追記する運用。行末だけを見ると追記した行を NO_ISSUE と誤判定する）。
# new-issues-from-tasklist.ps1 の「起票済み」判定と同じ式に揃える。
$IssueRefPattern = '\(#(\d+)\)(?=\s*$|\s+[—–-]\s)'

foreach ($line in $lines) {
    if ($line -match '^\s*```') { $inFence = -not $inFence; continue }
    if ($inFence) { continue }
    if ($line -match '^tracking:\s*#(\d+)') { $parent = [int]$Matches[1]; continue }
    if ($line -match '^-\s\[( |~|x)\]\s+(.+?)\s*$') {
        $state = $Matches[1]
        $title = $Matches[2]
        if ($title -match '~~') { continue }              # 技術的理由でスキップ済み
        if ($title -match '^\{.*\}$') { continue }         # テンプレのプレースホルダ
        if ($title -match $IssueRefPattern) {
            $local[[int]$Matches[1]] = @{ State = $state; Title = $title }
        }
        elseif ($state -ne 'x') {
            # 完了済み（[x]）で issue が無い行（承認・起票そのもの等）は起票対象にならないので報告しない
            # （new-issues-from-tasklist.ps1 も [ ] / [~] の行だけを起票する）
            $noIssue += $title
        }
    }
}

# --- issue 側 ---
$json = Invoke-Gh @('issue', 'list', '--label', $label, '--state', 'all', '--limit', '200', '--json', 'number,title,state,stateReason,labels')
# 注: PowerShell 5.1 の ConvertFrom-Json はパイプラインで配列を列挙せず1オブジェクトとして流す。
#     いったん変数に受けてから @() で配列化しないと件数が常に1になる。
$parsed = if ([string]::IsNullOrWhiteSpace($json)) { @() } else { ConvertFrom-Json -InputObject $json }
$issues = @($parsed) | Where-Object { -not ($_.labels.name -contains 'tracking') }

$findings = @()

foreach ($iss in $issues) {
    if (-not $local.ContainsKey($iss.number)) {
        $findings += [pscustomobject]@{ Kind = 'ISSUE_ONLY'; Issue = "#$($iss.number)"; Title = $iss.title; Action = 'tasklist に "- [ ] タイトル (#NN)" を追記して着手対象に含める' }
        continue
    }
    $st = $local[$iss.number].State
    if ($iss.state -eq 'CLOSED' -and $st -ne 'x') {
        $findings += [pscustomobject]@{ Kind = 'CLOSED_OPEN'; Issue = "#$($iss.number)"; Title = $iss.title; Action = 'tasklist を [x] にする（勝手に再実施しない）' }
    }
    elseif ($iss.state -eq 'OPEN' -and $st -eq 'x') {
        $findings += [pscustomobject]@{ Kind = 'OPEN_DONE'; Issue = "#$($iss.number)"; Title = $iss.title; Action = '差し戻しの可能性。どちらも上書きせずユーザーに確認する' }
    }
}
foreach ($t in $noIssue) {
    $findings += [pscustomobject]@{ Kind = 'NO_ISSUE'; Issue = '-'; Title = $t; Action = 'new-issues-from-tasklist.ps1 で起票する' }
}
# tasklist が参照している issue が存在しない（番号の書き間違い・別リポジトリの番号）
$issueNumbers = @($issues | ForEach-Object { $_.number })
foreach ($n in $local.Keys) {
    if ($issueNumbers -notcontains $n) {
        $findings += [pscustomobject]@{ Kind = 'LOCAL_ONLY'; Issue = "#$n"; Title = $local[$n].Title; Action = 'このラベルの issue に該当番号が無い。番号の誤りかラベル違いを確認する' }
    }
}

Write-Host "作業単位: $unit   ラベル: $label   親issue: $(if ($parent) { "#$parent" } else { '未設定' })" -ForegroundColor Cyan
Write-Host "tasklist の紐付け済みタスク: $($local.Count) 件 / issue（子）: $($issues.Count) 件" -ForegroundColor Cyan
Write-Host ''

if ($findings.Count -eq 0) {
    Write-Host '食い違いはありません（tasklist と issue は一致）。' -ForegroundColor Green
}
else {
    $findings | Format-Table Kind, Issue, Title, Action -AutoSize -Wrap
}
