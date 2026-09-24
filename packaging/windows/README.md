# Build a Windows package

Run from a Windows x64 checkout using PowerShell 7. The package contains
`Celesta.exe`, `Celesta-export.exe`, Node.js, the React runtime and its
dependencies (including native esbuild), and the MSVC runtime DLLs.
Users do not need Node.js, pnpm, Rust, or FFmpeg installed.

## Prerequisites

- The `x86_64-pc-windows-msvc` Rust toolchain and Visual Studio C++ build tools.
- FFmpeg built with `vcpkg install ffmpeg[x264]:x64-windows-static-md` and
  `VCPKG_ROOT` pointing to that installation. Use a vcpkg release whose
  `ffmpeg` port is 8.1.x, such as `2026.07.29`.
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
24.20.0 Windows x64 archive from nodejs.org. Its SHA256 is checked against the
official release checksums before extraction. `-NodeVersion` selects another
version; downloaded archives and checksums are cached in
`target/package-downloads`.

Outputs are placed in `target/packages`:

- `Celesta-<version>-windows-x64.zip`
- `Celesta-<version>-windows-x64-setup.exe`
- A unique `staging-*/Celesta` directory for inspecting the package.

Existing output files are never overwritten. `-SkipBuild` reuses already-built
`target/release` executables and `packages/react/dist`; use it only after building
the current sources. A custom Cargo target directory is not supported.

The installer installs for the current user without elevation, adds a Start menu
shortcut, and registers an uninstaller. It does not change PATH, install Node.js
globally, or associate project file extensions.

## Verification

Before creating the archives, the script automatically runs a React composition
with hooks and exports a short MP4 using only the bundled executables. The test
uses a separate directory containing spaces and Japanese characters, removes
Node.js from PATH, and checks that runtime files contain no pnpm symlinks.
Re-run it against an extracted package:

```powershell
& 'C:\path\to\Celesta\runtime\node.exe' `
  scripts/test-package.mjs 'C:\path\to\Celesta'
```

GPU access is needed for the MP4 check. Also launch `Celesta.exe` on a clean
Windows machine and check preview, audio, installation, upgrade, and uninstall
before publishing. The automated export check does not replace a GUI check.

## Public releases

These are unsigned development packages. Code signing is not configured.
The package collects Node.js and JavaScript license texts, native vcpkg notices,
and available Rust license texts plus a dependency inventory, and includes the
repository's root `LICENSE*` files plus `packaging/GPL-SOURCE-OFFER.md`.

The x264-enabled FFmpeg configuration is GPL-2.0-or-later licensed, which makes
this binary package as a whole GPL-licensed in addition to Celesta's own
MIT/Apache-2.0 source license; see [`../GPL-SOURCE-OFFER.md`](../GPL-SOURCE-OFFER.md)
and the root [`README.md`](../../README.md#license) for details. That file's
written offer, together with `LICENSE-GPL-2.0`, is how this package satisfies
GPLv2 Section 3 without bundling full FFmpeg/x264 source in every download.
