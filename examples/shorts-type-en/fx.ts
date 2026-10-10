import { defineShader } from '@celesta/shader';

// The finishing pass over the whole frame: an RGB split that grows towards
// the edges, horizontal glitch slices, and a flash towards white. The film
// drives all three from the beat and the scene cuts.
const source = /* wgsl */ `
fn hash(p: vec2f) -> f32 {
    return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453);
}

fn effect(input: EffectInput) -> vec4f {
    let size = celesta.scene_size;
    var p = input.position;
    // Slices of random height shift sideways while glitch is up.
    let band = floor(p.y / mix(18.0, 120.0, hash(vec2f(floor(p.y / 120.0), params.seed))));
    let pick = hash(vec2f(band, params.seed + 3.0));
    if (pick > 1.0 - 0.45 * params.glitch) {
        p.x = p.x + (hash(vec2f(band, params.seed + 7.0)) - 0.5) * 260.0 * params.glitch;
    }
    // The split is zero at the centre and grows with the square of the
    // distance, so copy in the middle stays crisp and the edges tear.
    let from_center = (p - size * 0.5) / (size * 0.5);
    let shift = from_center * length(from_center) * params.split;
    let r = source_at(clamp(p + shift, vec2f(0.5), size - 0.5));
    let g = source_at(clamp(p, vec2f(0.5), size - 0.5));
    let b = source_at(clamp(p - shift, vec2f(0.5), size - 0.5));
    let color = vec3f(r.r, g.g, b.b);
    // A soft vignette keeps the eye in the middle of the tall frame.
    let vignette = 1.0 - 0.28 * smoothstep(0.55, 1.35, length(from_center * vec2f(1.0, 0.8)));
    let lit = mix(color * vignette, vec3f(1.0), params.flash);
    return vec4f(lit, 1.0);
}
`;

export const fx = defineShader({
  name: 'fx',
  wgsl: source,
  params: {
    // Pixels of RGB split at the frame's edge.
    split: 'f32',
    // 0–1: how many slices shift, and how far.
    glitch: 'f32',
    // Changes which slices shift.
    seed: 'f32',
    // 0–1 towards white.
    flash: 'f32',
  },
});
