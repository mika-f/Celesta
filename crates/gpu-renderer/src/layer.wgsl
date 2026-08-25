struct LayerUniform {
    matrix: vec4<f32>,
    translation_size: vec4<f32>,
    anchor_opacity: vec4<f32>,
    canvas: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> layer: LayerUniform;

@group(0) @binding(1)
var source_texture: texture_2d<f32>;

@group(0) @binding(2)
var source_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let coordinates = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let uv = coordinates[vertex_index];
    let size = layer.translation_size.zw;
    let anchor = layer.anchor_opacity.xy;
    let local = (uv - anchor) * size;
    let world = vec2<f32>(
        layer.matrix.x * local.x + layer.matrix.z * local.y + layer.translation_size.x,
        layer.matrix.y * local.x + layer.matrix.w * local.y + layer.translation_size.y,
    );
    let clip = vec2<f32>(
        world.x / layer.canvas.x * 2.0 - 1.0,
        1.0 - world.y / layer.canvas.y * 2.0,
    );
    return VertexOutput(vec4<f32>(clip, 0.0, 1.0), uv);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(source_texture, source_sampler, input.uv);
    return vec4<f32>(color.rgb, color.a * layer.anchor_opacity.z);
}
