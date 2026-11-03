# @celesta/shader: custom WGSL filters

```tsx
import { defineShader } from '@celesta/shader';
import rippleSource from './ripple.wgsl';
```

A custom shader filters a layer's pixels with WGSL code you write. Use it
for what `blur`, `shadow`, `glow`, `blendMode`, masks, and the built-in
filters cannot do: ripples, glitches, displacement maps. Check the
[built-in filters](#built-in-filters) first: pixelation, halftone, RGB
splits, waves, grain, scanlines, zoom blur, and chroma keys need no WGSL.
Celesta's runtime supplies the package; never install it from npm.

Shaders run in the desktop preview and in export (on the GPU), including
in components placed on project timelines (`registerComponent` resolved
into a `.celesta.json`). The browser editor (`@celesta/web`) cannot run
them; it runs the built-in filters.

## Contents

- [Define and use](#define-and-use)
- [What the shader sees](#what-the-shader-sees)
- [Parameters](#parameters)
- [Padding](#padding)
- [Reading past the edge](#reading-past-the-edge)
- [Built-in filters](#built-in-filters)
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
- An array applies several shaders in order, each filtering the result of
  the one before: `shader={[ripple({ time }), rgbSplit({ amount: 6 })]}`.
  It draws as nesting a group around each shader would, without the extra
  groups. A single shader still works as before.
- The shaders run **before** `blur`, `shadow`, and `glow`, so a shadow
  follows the last shader's output. To blur first, nest groups:
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
| `source_at_clamped(position)` / `source_at_mirrored(position)` | `source_at`, with a position past the layer's box moved onto its edge or folded back in (see [Reading past the edge](#reading-past-the-edge)) |
| `source_load(pixel)` | the layer's exact pixel at `vec2i` scene coordinates |
| `premultiply(c)` / `unpremultiply(c)` | convert between straight and premultiplied RGBA |
| `params.<name>` | the parameters (see below) |

- Colors are **premultiplied** RGBA in 0–1, with sRGB values as stored (no
  linearization). Return premultiplied too; Celesta clamps alpha to 0–1 and
  each color channel to alpha.
- Coordinates are output pixels after transforms: a rotated layer's shader
  still works in scene space.
- The layer's box is the box of what it draws: a rect's own rectangle, a
  text's or image's quad, a path's points (control points included) grown
  by half its stroke width, a group's children. A child's blur spreads its
  pixels, not the box, and neither does an earlier shader in an array:
  every shader in it sees the same box.
- There is no built-in time. Pass `frame / fps` (or any clock) as a
  parameter, so the shader follows `Sequence` and `FreezeFrame`.
- Do not declare `@group`/`@binding` resources, entry points, workgroup
  variables (`var<workgroup>`, compute only), or names starting with
  `celesta` (aliases and overrides included).

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

In an array, each shader writes what the one before it wrote, grown by
its own padding, so the paddings add up: two shaders that each push
pixels 8px outward reach 16px past the box.

## Reading past the edge

`source_at` reads transparent past what the layer drew. A shader that
pulls pixels inward from further out (a lens, a refraction, a ripple)
therefore reads transparency near the layer's edge. Over a layer that
fills the frame, such as a `Group` holding the background, this shows as
dark seams along the frame's edges, and `padding` does not help: there is
nothing outside to read.

Two reads keep to the layer's box (`celesta.content`) instead:

| Read | Past the box | Looks like |
| --- | --- | --- |
| `source_at_clamped(position)` | the nearest edge pixel | the edge pixels stretched into streaks |
| `source_at_mirrored(position)` | the pixel as far back inside, mirrored about the edge | the picture reflected; usually the one to use |

```wgsl
// A lens that shrinks: pixels within `params.radius` of `params.center`
// show what lies further out, so near the frame's edge they read past it.
fn effect(input: EffectInput) -> vec4f {
    let d = input.position - params.center;
    let k = clamp(length(d) / params.radius, 0.0, 1.0);
    return source_at_mirrored(params.center + d * mix(1.0 + params.strength, 1.0, k * k));
}
```

Both fold or clamp about the centres of the box's edge pixels, so they
never read past it; inside the box they read what `source_at` reads.

## Built-in filters

For brightness, contrast, saturation, hue, grayscale, invert, tint, a
vignette, pixelation, halftone, chromatic aberration, a wave, grain,
scanlines, a zoom blur, or a chroma key, write no WGSL: each layer and
`Group` takes them as props (`<Image src="a.png" saturation={0.4}
vignette={0.5} />`; see the layer props in [react-core.md](react-core.md)).
The props run after `shader`, in a fixed order. To place a built-in filter
elsewhere among custom shaders, use its function from `@celesta/shader` in
the `shader` array:

```tsx
import { brightness, contrast, grayscale, hue, invert, saturation, tint, vignette } from '@celesta/shader';

<Group shader={[ripple({ time }), grayscale(), tint({ color: '#704214', amount: 0.35 }), vignette(0.6)]}>…</Group>
```

| Function | Takes |
| --- | --- |
| `brightness(amount)`, `contrast(amount)`, `saturation(amount)` | `1` keeps the color; at least `0` |
| `hue(degrees)` | any angle |
| `grayscale(amount = 1)`, `invert(amount = 1)` | `0`–`1` |
| `tint({ color, amount? })` | as the `tint` prop |
| `vignette(amount)` or `vignette({ amount, radius?, softness? })` | as the `vignette` prop |

They take no padding, and the scene lists no source for them. They work on
the straight sRGB color, keep alpha, and clamp to 0–1, as CSS `filter`
functions do (`saturation` and `hue` use CSS's matrices). `vignette`
follows the same box as `uv`.

The stylize effects and the chroma key take the values of their props:

| Function | Takes |
| --- | --- |
| `pixelate(size)` or `pixelate({ size })` | cell size in px |
| `halftone(size)` or `halftone({ size, angle? })` | grid size in px, angle in degrees (45) |
| `chromaticAberration(offset)` or `chromaticAberration({ offset, angle? })` | px, degrees (0) |
| `wave({ amplitude, wavelength, phase?, angle? })` | px, px, degrees, degrees |
| `noise(amount)` or `noise({ amount, seed?, monochrome? })` | `0`–`1`; an integer seed, else the current frame |
| `scanlines(amount)` or `scanlines({ amount, spacing?, thickness?, offset? })` | `0`–`1`, px, `0`–`1`, px |
| `zoomBlur(amount)` or `zoomBlur({ amount, centerX?, centerY? })` | `0`–`1`, centre in the box |
| `chromaKey(color)` or `chromaKey({ color, tolerance?, softness?, spill? })` | `'#RRGGBB'`, CbCr distances, `0`–`1` |

Each runs as one pass. Their grids (pixelate, halftone, scanlines), wave,
and zoom centre follow the same box as `uv`; noise hashes the scene pixel,
so it does not move with the layer. Pixelate, halftone, chromatic
aberration, and wave write as far past the box as they reach; a zoom blur
up to 512 px. A noise without a seed takes the frame where the layer is
walked (local inside a `<Sequence>`, held inside a `<FreezeFrame>`), so a
frame always renders the same grain.

## Errors

A shader that does not compile fails the frame with its line and column in
your source:

```
invalid custom shader ripple: 3:17: no definition in scope for identifier: `amplitde`
```

A missing or misdeclared `effect` names the signature it expects:
`fn effect(input: EffectInput) -> vec4f`. A built-in filter with a value
out of range throws, naming the prop: `grayscale must be a finite number
between 0 and 1`, `wave.wavelength must be a finite number above 0`. The
CPU reference renderer does not run shaders or built-in filters; check the
output with PNG frames from the exporter ([verify.md](verify.md)).
