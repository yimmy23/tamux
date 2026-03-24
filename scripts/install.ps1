#Requires -RunAsAdministrator
#Requires -Version 5.1
<#
.SYNOPSIS
    Install tamux binaries to C:\Program Files\tamux

.DESCRIPTION
    Downloads pre-built tamux binaries from GitLab Releases, verifies SHA256
    checksums, installs to C:\Program Files\tamux, and updates system PATH.

.PARAMETER DryRun
    Print what would be done without downloading or modifying files.

.EXAMPLE
    irm https://tamux.dev/install.ps1 | iex
    Download and run the installer.

.EXAMPLE
    $env:TAMUX_VERSION = "0.1.10"; .\install.ps1
    Install a specific version.

.EXAMPLE
    .\install.ps1 -DryRun
    Show what would be installed without making changes.
#>

[CmdletBinding()]
param(
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$InstallDir = "C:\Program Files\tamux"
$BaseUrl = "https://gitlab.com/api/v4/projects/PROJECT_ID/packages/generic/tamux"
$Binaries = @("tamux-daemon.exe", "tamux.exe", "tamux-tui.exe")

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------

function Detect-Platform {
    $Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture

    switch ($Arch) {
        "X64"   { $script:ArchName = "x64" }
        "Arm64" { $script:ArchName = "arm64" }
        default {
            Write-Error "Unsupported architecture: $Arch"
            exit 1
        }
    }

    $script:Target = "windows-$script:ArchName"
    Write-Host "Detected platform: $script:Target"
}

# ---------------------------------------------------------------------------
# Version resolution
# ---------------------------------------------------------------------------

function Get-LatestVersion {
    if ($env:TAMUX_VERSION) {
        $script:Version = $env:TAMUX_VERSION
        Write-Host "Using specified version: $script:Version"
        return
    }

    try {
        $response = Invoke-WebRequest -Uri "https://gitlab.com/api/v4/projects/PROJECT_ID/releases" `
            -UseBasicParsing -ErrorAction Stop
        $content = $response.Content
        if ($content -match '"tag_name":"v([^"]+)"') {
            $script:Version = $Matches[1]
        } else {
            throw "No version tag found"
        }
    } catch {
        Write-Error "Could not determine latest version. Set `$env:TAMUX_VERSION=x.y.z"
        exit 1
    }

    Write-Host "Latest version: $script:Version"
}

# ---------------------------------------------------------------------------
# Download and verify
# ---------------------------------------------------------------------------

function Download-AndVerify {
    $script:Tarball = "tamux-binaries-v$script:Version-$script:Target.tar.gz"
    $script:Sums = "SHA256SUMS-$script:Target.txt"
    $script:TmpDir = Join-Path $env:TEMP "tamux-install"

    # Clean and create temp directory
    if (Test-Path $script:TmpDir) {
        Remove-Item -Recurse -Force $script:TmpDir
    }
    New-Item -ItemType Directory -Force -Path $script:TmpDir | Out-Null

    Write-Host "Downloading tamux v$script:Version for $script:Target..."
    Invoke-WebRequest -Uri "$BaseUrl/$script:Version/$script:Tarball" `
        -OutFile (Join-Path $script:TmpDir $script:Tarball) `
        -UseBasicParsing
    Invoke-WebRequest -Uri "$BaseUrl/$script:Version/$script:Sums" `
        -OutFile (Join-Path $script:TmpDir $script:Sums) `
        -UseBasicParsing

    # SHA256 checksum verification
    Write-Host "Verifying SHA256 checksum..."
    $sumsContent = Get-Content (Join-Path $script:TmpDir $script:Sums)
    $expectedLine = $sumsContent | Select-String $script:Tarball
    if (-not $expectedLine) {
        Write-Warning "Tarball not found in checksums file, skipping verification"
        return
    }

    $ExpectedHash = $expectedLine.ToString().Split(" ")[0].ToLower()
    $ActualHash = (Get-FileHash (Join-Path $script:TmpDir $script:Tarball) -Algorithm SHA256).Hash.ToLower()

    if ($ActualHash -ne $ExpectedHash) {
        Write-Warning "SHA256 checksum mismatch! Expected: $ExpectedHash, Got: $ActualHash"
        throw "Checksum verification failed"
    }
    Write-Host "SHA256 checksum verified."
}

# ---------------------------------------------------------------------------
# Install binaries
# ---------------------------------------------------------------------------

function Install-Binaries {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

    Write-Host "Extracting binaries..."
    tar xzf (Join-Path $script:TmpDir $script:Tarball) -C $InstallDir

    # Verify binaries exist after extraction
    foreach ($bin in $Binaries) {
        $binPath = Join-Path $InstallDir $bin
        if (Test-Path $binPath) {
            Write-Host "  Installed: $bin"
        } else {
            Write-Warning "$bin not found after extraction"
        }
    }

    Write-Host "Installed: tamux-daemon.exe, tamux.exe, tamux-tui.exe -> $InstallDir"
}

# ---------------------------------------------------------------------------
# PATH update
# ---------------------------------------------------------------------------

function Update-Path {
    $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "Machine")

    # Check if InstallDir already in PATH
    if ($CurrentPath -split ";" | Where-Object { $_ -eq $InstallDir }) {
        Write-Host "$InstallDir is already in system PATH."
        return
    }

    # Update persistent system PATH (registry)
    [Environment]::SetEnvironmentVariable("Path", "$CurrentPath;$InstallDir", "Machine")

    # Update current session PATH
    $env:Path = "$env:Path;$InstallDir"

    Write-Host "Added $InstallDir to system PATH."
    Write-Host "Note: Other open terminals need to be restarted to pick up PATH change."
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

Detect-Platform
Get-LatestVersion

if ($DryRun) {
    $tarball = "tamux-binaries-v$Version-$Target.tar.gz"
    Write-Host ""
    Write-Host "Platform: $Target"
    Write-Host "Version: $Version"
    Write-Host "Would download: $BaseUrl/$Version/$tarball"
    Write-Host "Would install to: $InstallDir"
    Write-Host "Dry run complete -- no files downloaded or modified."
    exit 0
}

Download-AndVerify
Install-Binaries
Update-Path

# Cleanup
Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "tamux installed successfully! Run 'tamux' to get started."
