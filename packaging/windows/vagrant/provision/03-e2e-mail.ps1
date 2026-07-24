# Full mail E2E: accounts + SMTP + IMAP (Python).
$ErrorActionPreference = "Stop"

Write-Host "==> [3/3] Mail E2E"

$Madmail = "C:\Program Files\Madmail\madmail.exe"
$Config = "$env:ProgramData\Madmail\config\madmail.conf"
$State = "$env:ProgramData\Madmail\data"
$E2E = "C:\vagrant\e2e\mail_e2e.py"

if (-not (Test-Path $Madmail)) { throw "madmail missing: $Madmail" }
if (-not (Test-Path $Config)) { throw "config missing: $Config (run install provision first)" }
if (-not (Test-Path $E2E)) { throw "e2e script missing: $E2E" }

# Ensure service is up
& $Madmail --config $Config --state-dir $State service start 2>$null
Start-Sleep -Seconds 2
& $Madmail --config $Config --state-dir $State service status

# Tray smoke if present
$Tray = "C:\Program Files\Madmail\madmail-tray.exe"
if (Test-Path $Tray) {
    Write-Host "Tray smoke..."
    & $Tray --config $Config --state-dir $State --smoke-exit
}

Write-Host "Running Python mail E2E..."
python $E2E `
    --madmail $Madmail `
    --config $Config `
    --state-dir $State `
    --host 127.0.0.1 `
    --domain 127.0.0.1

if ($LASTEXITCODE -ne 0) {
    throw "mail E2E failed with exit $LASTEXITCODE"
}

Write-Host "==> E2E PASS"
