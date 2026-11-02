// Draws a plugin's scene (see `scene.rs`) in two passes.
//
// `vs_main` and a fragment entry point for the material's alpha mode
// (`fs_opaque`, `fs_mask`, `fs_blend`) draw each draw's skinned vertices
// (from `scene_skin.wgsl`) through the camera onto a multisampled canvas of
// linear, premultiplied color with depth. An unlit material's color is its
// base color times its texture.
//
// `vs_full`/`fs_convert` then take the resolved canvas to the layer's
// encoding: un-premultiplied, encoded linear to sRGB, and premultiplied
// again, as 8-bit values.

struct Camera {
    // Projection × view.
    view_projection: mat4x4<f32>,
};

struct Draw {
    world: mat4x4<f32>,
    base_color: vec4<f32>,
    // The texture transform's rows: `u' = dot(uv_u.xyz, (u, v, 1))`, and
    // the same for `v'`. `uv_u.w` is a mask material's alpha cutoff.
    uv_u: vec4<f32>,
    uv_v: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;
@group(1) @binding(0)
var<uniform> draw: Draw;
@group(2) @binding(0)
var base_texture: texture_2d<f32>;
@group(2) @binding(1)
var base_sampler: sampler;

struct VertexInput {
    @location(0) position: vec4<f32>,
    // Counting `v` down from the texture's top row, as textures are stored.
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = camera.view_projection * (draw.world * input.position);
    let uv = vec3<f32>(input.uv, 1.0);
    output.uv = vec2<f32>(dot(draw.uv_u.xyz, uv), dot(draw.uv_v.xyz, uv));
    return output;
}

// Samples per pixel of the canvas: `scene.rs`'s `SAMPLES`.
const SAMPLES: f32 = 4.0;

// An unlit material's straight color and alpha.
fn unlit(input: VertexOutput) -> vec4<f32> {
    return draw.base_color * textureSample(base_texture, base_sampler, input.uv);
}

// An opaque material ignores its alpha.
@fragment
fn fs_opaque(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(unlit(input).rgb, 1.0);
}

struct Covered {
    @location(0) color: vec4<f32>,
    @builtin(sample_mask) mask: u32,
};

// A mask material covers as many of a pixel's samples as its alpha,
// sharpened to about a pixel's width around the cutoff, says; each sample
// it covers is opaque. This is alpha to coverage, done here because the
// pipeline's own would store that alpha in the samples too.
@fragment
fn fs_mask(input: VertexOutput) -> Covered {
    let color = unlit(input);
    let cutoff = draw.uv_u.w;
    let sharpened = (color.a - cutoff) / max(fwidth(color.a), 1e-5) + 0.5;
    let samples = u32(clamp(sharpened, 0.0, 1.0) * SAMPLES + 0.5);
    return Covered(vec4<f32>(color.rgb, 1.0), (1u << samples) - 1u);
}

// A blend material's straight color, which the pipeline blends by its
// alpha over the canvas's premultiplied color.
@fragment
fn fs_blend(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = unlit(input);
    return vec4<f32>(color.rgb, clamp(color.a, 0.0, 1.0));
}

@group(0) @binding(0)
var resolved: texture_2d<f32>;

// One triangle over the whole target.
@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

fn encode_srgb(linear: vec3<f32>) -> vec3<f32> {
    let low = linear * 12.92;
    let high = 1.055 * pow(linear, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, linear <= vec3<f32>(0.0031308));
}

@fragment
fn fs_convert(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let color = textureLoad(resolved, vec2<i32>(floor(position.xy)), 0);
    let alpha = clamp(color.a, 0.0, 1.0);
    if alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    // Colors beyond 1 are clipped, not tone mapped.
    let straight = clamp(color.rgb / alpha, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(encode_srgb(straight) * alpha, alpha);
}
