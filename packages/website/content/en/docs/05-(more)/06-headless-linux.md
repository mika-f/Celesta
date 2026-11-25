This page sets up a Linux machine with no display and no GPU as a rendering backend that another application drives through the `celesta` command. Every command below was run on a clean Ubuntu 22.04 (glibc 2.35, x86-64) container built from the main branch; the outputs shown are from that run.

The calling application only needs one executable, `celesta`: `celesta init` creates a project, `celesta doctor` checks the machine and a planned render, `celesta plugin` installs codecs, and `celesta export` and `celesta info` render and inspect. With `--json`, `doctor`, `plugin`, `export`, and `info` print one JSON result for the caller.

## 1. Install the system packages

Celesta renders with Vulkan. Without a GPU, Mesa's software Vulkan driver (lavapipe) renders on the CPU. Text needs at least one installed font.

```sh title="System packages (Ubuntu 22.04)"
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential ca-certificates curl git pkg-config xz-utils libclang-dev \
  libvulkan1 mesa-vulkan-drivers fontconfig fonts-dejavu-core
```

Add `fonts-noto-cjk` for Japanese text.

## 2. Build `celesta`

There is no Linux package of the `celesta` command yet, so build it from source. Use Rust 1.99.1, Node.js 26, and the pnpm version in the root `package.json` (12.17.0), as CI does:

```sh title="Build celesta"
git clone https://github.com/mika-f/Celesta.git ~/celesta
cd ~/celesta
pnpm install --frozen-lockfile
pnpm run build:runtime
cargo build --release --locked -p celesta-cli
mkdir -p ~/.local/bin
ln -s "$PWD/target/release/celesta" ~/.local/bin/celesta
export PATH="$HOME/.local/bin:$PATH"
```

A source-built `celesta` loads the React runtime from this checkout (`packages/cli/dist/cli.js`) and runs `node` from `PATH`, so keep the checkout where it is and link the executable rather than copying it elsewhere. If the checkout moves, `celesta doctor` reports `runtime_unavailable` for `checks.node`.

On a machine with no Wayland session, each `celesta` run that opens the graphics adapter prints `error: XDG_RUNTIME_DIR not set in the environment.` on stderr. Rendering is not affected. Set `XDG_RUNTIME_DIR` (for example to `/tmp`) in the worker's environment to silence it.

## 3. Create a project

```sh title="Create a project"
mkdir -p ~/qs && cd ~/qs
celesta templates
celesta init film --template blank
cd film
```

```text
Initialized Celesta project in /home/render/qs/film
Edit film.tsx.
Run pnpm install for TypeScript or external dependencies.
```

The built-in templates need no `pnpm install`; run it only when a composition imports external packages. The generated `package.json` scripts call `celesta-editor` and `celesta-exporter`, which a headless worker does not need: use `celesta export` instead.

## 4. Check the machine

```sh title="Check Node.js, the React runtime, and graphics"
celesta doctor --json
```

```json
{"schemaVersion":1,"status":"ok","checks":{"node":{"status":"ok","message":"Node.js v26.13.0; /home/render/celesta/crates/react-bridge/../../packages/cli/dist/cli.js"},"graphics":{"status":"ok","message":"llvmpipe (LLVM 15.0.7, 256 bits) (Vulkan)"},"output":{"status":"skipped","message":"no render requested"}}}
```

`llvmpipe` is lavapipe, the software renderer. See [On Linux without a GPU](/en/docs/export/#on-linux-without-a-gpu) when `checks.graphics` fails.

## 5. Install the codec plugins

A still image needs no plugin. MOV (ProRes 4444 with alpha) and WebM (VP9) need the standard `ffmpeg` media plugin, and MP4 (H.264) also needs the optional `ffmpeg-x264` plugin. Install them for every project on the machine:

```sh title="Install the standard media plugin"
celesta plugin install --global ffmpeg --json
```

```json
{"command":"install","plugins":[{"id":"ffmpeg","scope":"global","status":"installed","version":"0.10.0"}],"schemaVersion":1,"scope":"global","status":"ok"}
```

```sh title="Install H.264 for MP4"
celesta plugin install --global ffmpeg-x264 --json
```

> [!NOTE]
> **MP4 on Ubuntu 22.04 with Celesta 0.10.0**
> The registry's `ffmpeg-x264` 0.10.0 for `linux-x86_64` needs glibc 2.38 (Ubuntu 24.04 or later). On Ubuntu 22.04 it installs and `celesta plugin audit` reports no problems, but it cannot start, and an MP4 export fails with `component_unavailable` and ``GLIBC_2.38' not found``. Releases after 0.10.0 build it for glibc 2.35. Until then, build both FFmpeg plugins from the checkout as below.

This builds FFmpeg twice in the checkout, stages both plugins in `~/celesta-plugins`, and returns to the project:

```sh title="Build the FFmpeg plugins from source (Ubuntu 22.04)"
cd ~/celesta
sudo apt-get install -y --no-install-recommends nasm zlib1g-dev libx264-dev libvpx-dev libopus-dev
FFMPEG_VARIANT=standard scripts/build-ffmpeg-linux.sh ~/ffmpeg8-standard
FFMPEG_VARIANT=x264 scripts/build-ffmpeg-linux.sh ~/ffmpeg8-x264
PKG_CONFIG_PATH=~/ffmpeg8-standard/lib/pkgconfig FFMPEG_X264_PKG_CONFIG_PATH=~/ffmpeg8-x264/lib/pkgconfig \
  node scripts/plugins/build.mts plugins/ffmpeg plugins/ffmpeg-x264 --out ~/celesta-plugins
export CELESTA_PLUGIN_PATH=~/celesta-plugins
cd ~/qs/film
```

Celesta searches `CELESTA_PLUGIN_PATH` before the registry plugins, so these are used even with the registry's `ffmpeg-x264` installed. `--plugin-dir ~/celesta-plugins` on a single `celesta export` or `celesta doctor` works too.

`celesta plugin audit` checks the installed plugins against the registry's security advisories and withdrawn versions; it does not start them. To know whether a plugin can actually run here, test the render you plan with `celesta doctor` (next step).

## 6. Test the planned render

Pass `celesta doctor` the entry and the output you plan to write. It renders through the same media components as the export, discards the result, and exits nonzero when anything on the way fails:

```sh title="Test each output format"
celesta doctor --react film.tsx --frame 0 frame.png --json
celesta doctor --react film.tsx film.mov --json
celesta doctor --react film.tsx film.mp4 --json
```

On the run above, with the registry `ffmpeg-x264` 0.10.0 on Ubuntu 22.04, the MP4 check failed before any job was taken:

```json
{"schemaVersion":1,"status":"error","checks":{"node":{"status":"ok","message":"…"},"graphics":{"status":"ok","message":"llvmpipe (LLVM 15.0.7, 256 bits) (Vulkan)"},"output":{"status":"error","code":"component_unavailable","message":"ffmpeg-x264/libx264 is unavailable: the media plugin is unavailable: …/celesta-codec-ffmpeg-x264 is unavailable: its process exited; it last printed:\n…: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found (required by …/libx264.so.164)"}}}
```

With the plugins built from source on `CELESTA_PLUGIN_PATH`, the same check reports `"output":{"status":"ok","message":"rendered through the selected media path; output discarded"}`.

## 7. Export

```sh title="Export PNG, MOV, and MP4"
celesta export --react film.tsx --frame 0 frame.png
celesta export --react film.tsx film.mov
celesta export --react film.tsx film.mp4
```

```text
format: Image · PNG
graphics: vulkan (software) · llvmpipe (LLVM 15.0.7, 256 bits)
rendering frame 1/1  — fps  elapsed 00:00:00  eta —
warning: font family "sans-serif" (weight 400) is not installed or loaded; text layer "root.0.1" uses a fallback font
export complete: frame.png
```

The blank template's text uses the generic family `sans-serif`, which the exporter reports as `font family "sans-serif" (weight 400) is not installed or loaded` even when fonts are installed; the text is drawn with an installed fallback font (DejaVu Sans here). Name an installed family, or load a font file with `<Font>`, to choose the face yourself.

On 8 CPU cores with lavapipe, the 5-second 1080p blank template rendered at about 18 frames per second to MOV and 30 to MP4. Add `--overwrite` to replace an existing output, and `--from` and `--to` to export part of the composition.

## 8. Call Celesta from another application

Give the calling application these rules:

- Add `--json` to `celesta export`, `celesta info`, `celesta doctor`, and `celesta plugin list|install|audit`. Each prints exactly one JSON object on stdout. Diagnostics, such as a plugin process's own error output, go to stderr, so read only stdout as JSON.
- Check the exit status first: 0 is success, 1 a failed export or check, 2 rejected arguments. Then read `status` (`"ok"` or `"error"`) and, on failure, `error.code` and, when present, `error.hint`. `celesta doctor` reports per check instead: each of `checks.node`, `checks.graphics`, and `checks.output` has a `status`, a `message`, and, on failure, a `code` such as `component_unavailable`.
- For long renders, add `--json-progress` after `--json` to get one JSON line per progress event before the final result.

A successful MP4 export returns the written files, the components that wrote them, and the graphics adapter:

```sh title="Export for a caller"
celesta export --react film.tsx film.mp4 --overwrite --json
```

```json
{"status":"ok","source":"/home/render/qs/film/film.tsx","composition":{"width":1920,"height":1080,"fps":30.0,"frames":150,"duration":5.0},"graphics":{"backend":"vulkan","adapter":"llvmpipe (LLVM 15.0.7, 256 bits)","software":true},"compositionId":"default","compositions":[{"id":"default","width":1920,"height":1080,"frameRate":{"numerator":30,"denominator":1},"durationInFrames":150}],"outputs":[{"type":"video","path":"/home/render/qs/film/film.mp4","firstFrame":0,"frames":150,"start":0.0,"duration":5.0,"muxer":"ffmpeg/mp4","video":{"component":"ffmpeg-x264/libx264","codec":"avc1.640028"},"audio":null,"alpha":false,"files":["/home/render/qs/film/film.mp4"]}],"warnings":["font family \"sans-serif\" (weight 400) is not installed or loaded; text layer \"root.0.1\" uses a fallback font"],"quality":[],"stats":{"elapsedSeconds":5.61,"renderFps":29.8}}
```

Failures keep the same shape, with an error code the caller can branch on and a hint for the operator:

```json
{"status":"error","error":{"code":"output_exists","message":"output already exists: frame.png","hint":"pass --overwrite to replace it, or write to another path"},…}
{"status":"error","error":{"code":"unsupported_output","message":"no muxer writes .png files","hint":"PNG, JPEG and WebP outputs are still images: select frames with --frame, as in --frame 0 (or --frame 0,30,60, or --every 30)"},…}
{"status":"error","error":{"code":"project_json","message":"could not load export project: invalid project JSON: expected value at line 1 column 1","hint":"this looks like a React entry: add --react to read it as one"},…}
```

`celesta info --react film.tsx --json` returns the size, frame rate, and compositions without rendering, and `--props` or `--props-file` pass the job's data to the composition. The [export guide](/en/docs/export/#from-the-command-line) lists every field and error code, and [Registry plugins](/en/docs/plugins/#registry-plugins) the plugin commands' JSON.

## Without building: the Docker image

`packaging/linux/Dockerfile` builds an image with `celesta`, the standard `ffmpeg` plugin, lavapipe, and fonts, so it exports PNG, MOV, and WebM with no other setup. It has no H.264 plugin, so MP4 needs `ffmpeg-x264` mounted and passed with `--plugin-dir`. Its default entry point is `celesta-exporter`; run `celesta` subcommands with `--entrypoint /opt/celesta/celesta`:

```sh title="Export a MOV with the image"
docker build -f packaging/linux/Dockerfile -t celesta-exporter ~/celesta
docker run --rm -u "$(id -u):$(id -g)" -v "$HOME/qs/film:/work" -w /work \
  --entrypoint /opt/celesta/celesta celesta-exporter \
  export --react film.tsx film.mov --json
```

The image is based on Ubuntu 26.04, so its plugins need nothing from the host's glibc. The [Linux container README](https://github.com/mika-f/Celesta/blob/main/packaging/linux/README.md#unified-cli) has more examples.
