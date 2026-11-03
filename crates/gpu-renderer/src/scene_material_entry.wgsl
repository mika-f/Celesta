// The entry points every scene material is assembled with
// (`scene_material.rs`), after the material's code: the surface pass's
// vertex, which places a skinned vertex in the world and calls the
// material's `material_vertex`, and a fragment entry point for each alpha
// mode, which calls its `material_fragment`.

// The skinned vertex (buffer 0) and the geometry's first texture
// coordinates (buffer 1).
struct CelestaVertexInput {
    @location(0) position: vec4f,
    @location(1) normal: vec4f,
    @location(2) tangent: vec4f,
    @location(3) uv: vec2f,
};

struct CelestaVarying {
    @builtin(position) clip: vec4f,
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) tangent: vec4f,
    @location(3) uv: vec2f,
};

// A mask material's color and the samples it covers.
struct CelestaCovered {
    @location(0) color: vec4f,
    @builtin(sample_mask) mask: u32,
};

// Samples per pixel of the canvas: `scene.rs`'s `SAMPLES`.
const CELESTA_SAMPLES: f32 = 4.0;

// `input` in world space, as the material's vertex hooks read it.
fn celesta_vertex(input: CelestaVertexInput) -> Vertex {
    let position = draw.world * input.position;
    let world = mat3x3f(draw.world[0].xyz, draw.world[1].xyz, draw.world[2].xyz);
    return Vertex(
        position.xyz,
        safe_normalize(draw.normal_matrix * input.normal.xyz),
        // `sign` keeps a geometry without tangents at 0.
        vec4f(safe_normalize(world * input.tangent.xyz), sign(input.tangent.w) * draw.handedness),
        input.uv,
        frame.view_projection * position,
    );
}

// `vertex` moved by `offset`: its position, and so its projection, by
// `offset.world`, then the projection by `offset.clip`. A vertex not moved
// in the world keeps the projection `celesta_vertex` made.
fn celesta_place(vertex: Vertex, offset: VertexOffset) -> CelestaVarying {
    let position = vertex.position + offset.world;
    let moved = frame.view_projection * vec4f(position, 1.0);
    let clip = select(moved, vertex.clip, all(offset.world == vec3f(0.0)));
    return CelestaVarying(
        vec4f(clip.xy + offset.clip, clip.zw),
        position,
        vertex.normal,
        vertex.tangent,
        vertex.uv,
    );
}

// What the material's fragment functions read of `input`.
fn celesta_fragment(input: CelestaVarying, front_facing: bool) -> Fragment {
    let tangent = vec4f(safe_normalize(input.tangent.xyz), input.tangent.w);
    // In perspective, towards the eye; orthographic, along the camera's +Z.
    let view = select(
        safe_normalize(frame.forward.xyz),
        safe_normalize(frame.eye.xyz - input.position),
        frame.eye.w == 1.0,
    );
    return Fragment(
        input.position,
        safe_normalize(input.normal),
        tangent,
        input.uv,
        view,
        front_facing,
        input.clip.xy,
    );
}

// The samples an `alpha` against the draw's cutoff covers: as many of a
// pixel's samples as its alpha, sharpened to about a pixel's width around
// the cutoff, says. This is alpha to coverage, done here because the
// pipeline's own would store that alpha in the samples too.
fn celesta_coverage(alpha: f32) -> u32 {
    let sharpened = (alpha - draw.alpha_cutoff) / max(fwidth(alpha), 1e-5) + 0.5;
    let samples = u32(clamp(sharpened, 0.0, 1.0) * CELESTA_SAMPLES + 0.5);
    return (1u << samples) - 1u;
}

@vertex
fn celesta_surface_vertex(input: CelestaVertexInput) -> CelestaVarying {
    let vertex = celesta_vertex(input);
    let offset: VertexOffset = material_vertex(vertex);
    return celesta_place(vertex, offset);
}

// An opaque material's alpha is ignored.
@fragment
fn celesta_surface_opaque(
    input: CelestaVarying,
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4f {
    let color: vec4f = material_fragment(celesta_fragment(input, front_facing));
    return vec4f(color.rgb, 1.0);
}

// A mask material covers samples by its alpha; each one it covers is
// opaque.
@fragment
fn celesta_surface_mask(
    input: CelestaVarying,
    @builtin(front_facing) front_facing: bool,
) -> CelestaCovered {
    let color: vec4f = material_fragment(celesta_fragment(input, front_facing));
    return CelestaCovered(vec4f(color.rgb, 1.0), celesta_coverage(color.a));
}

// A blend material's straight color, which the pipeline blends by its
// alpha over the canvas's premultiplied color. Alpha below 2^−10 is
// discarded, after the material has run (so its texture samples stay in
// uniform control flow), so that a clear pixel of a blend writing depth
// writes none.
@fragment
fn celesta_surface_blend(
    input: CelestaVarying,
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4f {
    let color: vec4f = material_fragment(celesta_fragment(input, front_facing));
    let alpha = clamp(color.a, 0.0, 1.0);
    if alpha < 0.0009765625 {
        discard;
    }
    return vec4f(color.rgb, alpha);
}
