# Export in a Linux container

[`Dockerfile`](Dockerfile) builds an Ubuntu 26.04 image that contains only the
command-line exporter (`celesta-exporter`), Node.js, and the React runtime. It
renders with Mesa's software Vulkan driver (lavapipe), so it runs on machines
without a GPU, such as CI runners and cloud containers.

## Build the image

From the repository root:

```sh
docker build -f packaging/linux/Dockerfile -t celesta-exporter .
```

The build compiles FFmpeg 8.1.x with
[`scripts/build-ffmpeg-linux.sh`](../../scripts/build-ffmpeg-linux.sh), then
the React runtime and the exporter. It takes a while the first time. To use
another 8.1.x release or download FFmpeg's source from a mirror, pass
`--build-arg FFMPEG_VERSION=8.1.x` or `--build-arg FFMPEG_URL=…`.

## Export

Mount the folder that contains your composition and its media at `/work`. The
image's entry point is `celesta-exporter`, so the arguments are the same as in
[Export from the command line](../../README.md#export-from-the-command-line).
`--user` makes the output files belong to you instead of root:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter --react film.tsx film.mov
```

Check a single frame first; it is much faster than a full export:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter --react film.tsx --frame 0 frame.png
```

The image includes the DejaVu and Noto CJK fonts. To use other fonts, keep the
font files in the mounted folder and load them with `<Font>`.

Without a terminal attached, the exporter prints text progress. Add `-t` to
`docker run` for the interactive dashboard.

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

`celesta-exporter` and `celesta-mcp` link no FFmpeg. The image bundles only the
standard LGPL `ffmpeg` process under `/opt/celesta/plugins`, for decoding,
ProRes/MOV, AAC/PCM and muxing. It installs no optional x264, libopus or libvpx
runtime packages. The standard plugin carries the [LGPL source notice](../FFMPEG-SOURCE.md).

MP4 needs the separate Linux `ffmpeg-x264` ZIP; WebM needs `ffmpeg-vpx` and
`ffmpeg-opus`. Extract matching release downloads into a host folder and mount
it separately, preserving the image's standard plugin:

```sh
docker run --rm -v "$PWD:/work" -v "$PWD/codecs:/plugins:ro" celesta-exporter \
  --plugin-dir /plugins --react film.tsx film.mp4
```

The separate ZIPs include native codec libraries. See [plugin packaging](../plugins/README.md)
and the root [license](../../README.md#license).
