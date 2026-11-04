// The cube's toon shading, written against Celesta's scene material
// interface: the checker stepped from a shaded to a lit color by the main
// light, the ambient light, a rim where a face turns from the camera, and an
// outline drawn by the material's outline pass. It is MToon's shading cut
// down to what this sample uses: one light, no emission, no matcap, no rim
// lift.
//
// Its one texture slot, `texture_0`, is the checker, both the lit color
// and, times the shade color, the shaded one.

// `toon_params` in `lib.rs`.
struct Params {
    // The shade color, times the checker, and the shading shift, added to
    // N·L before the step: below 0, more of the surface is in shade.
    shade: vec4f,
    // The rim color and its fresnel power.
    rim: vec4f,
    // The outline color and how far it is mixed towards the surface's color.
    outline: vec4f,
    // The toony factor, how sharp the step is: 1 a hard step, lower a
    // softer one. The outline width, a share of the canvas's height where a
    // metre fills it. Two unused.
    shape: vec4f,
};

// The checker's color at `uv`, lit and opaque.
fn checker(uv: vec2f) -> vec3f {
    return textureSample(texture_0, sampler_0, uv).rgb;
}

// The color of the surface `lit` with the outward normal `n`, seen along
// `v`: the main light's step from shade to lit, the ambient light, and the
// rim, lit by both.
fn toon_color(n: vec3f, v: vec3f, lit: vec3f) -> vec3f {
    // Black when the frame has no lights.
    let light = frame.lights[0];
    let toony = params.shape.x;
    let step = linearstep(-1.0 + toony, 1.0 - toony, dot(n, light.to_light.xyz) + params.shade.w);
    let ambient = ambient_light(n);
    // `pow(x, power)`, 0 at `x` 0, where WGSL's `pow` is undefined.
    let x = saturate(1.0 - dot(n, v));
    let rim = select(pow(x, params.rim.w), 0.0, x <= 0.0) * params.rim.rgb;

    var color = mix(params.shade.rgb * lit, lit, step) * light.color.rgb;
    color += lit * ambient;
    color += rim * (light.color.rgb + ambient);
    return color;
}

// The cube is single-sided, so every fragment is on a front face and its
// normal faces the viewer's side.
fn material_fragment(fragment: Fragment) -> vec4f {
    return vec4f(toon_color(fragment.normal, fragment.view, checker(fragment.uv)), 1.0);
}

// The outline's offset, as MToon's in screen coordinates: along the normal
// on screen by the width, a share of the canvas's height, applied in clip
// space up to the distance at which a metre fills the height, and fading
// out as the normal turns to face the camera.
fn material_outline_vertex(vertex: Vertex) -> VertexOffset {
    let n_view = safe_normalize((frame.view * vec4f(vertex.normal, 0.0)).xyz);
    let n_clip = (frame.projection * vec4f(n_view, 0.0)).xy;
    // A normal facing the camera has no direction on screen.
    if dot(n_clip, n_clip) <= 0.0 {
        return VertexOffset(vec3f(0.0), vec2f(0.0));
    }
    var offset = normalize(n_clip) * 2.0 * params.shape.y
        * min(vertex.clip.w, frame.projection[1][1]);
    // Clip space x spans the canvas's width, not its height.
    offset.x *= frame.height_over_width;
    let facing = saturate(1.0 - n_view.z * n_view.z);
    return VertexOffset(vec3f(0.0), offset * facing);
}

// The outline's color, mixed toward the surface's. The outline pass culls
// front faces, so every fragment is on a back face; its normal is the
// outward one all the same, so it is shaded as the surface beside it.
fn material_outline_fragment(fragment: Fragment) -> vec4f {
    let surface = toon_color(fragment.normal, fragment.view, checker(fragment.uv));
    return vec4f(params.outline.rgb * mix(vec3f(1.0), surface, params.outline.w), 1.0);
}
