#!/usr/bin/env bash
# Builds FFmpeg 8.1.x on Linux or macOS, with separate LGPL/GPL prefixes.
# FFMPEG_VARIANT=standard (default) or x264 selects the build. standard is
# LGPL with zlib (zlib licence), libopus, and libvpx (both BSD licensed); x264 is GPL with
# libx264 and serves only the optional ffmpeg-x264 plugin. Each variant
# needs its own prefix.
# macOS builds dylibs; Linux builds static archives.
#
#   scripts/build-ffmpeg-linux.sh [PREFIX]    # PREFIX defaults to /opt/ffmpeg8-<variant>
#
# Needs a C toolchain, nasm, pkg-config, curl, xz, and the zlib, x264, libvpx,
# and libopus development files; on Debian and Ubuntu:
#
#   sudo apt-get install -y build-essential nasm pkg-config curl xz-utils zlib1g-dev libx264-dev libvpx-dev libopus-dev
#
# Run it as a user that can write to PREFIX (for /opt/ffmpeg8-<variant>, use sudo), then
# build Celesta with PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig" for the standard
# prefix and FFMPEG_X264_PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig" for the x264 one.
#
# FFMPEG_VERSION selects the 8.1.x release, and FFMPEG_URL overrides where the
# source tarball is downloaded from. The tarball must match its SHA-256 before
# it is unpacked: the one listed below for its version, or FFMPEG_SHA256 for a
# version not listed. A version bump adds its line, with a hash taken from a
# tarball whose signature the FFmpeg release signing key verifies
# (FCF9 86EA 15E6 E293 A564 4F10 B432 2F04 D676 58D8, ffmpeg.org/ffmpeg-devel.asc).
set -euo pipefail

version="${FFMPEG_VERSION:-8.1.3}"
case "$version" in
  8.1.3) known_sha256=7138d28c96d9d3e3af4ee3d8cad72741f8ffb40da90c1112235dea3ecd3178a3 ;;
  *) known_sha256= ;;
esac
sha256="${FFMPEG_SHA256:-$known_sha256}"
if [[ -z "$sha256" ]]; then
  echo "FFmpeg $version has no known SHA-256: add it to this script or set FFMPEG_SHA256" >&2
  exit 2
fi
variant="${FFMPEG_VARIANT:-standard}"
case "$variant" in
  standard) codec_flags=(--disable-gpl --disable-nonfree --enable-zlib --enable-libopus --enable-libvpx) ;;
  x264) codec_flags=(--enable-gpl --disable-nonfree --enable-libx264) ;;
  *) echo "FFMPEG_VARIANT must be standard or x264" >&2; exit 2 ;;
esac
mkdir -p "${1:-/opt/ffmpeg8-$variant}"
# Absolute, because configure and make install run from the temporary source tree.
prefix="$(cd "${1:-/opt/ffmpeg8-$variant}" && pwd)"
if [[ "$(uname -s)" == Darwin ]]; then
  link_flags=(--enable-shared --disable-static)
  jobs="$(sysctl -n hw.ncpu)"
else
  link_flags=(--enable-static --disable-shared)
  jobs="$(nproc)"
fi
url="${FFMPEG_URL:-https://ffmpeg.org/releases/ffmpeg-$version.tar.xz}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

curl -fsSL --retry 3 "$url" -o "$work/ffmpeg.tar"
if command -v sha256sum >/dev/null; then
  actual="$(sha256sum "$work/ffmpeg.tar" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "$work/ffmpeg.tar" | awk '{print $1}')"
fi
if [[ "$actual" != "$sha256" ]]; then
  echo "FFmpeg $version from $url has SHA-256 $actual, not $sha256" >&2
  exit 1
fi
mkdir "$work/src"
tar -xf "$work/ffmpeg.tar" -C "$work/src" --strip-components=1
cd "$work/src"

# On Linux Celesta links these libraries statically (ez-ffmpeg's `static`
# feature) and the pkg-config files carry the extra libraries to link; on macOS
# it links the dylibs built here, which packaging bundles. --disable-autodetect
# keeps the build from picking up whatever optional libraries (X11, ALSA,
# VA-API, ...) happen to be installed; the standard variant enables zlib
# explicitly because the PNG decoder needs it.
# GPL and LGPL libraries must never be installed into the same prefix.
./configure \
  --prefix="$prefix" \
  "${codec_flags[@]}" \
  "${link_flags[@]}" \
  --enable-pic \
  --disable-autodetect \
  --disable-programs \
  --disable-doc
make -j"${FFMPEG_JOBS:-$jobs}"
make install
