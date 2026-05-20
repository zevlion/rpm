#Requires -Version 5.1

<#
.SYNOPSIS
    Installs or updates rpm2 on Windows.

.DESCRIPTION
    Downloads the latest rpm2 release from GitHub, verifies it,
    installs it to a user or system directory, and optionally adds
    it to PATH.

.PARAMETER InstallDir
    Directory to install rpm2 into.
    Defaults to %LOCALAPPDATA%\rpm2 (user, no elevation needed).
    Pass "C:\Program Files\rpm2" or similar for a system-wide install.

.PARAMETER Force
    Re-install even if the current version is already up to date.

.EXAMPLE
    .\windows-installer.ps1
    .\windows-installer.ps1 -InstallDir "C:\Program Files\rpm2" -Force
#>

param (
    [string] $InstallDir = "$env:LOCALAPPDATA\rpm2",
    [switch] $Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# ── config ────────────────────────────────────────────────────────────────────

$Repo        = "zevlion/rpm2"
$Binary      = "rpm2.exe"
$DownloadUrl = "https://github.com/$Repo/releases/download/latest/$Binary"
$TmpPath     = Join-Path $env:TEMP "rpm2_download.exe"
$InstallPath = Join-Path $InstallDir $Binary

# ── helpers ───────────────────────────────────────────────────────────────────

function Write-Info    ($msg) { Write-Host "  $msg" -ForegroundColor Cyan }
function Write-Success ($msg) { Write-Host "✓ $msg" -ForegroundColor Green }
function Write-Warn    ($msg) { Write-Host "⚠ $msg" -ForegroundColor Yellow }
function Write-Fail    ($msg) { Write-Host "✗ $msg" -ForegroundColor Red; exit 1 }

function Test-IsElevated {
    $id = [System.Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [System.Security.Principal.WindowsPrincipal] $id
    return $principal.IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Test-InPath ($dir) {
    $userPath   = [Environment]::GetEnvironmentVariable("PATH", "User")
    $systemPath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
    return ($userPath -split ";") -contains $dir -or ($systemPath -split ";") -contains $dir
}

function Add-ToUserPath ($dir) {
    $current = [Environment]::GetEnvironmentVariable("PATH", "User")
    $entries = ($current -split ";") | Where-Object { $_ -ne "" }
    if ($entries -notcontains $dir) {
        $updated = ($entries + $dir) -join ";"
        [Environment]::SetEnvironmentVariable("PATH", $updated, "User")
        # Also update the current session
        $env:PATH = "$env:PATH;$dir"
        return $true
    }
    return $false
}

# ── elevation check for system-wide installs ──────────────────────────────────

$IsSystemDir = $InstallDir -match "^C:\\Program Files" -or
               $InstallDir -eq "C:\Windows\System32"

if ($IsSystemDir -and -not (Test-IsElevated)) {
    Write-Warn "Installing to '$InstallDir' requires Administrator privileges."
    Write-Warn "Re-launching as Administrator..."
    $args = "-ExecutionPolicy Bypass -File `"$PSCommandPath`" -InstallDir `"$InstallDir`""
    if ($Force) { $args += " -Force" }
    Start-Process powershell -ArgumentList $args -Verb RunAs
    exit
}

# ── detect update vs fresh install ───────────────────────────────────────────

$IsUpdate = $false
$CurrentVersion = $null

$Existing = Get-Command rpm2 -ErrorAction SilentlyContinue
if ($Existing) {
    try { $CurrentVersion = & rpm2 --version 2>$null } catch {}
    if ($CurrentVersion) {
        Write-Host "Updating rpm2 ($CurrentVersion → latest)..."
    } else {
        Write-Host "Updating rpm2..."
    }
    $IsUpdate = $true
} else {
    Write-Host "Installing rpm2..."
}

# ── create install directory ──────────────────────────────────────────────────

if (-not (Test-Path $InstallDir)) {
    Write-Info "Creating install directory: $InstallDir"
    try {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    } catch {
        Write-Fail "Could not create directory '$InstallDir': $_"
    }
}

# ── download ──────────────────────────────────────────────────────────────────

Write-Info "Downloading from $DownloadUrl"

# Clean up any previous failed attempt
if (Test-Path $TmpPath) { Remove-Item $TmpPath -Force }

try {
    # Use TLS 1.2+ — older Windows defaults to TLS 1.0 which GitHub rejects
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

    $ProgressPreference = "SilentlyContinue"   # Invoke-WebRequest progress bar is extremely slow
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $TmpPath -UseBasicParsing
    $ProgressPreference = "Continue"
} catch {
    Write-Fail "Download failed: $_`nCheck your connection or visit: https://github.com/$Repo/releases"
}

# ── validate ──────────────────────────────────────────────────────────────────

if (-not (Test-Path $TmpPath)) {
    Write-Fail "Download produced no file."
}

$FileSize = (Get-Item $TmpPath).Length
if ($FileSize -eq 0) {
    Remove-Item $TmpPath -Force
    Write-Fail "Downloaded file is empty."
}

# Check MZ magic bytes (Windows PE executable header)
$Bytes = [System.IO.File]::ReadAllBytes($TmpPath)
if ($Bytes[0] -ne 0x4D -or $Bytes[1] -ne 0x5A) {
    Remove-Item $TmpPath -Force
    Write-Fail "Downloaded file is not a valid Windows executable (bad MZ header). The release may not exist yet."
}

Write-Info "Downloaded $([math]::Round($FileSize / 1KB, 1)) KB — PE header OK"

# ── stop running daemon before replacing the binary ───────────────────────────

if ($IsUpdate) {
    Write-Info "Stopping rpm2 daemon (if running)..."
    try { & rpm2 kill 2>$null } catch {}
    Start-Sleep -Milliseconds 400
}

# ── install ───────────────────────────────────────────────────────────────────

Write-Info "Installing to $InstallPath"

try {
    Move-Item -Path $TmpPath -Destination $InstallPath -Force
} catch {
    Remove-Item $TmpPath -Force -ErrorAction SilentlyContinue
    Write-Fail "Failed to install binary: $_"
}

# ── PATH ──────────────────────────────────────────────────────────────────────

if (-not (Test-InPath $InstallDir)) {
    Write-Info "Adding $InstallDir to your PATH..."
    try {
        $Added = Add-ToUserPath $InstallDir
        if ($Added) {
            Write-Success "Added to PATH. Restart your terminal for it to take effect."
        }
    } catch {
        Write-Warn "Could not update PATH automatically: $_"
        Write-Warn "Add this to your PATH manually: $InstallDir"
    }
} 

# ── verify ────────────────────────────────────────────────────────────────────

try {
    $NewVersion = & $InstallPath --version 2>$null
} catch {
    $NewVersion = "unknown"
}

if ($IsUpdate) {
    Write-Success "Updated to $NewVersion"
} else {
    Write-Success "Installed $NewVersion — run 'rpm2 --help' to get started"
}