<#
.SYNOPSIS
Checks the ffmpeg features in a vcpkg installed/ tree against the licence a
Celesta.FFmpeg.Windows package is published under.

.DESCRIPTION
The licence follows from the ffmpeg features vcpkg installed, which a
dependency can add to, and vcpkg resolves features per triplet. So every
triplet in the tree must have every expected feature and none of the
forbidden ones: neither package may be version3 (LGPL/GPL 3) or nonfree, and
an LGPL one may not be gpl.

.github/workflows/ffmpeg-windows.yml runs it before publishing, since a
published version cannot be replaced, and .github/actions/setup-ffmpeg runs
it on the pinned packages it downloads, so pull requests check the status
parser against real packages. Throws with every problem it finds.

.EXAMPLE
./scripts/check-ffmpeg-features.ps1 -InstalledPath "$env:VCPKG_ROOT/installed" -Features zlib,opus,vpx -License LGPL-2.1-or-later
#>
param(
  # vcpkg's installed/ directory, which holds the vcpkg/status database.
  [Parameter(Mandatory)] [string] $InstalledPath,
  # The ffmpeg features the package was built with; commas also separate.
  [Parameter(Mandatory)] [string[]] $Features,
  # The package's SPDX licence expression.
  [Parameter(Mandatory)] [string] $License,
  # Triplets that must be installed. Any other triplet with ffmpeg in the
  # tree is checked too, since the package ships the whole tree.
  [string[]] $Triplets = @('x64-windows-static-md-release', 'x64-windows-release')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$expected = @($Features | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$forbidden = @('nonfree', 'version3')
if ($License -notlike 'GPL*') { $forbidden += 'gpl' }

# vcpkg's database is Debian-style paragraphs, separated by blank lines, of
# "Key: value" lines; a value continues on lines that start with a space.
# vcpkg appends installs to installed/vcpkg/updates/ and folds them into
# status the next time it loads the database, so read the updates after
# status, in order: a later paragraph for the same package, feature, and
# triplet replaces an earlier one.
$database = Join-Path $InstalledPath 'vcpkg'
$files = @(Join-Path $database 'status')
$updates = Join-Path $database 'updates'
if (Test-Path -LiteralPath $updates) {
  $files += @(Get-ChildItem -LiteralPath $updates -File | Sort-Object Name | ForEach-Object FullName)
}
$paragraphs = [ordered]@{}
foreach ($file in $files) {
  $text = Get-Content -Raw -LiteralPath $file
  if (-not $text) { continue }
  foreach ($paragraph in ($text -split '\r?\n(?:[ \t]*\r?\n)+')) {
    $fields = @{}
    foreach ($line in ($paragraph -split '\r?\n')) {
      if ($line -match '^([A-Za-z-]+):[ \t]*(.*)$') { $fields[$Matches[1]] = $Matches[2].Trim() }
    }
    if (-not $fields['Package'] -or -not $fields['Architecture']) { continue }
    $paragraphs["$($fields['Package'])|$($fields['Feature'])|$($fields['Architecture'])"] = $fields
  }
}
$ffmpeg = @($paragraphs.Values | Where-Object { $_['Package'] -eq 'ffmpeg' -and $_['Status'] -eq 'install ok installed' })

$problems = @()
foreach ($triplet in @($Triplets + @($ffmpeg | ForEach-Object { $_['Architecture'] }) | Select-Object -Unique)) {
  $installed = @($ffmpeg | Where-Object { $_['Architecture'] -eq $triplet })
  if (-not @($installed | Where-Object { -not $_['Feature'] })) {
    $problems += "ffmpeg:$triplet is not installed"
    continue
  }
  $present = @($installed | Where-Object { $_['Feature'] } | ForEach-Object { $_['Feature'] } | Sort-Object)
  Write-Host "ffmpeg:$triplet features installed: $($present -join ', ')"
  foreach ($feature in $expected) {
    if ($present -notcontains $feature) { $problems += "ffmpeg[$feature]:$triplet is not installed" }
  }
  foreach ($feature in $forbidden) {
    if ($present -contains $feature) { $problems += "the $License package has ffmpeg[$feature]:$triplet" }
  }
}
if ($problems) { throw ($problems -join "`n") }
