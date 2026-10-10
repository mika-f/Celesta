// Folds the layer into a kaleidoscope around `params.center`. The plane is
// cut into wedges; each wedge mirrors one thin slice of the layer, so a dot
// passing through that slice appears in every wedge at once. Two wedge counts
// (`params.sectors`) cross-fade by `params.blend`, and the three color
// channels are read `params.split` pixels apart along the radius.

const TAU: f32 = 6.28318530718;

// Where in the layer an offset `d` from the centre reads from.
fn fold(d: vec2f, sectors: f32) -> vec2f {
    let r = length(d);
    let wedge = TAU / sectors;
    var a = atan2(d.y, d.x) - params.spin;
    a = a - wedge * floor(a / wedge);
    a = min(a, wedge - a);
    a += params.slice + params.twist * sin(r * 0.012);
    return params.center + r * vec2f(cos(a), sin(a));
}

fn mirror(d: vec2f, split: vec2f, sectors: f32) -> vec4f {
    let red = source_at(fold(d + split, sectors));
    let green = source_at(fold(d, sectors));
    let blue = source_at(fold(d - split, sectors));
    // Premultiplied: each channel stays within the largest of the alphas.
    return vec4f(red.r, green.g, blue.b, max(green.a, max(red.a, blue.a)));
}

fn effect(input: EffectInput) -> vec4f {
    let d = input.position - params.center;
    // Guarded: the direction of the zero offset at the centre is undefined.
    let split = d / max(length(d), 1e-4) * params.split;
    return mix(mirror(d, split, params.sectors.x), mirror(d, split, params.sectors.y), params.blend);
}
