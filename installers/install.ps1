<#
Installs Mr. Usage on Windows, or removes it.

  irm https://raw.githubusercontent.com/AlveeeRahman/mr-usage/main/installers/install.ps1 | iex
  & ([scriptblock]::Create((irm https://raw.githubusercontent.com/AlveeeRahman/mr-usage/main/installers/install.ps1))) -Uninstall

What it does: downloads the per-user installer from GitHub Releases together
with the release's SHA256SUMS.txt, refuses to run anything whose checksum is
not on that list, clears the download's mark-of-the-web (the checksum stands
in for the publisher signature SmartScreen looks for), and runs the installer
silently. -Uninstall first asks the app to delete the API keys it stored in
Credential Manager, then runs the uninstaller and removes the settings folder.

Options:
  -Version vX.Y.Z   a specific release instead of the latest
  -Msi              the per-machine .msi (needs an elevated PowerShell)
  -From DIR         use an installer already in DIR (no download)
  -Uninstall        remove Mr. Usage and the keys it stored
#>
param(
  [string] $Version = '',
  [switch] $Msi,
  [string] $From = '',
  [switch] $Uninstall
)
$ErrorActionPreference = 'Stop'
$Repo = 'AlveeeRahman/mr-usage'
$Identifier = 'io.github.alveeerahman.mr-usage'

function Find-UninstallEntry {
  $roots = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
           'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
           'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
  foreach ($r in $roots) {
    if (Test-Path $r) {
      $hit = Get-ChildItem $r | ForEach-Object { Get-ItemProperty $_.PSPath } |
        Where-Object { $_.DisplayName -like 'Mr. Usage*' } | Select-Object -First 1
      if ($hit) { return $hit }
    }
  }
}

function Find-Exe {
  Get-ChildItem -Path @("$env:LOCALAPPDATA", "$env:ProgramFiles", "${env:ProgramFiles(x86)}") -Filter 'mr-usage.exe' -Recurse -ErrorAction SilentlyContinue -Depth 2 |
    Select-Object -First 1
}

if ($Uninstall) {
  $exe = Find-Exe
  if ($exe) {
    & $exe.FullName --forget-keys
    if ($LASTEXITCODE -eq 0) { Write-Host 'API keys removed from Credential Manager.' }
    else { Write-Host 'Could not reach Credential Manager, so any stored keys were left in place.' }
  } else {
    Write-Host 'Mr. Usage is not installed, so there are no keys to remove through it.'
  }
  $entry = Find-UninstallEntry
  if ($entry) {
    if ($entry.UninstallString -match 'msiexec') {
      $code = ($entry.UninstallString -replace '.*(\{[0-9A-Fa-f-]+\}).*', '$1')
      Start-Process -FilePath msiexec.exe -ArgumentList '/x', $code, '/qn', '/norestart' -Wait
    } else {
      Start-Process -FilePath ($entry.UninstallString -replace '"', '') -ArgumentList '/S' -Wait
      # The NSIS uninstaller re-launches itself from %TEMP%, so wait for the exe to go.
      for ($i = 0; $i -lt 60 -and $exe -and (Test-Path $exe.FullName); $i++) { Start-Sleep 1 }
    }
  }
  foreach ($d in "$env:APPDATA\$Identifier", "$env:LOCALAPPDATA\$Identifier") {
    if (Test-Path $d) { Remove-Item -Recurse -Force $d -ErrorAction SilentlyContinue }
  }
  Write-Host 'Mr. Usage removed.'
  return
}

$suffix = if ($Msi) { '_x64_en-US.msi' } else { '_x64-setup.exe' }
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("mr-usage-install-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  if ($From) {
    $pkg = Get-ChildItem -Path $From -Filter "*$suffix" | Select-Object -First 1
    if (-not $pkg) { throw "no *$suffix in $From" }
    $pkg = $pkg.FullName
    $sums = Join-Path $From 'SHA256SUMS.txt'
  } else {
    $headers = @{ Accept = 'application/vnd.github+json' }
    # Maintainers and the release workflow set GH_TOKEN so a draft release is
    # visible. A draft has no git tag, so it cannot be fetched by tag; it is
    # found by listing releases and matching tag_name. Its files must then come
    # through the API asset URL.
    $token = $env:GH_TOKEN
    if ($token) { $headers.Authorization = "Bearer $token" }
    Write-Host 'Looking up the release...'
    if ($token -and $Version) {
      $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases?per_page=50" -Headers $headers |
        Where-Object { $_.tag_name -eq $Version } | Select-Object -First 1
      if (-not $release) { throw "no release (draft or published) is tagged $Version" }
    } else {
      $api = if ($Version) { "https://api.github.com/repos/$Repo/releases/tags/$Version" } else { "https://api.github.com/repos/$Repo/releases/latest" }
      $release = Invoke-RestMethod -Uri $api -Headers $headers
    }
    $asset = $release.assets | Where-Object { $_.name -like "*$suffix" } | Select-Object -First 1
    $sumsAsset = $release.assets | Where-Object { $_.name -eq 'SHA256SUMS.txt' } | Select-Object -First 1
    if (-not $asset) { throw "this release has no *$suffix file" }
    if (-not $sumsAsset) { throw 'this release has no SHA256SUMS.txt, so the download cannot be verified; nothing installed' }
    $pkg = Join-Path $tmp "package$suffix"
    $sums = Join-Path $tmp 'SHA256SUMS.txt'
    Write-Host "Downloading $($asset.name)..."
    if ($token) {
      $dl = @{ Accept = 'application/octet-stream'; Authorization = "Bearer $token" }
      Invoke-WebRequest -Uri $asset.url -Headers $dl -OutFile $pkg
      Invoke-WebRequest -Uri $sumsAsset.url -Headers $dl -OutFile $sums
    } else {
      Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $pkg
      Invoke-WebRequest -Uri $sumsAsset.browser_download_url -OutFile $sums
    }
  }

  if (Test-Path $sums) {
    $hash = (Get-FileHash -Path $pkg -Algorithm SHA256).Hash.ToLowerInvariant()
    $listed = Get-Content $sums | Where-Object { $_.ToLowerInvariant().StartsWith("$hash ") }
    if (-not $listed) { throw "the download's SHA-256 ($hash) is not in SHA256SUMS.txt; nothing installed" }
    Write-Host 'Checksum verified.'
  } elseif ($From) {
    Write-Host 'No SHA256SUMS.txt next to the installer; skipping the checksum (local files).'
  }

  # Remove the mark-of-the-web so SmartScreen does not stop an unsigned,
  # checksum-verified installer.
  Unblock-File -Path $pkg -ErrorAction SilentlyContinue

  if ($Msi) {
    $p = Start-Process -FilePath msiexec.exe -ArgumentList '/i', "`"$pkg`"", '/qn', '/norestart' -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "msiexec exited with code $($p.ExitCode)" }
  } else {
    Start-Process -FilePath $pkg -ArgumentList '/S' -Wait
  }
  $exe = $null
  for ($i = 0; $i -lt 30 -and -not $exe; $i++) { $exe = Find-Exe; if (-not $exe) { Start-Sleep 1 } }
  if (-not $exe) { throw 'the installer finished but mr-usage.exe was not found' }
  Write-Host "Installed: $($exe.FullName). Find Mr. Usage in the Start menu."
} catch {
  # In GitHub Actions, repeat the failure as an annotation so it shows in the run summary.
  if ($env:GITHUB_ACTIONS) { Write-Host "::error title=install.ps1::$($_.Exception.Message -replace "`r?`n", ' ')" }
  throw
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
