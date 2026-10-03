# Headless Celesta exporter for Linux containers and CI, on Ubuntu 24.04.
#
# Build from the repository root:
#
#   docker build -f packaging/linux/Dockerfile -t celesta-exporter .
#
# and export with the current directory mounted at /work:
#
#   docker run --rm -v "$PWD:/work" celesta-exporter --react film.tsx film.mp4
#
# The image renders with Mesa's software Vulkan driver (lavapipe), so it needs
# no GPU. See packaging/linux/README.md.

# --- Build stage: FFmpeg 8.1.x, Rust, and the React runtime ----------------
FROM ubuntu:24.04 AS build

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl nasm pkg-config xz-utils zlib1g-dev libx264-dev libx265-dev libclang-dev \
  && rm -rf /var/lib/apt/lists/*

# Override with --build-arg to pick another 8.1.x release or source mirror.
ARG FFMPEG_VERSION=8.1.3
ARG FFMPEG_URL
COPY scripts/build-ffmpeg-linux.sh /tmp/build-ffmpeg-linux.sh
RUN /tmp/build-ffmpeg-linux.sh /opt/ffmpeg8
ENV PKG_CONFIG_PATH=/opt/ffmpeg8/lib/pkgconfig

# Rust and Node.js come from their official images. Keep Rust in step with the
# toolchain CI uses (.github/workflows/ci.yml). The exporter bundles this same
# Node.js binary.
COPY --from=rust:1.99.0-slim-bookworm /usr/local/rustup /usr/local/rustup
COPY --from=rust:1.99.0-slim-bookworm /usr/local/cargo /usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
COPY --from=node:24-bookworm-slim /usr/local/bin/node /usr/local/bin/node
COPY --from=node:24-bookworm-slim /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm

WORKDIR /celesta
COPY . .
# Use the pnpm version pinned by package.json's `packageManager`.
RUN npm install -g "$(node -p "require('./package.json').packageManager")"

RUN pnpm --dir packages/react install --frozen-lockfile \
  && pnpm --dir packages/react run codegen --locked \
  && pnpm --dir packages/react run build
RUN cargo build -p celesta-exporter --release --locked

# Lay the exporter out like the Windows package: a `runtime` directory beside
# the executable holds Node.js and the React runtime, so nothing depends on the
# source tree at run time.
RUN mkdir -p /out/runtime \
  && cp target/release/celesta-exporter /out/ \
  && cp /usr/local/bin/node /out/runtime/node \
  && node scripts/stage-react-runtime.mjs /out/runtime/react

# --- Runtime stage ----------------------------------------------------------
FROM ubuntu:24.04

# libx264 and libx265 are linked dynamically; FFmpeg itself is linked into the
# executable.
# mesa-vulkan-drivers provides lavapipe, a CPU implementation of Vulkan. Text
# needs at least one installed font: DejaVu for Latin text and Noto CJK for
# Japanese, Chinese, and Korean. Load other fonts with <Font>.
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates libx264-164 libx265-199 libvulkan1 mesa-vulkan-drivers fonts-dejavu-core fonts-noto-cjk \
  && rm -rf /var/lib/apt/lists/*

COPY --from=build /out /opt/celesta
WORKDIR /work
ENTRYPOINT ["/opt/celesta/celesta-exporter"]
