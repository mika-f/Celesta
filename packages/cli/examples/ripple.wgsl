// Rings spreading from the centre of the layer, pushing its pixels outward
// and back. `params.time` is in seconds; `params.amplitude` in pixels.
fn effect(input: EffectInput) -> vec4f {
    let center = mix(celesta.content.xy, celesta.content.zw, vec2f(0.5));
    let offset = input.position - center;
    let distance = length(offset);
    let wave = sin(distance * 0.06 - params.time * 6.0) * params.amplitude;
    let direction = offset / max(distance, 1e-4);
    return source_at(input.position - direction * wave);
}
