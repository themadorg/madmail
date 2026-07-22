# Bootstrap Windows guest for Madmail E2E (elevated).
$ErrorActionPreference = "Stop"

Write-Host "==> [1/3] Bootstrap tools"

# Chocolatey (optional package manager)
if (-not (Get-Command choco -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Chocolatey..."
    Set-ExecutionPolicy Bypass -Scope Process -Force
    [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
    Invoke-Expression ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
    $env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
                [System.Environment]::GetEnvironmentVariable("Path", "User")
}

# Python 3 for mail E2E (smtplib/imaplib)
if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Python 3..."
    choco install -y python3 --version=3.12.0
    $env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
                [System.Environment]::GetEnvironmentVariable("Path", "User")
}

python --version
Write-Host "==> Bootstrap done"
