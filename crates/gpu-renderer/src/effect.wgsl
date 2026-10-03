// Separable Gaussian blur of a premultiplied scene-sized layer. The second
// pass can shift and tint the blurred alpha for a shadow or glow.
struct Params {
    direction_radius_mode: vec4<f32>,
    offset: vec4<f32>,
    color: vec4<f32>,
};

// Bound like `layer.wgsl`'s layer texture, so a canvas's bind group serves both.
@group(0) @binding(0)
var source: texture_2d<f32>;

// One pass's parameters, at a dynamic offset into the frame's buffer.
@group(1) @binding(0)
var<uniform> params: Params;

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

fn load_pixel(point: vec2<i32>, size: vec2<i32>) -> vec4<f32> {
    if any(point < vec2<i32>(0)) || any(point >= size) {
        return vec4<f32>(0.0);
    }
    return textureLoad(source, point, 0);
}

fn sample_pixel(point: vec2<f32>, size: vec2<i32>) -> vec4<f32> {
    let base = vec2<i32>(floor(point));
    let fraction = fract(point);
    if all(fraction == vec2<f32>(0.0)) {
        return load_pixel(base, size);
    }
    let top = mix(load_pixel(base, size), load_pixel(base + vec2<i32>(1, 0), size), fraction.x);
    let bottom = mix(load_pixel(base + vec2<i32>(0, 1), size),
        load_pixel(base + vec2<i32>(1, 1), size), fraction.x);
    return mix(top, bottom, fraction.y);
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let center = input.position.xy - vec2<f32>(0.5) - params.offset.xy;
    let direction = params.direction_radius_mode.xy;
    let sigma = clamp(params.direction_radius_mode.z, 0.0, 64.0);
    var extent = i32(ceil(sigma * 3.0));
    if sigma == 0.0 {
        extent = 0;
    }
    var sum = vec4<f32>(0.0);
    var weight_sum = 0.0;
    for (var i = -extent; i <= extent; i++) {
        var weight = 1.0;
        if sigma > 0.0 {
            weight = exp(-f32(i * i) / (2.0 * sigma * sigma));
        }
        let point = center + direction * f32(i);
        sum += sample_pixel(point, size) * weight;
        weight_sum += weight;
    }
    let blurred = sum / weight_sum;
    if params.direction_radius_mode.w > 0.5 {
        let alpha = blurred.a * params.color.a;
        return vec4<f32>(params.color.rgb * alpha, alpha);
    }
    return blurred;
}
