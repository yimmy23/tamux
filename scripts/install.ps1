#Requires -RunAsAdministrator
#Requires -Version 5.1
<#
.SYNOPSIS
    Install tamux binaries to C:\Program Files\tamux

.DESCRIPTION
    Downloads pre-built tamux binaries from GitHub Releases, verifies SHA256
    checksums for the extracted binaries, installs to C:\Program Files\tamux,
    and updates system PATH.

.PARAMETER DryRun
    Print what would be done without downloading or modifying files.

.EXAMPLE
    irm https://raw.githubusercontent.com/mkurman/tamux/main/scripts/install.ps1 | iex
    Download and run the installer.

.EXAMPLE
    $env:TAMUX_VERSION = "0.4.2"; .\install.ps1
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

$InstallDir = if ($env:TAMUX_INSTALL_DIR) { $env:TAMUX_INSTALL_DIR } else { "C:\Program Files\tamux" }
$SkillsDir = if ($env:TAMUX_SKILLS_DIR) { $env:TAMUX_SKILLS_DIR } else { Join-Path $HOME ".tamux\skills" }
$GuidelinesDir = if ($env:TAMUX_GUIDELINES_DIR) { $env:TAMUX_GUIDELINES_DIR } else { Join-Path $HOME ".tamux\guidelines" }
$GitHubOwner = "mkurman"
$GitHubRepo = "tamux"
$GitHubApiUrl = "https://api.github.com/repos/$GitHubOwner/$GitHubRepo"
$DownloadBaseUrl = "https://github.com/$GitHubOwner/$GitHubRepo/releases/download"
$RequestHeaders = @{
    "Accept" = "application/vnd.github+json"
    "User-Agent" = "tamux-installer"
}
$DirectInstallMarker = Join-Path $InstallDir ".tamux-install-source"
$Binaries = @("tamux.exe", "tamux-daemon.exe", "tamux-tui.exe", "tamux-gateway.exe", "tamux-mcp.exe")

function Normalize-Version {
    param([string]$Value)

    if (-not $Value) {
        return $Value
    }

    return $Value.TrimStart("v")
}

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
    $script:ArchiveName = "tamux-windows-$script:ArchName.zip"
    $script:ChecksumName = "SHA256SUMS-windows-$script:ArchName.txt"
    Write-Host "Detected platform: $script:Target"
}

# ---------------------------------------------------------------------------
# Version resolution
# ---------------------------------------------------------------------------

function Get-LatestVersion {
    if ($env:TAMUX_VERSION) {
        $script:Version = Normalize-Version $env:TAMUX_VERSION
        Write-Host "Using specified version: $script:Version"
        return
    }

    try {
        $release = Invoke-RestMethod -Uri "$GitHubApiUrl/releases/latest" `
            -Headers $RequestHeaders -ErrorAction Stop
        $script:Version = Normalize-Version $release.tag_name
        if (-not $script:Version) {
            throw "No version tag found"
        }
    } catch {
        Write-Error "Could not determine latest version. Set `$env:TAMUX_VERSION=x.y.z"
        exit 1
    }

    Write-Host "Latest version: $script:Version"
}

function Wait-ForPreviousTamux {
    if (-not $env:TAMUX_UPGRADE_WAIT_PID) {
        return
    }

    Write-Host "Waiting for tamux process $env:TAMUX_UPGRADE_WAIT_PID to exit..."
    while (Get-Process -Id $env:TAMUX_UPGRADE_WAIT_PID -ErrorAction SilentlyContinue) {
        Start-Sleep -Milliseconds 500
    }
}

# ---------------------------------------------------------------------------
# Download and verify
# ---------------------------------------------------------------------------

function Get-ChecksumMap {
    param([string]$Path)

    $checksums = @{}
    foreach ($line in Get-Content -Path $Path) {
        if ($line -match '^([A-Fa-f0-9]+)\s+\*?(.+)$') {
            $checksums[$Matches[2]] = $Matches[1].ToLower()
        }
    }

    return $checksums
}

function Verify-ExtractedBinary {
    param(
        [string]$BinaryName,
        [hashtable]$Checksums
    )

    $binaryPath = Join-Path $script:ExtractDir $BinaryName
    if (-not (Test-Path $binaryPath)) {
        throw "Release bundle is missing required binary $BinaryName"
    }

    $expectedHash = $Checksums[$BinaryName]
    if (-not $expectedHash) {
        throw "Checksum not found for $BinaryName in $script:ChecksumName"
    }

    $actualHash = (Get-FileHash -Path $binaryPath -Algorithm SHA256).Hash.ToLower()
    if ($actualHash -ne $expectedHash) {
        throw "SHA256 checksum mismatch for $BinaryName"
    }
}

function Download-AndVerify {
    $script:TmpDir = Join-Path $env:TEMP "tamux-install-$PID"
    $script:ArchivePath = Join-Path $script:TmpDir $script:ArchiveName
    $script:ChecksumPath = Join-Path $script:TmpDir $script:ChecksumName
    $script:ExtractDir = Join-Path $script:TmpDir "extract"

    # Clean and create temp directory
    if (Test-Path $script:TmpDir) {
        Remove-Item -Recurse -Force $script:TmpDir
    }
    New-Item -ItemType Directory -Force -Path $script:TmpDir | Out-Null
    New-Item -ItemType Directory -Force -Path $script:ExtractDir | Out-Null

    Write-Host "Downloading tamux v$script:Version for $script:Target..."
    Invoke-WebRequest -Uri $script:ChecksumUrl -Headers $RequestHeaders `
        -OutFile $script:ChecksumPath -ErrorAction Stop
    Invoke-WebRequest -Uri $script:ArchiveUrl -Headers $RequestHeaders `
        -OutFile $script:ArchivePath -ErrorAction Stop

    Write-Host "Extracting binaries, skills, and guidelines..."
    Expand-Archive -Path $script:ArchivePath -DestinationPath $script:ExtractDir -Force

    Write-Host "Verifying extracted binaries..."
    $script:Checksums = Get-ChecksumMap -Path $script:ChecksumPath
    foreach ($bin in $Binaries) {
        Verify-ExtractedBinary -BinaryName $bin -Checksums $script:Checksums
    }
}

# ---------------------------------------------------------------------------
# Install binaries
# ---------------------------------------------------------------------------

function Install-Binaries {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

    foreach ($bin in $Binaries) {
        $sourcePath = Join-Path $script:ExtractDir $bin
        if (-not (Test-Path $sourcePath)) {
            throw "Expected extracted binary not found: $bin"
        }

        Copy-Item -Path $sourcePath -Destination (Join-Path $InstallDir $bin) -Force
        Write-Host "  Installed: $bin"
    }

    @(
        "source=direct",
        "install_dir=$InstallDir"
    ) | Set-Content -Path $DirectInstallMarker

    Write-Host "Installed: $($Binaries -join ', ') -> $InstallDir"
}

function Install-Skills {
    $skillsSource = Join-Path $script:ExtractDir "skills"
    if (-not (Test-Path $skillsSource)) {
        throw "Release bundle is missing bundled skills"
    }

    New-Item -ItemType Directory -Force -Path $SkillsDir | Out-Null
    Copy-Item -Path (Join-Path $script:ExtractDir "skills\*") -Destination $SkillsDir -Recurse -Force
    Write-Host "Installed bundled skills -> $SkillsDir"
}

function Install-Guidelines {
    $guidelinesSource = Join-Path $script:ExtractDir "guidelines"
    if (-not (Test-Path $guidelinesSource)) {
        throw "Release bundle is missing bundled guidelines"
    }

    New-Item -ItemType Directory -Force -Path $GuidelinesDir | Out-Null
    Get-ChildItem -Path $guidelinesSource -Recurse -File | ForEach-Object {
        $relativePath = [System.IO.Path]::GetRelativePath($guidelinesSource, $_.FullName)
        $targetPath = Join-Path $GuidelinesDir $relativePath
        if (Test-Path $targetPath) {
            return
        }

        New-Item -ItemType Directory -Force -Path (Split-Path $targetPath -Parent) | Out-Null
        Copy-Item -Path $_.FullName -Destination $targetPath
    }
    Write-Host "Installed missing bundled guidelines -> $GuidelinesDir"
}

function Install-CustomAuthTemplate {
    $rootDir = if ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "tamux" } else { Join-Path $HOME ".tamux" }
    $customAuthPath = Join-Path $rootDir "custom-auth.yaml"
    New-Item -ItemType Directory -Force -Path $rootDir | Out-Null

    if (Test-Path $customAuthPath) {
        return
    }

    @"
# Add named custom providers here. The daemon reloads this file before
# provider/model setup in the TUI and desktop app.
# Prefer api_key_env for secrets, for example:
# providers:
#   - id: local-openai
#     name: Local OpenAI-Compatible
#     default_base_url: http://127.0.0.1:11434/v1
#     default_model: llama3.3
#     api_key_env: LOCAL_OPENAI_API_KEY
providers: []
"@ | Set-Content -Path $customAuthPath -Encoding UTF8
    Write-Host "Created custom provider template -> $customAuthPath"
}

function Start-DaemonAfterUpgrade {
    if ($env:TAMUX_START_DAEMON_AFTER_INSTALL -ne "1") {
        return
    }

    $daemonPath = Join-Path $InstallDir "tamux-daemon.exe"
    if (-not (Test-Path $daemonPath)) {
        throw "Installed daemon binary not found: $daemonPath"
    }

    Write-Host "Starting tamux-daemon..."
    Start-Process -FilePath $daemonPath -WindowStyle Hidden | Out-Null
}

# ---------------------------------------------------------------------------
# PATH update
# ---------------------------------------------------------------------------

function Update-Path {
    $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "Machine")
    if (-not $CurrentPath) {
        $CurrentPath = ""
    }

    # Check if InstallDir already in PATH
    if ($CurrentPath -split ";" | Where-Object { $_.TrimEnd('\\') -ieq $InstallDir.TrimEnd('\\') }) {
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

$script:ArchiveUrl = "$DownloadBaseUrl/v$Version/$script:ArchiveName"
$script:ChecksumUrl = "$DownloadBaseUrl/v$Version/$script:ChecksumName"

if ($DryRun) {
    Write-Host ""
    Write-Host "Platform: $Target"
    Write-Host "Version: $Version"
    Write-Host "Would download: $script:ArchiveUrl"
    Write-Host "Checksum URL: $script:ChecksumUrl"
    Write-Host "Would install to: $InstallDir"
    Write-Host "Would install bundled skills to: $SkillsDir"
    Write-Host "Would install bundled guidelines to: $GuidelinesDir"
    Write-Host "Binaries: $($Binaries -join ', ')"
    Write-Host "Dry run complete -- no files downloaded or modified."
    exit 0
}

try {
    Wait-ForPreviousTamux
    Download-AndVerify
    Install-Binaries
    Install-Skills
    Install-Guidelines
    Install-CustomAuthTemplate
    Update-Path
    Start-DaemonAfterUpgrade
} finally {
    if ($script:TmpDir -and (Test-Path $script:TmpDir)) {
        Remove-Item -Recurse -Force $script:TmpDir -ErrorAction SilentlyContinue
    }
}

Write-Host ""
Write-Host "tamux installed successfully! Run 'tamux' to get started."
