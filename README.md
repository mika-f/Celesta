# Celesta

Celesta is a code-first video tool. Build animated scenes with React and
TypeScript, combining video, images, text, shapes, and audio. Preview your work
in the Celesta desktop app and export it as MP4, transparent MOV or WebM, or
PNG, JPEG, or WebP frames.

**[Website](https://celesta.natsuneko.cat/)** ·
**[Documentation](https://celesta.natsuneko.cat/en/docs/)** ·
**[日本語ドキュメント](https://celesta.natsuneko.cat/ja/docs/)** ·
**[Download](https://github.com/mika-f/Celesta/releases/latest)** ·
**[Showcase](https://celesta.natsuneko.cat/en/showcase/)**

Celesta is under active development. The official website includes a browser
playground, example projects, and the full user guide in English and Japanese.

## See it in action

### [Reel](examples/reel/film.tsx) by Claude Opus 5.5

https://github.com/user-attachments/assets/df585cd3-26a1-4d35-aaff-e36eea4e7d14

Explore the [showcase](https://celesta.natsuneko.cat/en/showcase/) to try
browser-ready projects and view their source. The [examples directory](examples)
contains the projects and their setup instructions.

## What you can do

- **Compose with React:** build reusable scenes with components, hooks,
  sequences, transitions, interpolation, and springs.
- **Create motion graphics:** combine text, shapes, paths, particles, layouts,
  and visual effects with local or remote media.
- **Preview as you edit:** play with synchronized audio, scrub or step through
  frames, and inspect layers and tracks. The app reloads when you save your source.
- **Adjust on the canvas:** move, rotate, resize, or retype a layer, or edit its
  props in the Properties tab. Celesta writes the change back into your source,
  with undo.
- **Build character dialogue:** combine image or PSD portraits, expressions,
  subtitles, voice recordings, lip sync, and blinking.
- **Export and automate:** render videos, PNG, JPEG, or WebP frames, and contact sheets from
  the desktop app or CLI, with JSON results and MCP tools for scripts and agents.

## Get started

1. [Download and install Celesta](https://celesta.natsuneko.cat/en/docs/installation/)
   for macOS or Windows. The packages include the React runtime, Node.js, and
   media dependencies.
2. Choose **File › Create New Project…**, pick a template, and select a folder.
3. Edit the generated `film.tsx` in your text editor. Save to reload the preview,
   then choose **Export…** to write a video.

For Linux, follow the [source-build guide](https://celesta.natsuneko.cat/en/docs/build-from-source/)
or use the [headless container](packaging/linux/README.md).

The website documents the complete workflow:

| Task | Guide |
| --- | --- |
| Start a project or use a template | [Create a project](https://celesta.natsuneko.cat/en/docs/create-project/) |
| Write your first scene | [React compositions](https://celesta.natsuneko.cat/en/docs/react-compositions/) |
| Play, scrub, and inspect | [Preview](https://celesta.natsuneko.cat/en/docs/preview/) |
| Export video or check frames from the CLI | [Export](https://celesta.natsuneko.cat/en/docs/export/) |
| Find components and hooks | [API reference](https://celesta.natsuneko.cat/en/docs/reference/) |
| Resolve setup or rendering problems | [Troubleshooting](https://celesta.natsuneko.cat/en/docs/troubleshooting/) |

## Use with AI agents

Install the [Celesta Agent Skill](skills/celesta) to teach coding agents such
as Claude Code and Codex to write, inspect, and export compositions:

```sh
npx skills add mika-f/celesta
```

The desktop packages also ship `Celesta-mcp`, an MCP server for project
initialization, scene inspection, PNG frame and contact-sheet rendering, video
export, and documentation matching the installed version. For example, register
it with Claude Code on macOS:

```sh
claude mcp add celesta -- /Applications/Celesta.app/Contents/MacOS/Celesta-mcp
```

On Windows, register `Celesta-mcp.exe` from the install folder. From source,
build it with `cargo build -p celesta-mcp --release`. See the
[skill setup guide](skills/celesta/references/setup.md) for runtime setup and
[inspection tools](skills/celesta/scripts) for checks without the GUI.

## Develop Celesta

To run from source, use Rust 1.89 or later, Node.js 26, the pnpm version in
[`package.json`](package.json), and your platform's native build tools.
[`mise.toml`](mise.toml) pins the Node.js and pnpm versions CI uses.
Prepare the **FFmpeg 8.1.x development libraries** using the
[source-build guide](https://celesta.natsuneko.cat/en/docs/build-from-source/)
before building the media plugin (`plugins/ffmpeg`), which links them, as
its tests do; Celesta's own crates link no FFmpeg, and the core's tests use
a lossless test codec instead. The FFmpeg
executable alone is insufficient; FFmpeg 9 and later are not supported yet.

```sh
git clone https://github.com/mika-f/Celesta.git celesta
cd celesta
pnpm install
pnpm run build:runtime
node scripts/plugins/build.mts plugins/ffmpeg
export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"   # PowerShell: $env:CELESTA_PLUGIN_PATH = "$PWD\plugins\dist"
cargo run -p celesta-editor --release -- packages/cli/examples/title.tsx
```

Celesta reads and writes video and audio through the media plugin, which it
finds through `CELESTA_PLUGIN_PATH`. Set it to the absolute path of
`plugins/dist`, in each shell where you run Celesta.

`pnpm run build:runtime` also generates the TypeScript bindings from the Rust
types. It regenerates them only when those Rust sources change, so run it again
after pulling.

On Linux, clone the repository before preparing FFmpeg so the guide's build
script is available. The [source of the build guide](packages/website/content/en/docs/05-(more)/03-build-from-source.md)
is also in this repository.

After building the runtime, check the area you changed. Common checks are:

```sh
pnpm run test:runtime
pnpm run typecheck:examples
cargo test --workspace --locked
```

Tests that need a GPU adapter, Node.js, or a color emoji font print a line
starting with `skipping` and pass when it is missing. On a machine with a GPU,
set `CELESTA_REQUIRE_GPU=1` so a missing adapter fails the GPU tests instead;
after building the runtime, `CELESTA_REQUIRE_RUNTIME=1` does the same for a
missing `node` or `packages/cli/dist/cli.js`.
CI lists the skipped tests in each job's summary.

See the [CI workflow](.github/workflows/ci.yml) for the full checks and platform
setup. For rendering changes, include screenshots of the same scene before and
after the change.

### Repository guide

| Location | Contents |
| --- | --- |
| [`crates/`](crates) | Native desktop app, renderers, exporter, media pipeline, and MCP server |
| [`packages/`](packages) | React composition APIs and the Node.js runtime |
| [`packages/web/`](packages/web/README.md) | Browser evaluator, Canvas preview, and MP4 exporter |
| [`packages/website/`](packages/website/README.md) | Official website, English and Japanese documentation, and playground |
| [`examples/`](examples) | Complete video projects; [`packages/cli/examples/`](packages/cli/examples) has smaller API examples |
| [`templates/`](templates) | Starter projects bundled with Celesta |
| [`skills/celesta/`](skills/celesta) | Agent instructions, API guides, and inspection scripts |
| [`HANDOFF.md`](HANDOFF.md) | Architecture, runtime constraints, and validation for contributors |
| [`docs/handoff/`](docs/handoff) | Dated implementation notes for each feature |
| [`docs/superpowers/`](docs/superpowers) | Design specs and implementation plans |
| [`docs/performance/`](docs/performance) | Benchmarks and renderer performance notes |

To build release packages, use the [macOS](packaging/macos/README.md),
[Windows](packaging/windows/README.md), or [Linux container](packaging/linux/README.md)
packaging guide.

## Report a problem

[Open an issue](https://github.com/mika-f/Celesta/issues) with your operating
system, steps to reproduce the problem, and any error message. If possible,
include a small project that reproduces it, along with media you have permission
to share.

## License

Celesta's own source code is declared under MIT OR Apache-2.0.

Celesta's executables (the app, `celesta-exporter`, and `Celesta-mcp`) link
no FFmpeg. Video and audio files are read and written by a separate program,
the media plugin `celesta-codec-ffmpeg`, which Celesta starts and talks to only
over its documented plugin protocol on standard input and output. The
executables are distributed under MIT OR Apache-2.0 alone.

The official Windows and macOS packages, and the Linux container image, ship
the media plugin built against an FFmpeg with the `x264` encoder enabled, which
is GPL-2.0-or-later licensed. That makes the media plugin's program, as
distributed, subject to GPL-2.0-or-later; it sits in its own plugin folder
(`plugins/ffmpeg`, or `Contents/Resources/PlugIns/ffmpeg` in the macOS app)
with
[`LICENSE-GPL-2.0`](LICENSE-GPL-2.0) and the corresponding-source offer in
[`packaging/GPL-SOURCE-OFFER.md`](packaging/GPL-SOURCE-OFFER.md). Its own
source code is MIT OR Apache-2.0 like the rest of the repository. The same
FFmpeg build also links `libvpx` and `libopus` (BSD-3-Clause), the WebM
encoders. Building the media plugin against an LGPL-only FFmpeg configuration
avoids the GPL, at the cost of MP4 (H.264) export.
