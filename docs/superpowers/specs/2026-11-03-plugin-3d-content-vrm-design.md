# 3D plugin content and the VRM plugin design

Builds on the content plugin system and the layers stacked on it
(`docs/handoff/2026-11-01-plugin-system.md`, `2026-11-02-plugin-search-paths.md`,
`2026-11-02-plugin-trust.md`, `2026-11-02-plugins-panel.md`,
`2026-11-02-plugin-add-menu.md`, `2026-11-02-plugin-waiting-stand-in.md`).
Related: mika-f/Celesta#645 (encoder and decoder plugins), which reshapes the
same protocol.

## Goal

A plugin can return 3D content: skinned, morphing triangle meshes with
materials, a camera, and lights. Celesta draws it with depth into the layer's
own canvas, which then composites like any other layer. On top of that, a VRM
plugin draws VRM 1.0 avatars (and later, as an experiment, VRM 0.x ones)
that look as close as practical to
UniVRM (vrm-c/UniVRM), the reference implementation.

## Decisions taken

- **Extend the plugin protocol.** It is unreleased, so it changes in place;
  nothing has to stay compatible with today's samples beyond what this
  design keeps (2D meshes stay as they are).
- **Celesta draws.** The plugin sends geometry, materials, and per-frame
  state, never pixels. Celesta owns the 3D pass. Celesta knows no VRM
  concept: it knows triangle meshes, skins, morph targets, a camera,
  lights, render queues and alpha modes, an outline pass, a built-in unlit
  material, and materials whose shading the plugin writes in WGSL against
  Celesta's material interface. MToon is such a material, shipped by the
  VRM plugin.
- **Look like UniVRM.** Where the VRM specification leaves room (defaults,
  lighting, outline width, draw order, spring bone stepping), UniVRM's
  behavior decides. three-vrm and the other implementations are references
  for code structure only.
- **VRM 1.0 first.** VRM 0.x support follows later and is experimental. It
  works by migrating the file to 1.0 on load as UniVRM's VRM10
  loader does, so one shading formula draws both. That migration is also the
  bar for 0.x, not UniVRM's legacy 0.x loader: the legacy loader is a
  separate package, and a usual UniVRM set-up installs only VRM10 and lets
  it convert 0.x files.
- **glTF PBR is optional.** Unlit and toon come first; metallic-roughness is
  a later phase.
- **The VRM plugin lives in this repository** but builds apart from the
  workspace and is not bundled with Celesta.
- **Existing libraries where they fit:** crates for the file formats, and
  UniVRM's code, ported, for the runtime behavior.
- **GPU renderer only.** `@celesta/web` does not draw 3D content. The CPU
  reference renderer keeps refusing plugin content (`UnsupportedPlugin`);
  nothing Celesta ships draws scenes with it (the exporter, editor, and MCP
  server use the GPU renderer, which runs on a software adapter such as
  lavapipe or WARP where there is no GPU).

## Non-goals

- Depth shared between layers, a scene-wide 3D camera, or 3D layers cutting
  into each other. A 3D layer is flattened into its parent like every other
  layer (CSS `transform-style: flat`).
- Writing or editing VRM files, retargeting arbitrary skeletons, IK, and
  physics beyond spring bones.
- First-person rendering. The third-person rule of `firstPerson` applies
  (`firstPersonOnly` meshes hidden).
- Matching UniVRM's tone mapping, post effects, or Unity's
  `RecalculateTangents` bit for bit.

## Where the pieces live

| Piece | Responsibility |
| --- | --- |
| `celesta-plugin` (protocol crate) | The `scene` content kind: model and frame types (shaders, custom and unlit materials, parameter bytes), their encoding, `ContentProvider` returning either kind. |
| `celesta-gpu-renderer` | The 3D pass: skinning and morphing, depth, MSAA, render queues, alpha modes, the material interface (checking and compiling a plugin's WGSL), lights and ambient as frame data, the outline pass, the built-in unlit material, conversion to the layer texture. |
| VRM plugin (`plugins/vrm` in this repository, built on its own) | glTF/VRM loading, 0.x migration, humanoid, VRMA, expressions, lookAt, constraints, spring bones, MToon as its own WGSL material, camera presets, usage terms, the `<Vrm>` component. |

- **In the monorepo, built apart.** `plugins/vrm` has a `[workspace]` of its
  own and is excluded from the root workspace, so `cargo build --workspace`,
  the root's tests, and the official packages do not build it. It depends on
  `crates/plugin` by path (no `[patch]` set-up is needed inside the
  repository). Its own build script writes the plugin directory (manifest,
  per-platform executable, component module, declarations), as
  `examples/plugins/build.mjs` does for the samples, and its own CI job runs
  its tests.
- **Not bundled.** It is not put in the bundled plugin folder. A user
  installs the built directory into the user plugin folder, where the
  editor asks about it like any user plugin; a `--plugin-dir` path is
  explicit and runs without asking (`2026-11-02-plugin-trust.md`).
- It stays a separate process like any plugin: Celesta's crates do not
  depend on it. Its licence is MIT OR Apache-2.0, like the workspace; code
  ported from UniVRM (MIT) carries UniVRM's notice.

### Libraries for the VRM plugin

The file formats have good crates; the runtime behavior does not, and it has
to follow UniVRM's code anyway, so it is ported.

| Part | Source |
| --- | --- |
| glb, accessors (including sparse ones), skins, morph targets, embedded images | `gltf` (MIT/Apache); VRM extensions read as raw JSON from it |
| VRM extension types | `vrm-spec` (pixiv's `vrm-utils-rs`, Apache-2.0): `VRMC_vrm`, `VRMC_materials_mtoon`, `VRMC_springBone`, `VRMC_vrm_animation` 1.0, and VRM 0.0. Early (0.1.x), so pinned. It has no `VRMC_node_constraint` or `VRMC_springBone_extended_collider` types; the plugin defines those (and anything else it lacks) with serde itself. |
| Math, tangents, images | `glam`, `mikktspace`, `image` |
| 0.x migration, expressions and their overrides, lookAt, constraints, spring bones, VRMA retargeting | Ported from UniVRM (`Packages/VRM10/Runtime`, `Packages/UniGLTF/Runtime/SpringBoneJobs`) |

- **Not dependencies:** existing Rust runtimes.
  - `vrm-runtime` is renderer-agnostic, but it is at 0.1 with one author, and it refuses VRM 0.x.
  - `bevy_vrm` and `bevy_vrm1` are Bevy ECS systems that render themselves.
  - Neither follows UniVRM's runtime order or spring stepping.
  - They, three-vrm, and godot-vrm stay references for structure and test ideas.
- **Migration is a separate step.** The 0.x migration is a document-to-document step: a 0.x glTF in, a VRM 1.0 model out. UniVRM migrates this way. Everything after loading then knows VRM 1.0 only.

## Protocol

### Kinds and roles

- `hello`'s answer gains `roles`, here `["content"]`, so #645's `encoder`
  and `decoder` roles fit beside it.
- `opened` gains `kind`: `"meshes"` (today's 2D meshes, unchanged) or
  `"scene"`. A frame answers in the kind its model was opened with.
- `opened` gains `notices`: `[{ "level": "info" | "warning", "message" }]`,
  shown in the editor's Problems tab and printed by the exporter. Generic;
  the VRM plugin uses it for the model's usage terms.

### Conventions for `scene`

- Right-handed, +Y up, metres, as glTF. The camera looks down its −Z.
- Matrices are `f32` 4×4, column-major. Quaternions are `x, y, z, w`.
- UV origin is the texture's top-left (glTF), unlike 2D meshes.
- Colors are linear unless stated.

### Model (sent once at `open`)

- `canvas: [width, height]`: the layer's size in pixels, as for meshes.
- `textures`: as today (path or inline RGBA), plus `colorSpace`
  (`"srgb"` | `"linear"`) and a sampler (`wrapS`, `wrapT`: repeat, mirrored
  repeat, clamp; `magFilter`, `minFilter`, mipmaps).
- `geometries`: indexed triangle lists with vertex streams in the payload:
  `position` (3×f32), `normal` (3×f32), `uv0` (2×f32), optional `tangent`
  (4×f32), optional `joints` (4×u16) and `weights` (4×f32), and morph
  targets as dense `position`/`normal` deltas. The plugin supplies
  MikkTSpace tangents where a material needs them (the file's are ignored,
  as UniVRM does).
- `skins`: joint counts. Inverse bind matrices stay in the plugin, which
  sends final skinning matrices.
- `shaders`: the model's material shaders, each
  `{ name?, wgsl, textures: [{ name?, empty: "white" | "black" }] }`: WGSL
  written against Celesta's material interface (below) and its texture
  slots, slot `k` being `texture_k` and `sampler_k` in the WGSL. Many
  materials share one shader; Celesta compiles a source once.
- `materials`: `alphaMode` (`opaque`; `mask` with `alphaCutoff`; `blend`
  with optional `depthWrite` and `queueOffset`), `doubleSided`, and `kind`:
  - `unlit` (`KHR_materials_unlit`): base colour × texture, Celesta's own.
  - `custom`: `shader` (an index into `shaders`), `textures` (a texture
    index or `null` per slot), optional `outline` (`{ cull, depthBias }`,
    an outline pass), `requiresTangents`, and a parameter block of bytes
    laid out as the shader's `struct Params` in WGSL's uniform layout. The
    bytes travel in the payload.
- `draws`: `{ geometry, material, skin? }`, the units of drawing and of
  transparent sorting (Unity's renderers).

### Frame (per request)

- `camera`: `view` matrix and `projection`, either
  `{ perspective: { yfov, near, far } }` or
  `{ orthographic: { height, near, far } }`. The aspect is the canvas's.
- `lights`: up to four directional lights `{ direction, color, intensity }`
  (the direction light travels; the first is the main light; Unity's default
  pixel-light count) and an `ambient` as nine RGB coefficients `c0…c8` of
  `c0·x + c1·y + c2·z + c3 + c4·xy + c5·yz + c6·z² + c7·zx + c8·(x² − y²)`,
  Unity's `ShadeSH9` (SHA, SHB, SHC) in this space's axes; a flat ambient
  is `c3` alone. Values copied from a Unity scene negate `c0`, `c4`, `c7`
  (and a light's x), since UniVRM mirrors X.
- `time`: seconds for material UV animation.
- `draws`: every draw, with `visible`, `world` matrix, morph weights, and
  either `params` (a custom material's parameter bytes for this frame,
  replacing the material's) or `overrides` (an unlit material's colour and
  texture transform for this frame). The payload carries each skin's
  skinning matrices (joint world × inverse bind), the weights, and the
  parameter bytes.

A VRoid-sized avatar (about 100 joints, 50 morph targets) is a few KB per
frame, so the pipe is enough. The `open` payload is a few MB once.

## Rendering in Celesta

### Placement

`plugin_texture` keeps its role: content is drawn onto an off-screen canvas
of the layer's size at the layer's density, and that texture composites as
the layer. Effects, masks, blend modes, and 2D/3D layer transforms apply to
it unchanged.

### Targets

- Color `Rgba16Float`, linear, 4× MSAA. Depth `Depth32Float`, 4× MSAA.
  Where the GPU cannot render, sample and blend `Rgba16Float`, the color
  target is `Rgba8Unorm`; where the chosen format or `Depth32Float` cannot
  be multisampled 4×, both are drawn with 1 sample. A GPU with no usable
  color format refuses scene content with an error rather than panicking.
- Cleared to transparent black.
- Resolved, then converted to the layer texture's encoding: premultiplied,
  sRGB-encoded values, as the compositor holds every layer (it blends like a
  browser canvas). No tone mapping, matching a Unity linear project's
  output without post effects.

### Per frame

1. **Skin and morph** in a compute pass into per-draw storage buffers
   (positions, normals, tangents). A draw's surface and its outline pass
   read the same results: an outline pass offsets post-skinning vertices.
2. **Draw** in render-queue order, opaque before transparent, ties by draw
   index for determinism, for every material kind:

   | Alpha mode | Queue | Blend | Depth write | Notes |
   | --- | --- | --- | --- | --- |
   | `opaque` | 2000 | none | on | |
   | `mask` | 2450 | none | on | the material's alpha to coverage, sharpened: `(a − cutoff) / max(fwidth(a), 1e-5) + 0.5` |
   | `blend` with `depthWrite` | 2501 + clamp(`queueOffset`, 0, 9) | src-alpha | on | |
   | `blend` | 3000 + clamp(`queueOffset`, −9, 0) | src-alpha | off | sorted back to front by draw bounds centre |

   Colour blends as Unity does (`SrcAlpha, OneMinusSrcAlpha` in linear
   space). Alpha accumulates as coverage (`One, OneMinusSrcAlpha`), because
   Celesta composites the layer's alpha. Depth compares `LessEqual`. A
   blended fragment with alpha below 2⁻¹⁰ is discarded, so a clear pixel of
   a depth-writing blend writes no depth.
3. **Outline pass** for a material that asks for one, right after its
   surface in the same queue slot, from the same skinned vertices: the cull
   face and depth bias the material gives (by default front faces culled
   and Unity's `Offset 1, 1`), the material's alpha mode, and the
   material's outline functions, whose vertex hook offsets each vertex in
   world space before projection and in clip space after it.

### Materials

- **The material interface.** Celesta prepends a prelude to a material's
  WGSL and appends its entry points. The prelude gives `frame` (camera
  matrices, eye, forward, time, the canvas's height over width, up to four
  directional lights, nine SH ambient coefficients), `draw` (world, normal
  matrix, handedness, alpha cutoff), the material's `params`, its texture
  slots, the structs `Vertex`, `VertexOffset`, and `Fragment`, and the
  helpers `linearstep`, `ambient_light` (the SH polynomial), and
  `safe_normalize`. A material defines `struct Params` and
  `fn material_fragment(fragment: Fragment) -> vec4f` (straight linear
  colour and alpha), and may define `material_vertex`,
  `material_outline_fragment`, and `material_outline_vertex`. Celesta's
  entry points apply the alpha mode: alpha 1 for opaque, coverage for
  mask, the near-clear discard for blend.
- **Safety and limits.** A material binds nothing of its own; the bindings
  are Celesta's and hold only the scene's own frame, draw, parameters, and
  textures. Celesta checks the WGSL with naga before any pipeline exists
  (no entry points, resources, workgroup variables, `override` constants,
  names WGSL predeclares, or `celesta` names of the material's own; at
  most 262,144 bytes; at most 12 texture slots;
  `Params` at most 512 bytes and equal to the bytes sent; at most 32
  shaders a model) and compiles it with wgpu's runtime checks (bounds
  checks and loop bounding), which its safe `create_shader_module` always
  applies. Any failure is
  `InvalidPluginContent` naming the material and the line in its WGSL.
- **`unlit`** is Celesta's built-in material, drawn through the same
  interface.
- **PBR** (later) is a plugin material too: the VRM plugin brings glTF
  metallic-roughness as WGSL under the same lights. UniVRM maps it to
  Unity's Standard shader; matching that is out of scope, so it is
  documented as an approximation.

### Lighting presets and shadows

- UniVRM takes lights from the Unity scene, so there is no "UniVRM default".
  The VRM plugin's default is the reference scene below (one directional
  light, a flat ambient), which is also what the parity checks use.
- Shadows (the main light's `atten`) are a later phase: Celesta renders a
  shadow map for the main light, self-shadowing only, since the layer has
  no other casters, and offers it to materials through the interface.

## The VRM plugin

### Loading

- `.vrm` (glb) with `VRMC_vrm` 1.0; `.vrma` (`VRMC_vrm_animation`) as
  motion assets.
- **VRM 0.x** (the `VRM` extension):
  - Until the migration lands, opening a 0.x file is an error. The error says that VRM 0.x is not supported yet, and that UniVRM or Blender's VRM add-on can convert the file to 1.0.
  - Once the migration lands, a 0.x file opens with a `warning` notice that 0.x support is experimental. The parity checks below cover 1.0 only.
- **0.x migration** (experimental, a later phase), ported from UniVRM's
  `Packages/VRM10/Runtime/Migration`:
  - Coordinates turned 180° about Y (positions, normals, morph deltas,
    node translations, inverse binds: `(x, y, z) → (−x, y, −z)`);
    extension vectors (collider offsets, gravity, first-person offset)
    `(x, y, z) → (−x, y, z)`.
  - Blend shape groups → expressions (`a/i/u/e/o → aa/ih/ou/ee/oh`,
    `joy → happy`, `sorrow → sad`, `fun → relaxed`, …), weights ÷ 100,
    material binds and texture-transform binds carried over.
  - Secondary animation → spring chains (group parameters on every joint,
    a 7 cm `_end` tail for leaves, sphere colliders).
  - LookAt (bone or blend shape, range maps), first person.
  - Materials: MToon 0.x → MToon 1.0 with UniVRM's formulas (shading
    range, `giEqualization = 1 − indirectLightIntensity`, outline width
    units, UV animation units, render queue re-ranking); the 0.x unlit
    shaders → unlit (and `UnlitTransparentZWrite` → MToon). Parameters with
    no 1.0 equivalent (light color attenuation, receive-shadow and
    shading-grade, the `min(col, lit)` clamp) are dropped, as UniVRM does.
- Tangents are always recomputed (UniVRM ignores the file's), with
  MikkTSpace.

### MToon (UniVRM's MToon10, built-in render pipeline)

The plugin's MToon is a material in WGSL (`plugins/vrm/src/mtoon.wgsl`)
with a parameter block it packs from `VRMC_materials_mtoon` (and, each
frame, from material binds and the UV animation's time). It maps the
file's `transparentWithZWrite` and `renderQueueOffsetNumber` to
`alphaMode: blend`'s `depthWrite` and `queueOffset`, asks for an outline
pass when `outlineWidthMode` is not `none`, and asks for tangents when the
material has a `normalTexture`.

For the main light (`atten` is 1 until shadows exist):

```text
lit    = baseTex × baseColorFactor
shift  = shadingShiftFactor + shadingShiftTex.r × scale
shade  = linearstep(−1 + toony, 1 − toony, dot(N, L) + shift) × atten
direct = lerp(shadeColor × shadeMultiplyTex.rgb, lit, shade) × lightColor
gi     = lit × lerp(SH(N), ½(SH(+Y) + SH(−Y)), giEqualization)
rim    = (matcapFactor × matcapTex(uvMatcap)
          + pow(saturate(1 − dot(N, V) + rimLift), max(fresnelPower, 2^−10)) × rimColor)
         × lerp(1, lightColor + SH(N), rimLightingMix) × rimMultiplyTex
color  = direct + gi + emissiveTex × emissiveFactor + rim
```

- `linearstep(a, b, t) = saturate((t − a) / (b − a))`, as UniVRM writes
  it: above toony 1, `b < a` and the ramp turns around; at toony 1,
  `a = b` and it is a hard step, 0 up to `a` and 1 above (Unity's division
  by 0).
- `uvMatcap`: UniVRM's basis (`right = cross(V, +Y)`, `up = cross(right, V)`
  in Unity's left-handed space) written in this right-handed space:
  `right = normalize(cross(+Y, V))`, `up = cross(V, right)`,
  `(dot(right, N) × ½ + ½, ½ − dot(up, N) × ½)` (glTF's top-left UV origin).
  When V is parallel to ±Y, `right` falls back to the view's x axis.
- Additional directional lights add
  `lerp(shadeColor, lit, linearstep(…)) × lightColor × shadow`, with
  `shadow = atten × ½(min(0, dot(N, L)) + 1)` and no attenuation inside the
  step, and their rim with `lerp(0, lightColor × shadow, rimLightingMix)`;
  no GI or emission (Unity's ForwardAdd pass).
- UV animation, as UniVRM's `GetMToonGeometry_Uv` does it: the base color
  texture's transform applies first (to every MToon texture but the matcap,
  like `_MainTex_ST`); the animation mask is sampled there; then
  `uv = R(−t × rotationSpeed) × (uv + t × scroll − ½) + ½` with
  `t = time × uvAnimationMaskTex.b`. The file's speeds are used as they are
  (scroll in UV units per second in glTF's UV space, rotation in radians per
  second), which is what UniVRM's conversion to Unity's space amounts to.
- Back faces of double-sided materials flip the normal. Vertex colors are
  ignored.
- **Defaults follow UniVRM's importer.** A field missing from the file takes
  the value UniVRM's shader would, not the specification's: shading shift
  −0.05, toony 0.95, shade color white, matcap factor black. Empty texture
  slots take Unity's default textures (black for shading shift and matcap,
  white for the rest).
- Also taken from UniVRM's MToon10: depth compare `LessEqual` (Unity's
  `ZTest LEqual`); `blend` pixels with alpha below 2^−10 are discarded; the
  outline pass shades with the main light only (ForwardBase), with the
  outward normal, and draws whenever its mode is not `none`; the tangent's
  sign follows the world matrix's determinant; normal maps decode as
  Unity's desktop layout (`z = sqrt(1 − |xy|²)` after scaling).
- `mask` keeps coverage from the sharpened alpha (above) rather than
  UniVRM's `clip(t − cutoff)` with alpha 1, whose alpha-to-coverage has no
  effect: edges move by at most `|cutoff − ½|` of a pixel. A known
  difference, recorded for the parity checks.
- Outline colour `outlineColor × lerp(1, shaded, outlineLightingMix)` where
  `shaded` includes emission and rim, under the main light only.
- World width: `position + normalize(normal) × width × widthTex.g`
  (metres, unaffected by scale), as the outline's world offset.
- Screen width, as the outline's clip offset:
  `offset = normalize(normalClip.xy) × 2 × width × min(clip.w, P[1][1])`,
  `offset.x ×= height / width`, `offset ×= saturate(1 − normalView.z²)`.
- The width texture is sampled with raw `uv0`: no texture transform, no
  UV animation.
- Material binds (`color`, `shadeColor`, `emissionColor`, `matcapColor`,
  `rimColor`, `outlineColor`, texture transforms) and the UV animation's
  scroll, formed in `f64`, are packed into each frame's parameters by the
  plugin.

### Per frame

The order is UniVRM's runtime (`Vrm10Instance` then the spring bone
service), which differs from the specification's recommendation:

1. Humanoid pose from motions and tracks.
2. Constraints.
3. LookAt (yaw/pitch, eye bones or look expressions, range maps).
4. Expressions, with UniVRM's override rules (`overrideBlink`,
   `overrideLookAt`, `overrideMouth`, `isBinary`) and material binds
   (`base + Σ(target − base) × w`).
5. Spring bones.

### Spring bones, deterministic

- UniVRM's formula: inertia `(cur − prev) × (1 − drag)` not scaled by the
  step, stiffness and gravity scaled by it; length re-normalised; colliders
  (sphere, capsule from the head position, and the extended plane and inside
  shapes) in order; centre space for the tails.
- UniVRM steps once per rendered frame, so its motion depends on the frame
  rate. Here the step is fixed at 60 Hz (a usual Unity frame rate, and the
  rate the parity recordings use), advanced in whole steps to the requested
  time, so a 30 fps and a 60 fps export show the same motion.
- Frames may be asked for in any order. As in the Live2D plugin, the
  simulation runs from time 0 with checkpoints every 30 steps, keyed by
  everything the motion depends on (motions, tracks, expressions, lookAt,
  the springs' settings); a request resumes from the nearest checkpoint
  before it.

### The `<Vrm>` component

A sketch; the plan settles the names.

```tsx
<Vrm
  src="./avatar.vrm"
  motions={[{ src: './idle.vrma', loop: true }, { src: './wave.vrma', from: 60 }]}
  expressions={{ happy: 0.6 }}
  mouth={vowelTrack}           // aa/ih/ou/ee/oh, e.g. from VOICEVOX lip sync
  blink="auto"                 // seeded, deterministic
  lookAt="camera"              // or a point, or yaw/pitch
  tracks={{ head: { rotation: … } }}
  camera="bust"                // presets from the humanoid, or explicit
  lights={…}
/>
```

- The manifest's `components` offers it in the Add menu. `src` there is
  `image` or `video` today; it gains a model kind (a file filter the plugin
  declares) so the editor can pick a `.vrm`.
- A waiting plugin's stand-in renders nothing, as for any plugin.

### Usage terms

The VRM `meta` (avatar permission, commercial use, violent or sexual use,
political or religious use, redistribution, credit) comes back as `info`
notices on open, and as a `warning` where a field forbids what an export is
likely to be (for example commercial use not allowed). The plugin does not
refuse to draw; the user decides.

## Checks

- **Renderer**, with a fake provider and small test materials written
  against the interface: depth occlusion; the four queues and their order;
  alpha-to-coverage edges; parameters, texture slots and their empty
  texels, lights and ambient, the normal matrix and handedness, and the
  view direction reaching a material; the outline pass's world and clip
  offsets, culling, and bias; per-frame parameters; refused WGSL, sizes,
  and limits; a shader compiled once; skinning matrices and morph weights
  moving vertices; the final encoding against a 2D layer of the same
  colour; frames on lavapipe in CI.
- **Plugin:**
  - a 0.x file refused with the conversion hint (until the migration),
    then 0.x migration of sample files against fixtures produced once with
    UniVRM;
  - frames for out-of-order times equal frames played in order, including
    after a restart;
  - spring bone sequences against UniVRM recordings at 60 fps.
- **Parity:** in the VRM plugin. A reference Unity scene with UniVRM, a
  fixed camera, one directional light, a flat ambient colour, and shadows
  off renders sample VRM 1.0 models (MToon with outline, rim, matcap,
  cutout hair, transparency). Experimental 0.x support has no parity bar.
  Celesta renders the same with the plugin's reference preset and its
  MToon material. The images are compared with a stated tolerance, and the
  differences are recorded. Only models whose licence allows it are
  committed; the rest are checked locally. The plugin's MToon formula tests
  (one lit square per term, expected pixels worked out by hand) live with
  its material.

## Phases

1. **Protocol and the 3D pass:** the `scene` kind, `notices`, `roles`;
   skinning and morphing, depth, MSAA, unlit, camera; renderer tests.
2. **Materials:** the material interface and custom WGSL materials; lights
   and SH ambient as frame data; the queues with depth-writing blend and
   offsets; the outline pass; per-frame parameters; the built-in unlit
   material on the same interface.
3. **The VRM plugin, 1.0:** loading, MToon as its own material (moved from
   Celesta with its formula tests), humanoid and VRMA, expressions, lookAt,
   constraints, deterministic spring bones, usage terms, `<Vrm>`, Add menu.
4. **VRM 0.x, experimental:** the migration step, its fixtures, and the
   experimental notice. Until then 0.x files are refused (phase 3).
5. **Later:** main-light shadows offered to materials (Celesta), point and
   spot lights (Celesta), glTF PBR as a plugin material.

Phases 1–2 (Celesta) and 3 (plugin, drawn unlit at first) can proceed side by
side once the protocol types land.

## Open questions

- The parity tolerance, and which sample models can be committed.
- How far the per-frame protocol should go toward #645's shared-memory
  transport, should a plugin ever send CPU-deformed vertices instead of
  skinning matrices.
