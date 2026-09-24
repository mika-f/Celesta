# Celesta

Celesta is a desktop video editor that combines timeline editing with
React-based compositions. Arrange video, images, text, and audio visually, or
use React to create animated titles and reusable components. Preview your work
in the editor and export it as an MP4.

Celesta is under active development. The instructions below run it from
source. Existing package names, commands, and the `.celesta.json` project file
extension still use `celesta` for compatibility.

## What you can do

- **Edit on a timeline:** import local media, arrange clips on multiple tracks,
  move and trim clips, and undo or redo edits.
- **Preview frame by frame:** play your composition with synchronized audio,
  scrub the timeline, or step through individual frames.
- **Mix audio:** adjust clip and master volume, add volume keyframes, and mute
  or solo tracks. Waveforms help you place and trim audio.
- **Create character dialogue:** combine portraits, expressions, subtitles,
  and voice recordings. Assign mouth images and generate lip-sync cues from
  dialogue text and audio.
- **Compose with React:** use components, hooks, animation helpers, and layouts
  to build scenes. Expose editable properties in the editor and combine React
  content with a project timeline.
- **Export MP4:** render H.264 video with AAC audio, either from the editor or
  the command line. Export the whole composition or a selected time range.

## Run from source

You need Rust 1.89 or later, the native build tools for your platform, and
FFmpeg 7.1 or later development libraries. React compositions additionally
require Node.js 18 or later and pnpm.

### 1. Prepare FFmpeg

Celesta links to FFmpeg libraries; installing only the `ffmpeg` command-line
executable is not sufficient.

- **Windows:** install the MSVC C++ build tools and vcpkg, run
  `vcpkg install ffmpeg[x264]:x64-windows-static-md`, and set `VCPKG_ROOT` to
  your vcpkg directory.
- **macOS:** install the Xcode Command Line Tools, then run
  `brew install ffmpeg pkg-config`.
- **Linux:** install `pkg-config` and the FFmpeg development packages:
  `libavcodec-dev`, `libavformat-dev`, `libavfilter-dev`, `libavdevice-dev`,
  `libavutil-dev`, `libswscale-dev`, and `libswresample-dev`. Your distribution
  must provide FFmpeg 7.1 or later. Native window-system and graphics development
  packages may also be required by GPUI.

### 2. Get the source and launch

```sh
git clone https://github.com/mika-f/celesta.git
cd celesta
cargo run -p celesta-editor --release
```

The editor opens a built-in demo. To open an existing project instead, pass its
path:

```sh
cargo run -p celesta-editor --release -- examples/voiceroid.celesta.json
```

## Make your first video

1. **Start with a project.** Launch the demo above, or open the minimal example
   with `cargo run -p celesta-editor --release -- examples/minimal.celesta.json`.
   Use **Save As** to save your own copy.
2. **Import your media.** Choose **Import** in the Assets panel and select local
   video, image, or audio files.
3. **Add clips.** Select an asset and choose **Add** to insert it at the playhead,
   or drag it to a compatible timeline track.
4. **Arrange and trim.** Drag a clip's body to move it. Select a clip and drag
   either edge handle to change its start or end. Use the Inspector to edit the
   selected clip's available properties.
5. **Preview.** Press Space to play or pause. Use the left and right arrow keys
   to step one frame at a time, or drag along the timeline ruler to scrub.
6. **Save and export.** Save your project, then choose **Export** and an MP4
   destination. Progress appears in the toolbar; **Cancel Export** stops the job.

Project files reference your source media. Keep those files available when
reopening or sharing a project. If you move a file, select the missing asset in
the Assets panel and use **Relink** to locate it again.

### Character dialogue

Open `examples/voiceroid.celesta.json` to try a dialogue project with a sample
portrait and voice recording. In your own project, assign portrait expressions
and mouth images to a character, then select an audio-backed Dialogue clip and
choose **Generate from voice** to create lip-sync cues. Regenerate the cues after
changing the dialogue text or voice recording.

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

## Current limitations

- Media must be available as local files; remote media URLs are not supported.
- MP4 export requires non-zero, even-numbered width and height.
- Lip-sync generation uses dialogue text and the voice waveform. It does not
  perform speech recognition, so the text should match the recording.

## Build a Windows package

To create a Windows installer or portable ZIP with Node.js included, follow the
[Windows packaging guide](packaging/windows/README.md).

## Build a macOS package

To create a DMG with `Celesta.app` and an Applications shortcut for
drag-and-drop installation, follow the [macOS packaging guide](packaging/macos/README.md).
Node.js is included. Developer ID signing and notarization are supported for
distribution outside the Mac App Store.

## Report a problem

Open an issue in this repository with your operating system, steps to reproduce
the problem, and any error message. If possible, include a small project that
reproduces it, along with media you have permission to share.

## License

Celesta's packages are declared under MIT OR Apache-2.0.
