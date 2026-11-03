# MToon, lights, and outlines for scene content (2026-11-03)

A scene's materials can now be MToon, VRM 1.0's toon shading
(`VRMC_materials_mtoon`), drawn as UniVRM's MToon10 shader draws it in
Unity's built-in render pipeline: shaded from lit to shade under the
frame's directional lights, lit by spherical harmonics ambient light, with
emission, a rim, a matcap, normal maps, UV animation, and outlines. The
render queues and the sorting of blended draws follow UniVRM's, and each
frame can override MToon's colors as VRM's material animation does. Scene
frames keep their canvases and buffers from frame to frame.

This is the second phase of the design in
`docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md`. The
first, `2026-11-03-plugin-scene-content.md`, has the scene protocol, the 3D
pass, unlit materials, and notices; what it says still holds unless this
note says otherwise. The VRM plugin comes later.

## MToon materials

`MaterialKind::Mtoon(MtoonMaterial)` (`"kind": "mtoon"`) sits beside
`unlit`; a material's `alphaMode` and `doubleSided` work for it as for
unlit. Every field may be left out and then takes the default UniVRM's
importer gives it (`MtoonMaterial::default()`), so a plugin sends what the
file has.

| Field | Default | What it does |
| --- | --- | --- |
| `baseColor` | `[1, 1, 1, 1]` | Linear RGBA, times `baseColorTexture`: the lit color and the alpha. |
| `baseColorTexture` | none | `{ texture, transform? }`; its transform moves the other textures too (below). |
| `normalTexture` | none | `{ texture, scale = 1 }`, a tangent-space normal map. |
| `emissiveFactor` | `[0, 0, 0]` | Linear RGB, times `emissiveTexture`. The plugin multiplies `KHR_materials_emissive_strength` in. |
| `emissiveTexture` | none | `{ texture }`. |
| `transparentWithZWrite` | `false` | A blended material also writes depth. |
| `renderQueueOffsetNumber` | `0` | Moves a blended material within its queue. |
| `shadeColorFactor` | `[1, 1, 1]` | The shaded side's color, times `shadeMultiplyTexture`. |
| `shadeMultiplyTexture` | none | `{ texture }`. |
| `shadingShiftFactor` | `-0.05` | Shifts where light turns to shade. |
| `shadingShiftTexture` | none | `{ texture, scale = 1 }`: its red times `scale` is added to the shift. |
| `shadingToonyFactor` | `0.95` | How hard the edge is, 0 smooth to 1 a step; above 1 the ramp turns around. |
| `giEqualizationFactor` | `0.9` | How far the ambient light is flattened. |
| `matcapFactor` | `[0, 0, 0]` | Linear RGB, times `matcapTexture`. |
| `matcapTexture` | none | `{ texture }`. |
| `parametricRimColorFactor` | `[0, 0, 0]` | The rim's color. |
| `rimMultiplyTexture` | none | `{ texture }`, scaling the rim and the matcap. |
| `rimLightingMixFactor` | `1` | How far the rim follows the scene's light. |
| `parametricRimFresnelPowerFactor` | `5` | The rim's falloff. |
| `parametricRimLiftFactor` | `0` | Widens the rim. |
| `outlineWidthMode` | `none` | `none`, `worldCoordinates`, or `screenCoordinates`. |
| `outlineWidthFactor` | `0` | Metres, or a share of the canvas's height. |
| `outlineWidthMultiplyTexture` | none | `{ texture }`: its green scales the width. |
| `outlineColorFactor` | `[0, 0, 0]` | The outline's color. |
| `outlineLightingMixFactor` | `1` | How far the outline is shaded like the surface. |
| `uvAnimationMaskTexture` | none | `{ texture }`: its blue scales the animation. |
| `uvAnimationScrollXSpeedFactor` | `0` | Texture widths a second. |
| `uvAnimationScrollYSpeedFactor` | `0` | Texture heights a second, down the texture. |
| `uvAnimationRotationSpeedFactor` | `0` | Radians a second. |

- **Texture coordinates.** As UniVRM's single `_MainTex_ST`, every texture
  but the matcap and the outline width is sampled at the base color
  texture's coordinates: through `baseColorTexture.transform` (and the
  frame's `textureOffset` and `textureScale` overrides), then the UV
  animation. That is why the other slots (`TextureSlot`,
  `ScaledTextureSlot`) have no transform. The outline width texture is
  sampled at the geometry's raw coordinates; the matcap at its own.
- **Channels and color spaces.** The shading shift texture is read from its
  red channel, the outline width texture from its green, and the UV
  animation mask from its blue; the others' RGB is used. The plugin sets
  each texture's `colorSpace`: sRGB for the base color, shade multiply,
  emissive, matcap, and rim multiply textures, linear for the normal,
  shading shift, outline width, and mask textures.
- **Empty slots** bind Unity's default texel for the property, which leaves
  each term as UniVRM's keyword gates do without the texture: black for the
  shading shift and the matcap (no shift but the factor's, no matcap),
  white for the rest. An empty normal slot is not read.
- **Checked on open.** Each slot's texture must exist
  (`material {i}'s shadeMultiplyTexture uses texture {t}, of {n}`), every
  MToon number must be finite, and a draw whose material has a
  `normalTexture` needs a geometry with tangents: none are made up.
  Anything else is `InvalidPluginContent`.

## Lights

`SceneFrame::lights`, sent since the first phase, is now used.

- `directional`: the first is the main light. Each lights along
  `L = −normalize(direction)` with `color × intensity`. A frame has at most
  4 (Unity's default pixel light count); more, or a light whose
  `direction` has no length, is `InvalidPluginContent`. A frame without
  lights has a black main light, as Unity's `_LightColor0` is without a
  directional light.
- `ambient`: nine RGB coefficients of
  `SH(n) = c0·x + c1·y + c2·z + c3 + c4·xy + c5·yz + c6·z² + c7·zx + c8·(x² − y²)`
  for a unit normal `n` in the scene's axes: Unity's `ShadeSH9` order and
  scale (`unity_SHAr`/`Ag`/`Ab` xyzw, then `unity_SHBr`/`Bg`/`Bb` xyzw,
  then `unity_SHC` rgb). A flat ambient color is `c3` alone.
- UniVRM mirrors X between glTF and Unity: coefficients taken from a Unity
  scene negate `c0`, `c4`, and `c7`, and its light directions negate x.
- `time` is now used by the UV animation.

## How MToon shades

`crates/gpu-renderer/src/scene_mtoon.wgsl`, per fragment, in world space:

- **The normal** `N` is the vertex normal through the draw's normal matrix
  (below), turned to face the viewer on a back face (which only a
  double-sided material draws), then tilted by the normal texture: decoded
  as Unity's `UnpackNormalWithScale` reads its desktop layout (red and green
  times `scale`, blue unread), in the tangent space of UniVRM's
  `MToon_GetTangentToWorld`: the tangent through `world`'s 3×3 (not the
  normal matrix), the bitangent's sign times `world`'s handedness.
- **The view direction** `V` is towards the eye through a perspective
  camera and the camera's +Z through an orthographic one. The eye is where
  the view takes the origin from (`−M⁻¹t` of the view's 3×3 `M` and
  translation `t`).
- **The main light.** `lit = baseColor × baseColorTexture`,
  `shade = shadeColorFactor × shadeMultiplyTexture`,
  `shift = shadingShiftFactor + shadingShiftTexture.r × scale`, and
  `color = mix(shade, lit, linearstep(−1 + toony, 1 − toony, N·L + shift)) × C₀`.
  `linearstep(a, b, t)` is UniVRM's `saturate((t − a) / (b − a))`, so above
  `toony` 1 the ramp turns around (lit below `b`, shade above `a`). At
  `toony` 1, `a = b` and the step is hard, 0 up to `a` and 1 above, as
  Unity's division by 0 gives.
- **Ambient.** `color += lit × mix(SH(N), ½(SH(+Y) + SH(−Y)), giEqualization)`.
- **Emission.** `color += emissiveFactor × emissiveTexture`.
- **Rim.** `rim = (matcap + parametric) × rimMultiplyTexture`, with
  `parametric = pow(saturate(1 − N·V + lift), max(power, 2⁻¹⁰)) × rimColor`
  (0 where the base is 0) and the matcap sampled at the view-space normal:
  `right = normalize(cross(+Y, V))`, `up = normalize(cross(V, right))` (the
  view's x axis as `right` when looking along ±Y), at
  `(N·right × ½ + ½, ½ − N·up × ½)`, so a normal leaning right samples the
  texture's right and one leaning up its top row.
  `color += rim × mix(1, C₀ + SH(N), rimLightingMix)`.
- **Additional lights** (the second to the fourth) are added in the same
  fragment, as Unity's ForwardAdd pass adds them: each adds
  `mix(shade, lit, step) × Cᵢ × shadow` and `rim × Cᵢ × shadow × rimLightingMix`,
  with `shadow = ½(min(0, N·L) + 1)` outside the step; no ambient or
  emission.
- There are no shadows: Unity's `atten` is 1.
- **UV animation**, as UniVRM's `GetMToonGeometry_Uv` after the texture
  transform, in glTF's `v`-down texture space: the mask's blue at the
  transformed coordinates scales the time; the coordinates scroll by the
  time times the scroll speeds, then turn about the texture's centre by the
  time times the rotation speed (after a quarter turn, `(u, v)` samples
  `(v, 1 − u)`).
- **Alpha.** Mask turns the base alpha into sample coverage around the
  cutoff, as unlit does. Blend blends straight color, as unlit does, and
  discards alpha below 2⁻¹⁰ as UniVRM does (vrm-c/UniVRM#2598), so that a
  clear pixel of a `transparentWithZWrite` material writes no depth.

## Render queues and sorting

- A draw's queue is UniVRM's (`MToonValidator`): opaque 2000, mask 2450,
  blend 3000. A blended MToon material with `transparentWithZWrite` is at
  2501 and writes depth. `renderQueueOffsetNumber` moves only blended MToon
  materials: clamped to 0..9 with `transparentWithZWrite` (2501 to 2510)
  and to −9..0 without (2991 to 3000). Unlit materials have no offset.
- Lower queues draw first. Both blend queues (2501 and up) go back to
  front, as Unity sorts transparent draws: by the squared distance from the
  eye to the draw's `center` (or its rest-bounds centre moved by `world`)
  through a perspective camera, and by view-space z through an orthographic
  one. Ties, and the opaque and mask queues, go by draw index.
- Every scene pipeline, unlit included, now tests depth with `LessEqual`
  (Unity's `ZTest LEqual`): of two exactly coplanar draws, the later in the
  queue shows. Before, the earlier one did.

## Outlines

- An MToon material whose `outlineWidthMode` is not `none` has an outline,
  drawn right after the draw's surface in the same queue slot, from the
  same skinned vertices and textures. It is drawn whatever the width, 0
  included, as UniVRM draws it.
- The outline pass culls front faces whatever `doubleSided` is (UniVRM's
  `Cull Front`) and pushes vertices out along their world normal, its width
  `outlineWidthFactor` times the width texture's green at the raw
  coordinates. Its depth is biased away from the camera (Unity's
  `Offset 1, 1`: constant 1, slope 1), so the surface stays in front of its
  own outline.
- `worldCoordinates`: the width is in metres, whatever `world`'s scale.
- `screenCoordinates`: the width is a share of the canvas's height, applied
  in clip space, corrected for the canvas's aspect, growing with distance
  only up to the distance at which a metre fills the height
  (`min(clip.w, P[1][1])`), and fading as the normal turns to face the
  camera (`1 − n_view.z²`). A normal facing the camera does not move.
- The color is `outlineColorFactor × mix(1, shaded, outlineLightingMix)`,
  `shaded` being the surface's MToon color under the main light alone (the
  built-in pipeline draws outlines in ForwardBase only), at the outward
  normal, following the UV animation and the normal texture.
- The outline draws the material's alpha: coverage for mask, blending and
  the near-clear discard for blend.
- The outline breaks at hard edges, as in UniVRM: vertices that do not
  share a normal move apart. A model whose outline must close shares its
  normals across the edge.

## Overrides

`MaterialOverrides` gains five colors, VRM 1.0's `materialColorBind`
targets, each replacing the material's factor for that frame:
`shadeColor` (`shadeColorFactor`), `emissionColor` (`emissiveFactor`),
`matcapColor` (`matcapFactor`), `rimColor` (`parametricRimColorFactor`),
and `outlineColor` (`outlineColorFactor`). `baseColor`, `textureOffset`,
and `textureScale` act on MToon as on unlit (`textureTransformBind`
moves every texture that follows the base color texture). An unlit draw
ignores the MToon colors, but one that is not finite is refused on any
draw: the plugin that sent it is broken all the same.

## How Celesta draws it

`crates/gpu-renderer/src/scene.rs`, `scene.wgsl`, `scene_mtoon.wgsl`
(compiled after `scene.wgsl` as one module):

- **Bind groups.** Group 0 is the frame uniform (`FrameUniform`, 512
  bytes): view-projection, view, and projection; the eye (`w` 1 for
  perspective) and the camera's +Z; the time, the canvas's height over its
  width, and the light count; four lights; nine SH coefficients. Group 1 is
  the draw uniform (`DrawUniform`, 288 bytes, at a dynamic offset: 512
  bytes a draw at the usual 256-byte alignment): `world`, the normal
  matrix, the base color and texture transform, MToon's factors, the
  outline's width and mode, `world`'s handedness, and the UV animation's
  scroll and rotation. Group 2 is the material's, made once when the model
  is opened: unlit has its base color texture and sampler; MToon has nine
  slots, slot `k`'s texture at binding `2k` and its sampler at `2k + 1`
  (base color, shade multiply, shading shift, normal, emissive, matcap, rim
  multiply, outline width, mask). The outline width slot is the vertex
  stage's; the others the fragment stage's.
- **Pipelines** are made when a model is opened, for what its materials
  use: keyed by shading (unlit, MToon), pass (surface, outline), culling
  and winding (both windings, since a mirroring `world` turns its front
  faces clockwise), and alpha (opaque, mask, blend writing depth, blend).
- **The normal matrix** is the cofactor matrix of `world`'s upper 3×3
  times the sign of its determinant (+1 for 0), worked out in `f64` and
  divided by its largest entry: the inverse transpose up to a positive
  scale, finite for a `world` of scale 0 or of any size. `world`'s
  handedness (−1 when it mirrors) travels in the draw uniform for the
  bitangent's sign. Skinning still moves normals by the skin matrices'
  3×3, as Unity's does; a skin matrix that mirrors does not flip the
  bitangent, as in Unity.
- **The frame resource cache** (`SceneFrameResources`). The multisampled
  color and depth and the resolved color, with the bind group that
  converts them, are kept by canvas size. A frame keeps every size it
  draws; a size the last frame did not draw is dropped when the next one
  begins. Past 16,777,216 texels of kept canvases (`SCENE_TARGET_TEXELS`,
  two capped canvases, about 1 GB), a new size first drops the sizes the
  current frame has not drawn; the sizes it has drawn stay even over the
  budget. Two layers of one size share the canvas, each converted to its
  own layer texture. The skinning matrices and morph weights, the
  per-draw skinning parameters and draw uniforms, and the frame uniform
  are buffers that grow to the next power of two (at least 256 bytes) when
  a frame does not fit, are rewritten each frame, and do not shrink; their
  bind groups are made again only when a buffer is. After a frame that
  draws no scene, all of it is let go.

## Protocol (still version 1)

| Message | Change |
| --- | --- |
| `opened`, `kind: "scene"` | A material's `kind` can be `"mtoon"`, with the fields above (camelCase, each optional). Texture slots are `{ "texture": i }`, or `{ "texture": i, "scale": s }` for `normalTexture` and `shadingShiftTexture`. |
| `frame` for a scene | A draw's `overrides` can also hold `shadeColor`, `emissionColor`, `matcapColor`, `rimColor`, and `outlineColor` (RGB triples). |

The payloads are unchanged. A plugin that sends only unlit materials is
unaffected, but `ContentProvider` code that matches on `MaterialKind`
gains an arm, and `MaterialOverrides` built field by field needs
`..MaterialOverrides::default()`.

## The turntable sample

`examples/plugins/turntable`'s cube is now MToon, opaque and single-sided:
the checker as its lit color and, times a bluish lilac
(`shadeColorFactor` (0.4, 0.42, 0.75)), as its shaded one, with the
default shift and toony; a faint blue-grey rim (0.25, 0.25, 0.32), fresnel
power 8, no lift; and a near-black outline (0.01, 0.008, 0.012),
`screenCoordinates` 0.02, lighting mix 0.5, about 5 of the layer's pixels
at the widest. Each frame lights it with one white directional light of
intensity 0.8 travelling along (−0.65, −0.65, 0.3), from above, behind, and
to the right, and a flat ambient `ambient[3]` = (0.22, 0.22, 0.24): the top
and a face turned right are lit, a face turned left or toward the camera is
in shade, and a lit white texel stays below white. The pane and the leaf
stay unlit, so only the cube looks different. Its faces keep their own
normals, so its outline breaks at the corners; the README says so.

## What landed

| Where | What |
| --- | --- |
| `crates/plugin/src/scene.rs` | `MaterialKind::Mtoon`, `MtoonMaterial` with UniVRM's defaults, `OutlineWidthMode`, `TextureSlot`, `ScaledTextureSlot`; the five MToon colors in `MaterialOverrides`; the `Lights` SH polynomial and light direction documented. |
| `crates/plugin/src/protocol.rs` | Round-trip tests for an MToon model and the new overrides. |
| `crates/gpu-renderer/src/scene_mtoon.wgsl` | MToon's surface and outline passes. |
| `crates/gpu-renderer/src/scene.wgsl` | The frame uniform (camera, eye, lights, ambient, time) and the larger draw uniform; mask coverage shared with MToon. |
| `crates/gpu-renderer/src/scene.rs` | MToon checks, material bind groups and pipelines made on open, the normal matrix, the frame and draw uniforms, render queues with offsets and distance sorting, `LessEqual`, outlines, overrides, the UV scroll, and `SceneFrameResources`. |
| `crates/gpu-renderer/src/mesh.rs`, `prepare.rs` | The renderer keeps `SceneFrameResources`; each frame begins it. |
| `examples/plugins/turntable` | The MToon cube, its light, and tests. |

## Not done

- **Shadows.** `atten` is 1: nothing casts a shadow.
- **glTF PBR** (`MaterialKind` has `unlit` and `mtoon` only).
- **Point and spot lights.** Only directional lights, at most 4; more is
  refused rather than dropped.
- **Outlines break at hard edges,** as UniVRM's do: Celesta does not
  smooth the normals it pushes vertices along.
- **Long videos.** The scroll is worked out on the CPU in `f64` and wrapped
  by whole textures (two at a time, for a mirrored repeat) only where that
  cannot show: when the material does not rotate, has no mask, and every
  texture that follows the animation repeats along that axis. A masked or
  rotating scroll keeps `f32`'s precision, and the rotation's angle is
  formed from the `f32` time, so either drifts at very large times.
- **Views that are not rigid.** The eye is exact for any invertible view,
  but the orthographic `V` and the matcap's fallback axis come from the
  view's rows, which a view of non-uniform scale or shear skews. Such views
  are not refused.
- **The layer texture is still made each frame;** only the scene canvases
  and buffers are kept. The buffers do not shrink until a frame draws no
  scene.
- **The outline's depth-bias tests have run on Metal only.** WebGPU scales
  a float depth's constant bias by its exponent, so other backends should
  agree.
- **Pipelines of a model refused for its GPU limits** (a buffer or
  texture too large, found after the checks above pass) stay in the
  shared cache, where later models reuse them.
- From the first phase, still open: straight-alpha mipmaps fringe cut-outs
  at distance; the default 128 MiB storage-binding limit may refuse
  morph-heavy geometry; the canvas (`SCENE_CANVAS_TEXELS`) and buffer
  (`SCENE_BUFFER_BUDGET`) caps; no Add-menu entry for `<Turntable>`; the
  CPU renderer and `@celesta/web` refuse plugin content; the out-of-tree
  Live2D plugin's interface update; zero-weight joint slots counting toward
  the skin check; shared glTF accessors copied per primitive; no test for a
  reopen of the other kind.

Where Celesta knowingly differs from UniVRM, or matches it only by
reading its source, to check once UniVRM reference renders exist:

- **The mask's edge.** UniVRM clips at `alpha < cutoff` and outputs alpha
  1, so its `AlphaToMask` adds nothing; Celesta turns the alpha into
  coverage around the cutoff, sharpened to about a pixel. The edge moves by
  at most `|cutoff − ½|` of a pixel and is anti-aliased.
- **`V` per fragment.** UniVRM works out the view direction per vertex
  and interpolates it; Celesta works it out per fragment. The two differ
  most on large triangles seen from close by.
- **`KHR_texture_transform` rotation** is honoured, though UniVRM's
  `_MainTex_ST` cannot hold one.
- **Emissive strength** is folded into `emissiveFactor` by the plugin.
- **Additional lights** are summed in one fragment rather than in
  ForwardAdd passes; the blend equation gives the same result.
- **Translated into glTF's axes:** the matcap's axes (right-handed, `v`
  down), the UV animation's direction (scroll y down the texture, the turn
  reversed), and the SH and light mirroring above.
- **The screen-width outline's formula** and its cap at `P[1][1]`.
- **`linearstep` at `toony` 1**, a hard step following Unity's division
  by 0, and the turned-around ramp above 1, following UniVRM's formula.
- **Coplanar draws** under `LessEqual`, and the outline's depth bias.

## Checks

- `celesta-plugin`'s tests read an MToon material with no fields as
  UniVRM's defaults and a scaled slot without `scale` as 1, check the JSON
  names, and round-trip an MToon model with every slot and value set and a
  frame with every override through the wire.
- The GPU renderer's scene tests (`crates/gpu-renderer/src/tests/scene.rs`)
  draw made-up scenes, mostly a lit square seen from +Z, and compare pixels
  with values worked out by hand:
  - `lit_`: the main light's step by toony and shift; the shading shift
    texture's red times its scale; the shade multiply texture; a flat
    ambient; each SH coefficient; GI equalization; normals through the
    inverse transpose of `world`; back faces of double-sided materials;
    toony 1 as a hard step and above 1 as the turned-around ramp; degenerate and mirroring worlds without NaN;
    more than 4 lights and a light without a direction refused; an MToon
    texture out of range refused.
  - `shading_`: additional lights with the half-Lambert shadow; emission;
    the parametric rim's lift, power, and mix (an additional light's rim
    included), and the eye through a perspective camera; the rim multiply
    texture; the matcap's right and up, and looking straight down.
  - `surface_`: the normal texture's tilt and scale; mirrored worlds
    keeping normal maps upright; a normal texture without tangents
    refused; UV scroll and turn with time; the mask's blue; the texture
    transform before the animation; scrolling `v` down; the scroll's
    precision at long times, and wrapping only where whole textures
    vanish.
  - `queue_`: offsets before distance; `transparentWithZWrite` drawn first
    and hiding what is behind; near-clear blend pixels discarded;
    perspective sorting by distance and orthographic by view depth;
    coplanar opaque draws, the later winning. `mask_mtoon_covers_by_cutoff`
    checks MToon's mask coverage at the cutoff.
  - `outline_`: world width in metres; mirrored worlds still culling the
    front faces and keeping a mirrored shell's ring; drawn at width 0;
    screen width as a share of the height, capped past `P[1][1]`, and
    corrected for aspect; the width texture's green at the raw
    coordinates; the color's lighting mix; the main light only; the
    outward normal; following the UV animation and the normal texture;
    `none` drawing nothing; the material's blend; normals facing the
    camera staying put.
  - `override_`: each MToon color replaced; MToon colors ignored on unlit
    draws; a color that is not finite refused.
  - `scene_resources_`, `scene_frame_buffers_`: canvases and buffers
    reused between frames, following the canvas size, kept for every size
    a frame draws, dropped over the budget when unused, released after a
    frame without a scene, grown and rebound when a frame outgrows them;
    two layers of one size keeping their own pixels.
- Unit tests in `scene.rs` check the render queue's clamping and the
  offsets it ignores, the signed-cofactor normal matrix, the eye of a
  view, MToon's slot and number checks, and that `make_room` drops only
  sizes not drawn, only past the budget.
- `celesta-plugin-turntable`'s unit tests check that the cube is MToon
  with a screen-width outline and the checker as both lit and shade
  texture, the other materials unlit, and that every frame has one light
  and a flat ambient. Its process tests draw the program's scene as before.
