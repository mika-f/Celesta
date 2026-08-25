@group(0) @binding(0)
var source: texture_2d<f32>;

@group(0) @binding(1)
var source_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let position = positions[index];
    return VertexOutput(
        vec4<f32>(position, 0.0, 1.0),
        position * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5),
    );
}

fn rgb(uv: vec2<f32>) -> vec3<f32> {
    return textureSample(source, source_sampler, uv).rgb;
}

@fragment
fn luma(input: VertexOutput) -> @location(0) f32 {
    return dot(rgb(input.uv), vec3<f32>(0.299, 0.587, 0.114));
}

@fragment
fn chroma(input: VertexOutput) -> @location(0) vec2<f32> {
    let color = rgb(input.uv);
    return vec2<f32>(
        dot(color, vec3<f32>(-0.168736, -0.331264, 0.5)) + 0.5,
        dot(color, vec3<f32>(0.5, -0.418688, -0.081312)) + 0.5,
    );
}
