# Export in a Linux container

[`Dockerfile`](Dockerfile) builds an Ubuntu 26.04 image with the unified
`celesta` CLI, the standalone exporter and MCP server, Node.js, and the React runtime. It
renders with Mesa's software Vulkan driver (lavapipe), so it runs on machines
without a GPU, such as CI runners and cloud containers.

## Build the image

From the repository root:

```sh
docker build -f packaging/linux/Dockerfile -t celesta-exporter .
```

The build compiles separate standard and x264 variants of FFmpeg 8.1.x with
[`scripts/build-ffmpeg-linux.sh`](../../scripts/build-ffmpeg-linux.sh), plus
the React runtime and CLI tools. It takes a while the first time. To use
another 8.1.x release or download FFmpeg's source from a mirror, pass
`--build-arg FFMPEG_VERSION=8.1.x` or `--build-arg FFMPEG_URL=…`. The tarball
must match the SHA-256 the script lists for its version; for a release it does
not list, also pass `--build-arg FFMPEG_SHA256=…`.

## Export

Mount the folder that contains your composition and its media at `/work`.
Use the unified `export` subcommand; no entry point override is needed.
MP4 (H.264), ProRes/MOV, and WebM work without mounting extra codecs.
`--user` makes the output files belong to you instead of root:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter export --react film.tsx film.mp4
```

Check a single frame first; it is much faster than a full export:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter export --react film.tsx --frame 0 frame.png
```

The image includes the DejaVu and Noto CJK fonts. To use other fonts, keep the
font files in the mounted folder and load them with `<Font>`.

Without a terminal attached, the exporter prints text progress. Add `-t` to
`docker run` for the interactive dashboard.

## Unified CLI

The image runs the unified CLI for initialization, diagnostics, metadata, and
plugin management:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter init my-video
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter doctor --react film.tsx film.mp4 --json
docker run --rm -v "$PWD:/work" celesta-exporter info --react film.tsx --json
docker run --rm celesta-exporter plugin --help
```

Existing invocations without a subcommand still run the standalone exporter,
for example `docker run --rm -v "$PWD:/work" celesta-exporter --react film.tsx film.mov`.
If an input is named like a subcommand, pass it as a path (for example, `./info`)
or use the explicit `export` subcommand.

If the container user has no writable home, Celesta uses a private temporary
one but still reads settings and presets from an existing home configuration.
To keep installed registry plugins across runs, mount a writable home and
set `HOME` to its path in the container.

## MCP server

The image also contains `celesta-mcp`, the MCP server that lets coding agents
inspect, render, and export compositions. Run it with stdin attached (`-i`) as
the agent's MCP command:

```sh
docker run --rm -i --user "$(id -u):$(id -g)" -v "$PWD:/work" --entrypoint /opt/celesta/celesta-mcp celesta-exporter
```

Paths the agent passes are resolved inside the container, so use paths under
`/work`.

Software rendering is much slower than a GPU; see
[Export without a GPU](../../README.md#export-without-a-gpu-on-linux).

## License

`celesta-exporter`, `celesta-mcp`, and `celesta` link no FFmpeg. The image bundles the
standard LGPL `ffmpeg` process under `/opt/celesta/plugins`, for decoding,
ProRes/MOV, VP9/Opus WebM, AAC/PCM and muxing. FFmpeg is linked into that
program statically; libopus and libvpx (both BSD licensed) come from the
image's `libopus0` and `libvpx12` packages. The exact binary and
source-package versions of every shared object the program loads but glibc's
own (these two, zlib, and the GCC runtime) are recorded in
`/opt/celesta/plugins/ffmpeg/licenses/native/debian-packages.json` beside
their copyright files; each entry's `copyright_from` names the package the
file comes from, a same-source sibling when the package has no doc folder of
its own (`libgcc-s1` uses `gcc-N-base`'s). The standard plugin carries the
[LGPL source notice](../FFMPEG-SOURCE.md).

The separate GPL `ffmpeg-x264` process under `/opt/celesta/plugins/ffmpeg-x264`
provides H.264 encoding. It and its bundled libx264 come from the Ubuntu 22.04
build stage, using the same packager as the standalone ZIP. Its `licenses/`
includes the GPL text, [corresponding-source offer](../GPL-SOURCE-OFFER.md),
and native package versions and copyright notices. See
[plugin packaging](../plugins/README.md) and the root [license](../../README.md#license).
