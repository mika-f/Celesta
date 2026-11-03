# A scene frame's own canvas: one request, one rule (2026-11-03)

Builds on the frame canvas of `2026-11-03-plugin-scene-content.md`
(`SceneFrame::canvas`, which replaces the model's canvas for one frame).
The renderer asks for a scene frame before it sizes the canvas, also when
a 3D plane only measures the layer. This note covers how often a render
asks a provider for a frame, the rule every scene canvas follows, and
their tests.

## Specification

- **One request a render.** Within one render, Celesta asks a model's
  provider for its frame at a given `time` and `data` once. Measuring a 3D
  plane's content, drawing each of its bands, and nested planes' measures
  all draw that one answer. Two layers of one model ask once each when
  their `time` or `data` differ, and once between them when both are the
  same. A later render, an export, or another process asks again.
- **The frame contract.** A frame must depend only on the model, `time`,
  and `data`, never on the requests before it: a simulation stepped once
  per request would step whenever Celesta asks again. It is stated in
  `ContentProvider::frame`'s doc, after the protocol table of
  `2026-11-01-plugin-system.md`, in the scene frame section of
  `2026-11-03-plugin-scene-content.md`, the plugin guide (English and
  Japanese), and the Celesta skill's plugin reference.
- **One canvas rule.** A scene's canvas, the model's or a frame's own, has
  sides that are finite, at least 1, and at most 16384
  (`celesta_plugin::scene::MAX_SCENE_CANVAS_SIDE`). At least 1 rather than
  positive: a subnormal side such as `1e-45` makes the aspect
  (`width / height`) 0 or infinite, which the projection divides by.
  The rule narrows the model's canvas, which Celesta had checked only to
  be finite and positive: a model canvas under 1 or over 16384 is now
  refused (`InvalidPluginContent`, naming `the model's canvas`). No model
  in the repository has one.

## Implementation

- `MeshState::asked` (`crates/gpu-renderer/src/mesh.rs`) holds the frames
  the provider answered in this render, matched by the model's key
  (plugin and asset paths), the time's bits, and `data`. `plugin_texture`
  draws a match without asking; it asks and records otherwise. `prepare`
  clears the list as a render begins and once the layer walk ends, so no
  frame outlives its render. This covers 2D meshes frames too, so a
  multi-band plane of a meshes layer also asks once.
- `invalid_canvas` (`crates/gpu-renderer/src/scene.rs`) is the rule;
  `invalid_scene` applies it to the model's canvas and
  `invalid_frame_canvas` to a frame's own.
- `invalid_scene_frame` checks a frame's canvas again, although
  `plugin_texture` already did, so that it stays the complete check of a
  frame.
- When a layer's canvas changes, the size of the model's last frame is
  marked before the next frame is asked for, so the old size is kept
  through the frame after the change and dropped as the one after that
  begins: up to one capped canvas (about 500 MB) for one frame, within
  `SCENE_TARGET_TEXELS` (`2026-11-03-plugin-scene-materials.md`).

## Checks

GPU tests in `crates/gpu-renderer/src/tests/scene.rs`:

- `a_render_asks_for_a_planes_scene_frame_once`: a scene layer in
  perspective (one band) is asked for its frame once a render, and once
  more by the next render. With the list never matching, it is asked
  twice a render.
- `a_render_asks_again_for_other_data`, `a_render_asks_again_for_another_time`,
  and `a_render_asks_once_for_the_same_time_and_data`: two layers of one
  model side by side in one render, the provider's square following the
  time and `data`. Other data, or another time, asks twice and draws each
  layer's own colour; the same time and data asks once. Matching without
  `data`, without the time, or never, fails the matching test.
- `measured_scene_layer_follows_the_frame_canvas`: with a 32x64 frame
  canvas on a 64² model, the bounds a 3D plane measures equal the bounds
  drawn and are 64 units higher than wide at ×2. Measuring at the model's
  canvas instead fails it: 132x132 measured against 68x132 drawn.
- `bad_frame_canvas_is_refused` and `bad_model_canvas_is_refused`:
  `[0, 64]`, `[NaN, 1]`, `[1e-45, 64]`, `[0.5, 64]`, `[20000, 10]`, and
  `[1e30, 64]` are refused for either.
- `a_frame_canvas_survives_a_frame_that_fails`: after a frame switches to
  its own canvas, a frame refused before it is sized (a `[0, 64]` canvas)
  keeps the frame canvas's targets for the next frame: no new allocations,
  and only 32x64 kept. If the switch did not update the size marked before
  asking, it would allocate again.
- `a_canvas_change_keeps_the_old_size_one_frame_more`: 64², then 32x64
  three times, keeps `[64²]`, `[32x64, 64²]`, `[32x64, 64²]`, `[32x64]`.

A unit test in `crates/gpu-renderer/src/scene.rs`
(`canvas_sides_are_at_least_1_and_at_most_the_longest`) checks the rule's
edges for the model and a frame: 1, 640.5, and 16384 pass; `1e-45`, 0.5,
0, −1, 16384.5, `1e30`, infinity, and NaN are refused.

No pixels change: the frame canvas's drawing is pinned by
`frame_canvas_overrides_the_model_canvas`, so no before/after stills were
taken for these changes.

Commands: `env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-gpu-renderer`,
`cargo test -p celesta-plugin`,
`env CELESTA_REQUIRE_GPU=1 cargo test -p celesta-plugin-turntable`,
`cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`.
