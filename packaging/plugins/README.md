# Optional codec downloads

The application packages and Linux image include **only `ffmpeg` among the FFmpeg builds**: the standard
LGPL decoder, ProRes, AAC/PCM and muxers. x264, Opus and VPX are individual ZIP
assets on the same GitHub release, not inside the app installer or portable ZIP.

| Plugin | Use |
| --- | --- |
| `ffmpeg-x264` | H.264 via libx264 (GPL); required for default MP4 on Linux |
| `ffmpeg-opus` | Opus audio, needed for WebM |
| `ffmpeg-vpx` | VP9 video, needed for WebM |

Both Opus and VPX are needed for the normal WebM preset. The base application
can decode H.264 and export MOV/ProRes or still images without any optional plugin.

## Build the archives

Prepare separate FFmpeg native prefixes as described in the
[build guide](https://celesta.natsuneko.cat/en/docs/build-from-source/). The standard
prefix is `PKG_CONFIG_PATH`; optional prefixes are selected with
`FFMPEG_X264_PKG_CONFIG_PATH`, `FFMPEG_OPUS_PKG_CONFIG_PATH`, and
`FFMPEG_VPX_PKG_CONFIG_PATH`. Windows uses matching `*_VCPKG_ROOT` variables
(and optional `*_VCPKGRS_TRIPLET`) plus standard `VCPKG_ROOT`.

```sh
python3 scripts/package-media-plugins.py
# Build just one optional download:
python3 scripts/package-media-plugins.py --plugins opus
```

Run natively with Python 3.9+, Node.js and Rust. macOS also needs Homebrew
codec libraries and dylibbundler; Windows needs MSVC redistributables and the
vcpkg installations; Linux needs patchelf and the codec development packages.
The script refuses to overwrite existing archives; use `--out <directory>`
for a separate staging run. `--version X.Y.Z` verifies the manifest versions.

Outputs are `target/packages/Celesta-plugin-ffmpeg-<codec>-<version>-<os>-<arch>.zip`,
with one `ffmpeg-<codec>/` folder at the archive root. Its manifest, executable,
DLLs/dylibs/shared objects, source notices and license inventories stay together.
Each archive collects only its own normal/build Rust dependencies and native
codec notices, without application-only or other optional plugin dependencies.

macOS release builds pass `--sign-identity` and `--notary-profile`, signing all
Mach-O files and notarizing the ZIP. Local builds use ad hoc signatures. ZIPs
have no stapled ticket; notarization is checked online when required by macOS.
macOS and Windows also bundle their native system-codec plugin, which can
encode H.264 without this download. Explicitly name `ffmpeg-x264/libx264` to
choose the downloaded encoder.

Linux uses static FFmpeg and bundles any dynamically linked codec library with
a plugin-relative RPATH; glibc and the platform runtime remain system provided.
`licenses/native/debian-packages.json` records exact binary and source-package
versions, architecture, and distribution, including downstream revisions.
Linux release archives are built on the workflow's Ubuntu runner and require a
compatible glibc. The supported release targets are macOS arm64, Windows x64,
and Linux x64.

`package-build.yml` builds these separately from the app, validates staged
native dependencies and manifests, and uploads codec archives as separate CI
artifacts. `package.yml` attaches both app and codec assets to the release.
The optional native-FFmpeg NuGet workflow publishes development libraries,
which are not the user-installable plugin ZIPs.

## Install a downloaded archive

Download the same release as the application and the correct OS/architecture.
Extract the ZIP and copy the complete plugin folder into:

- Windows: `%APPDATA%\Celesta\plugins`
- macOS: `~/Library/Application Support/Celesta/plugins`
- Linux: `$XDG_DATA_HOME/celesta/plugins` (default `~/.local/share/celesta/plugins`)

Open Celesta, choose **Reload Plugins**, and allow the new plugin. CLI users
can allow it in the app first, use `--allow-plugin <id>`, or pass its parent
folder with `--plugin-dir`. No separate FFmpeg installation is needed.

All release packagers pass `--distribution` to the plugin builder. It opens
the selected encoders with `--check-build` and checks linked-library licenses
with `--check-license`. Distribution validation requires the target host; a
cross-compiled FFmpeg download cannot be staged without executing these checks.
Local source builds without this flag may use a system GPL FFmpeg 8.1.x.
