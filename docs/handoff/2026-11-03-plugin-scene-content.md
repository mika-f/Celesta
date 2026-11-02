# Scene content for plugins (2026-11-03)

A content plugin's program can now answer with a 3D scene instead of 2D
meshes: skinned, morphing triangle geometry with materials, seen through a
camera the plugin places. Celesta draws it with depth onto the layer's own
canvas, which composites like any other layer's texture. Opening a model
can also return notices, such as the model's terms of use, which the
exporter and the editor show.

This is the first phase of the design in
`docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md`: the
protocol, the 3D pass with unlit materials, notices, and a sample. MToon
and lit shading, outlines, and the VRM plugin come later. The plugin system
itself (manifest, process, protocol framing, 2D meshes) is
`2026-11-01-plugin-system.md`.

## Content kinds

- A model is opened as one kind, `meshes` (the 2D meshes of
  `2026-11-01-plugin-system.md`, unchanged) or `scene`, and every frame of
  it is of that kind.
- `ContentModel::{Meshes, Scene}` and `ContentFrame::{Meshes, Scene}` carry
  either. The renderer refuses a frame of the other kind
  (`InvalidPluginContent`).
- Only the GPU renderer draws either kind. The CPU reference renderer and
  `@celesta/web` refuse plugin content as before.

## Scenes

The types are in `celesta_plugin` (`crates/plugin/src/scene.rs`). A scene
follows glTF's conventions: right-handed, +Y up, in metres, the camera
looking down its −Z, `f32` 4×4 matrices column-major, texture coordinates
from the texture's top-left (unlike 2D meshes' UVs), triangles
counter-clockwise from the side they face, and colors linear unless a
field says otherwise.

A model (`SceneModel`, sent once per opened asset set):

- `canvas: [width, height]`: the layer's size in pixels. The camera's
  aspect is the canvas's.
- `textures`: an image file (absolute path) or inline RGBA, straight alpha,
  each with a `colorSpace` (`srgb`, decoded to linear when sampled, or
  `linear` for data) and a glTF-like `sampler` (`wrapS`, `wrapT`:
  `repeat`, `mirroredRepeat`, `clampToEdge`; `magFilter`, `minFilter`:
  `nearest`, `linear`; `mipmaps`). Celesta builds the mipmaps, averaging
  sRGB colors in linear light.
- `geometries`: indexed triangle lists in their rest pose, with one entry
  per vertex in each stream: positions, normals, UVs, optional tangents
  (bitangent sign in `w`), optional joints (four `u16`s) and weights (four
  `f32`s), which a skinned geometry has both of, and morph targets (position
  offsets, and normal offsets or none).
- `skins`: each skin's joint count. The plugin keeps the inverse bind
  matrices and sends final skinning matrices each frame.
- `materials`: `alphaMode` (`opaque`, `mask` with `alphaCutoff`, `blend`),
  `doubleSided`, and a `kind` with its parameters. The only kind so far is
  `unlit`: `baseColor` times the optional `baseColorTexture`
  (`{ texture, transform? }`, the transform as `KHR_texture_transform`:
  scaled, rotated, then offset).
- `draws`: what can be drawn, each a `geometry` with a `material`, and a
  `skin` when the geometry is skinned. Draws are what frames place and what
  Celesta sorts.

A frame (`SceneFrame`, per request):

- `camera`: `view` (world to camera) and `projection`, either
  `{ "perspective": { yfov, near, far } }` (radians) or
  `{ "orthographic": { height, near, far } }` (metres).
- `lights`: `directional` lights (`direction` they travel, `color`,
  `intensity`; the first is the main light) and `ambient` as nine spherical
  harmonics coefficients per channel, in Unity's `ShadeSH9` order and scale.
  Checked, but not used by any shading yet.
- `time`: seconds, meant for materials that animate their texture
  coordinates. Checked, but not used yet.
- `draws`: the draws to place, in any order. Each has its `index`,
  `visible`, `world`, one morph weight per target of its geometry, an
  optional `center` (the world-space point a blended draw is sorted by),
  and `overrides` (`baseColor`, `textureOffset`, `textureScale`) replacing
  its material's values for this frame. A draw the frame does not list is
  not drawn.
- `skins`: each skin's skinning matrices, one per joint: the joint's world
  matrix times its inverse bind matrix.

A vertex is morphed first (the weighted target offsets added to its rest
position), then placed at `world × p` when the draw is unskinned, or at
`world × Σ wᵢ·skinᵢ·p` over its joints when it is skinned; a plugin sends
the identity as `world` for a glTF skinned mesh.

## How Celesta draws a scene

`crates/gpu-renderer/src/scene.rs`, `scene_skin.wgsl`, `scene.wgsl`:

- **Checked once and per frame.** On open, the model must have a canvas
  with area, vertex streams of equal length, triangles inside their
  vertices, references to things it has (textures, geometries, materials,
  skins), and skins with every joint their geometries' vertices name;
  buffers and textures must fit the GPU. Each frame must list only draws
  the model has, each once, with as many morph weights as targets, as many
  skinning matrices per skin as joints, finite numbers, and a camera that
  sees something (`0 < near < far`, a `yfov` between 0 and π or a positive
  `height`, an invertible `view`). Anything else is `InvalidPluginContent`.
  A model that fails to load is closed again.
- **Uploaded once.** Geometries and textures go to the GPU when the model
  is opened and are cached with it, by plugin and asset paths, as 2D models
  are.
- **Skinned by a compute pass.** Each frame, `scene_skin.wgsl` morphs and
  skins every visible draw's vertices: `Σ wᵢ·skinᵢ·(p + Σ wₜ·Δpₜ)`; normals
  and tangents follow the matrices' upper 3×3 and are renormalised.
- **Drawn in UniVRM's order.** Draws go by their material's render queue:
  opaque (2000), then mask (2450), then blend (3000). Blended draws go
  farthest first, by the view-space depth of their `center`, or else of
  their geometry's rest-bounds centre moved by `world`; ties go by draw
  index. Hidden draws and draws without triangles are skipped.
- **Depth and anti-aliasing.** The canvas is 4× multisampled linear,
  premultiplied `Rgba16Float` with `Depth32Float` depth, cleared to
  transparent black and to 1. Opaque and mask draws write depth; blend
  draws test it but do not write it. Back faces are culled unless the
  material is double-sided, and a `world` that mirrors flips which winding
  is front.
- **Alpha modes.** Opaque ignores alpha. Mask turns alpha into sample
  coverage around the cutoff, sharpened to about a pixel's width, each
  covered sample opaque. Blend blends straight color over the canvas as
  Unity does (`src·a + dst·(1 − a)`), alpha accumulating as coverage.
- **Unlit.** A material's color is its base color times its texture, at
  coordinates moved by the texture transform and the frame's overrides.
- **Into the layer.** The resolved canvas is converted to the layer
  texture's encoding: un-premultiplied, colors above 1 clipped (no tone
  mapping), encoded to sRGB, premultiplied again as `Rgba8Unorm`. The
  canvas's size follows the render density as 2D meshes' does.

## Notices

- `Opened::notices`: `[{ level: "info" | "warning", message }]`, what a
  plugin wants the user to know about a model it opened, such as its terms
  of use. Either kind of model can carry them.
- The GPU renderer keeps them with the model and lists those of the models
  a frame draws in `GpuRenderer::plugin_notices()` (`PluginNotice`: layer,
  plugin, level, message), once per plugin and message, with the first
  layer it was said for. Its `Display` is
  `<plugin> (layer <id>): <message>`, prefixed `note: ` for `info`.
- The exporter reports each one once per export as a warning
  (`warning: …` on the terminal, and in `--json`'s warnings), for videos
  and stills alike. The editor adds them to the preview's warnings, which
  the Problems tab lists.
- The plugin host keeps the notices of a model's first open: when a
  restarted process opens the model again, that answer's notices are
  dropped.

## Provider interface

`celesta_plugin::ContentProvider` now reads:

```rust
fn open(&mut self, plugin: &str, assets: &BTreeMap<String, PathBuf>)
    -> Result<Opened, PluginError>;
fn frame(&mut self, model: ModelId, time: f64, data: &serde_json::Value)
    -> Result<ContentFrame, PluginError>;
fn close(&mut self, _model: ModelId) {}

pub struct Opened {
    pub model: ModelId,
    pub content: ContentModel, // Meshes(MeshModel) or Scene(SceneModel)
    pub notices: Vec<Notice>,
}
```

`MeshModel`, `MeshFrame`, `SceneModel`, and `SceneFrame` convert into
`ContentModel` and `ContentFrame` with `.into()`. This breaks every Rust
provider written against the old signatures (`open` returning
`(ModelId, MeshModel)`, `frame` returning `MeshFrame`); the samples in this
repository are updated.

## Protocol (still version 1)

| Message | Change |
| --- | --- |
| `hello` answer | Gains `"roles":["content"]`. The host does not read it yet. |
| `opened` | Gains `"kind":"meshes"` or `"scene"` and, when there are any, `"notices":[{"level","message"}]`. An `opened` without `kind` is meshes, as before. |
| `opened`, `kind: "scene"` | Header: `model`, `canvas`, `textures` (`path` or `width`/`height`, with `colorSpace` and `sampler`), `geometries` (`vertices`, `indices`, and `tangents`, `skinned`, `targets`, `targetNormals` when present), `skins` (`joints`), `materials`, `draws`. Payload: for each geometry, its positions (`f32` triples), normals (triples), UVs (pairs), tangents (quads) if any, joints (`u16` quads) then weights (`f32` quads) if skinned, then each target's position offsets and, with `targetNormals`, its normal offsets (zeros for a target that has none); then every geometry's indices (`u32`); then inline textures' pixels. |
| `frame` for a scene | Header: `camera`, `lights`, `time`, `draws` (`index`, `visible`, `world`, `center`, `overrides`, and `weights`, the count of its morph weights), `skins` (`joints`, the count of its matrices). Payload: every skin's matrices (16 `f32`s each), then every draw's morph weights, in order. |

A frame's header does not say its kind: the host decodes it as the kind
its model was opened as, and refuses a model a restarted process reopens as
the other kind. Since an `opened` without `kind` or `notices` still reads as
before, plugin programs built for 2D meshes keep working over the wire;
only Rust code implementing `ContentProvider` has to change.

## The turntable sample

`examples/plugins/turntable` (`celesta-plugin-turntable`, a workspace
member) is a program and a component, built by `build.mjs` like `flag`.
`<Turntable speed>` draws a 640x360 layer: a checkered cube turning about
+Y by `speed` turns a second (default 0.5), a half-transparent blue pane in
front of it (`blend`), and a leaf cut out of a square (`mask`, its stem in
the cube). It takes no assets; its geometry and textures are made in code
(inline RGBA). It returns no notices.

## What landed

| Where | What |
| --- | --- |
| `crates/plugin/src/scene.rs` | The scene types: `SceneModel`, `SceneTexture`, `ColorSpace`, `Sampler`, `Geometry`, `MorphTarget`, `Skin`, `Material` (`AlphaMode`, `MaterialKind::Unlit`), `TextureRef`, `TextureTransform`, `DrawInfo`, `SceneFrame`, `Camera`, `Projection`, `Lights`, `DrawState`, `MaterialOverrides`. |
| `crates/plugin/src/provider.rs` | `ContentModel`, `ContentFrame`, `Opened`, `Notice`, `NoticeLevel`; `ContentProvider::open` returns `Opened` and `frame` returns `ContentFrame`. |
| `crates/plugin/src/protocol.rs` | `ContentKind`, `ROLES`, `encode_opened`/`decode_opened`, `encode_content_frame`/`decode_content_frame`, the scene encoding; `serve_on` answers `hello` with `roles`. A geometry's morph target count is bounded by the payload. |
| `crates/plugin-host/src/host.rs` | `PluginHost` returns `Opened`, remembers each model's kind, decodes its frames as that kind, and refuses a reopen of the other kind. |
| `crates/gpu-renderer/src/scene.rs`, `scene.wgsl`, `scene_skin.wgsl` | Loading and checking scenes, the compute skinning and morphing, the queue-ordered, depth-tested, multisampled unlit draw, and the conversion to the layer texture. |
| `crates/gpu-renderer/src/mesh.rs`, `renderer.rs` | Models of either kind; `PluginNotice` and `GpuRenderer::plugin_notices()`. |
| `crates/exporter/src/render.rs`, `stills.rs` | `report_render_warnings` reports plugin notices with the font warnings. |
| `crates/editor/src/preview.rs` | Plugin notices among the preview's warnings. |
| `examples/plugins/turntable` | The sample above, with `README.md`, `demo.tsx`, and tests. |
| `examples/plugins/flag` | Updated to `Opened` and `ContentFrame`. |

Not done:

- **MToon and lit shading.** `MaterialKind` has only `unlit`; the MToon
  and PBR kinds of the design come later.
- **Lights are not used.** `SceneFrame::lights` is checked and sent, but no
  shading reads it. `SceneFrame::time` is likewise unused.
- **Outlines** (MToon's), which need their own pass.
- **Normals are not transformed by `world`.** Skinning moves normals and
  tangents, but the draw pass applies `world` to positions only; lighting
  will need normals in world space.
- **Per-frame GPU resources are not cached.** Each frame makes its own
  canvases (multisampled color and depth, the resolved color, the layer
  texture), its buffers of skinning matrices, morph weights, and uniforms,
  and their bind groups. Only the model's buffers (its vertex streams and
  what each draw is skinned into) and textures outlive a frame.
- **No Add-menu entry for `<Turntable>`.** Its `plugin.json` lists no
  `components`.
- **The CPU renderer and `@celesta/web`** refuse plugin content, scenes
  included.
- **The out-of-tree Live2D plugin** must update to the new
  `ContentProvider`: `open` returns `Opened` (its `MeshModel` as
  `content`, no notices) and `frame` returns `ContentFrame` (its
  `MeshFrame` with `.into()`). Its process's answers need no change.

## Checks

- `celesta-plugin`'s tests round-trip a scene model (a skinned geometry
  with tangents and morph targets, an unskinned one, file and inline
  textures, mask and blend materials, a texture transform) and a scene
  frame through the wire, checking
  the payload's order; reject short scene payloads and morph targets of
  zero-vertex geometries; serve a scene session; read an `opened` without
  `kind` as meshes; encode a meshes `opened` as before plus `kind`;
  round-trip notices; and answer `hello` with `roles`.
- The GPU renderer's scene tests (`crates/gpu-renderer/src/tests/scene.rs`)
  draw made-up scenes through a fake provider: a valid scene renders; bad
  indices, morph weight counts, short skins, joints out of range, and bad
  cameras are refused; a nearer quad covers a farther one in either order;
  an orthographic camera maps its height to the canvas; skinning and morph
  weights move vertices; an sRGB texture round-trips; back faces are culled
  unless double-sided; hidden draws are not drawn; edges are anti-aliased
  at density 2; mask cuts at its cutoff; blend draws go back to front
  whatever the list order, after opaque ones, and by `center` when given;
  and frames repeat exactly. Unit tests check that projections put `near`
  at depth 0 and `far` at 1, the determinant, and that sRGB mipmaps
  average in linear light.
- `crates/gpu-renderer/src/tests/plugin.rs`: plugin notices are reported
  per frame, once per plugin and message.
- `celesta-plugin-turntable`'s unit tests check its geometry (faces out,
  counter-clockwise), camera, turn, and leaf; its process tests
  (`examples/plugins/turntable/tests/process.rs`) start the built program
  through `PluginHost`, compare its scene answers with the in-process
  provider's for out-of-order times, and draw it with the GPU renderer.
- `celesta-plugin-flag`'s tests pass on the new interface.
