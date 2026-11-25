# VRM MToon texture transform (#695)

The unlit renderer and the VRM plugin both use `TextureTransform::rows`
to pack the same UV rows, including per-frame offset and scale overrides.
The identity default uses the same arithmetic as before.

The VRM reader distinguishes a resolved image index from an uploaded texture
slot. If it skips a resolved base color image, MToon still shares that image's
transform with its other UV-transformed slots, and expression binds start from
the same transform. A KTX2-only texture with no core source still has no base
transform. Malformed data that already fails model loading or GPU texture
decoding remains an error.

The shared-row unit test covers identity, rotation, scale, offset, and overrides.
The VRM material tests cover a base PNG over the decode limit and a KTX2-only
base texture. For the before/after renders, the same synthetic avatar has a
32,768 × 32,768 base PNG header (4 GiB decoded, refused) and a red/blue emissive
atlas. The base image specifies a half-width UV offset: the colored regions
swap when the other slot starts using that offset.

Run the `celesta-plugin`, `celesta-gpu-renderer`, and `celesta-plugin-vrm` Rust
tests (the last via `plugins/Cargo.toml`), plus rustfmt and Clippy. The VRM
`render-bench` example takes a VRM path and saves a frame with
`RENDER_BENCH_PNG` set. On Linux, select lavapipe via `VK_ICD_FILENAMES` and
set `XDG_RUNTIME_DIR=/tmp` for headless rendering and GPU tests.
