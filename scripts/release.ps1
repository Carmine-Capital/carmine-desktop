#!/usr/bin/env pwsh
#Requires -Version 5.1
<#
.SYNOPSIS
    Release script for carmine-desktop (PowerShell equivalent of release.sh).

.EXAMPLE
    ./scripts/release.ps1 0.2.0
    ./scripts/release.ps1 0.2.0-rc.1
    ./scripts/release.ps1 0.2.0 -UploadOnly

.NOTES
    Requires rsync on PATH (e.g. via scoop install rsync, MSYS2, or Git Bash).
    Compatible with Windows PowerShell 5.1 and PowerShell 7+.
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Version,

    [switch]$UploadOnly
)

$ErrorActionPreference = 'Stop'

function Assert-LastExitZero {
    param([string]$What)
    if ($LASTEXITCODE -ne 0) { throw "$What failed (exit $LASTEXITCODE)" }
}

# Write UTF-8 without BOM, LF line endings — matches jq/sed behaviour from release.sh.
function Write-TextFileUtf8NoBom {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Content,
        [switch]$NoNewline
    )
    if (-not $NoNewline -and -not $Content.EndsWith("`n")) {
        $Content = $Content + "`n"
    }
    $utf8 = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, $Content, $utf8)
}

$RepoRoot = (git rev-parse --show-toplevel).Trim()
Assert-LastExitZero 'git rev-parse'
$CargoToml = Join-Path $RepoRoot 'Cargo.toml'
$TauriConf = Join-Path $RepoRoot 'crates/carminedesktop-app/tauri.conf.json'

$UploadHost = 'static.carminecapital.com'
$UploadPath = '/var/www/users/carminec/carmine-desktop'

$currentVersion = (Get-Content $TauriConf -Raw | ConvertFrom-Json).version

# --- Usage ---
if (-not $Version) {
    $name = $MyInvocation.MyCommand.Name
    Write-Host "Usage: $name <version> [-UploadOnly]"
    Write-Host ""
    Write-Host "  Current version: $currentVersion"
    Write-Host "  Example: $name 0.2.0"
    Write-Host "  Example: $name 0.2.0-rc.1"
    Write-Host "  Example: $name 0.2.0 -UploadOnly   (skip version bump, just upload)"
    exit 1
}

$newVersion = $Version
$tag = "v$newVersion"

if ($UploadOnly) {
    Write-Host "=== Upload Only Mode ==="
    Write-Host ""
    Write-Host "Uploading local build artifacts to $UploadHost..."

    $ArtifactsDir = Join-Path $RepoRoot 'target/release/bundle'
    if (-not (Test-Path $ArtifactsDir)) {
        Write-Host "ERROR: No build artifacts found at $ArtifactsDir"
        Write-Host "       Run 'cargo tauri build --features desktop' first."
        exit 1
    }

    # Collect artifacts in a staging dir
    $StagingDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $StagingDir | Out-Null

    try {
        Get-ChildItem -Path $ArtifactsDir -Recurse -File |
            Where-Object {
                $_.Name -like '*.exe' -or
                $_.Name -like '*.exe.sig' -or
                $_.Name -like '*.nsis.zip' -or
                $_.Name -like '*.nsis.zip.sig'
            } |
            ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $StagingDir }

        Write-Host "Staged artifacts:"
        Get-ChildItem -LiteralPath $StagingDir | Format-Table Mode, Length, Name -AutoSize | Out-Host

        # --- Generate latest.json for Tauri updater ---
        $BaseUrl = "https://$UploadHost/carmine-desktop"
        $PubDate = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')

        # Detect Windows updater bundle. Prefer .nsis.zip over raw setup.exe.
        $WinBundle = Get-ChildItem -LiteralPath $StagingDir -File -Filter '*.nsis.zip' |
            Where-Object { $_.Name -notlike '*.sig' } |
            Select-Object -First 1
        if (-not $WinBundle) {
            $WinBundle = Get-ChildItem -LiteralPath $StagingDir -File -Filter '*-setup.exe' |
                Select-Object -First 1
        }

        $WinSig = ''
        $WinFilename = ''
        if ($WinBundle) {
            $sigPath = "$($WinBundle.FullName).sig"
            if (Test-Path -LiteralPath $sigPath) {
                $WinSig = (Get-Content -LiteralPath $sigPath -Raw).Trim()
            }
            $WinFilename = $WinBundle.Name
        }

        if ([string]::IsNullOrEmpty($WinSig) -or [string]::IsNullOrEmpty($WinFilename)) {
            Write-Host ""
            Write-Host "ERROR: No signed Windows updater bundle found in $StagingDir"
            Write-Host "       Expected *.nsis.zip + .sig or *-setup.exe + .sig"
            exit 1
        }

        $manifest = [ordered]@{
            version   = $newVersion
            notes     = "Release v$newVersion"
            pub_date  = $PubDate
            platforms = [ordered]@{
                'windows-x86_64' = [ordered]@{
                    signature = $WinSig
                    url       = "$BaseUrl/$WinFilename"
                }
            }
        }
        $manifestPath = Join-Path $StagingDir 'latest.json'
        $manifestJson = $manifest | ConvertTo-Json -Depth 10
        Write-TextFileUtf8NoBom -Path $manifestPath -Content $manifestJson

        Write-Host ""
        Write-Host "=== latest.json ==="
        Get-Content -LiteralPath $manifestPath | Write-Host

        # Upload artifacts + manifest
        # Convert Windows path to forward-slash form for rsync (MSYS/Cygwin-friendly).
        $rsyncSrc = ($StagingDir -replace '\\', '/').TrimEnd('/') + '/'
        & rsync -avz --chmod=D755,F644 $rsyncSrc "carminec@${UploadHost}:${UploadPath}/"
        Assert-LastExitZero 'rsync'

        Write-Host ""
        Write-Host "Done. Artifacts uploaded to https://$UploadHost/carmine-desktop/"
        Write-Host "Updater manifest: https://$UploadHost/carmine-desktop/latest.json"
    }
    finally {
        Remove-Item -LiteralPath $StagingDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    exit 0
}

# --- Preflight checks ---
$currentBranch = (git branch --show-current).Trim()
Assert-LastExitZero 'git branch --show-current'
if ($currentBranch -ne 'main') {
    Write-Host "ERROR: Releases must be created from the main branch."
    Write-Host "       Current branch: $currentBranch"
    exit 1
}

$dirty = git status --porcelain
if ($dirty) {
    Write-Host "ERROR: Working tree is dirty. Commit or stash changes first."
    exit 1
}

& git rev-parse $tag *> $null
if ($LASTEXITCODE -eq 0) {
    Write-Host "ERROR: Tag $tag already exists."
    exit 1
}
$global:LASTEXITCODE = 0

# --- Summary & confirmation ---
Write-Host "=== Release ==="
Write-Host ""
Write-Host "  Current version : $currentVersion"
Write-Host "  New version     : $newVersion"
Write-Host "  Tag             : $tag"
Write-Host "  Branch          : $currentBranch"
Write-Host "  Upload target   : https://$UploadHost/carmine-desktop/"
Write-Host ""
Write-Host "This will:"
Write-Host "  1. Update version in Cargo.toml and tauri.conf.json"
Write-Host "  2. Regenerate Cargo.lock"
Write-Host "  3. Commit the version bump (Cargo.toml, tauri.conf.json, Cargo.lock)"
Write-Host "  4. Create tag $tag"
Write-Host "  5. Push commit and tag to origin (triggers release workflow)"
Write-Host "  6. Release workflow builds + uploads to $UploadHost"
Write-Host ""
$confirm = Read-Host "Proceed? [y/N]"
if ($confirm -notmatch '^[Yy]$') {
    Write-Host "Aborted."
    exit 0
}

# --- Update versions ---
$escapedCurrent = [regex]::Escape($currentVersion)
$cargoContent = Get-Content -LiteralPath $CargoToml -Raw
$cargoContent = $cargoContent -replace "(?m)^version = `"$escapedCurrent`"", "version = `"$newVersion`""
Write-TextFileUtf8NoBom -Path $CargoToml -Content $cargoContent -NoNewline

$tauriJson = Get-Content -LiteralPath $TauriConf -Raw | ConvertFrom-Json
$tauriJson.version = $newVersion
$tauriJsonText = $tauriJson | ConvertTo-Json -Depth 100
Write-TextFileUtf8NoBom -Path $TauriConf -Content $tauriJsonText

# --- Verify substitutions ---
$escapedNew = [regex]::Escape($newVersion)
if (-not (Select-String -Path $CargoToml -Pattern "^version = `"$escapedNew`"" -Quiet)) {
    Write-Host "ERROR: Failed to update version in Cargo.toml (expected version = `"$newVersion`")"
    git checkout -- $CargoToml $TauriConf
    exit 1
}
$confVersion = (Get-Content -LiteralPath $TauriConf -Raw | ConvertFrom-Json).version
if ($confVersion -ne $newVersion) {
    Write-Host "ERROR: Failed to update version in tauri.conf.json (got $confVersion, expected $newVersion)"
    git checkout -- $CargoToml $TauriConf
    exit 1
}

# --- Regenerate Cargo.lock ---
Write-Host "Regenerating Cargo.lock..."
& cargo generate-lockfile --quiet
Assert-LastExitZero 'cargo generate-lockfile'

# --- Commit, tag, push ---
$CargoLock = Join-Path $RepoRoot 'Cargo.lock'
git add $CargoToml $TauriConf $CargoLock
Assert-LastExitZero 'git add'
git commit -m "Bump version to $newVersion"
Assert-LastExitZero 'git commit'
git tag $tag
Assert-LastExitZero 'git tag'
git push origin $currentBranch $tag
Assert-LastExitZero 'git push'

Write-Host ""
Write-Host "Done. Release workflow triggered for $tag."
Write-Host "Watch it with: gh run list --limit 1"
Write-Host "Artifacts will be uploaded to: https://$UploadHost/carmine-desktop/"
