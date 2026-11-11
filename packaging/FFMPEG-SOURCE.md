# FFmpeg LGPL build

The distributed `ffmpeg`, `ffmpeg-opus`, and `ffmpeg-vpx` plugins use FFmpeg under LGPL-2.1-or-later, with GPL and nonfree
components disabled in distributed builds. Local source builds may use a
system FFmpeg 8.1.x of either license; `--check-license` validates releases. Its source is at <https://ffmpeg.org/download.html> and
<https://github.com/FFmpeg/FFmpeg>. Celesta's source and build scripts are at
<https://github.com/mika-f/celesta> (`plugins/ffmpeg`, `plugins/ffmpeg-opus`, `plugins/ffmpeg-vpx`,
`scripts/build-ffmpeg-linux.sh`, and `.github/actions/setup-ffmpeg/action.yml`).

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
`LICENSE-LGPL-2.1` into each LGPL plugin's `licenses` folder. It describes
the linked FFmpeg libraries; Celesta's own source remains MIT OR Apache-2.0.

The `ffmpeg-opus` download also links libopus (BSD-3-Clause;
<https://opus-codec.org/>), and `ffmpeg-vpx` links libvpx (BSD-3-Clause;
<https://chromium.googlesource.com/webm/libvpx>). Native notices and versions
are under `licenses/native`: vcpkg's share tree on Windows, `homebrew.json` on
macOS, and `debian-packages.json` plus distribution copyright files on Linux.
Source requests include the distribution's patches for the exact recorded
source-package version. Standalone users can identify a build by the ZIP
filename or its `celesta-plugin.json`; no app installation is needed.
