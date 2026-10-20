# Phase 0 W11 の検証用: 指定の位置・大きさに「常に最前面」の窓を seconds 秒だけ出す。
# 使い方: pwsh -File scripts/verify/topmost-form.ps1 -Mode overlap|fullscreen -X 900 -Y 480 -W 150 -H 150 -Seconds 4
param([string]$Mode = "overlap", [int]$X = 0, [int]$Y = 0, [int]$W = 200, [int]$H = 200, [int]$Seconds = 4)
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.Form
$f.Text = "pet-claude-verify"
$f.TopMost = $true
$f.BackColor = [System.Drawing.Color]::DarkSlateGray
$f.FormBorderStyle = "None"
if ($Mode -eq "fullscreen") {
  $f.WindowState = "Maximized"
} else {
  $f.StartPosition = "Manual"
  $f.Location = New-Object System.Drawing.Point($X, $Y)
  $f.Size = New-Object System.Drawing.Size($W, $H)
}
$t = New-Object System.Windows.Forms.Timer
$t.Interval = $Seconds * 1000
$t.Add_Tick({ $f.Close() })
$t.Start()
[void]$f.ShowDialog()
