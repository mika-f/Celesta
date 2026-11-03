// The built-in stylize effects and the chroma key (`LayerFilter`), assembled
// like a custom shader (`shader.rs`) with two parameters: `op` is the
// effect's kind and its first three values, `more` the rest. `@celesta.studio/web`
// repeats this arithmetic in `packages/web/src/filters.ts`.

const PIXELATE: u32 = 0u;
const HALFTONE: u32 = 1u;
const CHROMATIC_ABERRATION: u32 = 2u;
const WAVE: u32 = 3u;
const NOISE: u32 = 4u;
const SCANLINES: u32 = 5u;
const ZOOM_BLUR: u32 = 6u;
const CHROMA_KEY: u32 = 7u;

// Rec. 709 luma, as the color filters use.
const LUMA: vec3f = vec3f(0.2126, 0.7152, 0.0722);
const PI: f32 = 3.14159265358979;
const TAU: f32 = 6.28318530717959;
const ZOOM_SAMPLES: i32 = 32;

// `v` turned by `degrees`, clockwise on screen.
fn turned(v: vec2f, degrees: f32) -> vec2f {
    let angle = radians(degrees);
    let c = cos(angle);
    let s = sin(angle);
    return vec2f(c * v.x - s * v.y, s * v.x + c * v.y);
}

// The average of 4×4 samples over `cell` of a grid of `size` pixels turned
// by `degrees` about `origin`, at the centres of its sixteenths.
fn cell_average(origin: vec2f, cell: vec2f, size: f32, degrees: f32) -> vec4f {
    var sum = vec4f(0.0);
    for (var j = 0; j < 4; j++) {
        for (var i = 0; i < 4; i++) {
            let grid = (cell + (vec2f(f32(i), f32(j)) + 0.5) / 4.0) * size;
            sum += source_at(origin + turned(grid, degrees));
        }
    }
    return sum / 16.0;
}

// PCG's output permutation of `v`'s state step: a 32-bit integer hash.
fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// A uniform value in [0, 1) for `pixel`, `seed`, and `channel`, in steps of
// 2^-24 so that it converts to `f32` exactly.
fn random(pixel: vec2i, seed: u32, channel: u32) -> f32 {
    let hash = pcg(bitcast<u32>(pixel.x) ^ pcg(bitcast<u32>(pixel.y) ^ pcg(seed + channel * 0x9e3779b9u)));
    return f32(hash >> 8u) / 16777216.0;
}

// The length of the bands `width` thick, one at the start of each
// `spacing`, between 0 and `t`.
fn bands_to(t: f32, spacing: f32, width: f32) -> f32 {
    let period = floor(t / spacing);
    return period * width + min(t - period * spacing, width);
}

// BT.709 Cb and Cr of a straight sRGB-encoded color.
fn chroma(rgb: vec3f) -> vec2f {
    let y = dot(rgb, LUMA);
    return vec2f((rgb.b - y) / 1.8556, (rgb.r - y) / 1.5748);
}

fn effect(input: EffectInput) -> vec4f {
    let position = input.position;
    let box = celesta.content;
    let local = position - box.xy;
    let op = params.op;
    let more = params.more;
    switch u32(op.x) {
        case PIXELATE: {
            let size = op.y;
            if size <= 1.0 {
                return source_load(vec2i(floor(position)));
            }
            return cell_average(box.xy, floor(local / size), size, 0.0);
        }
        case HALFTONE: {
            let size = max(op.y, 1e-4);
            let angle = op.z;
            let grid = turned(local, -angle);
            let cell = floor(grid / size);
            let color = cell_average(box.xy, cell, size, angle);
            let luma = clamp(dot(unpremultiply(color).rgb, LUMA), 0.0, 1.0);
            let radius = size * sqrt(luma / PI);
            let distance = length(grid - (cell + 0.5) * size);
            // No pixel takes more than the whole dot, however small.
            let coverage = min(clamp(radius - distance + 0.5, 0.0, 1.0), PI * radius * radius);
            return color * coverage;
        }
        case CHROMATIC_ABERRATION: {
            let shift = turned(vec2f(op.y, 0.0), op.z);
            let red = source_at(position - shift);
            let green = source_at(position);
            let blue = source_at(position + shift);
            return vec4f(red.r, green.g, blue.b, max(red.a, max(green.a, blue.a)));
        }
        case WAVE: {
            let toward = turned(vec2f(1.0, 0.0), more.x);
            let across = vec2f(-toward.y, toward.x);
            // Reduced to one turn first, so a far pixel keeps its precision.
            let turn = fract(dot(local, across) / max(op.z, 1e-4) + op.w / 360.0);
            return source_at(position - toward * (op.y * sin(TAU * turn)));
        }
        case NOISE: {
            let pixel = vec2i(floor(position));
            let color = unpremultiply(source_load(pixel));
            let seed = u32(op.z) | (u32(op.w) << 16u);
            var grain = vec3f(random(pixel, seed, 0u));
            if more.x < 0.5 {
                grain = vec3f(grain.x, random(pixel, seed, 1u), random(pixel, seed, 2u));
            }
            let rgb = color.rgb + clamp(op.y, 0.0, 1.0) * (2.0 * grain - 1.0);
            return premultiply(vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), color.a));
        }
        case SCANLINES: {
            let color = unpremultiply(source_load(vec2i(floor(position))));
            let spacing = max(op.z, 1e-4);
            let width = clamp(op.w, 0.0, 1.0) * spacing;
            let y = local.y - more.x;
            let covered = bands_to(y + 0.5, spacing, width) - bands_to(y - 0.5, spacing, width);
            let rgb = color.rgb * (1.0 - clamp(op.y, 0.0, 1.0) * clamp(covered, 0.0, 1.0));
            return premultiply(vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), color.a));
        }
        case ZOOM_BLUR: {
            let amount = clamp(op.y, 0.0, 1.0);
            let centre = box.xy + vec2f(op.z, op.w) * (box.zw - box.xy);
            var sum = vec4f(0.0);
            for (var i = 0; i < ZOOM_SAMPLES; i++) {
                let scale = 1.0 - amount * f32(i) / f32(ZOOM_SAMPLES - 1);
                sum += source_at(centre + (position - centre) * scale);
            }
            return sum / f32(ZOOM_SAMPLES);
        }
        case CHROMA_KEY: {
            let color = unpremultiply(source_load(vec2i(floor(position))));
            let tolerance = max(op.y, 0.0);
            let softness = max(op.z, 1e-4);
            let spill = clamp(op.w, 0.0, 1.0);
            let key = chroma(more.rgb);
            var cbcr = chroma(color.rgb);
            let t = clamp((distance(cbcr, key) - tolerance) / softness, 0.0, 1.0);
            let keep = t * t * (3.0 - 2.0 * t);
            // Takes the key's chroma out of what remains, keeping luma.
            let reach = length(key);
            if reach > 1e-4 {
                let toward = key / reach;
                cbcr -= toward * max(dot(cbcr, toward), 0.0) * spill;
            }
            let y = dot(color.rgb, LUMA);
            let r = y + 1.5748 * cbcr.y;
            let b = y + 1.8556 * cbcr.x;
            let g = (y - LUMA.r * r - LUMA.b * b) / LUMA.g;
            return premultiply(vec4f(clamp(vec3f(r, g, b), vec3f(0.0), vec3f(1.0)), color.a * keep));
        }
        default: {
            return source_load(vec2i(floor(position)));
        }
    }
}
