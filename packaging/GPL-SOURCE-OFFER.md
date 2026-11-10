# GPL corresponding-source offer

Celesta's own source code is licensed under MIT OR Apache-2.0 (see
`LICENSE-MIT` and `LICENSE-APACHE`, at the repository's root and beside this
file in a package) and is always available at <https://github.com/mika-f/celesta>.

Celesta's own executables link no FFmpeg. The official Windows and macOS
binary packages, and the Linux container image, additionally bundle Celesta's
codec plugin, `celesta-codec-ffmpeg`: a separate program, in its own plugin
folder (`plugins/ffmpeg`, or `Contents/Resources/PlugIns/ffmpeg` in the macOS
app), that links an FFmpeg build compiled with the `x264` encoder enabled. `x264` is
licensed under the GNU General Public License version 2 or later
(GPL-2.0-or-later), and FFmpeg itself is dual-licensed LGPL/GPL, becoming
GPL-licensed as a whole once a GPL-only component such as `x264` is enabled.
Under GPLv2 Section 3, the codec plugin's program, which links this build,
therefore may only be distributed as object/executable code together with
either the complete corresponding source or a written offer for it. This file
is that offer; a copy of it ships in the codec plugin's `licenses` folder.

The build also links `libvpx` and `libopus` (BSD-3-Clause), the encoders behind
WebM export.

## The offer

For any Celesta Windows or macOS release you received in object/executable
form, we will provide, for a charge no more than our cost of physically
performing source distribution, a complete machine-readable copy of the
corresponding source code for the GPL-licensed components in that specific
build (FFmpeg and `x264`, built with the same version and configuration used
for that release, and the codec plugin's own source, which is in this
repository under `plugins/ffmpeg`), and for the BSD-licensed `libvpx` and
`libopus` linked into it, on a medium customarily used for software
interchange.

This offer is valid for at least three years from the date you received the
release, for anyone who possesses that release in object/executable form.

To request source under this offer, open an issue at
<https://github.com/mika-f/celesta/issues> naming the release version (from
`Celesta.exe`/`Celesta.app`'s About dialog or the package file name) and the
platform (Windows or macOS).

## Locating the exact build

None of FFmpeg, `x264`, `libvpx`, or `libopus` is modified by this project;
each release bundles an unmodified upstream build, so the corresponding source
is the matching upstream release. `libvpx` and `libopus` are BSD-3-Clause rather
than GPL licensed, but their sources are offered on the same terms:

- FFmpeg: <https://ffmpeg.org/download.html> (or <https://github.com/FFmpeg/FFmpeg>)
- x264: <https://code.videolan.org/videolan/x264>
- libvpx (BSD-3-Clause; vcpkg port `libvpx`): <https://chromium.googlesource.com/webm/libvpx>
- libopus (BSD-3-Clause; vcpkg port `opus`): <https://opus-codec.org/>

The exact versions and build configuration used for a given release are
recorded inside that release's own package, under `licenses/native` (Windows,
from the vcpkg `ffmpeg[x264,vpx,opus]:x64-windows-static-md` port) or
`licenses/native` and `licenses/homebrew.json` (macOS, from the Homebrew
`ffmpeg@8` formula and its dependency closure).

## Full license text

The complete GPL-2.0 text is included as `LICENSE-GPL-2.0` at the repository
root, and is copied into every release package alongside this offer, and into
the codec plugin's `licenses` folder.
