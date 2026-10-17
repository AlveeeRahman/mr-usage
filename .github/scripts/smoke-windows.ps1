# Install, start, stop and uninstall smoke test for the Windows installers.
#
#   pwsh .github/scripts/smoke-windows.ps1 -Kind nsis -Installer <path\Mr. Usage_*_x64-setup.exe>
#   pwsh .github/scripts/smoke-windows.ps1 -Kind msi  -Installer <path\Mr. Usage_*_x64_en-US.msi>
#
# Checks: silent install puts the exe and Start menu shortcut in place; the app
# starts and opens its window; it stops cleanly when asked (WM_CLOSE from
# CloseMainWindow, and `taskkill` without /F), exit code 0 within 10 s, with
# no WebView2 helper processes left; it starts again after each stop; silent
# uninstall removes the program files, shortcut and uninstall registry entry.
# Per-user data folders left after uninstall are reported, not failed: keeping
# user data on uninstall is the Windows convention (README, "Uninstalling").
param(
  [Parameter(Mandatory)] [ValidateSet('nsis', 'msi')] [string] $Kind,
  [Parameter(Mandatory)] [string] $Installer
)
$ErrorActionPreference = 'Stop'
$script:Fails = 0
function Pass($m) { Write-Host "PASS $m" }
function Fail($m) { Write-Host "FAIL $m"; $script:Fails++ }
$Identifier = 'io.github.alveeerahman.mr-usage'
$Installer = (Resolve-Path $Installer).Path

function Find-UninstallEntry {
  $roots = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
           'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
           'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
  foreach ($r in $roots) {
    if (Test-Path $r) {
      Get-ChildItem $r | ForEach-Object { Get-ItemProperty $_.PSPath } |
        Where-Object { $_.DisplayName -like 'Mr. Usage*' } | Select-Object -First 1
    }
  }
}

function Get-Helpers {
  @(Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" |
    Where-Object { $_.CommandLine -like "*$Identifier*" })
}

# --- install ---
if ($Kind -eq 'nsis') {
  Start-Process -FilePath $Installer -ArgumentList '/S' -Wait
} else {
  $p = Start-Process -FilePath msiexec.exe -ArgumentList '/i', "`"$Installer`"", '/qn', '/norestart' -Wait -PassThru
  if ($p.ExitCode -ne 0) { Fail "msiexec install exit code $($p.ExitCode)" }
}
$entry = $null
for ($i = 0; $i -lt 30 -and -not $entry; $i++) { $entry = Find-UninstallEntry; if (-not $entry) { Start-Sleep 1 } }
if (-not $entry) { Fail 'no uninstall registry entry after install'; exit 1 }
Pass "installed: registry entry '$($entry.DisplayName)'"

$exe = Get-ChildItem -Path @("$env:LOCALAPPDATA", "$env:ProgramFiles", "${env:ProgramFiles(x86)}") -Filter 'mr-usage.exe' -Recurse -ErrorAction SilentlyContinue -Depth 2 |
  Select-Object -First 1
if ($exe) { Pass "exe installed at $($exe.FullName)" } else { Fail 'mr-usage.exe not found after install'; exit 1 }
$installDir = $exe.DirectoryName
$shortcut = Get-ChildItem -Path @("$env:APPDATA\Microsoft\Windows\Start Menu\Programs", "$env:ProgramData\Microsoft\Windows\Start Menu\Programs") -Filter 'Mr. Usage*.lnk' -Recurse -ErrorAction SilentlyContinue |
  Select-Object -First 1
if ($shortcut) { Pass "Start menu shortcut at $($shortcut.FullName)" } else { Fail 'no Start menu shortcut' }

# --- start/stop cycles ---
foreach ($how in 'close', 'taskkill', 'close') {
  $proc = Start-Process -FilePath $exe.FullName -PassThru
  $ok = $false
  for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep -Milliseconds 500
    $proc.Refresh()
    if ($proc.HasExited) { break }
    if ($proc.MainWindowHandle -ne 0) { $ok = $true; break }
  }
  if (-not $ok) { Fail "[$how] window did not appear (exited: $($proc.HasExited))"; if (-not $proc.HasExited) { $proc.Kill() }; continue }
  # The first start after an install creates the WebView2 user-data folder,
  # which can take several seconds before the helper processes appear.
  for ($i = 0; $i -lt 30 -and (Get-Helpers).Count -lt 1; $i++) { Start-Sleep -Milliseconds 500 }
  if ((Get-Helpers).Count -ge 1) { Pass "[$how] started: window up, WebView2 helpers running" } else { Fail "[$how] no WebView2 helpers 15 s after the window appeared" }

  $sw = [Diagnostics.Stopwatch]::StartNew()
  if ($how -eq 'close') { [void]$proc.CloseMainWindow() } else { & taskkill.exe /PID $proc.Id | Out-Null }
  if (-not $proc.WaitForExit(10000)) {
    Fail "[$how] still running 10 s after the stop request"
    $proc.Kill(); continue
  }
  $proc.WaitForExit()
  if ($proc.ExitCode -eq 0) { Pass "[$how] exited with code 0 in $([math]::Round($sw.Elapsed.TotalSeconds, 1)) s" } else { Fail "[$how] exit code $($proc.ExitCode)" }
  $left = 0
  for ($i = 0; $i -lt 20; $i++) { $left = (Get-Helpers).Count; if ($left -eq 0) { break }; Start-Sleep -Milliseconds 500 }
  if ($left -eq 0) { Pass "[$how] no WebView2 helpers left" } else { Fail "[$how] $left WebView2 helper(s) left running" }
}

# --- uninstall ---
if ($Kind -eq 'nsis') {
  $uninstaller = ($entry.UninstallString -replace '"', '')
  Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait
} else {
  $p = Start-Process -FilePath msiexec.exe -ArgumentList '/x', "`"$Installer`"", '/qn', '/norestart' -Wait -PassThru
  if ($p.ExitCode -ne 0) { Fail "msiexec uninstall exit code $($p.ExitCode)" }
}
# The NSIS uninstaller re-launches itself from %TEMP%, so poll.
for ($i = 0; $i -lt 60 -and (Test-Path $exe.FullName); $i++) { Start-Sleep 1 }
if (-not (Test-Path $exe.FullName)) { Pass 'uninstall removed the exe' } else { Fail 'exe still present after uninstall' }
if (-not (Test-Path $installDir) -or -not (Get-ChildItem $installDir -Force)) { Pass 'install folder removed or empty' } else { Fail "install folder not empty: $(Get-ChildItem $installDir -Force | Select-Object -Expand Name)" }
if (-not (Find-UninstallEntry)) { Pass 'uninstall registry entry removed' } else { Fail 'uninstall registry entry still present' }
if ($shortcut -and (Test-Path $shortcut.FullName)) { Fail 'Start menu shortcut still present' } else { Pass 'Start menu shortcut removed' }

foreach ($d in "$env:APPDATA\$Identifier", "$env:LOCALAPPDATA\$Identifier") {
  if (Test-Path $d) {
    $names = (Get-ChildItem $d -Force | Select-Object -Expand Name) -join ', '
    Write-Host "INFO user data kept after uninstall (Windows convention): $d [$names]"
    $settings = Join-Path $d 'settings.json'
    if ((Test-Path $settings) -and (Select-String -Path $settings -Pattern 'key|token|email|remaining' -Quiet)) { Fail 'settings.json holds a secret-looking field' }
  }
}

Write-Host "SMOKE $Kind`: $script:Fails failure(s)"
exit $script:Fails
