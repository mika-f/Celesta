# GPL corresponding-source offer

Celesta's own source code is licensed under MIT OR Apache-2.0 (see
`LICENSE-MIT` and `LICENSE-APACHE`, at the repository's root and beside this
file in a package) and is always available at <https://github.com/mika-f/celesta>.

Celesta's own executables link no FFmpeg. The standard app packages and Linux
image include only the LGPL `ffmpeg` plugin. The separately downloadable
`ffmpeg-x264` archive contains `celesta-codec-ffmpeg-x264`, a separate program
in its own plugin folder (`ffmpeg-x264`), that
links an FFmpeg build compiled with the `x264` encoder enabled. `x264` is
licensed under the GNU General Public License version 2 or later
(GPL-2.0-or-later), and FFmpeg itself is dual-licensed LGPL/GPL, becoming
GPL-licensed as a whole once a GPL-only component such as `x264` is enabled.
Under GPLv2 Section 3, the codec plugin's program, which links this build,
therefore may only be distributed as object/executable code together with
either the complete corresponding source or a written offer for it. This file
is that offer; a copy of it ships in the codec plugin's `licenses` folder.

Separate LGPL plugin processes, `ffmpeg-vpx` and `ffmpeg-opus`, provide WebM
encoding. They do not link the GPL FFmpeg build.

## The offer

For any Celesta `ffmpeg-x264` plugin release you received in object/executable
form on Windows, macOS or Linux, we will provide, for a charge no more than our cost of physically
performing source distribution, a complete machine-readable copy of the
corresponding source code for the GPL-licensed components in that specific
build (FFmpeg and `x264`, built with the same version and configuration used
for that release, and the codec plugin's own source, which is in this
repository under `plugins/ffmpeg-x264`), and the shared FFmpeg implementation under `plugins/ffmpeg`, on a medium customarily used for software
interchange.

This offer is valid for at least three years from the date you received the
release, for anyone who possesses that release in object/executable form.

To request source under this offer, open an issue at
<https://github.com/mika-f/celesta/issues> naming the release version (from
`Celesta.exe`/`Celesta.app`'s About dialog or the package file name) and the
platform (Windows, macOS or Linux).

## Locating the exact build

Corresponding source includes the upstream releases and any build-system
patches (including those in the pinned vcpkg port), together with the plugin
and shared implementation. Upstream locations:

- FFmpeg: <https://ffmpeg.org/download.html> (or <https://github.com/FFmpeg/FFmpeg>)
- x264: <https://code.videolan.org/videolan/x264>
- libvpx (BSD-3-Clause; vcpkg port `libvpx`): <https://chromium.googlesource.com/webm/libvpx>
- libopus (BSD-3-Clause; vcpkg port `opus`): <https://opus-codec.org/>

The exact versions and build configuration used for a given release are
recorded by the build scripts: `FFMPEG_VARIANT=x264` in
`scripts/build-ffmpeg-linux.sh` (Unix), and the vcpkg `ffmpeg[x264]` build
in `.github/actions/setup-ffmpeg/action.yml` (Windows). Windows packages
include its native notices in the plugin's `licenses/native` folder.

## Full license text

The complete GPL-2.0 text is stored at
`plugins/ffmpeg-x264/LICENSE-GPL-2.0` in the repository. In release packages,
it is copied alongside this offer into the x264 plugin's `licenses` folder.
These GPL terms concern the linked x264 plugin build; Celesta's own source
remains MIT OR Apache-2.0.
