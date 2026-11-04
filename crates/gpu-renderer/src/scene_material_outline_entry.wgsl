// The outline pass's entry points (`scene_material.rs`), assembled after
// `scene_material_entry.wgsl` for a material that defines
// `material_outline_fragment`: the same as the surface pass's, around the
// material's `material_outline_vertex` and `material_outline_fragment`.

@vertex
fn celesta_outline_vertex(input: CelestaVertexInput) -> CelestaVarying {
    let vertex = celesta_vertex(input);
    let offset: VertexOffset = material_outline_vertex(vertex);
    return celesta_place(vertex, offset);
}

@fragment
fn celesta_outline_opaque(
    input: CelestaVarying,
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4f {
    let color: vec4f = material_outline_fragment(celesta_fragment(input, front_facing));
    return vec4f(color.rgb, 1.0);
}

@fragment
fn celesta_outline_mask(
    input: CelestaVarying,
    @builtin(front_facing) front_facing: bool,
) -> CelestaCovered {
    let color: vec4f = material_outline_fragment(celesta_fragment(input, front_facing));
    return CelestaCovered(vec4f(color.rgb, 1.0), celesta_coverage(color.a));
}

@fragment
fn celesta_outline_blend(
    input: CelestaVarying,
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4f {
    let color: vec4f = material_outline_fragment(celesta_fragment(input, front_facing));
    let alpha = clamp(color.a, 0.0, 1.0);
    if alpha < 0.0009765625 {
        discard;
    }
    return vec4f(color.rgb, alpha);
}
