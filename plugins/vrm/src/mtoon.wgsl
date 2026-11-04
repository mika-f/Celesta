// Ported from UniVRM (https://github.com/vrm-c/UniVRM), under
// Packages/VRM10/MToon10/Shaders/: vrmc_materials_mtoon.shader and the
// vrmc_materials_mtoon_*.hlsl it includes (forward_vertex, forward_fragment,
// geometry_uv, geometry_normal, geometry_vertex, lighting_mtoon,
// lighting_unity, utility).
// MIT License, Copyright (c) 2020 VRM Consortium, Copyright (c) 2018 Masataka
// SUMI for MToon.

// MToon, as UniVRM's MToon10 shader draws it in Unity's built-in render
// pipeline, written against Celesta's scene material interface (its
// prelude gives `Fragment`, `Vertex`, `VertexOffset`, `Light`, `frame`,
// `params`, `ambient_light`, `linearstep`, `safe_normalize`, and the
// texture slots).
//
// Its nine texture slots are `mtoon.rs`'s `SLOTS`, in order: the base
// color (0), the shade multiply (1), the shading shift (2), the normal (3),
// the emissive (4), the matcap (5), the rim multiply (6), the outline width
// (7, read by the vertex stage), and the UV animation mask (8). An empty
// slot binds Unity's default texel for the property: black for the shading
// shift and the matcap, white for the rest.
//
// `material_fragment` is the shaded color (lights, ambient, emission, rim,
// and matcap) with the base color's alpha. `material_outline_vertex` and
// `material_outline_fragment` draw the outline, mixing its color toward
// that shaded color.

// `mtoon.rs`'s `params`.
struct Params {
    base_color: vec4f,
    // The base color texture's transform's rows.
    uv_u: vec4f,
    uv_v: vec4f,
    // The shade color and the shading shift factor.
    shade: vec4f,
    // The emissive factor and the shading shift texture's scale.
    emissive: vec4f,
    // The matcap factor and the shading toony factor.
    matcap: vec4f,
    // The parametric rim color and the rim lighting mix.
    rim: vec4f,
    // The rim's fresnel power and lift, the GI equalization factor, and
    // the normal texture's scale.
    rim_shape: vec4f,
    // The outline color and the outline lighting mix.
    outline: vec4f,
    // The outline width factor and mode (0 none, 1 world, 2 screen).
    outline_width: vec4f,
    // How far the UV animation has scrolled in x and y at the frame's time,
    // before the mask scales it; its rotation in turns a second; and 1 when
    // the material has a normal texture.
    uv_animation: vec4f,
};

const TAU: f32 = 6.283185307179586;

// What `mtoon_color` shades: a point of a surface, in world space.
struct Surface {
    position: vec3f,
    // Length 1, facing the side drawn; on an outline, outward.
    normal: vec3f,
    // Towards the viewer, length 1.
    view: vec3f,
    // The texture coordinates every slot but the matcap, the outline width,
    // and the UV animation mask samples at: through the base color
    // texture's transform, then the UV animation.
    uv: vec2f,
    // The base color times its texture: the lit color and the alpha.
    lit: vec4f,
};

// `uv` through the base color texture's transform.
fn transformed_uv(uv: vec2f) -> vec2f {
    let point = vec3f(uv, 1.0);
    return vec2f(dot(params.uv_u.xyz, point), dot(params.uv_v.xyz, point));
}

// The outline pass's vertex offset: along the normal by the outline width
// times the width texture's green, sampled at the geometry's raw texture
// coordinates. In world mode the width is in metres, whatever `world`'s
// scale. In screen mode it is a share of the canvas's height, applied in
// clip space, up to the distance at which a metre fills the height, and
// fading out as the normal turns to face the camera.
fn material_outline_vertex(vertex: Vertex) -> VertexOffset {
    let width = params.outline_width.x
        * textureSampleLevel(texture_7, sampler_7, vertex.uv, 0.0).g;
    let n = vertex.normal;
    if params.outline_width.y == 1.0 {
        return VertexOffset(n * width, vec2f(0.0));
    }
    if params.outline_width.y == 2.0 {
        let n_view = safe_normalize((frame.view * vec4f(n, 0.0)).xyz);
        let n_clip = (frame.projection * vec4f(n_view, 0.0)).xy;
        // A normal facing the camera has no direction on screen.
        if dot(n_clip, n_clip) > 0.0 {
            var offset = normalize(n_clip) * 2.0 * width
                * min(vertex.clip.w, frame.projection[1][1]);
            // Clip space x spans the canvas's width, not its height.
            offset.x *= frame.height_over_width;
            let facing = saturate(1.0 - n_view.z * n_view.z);
            return VertexOffset(vec3f(0.0), offset * facing);
        }
    }
    return VertexOffset(vec3f(0.0), vec2f(0.0));
}

// `uv` moved by the UV animation, as UniVRM's `GetMToonGeometry_Uv` does
// after the base color texture's transform, in glTF's `v`-down texture
// space: scrolled by the time times the scroll speed (`mtoon.rs`'s
// `uv_scroll`, on the CPU), then turned about the texture's centre by the
// time times the rotation speed. The mask's blue, sampled at `uv`, scales
// the time.
fn animated_uv(uv: vec2f) -> vec2f {
    let mask = textureSample(texture_8, sampler_8, uv).b;
    let t = frame.time * mask;
    let angle = fract(t * params.uv_animation.z) * TAU;
    let d = uv + mask * params.uv_animation.xy - 0.5;
    let c = cos(angle);
    let s = sin(angle);
    return vec2f(c * d.x + s * d.y, -s * d.x + c * d.y) + 0.5;
}

// `n`, a geometric normal facing the side drawn, tilted by the normal
// texture's `packed` texel in the tangent space of `tangent` (the
// fragment's). Decoded as Unity's `UnpackNormalWithScale` reads its desktop
// layout: red and green scaled by the normal texture's scale, blue unread.
// `n` itself when the material has no normal texture.
fn normal_mapped(n: vec3f, tangent: vec4f, packed: vec4f) -> vec3f {
    let xy = (packed.rg * 2.0 - 1.0) * params.rim_shape.w;
    let z = sqrt(saturate(1.0 - dot(xy, xy)));
    let t = tangent.xyz;
    let sign = select(-1.0, 1.0, tangent.w > 0.0);
    let b = safe_normalize(sign * cross(n, t));
    return select(n, safe_normalize(t * xy.x + b * xy.y + n * z), params.uv_animation.w != 0.0);
}

// The surface `fragment` shows with the geometric normal `normal`, which
// then follows the normal texture.
fn mtoon_surface(fragment: Fragment, normal: vec3f) -> Surface {
    let uv = animated_uv(transformed_uv(fragment.uv));
    let lit = params.base_color * textureSample(texture_0, sampler_0, uv);
    let packed = textureSample(texture_3, sampler_3, uv);
    let mapped = normal_mapped(normal, fragment.tangent, packed);
    return Surface(fragment.position, mapped, fragment.view, uv, lit);
}

// Where the matcap texture is sampled for the normal `n` seen along `v`:
// UniVRM's view-space normal, its axes screen right and screen up. Unity's
// `cross(V, +Y)` and `cross(right, V)` are in its left-handed space; in
// this right-handed one the same axes are `cross(+Y, V)` and
// `cross(V, right)`. Looking along ±Y, right is the view's x axis. glTF's
// `v` counts down, so a normal leaning up samples the texture's top row.
fn matcap_uv(n: vec3f, v: vec3f) -> vec2f {
    let across = cross(vec3f(0.0, 1.0, 0.0), v);
    let view_x = vec3f(frame.view[0].x, frame.view[1].x, frame.view[2].x);
    let right = safe_normalize(select(view_x, across, dot(across, across) >= 1e-12));
    let up = safe_normalize(cross(v, right));
    return vec2f(dot(right, n) * 0.5 + 0.5, 0.5 - dot(up, n) * 0.5);
}

// How far `light` steps `shade` to `lit` at the normal `n`, `shift` and
// `toony` placing the step: UniVRM's `linearstep`, which above toony 1
// runs the other way and at toony 1 is a step.
fn toon_step(n: vec3f, light: Light, shift: f32, toony: f32) -> f32 {
    return linearstep(-1.0 + toony, 1.0 - toony, dot(n, light.to_light.xyz) + shift);
}

// MToon's color of `surface`: the main light's step from shade to lit, the
// ambient light (Unity's `ShadeSH9`), the emission, and the rim (the matcap
// and the parametric rim); then each additional light's step and rim, as
// Unity's ForwardAdd pass adds them. `main_only` leaves out the additional
// lights, which an outline does not see.
fn mtoon_color(surface: Surface, main_only: bool) -> vec3f {
    let n = surface.normal;
    let v = surface.view;
    let uv = surface.uv;
    let lit = surface.lit.rgb;
    // Every texture is sampled here, in uniform control flow.
    let shift = params.shade.w + textureSample(texture_2, sampler_2, uv).r * params.emissive.w;
    let toony = params.matcap.w;
    let shade = params.shade.rgb * textureSample(texture_1, sampler_1, uv).rgb;
    let emission = params.emissive.rgb * textureSample(texture_4, sampler_4, uv).rgb;
    // Black, so nothing, without a matcap texture.
    let matcap = params.matcap.rgb * textureSample(texture_5, sampler_5, matcap_uv(n, v)).rgb;
    let rim_multiply = textureSample(texture_6, sampler_6, uv).rgb;

    // The parametric rim: `pow(x, power)`, 0 at `x` 0, where WGSL's `pow`
    // is undefined.
    let x = saturate(1.0 - dot(n, v) + params.rim_shape.y);
    let parametric = select(pow(x, max(params.rim_shape.x, 1.0 / 1024.0)), 0.0, x <= 0.0)
        * params.rim.rgb;
    let rim = (matcap + parametric) * rim_multiply;
    let rim_mix = params.rim.w;

    // Black when the frame has no lights.
    let main_light = frame.lights[0];
    var color = mix(shade, lit, toon_step(n, main_light, shift, toony)) * main_light.color.rgb;
    let vertical = 0.5 * (ambient_light(vec3f(0.0, 1.0, 0.0)) + ambient_light(vec3f(0.0, -1.0, 0.0)));
    color += lit * mix(ambient_light(n), vertical, params.rim_shape.z);
    color += emission;
    color += rim * mix(vec3f(1.0), main_light.color.rgb + ambient_light(n), rim_mix);

    if !main_only {
        let count = min(frame.light_count, MAX_LIGHTS);
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

// The surface's color. Its normal turns to face the viewer's side on a
// back face, which only a double-sided material draws.
fn material_fragment(fragment: Fragment) -> vec4f {
    let normal = select(-fragment.normal, fragment.normal, fragment.front_facing);
    let surface = mtoon_surface(fragment, normal);
    return vec4f(mtoon_color(surface, false), surface.lit.a);
}

// The outline's color: its own, mixed toward the shaded color by the
// outline lighting mix. Unity's built-in pipeline draws the outline in
// ForwardBase only, so the additional lights do not reach it. Its pipeline
// culls front faces, so every fragment is on a back face; UniVRM negates
// the outline's normal and turning it to face the viewer negates it again,
// so it shades with the outward normal, not turned.
fn material_outline_fragment(fragment: Fragment) -> vec4f {
    let surface = mtoon_surface(fragment, fragment.normal);
    let color = params.outline.rgb * mix(vec3f(1.0), mtoon_color(surface, true), params.outline.w);
    return vec4f(color, surface.lit.a);
}
