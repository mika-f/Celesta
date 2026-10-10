import { defineShader } from '@celesta/shader';

// A halftone field drawn over a flat background rect: dots that grow towards
// the bottom of the frame (where the platform UI sits and no copy is placed),
// drift upwards, and swell where a ring sweeps up from below on every beat.
const source = /* wgsl */ `
fn hash(p: vec2f) -> f32 {
    return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453);
}

fn value_noise(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2f(1.0, 0.0)), u.x),
               mix(hash(i + vec2f(0.0, 1.0)), hash(i + vec2f(1.0, 1.0)), u.x), u.y);
}

fn effect(input: EffectInput) -> vec4f {
    let base = source_at(input.position);
    let size = celesta.scene_size;
    let lift = vec2f(0.0, params.time * params.drift);
    // The grid scrolls up; each pixel belongs to the dot of its cell.
    let p = input.position + lift;
    let center = (floor(p / params.cell) + 0.5) * params.cell - lift;
    let d = length(input.position - center);
    let density = smoothstep(params.fade, 1.0, center.y / size.y);
    let grain = value_noise(center / 240.0 + vec2f(params.time * 0.2, 0.0));
    // The beat's ring rises from below the frame and fades as it goes.
    let ring = abs(length(center - vec2f(size.x * 0.5, size.y * 1.1)) - params.phase * size.y * 1.1);
    let wave = exp(-ring * ring / 12000.0) * (1.0 - params.phase);
    let radius = params.cell * 0.48 * clamp(density * (0.3 + 0.7 * grain) + wave * 0.7, 0.0, 1.0);
    let cover = (1.0 - smoothstep(radius - 1.0, radius + 1.0, d)) * smoothstep(0.0, 1.5, radius);
    let tone = premultiply(params.tone) * cover;
    return tone + base * (1.0 - tone.a);
}
`;

export const halftone = defineShader({
  name: 'halftone',
  wgsl: source,
  params: {
    time: 'f32',
    // 0–1 through the current beat.
    phase: 'f32',
    tone: 'color',
    cell: { type: 'f32', default: 34 },
    // Dots start at this fraction of the height and are full size at the bottom.
    fade: { type: 'f32', default: 0.5 },
    drift: { type: 'f32', default: 40 },
  },
});
