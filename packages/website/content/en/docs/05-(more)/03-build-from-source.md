To build Celesta, install **Rust 1.89 or later**, your platform’s native build tools, and **FFmpeg 8.1.x development libraries**. Use Node.js 26 and the pnpm version declared in the root package.json to build the React runtime, matching CI.

> [!NOTE]
> **The libraries matter**
> The FFmpeg command-line executable alone is not enough. Celesta's media plugin, `plugins/ffmpeg` in the repository, links against the development libraries; the app and the exporter link no FFmpeg. FFmpeg 9 and later are not supported yet.

## 1. Prepare your platform

The media plugins use two independent FFmpeg 8.1.x builds. `ffmpeg` is the
standard LGPL build (decoding, ProRes, VP9, AAC, Opus, PCM and muxing), linked
with zlib, libopus and libvpx, which are BSD licensed. The optional
`ffmpeg-x264` build adds H.264 with libx264 and is the only one that enables
GPL code. Never install the two builds into the same prefix.

After cloning the repository in step 2, run the commands for your platform
from its root. Keep these environment variables in the shell that builds plugins.

::::tabs[Source build platform]{default="macOS"}
:::tab[macOS]

Install the Xcode Command Line Tools and Homebrew. The script builds FFmpeg
dylibs with a separate prefix for each variant:

```sh title="Native build dependencies"
brew install pkg-config nasm x264 libvpx opus
export PKG_CONFIG_PATH="$(brew --prefix x264)/lib/pkgconfig:$(brew --prefix opus)/lib/pkgconfig:$(brew --prefix libvpx)/lib/pkgconfig"
for variant in standard x264; do
  FFMPEG_VARIANT="$variant" scripts/build-ffmpeg-linux.sh "$PWD/plugins/target/ffmpeg/$variant"
done
export PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/standard/lib/pkgconfig"
export FFMPEG_X264_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/x264/lib/pkgconfig"
```

The standard build links Homebrew's `opus` and `libvpx`, and the x264 build
its `x264`; their `pkgconfig` folders must be on `PKG_CONFIG_PATH` while the
script runs.

For local use, Homebrew's `ffmpeg@8` also works for the standard plugin:
`brew install ffmpeg@8 pkg-config`, then set
`PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"`. A system GPL build
is accepted locally; the standard plugin still exposes no libx264 encoder.

:::
:::tab[Windows]

On Windows, use vcpkg 2026.07.29 (FFmpeg 8.1.x) and the MSVC build tools.
Use separate vcpkg installation roots for `ffmpeg[zlib,opus,vpx]` (standard)
and `ffmpeg[x264]`, with the `x64-windows-static-md` triplet. Set `VCPKG_ROOT`
to the standard root and `FFMPEG_X264_VCPKG_ROOT` to the x264 root
(`FFMPEG_X264_VCPKGRS_TRIPLET` optionally names its triplet). The repository's setup-ffmpeg CI action shows a complete
Windows setup. A system FFmpeg 8.1.x with the needed development libraries
also works locally, including a GPL build. Distribution packages require the
separate license profiles above.

:::
:::tab[Linux]

The script builds static FFmpeg libraries. On Debian and Ubuntu, install the
native dependencies and the editor's system libraries first:

```sh title="Native build dependencies"
sudo apt-get install -y build-essential nasm pkg-config curl xz-utils zlib1g-dev libx264-dev libvpx-dev libopus-dev libclang-dev
sudo apt-get install -y libasound2-dev libxkbcommon-x11-dev libfontconfig-dev
for variant in standard x264; do
  FFMPEG_VARIANT="$variant" scripts/build-ffmpeg-linux.sh "$PWD/plugins/target/ffmpeg/$variant"
done
export PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/standard/lib/pkgconfig"
export FFMPEG_X264_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/x264/lib/pkgconfig"
```

The standard build needs `libopus-dev` and `libvpx-dev`; the x264 build needs
`libx264-dev`.

:::
::::

Local builds may use a system FFmpeg 8.1.x. To prepare redistribution, pass
`--distribution` to `scripts/plugins/build.mts`: it opens the encoders and runs
the explicit `--check-license` command before staging. The standard
distribution requires LGPL-only FFmpeg. Run distribution validation on the
target OS and architecture; cross-compiles cannot bypass the check.

## 2. Get the source

```sh title="Clone Celesta"
git clone https://github.com/mika-f/Celesta.git celesta
cd celesta
```

## 3. Build the React runtime

Before launching a source build, install the workspace dependencies and build the React runtime. The build also generates the shared types from the Rust source, so run it again after updating the source:

```sh title="Build the React runtime from source"
pnpm install
pnpm run build:runtime
```

## 4. Open a React composition

Build the media plugins once, and again after updating the source. Celesta reads and writes video and audio through them, and finds them through `CELESTA_PLUGIN_PATH`, which must be an absolute path. `plugins/avfoundation` (VideoToolbox and AVFoundation) builds on macOS only, and `plugins/mediafoundation` (Media Foundation) on Windows only. On any other system the script prints `avfoundation: skipped, it is built for macos only` or `mediafoundation: skipped, it is built for windows only` and builds the rest, so Linux builds FFmpeg alone. Set the variable in every shell where you run Celesta:

```sh title="Build the media plugins"
node scripts/plugins/build.mts plugins/ffmpeg plugins/ffmpeg-x264 plugins/avfoundation plugins/mediafoundation
export CELESTA_PLUGIN_PATH="$PWD/plugins/dist"   # PowerShell: $env:CELESTA_PLUGIN_PATH = "$PWD\plugins\dist"
```

```sh title="Launch the native app"
cargo run -p celesta-editor --release -- packages/cli/examples/title.tsx
```

The app opens the sample title composition with a preview, timeline, Assets panel, and Inspector. Pass another React entry to open it:

```sh title="Open the example project"
cargo run -p celesta-editor --release -- examples/reel/film.tsx
```

Run these commands from the repository root. The React examples under `examples/` are pnpm workspace packages, so this also installs their dependencies. To start your own project:

```sh title="Initialize a project from source"
cargo run -p celesta-editor --release -- --init my-video
```

See [Create a project](/en/docs/create-project/) for the generated files and dependency setup.

```sh title="Preview React from source"
cargo run -p celesta-editor --release -- packages/cli/examples/title.tsx
```

```sh title="Export React from source"
cargo run -p celesta-exporter --release -- --react packages/cli/examples/title.tsx output.mp4
```

Run Cargo commands from the repository root. Packaged-app users can skip this entire section and go straight to their [first React composition](/en/docs/react-compositions/).
