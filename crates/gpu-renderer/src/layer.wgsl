// One instance per layer, drawn in painter's order. Consecutive layers that
// sample the same texture (every rect shares a placeholder) are one draw.
struct LayerInstance {
    @location(0) matrix: vec4<f32>,
    @location(1) translation_size: vec4<f32>,
    // anchor.xy, opacity, and the content kind: 0 samples `source_texture`,
    // 1 shades the rect described by `rect`/`fill`/`stroke`.
    @location(2) anchor_opacity_kind: vec4<f32>,
    // canvas width, height; the blend mode index (see `blend`), and 1 when
    // `source_texture` holds premultiplied alpha (an isolated group's canvas)
    @location(3) canvas: vec4<f32>,
    // half width, half height, corner radius, stroke width (0 without one)
    @location(4) rect: vec4<f32>,
    // Straight-alpha RGBA in 0-255 code values, or a gradient in `paints`
    // (see `paint_color`).
    @location(5) fill: vec4<f32>,
    @location(6) stroke: vec4<f32>,
    // The index of the innermost clip the layer is drawn through (-1 without
    // one); 1 to filter (the layer is scaled or rotated) or 0 to copy texel
    // for texel; the mip level to filter at; texels per layer unit.
    @location(7) clip_sampling: vec4<f32>,
    // The scene position of the canvas's top-left pixel (a group's canvas
    // only covers the part of the scene its layers draw on); unused, unused.
    @location(8) origin: vec4<f32>,
};

// Read with `textureLoad` only: `layer_color` filters by hand.
@group(0) @binding(0)
var source_texture: texture_2d<f32>;

// A copy of the canvas `fs_blend` draws onto, in premultiplied alpha.
@group(1) @binding(0)
var backdrop_texture: texture_2d<f32>;

// Every clipped group of the frame, three vec4s each:
//   inverse matrix (a, b, c, d)
//   translation to the clip rectangle's frame, half width, half height
//   corner radius, local-to-canvas distance scale, parent clip (-1 for none), unused
// Mirrors `ClipEntry` in lib.rs.
@group(2) @binding(0)
var<storage, read> clips: array<vec4<f32>>;

// The gradients rects of the frame are painted with. Each starts with
//   kind (1 linear, 2 radial), stop count, unused, unused
//   linear: start.xy, end.xy; radial: center.xy, radius, unused
// then two vec4s per stop: (offset, unused...), premultiplied RGBA in 0-255.
// Mirrors `encode_paint` in lib.rs.
@group(2) @binding(1)
var<storage, read> paints: array<vec4<f32>>;

// Mirrors `MAX_CLIP_DEPTH` in lib.rs.
const MAX_CLIP_DEPTH: i32 = 8;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // size.xy, opacity, kind
    @location(1) @interpolate(flat) size_opacity_kind: vec4<f32>,
    @location(2) @interpolate(flat) rect: vec4<f32>,
    @location(3) @interpolate(flat) fill: vec4<f32>,
    @location(4) @interpolate(flat) stroke: vec4<f32>,
    // blend mode index, premultiplied source
    @location(5) @interpolate(flat) blend: vec2<f32>,
    // The scene position of the fragment and the innermost clip it is drawn through.
    @location(6) world: vec2<f32>,
    @location(7) @interpolate(flat) clip: f32,
    // filter, mip level, texels per layer unit (see `LayerInstance`)
    @location(8) @interpolate(flat) sampling: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, layer: LayerInstance) -> VertexOutput {
    let coordinates = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let size = layer.translation_size.zw;
    let sampling = layer.clip_sampling.yzw;
    // A filtered layer's edge texels blend with the transparent texels
    // around it, so its quad grows by one texel on every side to draw that
    // anti-aliased edge.
    let texels = size * sampling.z;
    let margin = select(vec2<f32>(0.0), 1.0 / texels, sampling.x == 1.0);
    let uv = coordinates[vertex_index] * (1.0 + 2.0 * margin) - margin;
    let anchor = layer.anchor_opacity_kind.xy;
    let local = (uv - anchor) * size;
    let world = vec2<f32>(
        layer.matrix.x * local.x + layer.matrix.z * local.y + layer.translation_size.x,
        layer.matrix.y * local.x + layer.matrix.w * local.y + layer.translation_size.y,
    );
    let canvas_position = world - layer.origin.xy;
    let clip = vec2<f32>(
        canvas_position.x / layer.canvas.x * 2.0 - 1.0,
        1.0 - canvas_position.y / layer.canvas.y * 2.0,
    );
    return VertexOutput(
        vec4<f32>(clip, 0.0, 1.0),
        uv,
        vec4<f32>(size, layer.anchor_opacity_kind.zw),
        layer.rect,
        layer.fill,
        layer.stroke,
        layer.canvas.zw,
        world,
        layer.clip_sampling.x,
        sampling,
    );
}

// Inigo Quilez's rounded-box signed distance, as in `celesta-renderer`.
fn rounded_box(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

// `celesta_renderer::sample_stops`: the stops from `paints[first]` at `t`,
// interpolated premultiplied, as straight-alpha RGBA in whole 0-255 codes.
fn sample_stops(first: u32, count: u32, t: f32) -> vec4<f32> {
    if count == 0u {
        return vec4<f32>(0.0);
    }
    let last = first + 2u * (count - 1u);
    var value = paints[first + 1u];
    if t <= paints[first].x {
        // `value` is the first stop's color.
    } else if t >= paints[last].x {
        value = paints[last + 1u];
    } else {
        // The first stop past `t`; the one before it is at or below `t`.
        var next = first + 2u;
        while paints[next].x <= t {
            next += 2u;
        }
        let from_offset = paints[next - 2u].x;
        let span = paints[next].x - from_offset;
        var f = 1.0;
        if span > 0.0 {
            f = (t - from_offset) / span;
        }
        let from_color = paints[next - 1u];
        value = from_color + (paints[next + 1u] - from_color) * f;
    }
    let alpha = value.a;
    if alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    // `floor(x + 0.5)` rounds halves up like Rust's `f64::round` here.
    return vec4<f32>(
        clamp(floor(value.rgb * 255.0 / alpha + 0.5), vec3<f32>(0.0), vec3<f32>(255.0)),
        clamp(floor(alpha + 0.5), 0.0, 255.0),
    );
}

// `ResolvedPaint::color_at`: a rect's fill or stroke at `point` in its local
// pixels, straight-alpha RGBA in 0-255 code values.
fn paint_color(paint: vec4<f32>, point: vec2<f32>) -> vec4<f32> {
    if paint.w >= 0.0 {
        return paint;
    }
    let base = u32(paint.x);
    let header = paints[base];
    let geometry = paints[base + 1u];
    var t = 0.0;
    if header.x == 1.0 {
        let direction = geometry.zw - geometry.xy;
        let length_squared = dot(direction, direction);
        if length_squared > 0.0 {
            t = dot(point - geometry.xy, direction) / length_squared;
        }
    } else if geometry.z > 0.0 {
        t = distance(point, geometry.xy) / geometry.z;
    }
    return sample_stops(base + 2u, u32(header.y), t);
}

// The texel `celesta_renderer::rasterize_rect` would have produced at
// `coordinate`, straight alpha, so drawing a rect here matches uploading its
// rasterized texture and reading it the same way.
fn rect_texel(input: VertexOutput, coordinate: vec2<i32>) -> vec4<f32> {
    let texel = vec2<f32>(coordinate) + vec2<f32>(0.5);
    let half_size = input.rect.xy;
    let radius = input.rect.z;
    let stroke_width = input.rect.w;
    let p = texel - half_size;
    let outer = clamp(0.5 - rounded_box(p, half_size, radius), 0.0, 1.0);
    var color = paint_color(input.fill, texel);
    if stroke_width > 0.0 {
        let stroke = paint_color(input.stroke, texel);
        let inner = clamp(
            0.5 - rounded_box(
                p,
                max(half_size - vec2<f32>(stroke_width), vec2<f32>(0.0)),
                max(radius - stroke_width, 0.0),
            ),
            0.0,
            1.0,
        );
        // `floor(x + 0.5)` rounds like Rust's `f64::round` for these
        // non-negative values; WGSL's `round` rounds halves to even, and
        // lerped channels often land exactly on a half.
        color = floor(stroke + (color - stroke) * inner + 0.5);
    }
    return vec4<f32>(color.rgb, floor(color.a * outer + 0.5)) / 255.0;
}

// How much of the pixel at canvas position `world` is inside every clip in
// the chain starting at `index`, anti-aliased over the edge like `rect_texel`.
fn clip_coverage(world: vec2<f32>, index: f32) -> f32 {
    var coverage = 1.0;
    var current = index;
    for (var depth = 0; depth < MAX_CLIP_DEPTH; depth++) {
        if current < 0.0 {
            break;
        }
        let base = u32(current) * 3u;
        let matrix = clips[base];
        let placement = clips[base + 1u];
        let shape = clips[base + 2u];
        let local = vec2<f32>(
            matrix.x * world.x + matrix.z * world.y + placement.x,
            matrix.y * world.x + matrix.w * world.y + placement.y,
        );
        let distance = rounded_box(local, placement.zw, shape.x);
        coverage = coverage * clamp(0.5 - distance * shape.y, 0.0, 1.0);
        current = shape.z;
    }
    return coverage;
}

// The size in texels of the layer's content at mip `level`.
fn texel_size(input: VertexOutput, level: i32) -> vec2<i32> {
    if input.size_opacity_kind.w == 1.0 {
        // Rects are shaded at their rasterized size and have no mips.
        return vec2<i32>(round(input.size_opacity_kind.xy));
    }
    return vec2<i32>(textureDimensions(source_texture, level));
}

// The straight-alpha texel at `coordinate`, which must lie inside the layer.
fn texel(input: VertexOutput, coordinate: vec2<i32>, level: i32) -> vec4<f32> {
    if input.size_opacity_kind.w == 1.0 {
        return rect_texel(input, coordinate);
    }
    let color = textureLoad(source_texture, coordinate, level);
    if input.blend.y == 1.0 && color.a > 0.0 {
        return vec4<f32>(color.rgb / color.a, color.a);
    }
    return color;
}

// The premultiplied texel at `coordinate`, transparent outside the layer.
fn premultiplied_texel(
    input: VertexOutput,
    coordinate: vec2<i32>,
    size: vec2<i32>,
    level: i32,
) -> vec4<f32> {
    if any(coordinate < vec2<i32>(0)) || any(coordinate >= size) {
        return vec4<f32>(0.0);
    }
    let color = texel(input, coordinate, level);
    return vec4<f32>(color.rgb * color.a, color.a);
}

// Bilinear interpolation of mip `level` at `uv`, premultiplied.
fn bilinear(input: VertexOutput, uv: vec2<f32>, level: i32) -> vec4<f32> {
    let size = texel_size(input, level);
    let position = uv * vec2<f32>(size) - vec2<f32>(0.5);
    let corner = floor(position);
    let weight = position - corner;
    let first = vec2<i32>(corner);
    let top = mix(
        premultiplied_texel(input, first, size, level),
        premultiplied_texel(input, first + vec2<i32>(1, 0), size, level),
        weight.x,
    );
    let bottom = mix(
        premultiplied_texel(input, first + vec2<i32>(0, 1), size, level),
        premultiplied_texel(input, first + vec2<i32>(1, 1), size, level),
        weight.x,
    );
    return mix(top, bottom, weight.y);
}

// The layer's non-premultiplied color under `input.uv`.
fn layer_color(input: VertexOutput) -> vec4<f32> {
    if input.sampling.x == 0.0 {
        // Texels land one to one on pixels: copy the one under the pixel.
        let size = texel_size(input, 0);
        let coordinate = min(vec2<i32>(floor(input.uv * vec2<f32>(size))), size - vec2<i32>(1));
        return texel(input, coordinate, 0);
    }
    // Trilinear: blend the two mip levels around the level of detail.
    var levels = 1;
    if input.size_opacity_kind.w != 1.0 {
        levels = i32(textureNumLevels(source_texture));
    }
    let level = clamp(input.sampling.y, 0.0, f32(levels - 1));
    let lower = i32(floor(level));
    var color = bilinear(input, input.uv, lower);
    let between = level - f32(lower);
    if between > 0.0 {
        color = mix(color, bilinear(input, input.uv, lower + 1), between);
    }
    if color.a <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(color.rgb / color.a, color.a);
}

// The layer's non-premultiplied color, with its opacity applied to alpha.
fn source_color(input: VertexOutput) -> vec4<f32> {
    let color = layer_color(input);
    let coverage = clip_coverage(input.world, input.clip);
    return vec4<f32>(color.rgb, color.a * input.size_opacity_kind.z * coverage);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return source_color(input);
}

// `celesta_composition::BlendMode::blend_channel` for all three channels.
fn blend(mode: u32, backdrop: vec3<f32>, source: vec3<f32>) -> vec3<f32> {
    switch mode {
        case 1u: {
            return source * backdrop;
        }
        case 2u: {
            return backdrop + source - backdrop * source;
        }
        case 3u: {
            let doubled = 2.0 * backdrop;
            let screened = source + (doubled - 1.0) - source * (doubled - 1.0);
            return select(screened, source * doubled, backdrop <= vec3<f32>(0.5));
        }
        case 4u: {
            return min(source + backdrop, vec3<f32>(1.0));
        }
        case 5u: {
            return abs(backdrop - source);
        }
        default: {
            return source;
        }
    }
}

// Blends the layer with the backdrop under this fragment and replaces the
// canvas pixel with the result, as `celesta_renderer`'s `blend_with_mode`
// does, keeping the canvas premultiplied.
@fragment
fn fs_blend(input: VertexOutput) -> @location(0) vec4<f32> {
    let source = source_color(input);
    let backdrop = textureLoad(backdrop_texture, vec2<i32>(floor(input.position.xy)), 0);
    var backdrop_color = vec3<f32>(0.0);
    if backdrop.a > 0.0 {
        backdrop_color = backdrop.rgb / backdrop.a;
    }
    let mode = u32(input.blend.x + 0.5);
    let mixed = (1.0 - backdrop.a) * source.rgb
        + backdrop.a * blend(mode, backdrop_color, source.rgb);
    return vec4<f32>(
        source.a * mixed + backdrop.rgb * (1.0 - source.a),
        source.a + backdrop.a * (1.0 - source.a),
    );
}
