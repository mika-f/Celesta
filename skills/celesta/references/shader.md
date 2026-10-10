# @celesta/shader: custom WGSL filters

```tsx
import { defineShader } from '@celesta/shader';
import rippleSource from './ripple.wgsl';
```

A custom shader filters a layer's pixels with WGSL code you write. Use it
for what `blur`, `shadow`, `glow`, `blendMode`, and masks cannot do:
ripples, RGB splits, pixelation, color grades, glitches. Celesta's runtime
supplies the package; never install it from npm.

Shaders run in the desktop preview and in export (on the GPU). The browser
editor (`@celesta/web`) cannot run them, and components placed on project
timelines (`registerComponent` resolved into a `.celesta.json`) cannot use
them yet.

## Contents

- [Define and use](#define-and-use)
- [What the shader sees](#what-the-shader-sees)
- [Parameters](#parameters)
- [Padding](#padding)
- [Errors](#errors)

## Define and use

```tsx
const ripple = defineShader({
  name: 'ripple',              // names the shader in error messages
  wgsl: rippleSource,          // or an inline template string
  params: { time: 'f32', amplitude: { type: 'f32', default: 6 } },
  padding: 8,                  // see Padding
});

function Title() {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return <Text shader={ripple({ time: frame / fps })}>CELESTA</Text>;
}
```

- Call `defineShader` at module level, not inside a component.
- Any visual layer or `Group` takes `shader`. On a group it filters the
  children composited together.
- The shader runs **before** `blur`, `shadow`, and `glow`, so a shadow
  follows the shader's output. To blur first, nest groups:
  `<Group shader={…}><Group blur={8}>…</Group></Group>`.
- A `.wgsl` import gives the file's text. Saving the file reloads the
  preview.

## What the shader sees

The source defines one function, and may add helpers, constants, and
structs:

```wgsl
fn effect(input: EffectInput) -> vec4f {
    let center = mix(celesta.content.xy, celesta.content.zw, vec2f(0.5));
    let offset = input.position - center;
    let wave = sin(length(offset) * 0.06 - params.time * 6.0) * params.amplitude;
    // Guarded: `normalize` of the zero vector at the centre is undefined.
    let direction = offset / max(length(offset), 1e-4);
    return source_at(input.position - direction * wave);
}
```

| Name | Meaning |
| --- | --- |
| `input.position` | the output pixel's centre, in scene pixels (`0.5, 0.5` is the top-left pixel) |
| `input.uv` | `position` in the layer's box: `0,0` top-left, `1,1` bottom-right; outside 0–1 in the padding |
| `celesta.scene_size` | the scene's width and height |
| `celesta.content` | the layer's box: left, top, right, bottom, in scene pixels |
| `source_at(position)` | the layer at a scene position, interpolated; transparent outside what was drawn |
| `source_load(pixel)` | the layer's exact pixel at `vec2i` scene coordinates |
| `premultiply(c)` / `unpremultiply(c)` | convert between straight and premultiplied RGBA |
| `params.<name>` | the parameters (see below) |

- Colors are **premultiplied** RGBA in 0–1, with sRGB values as stored (no
  linearization). Return premultiplied too; Celesta clamps alpha to 0–1 and
  each color channel to alpha.
- Coordinates are output pixels after transforms: a rotated layer's shader
  still works in scene space.
- The layer's box is the box of what it draws: a rect's own rectangle, a
  text's or image's quad, a group's children. A child's blur spreads its
  pixels, not the box.
- There is no built-in time. Pass `frame / fps` (or any clock) as a
  parameter, so the shader follows `Sequence` and `FreezeFrame`.
- Do not declare `@group`/`@binding` resources or entry points, or names
  starting with `celesta`.

## Parameters

Up to 16, declared in order:

| Type | WGSL | Value in React |
| --- | --- | --- |
| `'f32'` | `f32` | `number` |
| `'vec2'` / `'vec3'` / `'vec4'` | `vec2f` / `vec3f` / `vec4f` | `[x, y]` / `[x, y, z]` / `[x, y, z, w]` |
| `'color'` | `vec4f` | `'#RRGGBB'` or `'#RRGGBBAA'`; straight RGBA in 0–1 (use `premultiply`) |

`{ type, default }` makes a parameter optional. Values are checked when
you call the shader: an unknown name, a missing value, a non-finite number,
a vector of the wrong length, or a bad color throws, naming the shader and
the parameter. Animate values like any prop (`interpolate`, `spring`,
`interpolateColor`).

## Padding

Without padding, the shader writes only the layer's box. A shader that
moves pixels outward or draws past the edge (ripple, outline, RGB split)
declares how far with `padding` in output pixels, on the definition or per
use: `ripple({ time }, { padding: 16 })`. The shader then writes the box
grown by that much on every side. The most is 512. A shader that only
recolors needs none.

## Errors

A shader that does not compile fails the frame with its line and column in
your source:

```
invalid custom shader ripple: 3:17: no definition in scope for identifier: `amplitde`
```

A missing or misdeclared `effect` names the signature it expects:
`fn effect(input: EffectInput) -> vec4f`. The CPU reference renderer does
not run shaders; check the output with PNG frames from the exporter
([verify.md](verify.md)).
