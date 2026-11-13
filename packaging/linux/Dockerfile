# Headless Celesta exporter for Linux containers and CI, on Ubuntu 26.04.
#
# Build from the repository root:
#
#   docker build -f packaging/linux/Dockerfile -t celesta-exporter .
#
# and export with the current directory mounted at /work:
#
#   docker run --rm -v "$PWD:/work" celesta-exporter --react film.tsx film.mov
#
# The image renders with Mesa's software Vulkan driver (lavapipe), so it needs
# no GPU. See packaging/linux/README.md.

# --- Build stage: FFmpeg 8.1.x, Rust, and the React runtime ----------------
FROM ubuntu:26.04 AS build

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl nasm pkg-config xz-utils zlib1g-dev libvpx-dev libopus-dev libclang-dev \
  && rm -rf /var/lib/apt/lists/*

# Override with --build-arg to pick another 8.1.x release or source mirror.
ARG FFMPEG_VERSION=8.1.3
ARG FFMPEG_URL
ARG FFMPEG_SHA256
COPY scripts/build-ffmpeg-linux.sh /tmp/build-ffmpeg-linux.sh
RUN /tmp/build-ffmpeg-linux.sh /opt/ffmpeg8-standard
ENV PKG_CONFIG_PATH=/opt/ffmpeg8-standard/lib/pkgconfig

# Rust and Node.js come from their official images. Keep Rust in step with the
# toolchain CI uses (.github/workflows/ci.yml). The exporter bundles this same
# Node.js binary.
COPY --from=rust:1.99.1-slim-bookworm /usr/local/rustup /usr/local/rustup
COPY --from=rust:1.99.1-slim-bookworm /usr/local/cargo /usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
COPY --from=node:26-bookworm-slim /usr/local/bin/node /usr/local/bin/node
COPY --from=node:26-bookworm-slim /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm

WORKDIR /celesta
COPY . .
# Use the pnpm version pinned by package.json's `packageManager`.
RUN npm install -g "$(node -p "require('./package.json').packageManager")"

RUN pnpm install --frozen-lockfile \
  && pnpm --dir packages/react run codegen --locked \
  && pnpm run build:runtime
# The bundled plugin folder is /opt/celesta/plugins, where the codec plugin goes.
RUN CELESTA_SYSTEM_PLUGIN_DIR=/opt/celesta/plugins \
  cargo build -p celesta-exporter -p celesta-mcp --release --locked

# Lay the exporter out like the Windows package: a `runtime` directory beside
# the executable holds Node.js and the React runtime, so nothing depends on the
# source tree at run time.
# FFmpeg is linked into the codec plugin's program alone, which goes in the
# bundled plugin folder: the shared build script builds it in the plugins
# workspace and lays out /out/plugins/ffmpeg.
RUN mkdir -p /out/runtime \
  && cp target/release/celesta-exporter target/release/celesta-mcp /out/ \
  && cp /usr/local/bin/node /out/runtime/node \
  && node scripts/stage-react-runtime.mjs /out/runtime/react \
  && node scripts/plugins/build.mts plugins/ffmpeg --distribution --out /out/plugins

# --- Runtime stage ----------------------------------------------------------
FROM ubuntu:26.04

# Only the standard LGPL FFmpeg plugin is bundled; ffmpeg-x264 is a separate
# download. FFmpeg is linked into the plugin's program statically, and
# libopus and libvpx (both BSD licensed) dynamically, from libopus0 and libvpx12.
# mesa-vulkan-drivers provides lavapipe, a CPU implementation of Vulkan. Text
# needs at least one installed font: DejaVu for Latin text and Noto CJK for
# Japanese, Chinese, and Korean. Load other fonts with <Font>.
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates libatomic1 libopus0 libvpx12 libvulkan1 mesa-vulkan-drivers fonts-dejavu-core fonts-noto-cjk \
  && rm -rf /var/lib/apt/lists/*

COPY --from=build /out /opt/celesta

# Record every library but glibc that the standard plugin loads from the image
# (packaging/linux/record-codec-libraries.sh), then check that the program
# opens every encoder, VP9 and Opus included, with what the image installs.
COPY packaging/linux/record-codec-libraries.sh /tmp/record-codec-libraries.sh
RUN program="$(find /opt/celesta/plugins/ffmpeg/bin -type f -name celesta-codec-ffmpeg)" \
  && sh /tmp/record-codec-libraries.sh "$program" /opt/celesta/plugins/ffmpeg/licenses libopus.so libvpx.so \
  && rm /tmp/record-codec-libraries.sh \
  && "$program" --check-license \
  && "$program" --check-build

WORKDIR /work
ENTRYPOINT ["/opt/celesta/celesta-exporter"]
