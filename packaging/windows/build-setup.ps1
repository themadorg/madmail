# Build Madmail Inno Setup installer(s).
# Prerequisites: Inno Setup 6 (ISCC.exe), pre-built Windows binaries under build/.
#
# Usage (from repo root or this directory):
#   .\packaging\windows\build-setup.ps1 -Arch amd64
#   .\packaging\windows\build-setup.ps1 -Arch amd64 -Version 2.35.0
#   .\packaging\windows\build-setup.ps1 -Arch amd64 -Version 2.35.0 -RequireTray
#   .\packaging\windows\build-setup.ps1 -Arch amd64 -Version 2.35.0 -RequireTray -Testing
#   .\packaging\windows\build-setup.ps1 -Arch arm64
#   .\packaging\windows\build-setup.ps1 -Arch all
#
# -Version is numeric X.Y.Z. When omitted, the repo .version file is used.
# Does NOT publish GitHub Releases.

param(
    [ValidateSet("amd64", "arm64", "all")]
    [string]$Arch = "amd64",
    [string]$IsccPath = "",
    [string]$Version = "",
    [switch]$PrintVersion,
    [switch]$RequireTray,
    [switch]$Testing
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Resolve-Path (Join-Path $Here "..\..")
$Build = Join-Path $Root "build"
$Iss = Join-Path $Here "Madmail.iss"

function Find-Iscc {
    if ($IsccPath -and (Test-Path $IsccPath)) { return $IsccPath }
    $candidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "${env:LOCALAPPDATA}\Programs\Inno Setup 6\ISCC.exe"
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { return $c }
    }
    $cmd = Get-Command iscc -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    throw "ISCC.exe not found. Install Inno Setup 6 or pass -IsccPath."
}

function Ensure-Binary([string]$Name) {
    $p = Join-Path $Build $Name
    if (-not (Test-Path $p)) {
        throw "Missing $p — build Windows binaries first (make build-windows / build-windows.ps1)."
    }
}

function Resolve-AppVersion {
    $pattern = '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
    if ($Version) {
        if ($Version -notmatch $pattern) { throw "Version must be numeric X.Y.Z" }
        return $Version
    }
    $versionFile = Join-Path $Root ".version"
    if (-not (Test-Path $versionFile)) { throw "Pass -Version X.Y.Z or provide .version" }
    $fromFile = (Get-Content -Raw $versionFile).Trim()
    if ($fromFile -notmatch $pattern) { throw ".version is not numeric X.Y.Z" }
    return $fromFile
}

function Build-One([string]$A, [string]$AppVersion, [bool]$RequireTrayApp) {
    Write-Host "==> Inno Setup Arch=$A version=$AppVersion"
    if ($A -eq "amd64") {
        Ensure-Binary "madmail-windows-amd64.exe"
        $trayName = "madmail-tray-windows-amd64.exe"
    } else {
        Ensure-Binary "madmail-windows-arm64.exe"
        $trayName = "madmail-tray-windows-arm64.exe"
    }
    if ($RequireTrayApp) {
        Ensure-Binary $trayName
    }
    $iscc = Find-Iscc
    Write-Host "    ISCC: $iscc"
    $defines = @("/DArch=$A", "/DMyAppVersion=$AppVersion")
    if ($RequireTrayApp) { $defines += "/DRequireTray=1" }
    if ($Testing) {
        if (-not $RequireTrayApp) { throw "-Testing requires -RequireTray" }
        $serverPath = Join-Path $Build "madmail-windows-$A.exe"
        $trayPath = Join-Path $Build $trayName
        $serverHash = (Get-FileHash -Algorithm SHA256 $serverPath).Hash.ToLowerInvariant()
        $trayHash = (Get-FileHash -Algorithm SHA256 $trayPath).Hash.ToLowerInvariant()
        $note = @"
TESTING ONLY - NOT FOR PRODUCTION USE
version:$AppVersion
server-sha256:$serverHash
tray-sha256:$trayHash
"@
        $notePath = Join-Path $Build "UNSTABLE.txt"
        $utf8 = New-Object System.Text.UTF8Encoding $false
        [System.IO.File]::WriteAllText($notePath, $note.Trim() + "`n", $utf8)
        $defines += "/DTestingBuild=1"
    }
    & $iscc @defines $Iss
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed for Arch=$A (exit $LASTEXITCODE)" }
    $out = Join-Path $Build "madmail-windows-$A-setup.exe"
    if (Test-Path $out) {
        Write-Host "    wrote $out"
    } else {
        Write-Warning "Expected output not found: $out"
    }
}

$AppVersion = Resolve-AppVersion
if ($PrintVersion) {
    Write-Output $AppVersion
    exit 0
}

New-Item -ItemType Directory -Force -Path $Build | Out-Null
# Do not leave the caller's cwd changed (GHA steps share the process with &).
Push-Location $Here
try {
    switch ($Arch) {
        "amd64" { Build-One "amd64" $AppVersion ([bool]$RequireTray) }
        "arm64" { Build-One "arm64" $AppVersion ([bool]$RequireTray) }
        "all" {
            Build-One "amd64" $AppVersion ([bool]$RequireTray)
            Build-One "arm64" $AppVersion ([bool]$RequireTray)
        }
    }
} finally {
    Pop-Location
}

Write-Host "==> done"
Get-ChildItem (Join-Path $Build "madmail-windows-*-setup.exe") -ErrorAction SilentlyContinue | Format-Table Name, Length, LastWriteTime
