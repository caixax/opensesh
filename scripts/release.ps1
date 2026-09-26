<#
.SYNOPSIS
Cuts a release from this machine: Windows packages here, Linux packages in the WSL distros, then
the tag and the GitHub release. Much faster than the GitHub Actions release workflow, which stays
as the fallback.

.DESCRIPTION
Picks the next version (or the one given), writes it into Cargo.toml, moves the CHANGELOG's
[Unreleased] section under it, runs the tests, builds the portable zip and the NSIS installer
(cargo xtask dist windows), builds the Linux packages inside the WSL distros of this machine
(scripts/linux/build.sh, one distro per family, at the same time), writes SHA256SUMS.txt, commits
and tags vX.Y.Z, pushes, and publishes the GitHub release with gh. Installed copies of the app see
the release on their next update check.

.PARAMETER Patch
Bump the patch number (default when nothing else is given).
.PARAMETER Minor
Bump the minor number and reset patch.
.PARAMETER Major
Bump the major number and reset minor and patch.
.PARAMETER Version
Exact version to release, for example 0.3.0. Alias: -V.
.PARAMETER SkipTests
Do not run cargo test first.
.PARAMETER NoPublish
Build, commit and tag, but do not push or create the GitHub release.
.PARAMETER SkipLinux
Release Windows only.
.PARAMETER LinuxDistros
WSL distros to build in: Debian 13 (.deb), Fedora (.rpm) and Arch (pacman package).

.EXAMPLE
scripts\release.bat -Patch
scripts\release.bat -V 0.1.0
#>
[CmdletBinding()]
param(
    [switch]$Patch,
    [switch]$Minor,
    [switch]$Major,
    [Alias("V")][string]$Version,
    [switch]$SkipTests,
    [switch]$NoPublish,
    [switch]$SkipLinux,
    [string[]]$LinuxDistros = @("Debian", "FedoraLinux-43", "archlinux")
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

# Qt for the Windows build (aqtinstall layout); QMAKE wins when it is already set.
if (-not $env:QMAKE) { $env:QMAKE = "C:\Qt\6.10.3\msvc2022_64\bin\qmake.exe" }
if (-not (Test-Path $env:QMAKE)) { throw "qmake not found at $env:QMAKE; set QMAKE" }
$env:PATH = (Split-Path -Parent $env:QMAKE) + ";" + $env:PATH

function Step($text) { Write-Host "`n== $text" -ForegroundColor Cyan }
function Run($command) {
    Write-Host "> $command" -ForegroundColor DarkGray
    Invoke-Expression $command
    if ($LASTEXITCODE -ne 0) { throw "command failed: $command" }
}

Step "Checking the working tree"
if (git status --porcelain) { throw "the working tree has uncommitted changes; commit or stash them first" }
$branch = (git rev-parse --abbrev-ref HEAD).Trim()
if ($branch -ne "main") { throw "releases are cut from main (current branch: $branch)" }

Step "Picking the version"
$toml = Get-Content Cargo.toml -Raw
$current = [regex]::Match($toml, '(?m)^version = "([^"]+)"').Groups[1].Value
if ($Version) {
    if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "version must look like 1.2.3" }
    $next = $Version
} else {
    $parts = $current.Split('.') | ForEach-Object { [int]$_ }
    if ($Major) { $parts[0]++; $parts[1] = 0; $parts[2] = 0 }
    elseif ($Minor) { $parts[1]++; $parts[2] = 0 }
    else { $parts[2]++ }
    $next = "$($parts[0]).$($parts[1]).$($parts[2])"
}
if (git tag --list "v$next") { throw "tag v$next already exists" }
Write-Host "Releasing $current -> $next"
$toml = [regex]::Replace($toml, '(?m)^version = "[^"]+"', "version = `"$next`"", 1)
[IO.File]::WriteAllText((Join-Path $repo "Cargo.toml"), $toml)
Run "cargo update --workspace --quiet"

# The changelog: what was under [Unreleased] now belongs to this version.
$changelog = [IO.File]::ReadAllText((Join-Path $repo "CHANGELOG.md"))
$date = Get-Date -Format "yyyy-MM-dd"
if ($changelog -notmatch "(?m)^## \[$([regex]::Escape($next))\]") {
    $changelog = [regex]::Replace($changelog, '(?m)^## \[Unreleased\][ \t]*$', "## [Unreleased]`n`n## [$next] - $date", 1)
    [IO.File]::WriteAllText((Join-Path $repo "CHANGELOG.md"), $changelog)
}

if (-not $SkipTests) {
    Step "Running the tests"
    Run "cargo test --workspace --quiet"
}

Step "Building the Windows packages"
if (Test-Path dist) { Remove-Item -Recurse -Force dist }
New-Item -ItemType Directory -Force dist | Out-Null
Run "cargo xtask dist windows"
Copy-Item "target\dist\OpenSesh-$next-windows-x64-portable.zip", "target\dist\OpenSesh-$next-windows-x64-setup.exe" dist\

if (-not $SkipLinux) {
    Step "Building the Linux packages in WSL ($($LinuxDistros -join ', '))"
    # The checkout as WSL sees it: I:\Projects\opensesh -> /mnt/i/Projects/opensesh.
    $drive = $repo.Substring(0, 1).ToLower()
    $script = "/mnt/$drive" + ($repo.Substring(2) -replace '\\', '/') + "/scripts/linux/build.sh"
    $jobs = foreach ($distro in $LinuxDistros) {
        Write-Host "> wsl -d $distro -- bash $script" -ForegroundColor DarkGray
        Start-Job -Name $distro -ArgumentList $distro, $script -ScriptBlock {
            param($distro, $script)
            $output = wsl.exe -d $distro -- bash $script 2>&1
            [pscustomobject]@{ Code = $LASTEXITCODE; Tail = ($output | Select-Object -Last 25) -join "`n" }
        }
    }
    $failed = @()
    foreach ($job in $jobs) {
        $result = Receive-Job -Job $job -Wait -AutoRemoveJob
        if ($result.Code -ne 0) {
            $failed += $job.Name
            Write-Host "`n--- $($job.Name) failed:`n$($result.Tail)" -ForegroundColor Red
        } else {
            Write-Host "$($job.Name): ok" -ForegroundColor Green
        }
    }
    if ($failed.Count -gt 0) {
        # Nothing is committed yet; put the version back so a retry starts clean.
        git checkout -- Cargo.toml Cargo.lock CHANGELOG.md
        throw "the Linux build failed in: $($failed -join ', ')"
    }
}

Step "Writing checksums"
$lines = Get-ChildItem dist -File | Where-Object { $_.Name -match '\.(exe|zip|deb|rpm|zst)$' } | Sort-Object Name | ForEach-Object {
    "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower())  $($_.Name)"
}
# LF only, also at the end: a CR glued to the last file name breaks sha256sum -c and install.sh.
[IO.File]::WriteAllText((Join-Path $repo "dist\SHA256SUMS.txt"), (($lines -join "`n") + "`n"), [Text.Encoding]::ASCII)
Get-Content dist\SHA256SUMS.txt

Step "Committing and tagging v$next"
Run "git add Cargo.toml Cargo.lock CHANGELOG.md"
git diff --cached --quiet
if ($LASTEXITCODE -ne 0) { Run "git commit -q -m `"chore: release v$next`"" }
Run "git tag v$next"

if ($NoPublish) {
    Write-Host "`nDone. Nothing was pushed (-NoPublish)." -ForegroundColor Yellow
    exit 0
}

Step "Pushing"
Run "git push origin main"
Run "git push origin v$next"

Step "Publishing the GitHub release"
$section = [regex]::Match($changelog, "(?ms)^## \[$([regex]::Escape($next))\][^\n]*\n(.*?)(?=^## \[|\z)").Groups[1].Value.Trim()
$notes = @"
## Install

- **Windows 10/11:** ``OpenSesh-$next-windows-x64-setup.exe`` (per-user install, no administrator rights, updates itself), or the portable ``OpenSesh-$next-windows-x64-portable.zip``.
- **Linux** (Debian 13, Fedora, Arch):
  ``````sh
  curl -fsSL https://raw.githubusercontent.com/caixax/opensesh/main/install.sh | bash
  ``````
  or install the ``.deb``, ``.rpm`` or ``.pkg.tar.zst`` below with your package manager.

Check the downloads against ``SHA256SUMS.txt``.

## Changes

$section
"@
$notesFile = Join-Path $repo "dist\release-notes.md"
[IO.File]::WriteAllText($notesFile, $notes)
$assets = Get-ChildItem dist -File | Where-Object { $_.Name -match '\.(exe|zip|deb|rpm|zst)$' -or $_.Name -eq 'SHA256SUMS.txt' } | ForEach-Object { "dist/$($_.Name)" }
Run "gh release create v$next --title `"OpenSesh v$next`" --notes-file `"$notesFile`" $($assets -join ' ')"
Write-Host "`nReleased v$next" -ForegroundColor Green
