// Multiplies a masked group's canvas by its mask, texel for texel. Both
// canvases hold premultiplied alpha and cover the same part of the scene.

@group(0) @binding(0)
var children: texture_2d<f32>;

@group(1) @binding(0)
var matte: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
    let vertices = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return VertexOutput(vec4<f32>(vertices[index], 0.0, 1.0));
}

// Rec. 709 luma of sRGB-encoded values. On a premultiplied color it already
// includes the alpha, as `celesta_renderer`'s `mask_value` multiplies it in.
const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);

fn masked(position: vec4<f32>, luminance: bool, invert: bool) -> vec4<f32> {
    let texel = vec2<i32>(floor(position.xy));
    let color = textureLoad(children, texel, 0);
    let mask = textureLoad(matte, texel, 0);
    var shown = mask.a;
    if luminance {
        shown = dot(mask.rgb, LUMA);
    }
    if invert {
        shown = 1.0 - shown;
    }
    return color * clamp(shown, 0.0, 1.0);
}

@fragment
fn alpha(input: VertexOutput) -> @location(0) vec4<f32> {
    return masked(input.position, false, false);
}

@fragment
fn alpha_inverted(input: VertexOutput) -> @location(0) vec4<f32> {
    return masked(input.position, false, true);
}

@fragment
fn luminance(input: VertexOutput) -> @location(0) vec4<f32> {
    return masked(input.position, true, false);
}

@fragment
fn luminance_inverted(input: VertexOutput) -> @location(0) vec4<f32> {
    return masked(input.position, true, true);
}
