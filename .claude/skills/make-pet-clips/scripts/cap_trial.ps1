# 画質の試し（2026-10-27 K 指示）: 動画のコマを持つ上限（3/4 ↔ 等倍）を切り替えて、同じペット・同じ大きさで
# 起動し、27 秒後の専有メモリ（子孫込み）と、持っているコマの大きさ（ログ）と、窓の撮影を残す。
# K の常駐ペットは止めない（試験用のデータフォルダなので別の名前のミューテックス）。
# 窓の撮影は座標系の違いで当たらないことがある（2026-10-27）。判定はログの held_cell と KB で行う。
# 使い方: $env:TGT=<ビルドの target（既定 target）>; pwsh -File cap_trial.ps1 -Out <出力フォルダ> -Pet rinrin [-Full] [-Monitor "0,0,2880x1800"] [-WaitSec 27]
#   -Full    PET_TEST_FULL_CELL=1（等倍で持つ）
#   -Monitor K の state.json の per_monitor にあるその画面の位置に置く（省略時は K の home）
param(
  [Parameter(Mandatory)][string]$Out,
  [string]$Pet = "rinrin",
  [switch]$Full,
  [string]$Monitor = "",
  [int]$WaitSec = 27
)
$ErrorActionPreference = "Stop"
$repo = Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")
$tgt = if ($env:TGT) { $env:TGT } else { "target" }
$exe = Join-Path $repo "$tgt\debug\pet-app.exe"
$winctl = Join-Path $repo "$tgt\debug\examples\winctl.exe"
if (-not (Test-Path $exe)) { throw "先に CARGO_TARGET_DIR=$tgt cargo build -p pet-app --all-targets" }
New-Item -ItemType Directory -Force $Out | Out-Null
$data = Join-Path (Resolve-Path $Out) "data"
if (Test-Path $data) { Remove-Item -Recurse -Force $data }
New-Item -ItemType Directory -Force $data | Out-Null

# K の state.json から大きさと位置を写し、ペットと試験の設定を上書きする
$src = Join-Path $env:LOCALAPPDATA "pet-claude\state.json"
$s = if (Test-Path $src) { Get-Content $src -Raw -Encoding UTF8 | ConvertFrom-Json } else { [pscustomobject]@{ version = 1 } }
$s | Add-Member -Force NoteProperty pet $Pet
$s | Add-Member -Force NoteProperty walk $false
$s | Add-Member -Force NoteProperty tts $false
$s | Add-Member -Force NoteProperty wake $false
if ($Monitor -and $s.positions -and $s.positions.per_monitor) {
  $here = @($s.positions.per_monitor | Where-Object { $_.monitor -eq $Monitor })
  if (-not $here) { throw "画面 $Monitor の位置が state.json に無い" }
  $s.positions.home = $here[0]
}
[IO.File]::WriteAllText((Join-Path $data "state.json"), ($s | ConvertTo-Json -Depth 8), (New-Object Text.UTF8Encoding $false))

$env:PET_CLAUDE_DATA_DIR = $data
$env:PET_TRACE = "1"
if ($Full) { $env:PET_TEST_FULL_CELL = "1" } else { Remove-Item Env:PET_TEST_FULL_CELL -ErrorAction SilentlyContinue }
$p = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
$log = Join-Path $data "logs\pet.log"
Start-Sleep -Seconds $WaitSec

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
$priv = ($tree | ForEach-Object { (Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue).PrivateMemorySize64 } | Measure-Object -Sum).Sum / 1MB
$own = (Get-Process -Id $p.Id).PrivateMemorySize64 / 1MB

# この試験のプロセスの窓だけを撮る（K の常駐も同じクラスなので、PID で選ぶ）
$sig = @"
using System; using System.Runtime.InteropServices; using System.Text;
public class W {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static string Find(uint pid) {
    string found = null;
    EnumWindows((h, l) => { uint q; GetWindowThreadProcessId(h, out q);
      if (q == pid && IsWindowVisible(h)) { var sb = new StringBuilder(64); GetClassName(h, sb, 64);
        if (sb.ToString() == "PetClaudeWindow") { RECT r; GetWindowRect(h, out r); found = r.L + " " + r.T + " " + (r.R - r.L) + " " + (r.B - r.T); return false; } }
      return true; }, IntPtr.Zero);
    return found;
  }
}
"@
Add-Type -TypeDefinition $sig
$rect = [W]::Find([uint32]$p.Id)
if ($rect) {
  $l, $t, $w, $h = $rect -split " "
  & $winctl shot ([int]$l - 20) ([int]$t - 20) ([int]$w + 40) ([int]$h + 40) (Join-Path (Resolve-Path $Out) "pet.png") | Out-Null
}
Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

$loaded = (Select-String -Path $log -Pattern "\) loaded in .* cell (\d+x\d+)" | Select-Object -Last 1).Matches.Groups[1].Value
$clips = (Select-String -Path $log -Pattern "clip \w+ loaded in \d+ ms: (\d+) KB \(cache (\d+) KB\)" | ForEach-Object { $_.Line -replace '.*clip (\w+) loaded in \d+ ms: (\d+) KB \(cache (\d+) KB\).*', '$1=$2KB(cache $3KB)' }) -join ", "
$mode = if ($Full) { "full" } else { "3/4" }
"mode=$mode pet=$Pet monitor=$Monitor window=$rect held_cell=$loaded private_tree={0:N1}MB private_app={1:N1}MB clips: $clips" -f $priv, $own
