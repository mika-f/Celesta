# Rust renderers reject an invalid Video size (2026-11-01)

Issue #562.

## Specification

An `Image` or `Video` is drawn at the size `display_size` derives from its
natural size and its optional `width` and `height`. That size must be finite
and positive. Before this change only `Image` was checked in Rust: a `Video`
layer with a zero, negative, or NaN side reached the CPU and GPU renderers,
which used it as a scale factor and drew nothing (or NaN) without an error.

Now both renderers return an error for such a `Video` layer, as they do for an
`Image`. Valid sizes render exactly as before.

## Implementation

- `crates/renderer/src/image_source.rs`: `display_size` returns
  `Result<(f64, f64), InvalidDisplaySize>`. It checks the size it derives,
  so a side computed from an empty natural size (`0 / 0`) is rejected too.
  `Source::layout` (`Image`) maps the error into its `ImageDecode` error in
  place of its own check; the message is now
  `display size {w}x{h} must be finite and positive`.
- CPU (`crates/renderer/src/layer.rs`): `LayerContent::Video` returns
  `RenderError::InvalidVideoSize { layer, source }`, both with a decoder and
  for the 320 x 180 placeholder drawn without one.
- GPU (`crates/gpu-renderer/src/prepare.rs`): returns
  `GpuRenderError::InvalidVideoSize { layer, source }`, which
  `is_scene_error` counts as a scene error.
- React (`packages/react/src/render.ts`): `<Image>` and `<Video>` share
  `displaySize(props, element)`, which checks `width` and `height` and returns
  the set sides for the layer content. Messages are unchanged
  (`Image width must be finite and positive`, `Video height ...`).

No user-facing behavior changed: the React runtime already rejected these
props, and project-file `Video` content has no `width` or `height`. The user
guide and the Celesta skill are unchanged.

## Checks

- `cargo test -p celesta-renderer`: 224 passed. New cases in
  `sizes_a_video_frame_like_an_image` (with and without a decoder) and
  `image_source::tests::display_size_must_be_finite_and_positive`.
- `cargo test -p celesta-gpu-renderer`: 257 passed, 1 failed, 1 ignored. The
  new cases in `sizes_a_video_frame_like_an_image` pass. The failure,
  `transform_3d::blurs_inside_a_plane_in_output_pixels` (`[254, 0, 0, 254]`
  for `[255, 0, 0, 255]`), draws no video and fails the same way on the
  unchanged base commit on this machine's GPU.
- `cargo clippy -p celesta-renderer -p celesta-gpu-renderer --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `git diff --check`: clean.
- `pnpm run build:runtime`, `pnpm --dir packages/react test`: 298 passed.
