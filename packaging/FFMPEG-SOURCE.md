# FFmpeg LGPL build

The distributed `ffmpeg` plugin uses FFmpeg under LGPL-2.1-or-later, with GPL
and nonfree components disabled in distributed builds. It links zlib, libopus,
and libvpx, all under permissive licenses, so the build stays LGPL. Local
source builds may use a system FFmpeg 8.1.x of either license;
`--check-license` validates releases. FFmpeg's source is at
<https://ffmpeg.org/download.html> and <https://github.com/FFmpeg/FFmpeg>.
Celesta's source and build scripts are at <https://github.com/mika-f/celesta>
(`plugins/ffmpeg`, `scripts/build-ffmpeg-linux.sh`,
`.github/actions/setup-ffmpeg/action.yml`, and
`.github/workflows/ffmpeg-windows.yml`).

The separate optional `ffmpeg-x264` program provides libx264. Its GPL terms and
corresponding-source offer are in that plugin's own `licenses/` folder.

For a distributed Celesta release, request the complete corresponding FFmpeg
source, build configuration, and materials needed to relink the LGPL plugin
with a modified FFmpeg by opening an issue at
<https://github.com/mika-f/celesta/issues>, naming the release and platform.
This source/materials offer is valid for at least three years after receipt.
Modification and reverse engineering for debugging modifications to the LGPL
libraries are permitted. Keep the LGPL notice and these materials with a
redistributed plugin. The full license is stored at
`plugins/ffmpeg/LICENSE-LGPL-2.1` in the repository and copied as
`LICENSE-LGPL-2.1` into the plugin's `licenses` folder. It describes the
linked FFmpeg libraries; Celesta's own source remains MIT OR Apache-2.0.

## Codec libraries

The standard build links:

- libopus (BSD-3-Clause; <https://opus-codec.org/>)
- libvpx (BSD-3-Clause; <https://chromium.googlesource.com/webm/libvpx>)
- zlib (Zlib license; <https://zlib.net/>)

Their notices and versions come with each distribution:

- Windows: vcpkg's share tree (`share/<port>/copyright`) from the
  `Celesta.FFmpeg.Windows.Standard` package, in the app's `licenses/native`.
- macOS: `licenses/homebrew.json` and `licenses/native/<formula>` for the
  Homebrew `opus` and `libvpx` libraries bundled in the plugin's `Frameworks`.
- Linux image: the Ubuntu packages `libopus0` and `libvpx9`, whose binary and
  source-package versions are recorded in
  `/opt/celesta/plugins/ffmpeg/licenses/native/debian-packages.json`, with
  their copyright files beside it.

Source requests include the distribution's patches for the exact recorded
source-package version.
