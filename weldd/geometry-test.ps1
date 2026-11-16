param([string]$ExePath)
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32G {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
Start-Process -FilePath $ExePath
Start-Sleep -Seconds 10
$p = Get-Process weldd -ErrorAction SilentlyContinue
if (-not $p) { Write-Output "FAIL: process not running"; exit 1 }
$h = $p.MainWindowHandle
$r = New-Object Win32G+RECT
[Win32G]::GetWindowRect($h, [ref]$r) | Out-Null
$wa = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
$w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
$cx = $r.Left + $w/2; $workCx = $wa.X + $wa.Width/2
Write-Output ("rect=({0},{1}) size={2}x{3} workarea={4}x{5}+{6},{7}" -f $r.Left,$r.Top,$w,$ht,$wa.Width,$wa.Height,$wa.X,$wa.Y)
$fail = 0
if ($w -ne 380 -or $ht -ne 64) { Write-Output "FAIL size ($w x $ht, want 380x64)"; $fail=1 } else { Write-Output "PASS size 380x64" }
if ([math]::Abs($cx - $workCx) -gt 2) { Write-Output "FAIL center (cx=$cx want $workCx)"; $fail=1 } else { Write-Output "PASS top-center" }
if (($r.Top - $wa.Y) -ne 8) { Write-Output "FAIL top gap ($($r.Top - $wa.Y), want 8)"; $fail=1 } else { Write-Output "PASS top gap 8" }
exit $fail
