// What every scene material's WGSL is written against (`scene_material.rs`):
// declared before the material's own code, which defines `struct Params`
// and `fn material_fragment(fragment: Fragment) -> vec4f`, and may define
// `fn material_vertex(vertex: Vertex) -> VertexOffset`,
// `fn material_outline_fragment(fragment: Fragment) -> vec4f`, and, with
// it, `fn material_outline_vertex(vertex: Vertex) -> VertexOffset`.
//
// The material's texture slots follow this file: slot `k`'s
// `texture_k: texture_2d<f32>` at group 2 binding `2k` and its
// `sampler_k: sampler` at binding `2k + 1`, read by both stages.

struct Light {
    // Towards the light, length 1.
    to_light: vec4f,
    // Its color times its intensity; black when the light is absent.
    color: vec4f,
};

// The frame: the camera, the lights, and the time. 512 bytes.
struct Frame {
    // Projection × view.
    view_projection: mat4x4f,
    view: mat4x4f,
    projection: mat4x4f,
    // The camera's position in world space; `w` 1 for a perspective
    // camera, 0 for an orthographic one.
    eye: vec4f,
    // The camera's +Z, towards the viewer, in world space.
    forward: vec4f,
    // The frame's time in seconds.
    time: f32,
    // The canvas's height over its width.
    height_over_width: f32,
    // How many of `lights` are lit: 0 to `MAX_LIGHTS`.
    light_count: u32,
    celesta_reserved: u32,
    // The main light first.
    lights: array<Light, MAX_LIGHTS>,
    // The RGB of the ambient light's nine spherical harmonics
    // coefficients, in `ambient_light`'s order.
    ambient: array<vec4f, 9>,
};

// The draw. 128 bytes.
struct Draw {
    world: mat4x4f,
    // Takes the model's normals to world space, up to a positive scale.
    normal_matrix: mat3x3f,
    // −1 when `world` mirrors, else 1.
    handedness: f32,
    // A mask material's alpha cutoff, else 0.
    alpha_cutoff: f32,
};

// One skinned vertex, in world space.
struct Vertex {
    position: vec3f,
    // `safe_normalize(normal_matrix × the skinned normal)`.
    normal: vec3f,
    // `xyz`: `safe_normalize` of `world`'s 3×3 times the skinned tangent;
    // `w`: the tangent's sign times `draw.handedness`. All 0 when the
    // geometry has no tangents.
    tangent: vec4f,
    // The geometry's first texture coordinates.
    uv: vec2f,
    // `view_projection × position`.
    clip: vec4f,
};

// How a vertex hook moves its vertex: `world` is added to the position
// before it is projected, then `clip` to the projected `xy`.
struct VertexOffset {
    world: vec3f,
    clip: vec2f,
};

// One fragment of a surface, in world space.
struct Fragment {
    // After the vertex offset.
    position: vec3f,
    // Interpolated and renormalised. Not turned towards the viewer on a
    // back face: `front_facing` says which side is drawn.
    normal: vec3f,
    // `xyz` renormalised, `w` interpolated.
    tangent: vec4f,
    uv: vec2f,
    // Towards the viewer, length 1: towards the eye in perspective, along
    // the camera's +Z orthographic.
    view: vec3f,
    front_facing: bool,
    // The fragment's position on the canvas, in pixels.
    pixel: vec2f,
};

@group(0) @binding(0)
var<uniform> frame: Frame;
@group(1) @binding(0)
var<uniform> draw: Draw;
// The material declares `struct Params`.
@group(1) @binding(1)
var<uniform> params: Params;

// The most lights a frame has.
const MAX_LIGHTS: u32 = 4u;

// `saturate((t − start) / (end − start))`: 0 at `start`, 1 at `end`,
// linear between and clamped beyond. When `end` equals `start`, a step: 0
// up to `start` and 1 above it.
fn linearstep(start: f32, end: f32, t: f32) -> f32 {
    let span = end - start;
    if span == 0.0 {
        return select(0.0, 1.0, t > start);
    }
    return saturate((t - start) / span);
}

// The ambient light from the direction `n`, length 1: `frame.ambient`'s
// coefficients c0…c8 times x, y, z, 1, xy, yz, z², zx, and x² − y².
fn ambient_light(n: vec3f) -> vec3f {
    let c = frame.ambient;
    return c[0].rgb * n.x + c[1].rgb * n.y + c[2].rgb * n.z + c[3].rgb
        + c[4].rgb * (n.x * n.y) + c[5].rgb * (n.y * n.z) + c[6].rgb * (n.z * n.z)
        + c[7].rgb * (n.z * n.x) + c[8].rgb * (n.x * n.x - n.y * n.y);
}

// `v` scaled to length 1, or `v` itself when it has no length.
fn safe_normalize(v: vec3f) -> vec3f {
    let length_squared = dot(v, v);
    if length_squared > 0.0 {
        return v * inverseSqrt(length_squared);
    }
    return v;
}
