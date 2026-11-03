// Ported from UniVRM (https://github.com/vrm-c/UniVRM), under
// Packages/UniGLTF/Runtime/: SpringBoneJobs/UpdateFastSpringBoneJob.cs,
// SpringBoneJobs/SpringBoneCollision.cs,
// SpringBoneJobs/InputPorts/FastSpringBoneBuffer.cs (LogicFromTransform),
// SpringBoneJobs/FastSpringBoneConbinedBuffer.cs (InitCurrentTailsJob and the
// tail swap), SpringBoneJobs/Blittables/BlittableTransform.cs, and
// Utils/MathHelper.cs; and under Packages/VRM10/Runtime/:
// Components/Vrm10Runtime/Springbone/FastSpringBoneBufferFactory.cs and
// IO/Vrm10Importer.cs (LoadSpringBoneAsync).
// MIT License, Copyright (c) 2020 VRM Consortium, Copyright (c) 2018 Masataka
// SUMI for MToon.

//! Spring bones (`VRMC_springBone`), stepped as UniVRM's FastSpringBone
//! steps them: a Verlet step per joint, colliders, and the head turned to
//! its new tail.
//!
//! UniVRM works in Unity's axes, glTF's with X mirrored; every rule here is
//! in glTF's. Its vector helpers (`normalizesafe`, `FromToRotation`, the
//! quaternion inverse) are ported as they are, edge cases included, rather
//! than taken from glam.
//!
//! A step reads the pose as UniVRM's job reads its `Transforms` array: every
//! node's world matrix and world rotation are taken once from the step's
//! pose, and only a joint's head is updated in place once it turns. A head's
//! parent, a spring's centre, and a collider's node are read from there, so
//! they are the turned ones only when they are themselves a head turned
//! earlier in the step. UniVRM then writes each listed node's *world*
//! rotation back to Unity (`PushTransformJob`); this module writes the
//! head's *local* rotation, which is the same wherever a head's parent is
//! the previous joint or a node no spring turns.
//!
//! Not read: `VRMC_springBone_limit` (UniVRM's angle limits) and UniVRM's
//! runtime options (external force, scaling with the model, stopping the
//! write-back), which stay at their defaults.

use crate::document::{Document, LoadError};
use crate::nodes::{NodeTree, Pose};
use glam::{Mat4, Quat, Vec3};
use serde::Deserialize;
use vrm_spec::vrmc_spring_bone_1_0::{Collider, VRMC_SPRING_BONE, VrmcSpringBoneSchema};

/// The collider extension that adds planes and inside shapes.
const EXTENDED_COLLIDER: &str = "VRMC_springBone_extended_collider";

/// The most joint–collider pairs a step checks, over every spring (each
/// spring's joints, its last excepted, times its colliders). A file lists
/// colliders through groups and may list a group many times, so the work is
/// bounded here, at load. Avatars check a few thousand pairs.
pub(crate) const SPRING_WORK_BUDGET: u64 = 1 << 20;

/// A collider's shape in its node's space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ColliderShape {
    Sphere {
        offset: Vec3,
        radius: f32,
    },
    Capsule {
        offset: Vec3,
        radius: f32,
        tail: Vec3,
    },
    Plane {
        offset: Vec3,
        normal: Vec3,
    },
    SphereInside {
        offset: Vec3,
        radius: f32,
    },
    CapsuleInside {
        offset: Vec3,
        radius: f32,
        tail: Vec3,
    },
}

/// One joint that moves: its head node turns so that its tail (the next
/// joint's node) follows the step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct JointLogic {
    pub head: usize,
    pub tail: usize,
    /// The head's parent node.
    pub parent: Option<usize>,
    /// The head's rest local rotation.
    pub local_rotation: Quat,
    /// The tail's direction from the head at rest, in the head's space.
    pub bone_axis: Vec3,
    pub length: f32,
    /// `hitRadius`.
    pub radius: f32,
    pub stiffness: f32,
    pub gravity_power: f32,
    pub gravity_dir: Vec3,
    pub drag: f32,
}

/// A spring: its moving joints in order, its colliders (each with its node)
/// in group then collider order, and the node whose space its tails are
/// kept in.
#[derive(Clone, Debug)]
pub(crate) struct Spring {
    pub joints: Vec<JointLogic>,
    pub colliders: Vec<(usize, ColliderShape)>,
    pub center: Option<usize>,
}

/// Every moving joint's tail, the step before and the last step, each in
/// its spring's centre space (model space without a centre), springs in
/// order and each spring's joints in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SpringState {
    pub prev: Vec<Vec3>,
    pub current: Vec<Vec3>,
}

/// The avatar's spring bones; springs with no joint that moves are left out.
#[derive(Clone, Debug, Default)]
pub(crate) struct Springs {
    pub springs: Vec<Spring>,
}

/// `VRMC_springBone_extended_collider`, read here: `vrm-spec` has no types
/// for it.
#[derive(Deserialize)]
struct ExtendedCollider {
    shape: ExtendedShape,
}

#[derive(Deserialize)]
struct ExtendedShape {
    sphere: Option<ExtendedSphere>,
    capsule: Option<ExtendedCapsule>,
    plane: Option<ExtendedPlane>,
}

#[derive(Deserialize)]
struct ExtendedSphere {
    offset: Option<[f64; 3]>,
    radius: Option<f64>,
    inside: Option<bool>,
}

#[derive(Deserialize)]
struct ExtendedCapsule {
    offset: Option<[f64; 3]>,
    radius: Option<f64>,
    tail: Option<[f64; 3]>,
    inside: Option<bool>,
}

#[derive(Deserialize)]
struct ExtendedPlane {
    offset: Option<[f64; 3]>,
    normal: Option<[f64; 3]>,
}

impl Springs {
    /// The springs of `document`, whose tree is `tree`, within
    /// [`SPRING_WORK_BUDGET`].
    pub(crate) fn read(document: &Document, tree: &NodeTree) -> Result<Self, LoadError> {
        Self::read_with_budget(document, tree, SPRING_WORK_BUDGET)
    }

    /// The springs of `document`, refused when a step would check more
    /// than `budget` joint–collider pairs.
    ///
    /// As UniVRM's importer: a joint, collider, or centre naming a node the
    /// file lacks is refused; a joint on a node an earlier joint already
    /// took is skipped; a spring without joints, and an index to a collider
    /// group or collider the file lacks, are skipped. A number that is not
    /// finite in `f32` is refused.
    pub(crate) fn read_with_budget(
        document: &Document,
        tree: &NodeTree,
        budget: u64,
    ) -> Result<Self, LoadError> {
        let Some(value) = document
            .gltf
            .extension_value(VRMC_SPRING_BONE)
            .filter(|value| !value.is_null())
        else {
            return Ok(Self::default());
        };
        let spec = VrmcSpringBoneSchema::deserialize(value).map_err(|error| {
            LoadError::Invalid(format!("{VRMC_SPRING_BONE} is malformed: {error}"))
        })?;
        let count = tree.parents.len();
        let node = |index: usize, what: &dyn Fn() -> String| {
            if index < count {
                Ok(index)
            } else {
                Err(LoadError::Invalid(format!(
                    "{VRMC_SPRING_BONE} {} names node {index}, but the file has {count} nodes",
                    what()
                )))
            }
        };

        let colliders = spec
            .colliders
            .iter()
            .flatten()
            .enumerate()
            .map(|(index, collider)| {
                let at = node(collider.node.value(), &|| format!("collider {index}"))?;
                Ok((at, collider_shape(index, collider)?))
            })
            .collect::<Result<Vec<_>, LoadError>>()?;
        let groups: Vec<Vec<usize>> = spec
            .collider_groups
            .iter()
            .flatten()
            .map(|group| {
                group
                    .colliders
                    .iter()
                    .copied()
                    .filter(|&collider| collider < colliders.len())
                    .collect()
            })
            .collect();

        let rest = tree.rest_pose();
        let rest_world = tree.world(&rest);
        let rest_rotation = world_rotations(tree, &rest);
        let mut taken = vec![false; count];
        let mut work = 0u64;
        let mut springs = Vec::new();
        for (s, spring) in spec.springs.iter().flatten().enumerate() {
            if spring.joints.is_empty() {
                continue;
            }
            let center = spring
                .center
                .map(|center| node(center.value(), &|| format!("spring {s} center")))
                .transpose()?;
            let mut chain = Vec::with_capacity(spring.joints.len());
            for (j, joint) in spring.joints.iter().enumerate() {
                let at = node(joint.node.value(), &|| format!("spring {s} joint {j}"))?;
                // UniVRM warns of a duplicated spring joint and skips it.
                if std::mem::replace(&mut taken[at], true) {
                    continue;
                }
                let value = |field: &str, value: Option<f64>, default: f32| {
                    let Some(value) = value else {
                        return Ok(default);
                    };
                    let narrow = value as f32;
                    if narrow.is_finite() {
                        Ok(narrow)
                    } else {
                        Err(LoadError::Invalid(format!(
                            "{VRMC_SPRING_BONE} spring {s} joint {j} {field} is not finite: {value}"
                        )))
                    }
                };
                let gravity_dir = match joint.gravity_dir {
                    None => Vec3::NEG_Y,
                    Some(dir) => vector(dir).ok_or_else(|| {
                        LoadError::Invalid(format!(
                            "{VRMC_SPRING_BONE} spring {s} joint {j} gravityDir is not finite: {dir:?}"
                        ))
                    })?,
                };
                chain.push((
                    at,
                    value("hitRadius", joint.hit_radius, 0.0)?,
                    value("stiffness", joint.stiffness, 1.0)?,
                    value("dragForce", joint.drag_force, 0.5)?,
                    value("gravityPower", joint.gravity_power, 0.0)?,
                    gravity_dir,
                ));
            }

            // `LogicFromTransform`: the last joint is a tail only.
            let mut joints = Vec::with_capacity(chain.len().saturating_sub(1));
            for pair in chain.windows(2) {
                let (head, radius, stiffness, drag, gravity_power, gravity_dir) = pair[0];
                let tail = pair[1].0;
                let scale = lossy_scale(&rest_world[tail], rest_rotation[tail]);
                let child = tree.rest[tail].translation * scale;
                let length = child.length();
                let bone_axis = normalize_safe(child);
                if !(length.is_finite() && bone_axis.is_finite() && scale.is_finite()) {
                    return Err(LoadError::Invalid(format!(
                        "{VRMC_SPRING_BONE} spring {s}: the bone from node {head} to node {tail} \
                         is not finite at rest"
                    )));
                }
                joints.push(JointLogic {
                    head,
                    tail,
                    parent: tree.parents[head],
                    local_rotation: tree.rest[head].rotation,
                    bone_axis,
                    length,
                    radius,
                    stiffness,
                    gravity_power,
                    gravity_dir,
                    drag,
                });
            }
            if joints.is_empty() {
                continue;
            }

            let indices: Vec<usize> = spring
                .collider_groups
                .iter()
                .flatten()
                .filter_map(|&group| usize::try_from(group).ok())
                .filter(|&group| group < groups.len())
                .collect();
            let listed = indices.iter().fold(0u64, |sum, &group| {
                sum.saturating_add(groups[group].len() as u64)
            });
            work = work.saturating_add(listed.saturating_mul(joints.len() as u64));
            if work > budget {
                return Err(LoadError::Invalid(format!(
                    "the spring bones check more than {budget} joint–collider pairs a step, \
                     the most the VRM plugin checks"
                )));
            }
            let colliders = indices
                .iter()
                .flat_map(|&group| &groups[group])
                .map(|&collider| colliders[collider])
                .collect();
            springs.push(Spring {
                joints,
                colliders,
                center,
            });
        }
        Ok(Self { springs })
    }

    /// Every joint at rest in `pose`, still (`InitCurrentTailsJob`): each
    /// tail's position in `pose`, in its spring's centre space.
    pub(crate) fn initial_state(&self, tree: &NodeTree, pose: &Pose) -> SpringState {
        let world = tree.world(pose);
        let current: Vec<Vec3> = self
            .springs
            .iter()
            .flat_map(|spring| {
                let to_centre = spring
                    .center
                    .map_or(Mat4::IDENTITY, |center| world[center].inverse());
                let world = &world;
                spring
                    .joints
                    .iter()
                    .map(move |logic| to_centre.transform_point3(position(&world[logic.tail])))
            })
            .collect();
        SpringState {
            prev: current.clone(),
            current,
        }
    }

    /// One step of `dt` seconds (`UpdateFastSpringBoneJob.Execute`), springs
    /// in order: turns each joint's head in `pose` and moves `state` on.
    ///
    /// # Panics
    ///
    /// When `state` is not this avatar's (from [`Self::initial_state`]).
    pub(crate) fn step(&self, tree: &NodeTree, pose: &mut Pose, state: &mut SpringState, dt: f32) {
        let mut transforms = Transforms::new(tree, pose);
        let mut next = Vec::with_capacity(state.current.len());
        let mut index = 0;
        for spring in &self.springs {
            for logic in &spring.joints {
                let head = transforms.head(logic, pose);
                let centre = spring.center.map(|center| transforms.world[center]);
                let to_world = |tail: Vec3| match centre {
                    Some(centre) => centre.transform_point3(tail),
                    None => tail,
                };
                let current = to_world(state.current[index]);
                let prev = to_world(state.prev[index]);
                index += 1;

                // Verlet: the last move, damped; the pull back to the rest
                // direction; gravity. UniVRM's model scaling is off, so its
                // factor is 1.
                let parent_rotation = transforms.parent_rotation(logic);
                let external = logic.gravity_dir * logic.gravity_power * dt;
                let mut tail = current
                    + (current - prev) * (1.0 - logic.drag)
                    + rotate(parent_rotation * logic.local_rotation, logic.bone_axis)
                        * logic.stiffness
                        * dt
                    + external;
                // Kept at the bone's length.
                tail = head + normalize_safe(tail - head) * logic.length;

                for &(node, shape) in &spring.colliders {
                    if let Some(pushed) = collide(logic, head, &transforms, node, shape, tail) {
                        tail = pushed;
                    }
                }

                next.push(match centre {
                    Some(centre) => centre.inverse().transform_point3(tail),
                    None => tail,
                });
                transforms.turn(logic, head, tail, pose);
            }
        }
        state.prev = std::mem::replace(&mut state.current, next);
    }

    /// Turns each joint's head in `pose` toward its tail in `state`, without
    /// stepping: the step's rotation with the last step's tails.
    ///
    /// # Panics
    ///
    /// When `state` is not this avatar's (from [`Self::initial_state`]).
    pub(crate) fn settle(&self, tree: &NodeTree, pose: &mut Pose, state: &SpringState) {
        let mut transforms = Transforms::new(tree, pose);
        let mut tails = state.current.iter();
        for spring in &self.springs {
            for logic in &spring.joints {
                let head = transforms.head(logic, pose);
                let tail = *tails.next().expect("a tail for every joint");
                let tail = match spring.center {
                    Some(center) => transforms.world[center].transform_point3(tail),
                    None => tail,
                };
                transforms.turn(logic, head, tail, pose);
            }
        }
    }
}

/// UniVRM's `Transforms` array for one step: every node's world matrix and
/// world rotation in the step's pose, a head's updated once it turns.
struct Transforms {
    world: Vec<Mat4>,
    rotation: Vec<Quat>,
}

impl Transforms {
    fn new(tree: &NodeTree, pose: &Pose) -> Self {
        Self {
            world: tree.world(pose),
            rotation: world_rotations(tree, pose),
        }
    }

    /// The head's position (`UpdateParentMatrix`): from its parent's entry
    /// and its local transform.
    fn head(&self, logic: &JointLogic, pose: &Pose) -> Vec3 {
        position(&match logic.parent {
            Some(parent) => self.world[parent] * pose.local[logic.head].matrix(),
            None => self.world[logic.head],
        })
    }

    fn parent_rotation(&self, logic: &JointLogic) -> Quat {
        logic
            .parent
            .map_or(Quat::IDENTITY, |parent| self.rotation[parent])
    }

    /// Turns the head at `head` so its bone points at `tail`
    /// (`UpdateRotation`): writes its local rotation to `pose` and its
    /// entry here.
    fn turn(&mut self, logic: &JointLogic, head: Vec3, tail: Vec3, pose: &mut Pose) {
        let rest = self.parent_rotation(logic) * logic.local_rotation;
        let rotation = from_to_rotation(rotate(rest, logic.bone_axis), tail - head) * rest;
        let local = &mut pose.local[logic.head];
        let world = match logic.parent {
            Some(parent) => {
                local.rotation = normalize_safe_rotation(inverse(self.rotation[parent]) * rotation);
                self.world[parent] * local.matrix()
            }
            None => {
                local.rotation = rotation;
                Mat4::from_scale_rotation_translation(local.scale, rotation, head)
            }
        };
        self.world[logic.head] = world;
        self.rotation[logic.head] = rotation;
    }
}

/// Where a collider moves `tail` to, if it touches it
/// (`SpringBoneCollision.TryCollide`), the joint's head at `head` and the
/// collider's node read from `transforms`.
fn collide(
    logic: &JointLogic,
    head: Vec3,
    transforms: &Transforms,
    node: usize,
    shape: ColliderShape,
    tail: Vec3,
) -> Option<Vec3> {
    let world = &transforms.world[node];
    let scale = lossy_scale(world, transforms.rotation[node]).abs();
    let max_scale = scale.x.max(scale.y).max(scale.z);
    // Outside a sphere: pushed out along its radius, then back to the
    // bone's length.
    let sphere = |centre: Vec3, radius: f32| {
        let r = logic.radius + radius * max_scale;
        let offset = tail - centre;
        (offset.length_squared() <= r * r).then(|| {
            let out = centre + normalize_safe(offset) * r;
            head + normalize_safe(out - head) * logic.length
        })
    };
    // Inside: pulled back in by how far it is out, not renormalised.
    let inside = |offset: Vec3, radius: f32| {
        let distance = radius - logic.radius - offset.length();
        (distance < 0.0).then(|| tail - (-normalize_safe(offset)) * distance)
    };
    match shape {
        ColliderShape::Sphere { offset, radius } => sphere(world.transform_point3(offset), radius),
        ColliderShape::Capsule {
            offset,
            radius,
            tail: end,
        } => {
            // The capsule's closest point to the joint's head, not its tail.
            let start = world.transform_point3(offset);
            let end = world.transform_point3(end);
            let direction = end - start;
            if direction.length_squared() == 0.0 {
                return sphere(start, radius);
            }
            let along = normalize_safe(direction);
            let dot = along.dot(head - start);
            if dot <= 0.0 {
                sphere(start, radius)
            } else if dot >= direction.length() {
                sphere(end, radius)
            } else {
                sphere(start + along * dot, radius)
            }
        }
        ColliderShape::Plane { offset, normal } => {
            let point = world.project_point3(offset);
            let normal = normalize_safe(world.transform_vector3(normal));
            let distance = (tail - point).dot(normal) - logic.radius;
            (distance < 0.0).then(|| tail - normal * distance)
        }
        ColliderShape::SphereInside { offset, radius } => {
            inside(tail - world.project_point3(offset), radius)
        }
        ColliderShape::CapsuleInside {
            offset,
            radius,
            tail: end,
        } => {
            let start = world.project_point3(offset);
            let end = world.project_point3(end);
            let start_to_end = end - start;
            let length_squared = start_to_end.length_squared();
            let mut delta = tail - start;
            let dot = start_to_end.dot(delta);
            if dot < 0.0 {
                // Before the start: measured from it.
            } else if dot > length_squared {
                delta -= start_to_end;
            } else {
                // A capsule of no length divides 0 by 0 here, as UniVRM's
                // does: the distance is NaN and nothing collides.
                delta -= start_to_end * (dot / length_squared);
            }
            inside(delta, radius)
        }
    }
}

/// A collider's shape: its base shape (a capsule over a sphere, as UniVRM
/// reads them), replaced by its `VRMC_springBone_extended_collider` shape.
fn collider_shape(index: usize, collider: &Collider) -> Result<ColliderShape, LoadError> {
    let invalid =
        |what: &str| LoadError::Invalid(format!("{VRMC_SPRING_BONE} collider {index} {what}"));
    let point = |value: Option<[f64; 3]>| value.map_or(Some(Vec3::ZERO), vector);
    let radius = |value: Option<f64>| narrow(value.unwrap_or(0.0));
    let shape = &collider.shape;
    let base = if let Some(capsule) = &shape.capsule {
        (|| {
            Some(ColliderShape::Capsule {
                offset: point(capsule.offset)?,
                radius: radius(capsule.radius)?,
                tail: point(capsule.tail)?,
            })
        })()
    } else if let Some(sphere) = &shape.sphere {
        (|| {
            Some(ColliderShape::Sphere {
                offset: point(sphere.offset)?,
                radius: radius(sphere.radius)?,
            })
        })()
    } else {
        return Err(invalid("has neither a sphere nor a capsule"));
    };
    let base = base.ok_or_else(|| invalid("is not finite"))?;

    let Some(extension) = collider
        .extensions
        .as_ref()
        .and_then(|extensions| extensions.get(EXTENDED_COLLIDER))
    else {
        return Ok(base);
    };
    let extended = serde_json::to_value(extension)
        .and_then(ExtendedCollider::deserialize)
        .map_err(|error| invalid(&format!("{EXTENDED_COLLIDER} is malformed: {error}")))?;
    let shape = extended.shape;
    let extended = if let Some(sphere) = shape.sphere {
        (|| {
            let offset = point(sphere.offset)?;
            let radius = radius(sphere.radius)?;
            Some(if sphere.inside.unwrap_or(false) {
                ColliderShape::SphereInside { offset, radius }
            } else {
                ColliderShape::Sphere { offset, radius }
            })
        })()
    } else if let Some(capsule) = shape.capsule {
        (|| {
            let offset = point(capsule.offset)?;
            let radius = radius(capsule.radius)?;
            let tail = point(capsule.tail)?;
            Some(if capsule.inside.unwrap_or(false) {
                ColliderShape::CapsuleInside {
                    offset,
                    radius,
                    tail,
                }
            } else {
                ColliderShape::Capsule {
                    offset,
                    radius,
                    tail,
                }
            })
        })()
    } else if let Some(plane) = shape.plane {
        (|| {
            Some(ColliderShape::Plane {
                offset: point(plane.offset)?,
                normal: plane.normal.map_or(Some(Vec3::Z), vector)?,
            })
        })()
    } else {
        return Err(invalid(&format!(
            "{EXTENDED_COLLIDER} has neither a sphere, a capsule, nor a plane"
        )));
    };
    extended.ok_or_else(|| invalid("is not finite"))
}

/// `value` in `f32`, if finite there.
fn narrow(value: f64) -> Option<f32> {
    let narrow = value as f32;
    narrow.is_finite().then_some(narrow)
}

/// Three numbers in `f32`, if each is finite there.
fn vector([x, y, z]: [f64; 3]) -> Option<Vec3> {
    Some(Vec3::new(narrow(x)?, narrow(y)?, narrow(z)?))
}

/// Each node's world rotation in `pose`, as Unity's `Transform.rotation`
/// keeps it: its parent's times its local rotation.
fn world_rotations(tree: &NodeTree, pose: &Pose) -> Vec<Quat> {
    let mut rotations = vec![Quat::IDENTITY; tree.parents.len()];
    for &node in &tree.pre_order {
        let local = pose.local[node].rotation;
        rotations[node] = match tree.parents[node] {
            Some(parent) => rotations[parent] * local,
            None => local,
        };
    }
    rotations
}

/// `BlittableTransform.lossyScale`: the diagonal of the world matrix with
/// its translation and rotation taken out.
fn lossy_scale(world: &Mat4, rotation: Quat) -> Vec3 {
    let position = world.w_axis.truncate();
    let m = Mat4::from_rotation_translation(rotation, position).inverse() * *world;
    Vec3::new(m.x_axis.x, m.y_axis.y, m.z_axis.z)
}

/// A world matrix's position.
fn position(world: &Mat4) -> Vec3 {
    world.w_axis.truncate()
}

/// Unity.Mathematics' `normalizesafe`: zero for a vector too short to
/// normalise.
fn normalize_safe(v: Vec3) -> Vec3 {
    let length = v.dot(v);
    if length > f32::MIN_POSITIVE {
        v * (1.0 / length.sqrt())
    } else {
        Vec3::ZERO
    }
}

/// Unity.Mathematics' `normalizesafe` for a quaternion: the identity for
/// one too short to normalise.
fn normalize_safe_rotation(q: Quat) -> Quat {
    let length = q.dot(q);
    if length > f32::MIN_POSITIVE {
        q * (1.0 / length.sqrt())
    } else {
        Quat::IDENTITY
    }
}

/// Unity.Mathematics' `inverse`: the conjugate over the squared length, so a
/// quaternion that is not quite unit is inverted rather than conjugated.
fn inverse(q: Quat) -> Quat {
    let scale = 1.0 / q.dot(q);
    Quat::from_xyzw(-q.x * scale, -q.y * scale, -q.z * scale, q.w * scale)
}

/// Unity.Mathematics' `mul(quaternion, float3)`.
fn rotate(q: Quat, v: Vec3) -> Vec3 {
    let axis = Vec3::new(q.x, q.y, q.z);
    let t = 2.0 * axis.cross(v);
    v + q.w * t + axis.cross(t)
}

/// `MathHelper.FromToRotation`: the turn from `from` to `to`, the identity
/// when either is too short or they nearly agree, and a half turn about an
/// axis across `from` when they nearly oppose.
fn from_to_rotation(from: Vec3, to: Vec3) -> Quat {
    const EPSILON: f32 = 1e-6;
    const EPSILON_SQ: f32 = EPSILON * EPSILON;
    if from.length_squared() < EPSILON_SQ || to.length_squared() < EPSILON_SQ {
        return Quat::IDENTITY;
    }
    let from = normalize_safe(from);
    let to = normalize_safe(to);
    let dot = from.dot(to).clamp(-1.0, 1.0);
    if dot > 1.0 - EPSILON {
        Quat::IDENTITY
    } else if dot < -1.0 + EPSILON {
        let mut axis = from.cross(Vec3::X);
        if axis.length_squared() < EPSILON_SQ {
            axis = from.cross(Vec3::Y);
        }
        axis_angle(normalize_safe(axis), std::f32::consts::PI)
    } else {
        axis_angle(normalize_safe(from.cross(to)), dot.acos())
    }
}

/// Unity.Mathematics' `quaternion.AxisAngle`, which does not ask for a unit
/// axis.
fn axis_angle(axis: Vec3, angle: f32) -> Quat {
    let (sin, cos) = (0.5 * angle).sin_cos();
    let v = axis * sin;
    Quat::from_xyzw(v.x, v.y, v.z, cos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticVrm, node};
    use serde_json::{Value, json};

    const DT: f32 = 1.0 / 60.0;

    /// The synthetic avatar with `spring_bone` as its `VRMC_springBone`.
    fn open(spring_bone: Value) -> (Document, NodeTree) {
        let mut vrm = SyntheticVrm::avatar();
        vrm.extensions.insert(VRMC_SPRING_BONE.into(), spring_bone);
        let document = Document::parse(&vrm.glb()).unwrap();
        let tree = NodeTree::new(&document).unwrap();
        (document, tree)
    }

    fn read(spring_bone: Value) -> (NodeTree, Springs) {
        let (document, tree) = open(spring_bone);
        let springs = Springs::read(&document, &tree).unwrap();
        (tree, springs)
    }

    fn refusal(spring_bone: Value) -> String {
        let (document, tree) = open(spring_bone);
        match Springs::read(&document, &tree) {
            Ok(_) => panic!("the springs were read"),
            Err(error) => error.to_string(),
        }
    }

    /// One spring of joints `[hair0, hair1]`, the first with `joint`'s
    /// fields, colliding with `colliders` (all in one group).
    fn hair(joint: Value, colliders: Value) -> Value {
        let mut head = json!({ "node": node::HAIR0 });
        head.as_object_mut()
            .unwrap()
            .extend(joint.as_object().unwrap().clone());
        let count = colliders.as_array().unwrap().len();
        json!({
            "specVersion": "1.0",
            "colliders": colliders,
            "colliderGroups": [{ "colliders": (0..count).collect::<Vec<_>>() }],
            "springs": [{
                "joints": [head, { "node": node::HAIR1 }],
                "colliderGroups": [0],
            }],
        })
    }

    /// Still: no stiffness, no gravity.
    fn still() -> Value {
        json!({ "stiffness": 0.0 })
    }

    /// The tail after `steps` steps from rest.
    fn tail_after(spring_bone: Value, steps: usize) -> Vec3 {
        let (tree, springs) = read(spring_bone);
        let mut pose = tree.rest_pose();
        let mut state = springs.initial_state(&tree, &pose);
        for _ in 0..steps {
            springs.step(&tree, &mut pose, &mut state, DT);
        }
        state.current[0]
    }

    fn assert_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).abs().max_element() < 1e-6,
            "{actual} is not {expected}"
        );
    }

    #[test]
    fn logic_from_rest() {
        let (_, springs) = read(hair(json!({}), json!([])));
        let [spring] = springs.springs.as_slice() else {
            panic!("{springs:?}");
        };
        let [logic] = spring.joints.as_slice() else {
            panic!("{spring:?}");
        };
        assert_eq!((logic.head, logic.tail), (node::HAIR0, node::HAIR1));
        assert_eq!(logic.parent, Some(node::HEAD));
        assert!((logic.length - 0.1118034).abs() < 1e-6, "{}", logic.length);
        assert_near(logic.bone_axis, Vec3::new(0.0, -0.8944272, -0.4472136));
        // The file's defaults.
        assert_eq!(
            (
                logic.radius,
                logic.stiffness,
                logic.drag,
                logic.gravity_power
            ),
            (0.0, 1.0, 0.5, 0.0)
        );
        assert_eq!(logic.gravity_dir, Vec3::NEG_Y);
    }

    #[test]
    fn a_spring_at_rest_stays_at_rest() {
        let spring_bone = hair(json!({ "stiffness": 1.0, "dragForce": 0.5 }), json!([]));
        assert_near(tail_after(spring_bone, 10), Vec3::new(0.0, 1.45, -0.1));
    }

    #[test]
    fn inertia_is_not_scaled_by_the_step() {
        let joint = json!({ "stiffness": 0.0, "dragForce": 0.0, "gravityPower": 1.0 });
        assert_near(
            tail_after(hair(joint.clone(), json!([])), 1),
            Vec3::new(0.0, 1.4472365, -0.0940415),
        );
        assert_near(
            tail_after(hair(joint, json!([])), 2),
            Vec3::new(0.0, 1.4432604, -0.0832666),
        );
    }

    /// Where the tail ends after one still step against the sphere and
    /// capsule tests' colliders.
    const PUSHED_OUT: Vec3 = Vec3::new(0.0, 1.4461932, -0.0915227);

    #[test]
    fn sphere_colliders_push_out_and_renormalise() {
        let colliders = json!([{
            "node": node::HEAD,
            "shape": { "sphere": { "offset": [0.0, 0.05, -0.15], "radius": 0.06 } },
        }]);
        assert_near(tail_after(hair(still(), colliders), 1), PUSHED_OUT);
    }

    #[test]
    fn capsules_measure_from_the_head() {
        let colliders = json!([{
            "node": node::HEAD,
            "shape": { "capsule": {
                "offset": [-1.0, 0.05, -0.15],
                "tail": [1.0, 0.05, -0.15],
                "radius": 0.06,
            } },
        }]);
        assert_near(tail_after(hair(still(), colliders), 1), PUSHED_OUT);
    }

    /// A plane on the head, 0.02 in front of the tail, facing +Z.
    fn plane() -> Value {
        json!({ "plane": { "offset": [0.0, 0.05, -0.08], "normal": [0.0, 0.0, 1.0] } })
    }

    #[test]
    fn planes_push_without_renormalising() {
        let colliders = json!([{
            "node": node::HEAD,
            "shape": { "sphere": { "radius": 0.0 } },
            "extensions": { EXTENDED_COLLIDER: { "specVersion": "1.0", "shape": plane() } },
        }]);
        assert_near(
            tail_after(hair(still(), colliders), 1),
            Vec3::new(0.0, 1.45, -0.08),
        );
    }

    #[test]
    fn inside_spheres_hold_the_tail_in() {
        let colliders = json!([{
            "node": node::HAIR0,
            "shape": { "sphere": { "radius": 0.0 } },
            "extensions": { EXTENDED_COLLIDER: {
                "specVersion": "1.0",
                "shape": { "sphere": { "radius": 0.08, "inside": true } },
            } },
        }]);
        assert_near(
            tail_after(hair(still(), colliders), 1),
            Vec3::new(0.0, 1.4784458, -0.0857771),
        );
    }

    #[test]
    fn inside_capsules_hold_the_tail_in() {
        // A capsule from hair0 straight back: the tail's closest point on
        // it is hair0 itself, so it holds like the inside sphere.
        let colliders = json!([{
            "node": node::HAIR0,
            "shape": { "sphere": { "radius": 0.0 } },
            "extensions": { EXTENDED_COLLIDER: {
                "specVersion": "1.0",
                "shape": { "capsule": { "tail": [0.0, 0.0, 1.0], "radius": 0.08, "inside": true } },
            } },
        }]);
        assert_near(
            tail_after(hair(still(), colliders), 1),
            Vec3::new(0.0, 1.4784458, -0.0857771),
        );
    }

    #[test]
    fn centre_space_moves_tails_with_the_centre() {
        let tail_world = |center: Option<usize>| {
            let mut spring_bone = hair(json!({}), json!([]));
            if let Some(center) = center {
                spring_bone["springs"][0]["center"] = json!(center);
            }
            let (tree, springs) = read(spring_bone);
            let mut pose = tree.rest_pose();
            let mut state = springs.initial_state(&tree, &pose);
            springs.step(&tree, &mut pose, &mut state, DT);
            pose.local[node::HIPS].translation.x += 1.0;
            springs.step(&tree, &mut pose, &mut state, DT);
            let centre = center.map_or(Mat4::IDENTITY, |center| tree.world(&pose)[center]);
            centre.transform_point3(state.current[0])
        };
        assert_near(tail_world(Some(node::HIPS)), Vec3::new(1.0, 1.45, -0.1));
        let lagging = tail_world(None);
        assert!(lagging.x < 1.0 - 1e-3, "{lagging}");
    }

    #[test]
    fn extended_colliders_replace_the_base_shape() {
        // The base sphere alone would push the tail out as the sphere test does.
        let colliders = json!([{
            "node": node::HEAD,
            "shape": { "sphere": { "offset": [0.0, 0.05, -0.15], "radius": 0.06 } },
            "extensions": { EXTENDED_COLLIDER: { "specVersion": "1.0", "shape": plane() } },
        }]);
        assert_near(
            tail_after(hair(still(), colliders), 1),
            Vec3::new(0.0, 1.45, -0.08),
        );
    }

    #[test]
    fn the_head_turns_toward_its_tail() {
        let joint = json!({ "stiffness": 0.0, "dragForce": 0.0, "gravityPower": 1.0 });
        let (tree, springs) = read(hair(joint, json!([])));
        let mut pose = tree.rest_pose();
        let mut state = springs.initial_state(&tree, &pose);
        springs.step(&tree, &mut pose, &mut state, DT);
        let drawn = tree.world(&pose)[node::HAIR1].w_axis.truncate();
        assert!((drawn - state.current[0]).length() < 1e-5, "{drawn}");

        // Settling a rest pose on the stepped tails turns it the same way.
        let mut settled = tree.rest_pose();
        springs.settle(&tree, &mut settled, &state);
        let turned = pose.local[node::HAIR0].rotation;
        let again = settled.local[node::HAIR0].rotation;
        assert!(turned.abs_diff_eq(again, 1e-6), "{turned} {again}");
    }

    #[test]
    fn heads_under_a_turned_parent_keep_their_local_rotation() {
        // The head is turned in the pose, so hair0's parent is not the
        // identity: its rest local rotation, written as a local, holds.
        let (tree, springs) = read(hair(json!({}), json!([])));
        let mut pose = tree.rest_pose();
        pose.local[node::HEAD].rotation = Quat::from_rotation_y(1.2) * Quat::from_rotation_x(0.4);
        let start = springs.initial_state(&tree, &pose);
        let mut state = start.clone();
        for _ in 0..10 {
            springs.step(&tree, &mut pose, &mut state, DT);
        }
        let local = pose.local[node::HAIR0].rotation;
        assert!(local.abs_diff_eq(Quat::IDENTITY, 1e-5), "{local}");
        assert!(
            (state.current[0] - start.current[0]).length() < 1e-5,
            "{} {}",
            state.current[0],
            start.current[0]
        );
    }

    #[test]
    fn chains_turn_from_their_turned_parents() {
        // hair1's head is read from hair0 as the step turned it, so the
        // drawn chain ends where the step put each tail.
        let (tree, springs) = read(json!({
            "specVersion": "1.0",
            "springs": [{
                "joints": [
                    { "node": node::HAIR0, "stiffness": 0.2, "gravityPower": 3.0 },
                    { "node": node::HAIR1, "stiffness": 0.2, "gravityPower": 3.0 },
                    { "node": node::HAIR2 },
                ],
            }],
        }));
        let mut pose = tree.rest_pose();
        let mut state = springs.initial_state(&tree, &pose);
        for _ in 0..5 {
            springs.step(&tree, &mut pose, &mut state, DT);
        }
        let world = tree.world(&pose);
        for (tail, &stepped) in [node::HAIR1, node::HAIR2].iter().zip(&state.current) {
            let drawn = position(&world[*tail]);
            assert!((drawn - stepped).length() < 1e-5, "{drawn} {stepped}");
        }
        assert!(state.current[1].y < 1.35 - 1e-3, "{}", state.current[1]);
    }

    #[test]
    fn steps_are_deterministic() {
        let colliders = json!([{
            "node": node::HEAD,
            "shape": { "sphere": { "offset": [0.0, 0.05, -0.15], "radius": 0.06 } },
        }]);
        let spring_bone = hair(json!({ "gravityPower": 2.0, "dragForce": 0.1 }), colliders);
        let run = || {
            let (tree, springs) = read(spring_bone.clone());
            let mut pose = tree.rest_pose();
            let mut state = springs.initial_state(&tree, &pose);
            for step in 0..40 {
                pose.local[node::HEAD].rotation = Quat::from_rotation_y(step as f32 * 0.05);
                springs.step(&tree, &mut pose, &mut state, DT);
            }
            (pose, state)
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn duplicated_joints_are_skipped_as_univrm_does() {
        // hair1 is taken by the first spring; the second keeps hair2 alone
        // and moves nothing.
        let (_, springs) = read(json!({
            "specVersion": "1.0",
            "springs": [
                { "joints": [{ "node": node::HAIR0 }, { "node": node::HAIR1 }] },
                { "joints": [{ "node": node::HAIR1 }, { "node": node::HAIR2 }] },
            ],
        }));
        assert_eq!(springs.springs.len(), 1);
        assert_eq!(springs.springs[0].joints.len(), 1);
    }

    #[test]
    fn missing_groups_and_colliders_are_skipped() {
        let mut spring_bone = hair(
            json!({}),
            json!([{ "node": node::HEAD, "shape": { "sphere": { "radius": 0.1 } } }]),
        );
        spring_bone["colliderGroups"][0]["colliders"] = json!([0, 7]);
        spring_bone["springs"][0]["colliderGroups"] = json!([0, 3, -1]);
        let (_, springs) = read(spring_bone);
        assert_eq!(springs.springs[0].colliders.len(), 1);
    }

    #[test]
    fn nodes_the_file_lacks_are_refused() {
        let mut joint = hair(json!({}), json!([]));
        joint["springs"][0]["joints"][1]["node"] = json!(99);
        let message = refusal(joint);
        assert!(
            message.contains("spring 0 joint 1 names node 99, but the file has 25 nodes"),
            "{message}"
        );

        let mut center = hair(json!({}), json!([]));
        center["springs"][0]["center"] = json!(25);
        let message = refusal(center);
        assert!(
            message.contains("spring 0 center names node 25"),
            "{message}"
        );

        let message = refusal(hair(
            json!({}),
            json!([{ "node": 30, "shape": { "sphere": {} } }]),
        ));
        assert!(message.contains("collider 0 names node 30"), "{message}");
    }

    #[test]
    fn numbers_past_f32_are_refused() {
        let message = refusal(hair(json!({ "stiffness": 1e39 }), json!([])));
        assert!(
            message.contains("spring 0 joint 0 stiffness is not finite"),
            "{message}"
        );
        let message = refusal(hair(json!({ "gravityDir": [0.0, -1e39, 0.0] }), json!([])));
        assert!(message.contains("gravityDir is not finite"), "{message}");
        let message = refusal(hair(
            json!({}),
            json!([{ "node": node::HEAD, "shape": { "sphere": { "radius": 1e39 } } }]),
        ));
        assert!(message.contains("collider 0 is not finite"), "{message}");
        let message = refusal(hair(
            json!({}),
            json!([{
                "node": node::HEAD,
                "shape": { "sphere": {} },
                "extensions": { EXTENDED_COLLIDER: {
                    "specVersion": "1.0",
                    "shape": { "plane": { "normal": [0.0, 1e39, 0.0] } },
                } },
            }]),
        ));
        assert!(message.contains("collider 0 is not finite"), "{message}");
    }

    #[test]
    fn a_bone_past_f32_at_rest_is_refused() {
        let mut vrm = SyntheticVrm::avatar();
        vrm.extensions
            .insert(VRMC_SPRING_BONE.into(), hair(json!({}), json!([])));
        let glb = vrm.glb_with(|root| {
            root["nodes"][node::HAIR0]["scale"] = json!([1e30, 1e30, 1e30]);
            root["nodes"][node::HAIR1]["scale"] = json!([1e30, 1e30, 1e30]);
        });
        let document = Document::parse(&glb).unwrap();
        let tree = NodeTree::new(&document).unwrap();
        let message = Springs::read(&document, &tree).unwrap_err().to_string();
        assert!(
            message.contains("the bone from node 8 to node 9 is not finite"),
            "{message}"
        );
    }

    #[test]
    fn shapeless_colliders_are_refused() {
        let message = refusal(hair(
            json!({}),
            json!([{ "node": node::HEAD, "shape": {} }]),
        ));
        assert!(
            message.contains("collider 0 has neither a sphere nor a capsule"),
            "{message}"
        );
        let message = refusal(hair(
            json!({}),
            json!([{
                "node": node::HEAD,
                "shape": { "sphere": {} },
                "extensions": { EXTENDED_COLLIDER: { "specVersion": "1.0", "shape": {} } },
            }]),
        ));
        assert!(
            message.contains("has neither a sphere, a capsule, nor a plane"),
            "{message}"
        );
    }

    #[test]
    fn work_past_the_budget_is_refused() {
        // Two moving joints against one collider listed three times: six
        // pairs a step.
        let mut spring_bone = json!({
            "specVersion": "1.0",
            "colliders": [{ "node": node::HEAD, "shape": { "sphere": { "radius": 0.1 } } }],
            "colliderGroups": [{ "colliders": [0, 0] }, { "colliders": [0] }],
            "springs": [{
                "joints": [
                    { "node": node::HAIR0 },
                    { "node": node::HAIR1 },
                    { "node": node::HAIR2 },
                ],
                "colliderGroups": [0, 1],
            }],
        });
        let (document, tree) = open(spring_bone.clone());
        let springs = Springs::read_with_budget(&document, &tree, 6).unwrap();
        assert_eq!(springs.springs[0].colliders.len(), 3);
        let message = Springs::read_with_budget(&document, &tree, 5)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("more than 5 joint–collider pairs a step"),
            "{message}"
        );

        // A spring that moves nothing checks nothing.
        spring_bone["springs"][0]["joints"] = json!([{ "node": node::HAIR0 }]);
        let (document, tree) = open(spring_bone);
        let springs = Springs::read_with_budget(&document, &tree, 0).unwrap();
        assert!(springs.springs.is_empty());
    }

    #[test]
    fn no_extension_no_springs() {
        let document = Document::parse(&SyntheticVrm::avatar().glb()).unwrap();
        let tree = NodeTree::new(&document).unwrap();
        assert!(Springs::read(&document, &tree).unwrap().springs.is_empty());
    }
}
