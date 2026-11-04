// The built-in unlit scene material, written against Celesta's material
// interface (`scene_material_prelude.wgsl`) with one texture slot: its
// base color texture, white when it has none. Its color is its base color
// times that texture, sampled through the texture's transform.

struct Params {
    base_color: vec4f,
    // The texture transform's rows: `u' = dot(uv_u.xyz, (u, v, 1))`, and
    // the same for `v'`.
    uv_u: vec4f,
    uv_v: vec4f,
};

fn material_fragment(fragment: Fragment) -> vec4f {
    let point = vec3f(fragment.uv, 1.0);
    let uv = vec2f(dot(params.uv_u.xyz, point), dot(params.uv_v.xyz, point));
    return params.base_color * textureSample(texture_0, sampler_0, uv);
}
