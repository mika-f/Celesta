// Pulls the red and blue channels apart horizontally by `params.amount`
// pixels, keeping green in place.
fn effect(input: EffectInput) -> vec4f {
    let shift = vec2f(params.amount, 0.0);
    let red = source_at(input.position - shift);
    let center = source_at(input.position);
    let blue = source_at(input.position + shift);
    return vec4f(red.r, center.g, blue.b, max(max(red.a, center.a), blue.a));
}
