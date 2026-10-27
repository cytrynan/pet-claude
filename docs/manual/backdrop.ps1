# マニュアルの撮影用: 指定の範囲（物理座標）に無地の最前面の窓を seconds 秒だけ出す。
# ペットは 1 秒ごとに最前面を取り直し、吹き出し・バー・メニューはこの窓より後に出るので、どれもこの窓の上に載る。
# K のデスクトップの中身（開いている文書など）をスクリーンショットに入れないための下敷き（2026-10-28）。
# 最前面でない窓は、入力の無いプロセスからは前に出せず、他の窓の裏に出てしまった（実測）。
# 使い方: powershell -File backdrop.ps1 -X 1500 -Y 300 -W 1400 -H 1300 -Seconds 90
param([int]$X = 0, [int]$Y = 0, [int]$W = 1400, [int]$H = 1300, [int]$Seconds = 90)
Add-Type -Name Dpi -Namespace PetManual -MemberDefinition @"
[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
"@
# PER_MONITOR_AWARE_V2 にして、Location と Size を物理ピクセルで指定できるようにする（200% の画面で半分の位置に出ないように）
[void][PetManual.Dpi]::SetProcessDpiAwarenessContext([IntPtr](-4))
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.Form
$f.Text = "pet-claude-manual-backdrop"
$f.TopMost = $true
$f.BackColor = [System.Drawing.Color]::FromArgb(236, 236, 240)
$f.FormBorderStyle = "None"
$f.ShowInTaskbar = $false
$f.StartPosition = "Manual"
$f.Location = New-Object System.Drawing.Point($X, $Y)
$f.Size = New-Object System.Drawing.Size($W, $H)
$t = New-Object System.Windows.Forms.Timer
$t.Interval = $Seconds * 1000
$t.Add_Tick({ $f.Close() })
$t.Start()
[void]$f.ShowDialog()
