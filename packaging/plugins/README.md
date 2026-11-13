# The optional x264 download

The application packages and Linux image include **only the standard `ffmpeg`
plugin among the FFmpeg builds**: the LGPL decoder, ProRes, VP9, AAC, Opus,
PCM, and the MP4, MOV, and WebM muxers (libopus and libvpx are BSD licensed,
so the build stays LGPL). libx264 is GPL, so it is an individual ZIP asset on
the same GitHub release, not inside the app installer or portable ZIP.

| Plugin | Use |
| --- | --- |
| `ffmpeg-x264` | H.264 via libx264 (GPL); required for default MP4 on Linux |

The base application decodes H.264 and exports MOV/ProRes, WebM (VP9 with
Opus), and still images without the download. macOS and Windows also encode
MP4 with their native system H.264 encoder.

## Build the archive

Prepare separate FFmpeg native prefixes as described in the
[build guide](https://celesta.natsuneko.cat/en/docs/build-from-source/). The
standard prefix is `PKG_CONFIG_PATH`; the x264 prefix is selected with
`FFMPEG_X264_PKG_CONFIG_PATH`. Windows uses `FFMPEG_X264_VCPKG_ROOT` (and an
optional `FFMPEG_X264_VCPKGRS_TRIPLET`) beside the standard `VCPKG_ROOT`.

```sh
python3 scripts/package-media-plugins.py
```

Run natively with Python 3.9+, Node.js and Rust. macOS also needs Homebrew
codec libraries and dylibbundler; Windows needs MSVC redistributables and the
vcpkg installation; Linux needs patchelf and the codec development packages.
The script refuses to overwrite an existing archive; use `--out <directory>`
for a separate staging run. `--version X.Y.Z` verifies the manifest version.

The output is `target/packages/Celesta-plugin-ffmpeg-x264-<version>-<os>-<arch>.zip`,
with one `ffmpeg-x264/` folder at the archive root. Its manifest, executable,
DLLs/dylibs/shared objects, source notices and license inventories stay together.
The archive collects only its own normal/build Rust dependencies and native
codec notices, without application-only dependencies.

macOS release builds pass `--sign-identity` and `--notary-profile`, signing all
Mach-O files and notarizing the ZIP. Local builds use ad hoc signatures. ZIPs
have no stapled ticket; notarization is checked online when required by macOS.
macOS and Windows also bundle their native system-codec plugin, which can
encode H.264 without this download. Explicitly name `ffmpeg-x264/libx264` to
choose the downloaded encoder.

Linux uses static FFmpeg and bundles the dynamically linked libx264 with a
plugin-relative RPATH; glibc and the platform runtime remain system provided.
`licenses/native/debian-packages.json` records exact binary and source-package
versions and architecture, including downstream revisions, and the
distribution as os-release's `ID VERSION_ID` (such as `ubuntu 26.04`), the same
format the Linux image records for the standard plugin. Each entry's
copyright file is that package's own `/usr/share/doc/<package>/copyright`, or
a same-source sibling's named by `copyright_from`; packaging fails when there
is neither. On macOS, `licenses/homebrew.json` and `licenses/native/<formula>`
cover each Homebrew formula whose library is bundled into `Frameworks/`, traced
from the program's `otool -L` (x264 must be one), and packaging fails on a
bundled library that is neither a formula's nor FFmpeg's own.
Linux release archives are built on `ubuntu-24.04`, named rather than
`ubuntu-latest` so the glibc they require, the bundled `libx264.so.N` and the
recorded `distribution` change only with the workflow. The supported release targets are macOS arm64, Windows x64,
and Linux x64.

`package-build.yml` builds the archive separately from the app, validates
staged native dependencies and the manifest, and uploads it as a separate CI
artifact. `package.yml` attaches both app and codec assets to the release.
The Windows FFmpeg NuGet packages (`ffmpeg-windows.yml`) hold development
libraries for CI, not the user-installable plugin ZIP.

## Install a downloaded archive

Download the same release as the application and the correct OS/architecture.
Extract the ZIP and copy the complete plugin folder into:

- Windows: `%APPDATA%\Celesta\plugins`
- macOS: `~/Library/Application Support/Celesta/plugins`
- Linux: `$XDG_DATA_HOME/celesta/plugins` (default `~/.local/share/celesta/plugins`)

Open Celesta, choose **Reload Plugins**, and allow the new plugin. CLI users
can allow it in the app first, use `--allow-plugin ffmpeg-x264`, or pass its
parent folder with `--plugin-dir`. No separate FFmpeg installation is needed.

All release packagers pass `--distribution` to the plugin builder. It opens
the build's encoders with `--check-build` (VP9 and Opus included for the
standard plugin) and checks linked-library licenses with `--check-license`.
Distribution validation requires the target host; a cross-compiled FFmpeg
build cannot be staged without executing these checks. Local source builds
without this flag may use a system GPL FFmpeg 8.1.x.
