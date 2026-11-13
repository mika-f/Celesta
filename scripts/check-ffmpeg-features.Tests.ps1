#Requires -Modules @{ ModuleName = 'Pester'; ModuleVersion = '5.0' }

# Tests for check-ffmpeg-features.ps1 on status databases shaped like the ones
# vcpkg writes. CI's setup-ffmpeg also runs the script on the pinned packages.
#   Invoke-Pester -Path scripts/check-ffmpeg-features.Tests.ps1

BeforeAll {
  $script:Check = Join-Path $PSScriptRoot 'check-ffmpeg-features.ps1'
  $script:Static = 'x64-windows-static-md-release'
  $script:Dynamic = 'x64-windows-release'

  # One status paragraph the way vcpkg writes it: the core package has no
  # Feature field, and descriptions continue on indented lines.
  function Paragraph([string] $Triplet, [string] $Feature, [string] $Status = 'install ok installed', [string] $Package = 'ffmpeg') {
    $lines = @("Package: $Package")
    if ($Feature) {
      $lines += "Feature: $Feature"
      $lines += "Depends: $Package"
    } else {
      $lines += 'Version: 8.1.2'
      $lines += 'Port-Version: 2'
    }
    $lines += "Architecture: $Triplet"
    $lines += 'Multi-Arch: same'
    if (-not $Feature) { $lines += 'Abi: 0123456789abcdef' }
    $lines += 'Description: a library to decode, encode, transcode, mux, demux,'
    $lines += '    stream, filter and play: pretty much anything'
    $lines += "Status: $Status"
    $lines -join "`n"
  }

  # The core package and each feature on one triplet.
  function Ffmpeg([string] $Triplet, [string[]] $Features) {
    @(Paragraph $Triplet) + @($Features | ForEach-Object { Paragraph $Triplet $_ })
  }

  # An installed/ tree whose vcpkg/status holds the paragraphs, and whose
  # vcpkg/updates holds one file for each entry of $Updates.
  function Tree([string[]] $Paragraphs, [string[]] $Updates = @(), [string] $NewLine = "`n") {
    $root = Join-Path $TestDrive ([Guid]::NewGuid())
    $database = Join-Path $root 'vcpkg'
    New-Item -ItemType Directory -Force (Join-Path $database 'updates') | Out-Null
    $text = (($Paragraphs -join "`n`n") + "`n") -replace "`n", $NewLine
    [IO.File]::WriteAllText((Join-Path $database 'status'), $text)
    for ($i = 0; $i -lt $Updates.Count; $i++) {
      [IO.File]::WriteAllText((Join-Path $database "updates/$('{0:D10}' -f ($i + 1))"), $Updates[$i] + "`n")
    }
    $root
  }

  $script:Standard = @('zlib', 'opus', 'vpx')
  $script:StandardFeatures = @('avcodec', 'avformat', 'swscale') + $Standard
  $script:Lgpl = 'LGPL-2.1-or-later'
  $script:Gpl = 'GPL-2.0-or-later'
}

Describe 'check-ffmpeg-features.ps1' {
  It 'passes a standard package with the features on both triplets' {
    $tree = Tree ((Ffmpeg $Static $StandardFeatures) + (Ffmpeg $Dynamic $StandardFeatures) + (Paragraph $Static -Package zlib))
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Not -Throw
  }

  It 'takes the features as one comma-separated value, as the workflows pass them' {
    $tree = Tree ((Ffmpeg $Static $StandardFeatures) + (Ffmpeg $Dynamic $StandardFeatures))
    { & $Check -InstalledPath $tree -Features 'zlib,opus,vpx' -License $Lgpl } | Should -Not -Throw
  }

  It 'reads a status file with CRLF line endings' {
    $tree = Tree ((Ffmpeg $Static $StandardFeatures) + (Ffmpeg $Dynamic $StandardFeatures)) -NewLine "`r`n"
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Not -Throw
    $tree = Tree ((Ffmpeg $Static ($StandardFeatures + 'gpl')) + (Ffmpeg $Dynamic $StandardFeatures)) -NewLine "`r`n"
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Throw "*ffmpeg``[gpl``]:$Static*"
  }

  It 'refuses gpl on the dynamic triplet only' {
    $tree = Tree ((Ffmpeg $Static $StandardFeatures) + (Ffmpeg $Dynamic ($StandardFeatures + 'gpl')))
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Throw "*the $Lgpl package has ffmpeg``[gpl``]:$Dynamic*"
  }

  It 'refuses an expected feature missing on one triplet' {
    $tree = Tree ((Ffmpeg $Static $StandardFeatures) + (Ffmpeg $Dynamic @('avcodec', 'zlib', 'opus')))
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Throw "*ffmpeg``[vpx``]:$Dynamic is not installed*"
  }

  It 'refuses a missing triplet' {
    $tree = Tree (Ffmpeg $Static $StandardFeatures)
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Throw "*ffmpeg:$Dynamic is not installed*"
  }

  It 'refuses a database with no ffmpeg in it' {
    $tree = Tree @(Paragraph $Static -Package zlib)
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Throw "*ffmpeg:$Static is not installed*"
  }

  It 'refuses a missing status file' {
    $tree = Tree @()
    Remove-Item -LiteralPath (Join-Path $tree 'vcpkg/status')
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Throw
  }

  It 'checks every other triplet with ffmpeg as well' {
    $tree = Tree ((Ffmpeg $Static $StandardFeatures) + (Ffmpeg $Dynamic $StandardFeatures) + (Ffmpeg 'x86-windows' ($StandardFeatures + 'gpl')))
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Throw '*ffmpeg`[gpl`]:x86-windows*'
  }

  It 'ignores features that are not installed' {
    $tree = Tree ((Ffmpeg $Static $StandardFeatures) + (Ffmpeg $Dynamic $StandardFeatures) + (Paragraph $Dynamic gpl 'purge ok not-installed'))
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Not -Throw
  }

  It 'reads the updates vcpkg has not folded into status yet' {
    $tree = Tree ((Ffmpeg $Static $StandardFeatures) + (Ffmpeg $Dynamic @('avcodec', 'zlib', 'opus'))) -Updates @(Paragraph $Dynamic vpx)
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Not -Throw
    $tree = Tree ((Ffmpeg $Static $StandardFeatures) + (Ffmpeg $Dynamic $StandardFeatures)) -Updates @((Paragraph $Dynamic gpl), (Paragraph $Static nonfree))
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Throw "*ffmpeg``[nonfree``]:$Static*ffmpeg``[gpl``]:$Dynamic*"
  }

  It 'lets a later update replace an earlier paragraph' {
    $tree = Tree ((Ffmpeg $Static $StandardFeatures) + (Ffmpeg $Dynamic ($StandardFeatures + 'gpl'))) -Updates @(Paragraph $Dynamic gpl 'purge ok not-installed')
    { & $Check -InstalledPath $tree -Features $Standard -License $Lgpl } | Should -Not -Throw
  }

  It 'passes gpl in a GPL package but refuses version3 and nonfree' {
    $x264 = @('avcodec', 'gpl', 'x264')
    $tree = Tree ((Ffmpeg $Static $x264) + (Ffmpeg $Dynamic $x264))
    { & $Check -InstalledPath $tree -Features x264 -License $Gpl } | Should -Not -Throw
    $tree = Tree ((Ffmpeg $Static ($x264 + 'version3')) + (Ffmpeg $Dynamic ($x264 + 'nonfree')))
    { & $Check -InstalledPath $tree -Features x264 -License $Gpl } | Should -Throw "*ffmpeg``[version3``]:$Static*ffmpeg``[nonfree``]:$Dynamic*"
  }
}
