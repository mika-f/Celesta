#requires -Version 7.0
[CmdletBinding()]
param(
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version = '0.0.0',
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$NodeVersion = '24.20.0',
    [string]$IsccPath,
    [string]$VcRedistDirectory,
    [switch]$ZipOnly,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'Run this script on Windows x64 with PowerShell 7.' }
$root = Split-Path $PSScriptRoot -Parent
$output = Join-Path $root 'target/packages'
$downloads = Join-Path $root 'target/package-downloads'
New-Item -ItemType Directory -Force $output, $downloads | Out-Null
$zipPath = Join-Path $output "Celesta-$Version-windows-x64.zip"
$setupPath = Join-Path $output "Celesta-$Version-windows-x64-setup.exe"
if ((Test-Path -LiteralPath $zipPath) -or (-not $ZipOnly -and (Test-Path -LiteralPath $setupPath))) {
    throw 'Output already exists. Use another -Version or move the previous artifacts.'
}
# Unique staging directories avoid deleting previous packages or merging stale files.
$stage = Join-Path $output ('staging-' + [guid]::NewGuid().ToString('N'))
$package = Join-Path $stage 'Celesta'
$runtime = Join-Path $package 'runtime'
New-Item -ItemType Directory -Force $runtime | Out-Null

function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}

if (-not $ZipOnly) {
    if (-not $IsccPath) {
        $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
        if ($command) { $IsccPath = $command.Source }
        else { $IsccPath = "${env:ProgramFiles(x86)}/Inno Setup 6/ISCC.exe" }
    }
    if (-not (Test-Path -LiteralPath $IsccPath)) {
        throw 'Install Inno Setup 6.3+ and pass -IsccPath, or use -ZipOnly.'
    }
}

$vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
if (-not $VcRedistDirectory) {
    $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $installation) { throw 'MSVC build tools were not found.' }
    $redist = Get-ChildItem -LiteralPath "$installation/VC/Redist/MSVC" -Directory |
        Where-Object Name -Match '^\d+\.' | Sort-Object Name -Descending | Select-Object -First 1
    $crt = Get-ChildItem -Path "$($redist.FullName)/x64/Microsoft.VC*.CRT" -Directory | Select-Object -First 1
    $VcRedistDirectory = $crt.FullName
}
if (-not (Test-Path -LiteralPath "$VcRedistDirectory/vcruntime140.dll")) {
    throw 'Pass -VcRedistDirectory pointing to the MSVC x64 CRT redistributable directory.'
}

Push-Location $root
try {
    $compiler = & rustc -vV
    if ($LASTEXITCODE -ne 0 -or -not ($compiler -contains 'host: x86_64-pc-windows-msvc')) {
        throw 'Use the x86_64-pc-windows-msvc Rust toolchain.'
    }
    if (-not $SkipBuild) {
        Invoke-Checked pnpm @('--dir', 'packages/react', 'install', '--frozen-lockfile')
        Invoke-Checked pnpm @('--dir', 'packages/react', 'run', 'codegen')
        Invoke-Checked pnpm @('--dir', 'packages/react', 'run', 'build')
        Invoke-Checked cargo @('build', '--release', '--locked', '-p', 'mikan-editor', '-p', 'mikan-exporter')
    }
    $binaries = Join-Path $root 'target/release'
    Copy-Item -LiteralPath "$binaries/mikan-editor.exe" -Destination "$package/Celesta.exe"
    Copy-Item -LiteralPath "$binaries/mikan-exporter.exe" -Destination "$package/Celesta-export.exe"
    Copy-Item -Path "$VcRedistDirectory/*.dll" -Destination $package

    $archiveName = "node-v$NodeVersion-win-x64.zip"
    $archive = Join-Path $downloads $archiveName
    $checksums = Join-Path $downloads "node-v$NodeVersion-SHASUMS256.txt"
    $baseUrl = "https://nodejs.org/dist/v$NodeVersion"
    if (-not (Test-Path -LiteralPath $archive)) {
        Invoke-WebRequest "$baseUrl/$archiveName" -OutFile $archive
    }
    if (-not (Test-Path -LiteralPath $checksums)) {
        Invoke-WebRequest "$baseUrl/SHASUMS256.txt" -OutFile $checksums
    }
    $expected = Get-Content -LiteralPath $checksums | Where-Object { $_ -match "  $([regex]::Escape($archiveName))$" }
    if (-not $expected -or ($expected -split '\s+')[0] -ne (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash) {
        throw "Node.js SHA256 verification failed: $archive"
    }
    # Extract only the runtime and its full license; npm is unnecessary at runtime.
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        foreach ($name in @('node.exe', 'LICENSE')) {
            $entry = $zip.GetEntry("node-v$NodeVersion-win-x64/$name")
            if (-not $entry) { throw "Missing $name in Node.js archive" }
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $runtime $name))
        }
    } finally { $zip.Dispose() }
    Invoke-Checked "$runtime/node.exe" @("$PSScriptRoot/stage-react-runtime.mjs", "$runtime/react")

    Copy-Item -LiteralPath "$root/README.md" -Destination $package
    New-Item -ItemType Directory "$package/examples" | Out-Null
    Copy-Item -LiteralPath "$root/examples/minimal.mikan.json", "$root/examples/editor-demo.mikan.json", "$root/packages/react/examples/title.tsx" -Destination "$package/examples"
    Copy-Item -LiteralPath "$root/packaging/windows/README.txt" -Destination "$package/START-HERE.txt"

    $licenses = Join-Path $package 'licenses'
    New-Item -ItemType Directory $licenses | Out-Null
    if (-not $env:VCPKG_ROOT) { throw 'VCPKG_ROOT is required to collect native library licenses.' }
    Copy-Item -LiteralPath "$env:VCPKG_ROOT/installed/x64-windows-static-md/share" -Destination "$licenses/native" -Recurse
    $metadataText = & cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
    $metadata = $metadataText | ConvertFrom-Json
    $metadata.packages | Select-Object name, version, license, repository | ConvertTo-Json -Depth 5 |
        Set-Content "$licenses/rust-packages.json"
    foreach ($crate in $metadata.packages) {
        $directory = Split-Path $crate.manifest_path -Parent
        $texts = @(Get-ChildItem -LiteralPath $directory -File | Where-Object Name -Match '^(LICENSE|COPYING|NOTICE)')
        if ($crate.license_file) { $texts += Get-Item -LiteralPath (Join-Path $directory $crate.license_file) }
        if ($texts.Count) {
            $destination = Join-Path $licenses "rust/$($crate.name)-$($crate.version)"
            New-Item -ItemType Directory -Force $destination | Out-Null
            $texts | Copy-Item -Destination $destination
        }
    }
    Invoke-Checked "$runtime/node.exe" @("$PSScriptRoot/test-package.mjs", $package)
    # Cargo registry archives sometimes contain timestamps outside ZIP's range.
    Get-ChildItem -LiteralPath $package -Recurse -File | Where-Object { $_.LastWriteTime.Year -lt 1980 } |
        ForEach-Object { $_.LastWriteTime = [datetime]'1980-01-01' }
    Compress-Archive -LiteralPath $package -DestinationPath $zipPath
    if (-not $ZipOnly) {
        Invoke-Checked $IsccPath @('/Qp', "/DPackageDir=$package", "/DOutputDir=$output", "/DAppVersion=$Version", "$root/packaging/windows/Celesta.iss")
    }
    Write-Host "Package: $package"
    Write-Host "ZIP: $zipPath"
} finally { Pop-Location }
