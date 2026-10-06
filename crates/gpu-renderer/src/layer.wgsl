// One instance per layer, drawn in painter's order. Consecutive layers that
// sample the same texture (every rect shares a placeholder) are one draw.
struct LayerInstance {
    @location(0) matrix: vec4<f32>,
    @location(1) translation_size: vec4<f32>,
    // anchor.xy, opacity, and the content kind: 0 samples `source_texture`,
    // 1 shades the rect described by `rect`/`fill`/`stroke`, 2 shades the
    // path whose first entry in `paths` is `rect.x`, painted with
    // `fill`/`stroke`.
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

// The edges of every path of the frame. Each path starts with
//   inverse matrix (a, b, c, d) from its region's pixels to its own coordinates
//   inverse translation, tile columns, entries per tile drawn
//   which entry of a tile is the fill's, the stroke's (-1 without one), the
//     index of the first tile's, unused
// then has, for each `PATH_TILE_COLUMNS`x`PATH_TILE_ROWS` tile drawn, an entry
// per outline
//   the tile's index in its region (row by row), index of the first of the
//   outline's edges there, edge count, backdrop winding
// and its edges, (x0, y0, x1, y1) in its region's pixels. Mirrors
// `ShadedPath` and `bin_tiles` in lib.rs.
@group(2) @binding(2)
var<storage, read> paths: array<vec4<f32>>;

// Mirrors `PATH_TILE_COLUMNS` and `PATH_TILE_ROWS` in lib.rs.
const PATH_TILE_COLUMNS: i32 = 8;
const PATH_TILE_ROWS: i32 = 8;

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
    // A path's tile: the index of its first entry in `paths`.
    @location(9) @interpolate(flat) path_tile: u32,
    // A rect's inverse map, from scene pixels to its own units from its
    // top-left corner: the linear part (du/dx, dv/dx, du/dy, dv/dy), then
    // the translation and the most of a pixel the rect and the box inside
    // its stroke can cover (see `rect_coverage_cap`).
    @location(10) @interpolate(flat) inverse: vec4<f32>,
    @location(11) @interpolate(flat) inverse_translation_caps: vec4<f32>,
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
    let corner = coordinates[vertex_index % 6u];
    var uv: vec2<f32>;
    var path_tile = 0u;
    var inverse = vec4<f32>(0.0);
    var inverse_translation_caps = vec4<f32>(0.0);
    let anchor = layer.anchor_opacity_kind.xy;
    var local: vec2<f32>;
    if layer.anchor_opacity_kind.w == 1.0 {
        // A rect is shaded wherever it lands, so its quad reaches one pixel
        // past every edge for the anti-aliasing there.
        let m = layer.matrix;
        inverse = vec4<f32>(m.w, -m.y, -m.z, m.x) / (m.x * m.w - m.y * m.z);
        let margin = vec2<f32>(length(inverse.xz), length(inverse.yw));
        local = (corner - anchor) * size + (corner * 2.0 - 1.0) * margin;
        let offset = -layer.translation_size.xy;
        let translation = vec2<f32>(
            inverse.x * offset.x + inverse.z * offset.y,
            inverse.y * offset.x + inverse.w * offset.y,
        ) + anchor * size;
        // Constant across the rect, so found once here.
        let half_size = layer.rect.xy;
        let inner_half_size = max(half_size - vec2<f32>(layer.rect.w), vec2<f32>(0.0));
        inverse_translation_caps = vec4<f32>(
            translation,
            rect_coverage_cap(half_size, inverse),
            rect_coverage_cap(inner_half_size, inverse),
        );
    } else if layer.anchor_opacity_kind.w == 2.0 {
        // A path draws a quad over each tile it lists, skipping the tiles
        // nothing covers. It is never filtered.
        let base = u32(layer.rect.x);
        let header = paths[base + 1u];
        path_tile = u32(paths[base + 2u].z) + vertex_index / 6u * u32(header.w);
        let tile = u32(paths[path_tile].x);
        let columns = u32(header.z);
        let extent = vec2<f32>(f32(PATH_TILE_COLUMNS), f32(PATH_TILE_ROWS));
        let origin = vec2<f32>(f32(tile % columns), f32(tile / columns)) * extent;
        uv = (origin + corner * (min(origin + extent, size) - origin)) / size;
    } else {
        // A filtered layer's edge texels blend with the transparent texels
        // around it, so its quad grows by one texel on every side to draw
        // that anti-aliased edge.
        let texels = size * sampling.z;
        let margin = select(vec2<f32>(0.0), 1.0 / texels, sampling.x == 1.0);
        uv = corner * (1.0 + 2.0 * margin) - margin;
    }
    if layer.anchor_opacity_kind.w != 1.0 {
        local = (uv - anchor) * size;
    }
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
        path_tile,
        inverse,
        inverse_translation_caps,
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

// `celesta_renderer`'s `RectDistance`: the rounded-box distance for a box
// drawn through the map whose inverse is `inverse`, in units of a pixel's
// width across the edge (`|n.x| + |n.y|` for its normal `n`), so
// `0.5 - distance` ramps across an edge as a pixel-sized box filter does and
// a thin strip draws the same wherever its edges fall: the local distance
// over the L1 length of its gradient in canvas pixels.
fn rect_distance(p: vec2<f32>, half_size: vec2<f32>, radius: f32, inverse: vec4<f32>) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(radius);
    if q.x > 0.0 && q.y > 0.0 {
        let length_q = length(q);
        // Signed by the quadrant: a shear stretches opposite corners apart.
        let direction = select(q, -q, p < vec2<f32>(0.0)) / length_q;
        let gradient = vec2<f32>(
            direction.x * inverse.x + direction.y * inverse.y,
            direction.x * inverse.z + direction.y * inverse.w,
        );
        return (length_q - radius) / (abs(gradient.x) + abs(gradient.y));
    }
    let footprint = rect_footprint(inverse);
    return max((q.x - radius) / footprint.x, (q.y - radius) / footprint.y);
}

// The local units across a pixel's width across a vertical and a horizontal
// edge: the L1 lengths of the local x and y gradients.
fn rect_footprint(inverse: vec4<f32>) -> vec2<f32> {
    return vec2<f32>(abs(inverse.x) + abs(inverse.z), abs(inverse.y) + abs(inverse.w));
}

// `RectDistance::coverage_cap`: the most of a pixel a box of `half_size` can
// cover, so a box thinner than a pixel (or empty) is not drawn heavier than
// it is. A box filter gives a pixel inside the ramps of a pair of parallel
// edges their distance apart in `rect_distance`'s units, each at most 1;
// inside both pairs', the product.
fn rect_coverage_cap(half_size: vec2<f32>, inverse: vec4<f32>) -> f32 {
    let fractions = clamp(2.0 * half_size / rect_footprint(inverse), vec2<f32>(0.0), vec2<f32>(1.0));
    return fractions.x * fractions.y;
}

// The rect under the pixel, straight alpha, as
// `celesta_renderer::rasterize_rect_transformed` shades it at the pixel's
// centre.
fn rect_color(input: VertexOutput) -> vec4<f32> {
    // From the scene position rather than `@builtin(position)`, which is in
    // the target's pixels: a preview draws the scene through a fitted
    // viewport.
    let pixel = input.world;
    let point = vec2<f32>(
        input.inverse.x * pixel.x + input.inverse.z * pixel.y,
        input.inverse.y * pixel.x + input.inverse.w * pixel.y,
    ) + input.inverse_translation_caps.xy;
    let half_size = input.rect.xy;
    let radius = input.rect.z;
    let stroke_width = input.rect.w;
    let p = point - half_size;
    let outer = clamp(
        0.5 - rect_distance(p, half_size, radius, input.inverse),
        0.0,
        input.inverse_translation_caps.z,
    );
    var color = paint_color(input.fill, point);
    if stroke_width > 0.0 {
        let stroke = paint_color(input.stroke, point);
        let inner = clamp(
            0.5 - rect_distance(
                p,
                max(half_size - vec2<f32>(stroke_width), vec2<f32>(0.0)),
                max(radius - stroke_width, 0.0),
                input.inverse,
            ),
            0.0,
            input.inverse_translation_caps.w,
        );
        // Of the part of the pixel the rect covers, the fill's share; the
        // rest is stroke.
        var fill_share = 0.0;
        if outer > 0.0 {
            fill_share = min(inner / outer, 1.0);
        }
        // `floor(x + 0.5)` rounds like Rust's `f64::round` for these
        // non-negative values; WGSL's `round` rounds halves to even, and
        // lerped channels often land exactly on a half.
        color = floor(stroke + (color - stroke) * fill_share + 0.5);
    }
    return vec4<f32>(color.rgb, floor(color.a * outer + 0.5)) / 255.0;
}

// Scanlines sampled per pixel row, as `tiny_skia`'s anti-aliasing does.
const PATH_SUBSCANLINES: i32 = 4;

// The length of [`left`, `left` + 1) that the nonzero fill of the edges
// `paths[first..first + count]` covers on the scanline at `y`, starting from
// `backdrop`, the winding left of the tile: crossing by crossing, nearest
// first. The first pass also sums the edges left of the pixel; each further
// pass finds the next crossing inside it, so most pixels take one pass and
// nothing needs storage: no local array, which DX12's shader compilers
// handle poorly in loops. Edges crossing at the same x count together, so
// coincident opposite contours cancel.
fn scanline_coverage(first: u32, count: u32, y: f32, left: f32, backdrop: i32) -> f32 {
    let right = left + 1.0;
    var winding = backdrop;
    var at = left;
    var covered = 0.0;
    var first_pass = true;
    loop {
        var next = right;
        var change = 0;
        for (var index = first; index < first + count; index++) {
            let edge = paths[index];
            // Half open, so an edge's shared end counts once.
            if (y < edge.y) == (y < edge.w) {
                continue;
            }
            let x = edge.x + (y - edge.y) * (edge.z - edge.x) / (edge.w - edge.y);
            let direction = select(-1, 1, edge.w > edge.y);
            if first_pass && x <= left {
                winding += direction;
                continue;
            }
            if x <= at || x >= right {
                continue;
            }
            if x < next {
                next = x;
                change = direction;
            } else if x == next {
                change += direction;
            }
        }
        first_pass = false;
        if winding != 0 {
            covered += next - at;
        }
        if next >= right {
            break;
        }
        winding += change;
        at = next;
    }
    return covered;
}

// How much of the pixel whose top-left corner is `pixel` the nonzero fill of
// the outline whose entry for the tile there is `tile` covers: the length of
// `PATH_SUBSCANLINES` horizontal scanlines through it, each measured
// exactly, averaged. `tiny_skia` samples the same scanlines, but rounds
// where they cross edges to quarter pixels.
fn nonzero_coverage(tile: vec4<f32>, pixel: vec2<f32>) -> f32 {
    let first = u32(tile.y);
    let count = u32(tile.z);
    let backdrop = i32(tile.w);
    if count == 0u {
        return select(0.0, 1.0, backdrop != 0);
    }
    var coverage = 0.0;
    for (var line = 0; line < PATH_SUBSCANLINES; line++) {
        let y = pixel.y + (f32(line) + 0.5) / f32(PATH_SUBSCANLINES);
        coverage += scanline_coverage(first, count, y, pixel.x, backdrop);
    }
    return coverage / f32(PATH_SUBSCANLINES);
}

// The texel `celesta_renderer::rasterize_path` would have produced at
// `coordinate` of the path's region, straight alpha: the stroke over the
// fill, each covering what the nonzero fill of its outline covers.
fn path_texel(input: VertexOutput, coordinate: vec2<i32>) -> vec4<f32> {
    let base = u32(input.rect.x);
    let matrix = paths[base];
    let translation = paths[base + 1u].xy;
    let outlines = paths[base + 2u];
    let pixel = vec2<f32>(coordinate);
    let center = pixel + vec2<f32>(0.5);
    let local = vec2<f32>(
        matrix.x * center.x + matrix.z * center.y + translation.x,
        matrix.y * center.x + matrix.w * center.y + translation.y,
    );
    var color = vec4<f32>(0.0);
    if outlines.y >= 0.0 {
        let stroke = nonzero_coverage(paths[input.path_tile + u32(outlines.y)], pixel);
        if stroke > 0.0 {
            let paint = paint_color(input.stroke, local) / 255.0;
            color = vec4<f32>(paint.rgb, 1.0) * paint.a * stroke;
        }
    }
    if outlines.x >= 0.0 && color.a < 1.0 {
        let fill = nonzero_coverage(paths[input.path_tile + u32(outlines.x)], pixel);
        if fill > 0.0 {
            let paint = paint_color(input.fill, local) / 255.0;
            color += vec4<f32>(paint.rgb, 1.0) * paint.a * fill * (1.0 - color.a);
        }
    }
    if color.a <= 0.0 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(color.rgb / color.a, color.a);
}

// How much of the pixel at canvas position `world` is inside every clip in
// the chain starting at `index`, anti-aliased over the edge like `rect_color`.
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
    if input.size_opacity_kind.w == 2.0 {
        // Paths are shaded at their rasterized size and have no mips.
        return vec2<i32>(round(input.size_opacity_kind.xy));
    }
    return vec2<i32>(textureDimensions(source_texture, level));
}

// The straight-alpha texel at `coordinate`, which must lie inside the layer.
fn texel(input: VertexOutput, coordinate: vec2<i32>, level: i32) -> vec4<f32> {
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
    // Paths are already outlined in canvas pixels and never filtered. Keep
    // their coverage loops out of `texel`, which is inlined for every tap
    // of the image filter by DX12's FXC compiler.
    if input.size_opacity_kind.w == 2.0 {
        let size = texel_size(input, 0);
        let coordinate = min(vec2<i32>(floor(input.uv * vec2<f32>(size))), size - vec2<i32>(1));
        return path_texel(input, coordinate);
    }
    // Rects are shaded per pixel and never filtered.
    if input.size_opacity_kind.w == 1.0 {
        return rect_color(input);
    }
    if input.sampling.x == 0.0 {
        // Texels land one to one on pixels: copy the one under the pixel.
        let size = texel_size(input, 0);
        let coordinate = min(vec2<i32>(floor(input.uv * vec2<f32>(size))), size - vec2<i32>(1));
        return texel(input, coordinate, 0);
    }
    // Trilinear: blend the two mip levels around the level of detail.
    var levels = 1;
    if input.size_opacity_kind.w == 0.0 {
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
