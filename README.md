# Celesta

Celesta is a code-first video tool. Build animated scenes with React and
TypeScript, combining video, images, text, shapes, and audio. Preview your work
in the Celesta desktop app and export it as an MP4 or PNG frames.

Celesta is under active development. Packaged desktop apps include the React
runtime and Node.js; the instructions below also cover running from source.

## Gallery

### Main Visual - [Reel](examples/reel/film.tsx) by Claude Opus 5.5

https://github.com/user-attachments/assets/df585cd3-26a1-4d35-aaff-e36eea4e7d14

for more examples, see the [examples directory](examples) and the [gallery](docs/gallery.md).

## What you can do

- **Compose with React:** use components, hooks, sequences, transitions, and
  layouts to build scenes. Join scenes with cuts, cross-fades, slides, or
  wipes using `<TransitionSeries>`, which works out the overlaps and length. Animate positions, colors, and effects with
  interpolation and springs, or hold a scene at a chosen frame with
  `<FreezeFrame>`.
- **Draw and lay out content:** combine rectangles, circles, ellipses, arrows,
  and paths with text. Load fonts, measure text, fit it into a box, and wrap
  Japanese text at phrase boundaries.
- **Use media:** load local or HTTP(S) video, images, and audio. Remote files
  are downloaded into a cache for native preview and export.
- **Preview frame by frame:** open a React composition, play it
  with synchronized audio, scrub the timeline, or step through frames. The
  timeline, asset list, and Inspector show what the composition contains.
  The preview reloads when you save its source.
- **Check audio:** see waveforms and track levels, mute or solo tracks, and
  set the preview volume. Use frame-based keyframes for volume fades and
  playback-rate changes.
- **Create character dialogue:** combine image or PSD portraits, expressions,
  subtitles, voice recordings, lip-sync cues, and blinking. Use
  `<DialogueSeries>` to sequence conversations.
- **Export MP4:** render H.264 video with AAC audio, either from the app or
  the command line. Export the whole composition or a selected time range.
- **Inspect frames:** export selected frames as PNG files or a labelled
  contact sheet, and request machine-readable export results for scripts.

## Start with the desktop app

Download a macOS or Windows package from the
[GitHub releases](https://github.com/mika-f/celesta/releases/latest).
Choose **File › Create New Project…**, select a folder, and edit the generated
`film.tsx` in your text editor. Celesta opens it for preview and reloads it
when you save. Choose **Export…** to write an MP4.

Preview and export use the bundled runtime. Install Node.js and a package
manager separately when you need TypeScript checking or external npm packages;
see [Create a React project](#create-a-react-project).

## Run from source

You need Rust 1.89 or later, the native build tools for your platform, and
FFmpeg 8.1.x development libraries (FFmpeg 9 and later are not supported yet).
To build the React runtime, use Node.js 24 and the pnpm version declared in
the root `package.json`, matching the repository's CI setup.

### 1. Prepare FFmpeg

Celesta links to FFmpeg libraries; installing only the `ffmpeg` command-line
executable is not sufficient.

- **Windows:** install the MSVC C++ build tools and vcpkg, run
  `vcpkg install ffmpeg[x264]:x64-windows-static-md`, and set `VCPKG_ROOT` to
  your vcpkg directory. Use a vcpkg release whose `ffmpeg` port is 8.1.x, such
  as `2026.07.29`; newer releases provide FFmpeg 9.
- **macOS:** install the Xcode Command Line Tools, then run
  `brew install ffmpeg@8 pkg-config`. `ffmpeg@8` is keg-only, so point
  `pkg-config` at it before building:
  `export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`.
- **Linux:** most distributions, including Ubuntu 24.04 LTS (FFmpeg 6.1), do
  not package FFmpeg 8.1.x, so build it from source with
  [`scripts/build-ffmpeg-linux.sh`](scripts/build-ffmpeg-linux.sh) after
  cloning the repository (step 2). On Debian and Ubuntu:

  ```sh
  sudo apt-get install -y build-essential nasm pkg-config curl xz-utils zlib1g-dev libx264-dev libclang-dev
  sudo scripts/build-ffmpeg-linux.sh /opt/ffmpeg8
  export PKG_CONFIG_PATH=/opt/ffmpeg8/lib/pkgconfig
  ```

  The script downloads the FFmpeg source from ffmpeg.org and builds static
  libraries with `libx264` into `/opt/ffmpeg8` (about 5 minutes on 4 cores).
  Celesta links them into its executables, so `LD_LIBRARY_PATH` is not needed;
  only `libx264` stays a shared library. With `libx264` the FFmpeg build is
  GPL-licensed (see [License](#license)). `libclang-dev` is for Cargo, which
  generates the FFmpeg bindings with it. Set `PKG_CONFIG_PATH` in every shell
  where you run Cargo. If your distribution does provide FFmpeg 8.1.x, install
  `pkg-config` and its `libavcodec`, `libavformat`, `libavfilter`,
  `libavdevice`, `libavutil`, `libswscale`, and `libswresample` development
  packages instead.

  The Celesta app also needs the ALSA, xkbcommon, and Fontconfig development
  packages:

  ```sh
  sudo apt-get install -y libasound2-dev libxkbcommon-x11-dev libfontconfig-dev
  ```

  These steps are tested on Ubuntu 24.04 in CI. To export on a machine without
  a GPU, see [Export without a GPU on Linux](#export-without-a-gpu-on-linux).

### 2. Get the source and launch

```sh
git clone https://github.com/mika-f/celesta.git
cd celesta
```

Prepare FFmpeg before running Cargo. On Linux, run the source-build script
from step 1 now. Then build the React runtime and launch the app:

```sh
pnpm install
pnpm --dir packages/react run codegen
pnpm --dir packages/react run build
cargo run -p celesta-editor --release -- packages/react/examples/title.tsx
```

The app opens the sample title composition. Choose **File › Open…** (Command-O
on macOS, Ctrl-O elsewhere) to open another React composition, or pass its path:

```sh
cargo run -p celesta-editor --release -- examples/reel/film.tsx
```

## Preview and export

1. **Open a composition.** Choose **File › Open…** and select a React
   composition (`.tsx`, `.jsx`, `.ts`, or `.js`).
   `packages/react/examples/title.tsx` is a small starting point.
2. **Edit the source file.** Change the composition in your text editor.
   Compositions reload automatically when you save. You can also choose
   **File › Reload** (Command-R on macOS, Ctrl-R elsewhere).
3. **Preview.** Press Space to play or pause (L plays, K stops). Use the left
   and right arrow keys to step one frame at a time, Shift with them to move
   one second, the up and down arrow keys to jump between clip edges, and Home
   and End to go to the start and end. Drag along the timeline ruler to scrub.
   Press = and - to zoom the timeline and Shift-Z to fit it; scroll sideways or
   drag the bar under the tracks to move along it. Loop playback (Command-/ on
   macOS, Ctrl-/ elsewhere) repeats the In–Out range when one is marked, and
   ' shows safe areas over the viewer. The Master meter beside the timeline
   shows the preview's audio levels. Select an asset, track, or clip to see its
   details in the Inspector.
4. **Export.** Choose **Export…** and an MP4 destination. To export a section,
   press I and O to mark its start and end. Progress appears in the status
   bar; **Cancel export** stops the job.

Compositions reference your source media. Keep local media available when
reopening or sharing a project. Assets whose files cannot be found are marked
in the Assets panel. Remote media needs network access on its first use;
subsequent runs reuse the cached files.

### Character dialogue

See [`with-dialogue-series.tsx`](packages/react/examples/with-dialogue-series.tsx)
for a conversation built with `<DialogueSeries>`, or
[`with-lip-sync.tsx`](packages/react/examples/with-lip-sync.tsx)
for a PSD portrait with lip-sync.

## Use React compositions

For source builds, complete [Run from source](#run-from-source) first. React
compositions under `examples/*` are workspace packages too.

Use the files in [`packages/react/examples`](packages/react/examples) as
starting points. They demonstrate text, animation, layout, shapes, dialogue,
audio, and project properties. Import composition components from
`@celesta/react`; deterministic random numbers, noise, and other math helpers
come from `@celesta/math`.

Syntax-highlighted code is available separately in [`@celesta/code`](packages/code/README.md).
The React build also builds it, and the desktop app bundles it separately.
Import `Code` from `@celesta/code`. It supports TSX, TypeScript, JSON,
Bash, line highlights, and measured caret positions. For typing animations,
combine it with `useTypewriter()` from `@celesta/react`.
It is optional and does not add highlighting dependencies to `@celesta/react`.

Visual layers and groups accept `blur`, `shadow`, and `glow`. Radii and shadow
offsets use output pixels. All three can change each frame through React props:

```tsx
import { Group, Text, useCurrentFrame } from '@celesta/react';

function Title() {
  const frame = useCurrentFrame();
  return <Group blur={Math.min(frame / 10, 8)}
    glow={{ color: '#FFCC6680', blur: 12 }}>
    <Text shadow={{ color: '#000000B0', blur: 6, offsetX: 4, offsetY: 6 }}>
      Celesta
    </Text>
  </Group>;
}
```

Blur radii are limited to 64 pixels.

### Create a React project

With Celesta's command-line executables on PATH, initialize a directory:

```sh
celesta-editor --init my-video
cd my-video
celesta-editor film.tsx
celesta-exporter --react film.tsx output.mp4
```

Use `celesta-editor --init` to initialize the current directory. In the GUI,
choose **File > Create New Project…** (Ctrl-N, or Command-N on macOS), or the
toolbar's **Create New Project…**, and select a project folder. The folder
picker can create a new folder. Celesta initializes it and opens `film.tsx`.

Initialization creates a five-second `film.tsx`, a private `package.json` with
preview/export/typecheck scripts, `tsconfig.json`, `.gitignore`, and the matching
type declarations in `.celesta/`. Preview and export use the bundled runtime
without a package installation. Run `pnpm install` and `pnpm typecheck` to use
the project's TypeScript compiler.

Re-running `--init` refreshes `.celesta/` and creates missing starter files.
Existing source files, `package.json` (including dependencies and scripts),
`.gitignore`, and `tsconfig.json` are kept unchanged. If your existing tsconfig
does not extend `./.celesta/tsconfig.json`, Celesta reports the required change;
merge the base configuration into your configuration as appropriate.

For source builds, use `cargo run -p celesta-editor -- --init my-video` after
building the React runtime above. Preview with
`cargo run -p celesta-editor -- my-video/film.tsx` and export with
`cargo run -p celesta-exporter --release -- --react my-video/film.tsx output.mp4`.
Desktop packages currently name these executables `Celesta` and `Celesta-export`
(`.exe` on Windows); use their installed paths in place of the Cargo binary
names, including in generated scripts when needed.

### Add external dependencies

Install general npm packages in the project that imports them. For a repository
example, add the dependency to that example's workspace package:

```sh
pnpm --dir examples/versus add ag-psd
cargo run -p celesta-editor --release -- examples/versus/film.tsx
cargo run -p celesta-exporter --release -- --react examples/versus/film.tsx output.mp4
```

For a project outside the repository, run `pnpm add ag-psd` inside the initialized
directory. Both preview and export resolve external imports from the importing
file's project `node_modules`. Celesta supplies `@celesta/react`, `@celesta/math`,
`@celesta/code`, and React from its runtime; these do not need to be added to an
external project's dependencies. Example manifests use `workspace:*` for the
local Celesta packages so Node tools can also resolve their imports.

Node asset preparation scripts use the same project dependencies directly. For
example, save this as `prepare-assets.mjs` beside your `package.json`:

```js
import { readFileSync, writeFileSync } from 'node:fs';
import { readPsd } from 'ag-psd';

const { width, height } = readPsd(readFileSync(new URL('./portrait.psd', import.meta.url)), {
  skipLayerImageData: true,
  skipCompositeImageData: true,
  skipThumbnail: true,
});
writeFileSync(new URL('./portrait-info.json', import.meta.url), JSON.stringify({ width, height }));
```

Run `node prepare-assets.mjs` from the project directory after installing its
dependencies; no `createRequire` workaround pointing to another package is
needed. Import the generated JSON from your composition as usual. For `.ts`
preparation scripts, use a Node version with TypeScript stripping support or
install a TypeScript runner in that project. The bundled Node runtime evaluates
compositions; direct Node scripts use your installed Node.

### Type-check your compositions

Choose **File > Set Up TypeScript** with a React composition open. Celesta
copies the `@celesta/react`, `@celesta/math`, `@celesta/code`, React, and Node.js type
declarations that match its bundled runtime into a `.celesta/` folder in your
project. To start a new
project before writing its first composition, choose **File > Set Up TypeScript
in Folder…** and pick the project folder instead. If the project
has no `tsconfig.json`, Celesta creates one that extends
`./.celesta/tsconfig.json`. If a `tsconfig.json` already exists, add
`"extends": "./.celesta/tsconfig.json"` to it. You don't need to install
`@celesta/react`, `@celesta/math`, `@celesta/code`, `react`, or `@types/*` from npm.

Celesta updates `.celesta/` when you open the project in a newer version. The
folder ignores itself in Git.

## Export from the command line

Export a React composition after completing the source-build setup:

```sh
cargo run -p celesta-exporter --release -- --react packages/react/examples/title.tsx output.mp4
```

In a terminal, exports show a [Ratatui](https://github.com/ratatui/ratatui)
dashboard with a timed pipeline, rendered-frame progress, a throughput history
graph, FPS, elapsed time, estimated rendering time remaining, export settings,
and warnings. Smaller terminals use a compact layout. The dashboard stays
in your terminal's scrollback after the export finishes. The rendering bar
can reach 100% while audio mixing and MP4 muxing are still running.

Use `--no-ui` for text progress. Redirected output, CI without a terminal,
and `TERM=dumb` automatically use text progress without terminal control codes.

For scripts and AI agents, `--json` prints no progress. When the export ends,
it prints one line of JSON on stdout: the status, each written file with its
frame numbers, warnings, the composition's size, fps and frame count once it
has been loaded, and on failure an error `code`, `message` and sometimes a
`hint`. When the arguments are rejected, the JSON has only `status` and
`error`. The exit status is 0 on success, 1 when the export fails, and 2 for
invalid arguments.

Add `--overwrite` to replace an existing output file. To export a section, add
`--from` and `--to` with times in `HH:MM:SS.mmm`, `MM:SS.mmm`, or seconds:

```sh
cargo run -p celesta-exporter --release -- --react --from 0 --to 1 packages/react/examples/title.tsx section.mp4
```

The video is encoded with libx264 using `--preset medium --crf 18` by default.
For faster exports, pass a faster preset such as `--preset veryfast`. The file
will be larger, but quality stays about the same. `--crf` takes a value from 0
to 51. Lower values give higher quality and larger files:

```sh
cargo run -p celesta-exporter --release -- --react --preset veryfast packages/react/examples/title.tsx draft.mp4
```

On a hardware GPU, frames are converted from RGB to the video's YUV colors
on the GPU. The exporter then reads less data back from the GPU, and the
encoder has less work to do. `--color-conversion encoder` does the conversion
in the encoder instead, which is how software renderers are always handled.
`--color-conversion gpu` forces the GPU conversion.

To check frames as PNG instead of encoding a video, select zero-based frames
with `--frame`/`--frames`, or every *n*th frame (plus the last one) with
`--every`, which can be narrowed with `--from`/`--to`. Add `--contact-sheet`
to lay the selection out as labelled tiles on one image (`--columns`,
`--tile-width`):

```sh
cargo run -p celesta-exporter --release -- --react --frames 0,90 packages/react/examples/title.tsx check.png
cargo run -p celesta-exporter --release -- --react --every 60 --contact-sheet packages/react/examples/title.tsx sheet.png
```

### Export without a GPU on Linux

The exporter renders through Vulkan on Linux. On machines without a GPU, such
as CI runners and cloud containers, install Mesa's software Vulkan driver
(lavapipe), which renders on the CPU:

```sh
sudo apt-get install -y mesa-vulkan-drivers
```

Software rendering is much slower than a GPU: with 4 CPU cores, 1080p
compositions render at roughly 10 to 60 frames per second, depending on the
scene. Check a single frame before exporting the whole video:

```sh
cargo run -p celesta-exporter --release -- --react packages/react/examples/title.tsx --frame 0 frame.png
```

Minimal containers often have no fonts installed, and text cannot be drawn
without at least one. Install some, such as `fonts-dejavu-core` (and
`fonts-noto-cjk` for Japanese), or load font files with `<Font>`.

On a software renderer, `--color-conversion auto` leaves the RGB-to-YUV
conversion to the encoder. `--render-quality draft` skips re-rasterizing scaled
text, which is quicker for checking timing but not for checking pixels.

To export in a container without building Celesta on the host, use the
[Linux container image](packaging/linux/README.md).

## Use with AI agents

[`skills/celesta`](skills/celesta) is an [Agent Skill](https://agentskills.io)
that teaches coding agents such as Claude Code and Codex to write React
compositions, check them without the GUI, and export them. Install it with
the `skills` CLI:

```sh
npx skills add mika-f/celesta
```

Or copy the `skills/celesta` folder into your agent's skills directory, for
example `~/.claude/skills/` or a project's `.claude/skills/`.

The skill includes `scripts/inspect.mjs`, which loads a React composition
with Celesta's bundled runtime and prints the layers and audio of selected
frames, and lists PSD layer paths for character portraits:

```sh
node skills/celesta/scripts/inspect.mjs packages/react/examples/title.tsx --frames 0,-1
node skills/celesta/scripts/inspect.mjs --psd-layers examples/assets/lipsync-fixture.psd
```

## Current limitations

- Remote media is cached by URL without revalidation. Use a new URL or clear
  the cache when the remote file changes.
- MP4 export requires non-zero, even-numbered width and height.
- The Celesta app previews and exports compositions; edit their source files
  in a text editor.

## Build a Windows package

To create a Windows installer or portable ZIP with Node.js included, follow the
[Windows packaging guide](packaging/windows/README.md).

## Build a macOS package

To create a DMG with `Celesta.app` and an Applications shortcut for
drag-and-drop installation, follow the [macOS packaging guide](packaging/macos/README.md).
Node.js is included. Developer ID signing and notarization are supported for
distribution outside the Mac App Store.

## Build a Linux container image

To export in Docker on a machine without a GPU, such as a CI runner, build the
headless exporter image described in the
[Linux container guide](packaging/linux/README.md).

## Website

The English product website lives in [`packages/website`](packages/website).
It uses Vite, React, and Tailwind CSS, with Cloudflare Workers Static Assets
deployment configured. See the [website guide](packages/website/README.md) for
local development, production builds, and deployment instructions.

The browser composition evaluator, Canvas preview, and MP4 exporter are a
reusable package in [`packages/web`](packages/web). The website consumes its
public `@celesta/web` API. See the [web package guide](packages/web/README.md)
for an embedding example and browser requirements.

## Report a problem

Open an issue in this repository with your operating system, steps to reproduce
the problem, and any error message. If possible, include a small project that
reproduces it, along with media you have permission to share.

## License

Celesta's own source code is declared under MIT OR Apache-2.0.

The official Windows and macOS binary packages additionally bundle an FFmpeg
build with the `x264` encoder enabled, which is GPL-2.0-or-later licensed.
Distributing that FFmpeg build makes the binary package as a whole subject to
GPL-2.0-or-later, in addition to Celesta's own MIT/Apache-2.0 source license.
See [`LICENSE-GPL-2.0`](LICENSE-GPL-2.0) for the license text and
[`packaging/GPL-SOURCE-OFFER.md`](packaging/GPL-SOURCE-OFFER.md) for the
corresponding-source offer that accompanies each release. Building Celesta
yourself from source, or building it with an LGPL-only FFmpeg configuration,
is not affected.
