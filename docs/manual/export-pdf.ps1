# pet-claude-manual.docx を Word（COM）で PDF にする。見出しから PDF のしおりを作り、フォントを埋め込む。
# 使い方: pwsh docs/manual/export-pdf.ps1 [docx のパス] [pdf のパス]
#   既定は docs/manual/pet-claude-manual.docx → docs/manual/pet-claude-manual.pdf
#   Word が無い PC では動かない（Microsoft 365 / Word 2016 以降）。作った後は python docs/manual/check-pdf.py。
param(
    [string]$Docx = (Join-Path $PSScriptRoot "pet-claude-manual.docx"),
    [string]$Pdf = (Join-Path $PSScriptRoot "pet-claude-manual.pdf")
)
$ErrorActionPreference = "Stop"
$Docx = (Resolve-Path $Docx).Path
$Pdf = [System.IO.Path]::GetFullPath($Pdf)

$word = New-Object -ComObject Word.Application
$word.Visible = $false
$word.DisplayAlerts = 0  # wdAlertsNone。読み取り専用の確認などで止まらないように
try {
    # ReadOnly で開く: 変換のために docx 自体は書き換えない（K の手直しを壊さない）
    $doc = $word.Documents.Open($Docx, $false, $true)
    $doc.Fields.Update() | Out-Null  # 総ページ数などのフィールドを現在の値にする
    # ExportAsFixedFormat(OutputFileName, ExportFormat=17(PDF), OpenAfterExport, OptimizeFor=0(Print),
    #   Range=0(全体), From, To, Item=0(内容), IncludeDocProps, KeepIRM, CreateBookmarks=1(見出しから), DocStructureTags, BitmapMissingFonts, UseISO19005_1)
    $doc.ExportAsFixedFormat($Pdf, 17, $false, 0, 0, 1, 1, 0, $true, $true, 1, $true, $true, $false)
    $doc.Close(0)  # wdDoNotSaveChanges
    Write-Output "wrote $Pdf"
} finally {
    $word.Quit()
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
}
