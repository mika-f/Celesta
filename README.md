# Celesta

Celesta is a code-first video tool. Describe a timeline of video, images,
text, and audio in a `.celesta.json` project, or build animated scenes with
React. Preview your work in the Celesta app and export it as an MP4.

Celesta is under active development. The instructions below run it from
source. Existing package names, commands, and the `.celesta.json` project file
extension still use `celesta` for compatibility.

## What you can do

- **Describe a timeline:** list local or `http(s)` media and place clips on
  multiple video, audio, overlay, and dialogue tracks in a project file.
- **Preview frame by frame:** open a project or React composition, play it
  with synchronized audio, scrub the timeline, or step through frames. The
  timeline, asset list, and Inspector show what the project contains.
- **Check audio:** see waveforms and track levels, mute or solo tracks, and
  set the preview volume.
- **Create character dialogue:** combine portraits, expressions, subtitles,
  voice recordings, and lip-sync cues.
- **Compose with React:** use components, hooks, animation helpers, and layouts
  to build scenes, and combine React content with a project timeline. The
  preview reloads when you save the composition.
- **Export MP4:** render H.264 video with AAC audio, either from the app or
  the command line. Export the whole composition or a selected time range.

## Run from source

You need Rust 1.89 or later, the native build tools for your platform, and
FFmpeg 8.1.x development libraries (FFmpeg 9 and later are not supported yet).
React compositions additionally require Node.js 18 or later and pnpm.

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
- **Linux:** install `pkg-config` and the FFmpeg development packages:
  `libavcodec-dev`, `libavformat-dev`, `libavfilter-dev`, `libavdevice-dev`,
  `libavutil-dev`, `libswscale-dev`, and `libswresample-dev`. Your distribution
  must provide FFmpeg 8.1.x. Native window-system and graphics development
  packages may also be required by GPUI.

### 2. Get the source and launch

```sh
git clone https://github.com/mika-f/celesta.git
cd celesta
cargo run -p celesta-editor --release
```

The app opens a built-in demo. Choose **File › Open…** (Command-O on macOS,
Ctrl-O elsewhere) to open a project or React composition, or pass its path:

```sh
cargo run -p celesta-editor --release -- examples/voiceroid.celesta.json
```

## Preview and export

1. **Open a project.** Choose **File › Open…** and select a `.celesta.json`
   project or a React composition (`.tsx`, `.jsx`, `.ts`, or `.js`).
   `examples/minimal.celesta.json` is a small starting point.
2. **Edit the source file.** Change the project or composition in your text
   editor. React compositions reload automatically when you save; for a
   project, choose **File › Reload** (Command-R on macOS, Ctrl-R elsewhere).
3. **Preview.** Press Space to play or pause. Use the left and right arrow keys
   to step one frame at a time, or drag along the timeline ruler to scrub.
   Select an asset, track, or clip to see its details in the Inspector.
4. **Export.** Choose **Export…** and an MP4 destination. To export a section,
   press I and O to mark its start and end. Progress appears in the status
   bar; **Cancel export** stops the job.

Project files reference your source media. Keep those files available when
reopening or sharing a project. Assets whose files cannot be found are marked
in the Assets panel.

### Character dialogue

Open `examples/voiceroid.celesta.json` to try a dialogue project with a sample
portrait and voice recording.

## Use React compositions

Set up the included React package once from the repository root:

```sh
cd packages/react
pnpm install
pnpm run codegen
pnpm run build
cd ../..
```

Open the sample title composition in the editor:

```sh
cargo run -p celesta-editor --release -- packages/react/examples/title.tsx
```

Use the files in `packages/react/examples` as starting points. They demonstrate
text, animation, layout, dialogue, and editable project properties. The package
is currently imported as `@celesta/react`.

### Type-check your compositions

Choose **File > Set Up TypeScript** with a React composition open. Celesta
copies the `@celesta/react`, React, and Node.js type declarations that match
its bundled runtime into a `.celesta/` folder in your project. If the project
has no `tsconfig.json`, Celesta creates one that extends
`./.celesta/tsconfig.json`. If a `tsconfig.json` already exists, add
`"extends": "./.celesta/tsconfig.json"` to it. You don't need to install
`@celesta/react`, `react`, or `@types/*` from npm.

Celesta updates `.celesta/` when you open the project in a newer version. The
folder ignores itself in Git.

To combine an existing timeline with React content, use a composition containing
`<ProjectTimeline />` and supply the companion project when exporting:

```sh
cargo run -p celesta-exporter --release -- --react packages/react/examples/with-project.tsx --project examples/editor-demo.celesta.json output.mp4
```

## Export from the command line

Export a project:

```sh
cargo run -p celesta-exporter --release -- examples/editor-demo.celesta.json output.mp4
```

Export a React composition after completing the React setup:

```sh
cargo run -p celesta-exporter --release -- --react packages/react/examples/title.tsx output.mp4
```

Add `--overwrite` to replace an existing output file. To export a section, add
`--from` and `--to` with times in `HH:MM:SS.mmm`, `MM:SS.mmm`, or seconds:

```sh
cargo run -p celesta-exporter --release -- --from 0 --to 1 examples/editor-demo.celesta.json section.mp4
```

The video is encoded with libx264 using `--preset medium --crf 18` by default.
For faster exports, pass a faster preset such as `--preset veryfast`. The file
will be larger, but quality stays about the same. `--crf` takes a value from 0
to 51. Lower values give higher quality and larger files:

```sh
cargo run -p celesta-exporter --release -- --preset veryfast examples/editor-demo.celesta.json draft.mp4
```

## Current limitations

- Media must be available as local files; remote media URLs are not supported.
- MP4 export requires non-zero, even-numbered width and height.
- The Celesta app previews projects but does not edit them. Change projects
  and compositions in their source files.

## Build a Windows package

To create a Windows installer or portable ZIP with Node.js included, follow the
[Windows packaging guide](packaging/windows/README.md).

## Build a macOS package

To create a DMG with `Celesta.app` and an Applications shortcut for
drag-and-drop installation, follow the [macOS packaging guide](packaging/macos/README.md).
Node.js is included. Developer ID signing and notarization are supported for
distribution outside the Mac App Store.

## Website

The English product website lives in [`packages/website`](packages/website).
It uses Vite, React, and Tailwind CSS, with Cloudflare Workers Static Assets
deployment configured. See the [website guide](packages/website/README.md) for
local development, production builds, and deployment instructions.

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
