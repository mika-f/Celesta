#!/usr/bin/env bash
# Builds the FFmpeg 8.1.x libraries Celesta links against from source and
# installs them into a prefix, for Linux distributions that do not package
# FFmpeg 8.1.x (Ubuntu 24.04 LTS ships 6.1).
#
#   scripts/build-ffmpeg-linux.sh [PREFIX]    # PREFIX defaults to /opt/ffmpeg8
#
# Needs a C toolchain, nasm, pkg-config, curl, xz, and the zlib and x264
# development files; on Debian and Ubuntu:
#
#   sudo apt-get install -y build-essential nasm pkg-config curl xz-utils zlib1g-dev libx264-dev
#
# Run it as a user that can write to PREFIX (for /opt/ffmpeg8, use sudo), then
# build Celesta with PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig".
#
# FFMPEG_VERSION selects the 8.1.x release, and FFMPEG_URL overrides where the
# source tarball is downloaded from.
set -euo pipefail

version="${FFMPEG_VERSION:-8.1.3}"
# Absolute, because configure and make install run from the temporary source tree.
prefix="$(realpath -m "${1:-/opt/ffmpeg8}")"
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
# libx264 is GPL, which makes the build GPL as a whole.
./configure \
  --prefix="$prefix" \
  --enable-gpl \
  --enable-libx264 \
  --enable-zlib \
  --enable-pic \
  --disable-autodetect \
  --disable-programs \
  --disable-doc
make -j"$(nproc)"
make install
