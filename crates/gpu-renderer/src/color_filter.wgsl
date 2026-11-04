// The built-in color filters (`LayerFilter`), assembled like a custom
// shader (`shader.rs`) with two parameters: `op` is the filter's kind and
// its amounts, `color` the tint's straight RGBA. `@celesta.studio/web` repeats
// this arithmetic in `packages/web/src/color-filters.ts`.

const BRIGHTNESS: u32 = 0u;
const CONTRAST: u32 = 1u;
const SATURATION: u32 = 2u;
const HUE: u32 = 3u;
const GRAYSCALE: u32 = 4u;
const INVERT: u32 = 5u;
const TINT: u32 = 6u;
const VIGNETTE: u32 = 7u;

// Rec. 709 luma, as CSS `saturate()` and `grayscale()` use.
const LUMA: vec3f = vec3f(0.2126, 0.7152, 0.0722);

// CSS `saturate()`: 1 keeps the color, 0 is its luma.
fn saturated(rgb: vec3f, amount: f32) -> vec3f {
    return mix(vec3f(dot(rgb, LUMA)), rgb, amount);
}

// CSS `hue-rotate()`.
fn hue_rotated(rgb: vec3f, degrees: f32) -> vec3f {
    let angle = radians(degrees);
    let c = cos(angle);
    let s = sin(angle);
    let r = vec3f(
        0.2126 + 0.7874 * c - 0.2126 * s,
        0.7152 - 0.7152 * c - 0.7152 * s,
        0.0722 - 0.0722 * c + 0.9278 * s,
    );
    let g = vec3f(
        0.2126 - 0.2126 * c + 0.143 * s,
        0.7152 + 0.2848 * c + 0.140 * s,
        0.0722 - 0.0722 * c - 0.283 * s,
    );
    let b = vec3f(
        0.2126 - 0.2126 * c - 0.7874 * s,
        0.7152 - 0.7152 * c + 0.7152 * s,
        0.0722 + 0.9278 * c + 0.0722 * s,
    );
    return vec3f(dot(r, rgb), dot(g, rgb), dot(b, rgb));
}

fn effect(input: EffectInput) -> vec4f {
    let pixel = unpremultiply(source_load(vec2i(floor(input.position))));
    let amount = params.op.y;
    var rgb = pixel.rgb;
    switch u32(params.op.x) {
        case BRIGHTNESS: {
            rgb = rgb * max(amount, 0.0);
        }
        case CONTRAST: {
            rgb = (rgb - 0.5) * max(amount, 0.0) + 0.5;
        }
        case SATURATION: {
            rgb = saturated(rgb, max(amount, 0.0));
        }
        case HUE: {
            rgb = hue_rotated(rgb, amount);
        }
        case GRAYSCALE: {
            rgb = saturated(rgb, 1.0 - clamp(amount, 0.0, 1.0));
        }
        case INVERT: {
            rgb = mix(rgb, 1.0 - rgb, clamp(amount, 0.0, 1.0));
        }
        case TINT: {
            rgb = mix(rgb, params.color.rgb, clamp(amount, 0.0, 1.0) * params.color.a);
        }
        case VIGNETTE: {
            // 0 at the content box's centre, 1 at its corners.
            let distance = length(input.uv - 0.5) * sqrt(2.0);
            let t = clamp((distance - params.op.z) / max(params.op.w, 1e-4), 0.0, 1.0);
            rgb = rgb * (1.0 - clamp(amount, 0.0, 1.0) * t * t * (3.0 - 2.0 * t));
        }
        default: {}
    }
    return premultiply(vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), pixel.a));
}
