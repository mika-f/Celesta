//! A VRM avatar as a Celesta scene: its geometries, materials, skins, and
//! draws, fixed when it opens, and each frame's pose, skinning matrices,
//! and draw states.
//!
//! - **Draws** go node by node in pre-order over the default scene, then
//!   primitive by primitive. Each (mesh, primitive) has one geometry, shared
//!   by every node drawing the mesh with the same skin (or none): a mesh
//!   drawn skinned by one node and unskinned by another gets a geometry
//!   for each, since only one of them has joints. A primitive that draws
//!   no triangles is skipped.
//! - **Skins:** one per glTF skin. A frame's skinning matrix for joint `j`
//!   is the joint's world times its inverse bind. A skinned draw's `world`
//!   is the identity (glTF ignores the mesh node's transform); an
//!   unskinned draw's is its node's world. A skinned node's primitive
//!   without `JOINTS_0` and `WEIGHTS_0` is not skinned, so its node places
//!   it like any unskinned draw.
//! - **`center`:** Unity sorts a renderer by its bounds' centre, and
//!   UniGLTF makes the skin's `skeleton` the renderer's root bone, whose
//!   space Unity's skinned bounds are in. So every primitive of a node's
//!   mesh is sorted by one point: the centre of the mesh's drawn
//!   primitives' bounds (mesh space), moved by the current world of the
//!   node's skin's skeleton, else of the node. Ties keep draw order.
//! - **Materials:** the file's, by index, then glTF's default material
//!   (white, opaque, one-sided; metallic-roughness, so drawn unlit like
//!   other PBR materials) once, when a primitive has no material.
//! - **Expressions** (see `expressions`) are checked after the draws and
//!   before the materials, so a model refused for them writes no texture.
//!   Each frame they set the morph targets they bind, on every draw of the
//!   bound node, and the bound materials' colours and texture transforms
//!   on every draw using them: an MToon draw's as parameters, packed with
//!   the frame's UV animation (sent only when they differ from the
//!   material's own); an unlit draw's base colour and texture transform as
//!   overrides, its other colours dropped.
//! - **The lookAt** (see `look_at`) is read before the geometry. Each frame
//!   it turns the eye bones, or writes the look expressions, for the gaze
//!   `data.look_at` asks for, scaled by what other expressions leave of it.
//! - **Budgets:** the model's geometry, draws, and skins together take at
//!   most [`GEOMETRY_BUDGET`] bytes, and its accessors decode at most
//!   `READ_BUDGET` (see `geometry`).
//!
//! Each frame runs UniVRM's order (`Vrm10Runtime.Process`): the base pose,
//! constraints, the lookAt, expressions, spring bones, then world matrices.

use crate::document::{Document, LoadError};
use crate::expressions::{Applied, ExpressionKey, Expressions, read_expressions};
use crate::first_person::hidden_nodes;
use crate::geometry::{
    Accessors, GEOMETRY_BUDGET, draws_triangles, geometry_bytes, inverse_binds, primitive_geometry,
};
use crate::humanoid::Humanoid;
use crate::look_at::LookAt;
use crate::materials::{materials, texture_cache_dir};
use crate::mtoon::{self, Mtoon, MtoonOverrides};
use crate::nodes::{NodeTree, Pose};
use crate::provider::{LookAtData, LookAtKeyword, PerformanceData};
use celesta_plugin::{
    AlphaMode, Camera, DrawInfo, DrawState, Geometry, Lights, Material, MaterialKind,
    MaterialOverrides, Notice, NoticeLevel, Projection, SceneFrame, SceneModel, Skin,
    UnlitMaterial,
};
use glam::{Mat4, Vec3};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;
use std::path::Path;

/// The canvas a model opens with, in pixels.
pub const CANVAS: [f32; 2] = [1920.0, 1080.0];

/// Where the camera stands until it frames the avatar (Task 7): 3.3 m in
/// front of it, 0.8 m up, looking down −Z.
const REST_EYE: Vec3 = Vec3::new(0.0, 0.8, 3.3);

/// The camera presets' lens: 30° vertically.
const PROJECTION: Projection = Projection::Perspective {
    yfov: std::f32::consts::FRAC_PI_6,
    near: 0.01,
    far: 100.0,
};

/// Why a frame could not be made.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct FrameError(pub String);

/// Where a draw comes from, and what each frame needs of it.
pub(crate) struct DrawSource {
    pub node: usize,
    pub mesh: usize,
    pub primitive: usize,
    /// The skin its geometry is skinned by: its node's, when the primitive
    /// has joints.
    pub skin: Option<usize>,
    /// Whether a third-person camera draws it (see `first_person`).
    pub visible: bool,
    /// The node whose world moves its `center`: its node's skin's
    /// skeleton, else its node.
    pub anchor: usize,
    /// Its morph weights with no expression: its node's `weights`, else its
    /// mesh's, else zeros, one per target.
    pub morph_weights: Vec<f32>,
}

/// A glTF skin: its joints' nodes and inverse bind matrices.
struct SkinSource {
    joints: Vec<usize>,
    inverse_binds: Vec<Mat4>,
}

/// An opened VRM avatar.
pub struct VrmModel {
    /// The file as read, with its buffers dropped once the scene is made.
    // Expressions, the lookAt, and the motion plan read its extensions.
    #[allow(dead_code)]
    document: Document,
    tree: NodeTree,
    humanoid: Humanoid,
    scene: SceneModel,
    /// By material: an MToon material's parameters, which each frame packs
    /// with its binds and UV animation.
    mtoon: Vec<Option<Mtoon>>,
    notices: Vec<Notice>,
    draws: Vec<DrawSource>,
    skins: Vec<SkinSource>,
    /// By mesh: the centre of its drawn primitives' bounds, in mesh space.
    centres: Vec<Vec3>,
    expressions: Expressions,
    look_at: LookAt,
    /// The bytes of the geometry budget the model takes.
    #[cfg_attr(not(test), allow(dead_code))]
    bytes: u64,
}

/// What is left of a model's geometry budget.
struct Budget {
    left: u64,
    total: u64,
}

impl Budget {
    fn take(&mut self, bytes: u64, what: impl FnOnce() -> String) -> Result<(), LoadError> {
        if bytes > self.left {
            return Err(LoadError::Invalid(format!(
                "{} takes {bytes} bytes, but the model has {} left of its {} MiB",
                what(),
                self.left,
                self.total >> 20
            )));
        }
        self.left -= bytes;
        Ok(())
    }
}

/// What the scene's draws are, before the materials are known.
struct Draws {
    sources: Vec<DrawSource>,
    infos: Vec<DrawInfo>,
    geometries: Vec<Geometry>,
    centres: Vec<Vec3>,
    /// A primitive has no material, so glTF's default one is appended.
    default_material: bool,
}

impl VrmModel {
    /// Opens the `.vrm` at `path`, writing its textures to the user's cache.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let bytes = std::fs::read(path)?;
        Self::load_slice(&bytes, &texture_cache_dir())
    }

    /// Opens a `.vrm` held in memory, writing its textures to `cache`.
    pub fn load_slice(bytes: &[u8], cache: &Path) -> Result<Self, LoadError> {
        Self::load_with(bytes, cache, GEOMETRY_BUDGET)
    }

    /// [`Self::load_slice`] with a geometry budget of `budget` bytes.
    fn load_with(bytes: &[u8], cache: &Path, budget: u64) -> Result<Self, LoadError> {
        let mut document = Document::parse(bytes)?;
        let tree = NodeTree::new(&document)?;
        let humanoid = Humanoid::new(&document);
        let look_at = LookAt::new(&document, &tree, &humanoid)?;
        let hidden = hidden_nodes(&document)?;
        let mut budget = Budget {
            left: budget,
            total: budget,
        };

        // The geometry first, so a model refused for it writes no texture.
        let (skins, draws) = {
            let accessors = Accessors::new(&document);
            let skins = read_skins(&document, &accessors, &mut budget)?;
            let draws = read_draws(&document, &accessors, &tree, &hidden, &skins, &mut budget)?;
            (skins, draws)
        };
        // Expressions are checked before the materials write any texture.
        let mut expression_notices = Vec::new();
        let expression_binds =
            read_expressions(&document, &draws.sources, &mut expression_notices)?;
        let read = materials(&document, cache)?;
        let mut scene_materials = read.materials;
        let mut mtoon = read.mtoon;
        if draws.default_material {
            mtoon.push(None);
            scene_materials.push(Material {
                alpha_mode: AlphaMode::Opaque,
                double_sided: false,
                kind: MaterialKind::Unlit(UnlitMaterial {
                    base_color: [1.0; 4],
                    base_color_texture: None,
                }),
            });
        }

        let mut notices = read.notices;
        let drawn: BTreeSet<usize> = draws.infos.iter().map(|info| info.material).collect();
        let pbr: Vec<&str> = read
            .pbr
            .iter()
            .filter(|(material, _)| drawn.contains(material))
            .map(|(_, name)| name.as_str())
            .collect();
        if !pbr.is_empty() {
            notices.push(Notice {
                level: NoticeLevel::Info,
                message: format!(
                    "{} glTF PBR material(s) are drawn as unlit base colour: {}",
                    pbr.len(),
                    pbr.join(", ")
                ),
            });
        }
        notices.extend(expression_notices);

        let expressions = Expressions::new(
            expression_binds,
            &scene_materials,
            &mtoon,
            &read.base_transforms,
        );
        let scene = SceneModel {
            canvas: CANVAS,
            textures: read.textures,
            geometries: draws.geometries,
            skins: skins
                .iter()
                .map(|skin| Skin {
                    joints: skin.joints.len() as u32,
                })
                .collect(),
            shaders: read.shaders,
            materials: scene_materials,
            draws: draws.infos,
        };
        // Nothing reads the buffers once the scene is made.
        document.buffers = Vec::new();
        Ok(Self {
            document,
            tree,
            humanoid,
            scene,
            mtoon,
            notices,
            draws: draws.sources,
            skins,
            centres: draws.centres,
            expressions,
            look_at,
            bytes: budget.total - budget.left,
        })
    }

    pub fn scene_model(&self) -> &SceneModel {
        &self.scene
    }

    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }

    pub fn tree(&self) -> &NodeTree {
        &self.tree
    }

    pub fn humanoid(&self) -> &Humanoid {
        &self.humanoid
    }

    /// The scene at `time` seconds, as `data` asks for it. The same model,
    /// time, and data always give the same frame.
    pub fn frame(&mut self, time: f64, data: &PerformanceData) -> Result<SceneFrame, FrameError> {
        // The camera, and the eye a `"camera"` gaze looks at.
        let camera = Camera {
            view: Mat4::from_translation(-REST_EYE).to_cols_array(),
            projection: PROJECTION,
        };
        let (mut pose, applied) = self.pose_and_weights(time, data, REST_EYE)?;
        let mut morph: Vec<Vec<f32>> = self
            .draws
            .iter()
            .map(|draw| draw.morph_weights.clone())
            .collect();
        let binds = self.binds_of(&applied, &mut morph);
        self.apply_springs(time, data, &mut pose);
        let worlds = self.tree.world(&pose);

        let mut skins = Vec::with_capacity(self.skins.len());
        for (index, skin) in self.skins.iter().enumerate() {
            let mut matrices = Vec::with_capacity(skin.joints.len());
            for (joint, (&node, bind)) in skin.joints.iter().zip(&skin.inverse_binds).enumerate() {
                let matrix = worlds[node] * *bind;
                if !matrix.is_finite() {
                    return Err(FrameError(format!(
                        "skin {index}'s joint {joint} (node {node}) has a skinning matrix that is not finite"
                    )));
                }
                matrices.push(matrix.to_cols_array());
            }
            skins.push(matrices);
        }

        // Bind weights and colours add up, so many large ones can overflow;
        // Celesta sees only the packed bytes, so the plugin checks them.
        for (material, binds) in binds.iter().enumerate() {
            if let Some(bind) = bind_not_finite(binds) {
                return Err(FrameError(format!(
                    "material {material}'s {bind} is not finite"
                )));
            }
        }
        let materials = self.material_states(&binds, time);

        let centres = self.mesh_centers(&worlds);
        let mut draws = Vec::with_capacity(self.draws.len());
        let sources = self.draws.iter().zip(centres).zip(morph);
        for (index, ((draw, centre), morph_weights)) in sources.enumerate() {
            if morph_weights.iter().any(|weight| !weight.is_finite()) {
                return Err(FrameError(format!(
                    "expressions make draw {index}'s morph weights not finite"
                )));
            }
            let world = match draw.skin {
                Some(_) => Mat4::IDENTITY,
                None => worlds[draw.node],
            };
            if !(world.is_finite() && centre.is_finite()) {
                return Err(FrameError(format!(
                    "draw {index} (node {}, mesh {} primitive {}) is placed by a matrix that is not finite",
                    draw.node, draw.mesh, draw.primitive
                )));
            }
            let (overrides, params) = self
                .scene
                .draws
                .get(index)
                .and_then(|info| materials.get(info.material))
                .cloned()
                .unwrap_or_default();
            draws.push(DrawState {
                index,
                visible: draw.visible,
                world: world.to_cols_array(),
                morph_weights,
                center: Some(centre.to_array()),
                overrides,
                params,
            });
        }

        Ok(SceneFrame {
            camera,
            lights: Lights::default(),
            time,
            draws,
            skins,
        })
    }

    /// By material: what the expressions' `applied` weights bind, with the
    /// morph targets they bind written into `morph` (by draw).
    fn binds_of(&self, applied: &Applied, morph: &mut [Vec<f32>]) -> Vec<MtoonOverrides> {
        let mut binds = vec![MtoonOverrides::default(); self.scene.materials.len()];
        self.expressions.apply(&applied.weights, morph, &mut binds);
        binds
    }

    /// By material: what its draws send at `time` with `binds` (by
    /// material). An MToon material sends its parameters packed with them
    /// and the UV animation, when they differ from the material's own; an
    /// unlit one its base colour and texture transform binds as overrides.
    fn material_states(
        &self,
        binds: &[MtoonOverrides],
        time: f64,
    ) -> Vec<(MaterialOverrides, Option<Vec<u8>>)> {
        self.scene
            .materials
            .iter()
            .zip(binds)
            .zip(&self.mtoon)
            .map(|((material, binds), mtoon)| match (&material.kind, mtoon) {
                (MaterialKind::Custom(custom), Some(mtoon)) => {
                    let params = mtoon::params(mtoon, binds, time, &self.scene.textures);
                    let changed = params[..] != custom.params[..];
                    (
                        MaterialOverrides::default(),
                        changed.then(|| params.to_vec()),
                    )
                }
                _ => (
                    MaterialOverrides {
                        base_color: binds.base_color,
                        texture_offset: binds.texture_offset,
                        texture_scale: binds.texture_scale,
                    },
                    None,
                ),
            })
            .collect()
    }

    /// Material `material`'s MToon parameters, if it is MToon.
    #[cfg(test)]
    pub(crate) fn mtoon(&self, material: usize) -> Option<&Mtoon> {
        self.mtoon.get(material).and_then(Option::as_ref)
    }

    /// By material: what `data`'s expressions bind at `time`.
    #[cfg(test)]
    pub(crate) fn binds(
        &self,
        time: f64,
        data: &PerformanceData,
    ) -> Result<Vec<MtoonOverrides>, FrameError> {
        let (_, applied) = self.pose_and_weights(time, data, REST_EYE)?;
        let mut morph: Vec<Vec<f32>> = self
            .draws
            .iter()
            .map(|draw| draw.morph_weights.clone())
            .collect();
        Ok(self.binds_of(&applied, &mut morph))
    }

    /// The frame's stages before the expressions' binds and the springs:
    /// the base pose, constraints, the gaze (at `camera_eye` for a
    /// `"camera"` target), and the expressions' actual weights, with the
    /// lookAt applied to the eyes or the look keys.
    pub(crate) fn pose_and_weights(
        &self,
        time: f64,
        data: &PerformanceData,
        camera_eye: Vec3,
    ) -> Result<(Pose, Applied), FrameError> {
        let mut pose = self.base_pose(time, data);
        self.apply_constraints(&mut pose);
        let (yaw, pitch) = self.gaze(data, &pose, camera_eye)?;
        let inputs = self.expression_inputs(time, data)?;
        let mut applied = self.expressions.validate(&inputs);
        // UniVRM's validator scales the gaze by what other expressions
        // leave of it.
        let scale = 1.0 - applied.look_at_rate;
        self.look_at
            .apply(yaw * scale, pitch * scale, &mut pose, &mut applied.weights);
        Ok((pose, applied))
    }

    /// The gaze's yaw and pitch (degrees) that `data` asks for, with the
    /// head at `pose` and the camera at `camera_eye`.
    fn gaze(
        &self,
        data: &PerformanceData,
        pose: &Pose,
        camera_eye: Vec3,
    ) -> Result<(f32, f32), FrameError> {
        let target = match &data.look_at {
            None => return Ok((0.0, 0.0)),
            Some(LookAtData::Angles { yaw, pitch }) => {
                for (name, value) in [("yaw", yaw), ("pitch", pitch)] {
                    if !value.is_finite() {
                        return Err(FrameError(format!("lookAt {name} is not finite: {value}")));
                    }
                }
                return Ok((*yaw, *pitch));
            }
            Some(LookAtData::Point(point)) => {
                let point = Vec3::from(*point);
                if !point.is_finite() {
                    return Err(FrameError(format!("lookAt point is not finite: {point}")));
                }
                point
            }
            Some(LookAtData::Keyword(LookAtKeyword::Camera)) => camera_eye,
        };
        let origin = self.look_at.origin(&self.tree.world(pose));
        self.look_at.yaw_pitch(origin, target).ok_or_else(|| {
            FrameError(
                "the lookAt origin cannot be inverted (the head is scaled to nothing), \
                 so the eyes cannot look at a point"
                    .into(),
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn look_at(&self) -> &LookAt {
        &self.look_at
    }

    /// The pose before constraints and springs. The rest pose; the motion
    /// plan's motions and tracks replace it.
    fn base_pose(&self, _time: f64, _data: &PerformanceData) -> Pose {
        self.tree.rest_pose()
    }

    /// The motion plan's node constraints; nothing yet.
    fn apply_constraints(&self, _pose: &mut Pose) {}

    /// The expression weights asked for, by key: `data.expressions`; the
    /// motion plan adds its clips'. A weight that is not finite is an
    /// error naming the expression.
    fn expression_inputs(
        &self,
        _time: f64,
        data: &PerformanceData,
    ) -> Result<BTreeMap<ExpressionKey, f32>, FrameError> {
        data.expressions
            .iter()
            .map(|(name, &weight)| {
                if weight.is_finite() {
                    Ok((ExpressionKey::parse(name), weight))
                } else {
                    Err(FrameError(format!("expression {name} is not finite")))
                }
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn expressions(&self) -> &Expressions {
        &self.expressions
    }

    /// The motion plan's spring bones; nothing yet.
    fn apply_springs(&mut self, _time: f64, _data: &PerformanceData, _pose: &mut Pose) {}

    /// By draw: the world-space point it is sorted by, for nodes at
    /// `worlds`. Every draw of one node's mesh has the same.
    pub(crate) fn mesh_centers(&self, worlds: &[Mat4]) -> Vec<Vec3> {
        self.draws
            .iter()
            .map(|draw| worlds[draw.anchor].transform_point3(self.centres[draw.mesh]))
            .collect()
    }
}

/// The first bind of `binds` that is set and not finite, by its VRM name.
fn bind_not_finite(binds: &MtoonOverrides) -> Option<&'static str> {
    let MtoonOverrides {
        base_color,
        texture_offset,
        texture_scale,
        shade_color,
        emission_color,
        matcap_color,
        rim_color,
        outline_color,
    } = binds;
    let named: [(&str, Option<&[f32]>); 8] = [
        ("color", base_color.as_ref().map(|value| &value[..])),
        (
            "texture offset",
            texture_offset.as_ref().map(|value| &value[..]),
        ),
        (
            "texture scale",
            texture_scale.as_ref().map(|value| &value[..]),
        ),
        ("shadeColor", shade_color.as_ref().map(|value| &value[..])),
        (
            "emissionColor",
            emission_color.as_ref().map(|value| &value[..]),
        ),
        ("matcapColor", matcap_color.as_ref().map(|value| &value[..])),
        ("rimColor", rim_color.as_ref().map(|value| &value[..])),
        (
            "outlineColor",
            outline_color.as_ref().map(|value| &value[..]),
        ),
    ];
    named.into_iter().find_map(|(name, values)| {
        values
            .is_some_and(|values| values.iter().any(|value| !value.is_finite()))
            .then_some(name)
    })
}

/// Every glTF skin's joints and inverse bind matrices, by skin index.
fn read_skins(
    document: &Document,
    accessors: &Accessors,
    budget: &mut Budget,
) -> Result<Vec<SkinSource>, LoadError> {
    // Each joint keeps its node and inverse bind, and each frame sends a
    // skinning matrix for it.
    let per_joint = (2 * size_of::<Mat4>() + size_of::<usize>()) as u64;
    document
        .gltf
        .skins()
        .map(|skin| {
            let joints: Vec<usize> = skin.joints().map(|joint| joint.index()).collect();
            // glTF asks for at least one; gltf-json does not check. A vertex
            // whose zero weights name joint 0 would reach Celesta with none.
            if joints.is_empty() {
                return Err(LoadError::Invalid(format!(
                    "skin {} has no joints",
                    skin.index()
                )));
            }
            if u32::try_from(joints.len()).is_err() {
                return Err(LoadError::Invalid(format!(
                    "skin {} has {} joints",
                    skin.index(),
                    joints.len()
                )));
            }
            budget.take(joints.len() as u64 * per_joint, || {
                format!("skin {}", skin.index())
            })?;
            Ok(SkinSource {
                inverse_binds: inverse_binds(accessors, &skin)?,
                joints,
            })
        })
        .collect()
}

/// By node: whether it is in the default scene (else scene 0), which is
/// all a model draws. A file with no scene draws every node.
fn scene_nodes(document: &Document, tree: &NodeTree) -> Vec<bool> {
    let gltf = &document.gltf;
    let count = tree.parents.len();
    let Some(scene) = gltf.default_scene().or_else(|| gltf.scenes().next()) else {
        return vec![true; count];
    };
    let mut in_scene = vec![false; count];
    for root in scene.nodes() {
        if tree.parents[root.index()].is_none() {
            in_scene[root.index()] = true;
        }
    }
    // Parents come before their children.
    for &node in &tree.pre_order {
        if let Some(parent) = tree.parents[node] {
            in_scene[node] = in_scene[parent];
        }
    }
    in_scene
}

/// A mesh's primitive that draws triangles, with what reading it needs.
struct DrawnPrimitive<'d> {
    primitive: gltf::Primitive<'d>,
    material: Option<usize>,
    needs_tangents: bool,
}

/// What a mesh draws, decided once for every node drawing it: the
/// primitives that draw triangles (whether one does depends neither on
/// the node nor on its skin), and the mesh's morph weights.
struct MeshDraws<'d> {
    primitives: Vec<DrawnPrimitive<'d>>,
    weights: Option<&'d [f32]>,
}

fn mesh_draws<'d>(document: &Document, mesh: &gltf::Mesh<'d>) -> Result<MeshDraws<'d>, LoadError> {
    let weights = mesh.weights();
    if let Some(weights) = weights
        && weights.iter().any(|weight| !weight.is_finite())
    {
        return Err(LoadError::Invalid(format!(
            "mesh {} has morph weights that are not finite: {weights:?}",
            mesh.index()
        )));
    }
    let primitives = mesh
        .primitives()
        .filter(draws_triangles)
        .map(|primitive| {
            let material = primitive.material();
            let needs_tangents = material
                .index()
                .is_some_and(|m| document.mtoon.get(m).is_some_and(Option::is_some))
                && material.normal_texture().is_some();
            DrawnPrimitive {
                material: material.index(),
                needs_tangents,
                primitive,
            }
        })
        .collect();
    Ok(MeshDraws {
        primitives,
        weights,
    })
}

/// The scene's draws, their geometries, and each mesh's centre.
///
/// Every node costs its own draws, each counted toward the budget: which
/// primitives draw is decided once per mesh, so a node never walks the
/// primitives that do not.
fn read_draws(
    document: &Document,
    accessors: &Accessors,
    tree: &NodeTree,
    hidden: &[bool],
    skins: &[SkinSource],
    budget: &mut Budget,
) -> Result<Draws, LoadError> {
    let gltf = &document.gltf;
    let nodes: Vec<gltf::Node> = gltf.nodes().collect();
    let in_scene = scene_nodes(document, tree);
    let default_material = gltf.materials().len();
    // By mesh: the bounds of its drawn primitives.
    let mut bounds: Vec<Option<(Vec3, Vec3)>> = vec![None; gltf.meshes().len()];
    // By mesh, once a node draws it.
    let mut meshes: Vec<Option<MeshDraws>> = (0..gltf.meshes().len()).map(|_| None).collect();
    // By (mesh, skin): the geometry of each primitive its `MeshDraws` has.
    let mut read: BTreeMap<(usize, Option<usize>), Vec<Option<usize>>> = BTreeMap::new();
    let mut draws = Draws {
        sources: Vec::new(),
        infos: Vec::new(),
        geometries: Vec::new(),
        centres: Vec::new(),
        default_material: false,
    };
    // Each draw's entry, source, and frame state, with two copies of its
    // morph weights.
    let per_draw =
        (size_of::<DrawInfo>() + size_of::<DrawSource>() + size_of::<DrawState>()) as u64;

    for &index in &tree.pre_order {
        let node = &nodes[index];
        let Some(mesh) = node.mesh().filter(|_| in_scene[index]) else {
            continue;
        };
        if meshes[mesh.index()].is_none() {
            meshes[mesh.index()] = Some(mesh_draws(document, &mesh)?);
        }
        let Some(drawn) = &meshes[mesh.index()] else {
            continue;
        };
        if drawn.primitives.is_empty() {
            continue;
        }
        let skin = node.skin();
        let key = (mesh.index(), skin.as_ref().map(gltf::Skin::index));
        if let Entry::Vacant(entry) = read.entry(key) {
            let mut geometries = Vec::new();
            for drawn in &drawn.primitives {
                let index = drawn.primitive.index();
                let Some(geometry) = primitive_geometry(
                    accessors,
                    mesh.clone(),
                    drawn.primitive.clone(),
                    skin.clone(),
                    drawn.needs_tangents,
                    budget.left,
                )?
                else {
                    // Not reached: `draws_triangles` kept only those that do.
                    geometries.push(None);
                    continue;
                };
                budget.take(geometry_bytes(&geometry), || {
                    format!("mesh {} primitive {index}", mesh.index())
                })?;
                let bound = &mut bounds[mesh.index()];
                for &position in &geometry.positions {
                    let position = Vec3::from(position);
                    *bound = Some(match *bound {
                        Some((min, max)) => (min.min(position), max.max(position)),
                        None => (position, position),
                    });
                }
                geometries.push(Some(draws.geometries.len()));
                draws.geometries.push(geometry);
            }
            entry.insert(geometries);
        }

        // The node's own weights (its own JSON, so checked per node), else
        // the mesh's (checked once, in `mesh_draws`).
        let node_weights = node.weights();
        if let Some(weights) = node_weights
            && weights.iter().any(|weight| !weight.is_finite())
        {
            return Err(LoadError::Invalid(format!(
                "node {index} has morph weights that are not finite: {weights:?}"
            )));
        }
        let defaults = node_weights.or(drawn.weights);
        let anchor = skin
            .as_ref()
            .and_then(gltf::Skin::skeleton)
            .map_or(index, |skeleton| skeleton.index());
        for (primitive, geometry) in drawn.primitives.iter().zip(&read[&key]) {
            let Some(geometry) = *geometry else {
                continue;
            };
            let targets = draws.geometries[geometry].targets.len();
            budget.take(per_draw + 2 * 4 * targets as u64, || {
                format!(
                    "node {index}'s draw of primitive {}",
                    primitive.primitive.index()
                )
            })?;
            let morph_weights: Vec<f32> = (0..targets)
                .map(|target| {
                    defaults
                        .and_then(|weights| weights.get(target))
                        .copied()
                        .unwrap_or(0.0)
                })
                .collect();
            let draw_skin = skin
                .as_ref()
                .filter(|_| draws.geometries[geometry].joints.is_some())
                .map(gltf::Skin::index);
            debug_assert!(draw_skin.is_none_or(|skin| skin < skins.len()));
            let material = primitive.material.unwrap_or_else(|| {
                draws.default_material = true;
                default_material
            });
            draws.sources.push(DrawSource {
                node: index,
                mesh: mesh.index(),
                primitive: primitive.primitive.index(),
                skin: draw_skin,
                visible: !hidden[index],
                anchor,
                morph_weights,
            });
            draws.infos.push(DrawInfo {
                geometry,
                material,
                skin: draw_skin,
            });
        }
    }

    draws.centres = bounds
        .into_iter()
        .map(|bound| bound.map_or(Vec3::ZERO, |(min, max)| (min + max) / 2.0))
        .collect();
    Ok(draws)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticVrm, node};
    use celesta_plugin::{AlphaMode, MaterialKind, NoticeLevel, UnlitMaterial};
    use serde_json::{Value, json};
    use tempfile::TempDir;

    fn load_glb(glb: &[u8]) -> Result<VrmModel, LoadError> {
        let cache = TempDir::new().unwrap();
        VrmModel::load_slice(glb, cache.path())
    }

    fn avatar() -> VrmModel {
        load_glb(&SyntheticVrm::avatar().glb()).unwrap()
    }

    fn edited(edit: impl FnOnce(&mut Value)) -> VrmModel {
        load_glb(&SyntheticVrm::avatar().glb_with(edit)).unwrap()
    }

    fn refusal(edit: impl FnOnce(&mut Value)) -> String {
        match load_glb(&SyntheticVrm::avatar().glb_with(edit)) {
            Ok(_) => panic!("the model opened"),
            Err(error) => error.to_string(),
        }
    }

    fn rest(model: &mut VrmModel) -> SceneFrame {
        model.frame(0.0, &PerformanceData::default()).unwrap()
    }

    fn near(actual: Vec3, expected: Vec3) -> bool {
        (actual - expected).abs().max_element() < 1e-5
    }

    fn translation(matrix: &[f32; 16]) -> Vec3 {
        Vec3::new(matrix[12], matrix[13], matrix[14])
    }

    fn is_identity(matrix: &[f32; 16]) -> bool {
        Mat4::from_cols_array(matrix).abs_diff_eq(Mat4::IDENTITY, 1e-5)
    }

    /// Each draw as (node, primitive, skin).
    fn sources(model: &VrmModel) -> Vec<(usize, usize, Option<usize>)> {
        model
            .draws
            .iter()
            .map(|draw| (draw.node, draw.primitive, draw.skin))
            .collect()
    }

    #[test]
    fn draws_follow_node_and_primitive_order() {
        let model = avatar();
        assert_eq!(
            sources(&model),
            [
                (node::BODY, 0, Some(0)),
                (node::BODY, 1, Some(0)),
                (node::FIRST_PERSON_ONLY, 0, None),
            ]
        );
        let scene = model.scene_model();
        let geometries: Vec<usize> = scene.draws.iter().map(|draw| draw.geometry).collect();
        assert_eq!(geometries, [0, 1, 2]);
        assert_eq!(scene.geometries.len(), 3);
        let materials: Vec<usize> = scene.draws.iter().map(|draw| draw.material).collect();
        assert_eq!(materials, [0, 1, 2]);
        let skins: Vec<Option<usize>> = scene.draws.iter().map(|draw| draw.skin).collect();
        assert_eq!(skins, [Some(0), Some(0), None]);
        assert_eq!(scene.skins, [Skin { joints: 3 }]);
        assert_eq!(scene.canvas, CANVAS);
        assert_eq!(scene.materials.len(), 4);
    }

    #[test]
    fn rest_frame_skins_to_the_identity() {
        let mut model = avatar();
        let frame = rest(&mut model);
        assert_eq!(frame.skins.len(), 1);
        assert_eq!(frame.skins[0].len(), 3);
        for matrix in &frame.skins[0] {
            assert!(is_identity(matrix), "{matrix:?}");
        }
        assert!(is_identity(&frame.draws[0].world));
        assert!(is_identity(&frame.draws[1].world));
        let quad = Mat4::from_cols_array(&frame.draws[2].world);
        assert!(quad.abs_diff_eq(Mat4::from_translation(Vec3::new(0.0, 1.45, 0.1)), 1e-5));
        let indices: Vec<usize> = frame.draws.iter().map(|draw| draw.index).collect();
        assert_eq!(indices, [0, 1, 2]);
        assert_eq!(frame.time, 0.0);
        assert_eq!(frame.lights, Lights::default());
        assert!(
            Mat4::from_cols_array(&frame.camera.view)
                .abs_diff_eq(Mat4::from_translation(Vec3::new(0.0, -0.8, -3.3)), 1e-6)
        );
        assert_eq!(
            frame.camera.projection,
            Projection::Perspective {
                yfov: 30f32.to_radians(),
                near: 0.01,
                far: 100.0
            }
        );
        for draw in &frame.draws {
            assert_eq!(draw.overrides, MaterialOverrides::default());
            assert_eq!(draw.params, None);
        }
    }

    #[test]
    fn the_scene_brings_the_mtoon_shader() {
        let model = avatar();
        let scene = model.scene_model();
        assert_eq!(scene.shaders, [mtoon::shader()]);
        for material in [0, 1] {
            assert!(matches!(
                &scene.materials[material].kind,
                MaterialKind::Custom(custom) if custom.shader == 0
            ));
        }
    }

    #[test]
    fn uv_animation_sends_parameters_as_time_moves() {
        // Body scrolls half a texture a second; the hair does not move.
        let mut model = edited(|root| {
            root["materials"][0]["extensions"]["VRMC_materials_mtoon"]["uvAnimationScrollXSpeedFactor"] =
                json!(0.5);
        });
        let at = |model: &mut VrmModel, time: f64| {
            model.frame(time, &PerformanceData::default()).unwrap()
        };
        // At 0 the material's own parameters are the frame's.
        assert_eq!(at(&mut model, 0.0).draws[0].params, None);
        let frame = at(&mut model, 1.0);
        let params = frame.draws[0].params.as_deref().unwrap();
        let scroll = f32::from_le_bytes(params[160..164].try_into().unwrap());
        assert_eq!(scroll, 0.5);
        assert_eq!(frame.draws[1].params, None);
        assert_eq!(frame.draws[2].params, None);
    }

    #[test]
    fn binds_not_finite_are_named() {
        assert_eq!(bind_not_finite(&MtoonOverrides::default()), None);
        type Set = fn(&mut MtoonOverrides);
        let cases: [(&str, Set); 8] = [
            ("color", |o| o.base_color = Some([0.0, 0.0, 0.0, f32::NAN])),
            ("texture offset", |o| {
                o.texture_offset = Some([f32::INFINITY, 0.0]);
            }),
            ("texture scale", |o| o.texture_scale = Some([0.0, f32::NAN])),
            ("emissionColor", |o| {
                o.emission_color = Some([f32::NAN, 0.0, 0.0]);
            }),
            ("shadeColor", |o| {
                o.shade_color = Some([0.0, f32::INFINITY, 0.0]);
            }),
            ("matcapColor", |o| {
                o.matcap_color = Some([0.0, 0.0, f32::NEG_INFINITY]);
            }),
            ("rimColor", |o| o.rim_color = Some([f32::NAN; 3])),
            ("outlineColor", |o| {
                o.outline_color = Some([f32::NAN, 1.0, 1.0]);
            }),
        ];
        for (name, set) in cases {
            let mut binds = MtoonOverrides::default();
            set(&mut binds);
            assert_eq!(bind_not_finite(&binds), Some(name));
        }
    }

    #[test]
    fn one_center_per_mesh_following_its_skeleton() {
        let mut model = avatar();
        let frame = rest(&mut model);
        let center = |draw: usize| Vec3::from(frame.draws[draw].center.unwrap());
        // The hips, (0, 0.9, 0), times Body's centre, (0, 0.8, 0).
        assert!(near(center(0), Vec3::new(0.0, 1.7, 0.0)), "{}", center(0));
        assert_eq!(center(0), center(1));
        // The quad's node, times the quad's centre.
        assert!(near(center(2), Vec3::new(0.0, 1.45, 0.1)), "{}", center(2));

        let mut pose = model.tree().rest_pose();
        pose.local[node::HIPS].translation = Vec3::new(1.0, 0.9, 0.0);
        let centers = model.mesh_centers(&model.tree().world(&pose));
        assert!(near(centers[0], Vec3::new(1.0, 1.7, 0.0)), "{}", centers[0]);
        assert_eq!(centers[0], centers[1]);
    }

    #[test]
    fn first_person_only_meshes_are_hidden() {
        let mut model = avatar();
        let frame = rest(&mut model);
        let visible: Vec<bool> = frame.draws.iter().map(|draw| draw.visible).collect();
        assert_eq!(visible, [true, true, false]);

        for annotation in ["thirdPersonOnly", "both", "auto"] {
            let mut model = edited(|root| {
                root["extensions"]["VRMC_vrm"]["firstPerson"]["meshAnnotations"][0]["type"] =
                    json!(annotation);
            });
            assert!(rest(&mut model).draws[2].visible, "{annotation}");
        }
        // The last annotation of a node is the one that holds.
        let mut model = edited(|root| {
            root["extensions"]["VRMC_vrm"]["firstPerson"]["meshAnnotations"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "node": node::FIRST_PERSON_ONLY, "type": "both" }));
        });
        assert!(rest(&mut model).draws[2].visible);
    }

    #[test]
    fn first_person_annotations_name_nodes_of_the_file() {
        let message = refusal(|root| {
            root["extensions"]["VRMC_vrm"]["firstPerson"]["meshAnnotations"][0]["node"] = json!(99);
        });
        assert!(
            message.contains("firstPerson annotation 0 names node 99, but the file has 25 nodes"),
            "{message}"
        );
    }

    #[test]
    fn morph_weights_default_to_the_mesh_weights() {
        let mut model = avatar();
        let frame = rest(&mut model);
        assert_eq!(frame.draws[0].morph_weights, [0.0, 0.25, 0.0]);
        assert_eq!(frame.draws[1].morph_weights, [0.0, 0.25, 0.0]);
        assert!(frame.draws[2].morph_weights.is_empty());
    }

    #[test]
    fn node_weights_win_over_the_mesh_weights() {
        // The defaults, before expressions bind targets 0 and 2 (see
        // `expressions`); target 1 is unbound, so the frame keeps it.
        let mut model =
            edited(|root| root["nodes"][node::BODY]["weights"] = json!([1.0, 0.5, 0.5]));
        assert_eq!(model.draws[0].morph_weights, [1.0, 0.5, 0.5]);
        assert_eq!(rest(&mut model).draws[0].morph_weights[1], 0.5);
        // Neither: zeros. Too few or too many: the rest are 0, or dropped.
        let model = edited(|root| {
            root["meshes"][0].as_object_mut().unwrap().remove("weights");
        });
        assert_eq!(model.draws[0].morph_weights, [0.0; 3]);
        let model = edited(|root| root["nodes"][node::BODY]["weights"] = json!([1.0]));
        assert_eq!(model.draws[0].morph_weights, [1.0, 0.0, 0.0]);
        let message = refusal(|root| {
            root["nodes"][node::BODY]["weights"] = json!([0.0, 1e39, 0.0]);
        });
        assert!(
            message.contains("node 23 has morph weights that are not finite"),
            "{message}"
        );
    }

    #[test]
    fn frames_repeat_exactly() {
        let mut model = avatar();
        let data = PerformanceData::default();
        let first = model.frame(1.5, &data).unwrap();
        let second = model.frame(1.5, &data).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.time, 1.5);
    }

    #[test]
    fn a_skinned_node_s_primitive_without_joints_is_placed_by_its_node() {
        let mut model = edited(|root| {
            root["nodes"][node::BODY]["translation"] = json!([0.0, 0.0, 1.0]);
            let hair = root["meshes"][0]["primitives"][1]["attributes"]
                .as_object_mut()
                .unwrap();
            hair.remove("JOINTS_0");
            hair.remove("WEIGHTS_0");
        });
        assert_eq!(model.scene_model().draws[1].skin, None);
        assert_eq!(model.scene_model().geometries[1].joints, None);
        let frame = rest(&mut model);
        assert!(is_identity(&frame.draws[0].world));
        assert!(near(
            translation(&frame.draws[1].world),
            Vec3::new(0.0, 0.0, 1.0)
        ));
        // Both still share the mesh's centre, by the skin's skeleton.
        assert_eq!(frame.draws[0].center, frame.draws[1].center);
    }

    #[test]
    fn a_mesh_drawn_with_and_without_a_skin_gets_a_geometry_each() {
        let model = edited(|root| {
            root["nodes"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "name": "Statue", "mesh": 0, "translation": [2.0, 0.0, 0.0] }));
            root["nodes"][node::ROOT]["children"]
                .as_array_mut()
                .unwrap()
                .push(json!(25));
        });
        assert_eq!(
            sources(&model),
            [
                (node::BODY, 0, Some(0)),
                (node::BODY, 1, Some(0)),
                (node::FIRST_PERSON_ONLY, 0, None),
                (25, 0, None),
                (25, 1, None),
            ]
        );
        let scene = model.scene_model();
        let geometries: Vec<usize> = scene.draws.iter().map(|draw| draw.geometry).collect();
        assert_eq!(geometries, [0, 1, 2, 3, 4]);
        assert!(scene.geometries[0].joints.is_some());
        assert_eq!(scene.geometries[3].joints, None);
        assert_eq!(scene.geometries[3].positions, scene.geometries[0].positions);
    }

    #[test]
    fn instances_of_a_mesh_share_its_geometries() {
        let model = edited(|root| {
            root["nodes"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "name": "Twin", "mesh": 1, "translation": [0.5, 1.45, 0.1] }));
            root["nodes"][node::ROOT]["children"]
                .as_array_mut()
                .unwrap()
                .push(json!(25));
        });
        let scene = model.scene_model();
        let geometries: Vec<usize> = scene.draws.iter().map(|draw| draw.geometry).collect();
        assert_eq!(geometries, [0, 1, 2, 2]);
        assert_eq!(scene.geometries.len(), 3);
        // Only node 24 is annotated.
        assert!(!model.draws[2].visible);
        assert!(model.draws[3].visible);
    }

    #[test]
    fn primitives_without_a_material_draw_unlit_white() {
        let model = edited(|root| {
            for (mesh, primitive) in [(0, 1), (1, 0)] {
                root["meshes"][mesh]["primitives"][primitive]
                    .as_object_mut()
                    .unwrap()
                    .remove("material");
            }
        });
        let scene = model.scene_model();
        // Appended once, after the file's four.
        assert_eq!(scene.materials.len(), 5);
        let materials: Vec<usize> = scene.draws.iter().map(|draw| draw.material).collect();
        assert_eq!(materials, [0, 4, 4]);
        let default = &scene.materials[4];
        assert_eq!(default.alpha_mode, AlphaMode::Opaque);
        assert!(!default.double_sided);
        assert_eq!(
            default.kind,
            MaterialKind::Unlit(UnlitMaterial {
                base_color: [1.0; 4],
                base_color_texture: None,
            })
        );
        // Without a material none is appended.
        assert_eq!(avatar().scene_model().materials.len(), 4);
    }

    fn pbr_notices(model: &VrmModel) -> Vec<&str> {
        model
            .notices()
            .iter()
            .filter(|notice| notice.message.contains("PBR"))
            .map(|notice| {
                assert_eq!(notice.level, NoticeLevel::Info);
                notice.message.as_str()
            })
            .collect()
    }

    #[test]
    fn the_pbr_notice_names_only_drawn_materials() {
        // Pbr is drawn (by the hidden quad); Spare is not.
        let model = edited(|root| {
            root["materials"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "name": "Spare" }));
        });
        assert_eq!(
            pbr_notices(&model),
            ["1 glTF PBR material(s) are drawn as unlit base colour: Pbr"]
        );
        let model = edited(|root| root["meshes"][1]["primitives"][0]["material"] = json!(3));
        assert!(pbr_notices(&model).is_empty());
    }

    #[test]
    fn nodes_outside_the_scene_are_not_drawn() {
        let model = edited(|root| {
            root["nodes"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "name": "Loose", "mesh": 1 }));
        });
        assert_eq!(model.draws.len(), 3);
        assert_eq!(model.scene_model().geometries.len(), 3);
    }

    #[test]
    fn primitives_that_draw_no_triangles_are_skipped() {
        let model = edited(|root| root["meshes"][0]["primitives"][1]["mode"] = json!(1));
        assert_eq!(
            sources(&model),
            [(node::BODY, 0, Some(0)), (node::FIRST_PERSON_ONLY, 0, None)]
        );
        let geometries: Vec<usize> = model
            .scene_model()
            .draws
            .iter()
            .map(|draw| draw.geometry)
            .collect();
        assert_eq!(geometries, [0, 1]);
    }

    /// The avatar with `copies` copies of Body primitive 0 after its two.
    fn copied_box(copies: usize) -> Vec<u8> {
        SyntheticVrm::avatar().glb_with(|root| {
            let primitives = root["meshes"][0]["primitives"].as_array_mut().unwrap();
            let first = primitives[0].clone();
            primitives.extend(std::iter::repeat_n(first, copies));
        })
    }

    #[test]
    fn the_geometry_budget_is_shared_by_every_primitive() {
        let glb = copied_box(50);
        let cache = TempDir::new().unwrap();
        let model = VrmModel::load_with(&glb, cache.path(), GEOMETRY_BUDGET).unwrap();
        assert_eq!(model.draws.len(), 53);
        // Each copy's geometry counts, though they share their accessors.
        let one = geometry_bytes(&model.scene_model().geometries[0]);
        assert!(model.bytes > 50 * one, "{} for {one} each", model.bytes);
        VrmModel::load_with(&glb, cache.path(), model.bytes).unwrap();
        let message = match VrmModel::load_with(&glb, cache.path(), model.bytes - 1) {
            Ok(_) => panic!("the model opened"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("bytes"), "{message}");
        assert!(message.contains("left"), "{message}");
    }

    #[test]
    fn many_nodes_over_many_skipped_primitives_cost_nothing_per_pair() {
        // Mesh 1 as 10,000 point primitives, drawn by 10,000 nodes with a
        // skin each: decided once per mesh, not per node and primitive.
        const COUNT: usize = 10_000;
        let model = edited(|root| {
            let point = json!({
                "attributes": { "POSITION": root["meshes"][1]["primitives"][0]["attributes"]["POSITION"] },
                "mode": 0,
            });
            root["meshes"][1]["primitives"] = json!(vec![point; COUNT]);
            let first = root["nodes"].as_array().unwrap().len();
            let skins = root["skins"].as_array_mut().unwrap();
            let first_skin = skins.len();
            skins.extend((0..COUNT).map(|_| json!({ "joints": [node::HIPS] })));
            let nodes = root["nodes"].as_array_mut().unwrap();
            nodes.extend((0..COUNT).map(|n| json!({ "mesh": 1, "skin": first_skin + n })));
            let children = root["nodes"][node::ROOT]["children"]
                .as_array_mut()
                .unwrap();
            children.extend((first..first + COUNT).map(|n| json!(n)));
        });
        assert_eq!(
            sources(&model),
            [(node::BODY, 0, Some(0)), (node::BODY, 1, Some(0))]
        );
        assert_eq!(model.scene_model().skins.len(), 1 + COUNT);
    }

    /// The avatar with node 25, drawing Body (mesh 0) with skin 1, whose
    /// joints are `joints`.
    fn second_skin(joints: Value) -> Vec<u8> {
        SyntheticVrm::avatar().glb_with(|root| {
            root["skins"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "joints": joints }));
            root["nodes"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "name": "Other", "mesh": 0, "skin": 1 }));
            root["nodes"][node::ROOT]["children"]
                .as_array_mut()
                .unwrap()
                .push(json!(25));
        })
    }

    #[test]
    fn a_mesh_drawn_with_two_skins_gets_a_geometry_for_each() {
        let model = load_glb(&second_skin(json!([node::HIPS, node::HEAD]))).unwrap();
        assert_eq!(
            sources(&model),
            [
                (node::BODY, 0, Some(0)),
                (node::BODY, 1, Some(0)),
                (node::FIRST_PERSON_ONLY, 0, None),
                (25, 0, Some(1)),
                (25, 1, Some(1)),
            ]
        );
        let scene = model.scene_model();
        let geometries: Vec<usize> = scene.draws.iter().map(|draw| draw.geometry).collect();
        assert_eq!(geometries, [0, 1, 2, 3, 4]);
        assert_eq!(scene.skins, [Skin { joints: 3 }, Skin { joints: 2 }]);
        // Each skin's joints are checked on its own: the hair names joint
        // 1, which a one-joint skin lacks.
        let message = match load_glb(&second_skin(json!([node::HIPS]))) {
            Ok(_) => panic!("the model opened"),
            Err(error) => error.to_string(),
        };
        assert!(
            message.contains("mesh \"Body\" primitive 1 names joint 1 of skin 1, which has 1"),
            "{message}"
        );
    }

    #[test]
    fn a_skin_without_joints_is_refused() {
        let message = refusal(|root| {
            root["skins"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "joints": [] }));
        });
        assert!(message.contains("skin 1 has no joints"), "{message}");
    }

    #[test]
    fn skins_without_inverse_binds_skin_by_the_joints() {
        let mut model = edited(|root| {
            root["skins"][0]
                .as_object_mut()
                .unwrap()
                .remove("inverseBindMatrices");
        });
        let frame = rest(&mut model);
        let head = translation(&frame.skins[0][1]);
        assert!(near(head, Vec3::new(0.0, 1.4, 0.0)), "{head}");
    }

    #[test]
    fn broken_inverse_binds_are_refused() {
        let message = refusal(|root| {
            let binds = root["skins"][0]["inverseBindMatrices"].as_u64().unwrap() as usize;
            root["accessors"][binds]["count"] = json!(2);
        });
        assert!(
            message.contains("skin 0 has 2 inverseBindMatrices for 3 joints"),
            "{message}"
        );
        let message = refusal(|root| {
            let binds = root["skins"][0]["inverseBindMatrices"].as_u64().unwrap() as usize;
            root["accessors"][binds]["type"] = json!("VEC4");
        });
        assert!(
            message.contains("skin 0 has inverseBindMatrices of Vec4 F32"),
            "{message}"
        );
        // An infinite matrix, from a buffer of its own.
        use base64::Engine as _;
        let mut bytes = Vec::new();
        for value in [f32::INFINITY; 48] {
            bytes.extend(value.to_le_bytes());
        }
        let uri = format!(
            "data:application/octet-stream;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        let message = refusal(|root| {
            root["buffers"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "byteLength": bytes.len(), "uri": uri }));
            let views = root["bufferViews"].as_array_mut().unwrap();
            views.push(json!({ "buffer": 1, "byteLength": bytes.len() }));
            let view = views.len() - 1;
            let binds = root["skins"][0]["inverseBindMatrices"].as_u64().unwrap() as usize;
            root["accessors"][binds]["bufferView"] = json!(view);
        });
        assert!(
            message.contains("skin 0 has an inverse bind matrix that is not finite"),
            "{message}"
        );
    }

    #[test]
    fn worlds_past_f32_are_a_frame_error() {
        let mut model = edited(|root| {
            root["nodes"][node::HIPS]["scale"] = json!([1e20, 1e20, 1e20]);
            root["nodes"][node::SPINE]["scale"] = json!([1e20, 1e20, 1e20]);
        });
        let message = match model.frame(0.0, &PerformanceData::default()) {
            Ok(_) => panic!("the frame was made"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("not finite"), "{message}");
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let missing = TempDir::new().unwrap().path().join("missing.vrm");
        assert!(matches!(VrmModel::load(&missing), Err(LoadError::Io(_))));
    }

    #[test]
    fn humanoid_and_tree_are_the_avatar_s() {
        let model = avatar();
        assert_eq!(
            model.humanoid().node(crate::HumanBone::Hips),
            Some(node::HIPS)
        );
        assert_eq!(model.tree().pre_order.len(), 25);
    }
}
