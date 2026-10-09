# Custom shader filters design (phase 1)

## Goal

A layer's effects are fixed today: `blur`, `shadow`, and `glow`. Anything
else — a ripple, chromatic aberration, a pixelate, a color grade, a glitch —
cannot be written in a composition. A `shader` effect lets an author write
the per-pixel function themselves, in WGSL, and apply it to any visual layer
or group the way `blur` is applied.

```tsx
import { Group, Text, useCurrentFrame, useVideoConfig } from '@celesta/react';
import { defineShader } from '@celesta/shader';
import rippleSource from './ripple.wgsl';

const ripple = defineShader({
  name: 'ripple',
  wgsl: rippleSource,
  params: { time: 'f32', amplitude: { type: 'f32', default: 6 } },
  padding: 8,
});

function Title() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return (
    <Group shader={ripple({ time: frame / fps })} shadow={{ color: '#00000080', blur: 12, offsetX: 0, offsetY: 6 }}>
      <Text>CELESTA</Text>
    </Group>
  );
}
```

```wgsl
// ripple.wgsl
fn effect(input: EffectInput) -> vec4f {
  let center = mix(celesta.content.xy, celesta.content.zw, vec2f(0.5));
  let d = input.position - center;
  let wave = sin(length(d) * 0.08 - params.time * 6.0) * params.amplitude;
  return source_at(input.position + normalize(d + vec2f(1e-4)) * wave);
}
```

This is phase 1. It covers single-input filters only. Generators (a shader
that fills a rectangle with no input), transition shaders (two inputs), and
extra texture inputs are later phases; see "Out of scope".

## Behavior

### What the shader sees

The shader filters the layer's composited pixels, exactly the pixels `blur`
filters today: the layer (or the group's children, after its mask) drawn at
opacity 1 with normal blending onto a transparent canvas. Like the other
effects, it works in output pixels after transforms. A rotated group's
ripple is centred in scene space, not in the group's own rotated space.

The author writes one function:

```wgsl
fn effect(input: EffectInput) -> vec4f
```

It runs once for every output pixel the shader can write (see "Output
area") and returns that pixel as premultiplied RGBA in 0–1, with the same
sRGB-encoded channel values the canvas stores (no linearization).

The prelude Celesta wraps around the author's code declares:

```wgsl
struct EffectInput {
  /// The pixel's centre, in scene pixels (0.5, 0.5 is the top-left pixel).
  position: vec2f,
  /// `position` relative to the content box: 0,0 at its top-left corner,
  /// 1,1 at its bottom-right. Outside 0–1 within the padding.
  uv: vec2f,
};

struct Celesta {
  /// The scene's width and height, in pixels.
  scene_size: vec2f,
  /// The content box: left, top, right, bottom, in scene pixels.
  content: vec4f,
};
var<uniform> celesta: Celesta;

/// The filtered layer at `position` (scene pixels), interpolated bilinearly
/// between pixel centres. Pixels outside what was drawn are transparent.
fn source_at(position: vec2f) -> vec4f;
/// The filtered layer's pixel at `pixel` (scene pixel coordinates), exactly.
fn source_load(pixel: vec2i) -> vec4f;
fn premultiply(color: vec4f) -> vec4f;
fn unpremultiply(color: vec4f) -> vec4f;
```

and, when the shader declares parameters, a `params` uniform (see
"Parameters").

- The **content box** is the axis-aligned box, in scene pixels, of the
  shapes the layer draws: a rect's own rectangle, an image's or text's
  quad, after transforms. For a group it is the box of its children's
  shapes; a child's blur or shadow spreads its pixels, not its shape. It
  does not include the anti-aliasing fringe or the rounding margin the
  renderer adds to size canvases, so `uv` is 0,0 and 1,1 at the corners of
  a rect drawn at whole pixels. The fringe still lies within the area the
  shader writes (see "Output area").
- There is no built-in time. Every animated shader takes time as a
  parameter. The author chooses the clock (`useCurrentFrame()` is local to
  the enclosing `<Sequence>`; a global clock is also possible), and a shader
  driven by a parameter freezes, rewinds, and retimes correctly under
  `<FreezeFrame>` and sequences.
- Reading outside the drawn canvas gives transparent pixels. The canvas is
  limited to the scene, so a shader cannot read layer pixels that lie off
  screen.
- The author's code may declare its own functions, constants, and structs.
  It must define `effect` with exactly that signature. It must not declare
  entry points or resource variables (`@group`/`@binding`), and must not
  use names the prelude declares or names starting with `celesta`.

### Output

The returned color is clamped: alpha to 0–1, then each color channel to
0–alpha, so the result stays a valid premultiplied color for the passes and
blending that follow. A NaN result gives an unspecified pixel.

### Output area

The shader writes the box of the layer's pixels (the content box with the
renderer's anti-aliasing and rounding margin) expanded by its `padding` on
every side, clipped to the scene. Pixels outside that area stay transparent. A shader
that displaces pixels outward or draws an outline declares how far it
reaches; one that only recolors needs no padding.

### Order of application

For a layer with effects:

1. The layer (or the group's children, after the mask) composites onto a
   transparent canvas, as today.
2. **The shader runs on that canvas.**
3. `blur` applies to the shader's result, and `shadow` and `glow` are made
   from it, as today.
4. The result composites onto the parent through the layer's `opacity`,
   `blendMode`, and the parent's clips, as today.

So a shadow follows the rippled shape, and a blur softens the shader's
output. A layer with a shader and no other effect still goes through steps
1, 2, and 4. To apply a shader after a blur, nest groups:
`<Group shader={…}><Group blur={8}>…</Group></Group>`.

### Parameters

A shader declares up to 16 parameters, each with a type:

| Type    | WGSL    | React value                   |
| ------- | ------- | ----------------------------- |
| `f32`   | `f32`   | `number`                      |
| `vec2`  | `vec2f` | `[number, number]`            |
| `vec3`  | `vec3f` | `[number, number, number]`    |
| `vec4`  | `vec4f` | `[number, number, number, number]` |
| `color` | `vec4f` | `'#RRGGBB'` or `'#RRGGBBAA'`  |

A `color` arrives as straight (not premultiplied) RGBA in 0–1, with the
sRGB-encoded channel values as written; `premultiply()` converts it.

The prelude declares them in the declared order, each 16-byte aligned:

```wgsl
struct Params {
  @align(16) time: f32,
  @align(16) amplitude: f32,
};
var<uniform> params: Params;
```

Values are evaluated by React every frame, so `interpolate`, `spring`, and
`interpolateColor` drive them like any other prop. A shader with no
parameters has no `params`.

## Scene model (`celesta-composition`)

```rust
pub struct Scene {
    // …existing fields…
    /// The custom shaders the scene's layers use, each once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shaders: Vec<ShaderSource>,
    pub layers: Vec<Layer>,
}

pub struct ShaderSource {
    /// Identifies the shader across frames; see "Shader identity".
    pub id: String,
    /// Names the shader in error messages. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The author's code, without the prelude.
    pub wgsl: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ShaderParam>,
}

pub struct ShaderParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: ShaderParamType,
}

pub enum ShaderParamType { F32, Vec2, Vec3, Vec4 }

pub struct LayerEffects {
    // …blur, shadow, glow…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shader: Option<LayerShader>,
}

pub struct LayerShader {
    /// A `ShaderSource::id` in the scene's `shaders`.
    pub id: String,
    /// Every parameter's components in declared order: 1 for `f32`, 2, 3,
    /// or 4 for the vectors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<f64>,
    /// Output pixels the shader may write beyond the content box.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub padding: f64,
}
```

- JSON: `"shaders":[{"id":"9f3c…","name":"ripple","wgsl":"fn effect…","params":[{"name":"time","type":"f32"}]}]`
  on the scene and `"effects":{"shader":{"id":"9f3c…","params":[1.25,6],"padding":8}}`
  on a layer.
- `color` exists only in React: it serializes as `vec4`. Rust does not
  need to tell them apart.
- `LayerEffects::is_empty` also requires `shader` to be `None`.
- Source sits on the scene, not on each layer. Many layers can share one
  shader, and a layer stays small. The scene is still serialized every
  frame (as `fonts` is), but a few kilobytes of shader source is small next
  to a frame's layers.
- ts-rs exports the new types; `pnpm run codegen` regenerates
  `packages/react/src/generated/`, and `scene.ts` re-exports them. The type
  is not called `Shader`, which `@celesta/shader` uses.
- Project JSON (`celesta-project`) does not get shaders. The evaluator
  builds effects with `shader: None`.

### Shader identity

The core's `shaderSource()` (see "Core") computes `id` once per definition:
a 64-bit FNV-1a hash, as 16 hex digits, of the source and the parameter
names and types, in a fixed encoding. The same
definition gives the same id every frame and every run, so the renderer's
pipeline cache survives frames and hot reloads that leave the shader
unchanged. Rust does not trust the hash on its own: a cached pipeline is
reused only when its source and parameters equal the scene's (see
"Compilation and cache").

## Packages

The authoring API is a package of its own, `@celesta/shader`, like
`@celesta/shapes` or `@celesta/transitions`. The core keeps only what the
scene walker needs to serialize a shader.

### Package boundary

| Package | Owns |
| --- | --- |
| `@celesta/react` | The `shader` prop on `CommonProps`; the opaque `ShaderEffect` type; turning a `ShaderEffect` into `effects.shader` and `scene.shaders`; the shader id; the generated scene types. |
| `@celesta/react/internal` | `shaderSource()` and `shaderEffect()`, which build the values the walker accepts. Not for compositions. |
| `@celesta/shader` | `defineShader`, the parameter types and defaults, value validation with author-facing messages, color parsing, packing, and the `*.wgsl` module declaration. |
| `@celesta/cli` | Depends on `@celesta/shader`, so the runtime serves it and its types are staged; bundles `.wgsl` files as text. |

The prop stays in the core, next to `blur`, rather than being added by the
package through module augmentation and a walker registration hook. The
scene types that carry shaders are generated into the core anyway, so the
walker must know the shape of `effects.shader` either way. A registration
hook would add machinery that only this one effect uses. The core accepts
only values built through `@celesta/react/internal`, so the authoring API,
its types, and its messages live entirely in `@celesta/shader`.

### Core (`@celesta/react`)

```ts
// @celesta/react
/** A custom shader with concrete values. Made by a shader from `@celesta/shader`. */
export interface ShaderEffect { /* opaque */ }

export interface CommonProps {
  // …existing props…
  /** A custom WGSL filter, applied before `blur`, `shadow`, and `glow`. */
  shader?: ShaderEffect;
}

// @celesta/react/internal
export interface ShaderSourceHandle { readonly id: string; /* opaque */ }

/** Registers a shader definition once: validates its shape, flattens its strings, computes its id. */
export function shaderSource(source: {
  name?: string;
  wgsl: string;
  params: readonly { name: string; type: 'f32' | 'vec2' | 'vec3' | 'vec4' }[];
}): ShaderSourceHandle;

/** One use of a shader: every parameter's components in declared order, and the padding. */
export function shaderEffect(
  source: ShaderSourceHandle,
  params: readonly number[],
  padding: number,
): ShaderEffect;
```

- `shaderSource` and `shaderEffect` check invariants, not author input: the
  parameter count and types, the component count, finite numbers, and a
  padding that is finite and ≥ 0. They throw plain errors.
  `@celesta/shader` validates the author's input before calling them, so
  these checks fire only on a bug in a caller.
- `shaderSource` passes the source, name, and id through `flatString()`
  once. Every frame's `JSON.stringify` then stays on V8's fast path
  (`docs/performance/react-scene-serialization.md`).
- `ShaderEffect` and `ShaderSourceHandle` are instances of internal
  classes. `extractEffects` in `render.ts` accepts `shader` beside `blur`.
  Anything other than a `ShaderEffect` instance or `undefined` throws
  "shader must be a value returned by a shader from @celesta/shader's
  defineShader()". The CLI shares one core module instance with the entry
  and every package, so the class check holds across packages.
- The walker writes `effects.shader` (`id`, `params`, `padding`). It adds
  the source to the frame's `shaders` once per id, and `renderAt` writes
  `shaders` beside `fonts` when any layer used a shader that frame.

### `@celesta/shader`

```ts
export type ShaderParamType = 'f32' | 'vec2' | 'vec3' | 'vec4' | 'color';

export type ShaderParamSpec =
  | ShaderParamType
  | { type: ShaderParamType; default?: ShaderParamValue };

export interface ShaderDefinitionOptions<P extends Record<string, ShaderParamSpec>> {
  /** The `effect` function and any helpers it uses, in WGSL. */
  wgsl: string;
  /** Names the shader in error messages. */
  name?: string;
  /** Up to 16 parameters, in the order they are declared to WGSL. */
  params?: P;
  /** Output pixels the shader may write beyond the content box. Defaults to 0. */
  padding?: number;
}

export interface Shader<P> {
  (params: ShaderParamValues<P>, options?: { padding?: number }): ShaderEffect;
  readonly id: string;
}

export function defineShader<const P extends Record<string, ShaderParamSpec> = {}>(
  options: ShaderDefinitionOptions<P>,
): Shader<P>;
```

- `ShaderParamValues<P>` maps each parameter to its value type from the
  table under "Parameters". A parameter with a `default` is optional; one
  without is required.
- `defineShader` is called at module level, like `registerComponent`. It
  validates the definition and throws at once:
  - `wgsl` must be a non-empty string;
  - at most 16 parameters;
  - each name must match `/^[A-Za-z][A-Za-z0-9_]*$/` and must not start
    with `celesta`;
  - each type must be one of the five;
  - a default must be valid for its type;
  - `padding` must be a finite number ≥ 0.

  It then calls `shaderSource()` with `color` declared as `vec4`.
- Calling the shader validates the values, fills in the defaults, and packs
  the values in declared order. A color becomes four straight 0–1
  components, parsed with the same `#RRGGBB` / `#RRGGBBAA` rule as effect
  colors. The call returns `shaderEffect(…)`. It throws on an unknown
  name, a missing required value, a non-finite number, a vector of the
  wrong length, or a bad color. Messages name the shader and the
  parameter: "shader ripple: amplitude must be a finite number".
- `package.json` follows the other split packages: `private`, `dist/`
  exports, `sideEffects: false`, and a peer dependency on `@celesta/react`.
  It does not depend on `react`, because it renders nothing.
- The package ships `wgsl.d.ts`, a script file with
  `declare module '*.wgsl' { const source: string; export default source; }`,
  and its `index.d.ts` references it. Importing `@celesta/shader` therefore
  makes `.wgsl` imports type-check, both in `.celesta/` projects and in
  npm projects.

### CLI and runtime

- `packages/cli/package.json` depends on `@celesta/shader`.
  `stage-project-types.mjs` already stages the declarations of every
  `@celesta/*` dependency and maps it in `.celesta/tsconfig.json`.
- `loadEntry` passes esbuild `loader: { '.wgsl': 'text' }`, so
  `import source from './ripple.wgsl'` gives the file's text.
- `crates/editor/src/source.rs` `newest_source_mtime` watches `wgsl` files
  too, so saving a shader reloads the preview.
- `migrate-packages.mjs` is unchanged: no name moves out of `@celesta/react`.

Inline template strings work as well; files are for editor syntax
highlighting and for reuse.

### Web (`@celesta/web`)

The browser renderer draws with Canvas 2D and cannot run WGSL.
`@celesta/shader` is not added to `runtimeModules`, so a composition that
imports it fails to compile with the existing "unavailable in the web
editor" message. `SceneCanvas` also throws "custom shaders are not
supported in the browser renderer" when a layer has `effects.shader`, so a
scene from elsewhere does not silently render without its shader.

## Implementation

### `celesta-composition`

Add the types above. `LayerEffects` gains `shader`, and `Scene` gains
`shaders`. Struct literals of `LayerEffects` and `Scene` in tests, the bench,
and examples get the new fields.

### CPU reference renderer (`crates/renderer`)

The CPU renderer cannot run WGSL. `render_effect_layer` returns a new
`RenderError::UnsupportedShader` when `effects.shader` is set. Neither
preview nor export uses the CPU renderer, so this only affects tests, and
the GPU tests for shaders compare against expected pixels instead of
against the CPU renderer.

### GPU renderer (`crates/gpu-renderer`)

New module `shader.rs`, with `ShaderProcessor` owned by the renderer next
to `EffectProcessor`.

**Assembly.** A shader's module is the prelude (`shader_prelude.wgsl`,
`include_str!`), the generated `Params` struct and binding when there are
parameters, then the author's code, then the entry points:

```wgsl
@group(0) @binding(0) var celesta_source: texture_2d<f32>;   // the canvas
@group(1) @binding(0) var<uniform> celesta: Celesta;          // + canvas origin, internal
@group(1) @binding(1) var<uniform> params: Params;            // only with params

@vertex fn celesta_vertex(…)   // the full-screen triangle effect.wgsl uses
@fragment fn celesta_fragment(@builtin(position) p: vec4f) -> @location(0) vec4f {
  let position = p.xy + celesta_canvas_origin();
  let size = max(celesta.content.zw - celesta.content.xy, vec2f(1.0));
  let color = effect(EffectInput(position, (position - celesta.content.xy) / size));
  let alpha = clamp(color.a, 0.0, 1.0);
  return vec4f(clamp(color.rgb, vec3f(0.0), vec3f(alpha)), alpha);
}
```

The internal part of the `Celesta` uniform also carries the canvas origin
in scene pixels and the canvas size, which `source_at` and `source_load`
use to convert scene positions to canvas texels. Those fields are named
`celesta_*`, and the author's code never sees them as part of the API.
`source_at` blends four `textureLoad`s by hand, treating texels outside the
canvas as transparent, as `effect.wgsl`'s `pair` does. It needs no sampler,
and its edges are exact.

**Validation.** Before creating anything on the device, the assembled
source goes through `naga::front::wgsl::parse_str` and
`naga::valid::Validator`. Then:

- the module must have a function `effect(EffectInput) -> vec4<f32>`;
- its only entry points and global resource variables must be the
  prelude's.

This catches the author declaring their own bindings or entry points. Only
a module that passes is handed to `create_shader_module` (as
`ShaderSource::Naga`, so it is not parsed twice) and
`create_render_pipeline`, inside a validation error scope as a second
guard. A failure the scope still reports becomes the same error.

**Errors.** `GpuRenderError::Shader { name, message }`. Display:
"invalid custom shader ripple: 3:17: unknown identifier `amplitde`". Parse
and validation spans that fall inside the author's code are reported as
line and column within that code, by subtracting the offset of the
author's code in the assembled source. Spans elsewhere (the prelude or
generated parts, such as a parameter name that is a WGSL keyword) give
the message without a location. A layer whose `shader.id` is not in
`scene.shaders`, a duplicate id in `scene.shaders`, or a parameter count
that does not match the declaration is also `GpuRenderError::Shader`. Any
of these fails the frame, as an invalid effect color does today. The
editor and the exporter already report frame errors.

**Compilation and cache.** At the start of `prepare`, each
`scene.shaders` entry is looked up in a map from id to the compiled
pipeline (or the compile error) and the `ShaderSource` it was built from.
An entry is reused only when the stored source equals the scene's;
otherwise it is compiled again. Errors are cached too, so a broken shader
is not recompiled every frame while the author fixes it. Entries no frame
has used for 300 frames are dropped, so hot reloads do not accumulate
pipelines. The first frame that uses a new shader pays the compile
(milliseconds; more for the platform's pipeline compile), and later frames
pay nothing.

**Planning.** `plan_groups` tracks, beside each group's conservative
`content`, its `shape`: the union of its layers' quads without margins
(`PreparedLayer::shape`), unioned through groups, intersected through
masks like `content`, and not spread by a nested effect. `EndEffect`
carries it as the shader's content box. `EffectSpec` gains `shader: Option<ShaderSpec>`: the cache
key, the padding (clamped to 0–512), and the byte offset of the pass's
uniforms in the frame's shader parameter data. `output_bounds` expands the
content by the padding first, and computes blur, shadow, and glow reach
from that shaded box instead of the content.

**Uniforms.** As for filter passes, every shader pass's built-ins and
parameters are packed into one per-frame buffer and bound with dynamic
offsets: built-ins at offset `a`, parameters at offset `b`, each aligned to
`min_uniform_buffer_offset_alignment`. Each parameter takes one 16-byte
slot, so the buffer layout matches the `@align(16)` struct with no further
layout rules. The buffer is sized in `begin_frame` from the number of
`EndEffect` items with a shader, and written in `end_frame`.

**Compositing.** In `compositor.rs` `draw_group`, after the layers have
drawn onto `canvas` and `content` is known: when the effect has a shader,
take a pooled canvas of the same size, run the shader pass into it with the
scissor set to the content expanded by the padding (in canvas pixels),
recycle the original canvas, and continue with the shader's result and its
expanded content. The existing blur, shadow, glow, and final draw then run
unchanged. One pipeline layout (group 0: `texture_layout`; group 1: the
two dynamic uniforms) serves every custom shader.

Scenes without shaders take exactly the code paths they take today.

## Testing

### Rust

- `celesta-composition`: serde round trip of `Scene.shaders` and
  `LayerEffects.shader`; the fields are omitted when empty or `None`;
  scenes and effects without them still deserialize.
- `celesta-renderer`: a layer with a shader returns `UnsupportedShader`.
- `celesta-gpu-renderer`, against expected pixel samples:
  - an identity shader (`return source_at(input.position);`) reproduces the
    unfiltered layer exactly;
  - a color-inverting shader on a `Rect`;
  - `uv` is 0,0 and 1,1 at the content box corners;
  - a translating shader with padding moves the rect, and without padding
    it is cut at the content box;
  - `source_at` beyond the canvas reads transparent;
  - out-of-range output is clamped to a valid premultiplied color;
  - parameters of every type arrive in order, including several `f32`s in
    a row (the 16-byte slots);
  - a shader then a shadow: the shadow follows the shader's output;
  - the shader applies after a group's mask;
  - two layers with the same shader and different values in one frame;
  - a rotated group: the shader runs in scene space.
- Errors: a syntax error reports the author's line and column; a missing
  `effect`, a wrong `effect` signature, a declared `@binding`, and an entry
  point are each rejected; an unknown id and a wrong parameter count fail
  the frame; a broken shader is compiled once across several frames.
- Cache: a changed source under the same id recompiles; an entry unused for
  300 frames is dropped.
- `celesta-exporter`: a React entry with a shader imported from a `.wgsl`
  file renders through the export path (`png_integration.rs`) and matches
  the GPU renderer's frame.

### Core (`packages/react/test/shader.test.mjs`)

Through `@celesta/react/internal` directly, without `@celesta/shader`:

1. A `shaderEffect` on a layer serializes `effects.shader`, and the scene
   carries the source once, however many layers use it.
2. The id is stable across calls and runs, and changes with the source or
   a parameter's name or type.
3. A frame where no layer uses a shader has no `shaders`.
4. The serialized source, name, and id are flat strings
   (`flat-strings.test.mjs`'s check).
5. A `shader` prop that is not a `ShaderEffect` (a plain object of the
   same shape, a string) throws; each invariant check of `shaderSource`
   and `shaderEffect` throws.

### `@celesta/shader` (`packages/shader/test/shader.test.mjs`)

1. Defaults fill missing values; a color packs to four straight 0–1
   components; vectors pack in order.
2. Each `defineShader` and parameter error above throws its message.
3. `shader.id` equals the id the core computes.

### Across packages (`packages/cli/test`)

1. A composition that imports `@celesta/shader` and a `.wgsl` file renders
   through the CLI, and the frame carries the file's text as the source.
2. The staged project types make `import source from './x.wgsl'`
   type-check once `@celesta/shader` is imported.
3. `@celesta/web`: importing `@celesta/shader` reports that it is
   unavailable, and `SceneCanvas` rejects a scene with `effects.shader`.

### Performance

Add a `shaders` workload to `crates/bench/src/workloads.rs`: a full-screen
color grade on a video and a padded ripple on text. Record it in
`docs/performance/` with reproduction steps and within-run ratios against
the existing `effects` workloads. The existing workloads must not slow
down.

## Examples

`packages/cli/examples/with-shader.tsx` with `ripple.wgsl` beside it: the
ripple above on a title, plus an RGB-split shader on a video whose amount
follows a `spring`.

## Delivery

Two pull requests:

1. The scene model, codegen, the GPU and CPU renderer changes, and the
   Rust tests. Shaders are usable from scene JSON at this point.
2. The core's `shader` prop and internal API, the new `@celesta/shader`
   package, the CLI dependency and `.wgsl` loader, the watcher, the web
   guard, the JavaScript tests, the exporter test, the example, the bench
   workload, and the docs.

## Docs

- `packages/website/src/docs-content.tsx` and its `en.json` / `ja.json`
  keys, after the effects section. The page documents the prelude
  (`EffectInput`, `celesta`, `source_at`, `source_load`, `premultiply`,
  `unpremultiply`), the premultiplied output, the order of application,
  and the padding.
- `skills/celesta/references/shader.md`, and the package list in
  `skills/celesta/SKILL.md`.
- `HANDOFF.md`'s workspace map gets a `packages/shader` row.
- A handoff note `docs/handoff/<date>-custom-shader-filters.md`, dated the
  day it lands.

## Out of scope

- Generators: a shader that fills a rectangle with no input layer
  (phase 2).
- Transition shaders with two inputs and a progress, for
  `<TransitionSeries>` (phase 2).
- Extra image or video inputs (LUTs, noise textures, displacement maps).
- Multi-pass shaders, and ordering a shader between `blur` and the
  others. Nest groups instead.
- GLSL or ShaderToy-compatible source. naga's GLSL front end could add it
  later behind the same prelude.
- Built-in time. Pass it as a parameter.
- Shaders in project JSON timelines.
- Shaders in the browser renderer (`@celesta/web`). It would need a WebGPU
  path beside Canvas 2D.
- Protection against shaders that hang the GPU. A shader that runs too
  long can trigger the driver's timeout; the docs warn about it.
