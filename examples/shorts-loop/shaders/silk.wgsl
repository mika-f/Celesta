// The background, drawn from nothing: the layer underneath is ignored and
// every pixel is computed here. Bands of color follow a plane folded by
// three waves. Each wave turns a whole number of times as `theta` runs from
// 0 to 2π, so the last frame flows into the first.

fn band(v: f32) -> f32 {
    return 0.5 + 0.5 * cos(v);
}

// Breaks up the gradient's 8-bit steps so the video encoder does not band them.
fn dither(position: vec2f) -> f32 {
    return fract(sin(dot(position, vec2f(12.9898, 78.233))) * 43758.5453) - 0.5;
}

fn effect(input: EffectInput) -> vec4f {
    let t = params.theta;
    // Centred on the scene and scaled by its width, so the field is round.
    var p = (input.position - 0.5 * celesta.scene_size) / celesta.scene_size.x;
    p += 0.10 * vec2f(sin(p.y * 5.0 + t), cos(p.x * 4.0 - t));
    p += 0.05 * vec2f(sin(p.y * 9.0 - 2.0 * t + 1.3), cos(p.x * 8.0 + 2.0 * t + 0.4));
    p += 0.025 * vec2f(sin(p.y * 17.0 + 3.0 * t + 2.1), cos(p.x * 15.0 - 3.0 * t + 0.9));

    let r = length(p - params.focus);
    let v = p.x * 6.0 + p.y * 3.0 + sin(r * 8.0 - t) * 1.6;
    let silk = band(v * 2.0 + t);
    let sheen = pow(band(v * 5.0 - 2.0 * t), 8.0);

    var color = mix(params.deep, params.low, silk * 0.8);
    color = mix(color, params.high, sheen * 0.55 * (1.0 - smoothstep(0.1, 0.9, r)));
    // Light from the kaleidoscope, a little stronger on every beat.
    color += params.high * (0.10 + 0.14 * params.pulse) * exp(-r * 4.0);
    color *= mix(1.0, 0.3, smoothstep(0.3, 1.0, r));
    return vec4f(color.rgb + dither(input.position) / 255.0, 1.0);
}
