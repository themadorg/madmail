# Install Madmail from host-shared build artifacts.
$ErrorActionPreference = "Stop"

Write-Host "==> [2/3] Install Madmail"

$BuildDir = "C:\madmail-build"
$AppDir = "C:\Program Files\Madmail"
$ConfigDir = "$env:ProgramData\Madmail\config"
$StateDir = "$env:ProgramData\Madmail\data"

$candidates = @(
    (Join-Path $BuildDir "madmail-windows-amd64.exe"),
    (Join-Path $BuildDir "madmail.exe"),
    (Join-Path $BuildDir "release\madmail.exe")
)
$MadmailSrc = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $MadmailSrc) {
    throw @"
No madmail Windows binary found under $BuildDir.
On the host, build or copy artifacts first:
  make build-windows-amd64
  # or place madmail-windows-amd64.exe in repo build/
Then: vagrant rsync && vagrant provision
"@
}

New-Item -ItemType Directory -Force -Path $AppDir, $ConfigDir, $StateDir | Out-Null
Copy-Item -Force $MadmailSrc (Join-Path $AppDir "madmail.exe")

$TraySrc = @(
    (Join-Path $BuildDir "madmail-tray-windows-amd64.exe"),
    (Join-Path $BuildDir "madmail-tray.exe")
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($TraySrc) {
    Copy-Item -Force $TraySrc (Join-Path $AppDir "madmail-tray.exe")
    Write-Host "Installed tray: $TraySrc"
}

$Madmail = Join-Path $AppDir "madmail.exe"
$Ip = "127.0.0.1"

# Idempotent-ish: stop/remove previous service if present
& $Madmail service stop 2>$null
& $Madmail service uninstall 2>$null

Write-Host "Running madmail install (self-signed, service, firewall)..."
& $Madmail install `
    --simple --ip $Ip --tls-mode self_signed --lang en `
    --config-dir $ConfigDir `
    --state-dir $StateDir `
    --binary-path $Madmail `
    --install-service --start-service --firewall `
    --no-obtain-certificate

if ($LASTEXITCODE -ne 0) {
    throw "madmail install failed with exit $LASTEXITCODE"
}

# Give listeners a moment to bind
Start-Sleep -Seconds 3

& $Madmail --config (Join-Path $ConfigDir "madmail.conf") --state-dir $StateDir service status
Write-Host "==> Install done"
Write-Host "    App:    $AppDir"
Write-Host "    Config: $ConfigDir"
Write-Host "    State:  $StateDir"
