# Taskbar-safety regression test.
# WARNING: it HIDES YOUR REAL TASKBAR for a few seconds per scenario and force-stops any running abra.exe / abra-guard.exe,
# so quit ABRA first (or pass -KillRunning). It uses a temporary NACRE_CONFIG_DIR, so your settings are untouched.
# (Verifikasi keselamatan taskbar: menyembunyikan taskbar asli sementara.)
param([string]$Release = "$PSScriptRoot\..\..\target\release", [switch]$KillRunning)

if ((Get-Process abra, abra-guard -ErrorAction SilentlyContinue) -and -not $KillRunning) {
  Write-Host "ABRA is running. Quit it first, or re-run with -KillRunning to let this script stop it."
  exit 1
}

Add-Type @"
using System; using System.Runtime.InteropServices;
public class Tb {
  [DllImport("user32.dll")] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
  public static string State() {
    IntPtr h = FindWindow("Shell_TrayWnd", null);
    if (h == IntPtr.Zero) return "none";
    RECT r; GetWindowRect(h, out r);
    return (IsWindowVisible(h) ? "visible" : "hidden") + "@" + r.L + "," + r.T;
  }
  public static int BaseTop = int.MinValue;
  public static bool Restored() {
    IntPtr h = FindWindow("Shell_TrayWnd", null);
    if (h == IntPtr.Zero) return false;
    RECT r; GetWindowRect(h, out r);
    // Harus terlihat DAN kembali ke posisi semula (bukan sekadar terlihat di tepi auto-hide).
    return IsWindowVisible(h) && r.T == BaseTop && r.R > r.L;
  }
  public static void Snapshot() { IntPtr h = FindWindow("Shell_TrayWnd", null); RECT r; GetWindowRect(h, out r); BaseTop = r.T; }
}
"@

$exe = Join-Path $Release "abra.exe"
$results = @()
function Wait-Until($cond, $sec) { $sw = [Diagnostics.Stopwatch]::StartNew(); while ($sw.Elapsed.TotalSeconds -lt $sec) { if (& $cond) { return $true }; Start-Sleep -Milliseconds 100 }; return $false }
function Fresh($name) { $d = Join-Path $env:TEMP "nacre-m1-$name"; Remove-Item $d -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory $d | Out-Null; $env:NACRE_CONFIG_DIR = $d; return $d }
function Add-Result($n, $ok, $d) { $script:results += [pscustomobject]@{ Test = $n; Pass = $ok; Detail = $d }; Write-Host ("{0,-34} {1}  {2}" -f $n, $(if ($ok) { "PASS" } else { "FAIL" }), $d) }
function Cleanup { Get-Process abra, abra-guard -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 500 }

Cleanup
[Tb]::Snapshot()
Write-Host "Baseline taskbar: $([Tb]::State())"
if (-not [Tb]::Restored()) { Write-Host "PERINGATAN: taskbar tidak dalam keadaan normal sebelum uji: $([Tb]::State())" }

# T1: keluar normal memulihkan taskbar
$d = Fresh "t1"
$p = Start-Process $exe -ArgumentList "--hide-taskbar", "--exit-after", "6" -PassThru
$hid = Wait-Until { -not [Tb]::Restored() } 4
$marker = Test-Path "$d\taskbar_hidden.flag"
$guard = [bool](Get-Process abra-guard -ErrorAction SilentlyContinue)
Add-Result "T1a taskbar tersembunyi" $hid ([Tb]::State())
Add-Result "T1b marker + guard aktif" ($marker -and $guard) "marker=$marker guard=$guard"
$p.WaitForExit()
$back = Wait-Until { [Tb]::Restored() } 5
Add-Result "T1c pulih setelah keluar normal" $back ([Tb]::State())
$gone = Wait-Until { -not (Test-Path "$d\taskbar_hidden.flag") } 3
Add-Result "T1d marker dihapus" $gone ""
Start-Sleep 1
Add-Result "T1e guard berhenti" (-not (Get-Process abra-guard -ErrorAction SilentlyContinue)) ""
Cleanup

# T2: proses utama dibunuh paksa -> guard memulihkan
$d = Fresh "t2"
$p = Start-Process $exe -ArgumentList "--hide-taskbar" -PassThru
$hid = Wait-Until { -not [Tb]::Restored() } 4
Add-Result "T2a taskbar tersembunyi" $hid ([Tb]::State())
Stop-Process -Id $p.Id -Force
$back = Wait-Until { [Tb]::Restored() } 8
Add-Result "T2b guard memulihkan setelah kill" $back ([Tb]::State())
$gone = Wait-Until { -not (Test-Path "$d\taskbar_hidden.flag") } 3
Add-Result "T2c marker dihapus oleh guard" $gone ""
Cleanup

# T3: kill utama DAN guard -> start berikutnya memulihkan
$d = Fresh "t3"
$p = Start-Process $exe -ArgumentList "--hide-taskbar" -PassThru
$hid = Wait-Until { -not [Tb]::Restored() } 4
Get-Process abra-guard -ErrorAction SilentlyContinue | Stop-Process -Force
Stop-Process -Id $p.Id -Force
Start-Sleep 1
$still = -not [Tb]::Restored()
Add-Result "T3a tanpa guard taskbar tetap hilang" ($hid -and $still) ([Tb]::State())
$p2 = Start-Process $exe -ArgumentList "--exit-after", "3" -PassThru
$back = Wait-Until { [Tb]::Restored() } 6
Add-Result "T3b start berikutnya memulihkan" $back ([Tb]::State())
$p2.WaitForExit()
Cleanup

# T4: marker rusak -> tetap menampilkan taskbar (tebakan aman)
$d = Fresh "t4"
$p = Start-Process $exe -ArgumentList "--hide-taskbar" -PassThru
$hid = Wait-Until { -not [Tb]::Restored() } 4
Get-Process abra-guard -ErrorAction SilentlyContinue | Stop-Process -Force
Stop-Process -Id $p.Id -Force
Set-Content "$d\taskbar_hidden.flag" "ini bukan marker"
$p2 = Start-Process $exe -ArgumentList "--exit-after", "3" -PassThru
$back = Wait-Until { [Tb]::Restored() } 6
Add-Result "T4 marker rusak tetap memulihkan" ($hid -and $back) ([Tb]::State())
$p2.WaitForExit()
Cleanup

# T5: instance kedua tidak membuat proses baru
$d = Fresh "t5"
$p = Start-Process $exe -ArgumentList "--exit-after", "8" -PassThru
Start-Sleep 3
$p2 = Start-Process $exe -PassThru
$p2.WaitForExit(3000) | Out-Null
Add-Result "T5 instance kedua langsung keluar" $p2.HasExited ""
Start-Sleep 0.5
$log = Get-Content "$d\abra.log" -Raw
Add-Result "T5b instance pertama dibangunkan" ($log -match "peluncuran kedua") ""
$p.WaitForExit()
Cleanup

Write-Host ""
$fail = @($results | Where-Object { -not $_.Pass }).Count
Write-Host "Ringkasan: $($results.Count - $fail)/$($results.Count) lulus"
Write-Host "State akhir taskbar: $([Tb]::State())"
exit $fail

