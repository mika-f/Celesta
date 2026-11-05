# Plugin textures: one size check, before decoding (2026-11-05)

Fixes mika-f/Celesta#685 and mika-f/Celesta#676. A content plugin's
textures, for 2D meshes (`ContentModel::Meshes`) or a 3D scene
(`ContentModel::Scene`), now pass one size check before they are decoded
or mip-chained. Before, only the scene path checked the size, and only
after decoding the whole file; the 2D path did not check it at all.

## Specification

- **One rule.** Each texture is 1 to `max_texture_dimension` texels a side.
  The renderer asks wgpu for its default limits, so that is 8192. Any
  other texture is refused as `InvalidPluginContent`, naming the layer:
  `texture {index} is {width}x{height}, outside 1 to {max} texels a side`.
  The scene path already used this message.
- **A file's size comes from its header.** For `TextureSource::Path`, the
  size is read from the file's header first, and an oversized file is
  refused without being decoded. A file within the limit is then decoded
  as before. A file whose header or pixels do not read is still
  `ImageDecode`, naming the file.
- **Sent pixels are checked before their byte count.** For
  `TextureSource::Rgba`, a side of 0 or over the limit is refused with the
  message above. A size within the limit whose bytes do not match is
  refused as before (`a {width}x{height} texture of {n} bytes`).

What it fixes in the 2D path: a `0x0` or `0xN` `Rgba` texture panicked in
`premultiplied_mips` (a subtraction overflow in debug builds, an
out-of-bounds index in release builds). An over-limit texture reached
`create_texture_with_data`, where wgpu's default error handler panicked.
Both stopped the editor's preview worker or the exporter. `<Flag>` with a
9000x100 PNG reached the second one.

## Implementation

- `check_texture_size` (`crates/gpu-renderer/src/mesh.rs`) is the rule.
- `texture_image` takes the texture's index and `max_texture_dimension`
  and applies the rule before it returns pixels. For a path, it opens the
  file once, reads `into_dimensions` through `image::ImageReader` and
  `with_guessed_format`, and checks them. It then rewinds the same handle
  and decodes it with `image::Limits` capped at the limit, and checks the
  decoded size again. A file replaced at the path between the two reads
  is not read, and contents rewritten in place are still held to the
  limit.
- `LoadedMeshes::load` now takes `max_texture_dimension`, as
  `LoadedScene::load` does; `plugin_texture` passes the renderer's.
- `LoadedScene::load` drops its own check after decoding; `texture_image`
  does it now.

## Checks

GPU tests in `crates/gpu-renderer/src/tests/plugin.rs` (2D meshes):

- `refuses_mesh_textures_without_a_texel`: `0x0`, `0x4`, and `4x0` `Rgba`
  textures are refused, naming the layer.
- `refuses_mesh_textures_past_the_gpu_limit`: an `8193x1` texture is
  refused with the 8192 limit.
- `refuses_an_oversized_texture_file_from_its_header`: a 5x1 PNG cut off
  inside its pixel data is refused at a limit of 4 from its header. At the
  default limit, the same file is decoded and fails as `ImageDecode`, so
  the refusal did not decode it.

Before the fix, all three panicked or failed: the subtraction overflow at
`mesh.rs:423`, wgpu's validation panic, and `ImageDecode` from decoding
the file.

In `crates/gpu-renderer/src/tests/scene.rs`:

- `scene_texture_past_the_gpu_limit_is_refused`: an `8193x1` scene
  texture is refused with the same message. The scene check had no test
  before.

`cargo test -p celesta-gpu-renderer --lib` passes (400 tests, 1 ignored).

End to end, with the `flag` sample built by `examples/plugins/build.mjs`
and a release `celesta-exporter`: `<Flag src="./wide.png" />` with a
9000x100 PNG ends the export with `plugin layer root.1 cannot be drawn:
texture 0 is 9000x100, outside 1 to 8192 texels a side`, not a panic. The
same composition with a 900x100 PNG exports.
