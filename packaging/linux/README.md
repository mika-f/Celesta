# Export in a Linux container

[`Dockerfile`](Dockerfile) builds an Ubuntu 24.04 image that contains only the
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
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" celesta-exporter --react film.tsx film.mp4
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

`celesta-exporter` and `celesta-mcp` link no FFmpeg. The image's codec plugin,
the `ffmpeg` folder in the bundled plugin folder `/opt/celesta/plugins` (which
the build sets with `CELESTA_SYSTEM_PLUGIN_DIR`), links an FFmpeg build with `libx264` enabled,
which is GPL-2.0-or-later licensed, and the image installs `libx264` itself.
Distributing the image (for example, pushing it to a public registry)
therefore distributes GPL-licensed software, whose terms apply to those parts;
see [License](../../README.md#license).

The same FFmpeg build links `libvpx` and `libopus` (BSD-3-Clause), the WebM
encoders, as shared libraries from the Debian packages `libvpx9` and
`libopus0`. Their copyright files ship in the image under
`/usr/share/doc/libvpx9` and `/usr/share/doc/libopus0`.
