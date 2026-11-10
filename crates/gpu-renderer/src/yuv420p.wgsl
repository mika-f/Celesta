// Converts a rendered frame, composited in `canvas_format`, into the three
// planes of yuv420p: a full-size Y plane (`luma`) and quarter-size U and V
// planes (`chroma`, one render target each), ready to hand to an H.264
// encoder without a CPU-side color conversion. `luma10` and `chroma10` write
// yuv420p10le's 10-bit codes instead, into 16-bit integer planes.
// `chroma_nv12` writes NV12's interleaved UV plane (one two-channel target),
// and `luma_p010` and `chroma_p010` write P010's planes: the 10-bit codes
// shifted left by 6 in 16-bit integers.
//
// The matrix is BT.709 with limited ("TV") range: the matrix players assume
// for HD video and the one every Celesta export is tagged with. Its 10-bit
// form is the 8-bit one scaled by 4. Chroma is the average of each 2x2 block,
// sited at the block's center.
//
// Each plane is rounded with `dither` (`dither.wgsl`, prepended) from the
// frame's own precision. Rounded alone, a dark gradient's luma would step
// only where its channels' steps add up to a whole code, in bands several
// times wider than an RGBA export's.

@group(0) @binding(0)
var frame: texture_2d<f32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

// Clamped to 0-1 as the layer pipelines' output is: a composited frame is
// converted straight from the root canvas.
fn rgb(pixel: vec2<u32>) -> vec3<f32> {
    return clamp(textureLoad(frame, pixel, 0).rgb, vec3<f32>(0.0), vec3<f32>(1.0));
}

// The pixel's BT.709 luma in 8-bit code values, unrounded.
fn luma_value(pixel: vec2<u32>) -> f32 {
    return 16.0 + dot(rgb(pixel), vec3<f32>(46.559, 156.629, 15.812));
}

// The U and V of the 2x2 block at `block` in 8-bit code values, unrounded.
fn chroma_values(block: vec2<u32>) -> vec2<f32> {
    let pixel = block * 2u;
    let color = (rgb(pixel) + rgb(pixel + vec2<u32>(1u, 0u)) + rgb(pixel + vec2<u32>(0u, 1u))
        + rgb(pixel + vec2<u32>(1u, 1u))) * 0.25;
    return vec2<f32>(
        128.0 + dot(color, vec3<f32>(-25.664, -86.336, 112.0)),
        128.0 + dot(color, vec3<f32>(112.0, -101.730, -10.270)),
    );
}

// `value`, in 8-bit code values, as a 10-bit code rounded after adding
// `offset` (in 10-bit codes).
fn code10(value: f32, offset: f32) -> u32 {
    return u32(clamp(floor(value * 4.0 + 0.5 + offset), 0.0, 1023.0));
}

@fragment
fn luma(@builtin(position) position: vec4<f32>) -> @location(0) f32 {
    let pixel = vec2<u32>(position.xy);
    return code(luma_value(pixel), dither(pixel));
}

struct Chroma {
    @location(0) u: f32,
    @location(1) v: f32,
};

@fragment
fn chroma(@builtin(position) position: vec4<f32>) -> Chroma {
    let block = vec2<u32>(position.xy);
    let values = chroma_values(block);
    let offset = dither(block);
    return Chroma(code(values.x, offset), code(values.y, offset));
}

@fragment
fn luma10(@builtin(position) position: vec4<f32>) -> @location(0) u32 {
    let pixel = vec2<u32>(position.xy);
    return code10(luma_value(pixel), dither(pixel));
}

struct Chroma10 {
    @location(0) u: u32,
    @location(1) v: u32,
};

@fragment
fn chroma10(@builtin(position) position: vec4<f32>) -> Chroma10 {
    let block = vec2<u32>(position.xy);
    let values = chroma_values(block);
    let offset = dither(block);
    return Chroma10(code10(values.x, offset), code10(values.y, offset));
}

@fragment
fn chroma_nv12(@builtin(position) position: vec4<f32>) -> @location(0) vec2<f32> {
    let block = vec2<u32>(position.xy);
    let values = chroma_values(block);
    let offset = dither(block);
    return vec2<f32>(code(values.x, offset), code(values.y, offset));
}

@fragment
fn luma_p010(@builtin(position) position: vec4<f32>) -> @location(0) u32 {
    let pixel = vec2<u32>(position.xy);
    return code10(luma_value(pixel), dither(pixel)) << 6u;
}

@fragment
fn chroma_p010(@builtin(position) position: vec4<f32>) -> @location(0) vec2<u32> {
    let block = vec2<u32>(position.xy);
    let values = chroma_values(block);
    let offset = dither(block);
    return vec2<u32>(code10(values.x, offset), code10(values.y, offset)) << vec2<u32>(6u);
}
