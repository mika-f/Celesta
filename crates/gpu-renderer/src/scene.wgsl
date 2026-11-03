// Draws a plugin's scene (see `scene.rs`) in two passes.
//
// `vs_main` and a fragment entry point for the material's alpha mode
// (`fs_opaque`, `fs_mask`, `fs_blend`) draw each draw's skinned vertices
// (from `scene_skin.wgsl`) through the camera onto a multisampled canvas of
// linear, premultiplied color with depth. An unlit material's color is its
// base color times its texture. `scene_mtoon.wgsl`, compiled after this
// file as one module, draws MToon materials with the same bindings.
//
// `vs_full`/`fs_convert` then take the resolved canvas to the layer's
// encoding: un-premultiplied, encoded linear to sRGB, and premultiplied
// again, as 8-bit values.

struct Light {
    // Towards the light, length 1.
    to_light: vec4<f32>,
    // Its color times its intensity.
    color: vec4<f32>,
};

// `scene.rs`'s `FrameUniform`.
struct Frame {
    // Projection × view.
    view_projection: mat4x4<f32>,
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    // The camera's position in world space; `w` 1 for a perspective
    // camera, 0 for an orthographic one.
    eye: vec4<f32>,
    // The camera's +Z, towards the viewer, in world space.
    forward: vec4<f32>,
    // Time in seconds, the canvas's height over its width, the number of
    // `lights`.
    params: vec4<f32>,
    // The main light first; black when there is none.
    lights: array<Light, 4>,
    // The RGB of `SH(n)`'s nine coefficients, in Unity's `ShadeSH9` order.
    ambient: array<vec4<f32>, 9>,
};

// `scene.rs`'s `DrawUniform`. An unlit draw reads `world` to `uv_v`.
struct Draw {
    world: mat4x4<f32>,
    // Takes the model's normals to world space, up to a positive scale.
    normal_matrix: mat3x3<f32>,
    base_color: vec4<f32>,
    // The texture transform's rows: `u' = dot(uv_u.xyz, (u, v, 1))`, and
    // the same for `v'`. `uv_u.w` is a mask material's alpha cutoff.
    uv_u: vec4<f32>,
    uv_v: vec4<f32>,
    // MToon: the shade color and the shading shift factor.
    shade: vec4<f32>,
    // The emissive factor and the shading shift texture's scale.
    emissive: vec4<f32>,
    // The matcap factor and the shading toony factor.
    matcap: vec4<f32>,
    // The parametric rim color and the rim lighting mix.
    rim: vec4<f32>,
    // The rim's fresnel power and lift, the GI equalization factor, and
    // the normal texture's scale.
    rim_shape: vec4<f32>,
    // The outline color and the outline lighting mix.
    outline: vec4<f32>,
    // The outline width factor and mode (0 none, 1 world, 2 screen), then
    // `world`'s handedness: −1 when it mirrors, else 1, the sign the
    // normal matrix and the bitangent take.
    outline_width: vec4<f32>,
    // How far the UV animation has scrolled in x and y at the frame's time,
    // before the mask scales it; its rotation in turns a second; and 1 when
    // the material has a normal texture.
    uv_animation: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> frame: Frame;
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

// `uv` through the base color texture's transform.
fn transformed_uv(uv: vec2<f32>) -> vec2<f32> {
    let point = vec3<f32>(uv, 1.0);
    return vec2<f32>(dot(draw.uv_u.xyz, point), dot(draw.uv_v.xyz, point));
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = frame.view_projection * (draw.world * input.position);
    output.uv = transformed_uv(input.uv);
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
    return Covered(vec4<f32>(color.rgb, 1.0), coverage(color.a));
}

// The samples an `alpha` against the draw's cutoff covers.
fn coverage(alpha: f32) -> u32 {
    let cutoff = draw.uv_u.w;
    let sharpened = (alpha - cutoff) / max(fwidth(alpha), 1e-5) + 0.5;
    let samples = u32(clamp(sharpened, 0.0, 1.0) * SAMPLES + 0.5);
    return (1u << samples) - 1u;
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
