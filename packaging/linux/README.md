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
[Export from the command line](../../README.md#export-from-the-command-line):

```sh
docker run --rm -v "$PWD:/work" celesta-exporter --react film.tsx film.mp4
```

Check a single frame first; it is much faster than a full export:

```sh
docker run --rm -v "$PWD:/work" celesta-exporter --react film.tsx --frame 0 frame.png
```

The image includes the DejaVu and Noto CJK fonts. To use other fonts, keep the
font files in the mounted folder and load them with `<Font>`.

Without a terminal attached, the exporter prints text progress. Add `-t` to
`docker run` for the interactive dashboard. Files are written as root unless
you add `--user "$(id -u):$(id -g)"`.

Software rendering is much slower than a GPU; see
[Export without a GPU](../../README.md#export-without-a-gpu-on-linux).

## License

The image contains an FFmpeg build with `libx264` enabled, which is
GPL-2.0-or-later licensed. Distributing the image (for example, pushing it to a
public registry) makes it subject to the GPL as a whole; see
[License](../../README.md#license).
