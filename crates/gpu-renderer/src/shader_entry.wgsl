// The entry points every custom shader is assembled with (`shader.rs`),
// after the author's code: a full-screen triangle whose fragments call the
// author's `effect` and clamp its result to a valid premultiplied color.

@vertex
fn celesta_vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let vertices = array<vec2f, 3>(
        vec2f(-1.0, -1.0),
        vec2f(3.0, -1.0),
        vec2f(-1.0, 3.0),
    );
    return vec4f(vertices[index], 0.0, 1.0);
}

@fragment
fn celesta_fragment(@builtin(position) pixel: vec4f) -> @location(0) vec4f {
    let position = pixel.xy + celesta.celesta_canvas_origin;
    let box = celesta.content;
    // Only keeps a box of no width or height from dividing by zero.
    let size = max(box.zw - box.xy, vec2f(1e-4));
    let color: vec4f = effect(EffectInput(position, (position - box.xy) / size));
    let alpha = clamp(color.a, 0.0, 1.0);
    return vec4f(clamp(color.rgb, vec3f(0.0), vec3f(alpha)), alpha);
}
