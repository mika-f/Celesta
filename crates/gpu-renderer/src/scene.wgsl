// Takes a plugin's resolved scene canvas (see `scene.rs`) of linear,
// premultiplied color to the layer's encoding: un-premultiplied, encoded
// linear to sRGB, and premultiplied again, as 8-bit values. The scene's
// materials draw onto that canvas through `scene_material.rs`'s modules.

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
