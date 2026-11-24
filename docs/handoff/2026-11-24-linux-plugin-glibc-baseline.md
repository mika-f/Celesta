# Linux plugin glibc baseline

The registry's `linux-x86_64` `ffmpeg-x264` archive installed on Ubuntu 22.04
(glibc 2.35) but could not start: the `libx264.so.164` it bundled from the
`ubuntu-24.04` runner needs `GLIBC_2.38`, so `--check-license` and
`--check-build` both failed in the dynamic loader.

The Linux plugin jobs now build in an `ubuntu:22.04` container on the
`ubuntu-24.04` runner: `plugin-release.yml`'s `linux-x86_64` matrix entry and
`package-build.yml`'s `linux-plugins` job. The glibc the archives require, the
bundled `libx264.so.N` (now `libx264.so.163`) and the `distribution` recorded in
`debian-packages.json` (now `ubuntu 22.04`) come from that image. A container,
rather than the `ubuntu-22.04` runner label, keeps the baseline when GitHub
retires that runner image. Each container job first installs what the runner
image had and the base image lacks (git, sudo, zstd, a compiler toolchain);
`setup-ffmpeg` names the container's distribution in its cache key, since
container jobs see no `ImageOS`.

`packaging/plugins/README.md` and the plugins guide (en/ja) state the minimum:
glibc 2.35 on x86-64, that is Ubuntu 22.04 LTS, Debian 12 or later.

## Checks

The workflow steps were reproduced in a `ubuntu:22.04` container
(`setup-ffmpeg`'s apt packages, `scripts/build-ffmpeg-linux.sh` for `standard`
and `x264`, then `scripts/package-media-plugins.py --registry`). The resulting
`ffmpeg`, `ffmpeg-x264` and `vrm` archives were extracted into clean
`ubuntu:22.04` and `debian:12` containers with no codec packages installed:
every binary and bundled library needs at most `GLIBC_2.35`, and both
`--check-license` and `--check-build` exit 0 for both FFmpeg plugins.
The published 0.10.0 `ffmpeg-x264` archive fails the same checks on
`ubuntu:22.04` with `GLIBC_2.38' not found (required by .../libx264.so.164)`.
