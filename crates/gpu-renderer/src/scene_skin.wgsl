// Morphs and skins one draw of a plugin's scene (see `scene.rs`), one
// invocation per vertex. A vertex's position is first moved by its weighted
// morph target offsets, then taken through its weighted skinning matrices:
// `Σ wᵢ·skinᵢ·(p + Σ wₜ·Δpₜ)`. Normals and tangents follow the matrices'
// upper 3×3 and are renormalised; a tangent keeps its `w`. An unskinned
// draw keeps its morphed values. The result is the vertex buffer every
// scene material's entry points read (`scene_material_entry.wgsl`).

struct SkinVertex {
    joints: vec4<u32>,
    weights: vec4<f32>,
};

struct Skinned {
    position: vec4<f32>,
    normal: vec4<f32>,
    tangent: vec4<f32>,
};

struct Params {
    // vertices; the draw's first skinning matrix in `frame`, in elements;
    // its first morph pair in `frame`; how many morph pairs it has
    counts: vec4<u32>,
    // skinned, has tangents, has morph target normals, unused (0 or 1)
    flags: vec4<u32>,
};

@group(0) @binding(0)
var<storage, read> positions: array<vec4<f32>>;
@group(0) @binding(1)
var<storage, read> normals: array<vec4<f32>>;
@group(0) @binding(2)
var<storage, read> tangents: array<vec4<f32>>;
@group(0) @binding(3)
var<storage, read> skin_vertices: array<SkinVertex>;
// Target `t`'s offset for vertex `v` is element `t × vertices + v`.
@group(0) @binding(4)
var<storage, read> morph_positions: array<vec4<f32>>;
@group(0) @binding(5)
var<storage, read> morph_normals: array<vec4<f32>>;
@group(0) @binding(6)
var<storage, read_write> skinned: array<Skinned>;

// The frame's skinning matrices, four columns of `f32` bits each, then
// every draw's morph pairs: a target in `x` and its non-zero weight's bits
// in `y`.
@group(1) @binding(0)
var<storage, read> frame: array<vec4<u32>>;
@group(1) @binding(1)
var<uniform> params: Params;

const WORKGROUP: u32 = 64u;

fn joint_matrix(joint: u32) -> mat4x4<f32> {
    let first = params.counts.y + joint * 4u;
    return mat4x4<f32>(
        bitcast<vec4<f32>>(frame[first]),
        bitcast<vec4<f32>>(frame[first + 1u]),
        bitcast<vec4<f32>>(frame[first + 2u]),
        bitcast<vec4<f32>>(frame[first + 3u]),
    );
}

// `value` scaled to length 1, or left as it is when it has no length.
fn unit(value: vec3<f32>) -> vec3<f32> {
    let length_squared = dot(value, value);
    if length_squared > 0.0 {
        return value * inverseSqrt(length_squared);
    }
    return value;
}

@compute @workgroup_size(64)
fn cs_main(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(num_workgroups) groups: vec3<u32>,
) {
    // Large geometries are dispatched as rows of workgroups.
    let vertex = id.y * groups.x * WORKGROUP + id.x;
    let vertices = params.counts.x;
    if vertex >= vertices {
        return;
    }
    var position = positions[vertex].xyz;
    var normal = normals[vertex].xyz;
    let morph_normals_present = params.flags.z != 0u;
    for (var pair = 0u; pair < params.counts.w; pair += 1u) {
        let morph = frame[params.counts.z + pair];
        let weight = bitcast<f32>(morph.y);
        let element = morph.x * vertices + vertex;
        position += weight * morph_positions[element].xyz;
        if morph_normals_present {
            normal += weight * morph_normals[element].xyz;
        }
    }
    var tangent = vec4<f32>(0.0);
    if params.flags.y != 0u {
        tangent = tangents[vertex];
    }
    var placed = vec4<f32>(position, 1.0);
    var tangent_xyz = tangent.xyz;
    if params.flags.x != 0u {
        let skin = skin_vertices[vertex];
        var matrix = mat4x4<f32>(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0));
        for (var i = 0u; i < 4u; i += 1u) {
            matrix += joint_matrix(skin.joints[i]) * skin.weights[i];
        }
        placed = matrix * placed;
        let linear = mat3x3<f32>(matrix[0].xyz, matrix[1].xyz, matrix[2].xyz);
        normal = linear * normal;
        tangent_xyz = linear * tangent_xyz;
    }
    skinned[vertex] = Skinned(
        placed,
        vec4<f32>(unit(normal), 0.0),
        vec4<f32>(unit(tangent_xyz), tangent.w),
    );
}
