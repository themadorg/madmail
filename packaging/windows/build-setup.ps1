# Build Madmail Inno Setup installer(s).
# Prerequisites: Inno Setup 6 (ISCC.exe), pre-built Windows binaries under build/.
#
# Usage (from repo root or this directory):
#   .\packaging\windows\build-setup.ps1 -Arch amd64
#   .\packaging\windows\build-setup.ps1 -Arch arm64
#   .\packaging\windows\build-setup.ps1 -Arch all
#
# Does NOT publish GitHub Releases.

param(
    [ValidateSet("amd64", "arm64", "all")]
    [string]$Arch = "amd64",
    [string]$IsccPath = ""
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

function Build-One([string]$A) {
    Write-Host "==> Inno Setup Arch=$A"
    if ($A -eq "amd64") {
        Ensure-Binary "madmail-windows-amd64.exe"
    } else {
        Ensure-Binary "madmail-windows-arm64.exe"
    }
    $iscc = Find-Iscc
    Write-Host "    ISCC: $iscc"
    & $iscc "/DArch=$A" $Iss
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed for Arch=$A (exit $LASTEXITCODE)" }
    $out = Join-Path $Build "madmail-windows-$A-setup.exe"
    if (Test-Path $out) {
        Write-Host "    wrote $out"
    } else {
        Write-Warning "Expected output not found: $out"
    }
}

New-Item -ItemType Directory -Force -Path $Build | Out-Null
# Do not leave the caller's cwd changed (GHA steps share the process with &).
Push-Location $Here
try {
    switch ($Arch) {
        "amd64" { Build-One "amd64" }
        "arm64" { Build-One "arm64" }
        "all" {
            Build-One "amd64"
            Build-One "arm64"
        }
    }
} finally {
    Pop-Location
}

Write-Host "==> done"
Get-ChildItem (Join-Path $Build "madmail-windows-*-setup.exe") -ErrorAction SilentlyContinue | Format-Table Name, Length, LastWriteTime
