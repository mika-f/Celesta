import { defineShader } from '@celesta.studio/shader';

// Custom WGSL filters (@celesta.studio/shader). Each one is a single `shader` prop;
// to stack two, nest groups.

// Rings spreading from the layer's centre push the pixels outward.
export const ripple = defineShader({
  name: 'ripple',
  params: { time: 'f32', amplitude: { type: 'f32', default: 8 }, wavelength: { type: 'f32', default: 90 } },
  padding: 16,
  wgsl: /* wgsl */ `
fn effect(input: EffectInput) -> vec4f {
    let center = mix(celesta.content.xy, celesta.content.zw, vec2f(0.5));
    let offset = input.position - center;
    let distance = length(offset);
    let wave = sin(distance / params.wavelength * 6.2831853 - params.time * 5.0) * params.amplitude;
    let direction = offset / max(distance, 1e-4);
    return source_at(input.position - direction * wave);
}`,
});

// Horizontal slices slip sideways and the channels split apart.
export const glitch = defineShader({
  name: 'glitch',
  params: {
    amount: 'f32',
    split: 'f32',
    seed: { type: 'f32', default: 0 },
    band: { type: 'f32', default: 36 },
  },
  padding: 64,
  wgsl: /* wgsl */ `
fn hash(n: f32) -> f32 {
    return fract(sin(n * 12.9898 + 78.233) * 43758.5453);
}

fn effect(input: EffectInput) -> vec4f {
    let band = floor(input.position.y / params.band);
    let h = hash(band + params.seed * 17.0);
    let slip = select(0.0, (h - 0.5) * 2.0 * params.amount, h > 0.62);
    let p = input.position + vec2f(slip, 0.0);
    let r = source_at(p + vec2f(params.split, 0.0));
    let g = source_at(p);
    let b = source_at(p - vec2f(params.split, 0.0));
    return vec4f(r.r, g.g, b.b, max(max(r.a, g.a), b.a));
}`,
});

// Square cells, each the colour at its centre.
export const pixelate = defineShader({
  name: 'pixelate',
  params: { size: 'f32' },
  wgsl: /* wgsl */ `
fn effect(input: EffectInput) -> vec4f {
    let s = max(params.size, 1.0);
    let cell = (floor((input.position - celesta.content.xy) / s) + 0.5) * s + celesta.content.xy;
    return source_at(cell);
}`,
});
