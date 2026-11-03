// Ported from UniVRM (https://github.com/vrm-c/UniVRM), under
// Packages/VRM10/Runtime/Components/: Constraint/Vrm10RollConstraint.cs,
// Constraint/Vrm10AimConstraint.cs, Constraint/Vrm10RotationConstraint.cs,
// and Vrm10Runtime/Vrm10Runtime.cs; and Packages/VRM10/Runtime/IO/Vrm10Importer.cs.
// MIT License, Copyright (c) 2020 VRM Consortium, Copyright (c) 2018 Masataka
// SUMI for MToon.

//! The avatar's node constraints (`VRMC_node_constraint`), with UniVRM's
//! rules, written in glTF's axes (UniVRM mirrors an aim axis's X into
//! Unity's; a roll about −X is the roll about +X).
//!
//! - **Reading** (`Vrm10Importer.LoadConstraintAsync`): each node's
//!   `extensions.VRMC_node_constraint`, `specVersion` 1.0, with exactly one
//!   of `roll`, `aim`, and `rotation`. A source that is not a node of the
//!   file or is the node itself, and a weight that is not finite, are
//!   refused; the weight is clamped to 0..1 (UniVRM's `Range(0, 1)`).
//! - **Order** (`Vrm10Runtime.Process`, whose `GetComponentsInChildren`
//!   walks the hierarchy in pre-order): each constrained node (the
//!   destination) once, in node pre-order, reading the pose as the
//!   constraints before it left it. So a destination whose source an
//!   earlier one moved sees the move, and a cycle through sources is no
//!   loop: each constraint still runs once.
//! - **Rest** (`InitPose`): `D₀` and `S₀` are the destination's and the
//!   source's local rest rotations; `slerp` takes the shorter arc.
//!   - **Rotation** (`Vrm10RotationConstraint`):
//!     `slerp(D₀, D₀ · (S₀⁻¹ · S), w)`.
//!   - **Roll** (`Vrm10RollConstraint`): the source's turn from rest,
//!     `Δ = S₀⁻¹ · S`, in the destination's rest frame,
//!     `Δd = D₀⁻¹ · (S₀ · Δ · S₀⁻¹) · D₀`, less its swing of the axis:
//!     `slerp(D₀, D₀ · from_to(axis, Δd · axis)⁻¹ · Δd, w)`.
//!   - **Aim** (`Vrm10AimConstraint`): with `P` the destination's parent's
//!     current world rotation, `from = P · D₀ · axis` and `to` the
//!     direction from the destination's current world position to the
//!     source's (`from` when they coincide):
//!     `slerp(D₀, P⁻¹ · from_to(from, to) · P · D₀, w)`.
//! - **World transforms** are read as the pose stands when a constraint
//!   runs, so they follow every constraint before it. Only an aim reads
//!   them: it climbs from its destination's parent and from its source to
//!   the root, composing local transforms (world rotations as products of
//!   local rotations, as Unity's are, so a node scaled to nothing still
//!   has one). A frame's constraints therefore cost one step per
//!   constraint plus, for each aim, the depth of its destination's parent
//!   and of its source. That sum is fixed when the model opens, and a
//!   model whose sum passes [`AIM_CLIMB_LIMIT`] is refused.

use crate::document::{Document, LoadError};
use crate::nodes::{NodeTree, Pose};
use glam::{Mat4, Quat, Vec3};
use serde::Deserialize;

/// The extension's name.
const VRMC_NODE_CONSTRAINT: &str = "VRMC_node_constraint";

/// The most nodes a frame's aim constraints climb together (see the module
/// documentation): far past any avatar's, whose aims are a few dozen
/// nodes deep.
const AIM_CLIMB_LIMIT: u64 = 1 << 20;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NodeConstraintExtension {
    spec_version: String,
    constraint: ConstraintJson,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConstraintJson {
    roll: Option<RollJson>,
    aim: Option<AimJson>,
    rotation: Option<RotationJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RollJson {
    source: usize,
    roll_axis: Axis,
    #[serde(default = "one")]
    weight: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AimJson {
    source: usize,
    aim_axis: AimAxis,
    #[serde(default = "one")]
    weight: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RotationJson {
    source: usize,
    #[serde(default = "one")]
    weight: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Copy, Deserialize)]
enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn vector(self) -> Vec3 {
        match self {
            Self::X => Vec3::X,
            Self::Y => Vec3::Y,
            Self::Z => Vec3::Z,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
enum AimAxis {
    PositiveX,
    NegativeX,
    PositiveY,
    NegativeY,
    PositiveZ,
    NegativeZ,
}

impl AimAxis {
    fn vector(self) -> Vec3 {
        match self {
            Self::PositiveX => Vec3::X,
            Self::NegativeX => Vec3::NEG_X,
            Self::PositiveY => Vec3::Y,
            Self::NegativeY => Vec3::NEG_Y,
            Self::PositiveZ => Vec3::Z,
            Self::NegativeZ => Vec3::NEG_Z,
        }
    }
}

/// What a constraint does, with its axis in the destination's space.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Roll(Vec3),
    Aim(Vec3),
    Rotation,
}

/// One node's constraint.
#[derive(Clone, Copy, Debug)]
struct Constraint {
    destination: usize,
    source: usize,
    /// In 0..1.
    weight: f32,
    kind: Kind,
}

/// The model's node constraints, by destination in node pre-order.
#[derive(Clone, Debug, Default)]
pub(crate) struct Constraints {
    constraints: Vec<Constraint>,
    /// The nodes a frame's aims climb (see the module documentation).
    climbed: u64,
}

impl Constraints {
    /// Every node's `VRMC_node_constraint` of `document`, whose nodes `tree`
    /// is; refused as the module documentation says.
    pub(crate) fn read(document: &Document, tree: &NodeTree) -> Result<Self, LoadError> {
        let count = tree.parents.len();
        let mut by_node: Vec<Option<Constraint>> = vec![None; count];
        for node in document.gltf.nodes() {
            let index = node.index();
            let Some(value) = node
                .extension_value(VRMC_NODE_CONSTRAINT)
                .filter(|value| !value.is_null())
            else {
                continue;
            };
            let extension = NodeConstraintExtension::deserialize(value).map_err(|error| {
                LoadError::Invalid(format!(
                    "node {index}'s {VRMC_NODE_CONSTRAINT} is malformed: {error}"
                ))
            })?;
            let version = &extension.spec_version;
            if !(version == "1.0" || version.starts_with("1.0-")) {
                return Err(LoadError::Invalid(format!(
                    "node {index}'s {VRMC_NODE_CONSTRAINT} specVersion {version} is not 1.0"
                )));
            }
            let ConstraintJson {
                roll,
                aim,
                rotation,
            } = extension.constraint;
            let kinds = usize::from(roll.is_some())
                + usize::from(aim.is_some())
                + usize::from(rotation.is_some());
            let (source, weight, kind) = match (roll, aim, rotation) {
                (Some(roll), None, None) => (
                    roll.source,
                    roll.weight,
                    Kind::Roll(roll.roll_axis.vector()),
                ),
                (None, Some(aim), None) => {
                    (aim.source, aim.weight, Kind::Aim(aim.aim_axis.vector()))
                }
                (None, None, Some(rotation)) => (rotation.source, rotation.weight, Kind::Rotation),
                _ => {
                    return Err(LoadError::Invalid(format!(
                        "node {index}'s constraint has {kinds} kinds; it needs one"
                    )));
                }
            };
            if source >= count {
                return Err(LoadError::Invalid(format!(
                    "node {index}'s constraint source {source} is not a node: the file has \
                     {count} nodes"
                )));
            }
            if source == index {
                return Err(LoadError::Invalid(format!(
                    "node {index}'s constraint source {source} is the node itself"
                )));
            }
            if !weight.is_finite() {
                return Err(LoadError::Invalid(format!(
                    "node {index}'s constraint weight {weight} is not finite"
                )));
            }
            by_node[index] = Some(Constraint {
                destination: index,
                source,
                weight: weight.clamp(0.0, 1.0),
                kind,
            });
        }

        let constraints: Vec<Constraint> = tree
            .pre_order
            .iter()
            .filter_map(|&node| by_node[node])
            .collect();

        // What a frame's aims climb: from each destination's parent and
        // from each source to the root.
        let mut depths = vec![0u64; count];
        for &node in &tree.pre_order {
            depths[node] = tree.parents[node].map_or(0, |parent| depths[parent]) + 1;
        }
        let climbed: u64 = constraints
            .iter()
            .filter(|constraint| matches!(constraint.kind, Kind::Aim(_)))
            .map(|constraint| {
                let parent = tree.parents[constraint.destination];
                parent.map_or(0, |parent| depths[parent]) + depths[constraint.source]
            })
            .sum();
        if climbed > AIM_CLIMB_LIMIT {
            return Err(LoadError::Invalid(format!(
                "the aim constraints climb {climbed} nodes a frame, more than the \
                 {AIM_CLIMB_LIMIT} the VRM plugin allows"
            )));
        }
        Ok(Self {
            constraints,
            climbed,
        })
    }

    /// The nodes a frame's aims climb.
    pub(crate) fn climbed(&self) -> u64 {
        self.climbed
    }

    /// Applies every constraint to `pose`, a pose of `tree`, in order.
    pub(crate) fn apply(&self, tree: &NodeTree, pose: &mut Pose) {
        for constraint in &self.constraints {
            let destination = constraint.destination;
            let rest = tree.rest[destination].rotation;
            let source_rest = tree.rest[constraint.source].rotation;
            let source = pose.local[constraint.source].rotation;
            let target = match constraint.kind {
                Kind::Rotation => rest * (source_rest.inverse() * source),
                Kind::Roll(axis) => {
                    let delta = source_rest.inverse() * source;
                    let in_parent = source_rest * delta * source_rest.inverse();
                    let in_destination = rest.inverse() * in_parent * rest;
                    let to = in_destination * axis;
                    rest * from_to_rotation(axis, to).inverse() * in_destination
                }
                Kind::Aim(axis) => {
                    let (parent_turn, parent_world) = match tree.parents[destination] {
                        Some(parent) => climb(tree, pose, parent),
                        None => (Quat::IDENTITY, Mat4::IDENTITY),
                    };
                    let position =
                        parent_world.transform_point3(pose.local[destination].translation);
                    let (_, source_world) = climb(tree, pose, constraint.source);
                    let from = parent_turn * rest * axis;
                    let to = (source_world.w_axis.truncate() - position)
                        .try_normalize()
                        .unwrap_or(from);
                    parent_turn.inverse() * from_to_rotation(from, to) * parent_turn * rest
                }
            };
            pose.local[destination].rotation = rest.slerp(target, constraint.weight);
        }
    }
}

/// `node`'s world rotation, as the product of its and its ancestors' local
/// rotations, and its world matrix, for `pose`.
fn climb(tree: &NodeTree, pose: &Pose, node: usize) -> (Quat, Mat4) {
    let mut turn = Quat::IDENTITY;
    let mut world = Mat4::IDENTITY;
    let mut at = Some(node);
    while let Some(node) = at {
        let local = &pose.local[node];
        turn = local.rotation * turn;
        world = local.matrix() * world;
        at = tree.parents[node];
    }
    (turn, world)
}

/// The rotation turning `from` onto `to` (Unity's `FromToRotation`): none
/// when they are within 1e-6 of each other (by their unit vectors' dot);
/// when they are opposite, a half turn about `normalize(cross(from, +X))`,
/// or `cross(from, +Y)` when that is shorter than 1e-6; else the shortest
/// arc. A vector of no length, or not finite, turns nothing.
pub(crate) fn from_to_rotation(from: Vec3, to: Vec3) -> Quat {
    let (Some(from), Some(to)) = (from.try_normalize(), to.try_normalize()) else {
        return Quat::IDENTITY;
    };
    let dot = from.dot(to);
    if dot >= 1.0 - 1e-6 {
        return Quat::IDENTITY;
    }
    if dot <= -1.0 + 1e-6 {
        let mut axis = from.cross(Vec3::X);
        if axis.length() < 1e-6 {
            axis = from.cross(Vec3::Y);
        }
        return Quat::from_axis_angle(axis.normalize(), std::f32::consts::PI);
    }
    Quat::from_rotation_arc(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::model::VrmModel;
    use crate::provider::PerformanceData;
    use crate::synthetic::{SyntheticVrm, node};
    use glam::{Quat, Vec3};
    use serde_json::{Value, json};
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn degrees(degrees: f32) -> f32 {
        degrees.to_radians()
    }

    /// The avatar with `constraints` (node, the `constraint` object), each
    /// in a `VRMC_node_constraint` of `specVersion` 1.0.
    fn constrained(constraints: &[(usize, Value)]) -> SyntheticVrm {
        let mut vrm = SyntheticVrm::avatar();
        vrm.node_extensions = constraints
            .iter()
            .map(|(node, constraint)| {
                (
                    *node,
                    json!({
                        "VRMC_node_constraint": { "specVersion": "1.0", "constraint": constraint },
                    }),
                )
            })
            .collect();
        vrm
    }

    fn read_glb(glb: &[u8]) -> Result<(NodeTree, Constraints), LoadError> {
        // The file must open: only `Constraints::read` may refuse it here.
        let document = match Document::parse(glb) {
            Ok(document) => document,
            Err(error) => panic!("the file did not parse: {error}"),
        };
        let tree = NodeTree::new(&document).unwrap();
        let constraints = Constraints::read(&document, &tree)?;
        Ok((tree, constraints))
    }

    fn read(vrm: &SyntheticVrm) -> (NodeTree, Constraints) {
        read_glb(&vrm.glb()).unwrap()
    }

    fn refusal_of(glb: &[u8]) -> String {
        match read_glb(glb) {
            Ok(_) => panic!("the constraints were read"),
            Err(error) => error.to_string(),
        }
    }

    fn refusal(constraints: &[(usize, Value)]) -> String {
        refusal_of(&constrained(constraints).glb())
    }

    /// The angle (radians) between two rotations; precise near 0, where
    /// glam's `angle_between` (an `acos`) is not in `f32`.
    fn angle(a: Quat, b: Quat) -> f32 {
        let turn = a.inverse() * b;
        2.0 * turn.xyz().length().atan2(turn.w.abs())
    }

    #[track_caller]
    fn assert_turn(actual: Quat, expected: Quat) {
        let angle = angle(actual, expected);
        assert!(angle < 1e-4, "{actual} is {angle} rad from {expected}");
    }

    #[track_caller]
    fn assert_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).abs().max_element() < 1e-4,
            "{actual} is not {expected}"
        );
    }

    #[test]
    fn rotation_copies_the_source_delta() {
        for (weight, expected) in [(1.0, 20.0), (0.5, 10.0)] {
            let vrm = constrained(&[(
                node::LEFT_LOWER_LEG,
                json!({ "rotation": { "source": node::LEFT_UPPER_LEG, "weight": weight } }),
            )]);
            let (tree, constraints) = read(&vrm);
            let mut pose = tree.rest_pose();
            pose.local[node::LEFT_UPPER_LEG].rotation = Quat::from_rotation_x(degrees(20.0));
            constraints.apply(&tree, &mut pose);
            assert_turn(
                pose.local[node::LEFT_LOWER_LEG].rotation,
                Quat::from_rotation_x(degrees(expected)),
            );
        }
    }

    #[test]
    fn the_weight_defaults_to_one_and_is_clamped() {
        for (weight, expected) in [(None, 20.0), (Some(2.0), 20.0), (Some(-1.0), 0.0)] {
            let mut rotation = json!({ "source": node::LEFT_UPPER_LEG });
            if let Some(weight) = weight {
                rotation["weight"] = json!(weight);
            }
            let vrm = constrained(&[(node::LEFT_LOWER_LEG, json!({ "rotation": rotation }))]);
            let (tree, constraints) = read(&vrm);
            let mut pose = tree.rest_pose();
            pose.local[node::LEFT_UPPER_LEG].rotation = Quat::from_rotation_x(degrees(20.0));
            constraints.apply(&tree, &mut pose);
            assert_turn(
                pose.local[node::LEFT_LOWER_LEG].rotation,
                Quat::from_rotation_x(degrees(expected)),
            );
        }
    }

    #[test]
    fn roll_keeps_only_the_twist() {
        let vrm = constrained(&[(
            node::RIGHT_LOWER_ARM,
            json!({ "roll": { "source": node::RIGHT_HAND, "rollAxis": "X", "weight": 0.5 } }),
        )]);
        let (tree, constraints) = read(&vrm);
        for (hand, expected) in [
            (
                Quat::from_rotation_x(degrees(60.0)),
                Quat::from_rotation_x(degrees(30.0)),
            ),
            (Quat::from_rotation_z(degrees(40.0)), Quat::IDENTITY),
        ] {
            let mut pose = tree.rest_pose();
            pose.local[node::RIGHT_HAND].rotation = hand;
            constraints.apply(&tree, &mut pose);
            assert_turn(pose.local[node::RIGHT_LOWER_ARM].rotation, expected);
        }
    }

    #[test]
    fn roll_reads_the_twist_through_rest_rotations() {
        // The left upper arm rests at Rz(90°), its X along the spine's Y: a
        // twist of the spine about its Y is a twist of the arm about its X.
        let vrm = constrained(&[(
            node::LEFT_UPPER_ARM,
            json!({ "roll": { "source": node::SPINE, "rollAxis": "X" } }),
        )]);
        let (tree, constraints) = read(&vrm);
        let mut pose = tree.rest_pose();
        pose.local[node::SPINE].rotation = Quat::from_rotation_y(degrees(30.0));
        constraints.apply(&tree, &mut pose);
        assert_turn(
            pose.local[node::LEFT_UPPER_ARM].rotation,
            Quat::from_rotation_z(degrees(90.0)) * Quat::from_rotation_x(degrees(30.0)),
        );
    }

    #[test]
    fn aim_points_the_axis_at_the_source() {
        let vrm = constrained(&[(
            node::LEFT_EYE,
            json!({ "aim": { "source": node::FIRST_PERSON_ONLY, "aimAxis": "PositiveZ" } }),
        )]);
        let (tree, constraints) = read(&vrm);
        let mut pose = tree.rest_pose();
        constraints.apply(&tree, &mut pose);
        let forward = tree.world(&pose)[node::LEFT_EYE]
            .transform_vector3(Vec3::Z)
            .normalize();
        assert_near(forward, Vec3::new(-0.5144958, 0.0, 0.8574929));
    }

    #[test]
    fn aim_follows_the_parent_turned_this_frame() {
        // The head turned by Ry(90°) this frame: the aim reads the turned
        // parent and the eye where the turn moved it.
        let vrm = constrained(&[(
            node::LEFT_EYE,
            json!({ "aim": { "source": node::FIRST_PERSON_ONLY, "aimAxis": "NegativeX" } }),
        )]);
        let (tree, constraints) = read(&vrm);
        let mut pose = tree.rest_pose();
        pose.local[node::HEAD].rotation = Quat::from_rotation_y(degrees(90.0));
        constraints.apply(&tree, &mut pose);
        let worlds = tree.world(&pose);
        let eye = worlds[node::LEFT_EYE].w_axis.truncate();
        let source = worlds[node::FIRST_PERSON_ONLY].w_axis.truncate();
        let axis = worlds[node::LEFT_EYE]
            .transform_vector3(Vec3::NEG_X)
            .normalize();
        assert_near(axis, (source - eye).normalize());
    }

    #[test]
    fn aim_at_the_opposite_direction_is_finite() {
        // The right eye is straight along the left eye's −X.
        let vrm = constrained(&[(
            node::LEFT_EYE,
            json!({ "aim": { "source": node::RIGHT_EYE, "aimAxis": "PositiveX" } }),
        )]);
        let (tree, constraints) = read(&vrm);
        let mut pose = tree.rest_pose();
        constraints.apply(&tree, &mut pose);
        let turn = pose.local[node::LEFT_EYE].rotation;
        assert!(turn.is_finite(), "{turn}");
        // A half turn about cross(+X, +Y) = +Z, as cross(+X, +X) is zero.
        assert_turn(turn, Quat::from_rotation_z(degrees(180.0)));
        let axis = tree.world(&pose)[node::LEFT_EYE].transform_vector3(Vec3::X);
        assert_near(axis, Vec3::NEG_X);
    }

    #[test]
    fn aim_at_a_coincident_source_keeps_the_rest() {
        let vrm = constrained(&[(
            node::HAIR0,
            json!({ "aim": { "source": node::FIRST_PERSON_ONLY, "aimAxis": "PositiveZ" } }),
        )]);
        let glb = vrm.glb_with(|root| {
            root["nodes"][node::FIRST_PERSON_ONLY]["translation"] = json!([0.0, 1.55, -0.05]);
        });
        let (tree, constraints) = read_glb(&glb).unwrap();
        let mut pose = tree.rest_pose();
        constraints.apply(&tree, &mut pose);
        assert_eq!(pose.local[node::HAIR0].rotation, Quat::IDENTITY);
    }

    #[test]
    fn from_to_rotation_follows_unity() {
        assert_eq!(from_to_rotation(Vec3::X, Vec3::X), Quat::IDENTITY);
        assert_turn(
            from_to_rotation(Vec3::X, Vec3::Y),
            Quat::from_rotation_z(degrees(90.0)),
        );
        // Half turns: about cross(from, +X), else cross(from, +Y).
        assert_turn(
            from_to_rotation(Vec3::Z, Vec3::NEG_Z),
            Quat::from_axis_angle(Vec3::Y, degrees(180.0)),
        );
        assert_turn(
            from_to_rotation(Vec3::NEG_X, Vec3::X),
            Quat::from_axis_angle(Vec3::NEG_Z, degrees(180.0)),
        );
    }

    #[test]
    fn chained_constraints_run_in_pre_order() {
        // Listed child first: the order is the nodes', not the list's.
        let vrm = constrained(&[
            (
                node::LEFT_LOWER_ARM,
                json!({ "rotation": { "source": node::LEFT_UPPER_ARM } }),
            ),
            (
                node::LEFT_UPPER_ARM,
                json!({ "rotation": { "source": node::SPINE } }),
            ),
        ]);
        let (tree, constraints) = read(&vrm);
        let mut pose = tree.rest_pose();
        pose.local[node::SPINE].rotation = Quat::from_rotation_y(degrees(30.0));
        constraints.apply(&tree, &mut pose);
        assert_turn(
            pose.local[node::LEFT_UPPER_ARM].rotation,
            Quat::from_rotation_z(degrees(90.0)) * Quat::from_rotation_y(degrees(30.0)),
        );
        // Reversed, it would read the upper arm at rest and stay identity.
        assert_turn(
            pose.local[node::LEFT_LOWER_ARM].rotation,
            Quat::from_rotation_y(degrees(30.0)),
        );
    }

    #[test]
    fn a_source_cycle_runs_each_constraint_once() {
        let vrm = constrained(&[
            (
                node::LEFT_UPPER_LEG,
                json!({ "rotation": { "source": node::RIGHT_UPPER_LEG } }),
            ),
            (
                node::RIGHT_UPPER_LEG,
                json!({ "rotation": { "source": node::LEFT_UPPER_LEG } }),
            ),
        ]);
        let (tree, constraints) = read(&vrm);
        let mut pose = tree.rest_pose();
        pose.local[node::RIGHT_UPPER_LEG].rotation = Quat::from_rotation_x(degrees(20.0));
        constraints.apply(&tree, &mut pose);
        // The left leg (first in pre-order) copies the right one; the right
        // then copies the left as it is now.
        assert_turn(
            pose.local[node::LEFT_UPPER_LEG].rotation,
            Quat::from_rotation_x(degrees(20.0)),
        );
        assert_turn(
            pose.local[node::RIGHT_UPPER_LEG].rotation,
            Quat::from_rotation_x(degrees(20.0)),
        );
    }

    #[test]
    fn a_deep_chain_of_constraints_runs_in_one_pass() {
        const DEPTH: usize = 100_000;
        // A chain under hair2, each node copying its parent's turn, and an
        // aim from hair0 at the chain's end.
        let glb = SyntheticVrm::avatar().glb_with(|root| {
            let nodes = root["nodes"].as_array_mut().unwrap();
            nodes[node::HAIR2]["children"] = json!([25]);
            for index in 25..25 + DEPTH {
                let mut chained = json!({
                    "translation": [0.0, -0.001, 0.0],
                    "extensions": { "VRMC_node_constraint": {
                        "specVersion": "1.0",
                        "constraint": { "rotation": { "source": index - 1 } },
                    } },
                });
                if index == 25 {
                    chained["extensions"]["VRMC_node_constraint"]["constraint"] =
                        json!({ "rotation": { "source": node::HAIR2 } });
                }
                if index + 1 < 25 + DEPTH {
                    chained["children"] = json!([index + 1]);
                }
                nodes.push(chained);
            }
            nodes[node::HAIR0]["extensions"] = json!({ "VRMC_node_constraint": {
                "specVersion": "1.0",
                "constraint": { "aim": { "source": 24 + DEPTH, "aimAxis": "NegativeY" } },
            } });
        });
        let (tree, constraints) = read_glb(&glb).unwrap();
        let mut pose = tree.rest_pose();
        pose.local[node::HAIR2].rotation = Quat::from_rotation_x(degrees(1.0));
        constraints.apply(&tree, &mut pose);
        assert_turn(
            pose.local[24 + DEPTH].rotation,
            Quat::from_rotation_x(degrees(1.0)),
        );
        assert!(pose.local.iter().all(|local| local.rotation.is_finite()));
    }

    #[test]
    fn aims_past_the_climb_limit_are_refused() {
        const DEPTH: usize = 2_000;
        const AIMS: usize = 600;
        let glb = SyntheticVrm::avatar().glb_with(|root| {
            let nodes = root["nodes"].as_array_mut().unwrap();
            nodes[node::HAIR2]["children"] = json!([25]);
            for index in 25..25 + DEPTH {
                let mut chained = json!({ "translation": [0.0, -0.001, 0.0] });
                if index < 25 + AIMS {
                    chained["extensions"] = json!({ "VRMC_node_constraint": {
                        "specVersion": "1.0",
                        "constraint": { "aim": { "source": 24 + DEPTH, "aimAxis": "NegativeY" } },
                    } });
                }
                if index + 1 < 25 + DEPTH {
                    chained["children"] = json!([index + 1]);
                }
                nodes.push(chained);
            }
        });
        let message = refusal_of(&glb);
        assert!(message.contains("aim constraints climb"), "{message}");
    }

    #[test]
    fn bad_constraints_are_refused() {
        let message = refusal(&[(
            node::LEFT_EYE,
            json!({ "rotation": { "source": node::LEFT_EYE } }),
        )]);
        assert!(message.contains("constraint source"), "{message}");
        assert!(message.contains("node 6"), "{message}");

        let message = refusal(&[(node::LEFT_EYE, json!({ "rotation": { "source": 25 } }))]);
        assert!(
            message.contains("node 6's constraint source 25"),
            "{message}"
        );

        let message = refusal(&[(
            node::LEFT_EYE,
            json!({
                "rotation": { "source": node::HEAD },
                "roll": { "source": node::HEAD, "rollAxis": "X" },
            }),
        )]);
        assert!(
            message.contains("node 6's constraint has 2 kinds; it needs one"),
            "{message}"
        );
        let message = refusal(&[(node::LEFT_EYE, json!({}))]);
        assert!(message.contains("has 0 kinds"), "{message}");

        let message = refusal(&[(
            node::LEFT_EYE,
            json!({ "rotation": { "source": node::HEAD, "weight": 1e39 } }),
        )]);
        assert!(message.contains("node 6's constraint weight"), "{message}");

        let message = refusal(&[(
            node::LEFT_EYE,
            json!({ "roll": { "source": node::HEAD, "rollAxis": "W" } }),
        )]);
        assert!(
            message.contains("node 6's VRMC_node_constraint"),
            "{message}"
        );

        let mut vrm = SyntheticVrm::avatar();
        vrm.node_extensions = vec![(
            node::LEFT_EYE,
            json!({ "VRMC_node_constraint": {
                "specVersion": "2.0",
                "constraint": { "rotation": { "source": node::HEAD } },
            } }),
        )];
        let message = refusal_of(&vrm.glb());
        assert!(message.contains("specVersion 2.0 is not 1.0"), "{message}");
    }

    #[test]
    fn the_frame_applies_constraints() {
        // Hair0 aims its +Z at the first-person quad in front of the face.
        let vrm = constrained(&[(
            node::HAIR0,
            json!({ "aim": { "source": node::FIRST_PERSON_ONLY, "aimAxis": "PositiveZ" } }),
        )]);
        let cache = TempDir::new().unwrap();
        let model = VrmModel::load_slice(&vrm.glb(), cache.path()).unwrap();
        let (pose, _) = model
            .pose_and_weights(0.0, &PerformanceData::default(), Vec3::ZERO)
            .unwrap();
        let forward = model.tree().world(&pose)[node::HAIR0]
            .transform_vector3(Vec3::Z)
            .normalize();
        assert_near(forward, Vec3::new(0.0, -0.1, 0.15).normalize());

        let bad = constrained(&[(node::HAIR0, json!({ "rotation": { "source": 99 } }))]);
        match VrmModel::load_slice(&bad.glb(), cache.path()) {
            Ok(_) => panic!("the model opened"),
            Err(error) => assert!(error.to_string().contains("constraint source 99")),
        }
    }

    /// The sample `name` in `CELESTA_VRM_SAMPLES_DIR`; `None`, having said the
    /// test is skipped, without the variable.
    fn sample(name: &str) -> Option<PathBuf> {
        let Some(directory) = std::env::var_os("CELESTA_VRM_SAMPLES_DIR") else {
            println!("skipped: CELESTA_VRM_SAMPLES_DIR is not set");
            return None;
        };
        let path = Path::new(&directory).join(name);
        assert!(path.is_file(), "{} is not a file", path.display());
        Some(path)
    }

    /// The sample at `path`, opened with its textures written to a fresh
    /// directory, never the user's texture cache; and its nodes' names.
    fn load_sample(path: &Path) -> (VrmModel, Vec<String>) {
        let cache = TempDir::new().unwrap();
        let model = VrmModel::load_with_cache(path, cache.path())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let document = Document::parse(&std::fs::read(path).unwrap()).unwrap();
        let names = document
            .gltf
            .nodes()
            .map(|node| node.name().unwrap_or_default().to_string())
            .collect();
        (model, names)
    }

    fn named(names: &[String], name: &str) -> usize {
        names
            .iter()
            .position(|node| node == name)
            .unwrap_or_else(|| panic!("no node is named {name}"))
    }

    #[test]
    fn seed_san_robot_arm_follows_the_left_arm() {
        let Some(path) = sample("Seed-san.vrm") else {
            return;
        };
        let (model, names) = load_sample(&path);
        let tree = model.tree();
        let constraints = model.constraints();
        let robot = [
            "robo_shoulder.L",
            "robo_upper_arm.L",
            "robo_forearm.L",
            "robo_hand.L",
        ]
        .map(|name| named(&names, name));

        // At rest every robot joint copies a rest delta: nothing moves.
        let rest = tree.rest_pose();
        let mut pose = rest.clone();
        constraints.apply(tree, &mut pose);
        let (before, after) = (tree.world(&rest), tree.world(&pose));
        for &joint in &robot {
            let (from, to) = (
                before[joint].w_axis.truncate(),
                after[joint].w_axis.truncate(),
            );
            println!("{} at rest: {from} → {to}", names[joint]);
            assert_near(to, from);
        }

        // The left upper arm lowered 60° (about the model's −Z, which takes
        // +X down): the robot's upper arm turns by the same local delta,
        // and its hand comes down.
        let upper = named(&names, "upper_arm.L");
        let parent = tree.parents[upper].unwrap();
        let (_, parent_turn, _) = before[parent].to_scale_rotation_translation();
        let lower = Quat::from_rotation_z(degrees(-60.0));
        let delta = rest.local[upper].rotation.inverse()
            * parent_turn.inverse()
            * lower
            * parent_turn
            * rest.local[upper].rotation;
        let mut pose = rest.clone();
        pose.local[upper].rotation = rest.local[upper].rotation * delta;
        constraints.apply(tree, &mut pose);
        let robot_upper = robot[1];
        assert_turn(
            pose.local[robot_upper].rotation,
            rest.local[robot_upper].rotation * delta,
        );
        let worlds = tree.world(&pose);
        for &joint in &robot {
            let (from, to) = (
                before[joint].w_axis.truncate(),
                worlds[joint].w_axis.truncate(),
            );
            println!("{} with the arm lowered: {from} → {to}", names[joint]);
        }
        let hand = robot[3];
        let drop = before[hand].w_axis.y - worlds[hand].w_axis.y;
        assert!(drop > 0.2, "the robot hand came down {drop} m");
    }

    #[test]
    fn twist_sample_rests_where_its_constraints_put_it() {
        let Some(path) = sample("VRM1_Constraint_Twist_Sample.vrm") else {
            return;
        };
        let (model, names) = load_sample(&path);
        let tree = model.tree();
        let rest = tree.rest_pose();
        let mut pose = rest.clone();
        model.constraints().apply(tree, &mut pose);
        // Its aims rest pointing at their sources and its rolls copy rest
        // deltas, so the constraints barely move it; a wrong axis would turn
        // an aim by a half turn.
        for (index, (from, to)) in rest.local.iter().zip(&pose.local).enumerate() {
            let angle = angle(from.rotation, to.rotation);
            if angle > 1e-6 {
                println!("{}: {angle} rad", names[index]);
            }
            assert!(angle < degrees(1.0), "{} turned {angle} rad", names[index]);
        }
    }
}
