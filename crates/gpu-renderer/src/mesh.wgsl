// Draws a plugin's meshes onto a transparent canvas the size of its own,
// premultiplied (see `mesh.rs`). Each mesh samples a premultiplied texture,
// multiplies its color, screens another over it, and scales by its opacity.
// A mesh can be clipped to where other meshes' textures cover: those are
// first drawn onto a mask canvas cleared to 1, each multiplying it by
// `1 - alpha`, so `1 - mask` is their union's coverage.

struct Mesh {
    multiply: vec4<f32>,
    screen: vec4<f32>,
    // opacity, mask mode (0 none, 1 inside the mask, 2 outside it), unused, unused
    opacity_mask: vec4<f32>,
};

@group(0) @binding(0)
var mesh_texture: texture_2d<f32>;
@group(0) @binding(1)
var mesh_sampler: sampler;

// Read with `textureLoad` at the fragment's own pixel.
@group(1) @binding(0)
var mask_texture: texture_2d<f32>;

@group(2) @binding(0)
var<storage, read> meshes: array<Mesh>;

struct VertexInput {
    // Clip space, y up.
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mesh: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) mesh: u32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    // Plugins count `v` up from the texture's bottom row.
    output.uv = vec2<f32>(input.uv.x, 1.0 - input.uv.y);
    output.mesh = input.mesh;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let mesh = meshes[input.mesh];
    var color = textureSample(mesh_texture, mesh_sampler, input.uv);
    let multiplied = color.rgb * mesh.multiply.rgb;
    // Screen on premultiplied color: c + s·a - c·s.
    color = vec4<f32>(
        multiplied + mesh.screen.rgb * color.a - multiplied * mesh.screen.rgb,
        color.a,
    );
    color *= mesh.opacity_mask.x;
    let mode = mesh.opacity_mask.y;
    if mode > 0.5 {
        let coverage = 1.0 - textureLoad(mask_texture, vec2<i32>(floor(input.position.xy)), 0).r;
        color *= select(coverage, 1.0 - coverage, mode > 1.5);
    }
    return color;
}

// A masking mesh's coverage: its texture's alpha alone, without its opacity
// or colors. Blended onto the mask with `dst * (1 - src)`.
@fragment
fn fs_mask(input: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(mesh_texture, mesh_sampler, input.uv).a;
    return vec4<f32>(alpha);
}
