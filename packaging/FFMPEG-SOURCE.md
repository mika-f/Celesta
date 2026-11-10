# FFmpeg LGPL build

The `ffmpeg`, `ffmpeg-opus`, and `ffmpeg-vpx` plugins use FFmpeg under LGPL-2.1-or-later, with GPL and nonfree
components disabled. Its source is at <https://ffmpeg.org/download.html> and
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
