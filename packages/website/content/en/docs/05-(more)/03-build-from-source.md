To build Celesta, install **Rust 1.89 or later**, your platform’s native build tools, and **FFmpeg 8.1.x development libraries**. Use Node.js 26 and the pnpm version declared in the root package.json to build the React runtime, matching CI.

> [!NOTE]
> **The libraries matter**
> The FFmpeg command-line executable alone is not enough. Celesta's media plugin, `plugins/ffmpeg` in the repository, links against the development libraries; the app and the exporter link no FFmpeg. FFmpeg 9 and later are not supported yet.

## 1. Prepare your platform

::::tabs[Source build platform]{default="macOS"}
:::tab[macOS]

Install the Xcode Command Line Tools and Homebrew. Then install the libraries and point `pkg-config` at the keg-only FFmpeg package in the shell where you build the media plugin (step 4).

```sh title="macOS dependencies"
xcode-select --install
brew install ffmpeg@8 pkg-config
export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"
```

:::
:::tab[Windows]

Install the MSVC C++ build tools and vcpkg. Set `VCPKG_ROOT` to your vcpkg checkout, then install the FFmpeg package below. Run Cargo from a shell configured for the MSVC toolchain.

```sh title="Windows dependencies"
vcpkg install ffmpeg[x264,vpx,opus]:x64-windows-static-md
```

Use a vcpkg release with FFmpeg 8.1.x, such as 2026.07.29. Newer ports may provide unsupported FFmpeg 9.

:::
:::tab[Linux]

Most distributions, including Ubuntu 24.04 LTS, do not package FFmpeg 8.1.x, so build it from source with the repository’s `scripts/build-ffmpeg-linux.sh` after you get the source in step 2. It builds static libraries with `libx264`, `libvpx`, and `libopus` into `/opt/ffmpeg8`, which takes about 5 minutes on 4 cores. Cargo uses `libclang-dev` to generate the FFmpeg bindings. On Debian and Ubuntu, run these from the repository root:

```sh title="Linux dependencies"
sudo apt-get install -y build-essential nasm pkg-config curl xz-utils zlib1g-dev libx264-dev libvpx-dev libopus-dev libclang-dev
sudo apt-get install -y libasound2-dev libxkbcommon-x11-dev libfontconfig-dev
sudo scripts/build-ffmpeg-linux.sh /opt/ffmpeg8
export PKG_CONFIG_PATH=/opt/ffmpeg8/lib/pkgconfig
```

Set `PKG_CONFIG_PATH` in the shell where you build the media plugin (`node scripts/plugins/build.mts plugins/ffmpeg`, step 4); Celesta's own crates link no FFmpeg. FFmpeg is linked into the media plugin’s program, so `LD_LIBRARY_PATH` is not needed. If your distribution does provide FFmpeg 8.1.x, install `pkg-config` and its `libavcodec`, `libavformat`, `libavfilter`, `libavdevice`, `libavutil`, `libswscale`, and `libswresample` development packages instead of running the script. To export without a GPU, see [Export a video](/en/docs/export/).

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
node scripts/plugins/build.mts plugins/ffmpeg plugins/avfoundation plugins/mediafoundation
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
