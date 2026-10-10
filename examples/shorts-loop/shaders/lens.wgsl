// A shock ring over the whole frame. Pixels near the ring, `params.radius`
// from `params.center`, are pulled in on its inside and pushed out on its
// outside, their red and blue drift apart, and the ring catches some light.

// Reads stay on the layer: past its edge `source_at` is transparent, which
// would show as a dark seam down the sides of the frame, and clamping would
// smear the edge pixels into streaks. A read past an edge mirrors back in.
fn read(position: vec2f) -> vec4f {
    let low = celesta.content.xy + vec2f(0.5);
    let span = celesta.content.zw - vec2f(0.5) - low;
    let t = abs(position - low) % (2.0 * span);
    return source_at(low + min(t, 2.0 * span - t));
}

fn effect(input: EffectInput) -> vec4f {
    let d = input.position - params.center;
    let r = length(d);
    let direction = d / max(r, 1e-4);
    let x = (r - params.radius) / params.width;
    let ring = exp(-x * x);
    let p = input.position + direction * (2.0 * x * ring * params.strength);
    let split = direction * ring * params.strength * 0.4;
    let red = read(p + split);
    let green = read(p);
    let blue = read(p - split);
    let color = vec4f(red.r, green.g, blue.b, max(green.a, max(red.a, blue.a)));
    return color + premultiply(params.tint) * ring * params.strength * 0.006;
}
