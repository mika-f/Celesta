# Plugin content: finite vertex streams (2026-11-07)

Fixes mika-f/Celesta#674. `invalid_scene` checked a scene model's vertex
streams for length but not for finiteness, so a plugin could send a NaN or
an infinity in a position, normal, texture coordinate, tangent, joint
weight, or morph target offset, and it reached the GPU. A NaN clip
position drops its triangle, so the damage stayed in the layer, but
nothing told the plugin's author. The 2D mesh check, `invalid_model`, had
the same gap for its texture coordinates.

## Specification

- **A scene model is refused when a vertex stream is not finite.** On
  open, every number of each geometry's `positions`, `normals`, `uvs`,
  `tangents` (when it has them), and `weights` (when it is skinned), and
  of each morph target's `positions` and `normals` offsets, must be
  finite. Otherwise the model fails to load with
  `InvalidPluginContent`, naming the geometry, the stream, and the first
  vertex with such a number:
  `geometry 0's normals are not finite at vertex 12`,
  `geometry 0's target 1's positions are not finite at vertex 3`.
- **A 2D mesh model is refused the same way** when a mesh's `uvs` are not
  finite: `mesh 0's uvs are not finite at vertex 1`.
- **Once at open, not per frame.** The scan is O(n) in the model's
  vertices, after the stream length checks (so the vertex it names exists)
  and before anything is uploaded. The plugin failure cache
  ([`2026-11-07-plugin-failure-cache.md`](2026-11-07-plugin-failure-cache.md))
  keeps the refusal, so later frames do not scan again.
- Joint indices are integers and need no check. Per-frame numbers of a
  scene were already checked by `invalid_scene_frame`.

## Implementation

- `scene::non_finite_vertex` (`crates/gpu-renderer/src/scene.rs`) returns
  the first vertex of a `[[f32; N]]` stream with a NaN or infinite
  number. `invalid_scene` runs it over each geometry's streams and then
  its morph targets, in that order; `invalid_model`
  (`crates/gpu-renderer/src/mesh.rs`) over each mesh's `uvs`.
- The `celesta-plugin` docs for `Geometry` and `MeshInfo::uvs` state that
  the numbers are finite.
- The VRM plugin already refuses non-finite glTF streams and falls back to
  a default tangent where MikkTSpace leaves none, so it sends nothing this
  check refuses.

## Not covered

A 2D frame's per-frame numbers (`MeshState::positions`, `opacity`,
`multiply`, `screen`) are still not checked for finiteness. They change
every frame, so a check there would run per frame; the issue scoped this
to the model.

## Checks

- New tests, each failing before the fix:
  `scene::tests::vertex_streams_and_morph_offsets_must_be_finite` (one
  case per stream: positions, normals, uvs, tangents, weights, and a
  morph target's positions and normals, with NaN, +∞, and −∞) and
  `mesh::tests::mesh_uvs_must_be_finite`.
- `cargo test -p celesta-gpu-renderer` (including the GPU tests),
  `cargo clippy -p celesta-gpu-renderer --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`, `git diff --check`.
- `cargo test -p celesta-plugin -p celesta-plugin-flag -p celesta-plugin-turntable`
  and `cargo test --manifest-path plugins/Cargo.toml -p celesta-plugin-vrm --locked`.
