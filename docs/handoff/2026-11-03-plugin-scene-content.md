# Scene content for plugins (2026-11-03)

A content plugin's program can now answer with a 3D scene instead of 2D
meshes: skinned, morphing triangle geometry with materials, seen through a
camera the plugin places. Celesta draws it with depth onto the layer's own
canvas, which composites like any other layer's texture. Opening a model
can also return notices, such as the model's terms of use, which the
exporter and the editor show.

This is the first phase of the design in
`docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md`: the
protocol, the 3D pass with unlit materials, notices, and a sample.
Plugins' own materials in WGSL, lights, and outlines landed next, in
`2026-11-03-plugin-scene-materials.md`; the VRM plugin after them, in
`2026-11-03-plugin-vrm.md`. The plugin system
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

- `canvas: [width, height]`: the layer's size in pixels, unless a frame
  sets its own. The camera's aspect is the canvas's. Each side must be
  finite, at least 1, and at most 16384 (`MAX_SCENE_CANVAS_SIDE`), the
  same rule as a frame's own canvas.
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
  `doubleSided`, and a `kind` with its parameters: `unlit`, `baseColor`
  times the optional `baseColorTexture` (`{ texture, transform? }`, the
  transform as `KHR_texture_transform`: scaled, rotated, then offset), or
  `custom`, the plugin's own WGSL against Celesta's material interface
  (`2026-11-03-plugin-scene-materials.md`, which also adds `depthWrite`
  and `queueOffset` to `blend`).
- `draws`: what can be drawn, each a `geometry` with a `material`, and a
  `skin` when the geometry is skinned. Draws are what frames place and what
  Celesta sorts.

A frame (`SceneFrame`, per request):

- `canvas` (optional, `[width, height]`): the layer's size in pixels for
  this frame, replacing the model's `canvas`. A model's canvas is fixed
  when it is opened, which sees only the assets, while a layer's
  resolution belongs to the composition: a component passes it in the
  frame's `data`, and the plugin answers it here. The canvas's texel
  size, the projection's aspect, and the layer size Celesta composites
  all follow it. Each side must be finite, at least 1, and at most 16384
  (`MAX_SCENE_CANVAS_SIDE`), as the model's; the 8,388,608-texel cap below
  still applies to the texels drawn. Without it, the frame is drawn on the
  model's canvas. 2D meshes frames have no such field.
- `camera`: `view` (world to camera) and `projection`, either
  `{ "perspective": { yfov, near, far } }` (radians) or
  `{ "orthographic": { height, near, far } }` (metres).
- `lights`: `directional` lights (`direction` they travel, `color`,
  `intensity`; the first is the main light) and `ambient` as nine spherical
  harmonics coefficients per channel, in Unity's `ShadeSH9` order and scale.
  Custom materials read them; unlit ones ignore them. At most 4
  directional lights (`celesta_plugin::scene::MAX_DIRECTIONAL_LIGHTS`;
  `2026-11-03-plugin-scene-materials.md`).
- `time`: seconds, which custom materials read as `frame.time`.
- `draws`: the draws to place, in any order. Each has its `index`,
  `visible`, `world`, one morph weight per target of its geometry, an
  optional `center` (the world-space point a blended draw is sorted by),
  and either `overrides` (`baseColor`, `textureOffset`, `textureScale`)
  replacing an unlit material's values or `params` replacing a custom
  material's parameters, for this frame
  (`2026-11-03-plugin-scene-materials.md`). A draw the frame does not list
  is not drawn.
- `skins`: each skin's skinning matrices, one per joint: the joint's world
  matrix times its inverse bind matrix.

A frame must depend only on the model, `time`, and `data`, never on the
requests before it: a stateful simulation stepped once per request would
step whenever Celesta asks again. Celesta can ask for the same frame again
(a re-render, an export, another process) and for times in any order.
Within one render it asks for a model's frame at a given `time` and `data`
once: measuring a 3D plane's content and drawing each of its bands share
that answer.

A vertex is morphed first (the weighted target offsets added to its rest
position), then placed at `world × p` when the draw is unskinned, or at
`world × Σ wᵢ·skinᵢ·p` over its joints when it is skinned; a plugin sends
the identity as `world` for a glTF skinned mesh.

## How Celesta draws a scene

`crates/gpu-renderer/src/scene.rs`, `scene_skin.wgsl`, `scene.wgsl`:

- **Checked once and per frame.** On open, the model must have a canvas
  whose sides are finite, at least 1, and at most 16384 (a subnormal side
  would make the projection's aspect 0 or infinite), vertex streams of
  equal length, finite vertex streams and morph target offsets (see
  [`2026-11-07-finite-vertex-streams.md`](2026-11-07-finite-vertex-streams.md)),
  triangles inside their
  vertices, references to things it has (textures, geometries, materials,
  skins), and skins with every joint their geometries' vertices name;
  buffers and textures must fit the GPU. The buffers the model has the GPU
  allocate (every geometry's streams, morph targets, and indices, and a
  48-byte skinned vertex per vertex of each draw's geometry) must come to
  at most 1 GiB (`SCENE_BUFFER_BUDGET`, 1 << 30 bytes), counted before
  anything is allocated, so a small model listing many draws of a large
  geometry is refused rather than run out of memory. A frame's own
  canvas, when it has one, is held to the model canvas's rule, checked
  before the canvas is sized, so that a bad one is refused rather than
  clamped. Each frame must list
  only draws the model has, each once, with as many morph weights as targets, as many
  skinning matrices per skin as joints, finite numbers, and a camera that
  sees something (`0 < near < far`, a `yfov` between 0 and π or a positive
  `height`, an invertible `view` that puts the eye within `f32`'s range),
  and its buffers (skinning matrices and
  morph weights, and the per-draw skinning and draw uniforms, one per
  draw, each aligned to the GPU's uniform offset alignment) must each fit the
  largest buffer the GPU allows.
  Anything else is `InvalidPluginContent`. A model that fails to load is
  closed again.
- **Uploaded once.** Geometries and textures go to the GPU when the model
  is opened and are cached with it, by plugin and asset paths, as 2D models
  are.
- **Skinned by a compute pass.** Each frame, `scene_skin.wgsl` morphs and
  skins every visible draw's vertices: `Σ wᵢ·skinᵢ·(p + Σ wₜ·Δpₜ)`; normals
  and tangents follow the matrices' upper 3×3 and are renormalised.
- **Drawn in Unity's render queue order.** Draws go by their material's
  render queue: opaque (2000), then mask (2450), then blend (3000).
  Blended draws go farthest first, by their `center`, or else their
  geometry's rest-bounds centre moved by `world`; ties go by draw index.
  Hidden draws and draws without triangles are skipped. A blend's
  `queueOffset` and `depthWrite`, and the farthest-first order (by
  distance from the eye through a perspective camera, by view-space depth
  through an orthographic one), are in
  `2026-11-03-plugin-scene-materials.md`.
- **Depth and anti-aliasing.** The canvas is linear, premultiplied color
  with `Depth32Float` depth, cleared to transparent black and to 1. Its
  color is `Rgba16Float` where the adapter can render to, blend, and sample
  it, else `Rgba8Unorm`; both color and depth are 4× multisampled where the
  adapter can multisample and resolve that format and multisample
  `Depth32Float` 4×, else drawn at 1 sample straight onto the resolved
  canvas (`scene_target`, chosen once when the renderer is made, as
  `canvas_format` is). No other count is tried: on a WebGPU-conformant
  backend the device checks formats against WebGPU's guaranteed features,
  which promise 4× but no other count, even where the adapter reports
  more. A GL adapter without half-float color buffers takes `Rgba8Unorm`;
  one with neither format refuses scene content with
  `InvalidPluginContent`. Opaque and mask draws write depth; blend
  draws test it but do not write it, unless the material sets
  `depthWrite`. Depth is tested with `LessEqual`, so of two
  exactly coplanar draws the later shows. Back faces are culled unless the
  material is double-sided, and a `world` or a camera `view` that mirrors
  (its upper 3×3's determinant negative) flips which winding is front; both
  mirroring flip it back.
- **Alpha modes.** Opaque ignores alpha. Mask turns alpha into sample
  coverage around the cutoff, sharpened to about a pixel's width, each
  covered sample opaque (at 1 sample, the pixel is covered from the
  cutoff up). Blend blends straight color over the canvas as
  Unity does (`src·a + dst·(1 − a)`), alpha accumulating as coverage.
- **Unlit.** A material's color is its base color times its texture, at
  coordinates moved by the texture transform and the frame's overrides.
- **Into the layer.** The resolved canvas is converted to the layer
  texture's encoding: un-premultiplied, colors above 1 clipped (no tone
  mapping), encoded to sRGB, premultiplied again as `Rgba8Unorm`.
- **Canvas size.** The canvas follows the render density as 2D meshes'
  does, no side longer than the GPU's largest texture, but a scene canvas
  also has at most 8,388,608 texels (`SCENE_CANVAS_TEXELS`): each costs at
  most about 60 bytes (four color and four depth samples, the resolved
  color, the layer texture), so the cap is about 500 MB. A 4K frame
  (3840 × 2160, 8,294,400 texels) fits at 1×; a canvas that would be
  larger, such as a zoomed layer's, is drawn at the density that fits, its
  aspect kept, and so softer. 2D mesh canvases keep the per-side limit
  only. The canvas sized is the frame's own when it sets one, so the
  plugin is asked for the frame before the canvas is sized, also when a
  3D plane only measures the layer's bounds. The render keeps that answer
  (`MeshState::asked`, cleared as each render begins and ends), and the
  plane's bands draw it without asking again.

## Notices

- `Opened::notices`: `[{ level: "info" | "warning", message }]`, what a
  plugin wants the user to know about a model it opened, such as its terms
  of use. Either kind of model can carry them.
- The GPU renderer keeps them with the model and lists those of the models
  a frame draws in `GpuRenderer::plugin_notices()` (`PluginNotice`: layer,
  plugin, level, message), once per plugin and message, with the first
  layer it was said for. Its `Display` is
  `<plugin> (layer <id>): <message>`, prefixed `note: ` for `info`.
- The exporter reports each one once per export, for videos and stills
  alike: a `warning` notice as `ExportProgress::Warning` (`warning: …` on
  the terminal), an `info` one as `ExportProgress::Note`, printed as it
  reads (`note: …`) without the `warning:` label. `--json` and
  `--json-progress` list both among the warnings (a note keeps its
  `note: `), so their schema is unchanged. The editor adds them to the
  preview's warnings, which the Problems tab lists.
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
| `frame` for a scene | Header: `canvas` (only when the frame sets its own), `camera`, `lights`, `time`, `draws` (`index`, `visible`, `world`, `center`, `overrides`, and `weights`, the count of its morph weights), `skins` (`joints`, the count of its matrices). Payload: every skin's matrices (16 `f32`s each), then every draw's morph weights, in order. |

A frame's header does not say its kind: the host decodes it as the kind
its model was opened as, and refuses a model a restarted process reopens as
the other kind. Since an `opened` without `kind` or `notices` still reads as
before, plugin programs built for 2D meshes keep working over the wire;
only Rust code implementing `ContentProvider` has to change.

## The turntable sample

`examples/plugins/turntable` (`celesta-plugin-turntable`, a workspace
member) is a program and a component, built by `scripts/plugins/build.mts` like `flag`.
`<Turntable speed>` draws a 640x360 layer: a checkered cube turning about
+Y by `speed` turns a second (default 0.5), a half-transparent blue pane in
front of it (`blend`), and a leaf cut out of a square (`mask`, its stem in
the cube). It takes no assets; its geometry and textures are made in code
(inline RGBA). It returns no notices. The cube is now toon-shaded by the
sample's own WGSL material, outlined, and lit
(`2026-11-03-plugin-scene-materials.md`).

## What landed

| Where | What |
| --- | --- |
| `crates/plugin/src/scene.rs` | The scene types: `SceneModel`, `SceneTexture`, `ColorSpace`, `Sampler`, `Geometry`, `MorphTarget`, `Skin`, `Material` (`AlphaMode`, `MaterialKind::Unlit`), `TextureRef`, `TextureTransform`, `DrawInfo`, `SceneFrame`, `Camera`, `Projection`, `Lights`, `DrawState`, `MaterialOverrides`. |
| `crates/plugin/src/provider.rs` | `ContentModel`, `ContentFrame`, `Opened`, `Notice`, `NoticeLevel`; `ContentProvider::open` returns `Opened` and `frame` returns `ContentFrame`. |
| `crates/plugin/src/protocol.rs` | `ContentKind`, `ROLES`, `encode_opened`/`decode_opened`, `encode_content_frame`/`decode_content_frame`, the scene encoding; `serve_on` answers `hello` with `roles`. A geometry's morph target count is bounded by the payload. |
| `crates/plugin-host/src/host.rs` | `PluginHost` returns `Opened`, remembers each model's kind, decodes its frames as that kind, and refuses (and closes) a reopen of the other kind. |
| `crates/gpu-renderer/src/scene.rs`, `scene.wgsl`, `scene_skin.wgsl` | Loading and checking scenes, the compute skinning and morphing, the queue-ordered, depth-tested, multisampled unlit draw (on the target `scene_target` picks), and the conversion to the layer texture. |
| `crates/gpu-renderer/src/mesh.rs`, `renderer.rs` | Models of either kind; `PluginNotice` and `GpuRenderer::plugin_notices()`. |
| `crates/exporter/src/render.rs`, `stills.rs` | `report_render_warnings` reports plugin notices with the font warnings: warnings as `ExportProgress::Warning`, info notices as `ExportProgress::Note`. |
| `crates/exporter/src/control.rs`, `progress.rs`, `report.rs` | `ExportProgress::Note`, printed without the `warning:` label and listed among the JSON warnings. |
| `crates/editor/src/preview.rs` | Plugin notices among the preview's warnings. |
| `examples/plugins/turntable` | The sample above, with `README.md`, `demo.tsx`, and tests. |
| `examples/plugins/flag` | Updated to `Opened` and `ContentFrame`. |

Not done:

- **Lit shading.** Done: a plugin brings its own materials in WGSL,
  lit as it likes (`2026-11-03-plugin-scene-materials.md`); glTF PBR is
  still to come, as a plugin's material.
- **Lights are not used.** Done: custom materials read
  `SceneFrame::lights` and `SceneFrame::time`
  (`2026-11-03-plugin-scene-materials.md`).
- **Outlines.** Done: a custom material can have an outline pass
  (`2026-11-03-plugin-scene-materials.md`).
- **Normals are not transformed by `world`.** Done: the draw pass takes
  normals to world space by `world`'s signed cofactor matrix
  (`2026-11-03-plugin-scene-materials.md`).
- **Per-frame GPU resources are not cached.** Done for the canvases
  (multisampled color and depth, or 1-sample depth, the resolved color),
  the buffers of
  skinning matrices, morph weights, and uniforms, and their bind groups,
  which `SceneFrameResources` keeps
  (`2026-11-03-plugin-scene-materials.md`). The layer texture too, since
  (`2026-11-08-plugin-performance.md`).
- **No Add-menu entry for `<Turntable>`.** Its `plugin.json` lists no
  `components`.
- **The CPU renderer and `@celesta/web`** refuse plugin content, scenes
  included.
- **The out-of-tree Live2D plugin** must update to the new
  `ContentProvider`: `open` returns `Opened` (its `MeshModel` as
  `content`, no notices) and `frame` returns `ContentFrame` (its
  `MeshFrame` with `.into()`). Its process's answers need no change.
- **Blend draws sort by view-space z,** not by distance to the camera as
  Unity does. Done: through a perspective camera they now sort by distance
  (`2026-11-03-plugin-scene-materials.md`). The VRM plugin should still send
  `center` for skinned draws, whose rest bounds do not follow the pose.
- **Joint indices in zero-weight slots count** toward the check that a
  skin has every joint its geometry names. The VRM plugin should zero
  them.
- **glTF primitives sharing accessors** become separate geometries, each a
  full copy of the shared streams. The plugin should compact each
  primitive to the vertices it references.
- **The default 128 MiB storage-binding limit**
  (`max_storage_buffer_binding_size`) may refuse morph-heavy high-poly
  geometry: all of a geometry's morph targets are one buffer.
- **Straight-alpha mipmaps fringe cut-outs at distance:** averaging
  straight colors with transparent texels pulls in their (often black or
  arbitrary) colors. Done: sRGB textures only cut-out and blended
  materials show weight color by alpha and fill transparent texels
  (`2026-11-08-plugin-mipmap-alpha.md`).
- **No test for the reopen of the other kind.** Closing the reopened model
  needs a plugin that changes kind across a restart, which no test plugin
  does.

## Checks

- `celesta-plugin`'s tests round-trip a scene model (a skinned geometry
  with tangents and morph targets, an unskinned one, file and inline
  textures, mask and blend materials, a texture transform) and a scene
  frame through the wire, checking
  the payload's order; round-trip a scene frame with and without its own
  `canvas`, writing no `canvas` key without one; reject short scene payloads and morph targets of
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
  frames repeat exactly; a layer zoomed ×64 renders on a canvas within
  `SCENE_CANVAS_TEXELS`; a frame's own canvas (32×64 on a 64×64 model)
  sizes the layer and the projection's aspect, a unit square staying
  square, and measures, as a 3D plane's content, as it draws; a render
  asks for a scene frame in a 3D plane once, and the next render asks
  again; a frame or model canvas of `[0, 64]`, `[NaN, 1]`, `[1e-45, 64]`,
  `[0.5, 64]`, `[20000, 10]`, or `[1e30, 64]` is refused naming `canvas`;
  a model over the 1 GiB buffer budget is refused;
  the texture transform offsets, scales, and rotates (a quarter turn
  taking `(u, v)` to `(−v, u)`, as `KHR_texture_transform`'s column-major
  `mat3`), scaled before it is rotated; and the frame's overrides replace
  the material's offset, scale, and base color. Drawn on the usual target
  and on each fallback in place of the one the GPU chooses (`Rgba8Unorm`
  at 4 and at 1 sample, `Rgba16Float` at 1), a square, a plugin's
  material, unlit and custom masks at their cutoff, half red over half
  blue, blend over opaque, and an outline come out the same; an edge pixel
  and a mask's edge pixel are partly covered at 4 samples and covered or
  not at 1; with no target, scene content is refused. Unit tests check
  which target `scene_target` picks when `Rgba16Float` cannot be
  multisampled or resolved, rendered to, blended, or sampled, when depth
  cannot be multisampled, and that neither format renderable gives none;
  and that
  projections put `near` at depth 0 and `far` at 1, the determinant, that
  sRGB mipmaps average in linear light, the capped canvas size (4K at 1×
  untouched, aspect kept), the canvas rule's edges (1 and 16384 drawn;
  `1e-45`, 0.5, 16384.5, `1e30`, infinity, and NaN refused, for the model
  and a frame alike), the buffer byte count, and the per-draw uniform
  check (a GPU test would need over 500,000 queued draws).
- The exporter's tests list a note among the JSON warnings and
  `--json-progress`'s `warning` events, and keep it out of the progress
  display's warnings.
- `crates/gpu-renderer/src/tests/plugin.rs`: plugin notices are reported
  per frame, once per plugin and message.
- `celesta-plugin-turntable`'s unit tests check its geometry (faces out,
  counter-clockwise), camera, turn, and leaf; its process tests
  (`examples/plugins/turntable/tests/process.rs`) start the built program
  through `PluginHost`, compare its scene answers with the in-process
  provider's for out-of-order times, and draw it with the GPU renderer.
- `celesta-plugin-flag`'s tests pass on the new interface.
