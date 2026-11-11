#!/usr/bin/env bash
# Builds FFmpeg 8.1.x on Linux or macOS, with separate LGPL/GPL prefixes.
# FFMPEG_VARIANT=standard (default) or x264 selects the build. standard is
# LGPL with zlib, libopus, and libvpx (both BSD licensed); x264 is GPL with
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
# build Celesta with PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig".
#
# FFMPEG_VERSION selects the 8.1.x release, and FFMPEG_URL overrides where the
# source tarball is downloaded from.
set -euo pipefail

version="${FFMPEG_VERSION:-8.1.3}"
# Absolute, because configure and make install run from the temporary source tree.
variant="${FFMPEG_VARIANT:-standard}"
case "$variant" in
  standard) codec_flags=(--disable-gpl --disable-nonfree --enable-zlib --enable-libopus --enable-libvpx) ;;
  x264) codec_flags=(--enable-gpl --disable-nonfree --enable-libx264) ;;
  *) echo "FFMPEG_VARIANT must be standard or x264" >&2; exit 2 ;;
esac
mkdir -p "${1:-/opt/ffmpeg8-$variant}"
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
mkdir "$work/src"
tar -xf "$work/ffmpeg.tar" -C "$work/src" --strip-components=1
cd "$work/src"

# Celesta links these libraries statically (ez-ffmpeg's `static` feature), so
# configure's default static build is what it needs; the pkg-config files carry
# the extra libraries to link. --disable-autodetect keeps the build from
# picking up whatever optional libraries (X11, ALSA, VA-API, ...) happen to be
# installed; zlib is enabled explicitly because the PNG decoder needs it.
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
