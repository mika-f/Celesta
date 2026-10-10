import { defineShader } from '@celesta/shader';
import kaleidoSource from './kaleido.wgsl';
import lensSource from './lens.wgsl';
import sheenSource from './sheen.wgsl';
import silkSource from './silk.wgsl';

// The four WGSL filters. None reads a clock of its own: each use passes the
// loop's angle or values derived from it (see ../loop.ts).

/** The background, computed per pixel. `focus` is in scene widths from the centre. */
export const silk = defineShader({
  name: 'silk',
  wgsl: silkSource,
  params: {
    theta: 'f32',
    pulse: 'f32',
    focus: 'vec2',
    deep: 'color',
    low: 'color',
    high: 'color',
  },
});

/** Mirrors a group of shapes into a kaleidoscope. Angles in radians, `split` in pixels. */
export const kaleido = defineShader({
  name: 'kaleido',
  wgsl: kaleidoSource,
  params: {
    center: 'vec2',
    sectors: 'vec2',
    blend: 'f32',
    spin: 'f32',
    slice: 'f32',
    twist: 'f32',
    split: { type: 'f32', default: 0 },
  },
  padding: 32,
});

/** A band of light across text, with a horizontal RGB split of `split` pixels. */
export const sheen = defineShader({
  name: 'sheen',
  wgsl: sheenSource,
  params: {
    sweep: 'f32',
    split: 'f32',
    tint: 'color',
  },
  padding: 8,
});

/** A refracting ring; `strength` is how far, in pixels, it moves what it crosses. */
export const lens = defineShader({
  name: 'lens',
  wgsl: lensSource,
  params: {
    center: 'vec2',
    radius: 'f32',
    width: 'f32',
    strength: 'f32',
    tint: 'color',
  },
});
