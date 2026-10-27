# 指定した PID のペットの窓（クラス PetClaudeWindow）の矩形を物理座標で出す。
# winctl の find は最初に見つかった PetClaudeWindow を返すので、常駐のペットと試しのペットが
# 同時に居るときは PID で選ぶ必要がある（マニュアルの撮影用。2026-10-28）。
# 使い方: powershell -File find-pet-window.ps1 <pid>   → "l t r b" を 1 行
param([Parameter(Mandatory = $true)][int]$TargetPid)
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class PetWin {
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static string Find(uint pid) {
    SetProcessDpiAwarenessContext((IntPtr)(-4)); // PER_MONITOR_AWARE_V2: 物理座標で読む
    string hit = "";
    EnumWindows((h, l) => {
      var sb = new StringBuilder(64); GetClassName(h, sb, 64);
      uint p; GetWindowThreadProcessId(h, out p);
      if (sb.ToString() == "PetClaudeWindow" && p == pid && IsWindowVisible(h)) {
        RECT r; GetWindowRect(h, out r); hit = r.L + " " + r.T + " " + r.R + " " + r.B; return false;
      }
      return true;
    }, IntPtr.Zero);
    return hit;
  }
}
"@
$r = [PetWin]::Find([uint32]$TargetPid)
if (-not $r) { Write-Error "PID $TargetPid の PetClaudeWindow が見つからない"; exit 1 }
Write-Output $r
