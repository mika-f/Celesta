// Separable Gaussian blur of a premultiplied scene-sized layer. The second
// pass can shift and tint the blurred alpha for a shadow or glow. A large
// blur runs on a copy `downsample` shrinks by a whole factor, and `upsample`
// enlarges the result again, shifting and tinting it instead.
struct Params {
    direction_radius_mode: vec4<f32>,
    offset: vec4<f32>,
    color: vec4<f32>,
};

// Bound like `layer.wgsl`'s layer texture, so a canvas's bind group serves both.
@group(0) @binding(0)
var source: texture_2d<f32>;

// One pass's parameters, at a dynamic offset into the frame's buffer:
// `direction_radius_mode` is the blur's direction, its sigma, and whether to
// tint; `offset.xy` the shadow's offset in output pixels, and `offset.z` the
// factor `downsample` and `upsample` scale by.
@group(1) @binding(0)
var<uniform> params: Params;

// Bilinear and clamped to the edge; the fragment shader keeps every fetch off
// texels outside the source, which count as transparent.
@group(1) @binding(1)
var bilinear: sampler;

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

// The weights of texels `low` and `low + 1` of a line `size` texels long, as
// one bilinear fetch: where between them to sample, and how much the fetch
// weighs. Texels outside the line weigh nothing, so a fetch beside an edge
// lands exactly on the texel inside it.
fn pair(low: i32, low_weight: f32, high_weight: f32, size: i32) -> vec2<f32> {
    let low_inside = select(0.0, low_weight, low >= 0 && low < size);
    let high_inside = select(0.0, high_weight, low + 1 >= 0 && low + 1 < size);
    let weight = low_inside + high_inside;
    if weight <= 0.0 {
        return vec2<f32>(f32(low), 0.0);
    }
    return vec2<f32>(f32(low) + high_inside / weight, weight);
}

// Sums the Gaussian's taps along `direction` around the possibly fractional
// `center` (a shadow's offset), reading between texels as bilinear
// interpolation would. Each fetch covers two adjacent texels — the sampler
// mixes them in the ratio of their weights — so a kernel of `2 * extent + 1`
// taps takes `extent + 1` fetches.
@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let texel = 1.0 / vec2<f32>(size);
    let center = input.position.xy - vec2<f32>(0.5) - params.offset.xy;
    let horizontal = params.direction_radius_mode.x != 0.0;
    let sigma = clamp(params.direction_radius_mode.z, 0.0, 64.0);
    var extent = i32(ceil(sigma * 3.0));
    if sigma == 0.0 {
        extent = 0;
    }
    // Across the blur, every fetch reads the same bilinear column (or row).
    let across = select(center.x, center.y, horizontal);
    let across_floor = floor(across);
    let across_fraction = across - across_floor;
    let across_pair = pair(i32(across_floor), 1.0 - across_fraction, across_fraction,
        select(size.x, size.y, horizontal));
    // Along it, `center` lies `fraction` past texel `base`, so texel
    // `base + k` weighs `(1 - fraction) * g(k) + fraction * g(k - 1)` for `k`
    // in `-extent..=extent + 1`: an even count, so pairs tile it.
    let along = select(center.y, center.x, horizontal);
    let along_size = select(size.y, size.x, horizontal);
    let base = i32(floor(along));
    let fraction = along - floor(along);
    // g(k) = exp(-k^2 / (2 sigma^2)) by recurrence rather than an `exp` per
    // tap: g(k + 1) = g(k) * ratio, with ratio shrinking by `step` each tap.
    // The clamps only bite below sigma ~0.09, where g(-extent) underflows
    // and the first ratio overflows; the taps beside the center are
    // negligible there, and the center still comes out as 1e-30 * 1e30.
    var current = 1.0;
    var ratio = 1.0;
    var step = 1.0;
    if sigma > 0.0 {
        let scale = 1.0 / (2.0 * sigma * sigma);
        current = max(exp(-f32(extent * extent) * scale), 1e-30);
        ratio = min(exp(f32(2 * extent - 1) * scale), 1e30);
        step = exp(-2.0 * scale);
    }
    var previous = 0.0;
    var sum = vec4<f32>(0.0);
    var weight_sum = 0.0;
    for (var k = -extent; k <= extent; k += 2) {
        let next = select(current * ratio, 0.0, k == extent);
        ratio *= step;
        let low_weight = mix(current, previous, fraction);
        let high_weight = mix(next, current, fraction);
        weight_sum += low_weight + high_weight;
        let tap = pair(base + k, low_weight, high_weight, along_size);
        if tap.y > 0.0 {
            var point = vec2<f32>(tap.x, across_pair.x);
            if !horizontal {
                point = vec2<f32>(across_pair.x, tap.x);
            }
            sum += textureSampleLevel(source, bilinear, (point + vec2<f32>(0.5)) * texel, 0.0)
                * tap.y;
        }
        previous = next;
        current = next * ratio;
        ratio *= step;
    }
    let blurred = sum * across_pair.y / weight_sum;
    if params.direction_radius_mode.w > 0.5 {
        let alpha = blurred.a * params.color.a;
        return vec4<f32>(params.color.rgb * alpha, alpha);
    }
    return blurred;
}

// Each texel is the mean of the `factor` x `factor` block of source texels it
// covers. Texels past the source's edge count as transparent, as they do for
// the blur.
@fragment
fn downsample(input: VertexOutput) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let factor = i32(params.offset.z);
    let origin = vec2<i32>(input.position.xy) * factor;
    let end = min(origin + vec2<i32>(factor), size);
    var sum = vec4<f32>(0.0);
    for (var y = origin.y; y < end.y; y++) {
        for (var x = origin.x; x < end.x; x++) {
            sum += textureLoad(source, vec2<i32>(x, y), 0);
        }
    }
    return sum / f32(factor * factor);
}

// Bilinearly enlarges a blur `downsample` shrank by `factor`, shifted by the
// offset and tinted like the blur's second pass. `direction_radius_mode.xy`
// is the size of the canvas it enlarges onto. Within the canvas the blurred
// field is smooth, so its outermost half texel is extrapolated from the two
// texels beside the edge; a shadow that reads past the canvas reads transparency there, weighted as
// the full-resolution pass's bilinear read would be.
@fragment
fn upsample(input: VertexOutput) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let canvas = params.direction_radius_mode.xy;
    let center = input.position.xy - vec2<f32>(0.5) - params.offset.xy;
    let inside = clamp(center + vec2<f32>(1.0), vec2<f32>(0.0), vec2<f32>(1.0))
        * clamp(canvas - center, vec2<f32>(0.0), vec2<f32>(1.0));
    let at = (center + vec2<f32>(0.5)) / params.offset.z - vec2<f32>(0.5);
    let last = size - vec2<i32>(1);
    let low = clamp(vec2<i32>(floor(at)), vec2<i32>(0), max(last - vec2<i32>(1), vec2<i32>(0)));
    let high = min(low + vec2<i32>(1), last);
    let fraction = at - vec2<f32>(low);
    let top = mix(textureLoad(source, low, 0), textureLoad(source, vec2<i32>(high.x, low.y), 0), fraction.x);
    let bottom = mix(textureLoad(source, vec2<i32>(low.x, high.y), 0), textureLoad(source, high, 0),
        fraction.x);
    // Past the outermost texel centers, `fraction` leaves 0..1 and the edge
    // pair's slope carries on for that half texel.
    let field = mix(top, bottom, fraction.y);
    let alpha = clamp(field.a, 0.0, 1.0);
    let enlarged = vec4<f32>(clamp(field.rgb, vec3<f32>(0.0), vec3<f32>(alpha)), alpha) * inside.x * inside.y;
    if params.direction_radius_mode.w > 0.5 {
        let alpha = enlarged.a * params.color.a;
        return vec4<f32>(params.color.rgb * alpha, alpha);
    }
    return enlarged;
}
