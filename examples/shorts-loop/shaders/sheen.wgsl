// Light on the copy. The layer's red and blue are read `params.split` pixels
// apart, and a slanted band crossing the layer's box turns the letters
// `params.tint`. The band sits at `params.sweep` (0 at the box's left edge, 1
// at its right); it is fully off the letters at both ends of its run, so
// restarting the run shows no jump.

fn effect(input: EffectInput) -> vec4f {
    let shift = vec2f(params.split, 0.0);
    let red = source_at(input.position + shift);
    let green = source_at(input.position);
    let blue = source_at(input.position - shift);
    var color = vec4f(red.r, green.g, blue.b, max(green.a, max(red.a, blue.a)));

    let u = input.uv.x + 0.25 * input.uv.y;
    let x = (u - params.sweep) / 0.09;
    let band = exp(-x * x);
    // The letters are near white, so adding light would only clip: inside the
    // band they take the tint instead, at their own coverage.
    let tinted = vec4f(params.tint.rgb * color.a, color.a);
    return mix(color, tinted, band * 0.9);
}
