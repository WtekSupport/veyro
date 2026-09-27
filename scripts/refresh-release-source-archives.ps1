#Requires -Version 5.1
<#
.SYNOPSIS
  Retarget release tags so GitHub "Source code (zip/tar.gz)" archives are stubs only.

  GitHub always shows those two links on every release; they cannot be removed via API.
  This script amends each tagged commit to add .gitattributes (export-ignore) and
  SOURCE-ARCHIVE.txt, then force-updates the tag and pushes to origin.
#>
param(
    [switch]$DryRun,
    [string[]] $Tags
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

function Get-ExportIgnoreGitAttributes {
    $path = Join-Path $repoRoot ".gitattributes"
    if (-not (Test-Path $path)) {
        throw "Missing .gitattributes at repo root."
    }
    return (Get-Content $path -Raw).TrimEnd() + "`n"
}

function Invoke-GitQuiet {
    param([Parameter(Mandatory)] [string[]] $GitArgs)
    $oldEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & git @GitArgs 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) {
            throw "git $($GitArgs -join ' ') failed (exit $LASTEXITCODE)"
        }
    } finally {
        $ErrorActionPreference = $oldEap
    }
}

function Test-TagHasExportStub {
    param([string] $Tag)
    try {
        $attrs = git show "${Tag}:.gitattributes" 2>$null
        $readme = git show "${Tag}:SOURCE-ARCHIVE.txt" 2>$null
        return ($attrs -match "export-ignore") -and ($null -ne $readme)
    } catch {
        return $false
    }
}

function Update-ReleaseTagExportStub {
    param(
        [string] $Tag,
        [string] $AttributesText,
        [string] $ArchiveReadme
    )

    if (Test-TagHasExportStub -Tag $Tag) {
        Write-Host "Skip $Tag (already has export stub)"
        return
    }

    $commit = git rev-parse $Tag
    Write-Host "Retag $Tag ($commit) ..."

    if ($DryRun) { return }

    $prevHead = git rev-parse HEAD
    $prevBranch = git branch --show-current
    try {
        Invoke-GitQuiet -GitArgs @("checkout", "-f", $commit)
        Set-Content -Path (Join-Path $repoRoot ".gitattributes") -Value $AttributesText -Encoding utf8 -NoNewline
        Set-Content -Path (Join-Path $repoRoot "SOURCE-ARCHIVE.txt") -Value $ArchiveReadme -Encoding utf8
        Invoke-GitQuiet -GitArgs @("add", ".gitattributes", "SOURCE-ARCHIVE.txt")
        Invoke-GitQuiet -GitArgs @("commit", "--amend", "--no-edit", "--no-verify")
        $newCommit = git rev-parse HEAD
        Invoke-GitQuiet -GitArgs @("tag", "-fa", $Tag, $newCommit, "-m", "Release $Tag (source archive stub via export-ignore)")
        Write-Host "  -> $newCommit"
    } finally {
        if ($prevBranch) {
            Invoke-GitQuiet -GitArgs @("checkout", "-f", $prevBranch)
        } else {
            Invoke-GitQuiet -GitArgs @("checkout", "-f", $prevHead)
        }
    }
}

if (-not $Tags -or $Tags.Count -eq 0) {
    $Tags = @(git tag -l) | Where-Object { $_ -match '^(v[0-9]|silero-te-)' } | Sort-Object
}

$attributesText = Get-ExportIgnoreGitAttributes
$archiveReadme = (Get-Content (Join-Path $repoRoot "SOURCE-ARCHIVE.txt") -Raw).TrimEnd() + "`n"

foreach ($tag in $Tags) {
    Update-ReleaseTagExportStub -Tag $tag -AttributesText $attributesText -ArchiveReadme $archiveReadme
}

if ($DryRun) {
    Write-Host "Dry run only; no tags changed."
    exit 0
}

Write-Host "Force-pushing updated tags to origin ..."
git push origin --force --tags

Write-Host "Done. Release pages still list Source code links; downloads are stub archives only."
