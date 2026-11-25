# Relocatable Linux CLI

## Scope

A Linux source build must be able to use the runtime in a moved checkout, and
a Linux x86_64 release archive must work without a checkout or system Node.js.
The compatibility baseline is Ubuntu 22.04 (glibc 2.35).

## Layout and packaging

- Runtime discovery prefers `runtime/` beside the executable, as in the
  existing Windows and Linux container builds. For a Linux Cargo build, it
  checks the original build tree first (including builds with a shared Cargo
  target directory). If both the original runtime and workspace manifest
  are gone, it looks for the source tree relative to `target/`, even if
  the move left empty directories at the original location.
- The archive contains `bin/{celesta,celesta-exporter,runtime/}` and the
  standard LGPL FFmpeg plugin under `lib/celesta/plugins/ffmpeg/`, the
  normal Linux executable-relative plugin location. It ships the plugin's
  libopus and libvpx with relative ELF lookup paths and source notices.
  The optional GPL x264 plugin remains a separate release asset.
- The shared package workflow stages the archive in its Ubuntu 22.04
  container. On a manual `ci.yml` run it uploads `celesta-linux-x64` as
  a workflow artifact only; the release workflow attaches the archive to the
  GitHub release through its existing artifact collection.

## Checks

The smoke test extracts and moves the archive in a fresh Ubuntu 22.04
container with only the documented runtime dependencies, then runs
`celesta doctor --json` and exports
the opening frames of `packages/cli/examples/title.tsx` to MOV with lavapipe.
The React bridge unit test exercises the moved Cargo workspace and verifies
that a staged runtime still takes precedence.
