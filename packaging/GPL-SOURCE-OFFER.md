# GPL corresponding-source offer

Celesta's own source code is licensed under MIT OR Apache-2.0 (see
[`LICENSE-MIT`](../LICENSE-MIT) and [`LICENSE-APACHE`](../LICENSE-APACHE)) and
is always available at <https://github.com/mika-f/celesta>.

The official Windows and macOS binary packages additionally bundle an FFmpeg
build compiled with the `x264` encoder enabled. `x264` is licensed under the
GNU General Public License version 2 or later (GPL-2.0-or-later), and FFmpeg
itself is dual-licensed LGPL/GPL, becoming GPL-licensed as a whole once a
GPL-only component such as `x264` is enabled. Under GPLv2 Section 3, a work
that links this build therefore may only be distributed as object/executable
code together with either the complete corresponding source or a written
offer for it. This file is that offer.

## The offer

For any Celesta Windows or macOS release you received in object/executable
form, we will provide, for a charge no more than our cost of physically
performing source distribution, a complete machine-readable copy of the
corresponding source code for the GPL-licensed components in that specific
build (FFmpeg and `x264`, built with the same version and configuration used
for that release), on a medium customarily used for software interchange.

This offer is valid for at least three years from the date you received the
release, for anyone who possesses that release in object/executable form.

To request source under this offer, open an issue at
<https://github.com/mika-f/celesta/issues> naming the release version (from
`Celesta.exe`/`Celesta.app`'s About dialog or the package file name) and the
platform (Windows or macOS).

## Locating the exact build

Neither FFmpeg nor `x264` is modified by this project; each release bundles
an unmodified upstream build, so the corresponding source is the matching
upstream release:

- FFmpeg: <https://ffmpeg.org/download.html> (or <https://github.com/FFmpeg/FFmpeg>)
- x264: <https://code.videolan.org/videolan/x264>

The exact versions and build configuration used for a given release are
recorded inside that release's own package, under `licenses/native` (Windows,
from the vcpkg `ffmpeg[x264]:x64-windows-static-md` port) or
`licenses/native` and `licenses/homebrew.json` (macOS, from the Homebrew
`ffmpeg` formula and its dependency closure).

## Full license text

The complete GPL-2.0 text is included as [`LICENSE-GPL-2.0`](../LICENSE-GPL-2.0)
at the repository root, and is copied into every release package alongside
this offer.
