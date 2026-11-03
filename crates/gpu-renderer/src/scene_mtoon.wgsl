
// MToon, as UniVRM's MToon10 shader draws it in Unity's built-in render
// pipeline. Compiled after `scene.wgsl`, as one module: it reads that
// file's `frame` (group 0) and `draw` (group 1), and its `base_texture` and
// `base_sampler` as slot 0 of the material's group 2.
//
// `vs_mtoon` and a fragment entry point for the material's alpha mode
// (`fs_mtoon_opaque`, `fs_mtoon_mask`, `fs_mtoon_blend`) draw a draw's
// skinned vertices. `mtoon_color` is the shaded color, which an outline
// pass also uses.

// Group 2: slot `k`'s texture at binding `2k`, its sampler at `2k + 1`, in
// `scene.rs`'s `MTOON_SLOTS` order. An empty slot binds Unity's default
// texel for the property: black for the shading shift and the matcap,
// white for the rest.
@group(2) @binding(2)
var shade_multiply_texture: texture_2d<f32>;
@group(2) @binding(3)
var shade_multiply_sampler: sampler;
@group(2) @binding(4)
var shading_shift_texture: texture_2d<f32>;
@group(2) @binding(5)
var shading_shift_sampler: sampler;
@group(2) @binding(6)
var normal_texture: texture_2d<f32>;
@group(2) @binding(7)
var normal_sampler: sampler;
@group(2) @binding(8)
var emissive_texture: texture_2d<f32>;
@group(2) @binding(9)
var emissive_sampler: sampler;
@group(2) @binding(10)
var matcap_texture: texture_2d<f32>;
@group(2) @binding(11)
var matcap_sampler: sampler;
@group(2) @binding(12)
var rim_multiply_texture: texture_2d<f32>;
@group(2) @binding(13)
var rim_multiply_sampler: sampler;
// Read by the vertex stage.
@group(2) @binding(14)
var outline_width_texture: texture_2d<f32>;
@group(2) @binding(15)
var outline_width_sampler: sampler;
@group(2) @binding(16)
var uv_animation_mask_texture: texture_2d<f32>;
@group(2) @binding(17)
var uv_animation_mask_sampler: sampler;

// `scene_skin.wgsl`'s `Skinned`, then the geometry's texture coordinates.
struct MtoonInput {
    @location(0) position: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
};

struct MtoonVarying {
    @builtin(position) clip: vec4<f32>,
    // Through the base color texture's transform.
    @location(0) uv: vec2<f32>,
    @location(1) position: vec3<f32>,
    // In world space, length 1 at the vertices.
    @location(2) normal: vec3<f32>,
};

// What `mtoon_color` shades: a point of a surface, in world space.
struct Surface {
    position: vec3<f32>,
    // Length 1, facing the side drawn.
    normal: vec3<f32>,
    // Towards the viewer, length 1.
    view: vec3<f32>,
    // The texture coordinates every slot but the matcap and the outline
    // width samples at.
    uv: vec2<f32>,
    // The base color times its texture: the lit color and the alpha.
    lit: vec4<f32>,
};

// `value` scaled to length 1, or left as it is when it has no length.
fn unit(value: vec3<f32>) -> vec3<f32> {
    let length_squared = dot(value, value);
    if length_squared > 0.0 {
        return value * inverseSqrt(length_squared);
    }
    return value;
}

@vertex
fn vs_mtoon(input: MtoonInput) -> MtoonVarying {
    var output: MtoonVarying;
    let position = draw.world * input.position;
    output.clip = frame.view_projection * position;
    output.uv = transformed_uv(input.uv);
    output.position = position.xyz;
    output.normal = unit(draw.normal_matrix * input.normal.xyz);
    return output;
}

// 0 below `start`, 1 above `end`, linear between. When `end` is not above
// `start` (toony 1 or more) it is a step, 0 up to `start`: Unity divides
// by 0 there and `max(NaN, 0)` gives 0 at `t = start`.
fn linearstep(start: f32, end: f32, t: f32) -> f32 {
    if end <= start {
        return select(0.0, 1.0, t > start);
    }
    return saturate((t - start) / (end - start));
}

// The ambient light from the direction `n`: Unity's `ShadeSH9`.
fn sh(n: vec3<f32>) -> vec3<f32> {
    let c = frame.ambient;
    return c[0].rgb * n.x + c[1].rgb * n.y + c[2].rgb * n.z + c[3].rgb
        + c[4].rgb * (n.x * n.y) + c[5].rgb * (n.y * n.z) + c[6].rgb * (n.z * n.z)
        + c[7].rgb * (n.z * n.x) + c[8].rgb * (n.x * n.x - n.y * n.y);
}

// The surface a fragment shows. The normal turns to face the viewer's side
// on a back face, which only a double-sided material draws.
fn mtoon_surface(input: MtoonVarying, front_facing: bool) -> Surface {
    let normal = unit(input.normal);
    // In perspective, towards the eye; orthographic, along the camera's +Z.
    let view = select(
        unit(frame.forward.xyz),
        unit(frame.eye.xyz - input.position),
        frame.eye.w == 1.0,
    );
    let lit = draw.base_color * textureSample(base_texture, base_sampler, input.uv);
    return Surface(
        input.position,
        select(-normal, normal, front_facing),
        view,
        input.uv,
        lit,
    );
}

// MToon's color of `surface`: the main light's step from shade to lit and
// the ambient light. `main_only` leaves out the additional lights, which
// an outline does not see.
fn mtoon_color(surface: Surface, main_only: bool) -> vec3<f32> {
    let n = surface.normal;
    let uv = surface.uv;
    let lit = surface.lit.rgb;
    let shift = draw.shade.w
        + textureSample(shading_shift_texture, shading_shift_sampler, uv).r * draw.emissive.w;
    let toony = draw.matcap.w;
    let shade = draw.shade.rgb * textureSample(shade_multiply_texture, shade_multiply_sampler, uv).rgb;
    // Black when the frame has no lights.
    let main_light = frame.lights[0];
    let step = linearstep(-1.0 + toony, 1.0 - toony, dot(n, main_light.to_light.xyz) + shift);
    var color = mix(shade, lit, step) * main_light.color.rgb;
    let vertical = 0.5 * (sh(vec3<f32>(0.0, 1.0, 0.0)) + sh(vec3<f32>(0.0, -1.0, 0.0)));
    color += lit * mix(sh(n), vertical, draw.rim_shape.z);
    return color;
}

@fragment
fn fs_mtoon_opaque(
    input: MtoonVarying,
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4<f32> {
    let surface = mtoon_surface(input, front_facing);
    return vec4<f32>(mtoon_color(surface, false), 1.0);
}

@fragment
fn fs_mtoon_mask(
    input: MtoonVarying,
    @builtin(front_facing) front_facing: bool,
) -> Covered {
    let surface = mtoon_surface(input, front_facing);
    let color = mtoon_color(surface, false);
    return Covered(vec4<f32>(color, 1.0), coverage(surface.lit.a));
}

@fragment
fn fs_mtoon_blend(
    input: MtoonVarying,
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4<f32> {
    let surface = mtoon_surface(input, front_facing);
    let color = mtoon_color(surface, false);
    return vec4<f32>(color, clamp(surface.lit.a, 0.0, 1.0));
}
