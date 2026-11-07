# Plugin materials in WGSL for scene content (2026-11-03)

A scene's materials can now be the plugin's own: WGSL written against
Celesta's material interface, with a block of parameters the plugin packs
and texture slots it fills. Celesta checks and compiles that WGSL once,
however many materials and models use it, and draws it with the frame's
directional lights and spherical harmonics ambient light, its render
queues and alpha modes, and, when the material asks, an outline pass.
Celesta keeps one built-in material, `unlit`, drawn through the same
interface. A blended material of any kind can write depth and move within
its render queue, and each frame can replace a custom material's
parameters, as a model's material animation does. Scene frames keep their
canvases and buffers from frame to frame.

Celesta knows no file format's shading. The VRM plugin
(`2026-11-03-plugin-vrm.md`) brings MToon, VRM's toon shading, as a
material of its own written against this interface; the turntable sample
brings a small toon material of its own.

This is the second phase of the design in
`docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md`. The
first, `2026-11-03-plugin-scene-content.md`, has the scene protocol, the 3D
pass, unlit materials, and notices; what it says still holds unless this
note says otherwise.

## Materials

A material's `alphaMode` and `doubleSided` work as before for every kind.
Its `kind` is one of:

- `unlit` (`MaterialKind::Unlit`): Celesta's own, unchanged: `baseColor`
  times the optional `baseColorTexture`, sampled through its transform,
  with no lighting. A frame's `overrides` (`baseColor`, `textureOffset`,
  `textureScale`) replace its values for one draw.
- `custom` (`MaterialKind::Custom(CustomMaterial)`): drawn with one of the
  model's shaders.

A model lists its shaders once in `SceneModel::shaders`; any number of
materials share one.

| `SceneShader` field | What it is |
| --- | --- |
| `name` | Optional. What Celesta's messages call the shader. |
| `wgsl` | The material's WGSL (below). |
| `textures` | The texture slots, each `{ name?, empty }`: slot `k` is `texture_k` and `sampler_k` in the WGSL. `empty` is what the slot samples when a material leaves it empty: `"white"` (the default, `(1, 1, 1, 1)`) or `"black"` (`(0, 0, 0, 1)`), a 1×1 texture with Celesta's default sampler. |

| `CustomMaterial` field | What it is |
| --- | --- |
| `shader` | Which `shaders` entry draws it. |
| `params` | The shader's `Params`, as bytes: exactly as many as its `struct Params` takes, in WGSL's uniform layout, little-endian. They travel in the payload, never in the JSON. |
| `textures` | A `SceneModel::textures` index or `null` per slot. Missing and `null` slots are empty. No more entries than the shader has slots. |
| `outline` | Optional `{ cull, depthBias }`: an outline pass (below). `cull` is `"front"` (the default), `"back"`, or `"none"`; `depthBias` is `{ constant, slopeScale }`, by default `{ 1, 1.0 }`, Unity's `Offset 1, 1`. `{}` reads as the defaults, for `outline` and `depthBias` alike, and a missing field takes its default. |
| `requiresTangents` | The shader reads `Vertex.tangent`: a draw of this material on a geometry without tangents is refused. Without it, such a geometry gives the shader tangents of 0. |

Each texture is bound with its own sampler. Celesta reads nothing of the
parameter bytes: they go to the GPU as they are.

## Blending, for every material

`alphaMode: "blend"` gains two optional fields, for every material kind,
`unlit` included (`AlphaMode::Blend { depth_write, queue_offset }`):

- `depthWrite` (default `false`): the blended material also writes depth.
- `queueOffset` (default `0`): moves the material within its render queue
  (below).

`opaque` and `mask` have neither: they cannot carry an offset that would
be ignored. `{"alphaMode":"blend"}` reads as before (`AlphaMode::BLEND`).

## The material interface

Celesta assembles a material's module from
`crates/gpu-renderer/src/scene_material_prelude.wgsl`, the declarations of
its texture slots, the material's WGSL, and Celesta's entry points
(`scene_material_entry.wgsl`, and `scene_material_outline_entry.wgsl` when
the material has an outline). The prelude gives:

```wgsl
struct Light { to_light: vec4f, color: vec4f };      // towards the light, length 1; color × intensity; black when absent
struct Frame {                                       // 512 bytes
    view_projection: mat4x4f, view: mat4x4f, projection: mat4x4f,
    eye: vec4f,               // the camera's position in world; w 1 perspective, 0 orthographic
    forward: vec4f,           // the camera's +Z (towards the viewer) in world
    time: f32,                // the frame's time in seconds, as f32
    height_over_width: f32,   // the canvas's height over its width
    light_count: u32,         // 0 to MAX_LIGHTS
    celesta_reserved: u32,
    lights: array<Light, MAX_LIGHTS>, // the main light first
    ambient: array<vec4f, 9>, // RGB of the SH coefficients c0…c8
};
struct Draw {                 // 128 bytes
    world: mat4x4f,
    normal_matrix: mat3x3f,   // takes model normals to world, up to a positive scale
    handedness: f32,          // −1 when world mirrors, else 1
    alpha_cutoff: f32,        // a mask material's cutoff, else 0
};
struct Vertex {               // one skinned vertex in world space
    position: vec3f,
    normal: vec3f,            // safe_normalize(normal_matrix × skinned normal)
    tangent: vec4f,           // xyz: safe_normalize(world's 3×3 × skinned tangent); w: sign(tangent.w) × handedness; all 0 without tangents
    uv: vec2f,                // the geometry's uv0
    clip: vec4f,              // view_projection × position
};
struct VertexOffset { world: vec3f, clip: vec2f };   // added to position before projecting, then to clip.xy
struct Fragment {             // one fragment, in world space
    position: vec3f,          // after the vertex offset
    normal: vec3f,            // interpolated, renormalised; not turned on back faces
    tangent: vec4f,           // xyz renormalised, w interpolated
    uv: vec2f,
    view: vec3f,              // towards the viewer: perspective normalize(eye − position), orthographic forward
    front_facing: bool,
    pixel: vec2f,             // the fragment's position on the canvas, in pixels
};
@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> draw: Draw;
@group(1) @binding(1) var<uniform> params: Params;   // the material declares `struct Params`
const MAX_LIGHTS: u32 = 4u;
fn linearstep(start: f32, end: f32, t: f32) -> f32   // saturate((t − start) / (end − start)); a step (0 up to start, 1 above) when end == start
fn ambient_light(n: vec3f) -> vec3f                  // c0·x + c1·y + c2·z + c3 + c4·xy + c5·yz + c6·z² + c7·zx + c8·(x² − y²)
fn safe_normalize(v: vec3f) -> vec3f                 // v / |v|, or v when |v| is 0
```

and, for each slot `k`, `@group(2) @binding(2k) var texture_k:
texture_2d<f32>;` (filterable float) and `@group(2) @binding(2k + 1) var
sampler_k: sampler;` (filtering), visible to both stages.

A material defines:

```wgsl
struct Params { … }                                           // required: at least one member, at most 512 bytes
fn material_fragment(fragment: Fragment) -> vec4f             // required: straight linear RGB and alpha
fn material_vertex(vertex: Vertex) -> VertexOffset            // optional; a zero offset by default
fn material_outline_fragment(fragment: Fragment) -> vec4f     // optional: the shader has an outline pass
fn material_outline_vertex(vertex: Vertex) -> VertexOffset    // optional, only with material_outline_fragment; zero by default
```

Celesta's entry points call these and apply the alpha mode, so a material
returns its color and alpha whatever its mode: opaque writes alpha 1;
mask writes alpha 1 and turns the returned alpha into sample coverage
around `draw.alpha_cutoff`, `(a − cutoff) / max(fwidth(a), 1e-5) + 0.5`
over the canvas's samples, 4 where the GPU multisamples the scene and 1
where it cannot (each pixel then covered from the cutoff up); blend
clamps alpha to 0..1 and discards a fragment whose
alpha is below 2⁻¹⁰ (`0.0009765625`), after the material has run, so a
clear pixel of a depth-writing blend writes no depth. The outline's entry
points do the same around the outline functions.

A minimal material, Lambert under the main light plus ambient:

```wgsl
struct Params { albedo: vec4f };
fn material_fragment(fragment: Fragment) -> vec4f {
    let light = frame.lights[0];
    let lit = light.color.rgb * max(dot(fragment.normal, light.to_light.xyz), 0.0)
        + ambient_light(fragment.normal);
    return vec4f(params.albedo.rgb * lit, params.albedo.a);
}
```

with `params` the 16 bytes of `albedo` and no texture slots.

- **Normals and faces.** `Fragment.normal` is not turned towards the
  viewer on a back face, which only a double-sided material or an outline
  pass draws: a material that wants that uses `front_facing`.
- **Vertex hooks** may read texture slots only with `textureSampleLevel`
  and take no derivatives; naga refuses the rest, and Celesta's message
  says so.
- **Uniformity.** There is one function per pass rather than a pass flag:
  a flag would be a function argument, which WGSL counts as non-uniform,
  and branching on a non-uniform value before `textureSample` leaves its
  derivatives undefined. naga does not enforce fragment uniformity, so
  Celesta does not refuse such branches; a material should not rely on
  them.
- **The view direction** `V` is per fragment: towards the eye through a
  perspective camera, the camera's +Z through an orthographic one. The eye
  is where the view takes the origin from (`−M⁻¹t` of the view's 3×3 `M`
  and translation `t`).
- **Time.** `frame.time` is `f32`. A material whose result depends on long
  times (a texture scroll over a long video, say) gets the precise value
  from its plugin, which forms it in `f64` and sends it in the parameters.

## Checks and limits

Every failure below is `InvalidPluginContent` for the layer; a model that
fails to load is closed again. The limits are `pub const`s in
`celesta_plugin::scene`.

- **On open, before naga or the GPU reads any WGSL:** at most 32 shaders a
  model (`MAX_SCENE_SHADERS`); at most 262,144 bytes of WGSL a shader
  (`MAX_SHADER_BYTES`); at most 12 texture slots a shader
  (`MAX_MATERIAL_TEXTURES`: WebGPU's default 16 sampled textures and 16
  samplers a stage, 4 kept for Celesta); a custom material's `shader` in
  range, its `textures` no longer than the shader's slots, and each filled
  slot's texture in range; an outline's `slopeScale` finite; an `unlit`
  material's `baseColor` and its texture's transform finite
  (`material {m}'s base color is not finite`,
  `material {m}'s texture transform is not finite`); a draw whose
  material has `requiresTangents` on a geometry without tangents. Messages
  name the shader as `shader {s} ({name})`, the name only when it has one.
- **Each shader the model lists, used or not, is checked** with naga on
  the CPU: the WGSL must parse and validate (`ValidationFlags::all()`,
  default capabilities) once assembled; declare no resource of its own
  (only `frame`, `draw`, `params`, and its own slots' `texture_k` and
  `sampler_k`), no entry point, no pipeline-overridable constant (a
  material pipeline sets only Celesta's entry points' `CELESTA_SAMPLES`),
  and no workgroup variable
  (`var<workgroup>`: WGSL allows workgroup memory only in compute shaders,
  but naga's validator lets any stage use it, so a backend's compiler or
  driver would meet it in a vertex or fragment shader); redeclare no name
  WGSL predeclares as a built-in function or type (naga would resolve
  Celesta's own calls to it); declare no name starting with `celesta` in
  any letter case (`Celesta_x` too, since Celesta's entry code declares
  `Celesta…` types); define `material_fragment`, and
  `material_outline_vertex` only with `material_outline_fragment`; and
  declare `Params` as a struct of at most 512 bytes
  (`MAX_MATERIAL_PARAMS`), its size naga's layout's. An error inside the
  material's WGSL is located as `{line}:{column}: {message}` in that WGSL;
  one in the prelude or the entry points says what the interface expects
  there. The message names the first material using the shader
  (`material {m}'s shader {s} ({name}): …`), or the shader alone when no
  material uses it.
- **Compiled** from naga's module through wgpu's safe
  `create_shader_module`, never `create_shader_module_trusted`. In wgpu 30
  the safe call always compiles with `ShaderRuntimeChecks::checked()`,
  every runtime check wgpu has: bounds checks on indexing, buffer access,
  and texel loads (naga's, or the driver's robust access where the backend
  has it), and loop bounding, so a backend compiler cannot take a loop
  that never ends as unreachable code. Nothing in Celesta sets these; they
  are what the safe call means. Modules and pipelines are made inside
  validation, internal, and out-of-memory error scopes together: a backend
  compiler refusing what naga accepted (Metal past a fragment function's
  stack, for one) is the material's error, never a panic.
- **Pipelines on open:** a model whose materials need more than 1,024
  distinct draw pipelines (`MAX_MODEL_PIPELINES`, in the GPU renderer's
  `scene.rs`) is refused before any is made (`the model's materials need
  {n} pipelines, more than 1024`). A shader has at most 16 surface
  pipelines (4 alpha modes, 2 cullings, 2 windings) and, for each depth
  bias its materials' outlines use, at most 24 outline pipelines (4 alpha
  modes, 3 cullings, 2 windings). The cap does not hold every combination:
  the surface pipelines of all 32 plugin shaders and the unlit one are
  528, and one bias's outlines for all 32 are 768 more.
- **Not bounded: a material's own memory and time.** Private and
  function-local memory (`var<private>`, a function's `var`) has no cap of
  Celesta's: naga accepts arrays of any size. A backend may refuse the
  pipeline (Metal past a fragment function's stack, refused as above: an
  Apple M5 makes a fragment of 12,288 private `vec4f`, 192 KiB, and
  refuses one of 16,384, 256 KiB), make it (the macOS CI runner's Metal
  makes one of 16 MiB), or
  give each invocation the scratch memory it asks for, which can run the
  device out of memory or, on Vulkan or DX12 (not tried), may lose the
  device when it draws. Loop bounding keeps a long loop defined, not
  short: a material that loops long enough hangs the GPU until the
  operating system resets it. GPU hangs and device loss a hostile plugin
  causes this way are out of scope: a plugin is a program the user
  installs and allows, which can already use the GPU as it likes. On the
  CPU, naga's WGSL parser caps its recursion depth (nested expressions and
  statements; 200 in naga 30): deeper nesting is refused as a parse error
  (`Parser recursion limit exceeded`), not a stack overflow.
- **Parameters on open:** a custom material's `params` must be as long as
  its shader's `Params` (`material {m}'s parameters are {a} bytes, but
  shader {s}'s Params is {b}`).
- **Parameters each frame:** a draw's `params` must be as long as its
  material's (`draw {d}'s parameters are {a} bytes, but its material
  {m}'s shader {s} takes {b}`); `params` on an `unlit` draw are refused
  (`… is unlit, which takes no parameters`), and so are `overrides` on a
  custom draw (`… is custom: overrides are for unlit materials`).

2D custom shaders (`defineShader`, `shader.rs`) share the declaration
checks (`wgsl.rs`), and change with them: a `var<workgroup>` in one is now
refused (it drew on Metal before), and a name starting with `celesta` is
now refused in an override, an alias, or a constant of abstract type too
(`alias celesta_t = f32;` was accepted before). A 2D shader's prefix check
stayed case-sensitive here; since
[`2026-11-07-shader-device-refusals.md`](2026-11-07-shader-device-refusals.md)
it covers every letter case, as a material's does, and 2D shaders compile
in the same error scopes.

## Per-frame parameters

`DrawState::params` (`params` in a frame's draw) replaces the draw's
custom material's `params` for that draw in that frame only: a later frame
without them draws the material's own, and two draws of one material each
show their own. The plugin packs whatever its material animates (colors,
texture transforms, a time-dependent scroll) into them. `MaterialOverrides`
are the unlit material's per-frame values.

## Lights

`SceneFrame::lights`, sent since the first phase, now reaches the
materials as `frame.lights` and `frame.ambient`.

- `directional`: the first is the main light. Each lights along
  `L = −normalize(direction)` with `color × intensity`
  (`Light.to_light` and `Light.color`). A frame has at most 4 (Unity's
  default pixel light count, `MAX_LIGHTS`); more, or a light whose
  `direction` has no length, is `InvalidPluginContent`. A frame without
  lights has `light_count` 0 and a black main light, as Unity's
  `_LightColor0` is without a directional light.
- `ambient`: nine RGB coefficients of
  `SH(n) = c0·x + c1·y + c2·z + c3 + c4·xy + c5·yz + c6·z² + c7·zx + c8·(x² − y²)`
  for a unit normal `n` in the scene's axes, which `ambient_light`
  evaluates: Unity's `ShadeSH9` order and scale (`unity_SHAr`/`Ag`/`Ab`
  xyzw, then `unity_SHBr`/`Bg`/`Bb` xyzw, then `unity_SHC` rgb). A flat
  ambient color is `c3` alone.
- Unity's X is mirrored against glTF's, so values copied from a Unity
  scene negate `c0`, `c4`, and `c7`, and a light's x.
- `unlit` ignores lights. A custom material uses what it reads.

## Render queues and sorting

- A draw's queue follows its material's alpha mode, for every material
  kind, as Unity's render queues: `opaque` 2000; `mask` 2450; `blend` 3000
  plus `queueOffset` clamped to −9..0 (2991 to 3000), depth not written;
  `blend` with `depthWrite` 2501 plus `queueOffset` clamped to 0..9 (2501
  to 2510), depth written.
- Lower queues draw first. Both blend queues (2501 and up) go back to
  front, as Unity sorts transparent draws: by the squared distance from the
  eye to the draw's `center` (or its rest-bounds centre moved by `world`)
  through a perspective camera, and by view-space z through an orthographic
  one. Ties, and the opaque and mask queues, go by draw index.
- Every scene pipeline tests depth with `LessEqual` (Unity's
  `ZTest LEqual`): of two exactly coplanar draws, the later in the queue
  shows. Before, the earlier one did.

## Outlines

- A custom material with `outline` has an outline pass, drawn right after
  the draw's surface in the same queue slot, from the same skinned
  vertices, draw uniform, parameters, and textures, with the material's
  alpha mode. A material asking for an outline whose shader defines no
  `material_outline_fragment` is refused.
- The pass culls what `outline.cull` says, whatever `doubleSided` is (by
  default front faces, so the pass shows the back faces it pushes out),
  and biases its depth by `outline.depthBias` (by default Unity's
  `Offset 1, 1`, constant 1 and slope 1, away from the camera, so the
  surface stays in front of its own outline).
- `material_outline_vertex` moves each vertex: `VertexOffset.world` is
  added to the world position before it is projected (a width in metres,
  whatever `world`'s scale), and `VertexOffset.clip` to the projected
  `xy` (a width on screen, which a material scales by `clip.w` to keep it
  constant). `Fragment.position` is the moved position.
  `material_outline_fragment` colors it.
- An outline pushed along vertex normals breaks at hard edges: vertices
  that do not share a normal move apart. A model whose outline must close
  shares its normals across the edge.

## How Celesta draws it

`crates/gpu-renderer/src/scene.rs`, `scene_material.rs`, the
`scene_material_*.wgsl` files, and `scene_unlit.wgsl`:

- **Bind groups.** Group 0 is the frame uniform (`FrameUniform`, the
  prelude's `Frame`, 512 bytes). Group 1 is the draw's slot in one buffer,
  at two dynamic offsets: the draw uniform (`DrawUniform`, the prelude's
  `Draw`, 128 bytes) at the slot's start and the material's parameters at
  128 aligned to the GPU's uniform offset alignment, always bound as 512
  bytes; a slot is 768 bytes at the usual 256-byte alignment. Every byte of
  every slot is written each frame (parameters zero-padded), so a slot
  shows nothing of an earlier frame or another draw. Group 2 is the
  material's texture slots, made once when the model is opened: slot `k`'s
  texture at binding `2k` with its own sampler at `2k + 1`, or the slot's
  empty texel with the default sampler.
- **The target.** Every material draws on the canvas `scene_target`
  chose when the renderer was made (`2026-11-03-plugin-scene-content.md`):
  `Rgba16Float`, else `Rgba8Unorm`, at 4 samples where the GPU can
  multisample it, else at 1. Every draw pipeline, surface and outline, of every alpha mode
  and winding, unlit's too, takes its color format and sample count, and
  sets `scene_material_entry.wgsl`'s `override CELESTA_SAMPLES` (4.0 by
  default) to that count, which a mask's coverage is counted over. At 1
  sample the draws go straight to the resolved canvas, with no
  multisampled color and no resolve. The target is fixed for the
  renderer's life, and so for its `ScenePipelines` and
  `SceneFrameResources`: neither cache is keyed by it.
- **Vertex buffers,** for every material: the skinned vertex (stride 48:
  position, normal, and tangent, each `Float32x4`, at locations 0 to 2)
  and the geometry's `uv0` (location 3, `Float32x2`).
- **`unlit`** is `scene_unlit.wgsl`, a material like any other: one slot
  (empty white) and a 48-byte `Params` of its base color and its texture
  transform's two rows, packed each frame with the draw's overrides. Its
  texture transform is applied per fragment.
- **The shader cache.** Each shader is checked and compiled once per
  renderer, keyed by its WGSL's SHA-256 digest and length and its slot
  count (names and empty texels do not change the module), whichever
  models list it. A shader that fails is cached as failed too, so a model
  refused every frame does not compile it again, except when the device ran
  out of memory, which the next model listing it tries again. The cache is
  bounded: when it holds 256 shaders (`MAX_CACHED_SHADERS`, eight models
  of the most shaders a model may list) and another comes, it first drops
  every failure and every compiled shader no loaded model holds, with the
  pipelines made or refused for them. A `ShaderId` is never given out
  again, so no pipeline key of a dropped shader stands for another.
- **Pipelines** are made when a model is opened, for what its materials
  use: keyed by shader, pass (surface, or outline with its depth bias),
  culling and winding (both windings, since a mirroring `world` or view
  turns its front faces clockwise), and alpha (opaque, mask, blend writing
  depth, blend). The loaded model holds the pipelines it draws with. Each
  pipeline is made in its own error scope: when one fails, the message
  names the material (`material {m}'s pipelines: …`) and none of that
  material's new pipelines is kept. A pipeline the device refused (a
  validation or internal error, such as a backend's compiler refusing
  what naga accepted) is remembered by what the backend compiled, its
  shader and its two entry points, while its shader is cached: culling,
  winding, depth bias, depth writes, and blending are the pipeline's state
  around the compiled stages. A model sent again, or with another depth
  bias or culling, is refused with the same message without making it
  again: a pipeline it lacks of refused stages refuses it before any of
  its pipelines, other materials' too, is made. A shader has at most one
  refusal for each pair of entry points. One that ran out of memory is not remembered and is tried
  again. A model whose materials need more than 1,024 pipelines
  (`MAX_MODEL_PIPELINES`) is refused before any is made, so distinct
  depth biases cannot make pipelines without end in one load. When the
  cache holds 256 pipelines
  (`MAX_CACHED_PIPELINES`), a model being opened first drops those no
  loaded model holds, before it makes any: the pipelines of models whose
  load failed after them (a later material's parameters, a texture, a
  buffer limit).
- **Both bounds are soft.** Shaders and pipelines that loaded models hold
  are never dropped, however many loaded models there are; the bounds are
  on what models that failed or are gone leave behind. A model is gone
  once no layer drew it in `MODEL_IDLE_RENDERS` (300) renders in a row
  (see [`2026-11-06-plugin-paths-model-eviction.md`](2026-11-06-plugin-paths-model-eviction.md)). Each model holds at
  most 32 plugin shaders and the unlit one, and 1,024 pipelines.
- **Another provider opens its own models.** Models are kept by plugin and
  asset paths, under the id the provider gave them.
  `with_content_provider` on a renderer that already opened models closes
  them on the previous provider (`ContentProvider::close`) and forgets
  them, so the next frame opens the model from the new provider, and the
  shaders and pipelines only they held can be dropped. Until then, the
  renderer kept drawing the previous provider's model and asked the new
  one for frames of an id it never gave. The editor and the exporter
  install their provider once, when the renderer is made; tests replace
  it.
- **The normal matrix** is the cofactor matrix of `world`'s upper 3×3
  times the sign of its determinant (+1 for 0), worked out in `f64` and
  divided by its largest entry: the inverse transpose up to a positive
  scale, finite for a `world` of scale 0 or of any size. `world`'s
  handedness (−1 when its upper 3×3 mirrors) is `draw.handedness`, folded
  into `Vertex.tangent.w`. Skinning moves normals and tangents by the skin
  matrices' 3×3, as Unity's does; a skin matrix that mirrors does not flip
  the bitangent's sign, as in Unity.
- **The frame resource cache** (`SceneFrameResources`). The multisampled
  color (none at 1 sample) and depth and the resolved color, with the bind
  group that converts them, are kept by canvas size. A frame keeps every size it
  draws; a size the last frame did not draw is dropped when the next one
  begins. Past 16,777,216 texels of kept canvases (`SCENE_TARGET_TEXELS`,
  two capped canvases, about 1 GB), a new size first drops the sizes the
  current frame has not drawn; the sizes it has drawn stay even over the
  budget. Two layers of one size share the canvas, each converted to its
  own layer texture. The skinning matrices and morph weights, the
  per-draw skinning parameters and draw slots, and the frame uniform are
  buffers that grow to the next power of two (at least 256 bytes) when a
  frame does not fit, are rewritten each frame, and do not shrink; their
  bind groups are made again only when a buffer is. After a frame without
  a scene layer, all of it is let go. A scene layer marks the size its
  model's last frame was drawn at as drawn before the plugin is asked for
  its frame, so a frame that the plugin fails, that is refused, or that
  has nothing to draw keeps the canvas and buffers for the next; a frame
  that sets another canvas (`2026-11-03-plugin-scene-content.md`) marks
  that size too once it arrives. So when a layer's canvas changes, the
  old size is kept through the frame after the change and dropped as the
  one after that begins: up to one capped canvas (about 500 MB) for one
  frame more, which the two capped canvases of `SCENE_TARGET_TEXELS` hold.

## Protocol (still version 1)

| Message | Change |
| --- | --- |
| `opened`, `kind: "scene"` | Header: `shaders` (`[{ name?, wgsl, textures?: [{ name?, empty? }] }]`, default `[]`) and `materialParams`, the byte count of each material's parameters, or `[]` when no material has any. A material's `kind` can be `"custom"` with `shader`, `textures`, `outline`, and `requiresTangents`; a `blend` material can have `depthWrite` and `queueOffset`. Payload: after the inline textures' pixels, each material's parameter bytes, in order. |
| `frame` for a scene | A draw can have `params`, its parameters' byte count, left out when it has none. Payload: each draw's parameter bytes follow that draw's morph weights. |

A `materialParams` whose length is neither 0 nor the material count, or
that gives bytes to a material that is not custom, is
`ProtocolError::Malformed`; so are parameter bytes past the payload's end.
Lengths are otherwise the renderer's to check. A plugin that sends only
unlit materials is unaffected over the wire. Rust code meets
`MaterialKind::Custom`, `AlphaMode::Blend`'s two fields
(`AlphaMode::BLEND` is the plain blend), `SceneModel::shaders`, and
`DrawState::params`.

## The turntable sample

`examples/plugins/turntable`'s cube is drawn by the sample's own WGSL toon
material (`src/toon.wgsl`, the shader `turntable toon` with one slot,
`checker`, empty white), opaque and single-sided, with an outline pass
(`OutlinePass::default()`). Its 64-byte `Params`:

| Member | Value | What it does |
| --- | --- | --- |
| `shade` | (0.4, 0.42, 0.75, −0.05) | The shade color (times the checker), a bluish lilac, and the shading shift. |
| `rim` | (0.25, 0.25, 0.32, 8) | A faint blue-grey rim and its fresnel power. |
| `outline` | (0.01, 0.008, 0.012, 0.5) | A near-black outline, half shaded like the surface. |
| `shape` | (0.95, 0.02, 0, 0) | Toony, and the outline's width: 2% of the canvas's height where a metre fills it. |

The material shades the checker from lit to shade with
`linearstep(−1 + toony, 1 − toony, N·L + shift)` under the main light,
adds the checker times the ambient light and a rim
`pow(saturate(1 − N·V), power) × rim × (C₀ + ambient)`, and grows its
outline on screen: the clip offset `normalize(n_clip) × 2 × width ×
min(clip.w, P[1][1])`, its x times `height_over_width`, times
`1 − n_view.z²`, about 5 of the layer's pixels at the widest. Each frame
lights the cube with one white directional light of intensity 0.8
travelling along (−0.65, −0.65, 0.3), from above, behind, and to the
right, and a flat ambient `ambient[3]` = (0.22, 0.22, 0.24): the top and a
face turned right are lit, a face turned left or toward the camera is in
shade. Light and ambient add up to about 1 (1.02, and the rim adds to
that), so the checker's light squares stay below white, though a lit white
texel would not. The pane and the leaf stay
unlit. The cube's faces keep their own normals, so its outline breaks at
the corners; the README says so.

## What landed

| Where | What |
| --- | --- |
| `crates/plugin/src/scene.rs` | `SceneModel::shaders`, `SceneShader`, `ShaderTexture`, `EmptyTexel`, `MaterialKind::Custom`, `CustomMaterial`, `OutlinePass`, `CullFace`, `DepthBias`, `DrawState::params`; `AlphaMode::Blend { depth_write, queue_offset }` and `AlphaMode::BLEND`; the limits; the `Lights` SH polynomial and light direction documented. |
| `crates/plugin/src/protocol.rs` | `shaders` and `materialParams` in a scene's `opened`, `params` in a frame's draws, their bytes in the payloads, and round-trip and malformed-payload tests. |
| `crates/gpu-renderer/src/wgsl.rs` | Locating WGSL errors in an author's code and checking its declarations (entry points, resources, workgroup variables, `celesta` names, aliases read from the source), shared by custom shaders (`shader.rs`) and materials. |
| `crates/gpu-renderer/src/scene_material.rs` | `check_material`: assembling a material's module and checking it with naga. |
| `crates/gpu-renderer/src/scene_material_prelude.wgsl`, `scene_material_entry.wgsl`, `scene_material_outline_entry.wgsl` | The interface: the prelude, and the surface and outline entry points for each alpha mode. |
| `crates/gpu-renderer/src/scene_unlit.wgsl` | The built-in unlit material against the interface. |
| `crates/gpu-renderer/src/scene.wgsl` | Only the conversion to the layer texture remains. |
| `crates/gpu-renderer/src/scene.rs` | The frame uniform and draw slots, the bounded shader and pipeline caches with refusals remembered by stages, a cap on one model's pipelines, material bind groups and pipelines made on open, outline passes, per-frame parameters, the normal matrix, render queues with offsets and distance sorting, `LessEqual`, and `SceneFrameResources`. |
| `crates/gpu-renderer/src/mesh.rs`, `prepare.rs` | A model's material and pipeline errors as `InvalidPluginContent`; the renderer keeps `SceneFrameResources`, and each frame begins it; another content provider closes the previous one's models (`MeshState::replace_provider`). |
| `examples/plugins/turntable` | The cube's own toon material (`src/toon.wgsl`), its light, and tests. |

## For plugins

What a plugin should handle, or Celesta should change once it shows. Each
item ends with what the VRM plugin does about it
(`2026-11-03-plugin-vrm.md`).

- **Transparent sorting per renderer.** Unity sorts a renderer's
  transparent submeshes as one, by one centre, and draws them in submesh
  order. Celesta sorts each draw by its own: a plugin should send one
  `center` for all of a mesh's parts and list them in order (ties go by
  draw index). A skinned draw's default centre, its rest bounds moved by
  `world`, does not follow the pose, since `world` is the identity for a
  skinned mesh; a plugin should send `center` for skinned draws. The VRM
  plugin does: one `center` per mesh, following the skin's skeleton, its
  primitives in order.
- **Shared accessors are copied per primitive.** Primitives that share a
  vertex accessor each upload all of it; a plugin should compact each
  primitive's vertices to those its indices use. The VRM plugin does.
- **Compute storage bindings are at the limit.** The skinning pass binds 8
  storage buffers, WebGPU's default `maxStorageBuffersPerShaderStage`; a
  new vertex stream would need a higher limit or packed streams. The VRM
  plugin sends only the streams there are, and leaves vertex colours and
  a second UV set out.
- **No byte budget on `model.textures`.** Each texture's dimensions are
  checked against the GPU's, but not their total bytes, which a model with
  many large textures can run past. The VRM plugin keeps its own: 1 GiB
  of decoded scene textures.
- **Tangents are the plugin's.** Celesta makes up none: a material that
  needs them sets `requiresTangents`, and the plugin generates MikkTSpace
  tangents for a primitive that lacks them. The VRM plugin does, for every
  primitive whose material has a normal texture.
- **A model's shaders and pipelines compile when it opens:** every shader
  it lists that is not cached, and, in both windings, each pass, culling,
  and alpha mode its materials use that is not cached, a one-off stall on
  open, most of it at the first model of a plugin. Still so for the VRM
  plugin: its MToon shader compiles when the first avatar opens.
- **Checking its own parameters.** Celesta cannot see inside the bytes: a
  plugin checks the numbers it packs (finite, in range) before sending
  them. The VRM plugin checks MToon's slots and numbers when it opens a
  model, and each frame's binds before it packs them.

## Not done

- **No custom varyings.** A material passes nothing from its vertex hooks
  to its fragment functions but what `Fragment` has; it works out per
  fragment what it needs from `Fragment`.
- **`Frame.time` is `f32`.** A plugin sends values that need the time's
  full precision in its parameters.
- **A refusal stands for its stages.** A pipeline the device refused is
  remembered by its shader and entry points, so every culling, winding,
  depth bias, and blend of those stages is refused with it. A backend that
  refused a pipeline for its state rather than its shader code would have
  the others refused too; none is known to.
- **A new pipeline is a new compile.** A plugin that opens a model with a
  new depth bias each time, and so new pipelines the device makes, costs
  one backend compile for each: at most 1,024 for one model, and kept only
  while a loaded model holds them or the cache has room.
- **Shadows.** Nothing casts a shadow. A main-light shadow map is a later
  phase, to be offered to materials through the interface.
- **Point and spot lights.** Only directional lights, at most 4; more is
  refused rather than dropped.
- **glTF PBR** is to be a plugin's material, in a later phase.
- **A material's NaN is not caught.** A material that computes NaN draws
  whatever the GPU makes of it, in its own layer only.
- **Views that are not rigid.** The eye is exact for any invertible affine
  view (a view whose bottom row is not `(0, 0, 0, 1)` is refused), but
  `frame.forward`, the orthographic `V`, is the view's third row,
  which a view of non-uniform scale or shear tilts. Such views are not
  refused.
- **The layer texture is still made each frame;** only the scene canvases
  and buffers are kept. The buffers do not shrink until a frame draws no
  scene.
- **The outline's depth-bias tests have run on Metal only.** WebGPU scales
  a float depth's constant bias by its exponent, so other backends should
  agree.
- **Masks on old GL.** The fallback targets have been drawn on Metal by
  forcing them, not on a GL adapter. A mask's entry points write
  `@builtin(sample_mask)`, which naga's GLSL backend writes as
  `gl_SampleMask`: desktop GL 4.0, or on GLES `GL_OES_sample_variables`,
  which it requires. A GL driver without it should refuse mask pipelines,
  at 1 sample too, and so the model, with `InvalidPluginContent` naming
  the material (pipelines are made in an error scope); opaque and blend
  materials draw. Mask entry points that
  discard instead at 1 sample would draw there.
- From the first phase, still open: straight-alpha mipmaps fringe cut-outs
  at distance (done since: `2026-11-08-plugin-mipmap-alpha.md`); the
  default 128 MiB storage-binding limit may refuse morph-heavy geometry;
  the canvas (`SCENE_CANVAS_TEXELS`) and buffer (`SCENE_BUFFER_BUDGET`)
  caps; no Add-menu entry for `<Turntable>`; the CPU renderer and `@celesta/web` refuse plugin content; the out-of-tree
  Live2D plugin's interface update; zero-weight joint slots counting toward
  the skin check; no test for a reopen of the other kind.

## Checks

- `celesta-plugin`'s tests read a blend without `depthWrite` and
  `queueOffset` as `AlphaMode::BLEND` and round-trip one with them; check
  the JSON names of a custom material (no `params` in the JSON; an empty
  `outline` as the defaults) and of shaders and their empty texels;
  round-trip a model with two shaders, custom materials of 16 and 32 bytes
  of parameters, and an unlit one (`materialParams` `[16, 32, 0]`, the
  bytes after the pixels), a model without parameters
  (`materialParams` `[]`), and a frame whose draws have parameters or not;
  and refuse a `materialParams` of the wrong length, bytes for an unlit
  material, and parameter bytes past the payload.
- `scene_material.rs`'s unit tests check a minimal material (its
  assembled module's one override `CELESTA_SAMPLES`), `Params`'
  size by WGSL's layout, the outline functions, errors located in the
  material's WGSL, wrong signatures naming the interface, missing parts,
  the material's own resources, entry points, overrides, workgroup
  variables (alone, and written in `material_vertex`), built-in names, and
  `celesta` names in any letter case refused, vertex hooks refused
  implicit-LOD sampling, slots declared by count, and the limits.
- The 2D custom shader tests (`crates/gpu-renderer/src/tests/shader.rs`)
  refuse a workgroup variable an `effect` reads and writes, and `celesta`
  names in overrides, aliases, and constants of abstract type, located in
  the author's code.
- Unit tests in `scene.rs` check that the frame and draw uniforms match
  the prelude's layout, that the built-in unlit material passes the
  material checks, that an unlit material's base color and texture
  transform must be finite, unlit's parameter packing, the render queue's
  clamping of blend offsets, the signed-cofactor normal matrix, the eye
  of a view, the draw slot's size and the per-draw uniform check, that
  `make_room` drops only sizes not drawn, only past the budget, and which
  target `scene_target` picks for what a GPU's formats lack.
- The GPU renderer's scene tests (`crates/gpu-renderer/src/tests/scene.rs`)
  draw made-up scenes, mostly a square seen from +Z with small test
  materials written against the interface, and compare pixels with values
  worked out by hand:
  - `material_`: parameters coloring the surface; texture slots sampling
    their texture or their empty texel; lights and ambient reaching the
    shader, and more lights than the frame holds or a light without a
    direction refused; each light its color times its own intensity (0.5,
    0.25, and 0.75), in order past the first, and a light the frame lacks
    black; the light count, the time (0.25), and the height over the width
    of a 128x64 canvas (0.5);
    the view and the projection apart, by the view's translation and the
    projection's `[1][1]`; each SH coefficient; normals through the inverse
    transpose of `world`; tangents and handedness following `world`
    (drawn with `requiresTangents` set), and tangents of 0 without them;
    `material_vertex` moving the surface in world and clip space; facing,
    and a back face's normal not turned to the viewer;
    the view direction, orthographic and perspective; `linearstep` and
    `safe_normalize`; the alpha modes around the returned alpha, with the
    near-clear discard of a depth-writing blend; degenerate worlds without
    NaN; broken WGSL refused naming the material and the line; parameters
    that do not fit their shader; the limits; tangents required on
    geometry without them; on Metal only, 16 MiB of private memory a
    fragment drawn or refused naming the material, whichever the device
    does; a pipeline the device refused (the test's hook, on every
    backend) refused naming the material and not made again, and a model
    of another shader drawn after it, while one that ran out of memory is
    made again;
    a refusal standing for every depth bias and culling of its stages,
    none of them made and nothing more kept; a model whose later material
    has refused stages refused, again and again, without making an earlier
    material's pipelines; a model needing 1,026
    pipelines refused before any is made, and one needing 1,024 not;
    a shader compiled once for many materials, the same WGSL with one
    texture slot and with two compiled apart and both drawn, and a broken
    one once for many frames; a shader the device ran out of memory
    compiling tried again; and the shader and pipeline caches dropping
    what no loaded model holds once past their caps, keeping what a loaded
    model holds.
  - `frame_params_`: a frame's parameters replacing the material's for
    that frame only, per draw, and refused when they do not fit, on an
    unlit draw, or beside overrides on a custom draw.
  - `queue_`: offsets before distance; a depth-writing blend drawn first
    and hiding what is behind; near-clear blend pixels discarded;
    perspective sorting by distance and orthographic by view depth;
    coplanar opaque draws, the later winning.
  - `outline_`: world and clip offsets; texture slots reaching the vertex
    hook; culling and bias as asked, and, on a single square facing the
    camera, each `cull` value and a mirrored world culling the front face;
    mirrored worlds still culling the front faces of a two-sided shell;
    the material's alpha; an outline without `material_outline_fragment`
    and a bias that is not finite refused; every alpha mode (`depthWrite`
    included) × side × no outline, world, or clip offset opened and drawn
    in both windings.
  - `override_not_finite_is_refused`: each of an unlit draw's overrides
    (base color, texture offset, texture scale) refused when not finite,
    naming the draw.
  - `every_target_draws_the_same_scenes`: on `Rgba16Float` × 4 and each
    fallback forced in its place (`Rgba8Unorm` × 4 and × 1, `Rgba16Float`
    × 1), a white square, a custom material's parameters, unlit and
    custom masks at their cutoff, blend over blend and over opaque, and an
    outline come out as on the usual target; an edge and a mask's edge
    (`RAMP`, a quarter of its samples covered) are partly covered at 4
    samples and covered or not at 1. `scene_without_a_target_is_refused`:
    a GPU with neither format refuses scene content.
  - `mirrored_views_keep_front_faces`: a view mirrored in x shows the
    square's front face and culls its back, also with a mirrored world.
  - `scene_resources_`, `scene_frame_buffers_`: canvases and buffers
    reused between frames, following the canvas size, kept for every size
    a frame draws, dropped over the budget when unused, released after a
    frame without a scene, kept through a frame that is refused or draws
    nothing, grown and rebound when a frame outgrows them; two layers of
    one size keeping their own pixels.
  - `a_new_content_provider_opens_its_own_models`: a red model drawn,
    then another provider's green one of the same plugin and assets drawn
    in its place; `a_new_content_provider_closes_the_models_the_previous_one_opened`:
    the drawn model closed once, on its own provider, and a model whose
    load failed not closed again.
  - The first phase's scene tests, unlit materials and overrides
    included, pass with their expected pixels unchanged.
- `celesta-plugin-turntable`'s unit tests check that the cube is drawn by
  its own shader (one slot, the checker; an outline pass; 64 bytes of
  parameters), the other materials unlit, and that every frame has one
  light and a flat ambient. Its process tests draw the program's scene,
  compiling `toon.wgsl` on the GPU.
