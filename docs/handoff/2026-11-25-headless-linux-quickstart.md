# Headless Linux operator quickstart

Adds the English and Japanese chapter `headless-linux` (`packages/website/content/{en,ja}/docs/05-(more)/06-headless-linux.md`): setting up a Linux machine with no display and no GPU as a rendering backend that another application drives through `celesta` alone. `packages/website/tests/content.test.ts` lists the new slug. No code changes.

Every command and output in the chapter comes from one run on a clean `ubuntu:22.04` container (glibc 2.35, 8 cores, no `/dev/dri`, no display) with `celesta` built from main (`b167ca6`) by the chapter's own steps, Node.js 26.13.0, pnpm 12.17.0, and Rust 1.99.1. The transcript is below.

## Findings from the run

- The registry's `ffmpeg-x264` 0.10.0 for `linux-x86_64` needs glibc 2.38. On 22.04, `celesta plugin install --global ffmpeg-x264` succeeds and `celesta plugin audit` returns `status: ok` with no findings, but `celesta doctor`/`celesta export` to `.mp4` fail with `component_unavailable` and ``GLIBC_2.38' not found``. The glibc 2.35 baseline is merged but not released, so the chapter shows building both FFmpeg plugins from the checkout onto `CELESTA_PLUGIN_PATH`, which wins over the registry copy. Remove that section once a release after 0.10.0 is published.
- `celesta plugin audit` only checks advisories and listing state; the chapter points operators at `celesta doctor <entry> <output>` for launchability.
- Without `XDG_RUNTIME_DIR`, each process that opens the adapter prints `error: XDG_RUNTIME_DIR not set in the environment.` (4 times for `celesta doctor`); rendering is unaffected. The transcript run sets `XDG_RUNTIME_DIR=/tmp`.
- The blank template's `fontFamily: 'sans-serif'` warns `not installed or loaded` with fontconfig and DejaVu installed; the fallback font draws it.
- `celesta init`'s `package.json` scripts call `celesta-editor`/`celesta-exporter`, not `celesta export`.
- `celesta doctor` reports `unsupported_output` and `component_unavailable` without the `hint` that `celesta export --json` gives for the same output.
- A source-built `celesta` reads the React runtime from its checkout (`crates/react-bridge/src/runtime.rs`), so the chapter links it into `~/.local/bin` instead of copying it.
- The Linux image (`packaging/linux/Dockerfile`, Ubuntu 26.04) was also built: with `--entrypoint /opt/celesta/celesta`, `doctor --json` is ok, a MOV export as UID 1000 succeeds, and `.mp4` reports `component_unavailable` since the image has no x264 plugin.

## Checks

- `pnpm --filter @celesta.studio/website test` (content tests including locale parity and internal links).
- `pnpm --filter @celesta.studio/website check`.

## Transcript

`t` prints the command, runs it with stderr merged, and prints its exit status. `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, and `XDG_CACHE_HOME` point at a new empty directory, so the registry plugin state starts empty. `~/celesta-plugins` holds the plugins built by the chapter's "Build the FFmpeg plugins from source" step. `ffprobe` and `file` ran on the host on copies of the outputs.

```text

$ uname -srm
Linux 6.8.0-1060-aws x86_64
[exit 0]

$ sh -c . /etc/os-release; echo $PRETTY_NAME
Ubuntu 22.04.5 LTS
[exit 0]

$ ldd --version
ldd (Ubuntu GLIBC 2.35-0ubuntu3.18) 2.35
Copyright (C) 2022 Free Software Foundation, Inc.
This is free software; see the source for copying conditions.  There is NO
warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
Written by Roland McGrath and Ulrich Drepper.
[exit 0]

$ command -v celesta
/home/render/.local/bin/celesta
[exit 0]

$ readlink -f /home/render/.local/bin/celesta
/home/render/celesta/target/release/celesta
[exit 0]

$ celesta --help
Create, manage, and export Celesta projects.

Usage: celesta <COMMAND>

Commands:
  init       Initialize a React project using a staged template
  templates  List the templates available to celesta init
  plugin     Add, install, and inspect registry plugins
  export     Exports a Celesta project or React composition to MP4, MOV or WebM video, an animated GIF, or PNG, JPEG or WebP stills
  info       Show the composition's size, frame rate, duration and React roots
  doctor     Check this machine or test a render without publishing its output
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
[exit 0]

$ celesta templates
blank       Blank — A five-second title card to start from scratch.
dialogue    キャラクター会話 — 立ち絵と字幕で二人が会話する解説動画。録音した音声も追加できます。
ranking     ランキング — データ JSON からランキングカードを作ります。配色やタイトルも差し替えられます。
slideshow   写真スライドショー — 写真とキャプションを並べ、トランジションでつなぐスライドショー。
[exit 0]

$ celesta init film --template blank
Initialized Celesta project in /home/render/qs/film
Edit film.tsx.
Run pnpm install for TypeScript or external dependencies.
[exit 0]

$ ls -A
.celesta
.gitignore
celesta.config.json
film.tsx
package.json
tsconfig.json
[exit 0]

$ celesta doctor --json
{"schemaVersion":1,"status":"ok","checks":{"node":{"status":"ok","message":"Node.js v26.13.0; /home/render/celesta/crates/react-bridge/../../packages/cli/dist/cli.js"},"graphics":{"status":"ok","message":"llvmpipe (LLVM 15.0.7, 256 bits) (Vulkan)"},"output":{"status":"skipped","message":"no render requested"}}}
[exit 0]

$ celesta doctor --react film.tsx --frame 0 frame.png --json
{"schemaVersion":1,"status":"ok","checks":{"node":{"status":"ok","message":"Node.js v26.13.0; /home/render/celesta/crates/react-bridge/../../packages/cli/dist/cli.js"},"graphics":{"status":"ok","message":"llvmpipe (LLVM 15.0.7, 256 bits) (Vulkan)"},"output":{"status":"ok","message":"rendered through the selected media path; output discarded"}}}
[exit 0]

$ celesta doctor --react film.tsx film.mov --json
{"schemaVersion":1,"status":"error","checks":{"node":{"status":"ok","message":"Node.js v26.13.0; /home/render/celesta/crates/react-bridge/../../packages/cli/dist/cli.js"},"graphics":{"status":"ok","message":"llvmpipe (LLVM 15.0.7, 256 bits) (Vulkan)"},"output":{"status":"error","code":"unsupported_output","message":"no muxer writes .mov files"}}}
[exit 1]

$ celesta plugin install --global ffmpeg --json
{"command":"install","plugins":[{"id":"ffmpeg","scope":"global","status":"installed","version":"0.10.0"}],"schemaVersion":1,"scope":"global","status":"ok"}
[exit 0]

$ celesta plugin install --global ffmpeg-x264 --json
{"command":"install","plugins":[{"id":"ffmpeg-x264","scope":"global","status":"installed","version":"0.10.0"}],"schemaVersion":1,"scope":"global","status":"ok"}
[exit 0]

$ celesta plugin list --json
{"advisoriesAvailable":true,"command":"list","plugins":[{"deprecated":false,"id":"ffmpeg","scope":"global","status":"installed","version":"0.10.0","yanked":false},{"deprecated":false,"id":"ffmpeg-x264","scope":"global","status":"installed","version":"0.10.0","yanked":false}],"schemaVersion":1,"status":"ok","target":"linux-x86_64"}
[exit 0]

$ celesta plugin audit --json
{"advisoriesAvailable":true,"command":"audit","plugins":[{"findings":[],"id":"ffmpeg","listingChecked":true,"scope":"global","status":"ok","version":"0.10.0"},{"findings":[],"id":"ffmpeg-x264","listingChecked":true,"scope":"global","status":"ok","version":"0.10.0"}],"schemaVersion":1,"status":"ok"}
[exit 0]

$ celesta doctor --react film.tsx film.mov --json
{"schemaVersion":1,"status":"ok","checks":{"node":{"status":"ok","message":"Node.js v26.13.0; /home/render/celesta/crates/react-bridge/../../packages/cli/dist/cli.js"},"graphics":{"status":"ok","message":"llvmpipe (LLVM 15.0.7, 256 bits) (Vulkan)"},"output":{"status":"ok","message":"rendered through the selected media path; output discarded"}}}
[exit 0]

$ celesta doctor --react film.tsx film.mp4 --json
/tmp/tmp.UMUU2mIbX4/data/celesta/registry/ffmpeg-x264/0.10.0/bin/linux-x86_64/celesta-codec-ffmpeg-x264: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found (required by /tmp/tmp.UMUU2mIbX4/data/celesta/registry/ffmpeg-x264/0.10.0/bin/linux-x86_64/../../lib/libx264.so.164)
{"schemaVersion":1,"status":"error","checks":{"node":{"status":"ok","message":"Node.js v26.13.0; /home/render/celesta/crates/react-bridge/../../packages/cli/dist/cli.js"},"graphics":{"status":"ok","message":"llvmpipe (LLVM 15.0.7, 256 bits) (Vulkan)"},"output":{"status":"error","code":"component_unavailable","message":"ffmpeg-x264/libx264 is unavailable: the media plugin is unavailable: /tmp/tmp.UMUU2mIbX4/data/celesta/registry/ffmpeg-x264/0.10.0/bin/linux-x86_64/celesta-codec-ffmpeg-x264 is unavailable: its process exited; it last printed:\n/tmp/tmp.UMUU2mIbX4/data/celesta/registry/ffmpeg-x264/0.10.0/bin/linux-x86_64/celesta-codec-ffmpeg-x264: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not found (required by /tmp/tmp.UMUU2mIbX4/data/celesta/registry/ffmpeg-x264/0.10.0/bin/linux-x86_64/../../lib/libx264.so.164)"}}}
[exit 1]

$ export CELESTA_PLUGIN_PATH=/home/render/celesta-plugins

$ celesta doctor --react film.tsx film.mp4 --json
{"schemaVersion":1,"status":"ok","checks":{"node":{"status":"ok","message":"Node.js v26.13.0; /home/render/celesta/crates/react-bridge/../../packages/cli/dist/cli.js"},"graphics":{"status":"ok","message":"llvmpipe (LLVM 15.0.7, 256 bits) (Vulkan)"},"output":{"status":"ok","message":"rendered through the selected media path; output discarded"}}}
[exit 0]

$ celesta export --react film.tsx --frame 0 frame.png
format: Image · PNG
graphics: vulkan (software) · llvmpipe (LLVM 15.0.7, 256 bits)
rendering frame 1/1  — fps  elapsed 00:00:00  eta —
warning: font family "sans-serif" (weight 400) is not installed or loaded; text layer "root.0.1" uses a fallback font
export complete: frame.png
[exit 0]

$ celesta export --react film.tsx film.mov
format: QuickTime · ProRes (prores_ks) · PCM
graphics: vulkan (software) · llvmpipe (LLVM 15.0.7, 256 bits)
rendering frame 1/150  — fps  elapsed 00:00:00  eta —
warning: font family "sans-serif" (weight 400) is not installed or loaded; text layer "root.0.1" uses a fallback font
rendering frame 19/150  17.7 fps  elapsed 00:00:01  eta 00:00:07
rendering frame 38/150  18.2 fps  elapsed 00:00:02  eta 00:00:06
rendering frame 57/150  18.2 fps  elapsed 00:00:03  eta 00:00:05
rendering frame 74/150  17.7 fps  elapsed 00:00:04  eta 00:00:04
rendering frame 93/150  17.9 fps  elapsed 00:00:05  eta 00:00:03
rendering frame 113/150  18.1 fps  elapsed 00:00:06  eta 00:00:02
rendering frame 131/150  18.1 fps  elapsed 00:00:07  eta 00:00:01
rendering frame 149/150  18.0 fps  elapsed 00:00:08  eta 00:00:00
rendering frame 150/150  18.0 fps  elapsed 00:00:08  eta 00:00:00
muxing
export complete: film.mov
[exit 0]

$ celesta export --react film.tsx film.mp4
format: MP4 · H.264 (x264) · AAC
graphics: vulkan (software) · llvmpipe (LLVM 15.0.7, 256 bits)
rendering frame 1/150  — fps  elapsed 00:00:00  eta —
warning: font family "sans-serif" (weight 400) is not installed or loaded; text layer "root.0.1" uses a fallback font
rendering frame 27/150  25.3 fps  elapsed 00:00:01  eta 00:00:04
rendering frame 58/150  27.8 fps  elapsed 00:00:02  eta 00:00:03
rendering frame 90/150  29.0 fps  elapsed 00:00:03  eta 00:00:02
rendering frame 121/150  29.4 fps  elapsed 00:00:04  eta 00:00:01
rendering frame 150/150  30.0 fps  elapsed 00:00:05  eta 00:00:00
muxing
export complete: film.mp4
[exit 0]

$ celesta export --react film.tsx film.mp4 --overwrite --json
{"status":"ok","source":"/home/render/qs/film/film.tsx","composition":{"width":1920,"height":1080,"fps":30.0,"frames":150,"duration":5.0},"graphics":{"backend":"vulkan","adapter":"llvmpipe (LLVM 15.0.7, 256 bits)","software":true},"compositionId":"default","compositions":[{"id":"default","width":1920,"height":1080,"frameRate":{"numerator":30,"denominator":1},"durationInFrames":150}],"outputs":[{"type":"video","path":"/home/render/qs/film/film.mp4","firstFrame":0,"frames":150,"start":0.0,"duration":5.0,"muxer":"ffmpeg/mp4","video":{"component":"ffmpeg-x264/libx264","codec":"avc1.640028"},"audio":null,"alpha":false,"files":["/home/render/qs/film/film.mp4"]}],"warnings":["font family \"sans-serif\" (weight 400) is not installed or loaded; text layer \"root.0.1\" uses a fallback font"],"quality":[],"stats":{"elapsedSeconds":5.61,"renderFps":29.8}}
[exit 0]

$ celesta info --react film.tsx --json
{"status":"ok","source":"/home/render/qs/film/film.tsx","composition":{"width":1920,"height":1080,"fps":30.0,"frames":150,"duration":5.0},"compositionId":"default","compositions":[{"id":"default","width":1920,"height":1080,"frameRate":{"numerator":30,"denominator":1},"durationInFrames":150}],"outputs":[],"warnings":[],"quality":[],"stats":{"elapsedSeconds":0.091}}
[exit 0]

$ celesta export --react film.tsx --frame 0 frame.png --json
{"status":"error","error":{"code":"output_exists","message":"output already exists: frame.png","hint":"pass --overwrite to replace it, or write to another path"},"source":"/home/render/qs/film/film.tsx","composition":{"width":1920,"height":1080,"fps":30.0,"frames":150,"duration":5.0},"compositionId":"default","compositions":[{"id":"default","width":1920,"height":1080,"frameRate":{"numerator":30,"denominator":1},"durationInFrames":150}],"outputs":[],"warnings":[],"quality":[],"stats":{"elapsedSeconds":0.407}}
[exit 1]

$ celesta export --react film.tsx still.png --json
{"status":"error","error":{"code":"unsupported_output","message":"no muxer writes .png files","hint":"PNG, JPEG and WebP outputs are still images: select frames with --frame, as in --frame 0 (or --frame 0,30,60, or --every 30)"},"source":"/home/render/qs/film/film.tsx","composition":{"width":1920,"height":1080,"fps":30.0,"frames":150,"duration":5.0},"compositionId":"default","compositions":[{"id":"default","width":1920,"height":1080,"frameRate":{"numerator":30,"denominator":1},"durationInFrames":150}],"outputs":[],"warnings":[],"quality":[],"stats":{"elapsedSeconds":0.447}}
[exit 1]

$ celesta export film.tsx out.png --json
{"status":"error","error":{"code":"project_json","message":"could not load export project: invalid project JSON: expected value at line 1 column 1","hint":"this looks like a React entry: add --react to read it as one"},"source":"/home/render/qs/film/film.tsx","outputs":[],"warnings":[],"quality":[],"stats":{"elapsedSeconds":0.342}}
[exit 1]

$ ffprobe -v error -show_entries stream=codec_name,codec_tag_string,width,height,pix_fmt,nb_frames -of compact film.mov
/src/t.sh: line 2: ffprobe: command not found
[exit 127]

$ ffprobe -v error -show_entries stream=codec_name,codec_tag_string,width,height,pix_fmt,nb_frames -of compact film.mp4
/src/t.sh: line 2: ffprobe: command not found
[exit 127]

$ ls -l frame.png film.mov film.mp4
-rw-r--r-- 1 render render 10832746 Nov 24 15:30 film.mov
-rw-r--r-- 1 render render    20418 Nov 24 15:30 film.mp4
-rw-r--r-- 1 render render    24830 Nov 24 15:30 frame.png
[exit 0]

$ ffprobe (host) film.mov
stream|codec_name=prores|codec_tag_string=ap4h|width=1920|height=1080|pix_fmt=yuva444p12le|nb_frames=150
format|duration=5.000000

$ ffprobe (host) film.mp4
stream|codec_name=h264|codec_tag_string=avc1|width=1920|height=1080|pix_fmt=yuv420p|nb_frames=150
format|duration=5.000000

$ file frame.png
qsout/frame.png: PNG image data, 1920 x 1080, 8-bit/color RGBA, non-interlaced
```
