//! A small VRM 1.0 avatar built in code, so every test has a model to open
//! without committing a real one. Its public fields let a test vary one part
//! (the meta, an expression, an image, a missing bone, …) and build the file
//! again with [`SyntheticVrm::glb`].
//!
//! The avatar, in glTF's space (+Y up, facing +Z, its left at +X), metres:
//!
//! - a humanoid of 19 bones from the hips to the eyes, with a three-node hair
//!   chain under the head and a left upper arm whose rest rotation is
//!   Rz(+90°), as VRoid exports arms;
//! - `Body` (mesh 0, skinned to hips, head, and spine): a box from
//!   (−0.2, 0, −0.1) to (0.2, 1.6, 0.1) in material `Body` and a quad at the
//!   back of the head in material `Hair`, both drawn from one 12-vertex
//!   accessor set, with three morph targets;
//! - `FirstPersonOnly` (mesh 1): an unskinned quad the first-person rule
//!   hides from a third-person camera;
//! - materials `Body` (MToon, opaque), `Hair` (MToon, blended), `Pbr` (plain
//!   glTF), and `Unlit` (`KHR_materials_unlit`);
//! - expressions `happy`, `blink`, `aa`, `lookLeft`, and a custom `wink`; a
//!   bone lookAt; permissive-for-video usage terms.
//!
//! [`SyntheticVrma`] builds a VRM animation (`.vrma`) the same way.

use glam::{Quat, Vec3};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// The avatar's nodes, numbered in pre-order from `Root`.
pub mod node {
    pub const ROOT: usize = 0;
    pub const HIPS: usize = 1;
    pub const SPINE: usize = 2;
    pub const CHEST: usize = 3;
    pub const NECK: usize = 4;
    pub const HEAD: usize = 5;
    pub const LEFT_EYE: usize = 6;
    pub const RIGHT_EYE: usize = 7;
    pub const HAIR0: usize = 8;
    pub const HAIR1: usize = 9;
    pub const HAIR2: usize = 10;
    pub const LEFT_UPPER_ARM: usize = 11;
    pub const LEFT_LOWER_ARM: usize = 12;
    pub const LEFT_HAND: usize = 13;
    pub const RIGHT_UPPER_ARM: usize = 14;
    pub const RIGHT_LOWER_ARM: usize = 15;
    pub const RIGHT_HAND: usize = 16;
    pub const LEFT_UPPER_LEG: usize = 17;
    pub const LEFT_LOWER_LEG: usize = 18;
    pub const LEFT_FOOT: usize = 19;
    pub const RIGHT_UPPER_LEG: usize = 20;
    pub const RIGHT_LOWER_LEG: usize = 21;
    pub const RIGHT_FOOT: usize = 22;
    pub const BODY: usize = 23;
    pub const FIRST_PERSON_ONLY: usize = 24;
}

/// A node: its name, the humanoid bone it is, its parent, its position in
/// model space at rest, and its local rest rotation (x, y, z, w).
struct NodeSpec {
    name: &'static str,
    bone: Option<&'static str>,
    parent: Option<usize>,
    world: [f64; 3],
    rotation: [f64; 4],
}

const NO_TURN: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
/// Rz(+90°): the upper arm's rest rotation.
const ARM_TURN: [f64; 4] = [
    0.0,
    0.0,
    std::f64::consts::FRAC_1_SQRT_2,
    std::f64::consts::FRAC_1_SQRT_2,
];

const fn spec(
    name: &'static str,
    bone: Option<&'static str>,
    parent: Option<usize>,
    world: [f64; 3],
) -> NodeSpec {
    NodeSpec {
        name,
        bone,
        parent,
        world,
        rotation: NO_TURN,
    }
}

const fn bone(name: &'static str, parent: usize, world: [f64; 3]) -> NodeSpec {
    spec(name, Some(name), Some(parent), world)
}

fn nodes() -> [NodeSpec; 25] {
    use node::*;
    [
        spec("Root", None, None, [0.0, 0.0, 0.0]),
        bone("hips", ROOT, [0.0, 0.9, 0.0]),
        bone("spine", HIPS, [0.0, 1.0, 0.0]),
        bone("chest", SPINE, [0.0, 1.15, 0.0]),
        bone("neck", CHEST, [0.0, 1.3, 0.0]),
        bone("head", NECK, [0.0, 1.4, 0.0]),
        bone("leftEye", HEAD, [0.03, 1.45, 0.05]),
        bone("rightEye", HEAD, [-0.03, 1.45, 0.05]),
        spec("hair0", None, Some(HEAD), [0.0, 1.55, -0.05]),
        spec("hair1", None, Some(HAIR0), [0.0, 1.45, -0.1]),
        spec("hair2", None, Some(HAIR1), [0.0, 1.35, -0.12]),
        NodeSpec {
            rotation: ARM_TURN,
            ..bone("leftUpperArm", CHEST, [0.15, 1.25, 0.0])
        },
        bone("leftLowerArm", LEFT_UPPER_ARM, [0.40, 1.25, 0.0]),
        bone("leftHand", LEFT_LOWER_ARM, [0.62, 1.25, 0.0]),
        bone("rightUpperArm", CHEST, [-0.15, 1.25, 0.0]),
        bone("rightLowerArm", RIGHT_UPPER_ARM, [-0.40, 1.25, 0.0]),
        bone("rightHand", RIGHT_LOWER_ARM, [-0.62, 1.25, 0.0]),
        bone("leftUpperLeg", HIPS, [0.1, 0.85, 0.0]),
        bone("leftLowerLeg", LEFT_UPPER_LEG, [0.1, 0.45, 0.0]),
        bone("leftFoot", LEFT_LOWER_LEG, [0.1, 0.05, 0.0]),
        bone("rightUpperLeg", HIPS, [-0.1, 0.85, 0.0]),
        bone("rightLowerLeg", RIGHT_UPPER_LEG, [-0.1, 0.45, 0.0]),
        bone("rightFoot", RIGHT_LOWER_LEG, [-0.1, 0.05, 0.0]),
        spec("Body", None, Some(ROOT), [0.0, 0.0, 0.0]),
        spec("FirstPersonOnly", None, Some(ROOT), [0.0, 1.45, 0.1]),
    ]
}

/// The skin's joints. None is rotated at rest, so each inverse bind is the
/// translation back from the joint's position.
const JOINTS: [usize; 3] = [node::HIPS, node::HEAD, node::SPINE];
/// The joint the Body box's second weight slot names, which the skin lacks.
const MISSING_JOINT: u16 = 99;

/// How to build the avatar; [`SyntheticVrm::avatar`] is the reference one.
#[derive(Clone, Debug)]
pub struct SyntheticVrm {
    /// Writes a VRM 0.x `VRM` extension instead of `VRMC_vrm`.
    pub vrm0: bool,
    /// `VRMC_vrm.specVersion`.
    pub spec_version: String,
    /// A humanoid bone (by its VRM name) left out of `humanBones`.
    pub omit_bone: Option<String>,
    /// Whether the meshes carry `NORMAL`.
    pub normals: bool,
    /// Every `TEXCOORD_0` at the same point.
    pub flat_uvs: bool,
    /// The Body box vertices' weight on their second joint slot, which names
    /// joint 99 of a 3-joint skin; the first slot keeps the rest.
    pub slot1_weight: f32,
    /// `VRMC_vrm.meta`.
    pub meta: Value,
    /// `VRMC_vrm.expressions`.
    pub expressions: Value,
    /// `VRMC_vrm.lookAt`.
    pub look_at: Value,
    /// The images by index: MIME type and bytes, embedded in the binary chunk.
    pub images: Vec<(String, Vec<u8>)>,
    /// Root extensions to add or replace; `null` removes one (`VRMC_vrm`
    /// included, which leaves a plain glTF file).
    pub extensions: Map<String, Value>,
    /// Extensions merged into a node's `extensions`, by node index.
    pub node_extensions: Vec<(usize, Value)>,
}

impl SyntheticVrm {
    /// The reference avatar described in the module documentation.
    pub fn avatar() -> Self {
        Self {
            vrm0: false,
            spec_version: "1.0".into(),
            omit_bone: None,
            normals: true,
            flat_uvs: false,
            slot1_weight: 0.0,
            meta: json!({
                "name": "Synthetic",
                "authors": ["Celesta"],
                "licenseUrl": "https://vrm.dev/licenses/1.0/",
                "avatarPermission": "everyone",
                "commercialUsage": "corporation",
                "creditNotation": "unnecessary",
                "allowExcessivelyViolentUsage": false,
                "allowExcessivelySexualUsage": false,
                "allowPoliticalOrReligiousUsage": false,
                "allowAntisocialOrHateUsage": false,
                "allowRedistribution": false,
                "modification": "prohibited",
            }),
            expressions: json!({
                "preset": {
                    "happy": {
                        "morphTargetBinds": [{ "node": node::BODY, "index": 0, "weight": 1.0 }],
                        "materialColorBinds": [
                            { "material": 0, "type": "color", "targetValue": [1.0, 0.0, 0.0, 1.0] },
                            { "material": 0, "type": "emissionColor", "targetValue": [1.0, 1.0, 0.0, 1.0] },
                        ],
                        "overrideBlink": "block",
                        "overrideMouth": "blend",
                        "overrideLookAt": "none",
                    },
                    "blink": {
                        "morphTargetBinds": [{ "node": node::BODY, "index": 0, "weight": 0.5 }],
                    },
                    "aa": {
                        "textureTransformBinds": [
                            { "material": 0, "offset": [0.25, 0.0], "scale": [1.0, 1.0] },
                        ],
                    },
                    "lookLeft": {
                        "morphTargetBinds": [{ "node": node::BODY, "index": 2, "weight": 1.0 }],
                    },
                },
                "custom": {
                    "wink": {
                        "isBinary": true,
                        "morphTargetBinds": [{ "node": node::BODY, "index": 0, "weight": 1.0 }],
                    },
                },
            }),
            look_at: json!({
                "type": "bone",
                "offsetFromHeadBone": [0.0, 0.06, 0.0],
                "rangeMapHorizontalInner": { "inputMaxValue": 90.0, "outputScale": 10.0 },
                "rangeMapHorizontalOuter": { "inputMaxValue": 90.0, "outputScale": 20.0 },
                "rangeMapVerticalDown": { "inputMaxValue": 90.0, "outputScale": 12.0 },
                "rangeMapVerticalUp": { "inputMaxValue": 90.0, "outputScale": 10.0 },
            }),
            images: vec![
                (
                    "image/png".into(),
                    png(
                        2,
                        2,
                        &[
                            [255, 0, 0, 255],
                            [0, 255, 0, 255],
                            [0, 0, 255, 255],
                            [255, 255, 255, 255],
                        ],
                    ),
                ),
                ("image/png".into(), png(1, 1, &[[128, 128, 255, 255]])),
            ],
            extensions: Map::new(),
            node_extensions: Vec::new(),
        }
    }

    /// The avatar as a `.vrm` (binary glTF) file.
    pub fn glb(&self) -> Vec<u8> {
        self.glb_with(|_| {})
    }

    /// The avatar as a `.vrm` file whose root JSON `edit` changes first,
    /// for files no field above describes (a broken accessor, a node with
    /// two parents, …). The binary chunk is the avatar's.
    pub fn glb_with(&self, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
        let specs = nodes();
        if let Some(omitted) = &self.omit_bone {
            assert!(
                specs.iter().any(|spec| spec.bone == Some(omitted.as_str())),
                "omit_bone names {omitted}, which the avatar does not have"
            );
        }
        let mut bin = Bin::default();

        // Mesh 0, Body: the box's eight corners (bit 0: +x, bit 1: top,
        // bit 2: +z), then the hair quad's four.
        let mut positions: Vec<[f32; 3]> = (0..8)
            .map(|i| {
                [
                    if i & 1 != 0 { 0.2 } else { -0.2 },
                    if i & 2 != 0 { 1.6 } else { 0.0 },
                    if i & 4 != 0 { 0.1 } else { -0.1 },
                ]
            })
            .collect();
        positions.extend([
            [-0.15, 1.3, -0.1],
            [0.15, 1.3, -0.1],
            [-0.15, 1.6, -0.1],
            [0.15, 1.6, -0.1],
        ]);
        let normals: Vec<[f32; 3]> = positions
            .iter()
            .enumerate()
            .map(|(i, &[x, y, z])| {
                if i < 8 {
                    let [x, y, z] = [x, y - 0.8, z];
                    let length = (x * x + y * y + z * z).sqrt();
                    [x / length, y / length, z / length]
                } else {
                    [0.0, 0.0, -1.0]
                }
            })
            .collect();
        // Every face of the box maps to a triangle of non-zero UV area, so
        // MikkTSpace has a tangent to give each corner.
        let uvs: Vec<[f32; 2]> = positions
            .iter()
            .enumerate()
            .map(|(i, &[x, y, z])| {
                if self.flat_uvs {
                    [0.0, 0.0]
                } else if i < 8 {
                    let z = (z + 0.1) / 0.2;
                    [
                        (x + 0.2) / 0.4 * 0.5 + z * 0.25,
                        (1.6 - y) / 1.6 * 0.75 + z * 0.25,
                    ]
                } else {
                    [(x + 0.15) / 0.3, (1.6 - y) / 0.3]
                }
            })
            .collect();
        let joints: Vec<[u16; 4]> = (0..12)
            .map(|i| {
                if i < 8 {
                    [0, MISSING_JOINT, 0, 0]
                } else {
                    [1, 0, 0, 0]
                }
            })
            .collect();
        let weights: Vec<[f32; 4]> = (0..12)
            .map(|i| {
                if i < 8 {
                    [1.0 - self.slot1_weight, self.slot1_weight, 0.0, 0.0]
                } else {
                    [1.0, 0.0, 0.0, 0.0]
                }
            })
            .collect();
        let mut body = Map::new();
        body.insert(
            "POSITION".into(),
            bin.floats(&positions, "VEC3", true).into(),
        );
        if self.normals {
            body.insert("NORMAL".into(), bin.floats(&normals, "VEC3", false).into());
        }
        body.insert("TEXCOORD_0".into(), bin.floats(&uvs, "VEC2", false).into());
        body.insert("JOINTS_0".into(), bin.shorts(&joints, "VEC4").into());
        body.insert(
            "WEIGHTS_0".into(),
            bin.floats(&weights, "VEC4", false).into(),
        );
        // Counter-clockwise from outside: −Z, +Z, −X, +X, −Y, +Y.
        let box_faces = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
        ];
        let box_indices: Vec<[u16; 1]> = box_faces
            .iter()
            .flat_map(|&[a, b, c, d]| [a, b, c, a, c, d])
            .map(|i| [i])
            .collect();
        let box_indices = bin.shorts(&box_indices, "SCALAR");
        // The hair quad faces −Z, like the box's back.
        let hair_indices = bin.shorts(&[[8], [10], [11], [8], [11], [9]], "SCALAR");
        let top = |delta: [f32; 3]| -> Vec<[f32; 3]> {
            (0..12)
                .map(|i| if i < 8 && i & 2 != 0 { delta } else { [0.0; 3] })
                .collect()
        };
        let targets: Vec<Value> = [[0.0, 0.1, 0.0], [0.05, 0.0, 0.0], [0.05, 0.0, 0.0]]
            .into_iter()
            .map(|delta| json!({ "POSITION": bin.floats(&top(delta), "VEC3", true) }))
            .collect();
        let body = Value::Object(body);

        // Mesh 1: a quad facing +Z around its node.
        let quad_positions = [
            [-0.05, -0.05, 0.0],
            [0.05, -0.05, 0.0],
            [-0.05, 0.05, 0.0],
            [0.05, 0.05, 0.0],
        ];
        let quad_uvs: Vec<[f32; 2]> = quad_positions
            .iter()
            .map(|&[x, y, _]| {
                if self.flat_uvs {
                    [0.0, 0.0]
                } else {
                    [x * 10.0 + 0.5, 0.5 - y * 10.0]
                }
            })
            .collect();
        let mut quad = Map::new();
        quad.insert(
            "POSITION".into(),
            bin.floats(&quad_positions, "VEC3", true).into(),
        );
        if self.normals {
            quad.insert(
                "NORMAL".into(),
                bin.floats(&[[0.0, 0.0, 1.0]; 4], "VEC3", false).into(),
            );
        }
        quad.insert(
            "TEXCOORD_0".into(),
            bin.floats(&quad_uvs, "VEC2", false).into(),
        );
        let quad_indices = bin.shorts(&[[0], [1], [3], [0], [3], [2]], "SCALAR");

        let inverse_binds: Vec<[f32; 16]> = JOINTS
            .iter()
            .map(|&joint| {
                let [x, y, z] = specs[joint].world.map(|v| -v as f32);
                [
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
                ]
            })
            .collect();
        let inverse_binds = bin.floats(&inverse_binds, "MAT4", false);

        let images: Vec<Value> = self
            .images
            .iter()
            .map(|(mime, bytes)| json!({ "bufferView": bin.view(bytes), "mimeType": mime }))
            .collect();

        let mut gltf_nodes: Vec<Value> = specs
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let mut node = json!({ "name": spec.name });
                let children: Vec<usize> = specs
                    .iter()
                    .enumerate()
                    .filter(|(_, child)| child.parent == Some(index))
                    .map(|(child, _)| child)
                    .collect();
                if !children.is_empty() {
                    node["children"] = json!(children);
                }
                if let Some(parent) = spec.parent {
                    let offset: [f64; 3] =
                        std::array::from_fn(|axis| spec.world[axis] - specs[parent].world[axis]);
                    // Into the parent's frame: undo every rotation above.
                    let turn = world_rotation(&specs, parent);
                    node["translation"] = json!(rotate(conjugate(turn), offset));
                }
                if spec.rotation != NO_TURN {
                    node["rotation"] = json!(spec.rotation);
                }
                node
            })
            .collect();
        gltf_nodes[node::BODY]["mesh"] = json!(0);
        gltf_nodes[node::BODY]["skin"] = json!(0);
        gltf_nodes[node::FIRST_PERSON_ONLY]["mesh"] = json!(1);
        let mut used: BTreeSet<String> = [
            "KHR_materials_unlit",
            "KHR_texture_transform",
            "VRMC_materials_mtoon",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        for (index, extensions) in &self.node_extensions {
            let node = &mut gltf_nodes[*index];
            if node.get("extensions").is_none() {
                node["extensions"] = json!({});
            }
            if let (Some(target), Some(source)) =
                (node["extensions"].as_object_mut(), extensions.as_object())
            {
                for (name, value) in source {
                    used.insert(name.clone());
                    target.insert(name.clone(), value.clone());
                }
            }
        }

        let bones: Vec<(&str, usize)> = specs
            .iter()
            .enumerate()
            .filter_map(|(index, spec)| Some((spec.bone?, index)))
            .filter(|(bone, _)| self.omit_bone.as_deref() != Some(*bone))
            .collect();
        let mut extensions = Map::new();
        if self.vrm0 {
            extensions.insert(
                "VRM".into(),
                json!({
                    "exporterVersion": "Celesta synthetic",
                    "specVersion": "0.0",
                    "meta": { "title": "Synthetic", "author": "Celesta" },
                    "humanoid": {
                        "humanBones": bones
                            .iter()
                            .map(|(bone, node)| json!({ "bone": bone, "node": node }))
                            .collect::<Vec<_>>(),
                    },
                }),
            );
        } else {
            let human_bones: Map<String, Value> = bones
                .iter()
                .map(|(bone, node)| (bone.to_string(), json!({ "node": node })))
                .collect();
            extensions.insert(
                "VRMC_vrm".into(),
                json!({
                    "specVersion": self.spec_version,
                    "meta": self.meta,
                    "humanoid": { "humanBones": human_bones },
                    "firstPerson": {
                        "meshAnnotations": [
                            { "node": node::FIRST_PERSON_ONLY, "type": "firstPersonOnly" },
                        ],
                    },
                    "lookAt": self.look_at,
                    "expressions": self.expressions,
                }),
            );
        }
        for (name, value) in &self.extensions {
            if value.is_null() {
                extensions.remove(name);
            } else {
                extensions.insert(name.clone(), value.clone());
            }
        }
        used.extend(extensions.keys().cloned());

        let mtoon = |extra: Value| {
            let mut mtoon = json!({ "specVersion": "1.0" });
            if let (Some(mtoon), Value::Object(extra)) = (mtoon.as_object_mut(), extra) {
                mtoon.extend(extra);
            }
            json!({ "VRMC_materials_mtoon": mtoon })
        };
        let mut root = json!({
            "asset": { "version": "2.0", "generator": "Celesta synthetic VRM" },
            "extensionsUsed": used,
            "extensions": extensions,
            "scene": 0,
            "scenes": [{ "nodes": [node::ROOT] }],
            "nodes": gltf_nodes,
            "skins": [{
                "joints": JOINTS,
                "inverseBindMatrices": inverse_binds,
                "skeleton": node::HIPS,
            }],
            "meshes": [
                {
                    "name": "Body",
                    "primitives": [
                        { "attributes": body, "indices": box_indices, "material": 0, "targets": targets },
                        { "attributes": body, "indices": hair_indices, "material": 1, "targets": targets },
                    ],
                    "weights": [0.0, 0.25, 0.0],
                },
                {
                    "name": "FirstPersonOnly",
                    "primitives": [{ "attributes": quad, "indices": quad_indices, "material": 2 }],
                },
            ],
            "materials": [
                {
                    "name": "Body",
                    "pbrMetallicRoughness": {
                        "baseColorTexture": {
                            "index": 0,
                            "extensions": { "KHR_texture_transform": { "offset": [0.5, 0.0] } },
                        },
                    },
                    "normalTexture": { "index": 1 },
                    "extensions": mtoon(json!({
                        "shadeMultiplyTexture": { "index": 0 },
                        "shadingShiftTexture": { "index": 2 },
                    })),
                },
                {
                    "name": "Hair",
                    "alphaMode": "BLEND",
                    "extensions": mtoon(json!({ "renderQueueOffsetNumber": -1 })),
                },
                {
                    "name": "Pbr",
                    "pbrMetallicRoughness": { "baseColorFactor": [0.5, 0.5, 0.5, 1.0] },
                },
                {
                    "name": "Unlit",
                    "pbrMetallicRoughness": { "baseColorFactor": [0.0, 1.0, 0.0, 1.0] },
                    "extensions": { "KHR_materials_unlit": {} },
                },
            ],
            "textures": [
                { "source": 0, "sampler": 0 },
                { "source": 1, "sampler": 0 },
                { "source": 0, "sampler": 1 },
            ],
            "samplers": [
                {},
                { "wrapS": 33071, "wrapT": 33648, "magFilter": 9728, "minFilter": 9728 },
            ],
            "images": images,
            "accessors": bin.accessors,
            "bufferViews": bin.views,
            "buffers": [{ "byteLength": bin.data.len() }],
        });
        edit(&mut root);
        glb(&root, bin.data)
    }
}

/// The clip's nodes, by index.
pub mod vrma_node {
    pub const HIPS: usize = 0;
    pub const SPINE: usize = 1;
    pub const LEFT_UPPER_ARM: usize = 2;
    pub const LEFT_LOWER_ARM: usize = 3;
    /// The node whose translation x is `happy`'s weight.
    pub const HAPPY: usize = 4;
}

/// How to build a VRM animation; [`SyntheticVrma::clip`] is the reference
/// one: a humanoid whose bones all rest unrotated, the hips at
/// (0, `hips_rest_y`, 0) at the scene's root, the spine 0.1 m above them,
/// the left upper arm 0.25 m above the spine and 0.15 m to the left, and
/// the left lower arm 0.25 m further left; one animation of one second.
/// A key list left empty writes no channel.
#[derive(Clone, Debug)]
pub struct SyntheticVrma {
    /// The hips' rest height, the rig's scale.
    pub hips_rest_y: f32,
    /// `leftUpperArm`'s rotation keys: (seconds, local rotation).
    pub arm: Vec<(f32, Quat)>,
    /// The hips' translation keys: (seconds, local translation).
    pub hips: Vec<(f32, Vec3)>,
    /// `happy`'s keys: (seconds, weight).
    pub happy: Vec<(f32, f32)>,
    /// Every sampler's: `"LINEAR"`, `"STEP"`, or `"CUBICSPLINE"` (with zero
    /// tangents).
    pub interpolation: &'static str,
}

impl SyntheticVrma {
    /// The reference clip: over one second the left upper arm turns from
    /// rest to Rz(−90°) (lowered), the hips travel 0.5 m to +Z, and `happy`
    /// rises from 0 to 1, all `LINEAR`.
    pub fn clip() -> Self {
        let h = 1.0;
        Self {
            hips_rest_y: h,
            arm: vec![
                (0.0, Quat::IDENTITY),
                (1.0, Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)),
            ],
            hips: vec![(0.0, Vec3::new(0.0, h, 0.0)), (1.0, Vec3::new(0.0, h, 0.5))],
            happy: vec![(0.0, 0.0), (1.0, 1.0)],
            interpolation: "LINEAR",
        }
    }

    /// The clip as a `.vrma` (binary glTF) file.
    pub fn glb(&self) -> Vec<u8> {
        self.glb_with(|_| {})
    }

    /// The clip as a `.vrma` file whose root JSON `edit` changes first, for
    /// files no field above describes. The binary chunk is the clip's.
    pub fn glb_with(&self, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
        use vrma_node::*;
        let cubic = self.interpolation == "CUBICSPLINE";
        let mut bin = Bin::default();
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        let mut keyed = |times: Vec<f32>, values: Vec<[f32; 4]>, kind: &str, node, path| {
            if times.is_empty() {
                return;
            }
            let times: Vec<[f32; 1]> = times.into_iter().map(|time| [time]).collect();
            // glTF asks for an input's bounds; NaN has none to write.
            let bounded = times.iter().all(|[time]| time.is_finite());
            let input = bin.floats(&times, "SCALAR", bounded);
            let values: Vec<[f32; 4]> = if cubic {
                values
                    .into_iter()
                    .flat_map(|value| [[0.0; 4], value, [0.0; 4]])
                    .collect()
            } else {
                values
            };
            let output = if kind == "VEC4" {
                bin.floats(&values, kind, false)
            } else {
                let values: Vec<[f32; 3]> = values.iter().map(|&[x, y, z, _]| [x, y, z]).collect();
                bin.floats(&values, kind, false)
            };
            channels.push(json!({
                "sampler": samplers.len(),
                "target": { "node": node, "path": path },
            }));
            samplers.push(json!({
                "input": input,
                "output": output,
                "interpolation": self.interpolation,
            }));
        };
        let (times, values) = self
            .arm
            .iter()
            .map(|&(time, rotation)| (time, rotation.to_array()))
            .unzip();
        keyed(times, values, "VEC4", LEFT_UPPER_ARM, "rotation");
        let (times, values) = self
            .hips
            .iter()
            .map(|&(time, Vec3 { x, y, z })| (time, [x, y, z, 0.0]))
            .unzip();
        keyed(times, values, "VEC3", HIPS, "translation");
        let (times, values) = self
            .happy
            .iter()
            .map(|&(time, weight)| (time, [weight, 0.0, 0.0, 0.0]))
            .unzip();
        keyed(times, values, "VEC3", HAPPY, "translation");

        let h = self.hips_rest_y;
        let mut root = json!({
            "asset": { "version": "2.0", "generator": "Celesta synthetic VRMA" },
            "extensionsUsed": ["VRMC_vrm_animation"],
            "extensions": {
                "VRMC_vrm_animation": {
                    "specVersion": "1.0",
                    "humanoid": {
                        "humanBones": {
                            "hips": { "node": HIPS },
                            "spine": { "node": SPINE },
                            "leftUpperArm": { "node": LEFT_UPPER_ARM },
                            "leftLowerArm": { "node": LEFT_LOWER_ARM },
                        },
                    },
                    "expressions": { "preset": { "happy": { "node": HAPPY } } },
                },
            },
            "scene": 0,
            "scenes": [{ "nodes": [HIPS, HAPPY] }],
            "nodes": [
                { "name": "hips", "translation": [0.0, h, 0.0], "children": [SPINE] },
                { "name": "spine", "translation": [0.0, 0.1, 0.0], "children": [LEFT_UPPER_ARM] },
                {
                    "name": "leftUpperArm",
                    "translation": [0.15, 0.25, 0.0],
                    "children": [LEFT_LOWER_ARM],
                },
                { "name": "leftLowerArm", "translation": [0.25, 0.0, 0.0] },
                { "name": "happy" },
            ],
            "animations": [{ "channels": channels, "samplers": samplers }],
            "accessors": bin.accessors,
            "bufferViews": bin.views,
            "buffers": [{ "byteLength": bin.data.len() }],
        });
        edit(&mut root);
        glb(&root, bin.data)
    }
}

/// A node's rest rotation in model space.
fn world_rotation(specs: &[NodeSpec], index: usize) -> [f64; 4] {
    let spec = &specs[index];
    match spec.parent {
        Some(parent) => multiply(world_rotation(specs, parent), spec.rotation),
        None => spec.rotation,
    }
}

fn multiply([ax, ay, az, aw]: [f64; 4], [bx, by, bz, bw]: [f64; 4]) -> [f64; 4] {
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

fn conjugate([x, y, z, w]: [f64; 4]) -> [f64; 4] {
    [-x, -y, -z, w]
}

/// `q v q⁻¹` for a unit quaternion `q`.
fn rotate(q: [f64; 4], [x, y, z]: [f64; 3]) -> [f64; 3] {
    let [x, y, z, _] = multiply(multiply(q, [x, y, z, 0.0]), conjugate(q));
    // Drop the rounding a quarter turn leaves (1e-17 rather than 0).
    [x, y, z].map(|v| (v * 1e9).round() / 1e9)
}

/// An RGBA PNG of `width × height` pixels, row by row from the top.
pub fn png(width: u32, height: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(
            pixels.as_flattened(),
            width,
            height,
            ExtendedColorType::Rgba8,
        )
        .expect("encoding a PNG in memory");
    bytes
}

/// The binary chunk and the views and accessors into it.
#[derive(Default)]
struct Bin {
    data: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Bin {
    fn view(&mut self, bytes: &[u8]) -> usize {
        while !self.data.len().is_multiple_of(4) {
            self.data.push(0);
        }
        self.views.push(json!({
            "buffer": 0,
            "byteOffset": self.data.len(),
            "byteLength": bytes.len(),
        }));
        self.data.extend_from_slice(bytes);
        self.views.len() - 1
    }

    fn accessor(&mut self, view: usize, component: u32, count: usize, kind: &str) -> usize {
        self.accessors.push(json!({
            "bufferView": view,
            "componentType": component,
            "count": count,
            "type": kind,
        }));
        self.accessors.len() - 1
    }

    /// `FLOAT` items; `bounds` writes `min` and `max`, which `POSITION` needs.
    fn floats<const N: usize>(&mut self, items: &[[f32; N]], kind: &str, bounds: bool) -> usize {
        let bytes: Vec<u8> = items
            .as_flattened()
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let view = self.view(&bytes);
        let accessor = self.accessor(view, 5126, items.len(), kind);
        if bounds {
            let fold = |pick: fn(f32, f32) -> f32| -> Vec<f32> {
                (0..N)
                    .map(|axis| {
                        items
                            .iter()
                            .map(|item| item[axis])
                            .reduce(pick)
                            .unwrap_or(0.0)
                    })
                    .collect()
            };
            self.accessors[accessor]["min"] = json!(fold(f32::min));
            self.accessors[accessor]["max"] = json!(fold(f32::max));
        }
        accessor
    }

    /// `UNSIGNED_SHORT` items.
    fn shorts<const N: usize>(&mut self, items: &[[u16; N]], kind: &str) -> usize {
        let bytes: Vec<u8> = items
            .as_flattened()
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let view = self.view(&bytes);
        self.accessor(view, 5123, items.len(), kind)
    }
}

/// A binary glTF file: the header, the JSON chunk padded with spaces, and
/// the binary chunk padded with zeros.
pub(crate) fn glb(root: &Value, mut bin: Vec<u8>) -> Vec<u8> {
    let mut json = serde_json::to_vec(root).expect("JSON in memory");
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let length = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(length);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(length as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}
