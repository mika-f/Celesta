
// MToon, as UniVRM's MToon10 shader draws it in Unity's built-in render
// pipeline. Compiled after `scene.wgsl`, as one module: it reads that
// file's `frame` (group 0) and `draw` (group 1), and its `base_texture` and
// `base_sampler` as slot 0 of the material's group 2.
//
// `vs_mtoon` and a fragment entry point for the material's alpha mode
// (`fs_mtoon_opaque`, `fs_mtoon_mask`, `fs_mtoon_blend`) draw a draw's
// skinned vertices. `mtoon_color` is the shaded color (lights, ambient,
// emission, rim, and matcap), which an outline pass also uses.

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
    // Through the base color texture's transform, before the UV animation.
    @location(0) uv: vec2<f32>,
    @location(1) position: vec3<f32>,
    // In world space, length 1 at the vertices.
    @location(2) normal: vec3<f32>,
    // The tangent through `world`'s 3×3, length 1 at the vertices; `w` the
    // bitangent's sign, ±1 at the vertices, flipped when `world` mirrors.
    @location(3) tangent: vec4<f32>,
};

const TAU: f32 = 6.283185307179586;

// What `mtoon_color` shades: a point of a surface, in world space.
struct Surface {
    position: vec3<f32>,
    // Length 1, facing the side drawn.
    normal: vec3<f32>,
    // Towards the viewer, length 1.
    view: vec3<f32>,
    // The texture coordinates every slot but the matcap, the outline width,
    // and the UV animation mask samples at: through the base color
    // texture's transform, then the UV animation.
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
    // UniVRM's `MToon_GetTangentToWorld`: the tangent through the world
    // matrix itself, not the normal matrix, and its sign times
    // `unity_WorldTransformParams.w`.
    let world = mat3x3<f32>(draw.world[0].xyz, draw.world[1].xyz, draw.world[2].xyz);
    let sign = select(-1.0, 1.0, input.tangent.w > 0.0) * draw.outline_width.z;
    output.tangent = vec4<f32>(unit(world * input.tangent.xyz), sign);
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

// `uv` moved by the UV animation, as UniVRM's `GetMToonGeometry_Uv` does
// after the base color texture's transform, in glTF's `v`-down texture
// space: scrolled by the time times the scroll speed, then turned about
// the texture's centre by the time times the rotation speed. The mask's
// blue, sampled at `uv`, scales the time.
fn animated_uv(uv: vec2<f32>) -> vec2<f32> {
    let mask = textureSample(uv_animation_mask_texture, uv_animation_mask_sampler, uv).b;
    let t = frame.params.x * mask;
    let angle = fract(t * draw.uv_animation.z) * TAU;
    let d = uv + t * draw.uv_animation.xy - 0.5;
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(c * d.x + s * d.y, -s * d.x + c * d.y) + 0.5;
}

// `n`, a geometric normal facing the side drawn, tilted by the normal
// texture's `packed` texel in the tangent space of `tangent` (a varying's
// interpolated value). Decoded as Unity's `UnpackNormalWithScale` reads its
// desktop layout: red and green scaled by the normal texture's scale, blue
// unread. `n` itself when the material has no normal texture.
fn normal_mapped(n: vec3<f32>, tangent: vec4<f32>, packed: vec4<f32>) -> vec3<f32> {
    let xy = (packed.rg * 2.0 - 1.0) * draw.rim_shape.w;
    let z = sqrt(saturate(1.0 - dot(xy, xy)));
    let t = unit(tangent.xyz);
    let sign = select(-1.0, 1.0, tangent.w > 0.0);
    let b = unit(sign * cross(n, t));
    return select(n, unit(t * xy.x + b * xy.y + n * z), draw.uv_animation.w != 0.0);
}

// The surface a fragment shows. The normal turns to face the viewer's side
// on a back face, which only a double-sided material draws, and then
// follows the normal texture.
fn mtoon_surface(input: MtoonVarying, front_facing: bool) -> Surface {
    let geometric = unit(input.normal);
    let uv = animated_uv(input.uv);
    // In perspective, towards the eye; orthographic, along the camera's +Z.
    let view = select(
        unit(frame.forward.xyz),
        unit(frame.eye.xyz - input.position),
        frame.eye.w == 1.0,
    );
    let lit = draw.base_color * textureSample(base_texture, base_sampler, uv);
    let packed = textureSample(normal_texture, normal_sampler, uv);
    let normal = normal_mapped(
        select(-geometric, geometric, front_facing),
        input.tangent,
        packed,
    );
    return Surface(input.position, normal, view, uv, lit);
}

// Where the matcap texture is sampled for the normal `n` seen along `v`:
// UniVRM's view-space normal, its axes screen right and screen up. Unity's
// `cross(V, +Y)` and `cross(right, V)` are in its left-handed space; in
// this right-handed one the same axes are `cross(+Y, V)` and
// `cross(V, right)`. Looking along ±Y, right is the view's x axis. glTF's
// `v` counts down, so a normal leaning up samples the texture's top row.
fn matcap_uv(n: vec3<f32>, v: vec3<f32>) -> vec2<f32> {
    let across = cross(vec3<f32>(0.0, 1.0, 0.0), v);
    let view_x = vec3<f32>(frame.view[0].x, frame.view[1].x, frame.view[2].x);
    let right = unit(select(view_x, across, dot(across, across) >= 1e-12));
    let up = unit(cross(v, right));
    return vec2<f32>(dot(right, n) * 0.5 + 0.5, 0.5 - dot(up, n) * 0.5);
}

// How far `light` steps `shade` to `lit` at the normal `n`, `shift` and
// `toony` placing the step.
fn toon_step(n: vec3<f32>, light: Light, shift: f32, toony: f32) -> f32 {
    return linearstep(-1.0 + toony, 1.0 - toony, dot(n, light.to_light.xyz) + shift);
}

// MToon's color of `surface`: the main light's step from shade to lit, the
// ambient light, the emission, and the rim (the matcap and the parametric
// rim); then each additional light's step and rim, as Unity's ForwardAdd
// pass adds them. `main_only` leaves out the additional lights, which an
// outline does not see.
fn mtoon_color(surface: Surface, main_only: bool) -> vec3<f32> {
    let n = surface.normal;
    let v = surface.view;
    let uv = surface.uv;
    let lit = surface.lit.rgb;
    // Every texture is sampled here, in uniform control flow.
    let shift = draw.shade.w
        + textureSample(shading_shift_texture, shading_shift_sampler, uv).r * draw.emissive.w;
    let toony = draw.matcap.w;
    let shade = draw.shade.rgb * textureSample(shade_multiply_texture, shade_multiply_sampler, uv).rgb;
    let emission = draw.emissive.rgb * textureSample(emissive_texture, emissive_sampler, uv).rgb;
    // Black, so nothing, without a matcap texture.
    let matcap = draw.matcap.rgb
        * textureSample(matcap_texture, matcap_sampler, matcap_uv(n, v)).rgb;
    let rim_multiply = textureSample(rim_multiply_texture, rim_multiply_sampler, uv).rgb;

    // The parametric rim: `pow(x, power)`, 0 at `x` 0, where WGSL's `pow`
    // is undefined.
    let x = saturate(1.0 - dot(n, v) + draw.rim_shape.y);
    let parametric = select(pow(x, max(draw.rim_shape.x, 1.0 / 1024.0)), 0.0, x <= 0.0)
        * draw.rim.rgb;
    let rim = (matcap + parametric) * rim_multiply;
    let rim_mix = draw.rim.w;

    // Black when the frame has no lights.
    let main_light = frame.lights[0];
    var color = mix(shade, lit, toon_step(n, main_light, shift, toony)) * main_light.color.rgb;
    let vertical = 0.5 * (sh(vec3<f32>(0.0, 1.0, 0.0)) + sh(vec3<f32>(0.0, -1.0, 0.0)));
    color += lit * mix(sh(n), vertical, draw.rim_shape.z);
    color += emission;
    color += rim * mix(vec3<f32>(1.0), main_light.color.rgb + sh(n), rim_mix);

    if !main_only {
        let count = min(u32(frame.params.z), 4u);
        for (var i = 1u; i < count; i++) {
            let light = frame.lights[i];
            // MToon's additional-light shadow: 1 facing the light, down to
            // 0 facing away; not applied inside the step.
            let shadow = 0.5 * (min(0.0, dot(n, light.to_light.xyz)) + 1.0);
            let radiance = light.color.rgb * shadow;
            color += mix(shade, lit, toon_step(n, light, shift, toony)) * radiance;
            // `lerp(0, radiance, rim_mix)`: no rim from it unmixed.
            color += rim * radiance * rim_mix;
        }
    }
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
