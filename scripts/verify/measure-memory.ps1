# pet-app を起動して、子孫プロセス（WebView2）を含めたメモリを測る。マウスは動かさない。
# 使い方: pwsh -File scripts/verify/measure-memory.ps1 [-WvArgs "<WebView2 の起動引数>"] [-WaitSec 10]
#        動いているアプリを測るだけなら -Attach（起動も停止もしない）
param([string]$WvArgs = "", [int]$WaitSec = 10, [switch]$Attach)
$exe = Join-Path $PSScriptRoot "..\..\target\debug\pet-app.exe"
if ($Attach) {
  $p = Get-Process pet-app -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $p) { "pet-app is not running"; exit 1 }
} else {
  Get-Process pet-app -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 1
  if ($WvArgs) { $env:PET_WV_ARGS = $WvArgs } else { Remove-Item Env:PET_WV_ARGS -ErrorAction SilentlyContinue }
  $p = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
  Start-Sleep -Seconds $WaitSec
}

function Get-Tree([int]$root) {
  $all = Get-CimInstance Win32_Process
  $ids = [System.Collections.Generic.HashSet[int]]::new(); [void]$ids.Add($root)
  do {
    $added = 0
    foreach ($x in $all) { if ($ids.Contains([int]$x.ParentProcessId) -and $ids.Add([int]$x.ProcessId)) { $added++ } }
  } while ($added -gt 0)
  $all | Where-Object { $ids.Contains([int]$_.ProcessId) }
}

$tree = Get-Tree $p.Id
$ws = ($tree | Measure-Object WorkingSetSize -Sum).Sum / 1MB
$priv = ($tree | ForEach-Object { (Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue).PrivateMemorySize64 } | Measure-Object -Sum).Sum / 1MB
"args='{0}' processes={1} workingset={2:N1}MB private={3:N1}MB" -f $WvArgs, @($tree).Count, $ws, $priv
$tree | Group-Object Name | ForEach-Object { "  {0} x{1}" -f $_.Name, $_.Count }
if (-not $Attach) { Stop-Process -Id $p.Id -Force }
