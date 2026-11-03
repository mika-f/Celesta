# The VRM plugin (2026-11-03)

`plugins/vrm` is a content plugin that draws VRM 1.0 avatars (`.vrm`) as
scene content: the avatar in its humanoid rest pose, MToon and unlit
materials, expressions with UniVRM's override rules, eyes following a
gaze target, a camera framed from the humanoid, the plugin's reference
light, and the model's usage terms as notices. `<Vrm>` puts an avatar in a
composition, and the editor's Add menu offers it with a `.vrm` file.

This is the third phase of the design in
`docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md`, its
first part: everything but motion. The scene protocol is
`2026-11-03-plugin-scene-content.md`; materials in WGSL, lights, render
queues, and outlines are `2026-11-03-plugin-scene-materials.md`; a frame's
own canvas is `2026-11-03-plugin-frame-canvas-checks.md`. Humanoid motion
(`.vrma`), constraints, and spring bones come next
(`docs/superpowers/plans/2026-11-03-plugin-vrm-motion.md`).

Wherever the VRM specification leaves room, UniVRM decides. UniVRM works
in Unity's axes, glTF's with X negated; every rule below is written in
glTF's: right-handed, +Y up, metres, the avatar facing +Z with its left at
+X, texture coordinates from the top-left, colours linear unless stated.
Files ported from UniVRM carry its MIT notice and name their source.

## The plugin, its build, and installing it

- A Cargo workspace of its own (`plugins/vrm/Cargo.toml` has an empty
  `[workspace]` and its own `Cargo.lock`), which the root workspace
  excludes: `cargo build --workspace` at the root never builds it. It
  depends on Celesta's `crates/plugin` by path and on nothing else of
  Celesta's; the GPU renderer, the plugin host, and the composition crate
  are dev-dependencies for its tests. Package and binary
  `celesta-plugin-vrm`, library `celesta_plugin_vrm`, plugin id `vrm`.
- The program serves `VrmProvider` over standard input and output
  (`celesta_plugin::serve`), as every plugin program does.
- `plugin.json` is the manifest's source: one component, `Vrm` ("VRM
  avatar"), taking a model file (below). `js/src/index.ts` is `<Vrm>`, a
  pnpm workspace package (`celesta-plugin-vrm-js`, private) for its tests.
- `node plugins/vrm/build.mjs` builds what Celesta loads into
  `plugins/vrm/dist/vrm`: `celesta-plugin.json`, the release program in
  `bin/<os>-<arch>/`, and `js/index.js` with `js/index.d.ts` (the module
  bundled with `react` and `@celesta/*` external, its declarations checked
  against the runtime's project types). It runs Cargo `--locked` from the
  plugin's folder and finds the executable through `cargo metadata`'s
  target directory. `--out`, `--target`, and `--skip-js` work as for
  `examples/plugins/build.mjs`; a `--target` triple's architecture is named
  as the host names it (`i686` is `x86`, `armv7` is `arm`).
- To install it, copy `plugins/vrm/dist/vrm` into a plugin folder, or name
  `plugins/vrm/dist` with `--plugin-dir` or `CELESTA_PLUGIN_PATH`.
- `plugins/vrm/demo.tsx` shows Seed-san (VRM Public License 1.0, from
  vrm-c/vrm-specification's `samples/Seed-san/vrm`, downloaded into the
  git-ignored `plugins/vrm/assets/`) from the bust up, mouthing the vowels.

## Loading

`VrmModel::load` reads the whole file and builds the scene once.

- A `.vrm` is binary glTF; a glTF JSON file whose buffers are `data:` URIs
  opens too. A buffer or an image that names another file is refused:
  `buffer {i} is an external file; a .vrm keeps everything in itself`.
- **Sizes.** A file past 1 GiB (`FILE_LIMIT`) is refused by its size
  before it is read, and while it is read when its size says nothing (a
  device): `the file is larger than the 1024 MiB the VRM plugin reads`.
  JSON past 64 MiB (`JSON_LIMIT`; a VRM's is a few MB) is refused before
  it is parsed: `the glTF JSON is {n} bytes, more than the 64 MiB the VRM
  plugin reads`. Parsed JSON takes many times its size, and the budgets
  count only what the buffers decode to.
- **Parsing.** The JSON is never held as one `serde_json::Value`: the
  root's `VRMC_vrm` and `VRM` are read first with the rest skipped, then
  gltf-json reads its typed root. Extensions `vrm-spec` has no types for
  stay in gltf-json's `extensions().others` of the object holding them
  (each material's `VRMC_materials_mtoon`, and the nodes' constraints for
  the motion plan).
- Refusals, in this order, the first three before glTF's own validation
  (so a VRM 0.x file always gets the conversion hint) and the humanoid's
  after it:
  - the root has `VRM` but no `VRMC_vrm`: `this is a VRM 0.x avatar, which
    the VRM plugin does not open yet; convert it to VRM 1.0 with UniVRM or
    the VRM Add-on for Blender`;
  - no `VRMC_vrm`: `this glTF file has no VRMC_vrm extension: it is not a
    VRM 1.0 avatar`;
  - `specVersion` not `1.0` nor starting `1.0-`: `VRMC_vrm specVersion {v}
    is not 1.0`;
  - a bone VRM 1.0 requires missing (`hips spine head`, both arms' upper,
    lower, and hand, both legs' upper, lower, and foot): `the humanoid has
    no {bone} bone, which VRM 1.0 requires`; a bone past the nodes, or two
    bones on one node, named.
- Model space is the default scene's root (`scene`, else scene 0). Nodes
  outside it stay in the tree but are not drawn. A node's `matrix` is
  decomposed into translation, rotation, and scale. Two parents, a cycle,
  or a transform past `f32` refuse the model.
- `VRMC_node_constraint` and `VRMC_springBone` are read by no one yet and
  refuse nothing.

## Geometry

- **Compaction.** A primitive keeps only the vertices its indices use, in
  their original order, and every stream and morph target is compacted the
  same way: UniVRM and VRoid write every primitive of a mesh over one set
  of vertex accessors, which Celesta would otherwise upload once per
  primitive. A primitive without indices draws every vertex.
- **Modes.** Triangle strips and fans become lists by glTF's winding
  rules. Points and lines are not drawn: a `warning`, once per mesh a
  scene node draws, `mesh "{name}" primitive {p} is LINES, which is not
  drawn`. A strip or fan too short for a triangle is skipped silently.
- **Streams.** Position, normal, `TEXCOORD_0`, tangents, and
  `JOINTS_0`/`WEIGHTS_0`, the streams the protocol has (the skinning pass
  is at its 8 storage bindings). Without `NORMAL`, area-weighted vertex
  normals over the compacted triangles (Unity's `RecalculateNormals` on an
  unwelded mesh); without `TEXCOORD_0`, zeros. Weights are read as `f32`
  and normalised to sum 1 where the sum is positive.
- **Joint slots.** A slot of weight 0 gets joint 0, so slots naming joints
  the skin lacks (UniVRM and VRoid exports have them) cost nothing. A slot
  of weight above 0 naming a joint the skin lacks refuses the model:
  `mesh "{name}" primitive {p} names joint {j} of skin {s}, which has {n}`.
- **Tangents** are MikkTSpace on the compacted triangles wherever the
  material has a `normalTexture`, never read from the file (UniVRM ignores
  them). A non-finite result, or a primitive whose UVs are all one point,
  gets `[1, 0, 0, 1]`.
- **Morph targets** are dense position deltas, and normal deltas where the
  target has `NORMAL`, sparse accessors included; tangent deltas are not
  read.
- **Hostile files.** Every accessor's type, count, stride, and byte range
  is checked before `gltf`'s reader touches it; non-finite vertex data and
  indices past the vertices are refused. Budgets: the accessors decode at
  most 1 GiB, each counted once however many primitives read it
  (`READ_BUDGET`); the model's geometry, draws, and skins take at most
  1 GiB, Celesta's scene buffer budget (`GEOMETRY_BUDGET`); a primitive has
  at most 65,536 morph targets.

## Materials and textures

- **Kinds.** A material with `VRMC_materials_mtoon` is MToon (next
  section), drawn as a custom material on the plugin's MToon shader, the
  model's shader 0; the shader is sent only when some material is MToon.
  MToon wins over a `KHR_materials_unlit` beside it. `KHR_materials_unlit`
  is Celesta's unlit material (base colour times its texture). Any other
  material is glTF PBR, **drawn as unlit base colour** for now, its
  emission and metal-roughness dropped; one `info` notice names the ones
  the draws use: `{n} glTF PBR material(s) are drawn as unlit base colour:
  {names}`. A primitive without a material uses glTF's default material
  (white, opaque, one-sided), appended once and drawn unlit.
- Materials keep glTF's indices, unused ones too, so expression binds name
  them directly. `alphaMode`, `alphaCutoff` (default 0.5), and
  `doubleSided` are the file's.
- **Texture coordinates.** Every slot samples `TEXCOORD_0`, whatever its
  `texCoord`, as UniVRM does. Only the base colour texture's
  `KHR_texture_transform` is kept (offset, rotation, and scale); it moves
  every MToon texture but the matcap and the outline width, as UniVRM's
  single `_MainTex_ST` does.
- **Colour spaces.** sRGB for the base colour, shade multiply, emissive,
  matcap, and rim multiply textures; linear for the normal, shading shift,
  outline width, and UV animation mask textures.
- **One scene texture per (image, colour space, sampler),** in first-use
  order over materials, then slots in MToon's field order.
- **Samplers.** Wrap 33071 clamps, 33648 mirrors, anything else repeats;
  `magFilter` 9728 is nearest, anything else linear; `minFilter` 9728,
  9984, and 9986 are nearest, anything else linear; mipmaps unless
  `minFilter` is 9728 or 9729. No sampler is Celesta's default.
- **Images are files, not pixels.** Each PNG, JPEG (also written
  `image/jpg`), or WebP (`EXT_texture_webp`) image a slot uses is written
  once to the texture cache as `<sha256>.<png|jpg|webp>` and sent as a
  path; Celesta decodes it. This keeps the `open` answer to a few MB. A
  file is written to a new temporary name and renamed into place, and only
  when no file there holds the same bytes. The cache is
  `$CELESTA_VRM_TEXTURE_CACHE`, else `celesta-plugin-vrm` in the user's
  cache directory (`%LOCALAPPDATA%`, `~/Library/Caches`, or
  `$XDG_CACHE_HOME` / `~/.cache`), else a directory named for the user in
  the temporary directory. It is made private (0700 on Unix); a cache that
  is a link, not a directory, another user's, or writable by others is
  refused, naming the variable that moves it. Nothing ever cleans it.
- **Budgets.** Each image is sized from its header alone. The scene
  textures may decode to at most 1 GiB at 4 bytes a texel
  (`TEXTURE_BUDGET`), an image two scene textures sample counted twice,
  since Celesta decodes each scene texture's file on its own: `the model's
  textures decode to {m} MiB, more than 1024 MiB`. An image that decodes
  past the 512 MiB Celesta decodes one image to, in its own pixel format,
  leaves its slots empty with a `warning`.
- **Images the plugin cannot read.** A texture whose only image is KTX2
  (`KHR_texture_basisu`) or DDS (`MSFT_texture_dds`), or an image of
  another MIME type, leaves the slot empty with a `warning`: `texture {t}
  is image/ktx2, which the VRM plugin cannot read; material "{name}" draws
  without it`. Such a texture with a `source` beside the extension uses
  the source. Bytes that are not the image they claim to be refuse the
  model.
- Every number is checked finite as it is read: `material "{name}" has a
  shadeColorFactor that is not finite: …` (or `material {i}` without a
  name).

## MToon

MToon is the plugin's own material: `src/mtoon.wgsl`, UniVRM's MToon10 as
it draws in Unity's built-in render pipeline, written against Celesta's
material interface, and `src/mtoon.rs`, which packs its parameters. Celesta
knows nothing of MToon; what it draws for every material (the alpha
modes, the render queues, the outline pass and its bias, `LessEqual`) is
in `2026-11-03-plugin-scene-materials.md`.

### Fields and defaults

`Mtoon` holds every field the file has, on top of UniVRM's importer
defaults (`Mtoon::default()`):

| Field | Default | What it does |
| --- | --- | --- |
| `baseColorFactor` (glTF) | `[1, 1, 1, 1]` | Linear RGBA, times `baseColorTexture`: the lit colour and the alpha. |
| `baseColorTexture` (glTF) | none | Its `KHR_texture_transform` moves the other textures too. |
| `normalTexture` (glTF) | none | A tangent-space normal map, times its `scale` (default 1). |
| `emissiveFactor` (glTF) | `[0, 0, 0]` | Linear RGB, times `emissiveTexture`. `KHR_materials_emissive_strength` is multiplied in, else `VRMC_materials_hdr_emissiveMultiplier`'s `emissiveMultiplier` (default 1; not a finite number refuses the model), as UniGLTF's `ImportLinearEmissiveFactor` does. |
| `emissiveTexture` (glTF) | none | |
| `shadeColorFactor` | `[1, 1, 1]` | The shaded side's colour, times `shadeMultiplyTexture`. |
| `shadeMultiplyTexture` | none | |
| `shadingShiftFactor` | `-0.05` | Shifts where light turns to shade. |
| `shadingShiftTexture` | none | Its red times `scale` (default 1) is added to the shift. |
| `shadingToonyFactor` | `0.95` | How hard the edge is, 0 smooth to 1 a step; above 1 the ramp turns around. |
| `giEqualizationFactor` | `0.9` | How far the ambient light is flattened. |
| `matcapFactor` | `[0, 0, 0]` | Linear RGB, times `matcapTexture`. |
| `matcapTexture` | none | |
| `parametricRimColorFactor` | `[0, 0, 0]` | The rim's colour. |
| `rimMultiplyTexture` | none | Scales the rim and the matcap. |
| `rimLightingMixFactor` | `1` | How far the rim follows the scene's light. |
| `parametricRimFresnelPowerFactor` | `5` | The rim's falloff. |
| `parametricRimLiftFactor` | `0` | Widens the rim. |
| `outlineWidthMode` | `none` | `none`, `worldCoordinates`, or `screenCoordinates`. |
| `outlineWidthFactor` | `0` | Metres, or a share of the canvas's height. |
| `outlineWidthMultiplyTexture` | none | Its green scales the width. |
| `outlineColorFactor` | `[0, 0, 0]` | The outline's colour. |
| `outlineLightingMixFactor` | `1` | How far the outline is shaded like the surface. |
| `uvAnimationMaskTexture` | none | Its blue scales the animation. |
| `uvAnimationScrollXSpeedFactor` | `0` | Texture widths a second; the picture moves left. |
| `uvAnimationScrollYSpeedFactor` | `0` | Texture heights a second; the sampled point moves down the texture, so the picture moves up. |
| `uvAnimationRotationSpeedFactor` | `0` | Radians a second; the picture turns clockwise. |

`transparentWithZWrite` and `renderQueueOffsetNumber` are not `Mtoon`
fields: they become the material's `AlphaMode::Blend { depth_write,
queue_offset }`, which Celesta draws for every material. As UniVRM's
`MToonValidator` does, only a blended material reads them; opaque and
mask drop both.

- **Texture coordinates.** Every texture but the matcap and the outline
  width is sampled at the base colour texture's coordinates: through its
  transform (and a frame's texture binds), then the UV animation. The
  outline width texture is sampled at the geometry's raw coordinates; the
  matcap at its own; the mask at the transformed coordinates before the
  animation.
- **Channels.** The shading shift texture is read from its red, the
  outline width texture from its green, the mask from its blue; the
  others' RGB is used.
- **The shader** (`mtoon::shader()`, named `MToon`) has nine slots, named
  as `VRMC_materials_mtoon` names its textures: `baseColorTexture`,
  `shadeMultiplyTexture`, `shadingShiftTexture`, `normalTexture`,
  `emissiveTexture`, `matcapTexture`, `rimMultiplyTexture`,
  `outlineWidthMultiplyTexture`, `uvAnimationMaskTexture`. An empty slot
  samples Unity's default texel for the property, which leaves each term
  as UniVRM's keyword gates do without the texture: black for the shading
  shift and the matcap, white for the rest. An empty normal slot is
  sampled but has no effect: the shader keeps the geometric normal.
- **The custom material** (`mtoon::custom_material`): parameters packed at
  time 0 without binds; the nine slots; an outline pass
  (`OutlinePass::default()`: front faces culled, Unity's `Offset 1, 1`)
  unless the width mode is `none`, whatever the width, 0 included, as
  UniVRM draws it; `requiresTangents` with a normal texture.
- **Checked before packing** (`mtoon::invalid`): each slot's texture is
  one of the model's (`material "Body": shadeMultiplyTexture uses texture
  {t}, of {n}`), and every number is finite. Celesta sees only bytes, so
  the plugin checks; Celesta still checks the slots' range itself.

### How it shades

Per fragment, in world space, from Celesta's `Fragment`:

- **The normal** `N` is the fragment's, turned to face the viewer on a
  back face (which only a double-sided material draws), then tilted by the
  normal texture: decoded as Unity's `UnpackNormalWithScale` reads its
  desktop layout (red and green times `scale`, blue unread), in the
  fragment's tangent space, the bitangent `sign(tangent.w) × cross(N, T)`,
  as UniVRM's `MToon_GetTangentToWorld`.
- **The view direction** `V` is Celesta's per-fragment `Fragment.view`.
- **The main light.** `lit = baseColor × baseColorTexture`,
  `shade = shadeColorFactor × shadeMultiplyTexture`,
  `shift = shadingShiftFactor + shadingShiftTexture.r × scale`, and
  `color = mix(shade, lit, linearstep(−1 + toony, 1 − toony, N·L + shift)) × C₀`.
  `linearstep` is UniVRM's `saturate((t − a) / (b − a))`: above toony 1
  the ramp turns around, and at toony 1 it is a hard step, 0 up to `a` and
  1 above, as Unity's division by 0 gives. A frame without lights has a
  black main light.
- **Ambient.** `color += lit × mix(SH(N), ½(SH(+Y) + SH(−Y)), giEqualization)`,
  `SH` being Celesta's `ambient_light` (Unity's `ShadeSH9`).
- **Emission.** `color += emissiveFactor × emissiveTexture`.
- **Rim.** `rim = (matcap + parametric) × rimMultiplyTexture`, with
  `parametric = pow(saturate(1 − N·V + lift), max(power, 2⁻¹⁰)) × rimColor`
  (0 where `1 − N·V + lift` ≤ 0) and the matcap sampled at the view-space
  normal: `right = normalize(cross(+Y, V))`, `up = normalize(cross(V, right))`
  (the view's x axis as `right` when looking along ±Y), at
  `(N·right × ½ + ½, ½ − N·up × ½)`, so a normal leaning right samples the
  texture's right and one leaning up its top row.
  `color += rim × mix(1, C₀ + SH(N), rimLightingMix)`.
- **Additional lights** (the second to the fourth) are added in the same
  fragment, as Unity's ForwardAdd pass adds them: each adds
  `mix(shade, lit, step) × Cᵢ × shadow` and
  `rim × Cᵢ × shadow × rimLightingMix`, with
  `shadow = ½(min(0, N·L) + 1)` outside the step; no ambient or emission.
- There are no shadows: Unity's `atten` is 1.
- **UV animation,** as UniVRM's `GetMToonGeometry_Uv` after the texture
  transform, in glTF's `v`-down texture space: the mask's blue scales the
  time; the coordinates scroll by the time times the scroll speeds, then
  turn about the texture's centre by the time times the rotation speed
  (after a quarter turn, `(u, v)` samples `(v, 1 − u)`).
- **Alpha.** The base colour's alpha, which Celesta's entry points turn
  into mask coverage or blending.

### Outlines

- The outline pass pushes vertices out along their world normal, the
  width `outlineWidthFactor` times the width texture's green at the raw
  coordinates.
- `worldCoordinates`: the width is in metres, whatever `world`'s scale.
- `screenCoordinates`: the width is a share of the canvas's height, applied
  in clip space, corrected for the canvas's aspect, growing with distance
  only up to the distance at which a metre fills the height
  (`min(clip.w, P[1][1])`), and fading as the normal turns to face the
  camera (`1 − n_view.z²`). A normal facing the camera does not move.
- The colour is `outlineColorFactor × mix(1, shaded, outlineLightingMix)`,
  `shaded` being the surface's MToon colour under the main light alone
  (the built-in pipeline draws outlines in ForwardBase only), at the
  outward normal, following the UV animation and the normal texture.
- The outline draws the material's alpha, and breaks at hard edges as
  UniVRM's does: vertices that do not share a normal move apart.

### Parameters, and binds packed into them

`mtoon::params(mtoon, binds, time, textures)` packs the shader's `Params`,
eleven `vec4f` (176 bytes, `MTOON_PARAMS`), little-endian:

| Row | Contents |
| --- | --- |
| 0 | Base colour (or its bind). |
| 1, 2 | The base colour texture's transform as two rows, scale then rotation then offset (`KHR_texture_transform`'s order); a texture bind's offset and scale replace the transform's. |
| 3 | Shade colour (or its bind), shading shift factor. |
| 4 | Emissive factor (or the `emissionColor` bind), the shading shift texture's scale (0 without one). |
| 5 | Matcap colour (or its bind), shading toony factor. |
| 6 | Rim colour (or its bind), rim lighting mix. |
| 7 | Rim fresnel power, rim lift, GI equalization, the normal texture's scale (0 without one). |
| 8 | Outline colour (or its bind), outline lighting mix. |
| 9 | Outline width factor, width mode (0 none, 1 world, 2 screen), 0, 0. |
| 10 | UV scroll x and y at `time`, rotation speed in turns a second, 1 with a normal texture else 0. |

- Each frame, an MToon draw's parameters are packed again with the
  frame's binds (`MtoonOverrides`: base colour, texture offset and scale,
  shade, emission, matcap, rim, and outline colours) and time, and sent as
  the draw's `params` only when the bytes differ from the material's own:
  when the UV animation moves or an expression binds one of its values.
  Celesta refuses `overrides` on a custom draw, so MToon never uses them.
- An unlit draw takes only its base colour and texture offset and scale,
  as `overrides`; the other colour binds are dropped (an unlit material
  has no such colour).
- Binds add up, so many large ones can overflow. The plugin checks what it
  sends (every bind for MToon; the base colour and texture transform for
  unlit): `material {m}'s {bind} is not finite`.
- **The scroll** is worked out on the CPU in `f64` (time times speed) and
  wrapped by whole textures, two at a time for a mirrored repeat's period,
  only where that cannot show: the material neither rotates nor has a
  mask, and every texture the animated coordinates sample repeats along
  that axis. A masked or rotating animation keeps `f32`'s precision there,
  and the rotation's angle and the mask's scaling use Celesta's `f32`
  `frame.time`, so either drifts at very large times. An unwrapped scroll
  whose `time × speed` passes `f32`'s range packs ±∞, as UniVRM's `float`
  `_Time.y * speed` overflows; left as it is, for parity. A still scroll
  at a negative time packs +0, so its bytes stay the material's own.

### Where MToon knowingly differs from UniVRM

To check once UniVRM reference renders exist:

- **The mask's edge.** UniVRM clips at `alpha < cutoff` and outputs alpha
  1; Celesta's mask turns the alpha into coverage around the cutoff,
  sharpened to about a pixel. The edge moves by at most `|cutoff − ½|` of
  a pixel and is anti-aliased.
- **`V` per fragment.** UniVRM works out the view direction per vertex and
  interpolates it. The two differ most on large triangles seen from close
  by.
- **`KHR_texture_transform` rotation** is honoured, though UniVRM's
  `_MainTex_ST` cannot hold one.
- **Emissive strength,** or VRM's emissive multiplier without it, is
  folded into `emissiveFactor` by the plugin.
- **Additional lights** are summed in one fragment rather than in
  ForwardAdd passes; the blend equation gives the same result.
- **Translated into glTF's axes:** the matcap's axes (right-handed, `v`
  down), the UV animation's direction (scroll y down the texture, the turn
  reversed), and the SH and light mirroring (values from a Unity scene
  negate `c0`, `c4`, `c7`, and a light's x).
- **The screen-width outline's formula** and its cap at `P[1][1]`.
- **`linearstep` at toony 1**, a hard step following Unity's division by
  0, and the turned-around ramp above 1, following UniVRM's formula.
- **Coplanar draws** under Celesta's `LessEqual`, and the outline's depth
  bias.

## Skins, draws, and `center`

- **Draws** go node by node in pre-order over the default scene, then
  primitive by primitive. Each (mesh, primitive) has one geometry, shared
  by every node drawing the mesh with the same skin, or none; a mesh drawn
  with two skins, or skinned by one node and not another, gets a geometry
  for each. `open` moves the geometries into its answer; the provider's
  model keeps none afterwards (`VrmModel::take_scene_model`).
- **Skins:** one per glTF skin. A frame's skinning matrix for joint `j` is
  the joint's world times its inverse bind (the identity without inverse
  binds). A skinned draw's `world` is the identity (glTF ignores the mesh
  node's transform); an unskinned draw's is its node's world. A skinned
  node's primitive without `JOINTS_0` and `WEIGHTS_0` is placed by its node
  like an unskinned one. A skin without joints is refused.
- **`center`, every draw.** Unity sorts a renderer by its bounds' centre,
  and UniGLTF makes the skin's `skeleton` the renderer's root bone, whose
  space Unity's skinned bounds are in. So every primitive of a node's mesh
  is sorted by one point: the centre of the mesh's drawn primitives'
  bounds (mesh space), moved by the current world of the skin's `skeleton`
  node, else of the mesh node. Ties keep draw (primitive) order, so a
  transparent face and hair of one mesh draw in file order.
- **Morph weights** per draw: the node's `weights`, else the mesh's, else
  zeros; expressions replace the targets they bind.

## Each frame

`VrmModel::frame(time, data)` runs UniVRM's order
(`Vrm10Runtime.Process`): the base pose (the rest pose here; motions and
tracks in the motion plan), constraints (nothing yet), the lookAt,
expressions, spring bones (nothing yet), then world matrices, skinning
matrices, and draw states. A frame depends only on the model, `time`, and
`data`; nothing iterates a hash map into it, and frames asked for in any
order are equal to frames played in order.

### Expressions

Ported from UniVRM's `Vrm10RuntimeExpression`, `DefaultExpressionValidator`,
`ExpressionMerger`, and its mergers.

- **Keys** go in the presets' order (`happy angry sad relaxed surprised aa
  ih ou ee oh blink blinkLeft blinkRight lookUp lookDown lookLeft lookRight
  neutral`), then custom names in byte order. A custom expression named
  like a preset is left out with a `warning`.
- **Inputs** (`data.expressions`) are clamped to 0..1, VRM 1.0's range;
  names the avatar has no expression for are ignored, as UniVRM's
  `SetWeight` does.
- **Validation:** a binary expression's weight is rounded, halves to even
  (Unity's `Mathf.Round`). Every expression that is not itself a blink,
  lookAt, or mouth key adds its override rate to that group: `none` 0,
  `block` 1 once its weight is above 0, `blend` its weight. Each group's
  rate is clamped to 0..1 and multiplies the group's expressions by
  `1 − rate`; a binary one is 0 once the rate is above 0. So `happy` with
  `overrideBlink: block` turns blinking off, and `blend` scales it.
- **Binds:** a binary expression counts as 1 above 0.5, else 0.
  - A morph target any expression binds is `Σ bind.weight × weight` from 0
    each frame, on every draw of the bound node; the others keep their
    defaults. A bind to a node the file lacks, a node without a mesh, or a
    target the mesh lacks is ignored with a `warning`, and so is a second
    bind of one target by one expression (UniVRM keeps the first).
  - A material colour is `base + Σ (target − base) × weight`. UniVRM keeps
    `color`, `shadeColor`, `matcapColor`, `rimColor`, and `outlineColor`
    as sRGB-encoded material colours and takes the file's `targetValue` as
    it is, so these blend in sRGB (alpha linearly); `emissionColor` blends
    linearly.
  - A texture transform bind blends scale and offset from the material's
    base colour transform (scale 1, offset 0 without one). UniVRM blends
    Unity's flipped `_MainTex_ST`; the flip is affine, so blending glTF's
    values gives the same.
  - A material is overridden only while a bind of it has a weight.
  - A colour or texture bind naming a material the file lacks refuses the
    model, as UniVRM's importer throws on it.
- `VRMC_vrm_expressions_node_transform` binds are not read.

### LookAt

Ported from UniVRM's `Vrm10RuntimeLookAt` and its helpers.

- **Input** (`data.lookAt`): `{ yaw, pitch }` in degrees, a point
  `[x, y, z]` in model space, or `"camera"`, the frame camera's eye.
  Absent, yaw 0 and pitch 0.
- **The origin** is parented to the head, placed at rest at
  `offsetFromHeadBone` (default (0, 0.06, 0)) in the head's space with the
  model's axes, and moves and turns with the head.
- **A point's yaw and pitch:** with `p` the point in the origin's space,
  `yaw = atan2(−p.x, p.z)` (positive to the avatar's right, −X) and
  `pitch = atan2(p.y, |p.xz|)` (positive up).
- **The override:** whatever the input, yaw and pitch are multiplied by
  `1 − lookAt rate` from the expressions.
- **Range maps:** `clamp01(d / max(0.001, inputMaxValue)) × outputScale`.
  A map the file lacks is (90, 10); a map missing one value takes that
  value's default (UniVRM's importer throws there). An `inputMaxValue` of
  0 saturates, as UniVRM's runtime does (only its editor reads it as 90).
- **Bones,** for type `bone` with both eyes: each eye's local rotation is
  `pre × Ry(−yaw) × Rx(pitch) × post` between its rest rotations, Unity's
  `Euler(pitch, yaw, 0)` in glTF's axes. Looking left, the left eye turns
  by the outer map and the right by the inner, and the other way round
  looking right; down by the down map, up by the up map.
- **Expressions,** for type `expression` or a humanoid without both eyes
  (UniVRM falls back the same way): `lookLeft` or `lookRight` by the outer
  map (UniVRM uses it both ways), `lookDown` or `lookUp` by theirs, each
  clamped to 0..1, the other of each pair 0, written after validation over
  any weight asked for.

### First person

Celesta's cameras are never the avatar's own eyes, so the third-person
view applies: a node annotated `firstPersonOnly` draws `visible: false`;
`thirdPersonOnly`, `both`, `auto`, and unannotated nodes are drawn. A node
annotated more than once takes its first annotation, as UniVRM's
`VRM10ObjectFirstPerson.SetupAsync` keeps a renderer's first; each later
one is skipped with a `warning`, `firstPerson annotation {i} names node
{n} again; only its first annotation counts`.

## Camera presets and the reference light

`data.camera` is `"full"` (the default), `"bust"`, `"face"`, or an
explicit camera; `data.canvas` is the frame's canvas, `[width, height]` in
pixels, each side from 1 to 16384, else the model's 1920×1080 (`CANVAS`).
The frame answers the canvas, so the layer is that size, and the presets
fit its aspect.

- **Framing bounds:** every vertex of every visible draw at rest, skinned
  with the rest skinning matrices and no morphs, in model space, computed
  once when the model opens (without a visible vertex, the humanoid's
  bones at rest). Draws of one mesh share its geometry, so a file can draw
  one large mesh from many nodes almost for free; the walk is bounded so
  that `open` stays fast for it. A skinned geometry ignores its nodes'
  transforms, so it is walked once however many draws use it. Unskinned
  draws are walked vertex by vertex until the model's walks reach
  `FRAMING_VISITS` (50 million vertices, far past any avatar); a later
  unskinned draw is bounded by the 8 corners of its mesh's bounds, placed
  by its node, which is conservative for a rotated instance. Real models
  stay under the cap, so their framing is the exact walk's.
- **Presets** frame a vertical span `[y0, y1]`, 10% larger, through a
  perspective lens of 30° (`yfov` 0.5235988), `near` 0.01, `far` 100,
  looking down −Z from `(x, (y0 + y1) / 2, z)`:
  - `full`: the bounds' height, at the bounds' x centre;
  - `bust`: from the spine to the top, at the head's x;
  - `face`: from the neck to the top (without a neck, from below the head
    bone by half the distance from it to the top), at the head's x.
  - The height fits from the head: `z = head.z + 1.1 (y1 − y0) / 2 / tan 15°`.
    `full` also fits the bounds' width from their front, where the widest
    points nearest the camera are:
    `z = max(that, max.z + 1.1 (max.x − min.x) / 2 / (tan 15° × aspect))`.
- **Explicit:** `{ position, target, fov?, near?, far? }`, looking from
  `position` at `target` with up +Y (−Z when looking straight up or
  down); `fov` in degrees, default 30, between 0 and 180; `near` 0.01 and
  `far` 100 by default, `0 < near < far`, both finite. A camera whose
  position is its target, or a number out of range, is a frame error
  naming the field.
- **The reference light,** absent `data.lights`: Unity's defaults with
  shadows off. A directional light of rotation (50°, 150°, 0°), colour
  sRGB (255, 244, 214), intensity 1 is `direction [-0.3213938, -0.7660444,
  -0.5566704]`, `color [1.0, 0.9046612, 0.6724432]`, intensity 1; ambient
  mode Color, sRGB (54, 58, 66), is `ambient[3] = [0.0368895, 0.0423114,
  0.0544803]`, the other coefficients 0 (`REFERENCE_LIGHT`,
  `REFERENCE_AMBIENT`, `reference_lights()`).
- **Given lights:** its directional lights (linear colours) and its
  `ambient` as `ambient[3]` alone. A light whose direction is zero, or a
  number that is not finite, is a frame error naming it
  (`lights.directional[0].direction`); more than 4 lights is Celesta's
  refusal.

## Usage terms

`VRMC_vrm.meta` comes back from `open` as notices, one a line, in this
order. A term the file leaves out takes VRM 1.0's default, given in
brackets. The plugin never refuses to draw for the terms: whether a use is
allowed is for the person to decide.

- `info`: `VRM "{name}" {version} by {authors}; licence {licenseUrl}`
  (`; other licence {otherLicenseUrl}` when given), then `Copyright: …`,
  `Contact: …`, `References: …`, and `Third-party licences: …` when given.
- `avatarPermission` [`onlyAuthor`]: `everyone` → `Avatar use: anyone`;
  `onlySeparatelyLicensedPerson` → **warning** `Avatar use: only people
  the author licenses separately`; `onlyAuthor` → **warning** `Avatar use:
  only the author`.
- `commercialUsage` [`personalNonProfit`]: `personalNonProfit` →
  **warning** `Commercial use: not allowed (personal, non-profit only)`;
  `personalProfit` → `Commercial use: individuals only, not
  corporations`; `corporation` → `Commercial use: allowed`.
- `creditNotation` [`required`]: `required` → **warning** `Credit:
  required; credit {authors} where the video is published`; `unnecessary`
  → `Credit: not required`.
- The four content flags [not allowed]: `Excessively violent use`,
  `Excessively sexual use`, `Political or religious use`, `Antisocial or
  hateful use`: `allowed` or `not allowed`.
- `allowRedistribution` [false] and `modification` [`prohibited`]:
  `Redistribution of the model: …` and `Modification of the model: …`,
  each saying that a video made with it is neither.

So a file that leaves the three warned terms out opens with three
warnings. Seed-san always gives one, its credit line. The other notices
follow the terms: unreadable images, PBR materials drawn unlit, primitives
that are not drawn, repeated `firstPerson` annotations, and expression
binds left out.

A file can repeat one mistake for a few bytes each, and every notice goes
back in `open`'s answer, whose header Celesta caps. So warnings of one
kind (primitives not drawn, textures a material draws without, ignored
morph target binds, repeated `firstPerson` annotations) are listed up to
20 (`notices::LISTED`), then counted in one line: `and {n} more primitives
that are not drawn`, `… textures a material draws without`, `… morph
target binds ignored`, `… repeated firstPerson annotations`. A custom
expression named like a preset can only repeat the 18 presets, so those
warnings are not counted.

## The provider and `data`

- `open` takes one asset, `model`, the `.vrm` (`asset "model" (the .vrm)
  is missing`); any other is refused (`unknown asset "{name}"`) until
  motions arrive. A load error is `"{path}: {error}"`. Every error is
  `PluginError::Failed` for plugin `vrm`, except a request for another
  plugin id (`UnknownPlugin`).
- `frame` reads `data` as `PerformanceData`: `canvas`, `expressions`,
  `lookAt`, `camera`, `lights`; `null` is the default.
- **`data` is not trusted.** Unknown keys are refused. JSON writes NaN and
  infinities as `null`, so any `null` inside `data` is refused naming its
  key (`invalid data: camera.position[1] is null, not a finite number
  (JSON writes NaN and infinities as null)`), `{"camera": null}` included.
  A number past `f32`'s range reads as infinity and is refused by the
  frame's own checks, each naming its key (`expression happy is not
  finite`, `lookAt yaw is not finite`, `data canvas is …`, the camera's and
  lights' fields). Other malformed data is `invalid data: {serde's
  message}`. `<Vrm>` checks its numbers too (below).

## `<Vrm>`

`@celesta-plugin/vrm`'s `Vrm` puts one layer in the scene: plugin content
`{ type: 'plugin', plugin: 'vrm', assets: { model }, time, data }`,
`time` the frame over the fps. `vrmData(props, frame, fps)`, a pure
function of the frame, is the `data`; the element keeps only the common
props and `src`.

| Prop | What it does |
| --- | --- |
| `src` | The `.vrm` (VRM 1.0), sent as asset `model`. |
| `width`, `height`, `aspect` | The layer's size in pixels, sent as `canvas`: both given, as given; one given, the other from `aspect` (default 16:9); neither, 1920×1080, or 1920 wide by `aspect`. Not finite or not positive throws. The layer is that size; it is not scaled by them. |
| `expressions` | Weights by preset or custom name, 0..1. They win over `blink` and the mouth. |
| `mouth` | A shape (`'a'` … `'o'` set `aa` … `oh` to 1; `'closed'` none) or vowel weights. |
| `lipSync`, `mouthSmoothing` | A lip-sync track, used without `mouth`, mixed over this frame and the `mouthSmoothing` frames before it (default 2), nearest weighted most, as `<Live2D>` mixes them. `mouthSmoothing` not finite or negative throws. |
| `blink` | `@celesta/character`'s `blinkPhase` (default `{}`): open 0, half 0.5, closed 1, as `blink`. `false` never blinks. |
| `lookAt` | `'camera'`, a point in the avatar's space, or `{ yaw, pitch }` in degrees. |
| `camera` | `'full'` (default), `'bust'`, `'face'`, or `{ position, target, fov?, near?, far? }`. |
| `lights` | `{ directional?: [{ direction, color?, intensity? }], ambient? }`, colours `#rgb` or `#rrggbb` (sRGB, decoded to linear; `color` default white, `intensity` 1); anything else throws. Default: the reference light. |

Celesta draws the scene at the layer's density, so a smaller layer scaled
up is not sharper than a larger one.

## Model-file components and the Add menu

The manifest gains a component kind for files a plugin reads itself;
Celesta's side knows no format.

- `"src": "model"` with `extensions` (the file types it takes, lowercase
  letters and digits, without the dot, none repeated, at least one) and an
  optional `aspect` (the layer's width over its height, from 1/16384 to
  16384). `extensions` or `aspect` on a component that does not take a
  model file is refused. A refused component skips its whole plugin with
  the reason, as before. An older Celesta that does not know `"model"`
  skips the plugin too.
- The editor adds a model component like an Image: it asks for a file,
  refuses one whose extension (compared ASCII case-insensitively) is not
  listed (`Choose a .vrm file`; several, `Choose a .glb, .gltf, or .obj
  file`), draws the box at the manifest's `aspect` without reading the
  file (a free box without one), and writes `src`, `x`, `y`, and `width`.
- Since only `width` is written, the component's own default aspect must
  equal its manifest `aspect`, or the layer is not the box that was drawn.
  `<Vrm>`'s is 16:9, and so is its manifest's (`1.7777778`).

## What landed

| Where | What |
| --- | --- |
| `plugins/vrm/Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/main.rs` | The crate, its own workspace, and the program. |
| `plugins/vrm/src/document.rs`, `nodes.rs`, `humanoid.rs` | Opening the file and its refusals; the node tree, rest pose, and world matrices; the humanoid. |
| `plugins/vrm/src/geometry.rs` | Compacted primitives, computed normals, MikkTSpace tangents, and the read and geometry budgets. |
| `plugins/vrm/src/materials.rs` | Materials, scene textures, the texture cache, and the texture budget. |
| `plugins/vrm/src/mtoon.rs`, `mtoon.wgsl` | MToon as the plugin's own material. |
| `plugins/vrm/src/model.rs`, `first_person.rs` | The scene model, skins, draws, `center`, and each frame. |
| `plugins/vrm/src/expressions.rs`, `look_at.rs` | Expressions and lookAt with UniVRM's rules. |
| `plugins/vrm/src/camera.rs` | The canvas, camera presets, explicit cameras, and the reference light. |
| `plugins/vrm/src/meta.rs`, `provider.rs` | Usage-term notices; the provider and `data`. |
| `plugins/vrm/src/notices.rs` | Warnings of one kind listed up to a limit, the rest counted. |
| `plugins/vrm/src/synthetic.rs` | A small VRM 1.0 avatar built in code (feature `synthetic`), which every test opens. |
| `plugins/vrm/js`, `plugin.json`, `build.mjs`, `demo.tsx`, `README.md` | `<Vrm>` and its tests, the manifest, the build, and the demo. |
| `crates/plugin-host/src/manifest.rs` | `ComponentSource::Model`, `extensions`, `aspect`, and their checks. |
| `crates/editor/src/add_element.rs` | Model components drawn as images, the file check, and the manifest's aspect. |
| `Cargo.toml`, `.gitignore`, `pnpm-workspace.yaml`, `.github/workflows/ci.yml` | The root workspace excludes the plugin; its `target`, `dist`, and `assets` are ignored; `plugins/*/js` is a workspace package; CI job `vrm-plugin`. |

## Not done

- **VRM 0.x** is refused with the conversion hint; its migration is phase
  4.
- **glTF PBR** is drawn as unlit base colour, its emission and
  metal-roughness dropped; a PBR material of the plugin's is phase 5.
- **Humanoid motion (`.vrma`), constraints, and spring bones** are the
  motion plan's. Until constraints run, a constrained part stays at its
  rest pose: Seed-san's robot arm sticks out sideways.
- **`full` centres on the framing bounds' x,** so a wide, one-sided prop
  (Seed-san's robot arm) widens the bounds and shifts the avatar off
  centre.
- **`VRMC_vrm_expressions_node_transform`** binds are not read.
- **KTX2 and DDS textures** draw without their texture, with a warning.
- **Not read:** `TEXCOORD_1` and other UV sets (every slot samples
  `TEXCOORD_0`), vertex colours (`COLOR_0`), more than four joints a
  vertex (`JOINTS_1`/`WEIGHTS_1`), tangent morph deltas.
- **The texture cache is never cleaned.**
- **Celesta compiles the MToon shader once per renderer, when the first
  avatar opens,** and the pipelines an avatar's materials need that no
  model has used: a stall on the first MToon avatar.
- **Parity renders against UniVRM** (a reference Unity scene with the
  reference light) have not been made; the MToon formula tests check
  pixels worked out by hand.
- **The UV scroll overflows `f32`** at an extreme speed times time, as
  UniVRM's does (above).
- MikkTSpace (a C port) has not been run on hostile finite input.

## Where Celesta knowingly differs from UniVRM

Besides MToon's list above, to check against UniVRM:

- **Colour binds blend in sRGB,** as UniVRM stores those colours; a render
  against UniVRM should confirm the result.
- **`center` is offset from the skin's skeleton,** as UniVRM's renderer
  bounds are in the root bone's space; and it is the centre of the drawn
  primitives' bounds, where Unity's comes from the whole mesh.
- **Expression inputs are clamped to 0..1;** UniVRM does not clamp.
- **`KHR_texture_transform` rotation** is honoured.
- **A lookAt range map missing a value** takes its default, where UniVRM's
  importer throws.
- **The lookAt origin under a non-uniformly scaled head** follows glTF's
  matrices and differs from where Unity's transform hierarchy puts it.
- **Stricter than UniVRM:** a mesh's morph weights that are not finite
  refuse the model even when nothing draws them, and so does a morph bind
  whose weight is not finite, even one ignored for its target or as a
  repeat.
- **Material binds are keyed by material index,** where UniVRM's runtime
  keys them by material name: the two differ only for materials sharing a
  name.

## Checks

- `cargo test --manifest-path plugins/vrm/Cargo.toml --locked`, with
  `CELESTA_REQUIRE_GPU=1` so the GPU tests fail without an adapter instead
  of returning early; plugin `cargo fmt --check` and
  `cargo clippy --all-targets --locked -- -D warnings`;
  `pnpm --filter celesta-plugin-vrm-js test` (after `pnpm run
  build:runtime`), which also type-checks `demo.tsx`; and
  `node plugins/vrm/build.mjs`. CI's `vrm-plugin` job runs them all on
  lavapipe.
- Unit tests, on the synthetic avatar:
  - `document`: VRM 0.x refused with the hint, plain glTF, other spec
    versions, a missing bone, external buffers, bones past the nodes or
    sharing one, truncated and garbage files, JSON past its limit (a GLB
    padded with spaces, and a JSON file), a file past its limit (sparse).
  - `nodes`, `humanoid`: rest worlds, decomposed matrices, deep chains
    without overflow, two parents and cycles refused.
  - `geometry`: shared accessors compacted and decoded once, zero-weight
    joint slots zeroed, weights normalised, tangents only for normal
    textures, flat UVs, computed normals, strips and fans, sparse targets,
    the budgets, and malformed accessors refused.
  - `materials`: scene textures by image, colour space, and sampler; the
    base colour transform; blend fields; PBR drawn unlit with its notice;
    the emissive strength, else VRM's emissive multiplier;
    samplers; images written once by content; the cache's privacy checks;
    the texture budget; compressed and oversized images leaving the slot
    empty; WebP and `data:` images.
  - `mtoon`: UniVRM's defaults packed, binds in place of the material's
    values, the scroll's precision and wrapping, the slot and number
    checks, the shader's nine slots, the outline and tangents.
  - `model`: draw order, the rest frame, one `center` per mesh following
    its skeleton, hidden first-person meshes and a node's first annotation
    holding, morph weight defaults,
    frames that repeat, the default material, notices in order, skins;
    the framing walk's vertex count for one large mesh drawn by 800 nodes
    (240 million vertices walked one by one before the cap), corners
    past the cap equal to the exact walk for nodes that only translate,
    and a skinned geometry walked once for 1,000 draws.
  - `expressions`: blink blocked and blended, binary rounding, the mouth
    override, clamped inputs, sRGB and linear colour binds, UV binds,
    ignored and refused binds, sums past `f32`.
  - `look_at`: range maps, eyes by angle and point, the camera target,
    expressions for the gaze, the override, UniVRM's defaults.
  - `camera`: framing bounds, the presets' eyes at 16:9, 9:16, and 1:10,
    explicit cameras, refused cameras, canvases, and lights, the reference
    light.
  - `meta`: every term as a line in order, and the warnings.
  - `notices`: warnings past the 20 listed counted in one line, and their
    messages not made; at each site (10,000 point primitives, 31 materials
    without a texture, 100 ignored morph binds, 100 repeated
    annotations).
  - `provider`: `data` checked, nulls named, assets checked, frames equal
    to the model's, textures written to the cache the provider is given.
- `tests/mtoon.rs`: Celesta's former MToon GPU tests (`lit_`, `shading_`,
  `surface_`, `outline_`, `override_`, `queue_`,
  `mask_mtoon_covers_by_cutoff`), drawn through Celesta's renderer as the
  plugin's custom material, with their expected pixels unchanged.
- `tests/process.rs`: the built program and the provider in process answer
  the same contents, notices, and frames. It sets
  `CELESTA_VRM_TEXTURE_CACHE` for the child with `set_var`, sound only
  while it is that binary's one test.
- `tests/render.rs`: the synthetic avatar drawn at 480×270 (a 1920×1080
  layer at density 0.25): the box's middle opaque, a corner clear, frames
  repeating, and `happy` changing a pixel.
- `tests/samples.rs`, with `CELESTA_VRM_SAMPLES_DIR` naming a folder of
  the official samples (`Seed-san.vrm`, `VRM1_Constraint_Twist_Sample.vrm`,
  `VRMC_materials_mtoon_UV_Animation_Test.vrm`; never committed); without
  it each prints `skipped: CELESTA_VRM_SAMPLES_DIR is not set` and passes:
  - `seed_san_opens`: draws; the first notice is `info` with the licence
    URL; the PBR notice; frames out of order equal frames in order, every
    number finite.
  - `constraint_twist_sample_opens_with_bone_look_at`: the eyes turn.
  - `uv_animation_sample_keeps_its_speeds`: the three materials' speeds
    are exactly (1, 0, 0), (0, 1, 0), and (0, 0, 1), and none has a mask
    texture.
  - `seed_san_draws` (GPU): the bust frame covers the canvas's middle.
- `js/test/vrm.test.mts`: blinking, mouth shapes, lip-sync smoothing,
  `mouthSmoothing` checked, light colours, the canvas rules, the element's
  props, lookAt and camera passed through, and the mounted layer's
  content.
- `celesta-plugin-host`'s `model_components_need_extensions` and
  `celesta-editor`'s `model_components_draw_as_images_and_check_the_file`,
  with format-neutral fixtures.
