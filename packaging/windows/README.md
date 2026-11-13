# Build a Windows package

Run from a Windows x64 checkout using PowerShell 7. The package contains
`Celesta.exe`, `Celesta-export.exe`, Node.js, the React runtime and its
dependencies (including native esbuild), and the MSVC runtime DLLs.
Users do not need Node.js, pnpm, Rust, or FFmpeg installed.

## Prerequisites

- The `x86_64-pc-windows-msvc` Rust toolchain and Visual Studio C++ build tools.
- A standard LGPL vcpkg FFmpeg 8.1.x install: `ffmpeg[zlib,opus,vpx]` with
  `x64-windows-static-md`, using the vcpkg release
  [`ffmpeg-windows.yml`](../../.github/workflows/ffmpeg-windows.yml) names
  (CI downloads it as the `Celesta.FFmpeg.Windows.Standard` package). Set
  `VCPKG_ROOT` to this install. Never add x264 to it: the separate
  `ffmpeg-x264` ZIP uses its own native root (`Celesta.FFmpeg.Windows.X264`);
  see [plugin packaging](../plugins/README.md).
- Node.js and pnpm for building the React package.
- [Inno Setup 6.3 or later](https://jrsoftware.org/isdl.php) for the setup EXE.
  Use `-ZipOnly` if you only need a portable ZIP.

## Build

From the repository root:

```powershell
pwsh -File scripts/package-windows.ps1 -Version 0.1.0
```

If the Inno compiler or MSVC redistributable is installed elsewhere:

```powershell
pwsh -File scripts/package-windows.ps1 -Version 0.1.0 `
  -IsccPath 'C:\Tools\Inno Setup 6\ISCC.exe' `
  -VcRedistDirectory 'C:\path\to\x64\Microsoft.VC143.CRT'
```

The script installs the locked JavaScript dependencies, generates bindings,
builds TypeScript and release executables, and downloads the pinned Node.js
26.12.1 Windows x64 archive from nodejs.org. Its SHA256 is checked against the
official release checksums before extraction. `-NodeVersion` selects another
version; downloaded archives and checksums are cached in
`target/package-downloads`.

Outputs are placed in `target/packages`:

- `Celesta-<version>-windows-x64.zip`
- `Celesta-<version>-windows-x64-setup.exe`
- A unique `staging-*/Celesta` directory for inspecting the package.

Existing output files are never overwritten. `-SkipBuild` reuses already-built
`target/release` executables and the built `packages/*/dist` runtime; the media plugins
(`plugins/ffmpeg` and `plugins/mediafoundation`) are always built, which costs nothing when
`plugins/target` is current. Use it only after building the
current sources. A custom Cargo target directory is not supported.

The installer installs for the current user without elevation, adds a Start menu
shortcut, and registers an uninstaller. It does not change PATH, install Node.js
globally, or associate project file extensions.

## Verification

Before creating the archives, the script automatically runs a React composition
with hooks and exports a short ProRes/MOV using only the bundled executables. The test
uses a separate directory containing spaces and Japanese characters, removes
Node.js from PATH, and checks that runtime files contain no pnpm symlinks.
Re-run it against an extracted package:

```powershell
& 'C:\path\to\Celesta\runtime\node.exe' `
  scripts/test-package.mjs 'C:\path\to\Celesta'
```

GPU access is needed for the MOV/ProRes check. Also launch `Celesta.exe` on a clean
Windows machine and check preview, audio, installation, upgrade, and uninstall
before publishing. The automated export check does not replace a GUI check.

## Public releases

These are unsigned development packages. Code signing is not configured.
The package collects Node.js and JavaScript license texts, native vcpkg notices,
and available Rust license texts plus a dependency inventory, and includes the
Celesta's `LICENSE-MIT` and `LICENSE-APACHE` in the package-wide license folder.
FFmpeg license texts and source notices stay in the corresponding plugin's
`licenses/` folder.

Celesta's own executables link no FFmpeg. Only the standard LGPL process
lives under `plugins/ffmpeg`, with its own CRT DLLs. The app package's native
notices come only from the standard installation under `licenses/native`, and
its Rust inventory excludes the optional x264 crate. The smoke test checks
that x264 was not bundled and exports MOV/ProRes and WebM (VP9 and Opus).

The optional `ffmpeg-x264` archive is a separate asset on the same GitHub
release, built with
[`package-media-plugins.py`](../../scripts/package-media-plugins.py). It
contains one installable plugin folder with its CRT, native libraries, license
texts and notices, including the [GPL offer](../GPL-SOURCE-OFFER.md).
See [plugin packaging](../plugins/README.md) for build and installation details.

`plugins\mediafoundation` holds the Media Foundation plugin
(`celesta-codec-mediafoundation`): the H.264, HEVC and AAC encoders and the
decoder of Windows' own Media Foundation. Its program links only Windows
system libraries, so the folder carries MIT/Apache-2.0 alone
(`plugins\mediafoundation\licenses`), and the program gets its own copies of
the MSVC runtime DLLs beside it. The plugin has no muxer: its encoders write
into the FFmpeg plugin's MP4. The shared build script builds it after
`plugins/ffmpeg`.
