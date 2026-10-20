# Prove the unsigned testing installer aborts before copying files.
# Run only on a disposable CI runner. Do not point this at a machine that
# already has Madmail, and do not use it against production or account state.
#
# Synthetic markers stand in for a stable registration and the shared
# resources the server and tray still use. Sentinel bytes must be unchanged
# when setup returns. The script is noninteractive and kills a hung setup.

param(
    [Parameter(Mandatory = $true)][string]$SetupPath
)

$ErrorActionPreference = "Stop"
$ConfirmPreference = "None"
$ProgressPreference = "SilentlyContinue"
$PSNativeCommandUseErrorActionPreference = $false

function Test-Absent([string]$Path) {
    if (Test-Path -LiteralPath $Path) {
        throw "refusing to touch a pre-existing path: $Path"
    }
}

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash
}

$stableGuid = "{8F3C2A91-6B4E-4D2A-9C71-0A1B2C3D4E5F}"
$testingGuid = "{E7A41C90-2B6D-4F58-8C13-5D9A0E6B7F24}"
$programData = Join-Path $env:ProgramData "Madmail"
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$serviceKey = "HKLM:\SYSTEM\CurrentControlSet\Services\Madmail"
$stableUninstall = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\${stableGuid}_is1"
$testingUninstall = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\${testingGuid}_is1"
$watched = @(
    $programData,
    (Join-Path $env:ProgramFiles "Madmail"),
    (Join-Path ${env:ProgramFiles(x86)} "Madmail"),
    (Join-Path $env:ProgramFiles "MadmailTesting"),
    (Join-Path ${env:ProgramFiles(x86)} "MadmailTesting")
)

if (-not (Test-Path -LiteralPath $SetupPath -PathType Leaf)) {
    throw "testing installer is missing"
}
foreach ($path in $watched) {
    Test-Absent $path
}
foreach ($key in @($stableUninstall, $testingUninstall, $serviceKey)) {
    if (Test-Path -LiteralPath $key) {
        throw "refusing to touch a pre-existing registry key: $key"
    }
}
$existingRun = (Get-ItemProperty -Path $runKey -Name MadmailTray -ErrorAction SilentlyContinue).MadmailTray
if ($existingRun) {
    throw "MadmailTray autostart already exists"
}

$created = $false
$reasons = New-Object System.Collections.Generic.List[string]
try {
    $sentinelRoot = Join-Path $env:RUNNER_TEMP "madmail-refusal-sentinel"
    New-Item -ItemType Directory -Force -Path $sentinelRoot | Out-Null
    $outside = Join-Path $sentinelRoot "untouched.bin"
    $payload = [byte[]](0..63)
    [System.IO.File]::WriteAllBytes($outside, $payload)
    New-Item -ItemType Directory -Force -Path $programData | Out-Null
    $inside = Join-Path $programData "sentinel.bin"
    [System.IO.File]::WriteAllBytes($inside, $payload)
    New-Item -Path $stableUninstall -Force | Out-Null
    New-ItemProperty -Path $stableUninstall -Name MadmailRefusalSentinel -Value "unchanged" -PropertyType String -Force | Out-Null
    New-ItemProperty -Path $runKey -Name MadmailTray -Value $outside -PropertyType String -Force | Out-Null
    $created = $true

    $outsideHash = Get-Sha256 $outside
    $insideHash = Get-Sha256 $inside
    $log = Join-Path $sentinelRoot "setup.log"
    # One raw command line. An argument array quotes each switch, and Inno
    # Setup then ignores "/VERYSILENT", so the hidden wizard waits forever.
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = (Resolve-Path -LiteralPath $SetupPath).Path
    $psi.Arguments = "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOCANCEL /SP- /LOG=`"$log`""
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $proc = New-Object System.Diagnostics.Process
    $proc.StartInfo = $psi
    try {
        [void]$proc.Start()
    } catch {
        $reasons.Add("installer did not start: $($_.Exception.Message)")
        $proc = $null
    }
    if ($null -ne $proc) {
        $timedOut = -not $proc.WaitForExit(90000)
        if ($timedOut) {
            & taskkill.exe /PID $proc.Id /T /F | Out-Null
            try { $proc.WaitForExit(5000) | Out-Null } catch { }
            $reasons.Add("installer exceeded 90000ms")
        } else {
            $proc.Refresh()
            if ($proc.ExitCode -eq 0) {
                $reasons.Add("installer exit code was 0")
            }
        }
    }

    if ((Get-Sha256 $outside) -ne $outsideHash) { $reasons.Add("outside sentinel changed") }
    if ((Get-Sha256 $inside) -ne $insideHash) { $reasons.Add("resource sentinel changed") }
    foreach ($path in $watched) {
        if ($path -eq $programData) { continue }
        if (Test-Path -LiteralPath $path) { $reasons.Add("installation path appeared: $path") }
    }
    if (Test-Path -LiteralPath (Join-Path $programData "install.log")) {
        $reasons.Add("install.log was written")
    }
    if (Test-Path -LiteralPath $testingUninstall) {
        $reasons.Add("testing uninstall key was created")
    }
    if (Test-Path -LiteralPath $serviceKey) {
        $reasons.Add("Madmail service key was created")
    }
    $registration = (Get-ItemProperty -Path $stableUninstall -Name MadmailRefusalSentinel).MadmailRefusalSentinel
    if ($registration -ne "unchanged") { $reasons.Add("registration sentinel changed") }
    $autostart = (Get-ItemProperty -Path $runKey -Name MadmailTray).MadmailTray
    if ($autostart -ne $outside) { $reasons.Add("autostart sentinel changed") }

    if ($reasons.Count -gt 0) {
        $detail = "setup log was not created"
        if (Test-Path -LiteralPath $log) {
            $detail = (Get-Content -LiteralPath $log -Tail 40 -ErrorAction SilentlyContinue) -join "`n"
        }
        throw ("testing installer was not refused before installation effects: " + ($reasons -join "; ") + "`n" + $detail)
    }
    Write-Host "testing installer refused with exit $($proc.ExitCode); sentinels unchanged"
} finally {
    if ($created) {
        Remove-Item -LiteralPath $stableUninstall -Recurse -Force -ErrorAction SilentlyContinue
        Remove-ItemProperty -Path $runKey -Name MadmailTray -Force -ErrorAction SilentlyContinue
        if (Test-Path -LiteralPath $serviceKey) {
            & sc.exe stop Madmail | Out-Null
            & sc.exe delete Madmail | Out-Null
        }
        Remove-Item -LiteralPath $programData -Recurse -Force -ErrorAction SilentlyContinue
        foreach ($path in @(
            (Join-Path $env:ProgramFiles "Madmail"),
            (Join-Path ${env:ProgramFiles(x86)} "Madmail"),
            (Join-Path $env:ProgramFiles "MadmailTesting"),
            (Join-Path ${env:ProgramFiles(x86)} "MadmailTesting")
        )) {
            if (Test-Path -LiteralPath $path) {
                Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction SilentlyContinue
            }
        }
        if ($sentinelRoot) {
            Remove-Item -LiteralPath $sentinelRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
