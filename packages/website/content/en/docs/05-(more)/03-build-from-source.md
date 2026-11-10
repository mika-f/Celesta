To build Celesta, install **Rust 1.89 or later**, your platform’s native build tools, and **FFmpeg 8.1.x development libraries**. Use Node.js 26 and the pnpm version declared in the root package.json to build the React runtime, matching CI.

> [!NOTE]
> **The libraries matter**
> The FFmpeg command-line executable alone is not enough. Celesta's media plugin, `plugins/ffmpeg` in the repository, links against the development libraries; the app and the exporter link no FFmpeg. FFmpeg 9 and later are not supported yet.

## 1. Prepare your platform

The media plugins use four independent FFmpeg 8.1.x builds. `ffmpeg` is the
standard LGPL build (decoding, ProRes, AAC, PCM and muxing). The optional
`ffmpeg-x264`, `ffmpeg-opus` and `ffmpeg-vpx` builds add H.264, Opus and VP9.
Only x264 enables GPL code. Never install these builds into the same prefix.

After cloning the repository in step 2, run the commands for your platform
from its root. Keep these environment variables in the shell that builds plugins.

::::tabs[Source build platform]{default="macOS"}
:::tab[macOS]

Install the Xcode Command Line Tools and Homebrew. The script builds FFmpeg
dylibs with a separate prefix for each variant:

```sh title="Native build dependencies"
brew install pkg-config nasm x264 libvpx opus
export PKG_CONFIG_PATH="$(brew --prefix x264)/lib/pkgconfig:$(brew --prefix opus)/lib/pkgconfig:$(brew --prefix libvpx)/lib/pkgconfig"
for variant in standard x264 opus vpx; do
  FFMPEG_VARIANT="$variant" scripts/build-ffmpeg-linux.sh "$PWD/plugins/target/ffmpeg/$variant"
done
export PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/standard/lib/pkgconfig"
export FFMPEG_X264_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/x264/lib/pkgconfig"
export FFMPEG_OPUS_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/opus/lib/pkgconfig"
export FFMPEG_VPX_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/vpx/lib/pkgconfig"
```

Homebrew's GPL `ffmpeg@8` can be used for `ffmpeg-x264`, but cannot build the standard plugin.

:::
:::tab[Windows]

On Windows, use vcpkg 2026.07.29 (FFmpeg 8.1.x) and the MSVC build tools.
Use separate vcpkg installation roots for `ffmpeg[zlib]` (standard),
`ffmpeg[x264]`, `ffmpeg[opus]`, and `ffmpeg[vpx]`, with the
`x64-windows-static-md` triplet. Set `VCPKG_ROOT` to the standard root and
`FFMPEG_X264_VCPKG_ROOT`, `FFMPEG_OPUS_VCPKG_ROOT`, and `FFMPEG_VPX_VCPKG_ROOT`
to the optional roots. The repository's setup-ffmpeg CI action shows a complete
Windows setup. Adding x264 to the standard root makes that build GPL and is rejected.

:::
:::tab[Linux]

The script builds static FFmpeg libraries. On Debian and Ubuntu, install the
native dependencies and the editor's system libraries first:

```sh title="Native build dependencies"
sudo apt-get install -y build-essential nasm pkg-config curl xz-utils zlib1g-dev libx264-dev libvpx-dev libopus-dev libclang-dev
sudo apt-get install -y libasound2-dev libxkbcommon-x11-dev libfontconfig-dev
for variant in standard x264 opus vpx; do
  FFMPEG_VARIANT="$variant" scripts/build-ffmpeg-linux.sh "$PWD/plugins/target/ffmpeg/$variant"
done
export PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/standard/lib/pkgconfig"
export FFMPEG_X264_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/x264/lib/pkgconfig"
export FFMPEG_OPUS_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/opus/lib/pkgconfig"
export FFMPEG_VPX_PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/vpx/lib/pkgconfig"
```

:::
::::

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
node scripts/plugins/build.mts plugins/ffmpeg plugins/ffmpeg-x264 plugins/ffmpeg-opus plugins/ffmpeg-vpx plugins/avfoundation plugins/mediafoundation
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
