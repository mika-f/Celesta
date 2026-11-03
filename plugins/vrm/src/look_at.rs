// Ported from UniVRM (https://github.com/vrm-c/UniVRM), under
// Packages/VRM10/Runtime/Components/: Vrm10Runtime/Vrm10RuntimeLookAt.cs,
// LookAt/CurveMapper.cs, LookAt/LookAtEyeDirectionApplicableToBone.cs,
// LookAt/LookAtEyeDirectionApplicableToExpression.cs, and
// LookAt/Matrix4x4Extensions.cs; and Packages/VRM10/Runtime/IO/Vrm10Importer.cs.
// MIT License, Copyright (c) 2020 VRM Consortium, Copyright (c) 2018 Masataka
// SUMI for MToon.

//! Where the avatar's eyes look, with UniVRM's rules.
//!
//! UniVRM works in Unity's axes, glTF's with X mirrored. In glTF's, a yaw
//! to the avatar's right turns toward −X.
//!
//! - **Origin** (`InitializeLookAtOriginTransform`): a point parented to the
//!   head, placed at rest at `offsetFromHeadBone` in the head's space, with
//!   the model's axes. It then moves and turns with the head.
//! - **Yaw and pitch of a point** (`CalcYawPitch`), in degrees: the point in
//!   the origin's space is `p`; yaw is `atan2(−p.x, p.z)`, positive to the
//!   avatar's right, and pitch `atan2(p.y, |p.xz|)`, positive up.
//! - **Range maps** (`CurveMapper`): `clamp01(d / max(0.001, inputMax)) ×
//!   outputScale`. UniVRM's runtime keeps an `inputMaxValue` of 0, which
//!   this saturates; only its editor reads it as 90.
//! - **Bones** (`LookAtEyeDirectionApplicableToBone`), when the file says
//!   `bone` and the humanoid has both eyes: each eye's local rotation is
//!   `pre × Ry(−yaw) × Rx(pitch) × post`, Unity's `Euler(pitch, yaw, 0)`
//!   in glTF's axes between the eye's rest rotations (`post`, its rest
//!   rotation in model space; `pre`, its local rest rotation times
//!   `post⁻¹`). Looking left (yaw < 0) the left eye turns by the outer map
//!   and the right by the inner one, and the other way round looking
//!   right; down turns by the down map, up by the up map.
//! - **Expressions** (`LookAtEyeDirectionApplicableToExpression`), for type
//!   `expression` or a humanoid without both eyes, as UniVRM falls back to
//!   them: `lookLeft` or `lookRight` by the outer map (UniVRM uses it both
//!   ways) and `lookDown` or `lookUp` by the down or up map, each clamped
//!   to 0..1, the other of each pair 0. They are written after the
//!   expressions are validated, every frame, over any weight asked for.
//! - **Defaults** (`VRM10ObjectLookAt`, `Vrm10Importer`): without a lookAt,
//!   type `bone`, offset (0, 0.06, 0), and every map (90, 10). A map the
//!   file has without one of its values takes that value's default;
//!   UniVRM's importer throws there.

use crate::document::{Document, LoadError};
use crate::expressions::{ExpressionKey, Preset};
use crate::humanoid::{HumanBone, Humanoid};
use crate::nodes::{NodeTree, Pose};
use glam::{Mat4, Quat, Vec3};
use std::collections::BTreeMap;
use vrm_spec::vrmc_vrm_1_0::{LookAtRangeMap, LookAtType};

/// The origin's offset from the head without one in the file.
const DEFAULT_OFFSET: Vec3 = Vec3::new(0.0, 0.06, 0.0);

/// How far an eye turns (degrees), or how much a look expression weighs,
/// for an angle from the head's forward (UniVRM's `CurveMapper`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RangeMap {
    /// The angle (degrees) past which the output stays at `output_scale`.
    pub input_max: f32,
    pub output_scale: f32,
}

impl RangeMap {
    /// A map the file does not have (`VRM10ObjectLookAt`'s).
    pub(crate) const DEFAULT: Self = Self {
        input_max: 90.0,
        output_scale: 10.0,
    };

    pub(crate) fn map(&self, degrees: f32) -> f32 {
        (degrees / self.input_max.max(0.001)).clamp(0.0, 1.0) * self.output_scale
    }
}

/// An eye bone and the rest rotations its gaze is turned between.
#[derive(Clone, Copy, Debug)]
struct Eye {
    node: usize,
    pre: Quat,
    post: Quat,
}

/// What the gaze moves.
#[derive(Clone, Copy, Debug)]
enum Applies {
    Bones { left: Eye, right: Eye },
    Expressions,
}

/// The avatar's lookAt.
#[derive(Clone, Debug)]
pub(crate) struct LookAt {
    head: usize,
    /// The origin in the head's space, fixed at rest.
    origin_local: Mat4,
    applies: Applies,
    inner: RangeMap,
    outer: RangeMap,
    down: RangeMap,
    up: RangeMap,
}

impl LookAt {
    /// The lookAt of `document`, whose tree and humanoid are `tree` and
    /// `humanoid`. A number that is not finite in `f32`, or an offset with
    /// fewer than three numbers, is refused.
    pub(crate) fn new(
        document: &Document,
        tree: &NodeTree,
        humanoid: &Humanoid,
    ) -> Result<Self, LoadError> {
        let spec = document.vrm.look_at.as_ref();
        let offset = match spec.and_then(|spec| spec.offset_from_head_bone.as_deref()) {
            None => DEFAULT_OFFSET,
            Some(&[x, y, z, ..]) => {
                let offset = Vec3::new(x as f32, y as f32, z as f32);
                if !offset.is_finite() {
                    return Err(LoadError::Invalid(format!(
                        "lookAt offsetFromHeadBone is not finite: {:?}",
                        [x, y, z]
                    )));
                }
                offset
            }
            Some(short) => {
                return Err(LoadError::Invalid(format!(
                    "lookAt offsetFromHeadBone has fewer than three numbers: {short:?}"
                )));
            }
        };
        let map = |name: &str, map: Option<&LookAtRangeMap>| -> Result<RangeMap, LoadError> {
            let value = |field: &str, value: Option<f64>, default: f32| {
                let Some(value) = value else {
                    return Ok(default);
                };
                let narrow = value as f32;
                if narrow.is_finite() {
                    Ok(narrow)
                } else {
                    Err(LoadError::Invalid(format!(
                        "lookAt {name} {field} is not finite: {value}"
                    )))
                }
            };
            let Some(map) = map else {
                return Ok(RangeMap::DEFAULT);
            };
            Ok(RangeMap {
                input_max: value("inputMaxValue", map.input_max_value, 90.0)?,
                output_scale: value("outputScale", map.output_scale, 10.0)?,
            })
        };
        let inner = map(
            "rangeMapHorizontalInner",
            spec.and_then(|spec| spec.range_map_horizontal_inner.as_ref()),
        )?;
        let outer = map(
            "rangeMapHorizontalOuter",
            spec.and_then(|spec| spec.range_map_horizontal_outer.as_ref()),
        )?;
        let down = map(
            "rangeMapVerticalDown",
            spec.and_then(|spec| spec.range_map_vertical_down.as_ref()),
        )?;
        let up = map(
            "rangeMapVerticalUp",
            spec.and_then(|spec| spec.range_map_vertical_up.as_ref()),
        )?;

        // `Document::parse` checked the head is there, on a node of the file.
        let rest = tree.world(&tree.rest_pose());
        let Some((head, &head_rest)) = humanoid
            .node(HumanBone::Head)
            .and_then(|head| Some((head, rest.get(head)?)))
        else {
            return Err(LoadError::Invalid(
                "the lookAt needs a head bone on a node of the file".into(),
            ));
        };
        // Placed at the offset in the head's space with the model's axes, then
        // parented to the head. A head that cannot be inverted gives an
        // origin that is not finite, which only a point target needs.
        let origin_local =
            head_rest.inverse() * Mat4::from_translation(head_rest.transform_point3(offset));

        let bones = matches!(
            spec.and_then(|spec| spec.look_at_type),
            None | Some(LookAtType::Bone)
        );
        let eye_of = |bone| humanoid.node(bone).and_then(|node| eye(tree, node));
        let applies = match (
            bones,
            eye_of(HumanBone::LeftEye),
            eye_of(HumanBone::RightEye),
        ) {
            (true, Some(left), Some(right)) => Applies::Bones { left, right },
            _ => Applies::Expressions,
        };
        Ok(Self {
            head,
            origin_local,
            applies,
            inner,
            outer,
            down,
            up,
        })
    }

    /// The origin's model-space matrix for nodes at `worlds`.
    pub(crate) fn origin(&self, worlds: &[Mat4]) -> Mat4 {
        let head = worlds.get(self.head).copied().unwrap_or(Mat4::IDENTITY);
        head * self.origin_local
    }

    /// The yaw and pitch (degrees) of model-space `target` from `origin`,
    /// or `None` when `origin` cannot be inverted.
    pub(crate) fn yaw_pitch(&self, origin: Mat4, target: Vec3) -> Option<(f32, f32)> {
        let p = origin.inverse().transform_point3(target);
        if !p.is_finite() {
            return None;
        }
        let yaw = (-p.x).atan2(p.z).to_degrees();
        let pitch = p.y.atan2(p.x.hypot(p.z)).to_degrees();
        Some((yaw, pitch))
    }

    /// Turns the eyes in `pose` for a gaze of `yaw` and `pitch` (degrees),
    /// or writes the look keys into `actual`.
    pub(crate) fn apply(
        &self,
        yaw: f32,
        pitch: f32,
        pose: &mut Pose,
        actual: &mut BTreeMap<ExpressionKey, f32>,
    ) {
        match self.applies {
            Applies::Bones { left, right } => {
                let (left_yaw, right_yaw) = if yaw < 0.0 {
                    (-self.outer.map(-yaw), -self.inner.map(-yaw))
                } else {
                    (self.inner.map(yaw), self.outer.map(yaw))
                };
                let pitch = if pitch < 0.0 {
                    self.down.map(-pitch)
                } else {
                    -self.up.map(pitch)
                };
                for (eye, yaw) in [(left, left_yaw), (right, right_yaw)] {
                    let turn = Quat::from_rotation_y(-yaw.to_radians())
                        * Quat::from_rotation_x(pitch.to_radians());
                    if let Some(local) = pose.local.get_mut(eye.node) {
                        local.rotation = (eye.pre * turn * eye.post).normalize();
                    }
                }
            }
            Applies::Expressions => {
                let weight = |map: &RangeMap, degrees: f32| map.map(degrees).clamp(0.0, 1.0);
                let (left, right) = if yaw < 0.0 {
                    (weight(&self.outer, -yaw), 0.0)
                } else {
                    (0.0, weight(&self.outer, yaw))
                };
                let (down, up) = if pitch < 0.0 {
                    (weight(&self.down, -pitch), 0.0)
                } else {
                    (0.0, weight(&self.up, pitch))
                };
                for (preset, weight) in [
                    (Preset::LookLeft, left),
                    (Preset::LookRight, right),
                    (Preset::LookDown, down),
                    (Preset::LookUp, up),
                ] {
                    actual.insert(ExpressionKey::Preset(preset), weight);
                }
            }
        }
    }
}

/// The eye bone at `node`, with its rest rotations; `None` when `node` is
/// not the tree's.
fn eye(tree: &NodeTree, node: usize) -> Option<Eye> {
    let local = tree.rest.get(node)?.rotation;
    let post = rest_world_rotation(tree, node)?;
    Some(Eye {
        node,
        pre: local * post.inverse(),
        post,
    })
}

/// `node`'s rest rotation in model space: its ancestors' local rotations
/// and its own, composed (Unity's `Transform.rotation`, which leaves scale
/// out).
fn rest_world_rotation(tree: &NodeTree, node: usize) -> Option<Quat> {
    let mut rotation = tree.rest.get(node)?.rotation;
    let mut current = node;
    // `NodeTree` refuses cycles, so a climb ends within one step per node.
    for _ in 0..tree.parents.len() {
        let Some(parent) = tree.parents.get(current).copied().flatten() else {
            return Some(rotation.normalize());
        };
        rotation = tree.rest.get(parent)?.rotation * rotation;
        current = parent;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VrmModel;
    use crate::provider::{LookAtData, LookAtKeyword, PerformanceData};
    use crate::synthetic::{SyntheticVrm, node};
    use glam::{Mat4, Vec3};
    use serde_json::{Value, json};
    use tempfile::TempDir;

    fn load(vrm: &SyntheticVrm) -> VrmModel {
        let cache = TempDir::new().unwrap();
        VrmModel::load_slice(&vrm.glb(), cache.path()).unwrap()
    }

    /// The avatar with its `VRMC_vrm.lookAt` changed by `edit`.
    fn with_look_at(edit: impl FnOnce(&mut Value)) -> SyntheticVrm {
        let mut vrm = SyntheticVrm::avatar();
        edit(&mut vrm.look_at);
        vrm
    }

    fn refusal(vrm: &SyntheticVrm) -> String {
        let cache = TempDir::new().unwrap();
        match VrmModel::load_slice(&vrm.glb(), cache.path()) {
            Ok(_) => panic!("the model opened"),
            Err(error) => error.to_string(),
        }
    }

    fn data(look_at: Option<LookAtData>, expressions: &[(&str, f32)]) -> PerformanceData {
        PerformanceData {
            expressions: expressions
                .iter()
                .map(|&(name, weight)| (name.to_string(), weight))
                .collect(),
            look_at,
        }
    }

    fn angles(yaw: f32, pitch: f32) -> Option<LookAtData> {
        Some(LookAtData::Angles { yaw, pitch })
    }

    /// The camera eye frames are seen from until Task 7 frames the avatar.
    const EYE: Vec3 = Vec3::new(0.0, 0.8, 3.3);

    /// The left and right eyes' forward, (0, 0, 1) turned by their worlds,
    /// for `data` seen from a camera at `eye`.
    fn eyes_seen_from(model: &VrmModel, data: &PerformanceData, eye: Vec3) -> (Vec3, Vec3) {
        let (pose, _) = model.pose_and_weights(0.0, data, eye).unwrap();
        let worlds = model.tree().world(&pose);
        let forward = |node: usize| worlds[node].transform_vector3(Vec3::Z).normalize();
        (forward(node::LEFT_EYE), forward(node::RIGHT_EYE))
    }

    fn eyes(model: &VrmModel, data: &PerformanceData) -> (Vec3, Vec3) {
        eyes_seen_from(model, data, EYE)
    }

    /// Forward turned by yaw (degrees, about +Y) then pitch (about +X).
    fn turned(yaw: f32, pitch: f32) -> Vec3 {
        Quat::from_rotation_y(yaw.to_radians())
            * Quat::from_rotation_x(pitch.to_radians())
            * Vec3::Z
    }

    fn assert_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).abs().max_element() < 1e-4,
            "{actual} against {expected}"
        );
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 1e-4
    }

    /// Morph target `target` of draw 0 (Body's box) in a frame for `data`.
    fn target(model: &mut VrmModel, data: &PerformanceData, target: usize) -> f32 {
        model.frame(0.0, data).unwrap().draws[0].morph_weights[target]
    }

    fn frame_error(model: &mut VrmModel, data: &PerformanceData) -> String {
        match model.frame(0.0, data) {
            Ok(_) => panic!("the frame was made"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn range_map_clamps_and_scales() {
        let map = RangeMap {
            input_max: 90.0,
            output_scale: 10.0,
        };
        assert!(close(map.map(45.0), 5.0));
        assert!(close(map.map(180.0), 10.0));
        assert_eq!(map.map(-5.0), 0.0);
        let zero = RangeMap {
            input_max: 0.0,
            output_scale: 10.0,
        };
        assert!(close(zero.map(45.0), 10.0));
        assert_eq!(zero.map(0.0), 0.0);
        assert_eq!(RangeMap::DEFAULT, map);
    }

    #[test]
    fn angles_turn_the_eyes_by_the_range_maps() {
        let model = load(&SyntheticVrm::avatar());
        let (left, right) = eyes(&model, &data(angles(20.0, -30.0), &[]));
        assert_near(left, Vec3::new(-0.0386809, -0.0697565, 0.9968138));
        assert_near(right, Vec3::new(-0.0773037, -0.0697565, 0.9945643));
        // Looking left and up: the left eye by the outer map, the right by
        // the inner one, both up by the up map.
        let (left, right) = eyes(&model, &data(angles(-45.0, 45.0), &[]));
        assert_near(left, turned(10.0, -5.0));
        assert_near(right, turned(5.0, -5.0));
        // No lookAt: the eyes keep their rest.
        let (left, right) = eyes(&model, &PerformanceData::default());
        assert_near(left, Vec3::Z);
        assert_near(right, Vec3::Z);
    }

    #[test]
    fn eyes_turn_in_the_model_s_axes_from_their_rest() {
        // A head turned Ry(30°) and a left eye turned Rx(10°) at rest: the
        // gaze turns each eye's rest in the model's axes.
        let half = |degrees: f32| (degrees.to_radians() / 2.0).sin_cos();
        let (y_sin, y_cos) = half(30.0);
        let (x_sin, x_cos) = half(10.0);
        let cache = TempDir::new().unwrap();
        let glb = SyntheticVrm::avatar().glb_with(|root| {
            root["nodes"][node::HEAD]["rotation"] = json!([0.0, y_sin, 0.0, y_cos]);
            root["nodes"][node::LEFT_EYE]["rotation"] = json!([x_sin, 0.0, 0.0, x_cos]);
        });
        let model = VrmModel::load_slice(&glb, cache.path()).unwrap();
        let rest_left =
            Quat::from_rotation_y(30f32.to_radians()) * Quat::from_rotation_x(10f32.to_radians());
        let rest_right = Quat::from_rotation_y(30f32.to_radians());
        let (left, right) = eyes(&model, &PerformanceData::default());
        assert_near(left, rest_left * Vec3::Z);
        assert_near(right, rest_right * Vec3::Z);
        // Yaw 20, pitch −30: left 2.222° (inner), right 4.444° (outer),
        // both 4° down.
        let (left, right) = eyes(&model, &data(angles(20.0, -30.0), &[]));
        let gaze = |yaw: f32| {
            Quat::from_rotation_y(-yaw.to_radians()) * Quat::from_rotation_x(4f32.to_radians())
        };
        assert_near(left, gaze(20.0 / 9.0) * rest_left * Vec3::Z);
        assert_near(right, gaze(40.0 / 9.0) * rest_right * Vec3::Z);
    }

    #[test]
    fn point_targets_give_yaw_and_pitch_from_the_origin() {
        let model = load(&SyntheticVrm::avatar());
        let look_at = model.look_at();
        let rest = model.tree().world(&model.tree().rest_pose());
        let origin = look_at.origin(&rest);
        assert!(origin.abs_diff_eq(Mat4::from_translation(Vec3::new(0.0, 1.46, 0.0)), 1e-6));
        let (yaw, pitch) = look_at
            .yaw_pitch(origin, Vec3::new(-1.0, 1.46, 1.0))
            .unwrap();
        assert!(close(yaw, 45.0) && close(pitch, 0.0), "{yaw}, {pitch}");
        let (yaw, pitch) = look_at
            .yaw_pitch(origin, Vec3::new(0.0, 2.46, 1.0))
            .unwrap();
        assert!(close(yaw, 0.0) && close(pitch, 45.0), "{yaw}, {pitch}");
        // Behind the avatar, to its left.
        let (yaw, _) = look_at
            .yaw_pitch(origin, Vec3::new(1.0, 1.46, -1.0))
            .unwrap();
        assert!(close(yaw, -135.0), "{yaw}");

        let point = Some(LookAtData::Point([-1.0, 1.46, 1.0]));
        let (_, right) = eyes(&model, &data(point, &[]));
        assert!(close(right.x, -0.1736482), "{right}");

        // The origin follows the head as posed.
        let mut pose = model.tree().rest_pose();
        pose.local[node::HEAD].translation += Vec3::new(1.0, 0.0, 0.0);
        let origin = look_at.origin(&model.tree().world(&pose));
        assert!(origin.abs_diff_eq(Mat4::from_translation(Vec3::new(1.0, 1.46, 0.0)), 1e-6));
    }

    #[test]
    fn the_origin_turns_with_the_head() {
        // A head turned 90° about +Y at rest: the origin keeps the model's
        // axes at rest, and turns as the head turns from there.
        let model = load(&SyntheticVrm::avatar());
        let look_at = model.look_at();
        let mut pose = model.tree().rest_pose();
        pose.local[node::HEAD].rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let origin = look_at.origin(&model.tree().world(&pose));
        // Ahead of the turned head is +X.
        let (yaw, pitch) = look_at
            .yaw_pitch(origin, Vec3::new(1.0, 1.46, 0.0))
            .unwrap();
        assert!(close(yaw, 0.0) && close(pitch, 0.0), "{yaw}, {pitch}");
    }

    #[test]
    fn camera_target_uses_the_camera_eye() {
        let model = load(&SyntheticVrm::avatar());
        let camera = Some(LookAtData::Keyword(LookAtKeyword::Camera));
        let (left, right) = eyes_seen_from(
            &model,
            &data(camera.clone(), &[]),
            Vec3::new(0.0, 1.46, 2.0),
        );
        assert_near(left, Vec3::Z);
        assert_near(right, Vec3::Z);
        // Yaw −45: the left eye by the outer map (10°), the right by the
        // inner one (5°), both to the avatar's left (+X).
        let (left, right) = eyes_seen_from(
            &model,
            &data(camera.clone(), &[]),
            Vec3::new(2.0, 1.46, 2.0),
        );
        assert_near(left, turned(10.0, 0.0));
        assert_near(right, turned(5.0, 0.0));
        // A frame's camera is the one its view is made from.
        let mut model = load(&with_look_at(|look_at| {
            look_at["type"] = json!("expression")
        }));
        let point = Some(LookAtData::Point(EYE.to_array()));
        assert_eq!(
            model.frame(0.0, &data(camera, &[])).unwrap(),
            model.frame(0.0, &data(point, &[])).unwrap()
        );
    }

    #[test]
    fn expression_type_sets_look_weights() {
        let vrm = with_look_at(|look_at| {
            look_at["type"] = json!("expression");
            look_at["rangeMapHorizontalOuter"] =
                json!({ "inputMaxValue": 90.0, "outputScale": 1.0 });
            look_at["rangeMapVerticalUp"] = json!({ "inputMaxValue": 90.0, "outputScale": 1.0 });
        });
        let mut model = load(&vrm);
        let gaze = data(angles(-30.0, 20.0), &[]);
        assert!(close(target(&mut model, &gaze, 2), 0.3333333));
        let (_, applied) = model.pose_and_weights(0.0, &gaze, EYE).unwrap();
        let weight = |preset| applied.weights[&ExpressionKey::Preset(preset)];
        assert!(close(weight(Preset::LookUp), 0.2222222));
        assert!(close(weight(Preset::LookLeft), 0.3333333));
        assert_eq!(weight(Preset::LookRight), 0.0);
        assert_eq!(weight(Preset::LookDown), 0.0);
        // The eyes are left alone.
        let (left, right) = eyes(&model, &gaze);
        assert_near(left, Vec3::Z);
        assert_near(right, Vec3::Z);

        // Right and down: lookRight by the outer map too, lookDown by the
        // down map; each clamped to 1.
        let (_, applied) = model
            .pose_and_weights(0.0, &data(angles(45.0, -90.0), &[]), EYE)
            .unwrap();
        let weight = |preset| applied.weights[&ExpressionKey::Preset(preset)];
        assert!(close(weight(Preset::LookRight), 0.5));
        assert_eq!(weight(Preset::LookLeft), 0.0);
        assert_eq!(weight(Preset::LookDown), 1.0);
        assert_eq!(weight(Preset::LookUp), 0.0);

        // The gaze writes the look keys every frame, over what was asked.
        assert_eq!(
            target(&mut model, &data(None, &[("lookLeft", 1.0)]), 2),
            0.0
        );
    }

    #[test]
    fn override_scales_the_gaze() {
        let mut vrm = SyntheticVrm::avatar();
        vrm.expressions["preset"]["happy"]["overrideLookAt"] = json!("blend");
        let model = load(&vrm);
        let (left, _) = eyes(&model, &data(angles(20.0, 0.0), &[("happy", 0.5)]));
        assert!(close(left.x, -0.0193913), "{left}");
        // Blocked, the eyes look ahead.
        let mut vrm = SyntheticVrm::avatar();
        vrm.expressions["preset"]["happy"]["overrideLookAt"] = json!("block");
        let model = load(&vrm);
        let (left, right) = eyes(&model, &data(angles(20.0, -30.0), &[("happy", 0.1)]));
        assert_near(left, Vec3::Z);
        assert_near(right, Vec3::Z);
    }

    #[test]
    fn a_bone_look_at_without_both_eyes_moves_expressions() {
        // UniVRM turns bones only when both eyes are bones; otherwise the
        // look expressions move, by the same range maps.
        let mut vrm = SyntheticVrm::avatar();
        vrm.omit_bone = Some("rightEye".into());
        let mut model = load(&vrm);
        // Outer (90, 20) maps 30 to 6.67, clamped to 1.
        assert_eq!(target(&mut model, &data(angles(-30.0, 0.0), &[]), 2), 1.0);
        let (left, _) = eyes(&model, &data(angles(-30.0, 0.0), &[]));
        assert_near(left, Vec3::Z);
    }

    #[test]
    fn missing_look_at_values_take_univrm_s_defaults() {
        // No lookAt at all: bone, offset (0, 0.06, 0), every map (90, 10).
        let mut vrm = SyntheticVrm::avatar();
        vrm.look_at = Value::Null;
        let model = load(&vrm);
        let (left, right) = eyes(&model, &data(angles(45.0, 0.0), &[]));
        assert_near(left, turned(-5.0, 0.0));
        assert_near(right, turned(-5.0, 0.0));
        let rest = model.tree().world(&model.tree().rest_pose());
        let origin = model.look_at().origin(&rest);
        assert!(origin.abs_diff_eq(Mat4::from_translation(Vec3::new(0.0, 1.46, 0.0)), 1e-6));

        // No type: bone. A map without one of its values: that value's
        // default.
        let model = load(&with_look_at(|look_at| {
            let look_at = look_at.as_object_mut().unwrap();
            look_at.remove("type");
            look_at.remove("offsetFromHeadBone");
            look_at.insert(
                "rangeMapHorizontalInner".into(),
                json!({ "outputScale": 20.0 }),
            );
            look_at.insert(
                "rangeMapHorizontalOuter".into(),
                json!({ "inputMaxValue": 45.0 }),
            );
        }));
        // Inner (90, 20) and outer (45, 10) at 45: 10 and 10.
        let (left, right) = eyes(&model, &data(angles(45.0, 0.0), &[]));
        assert_near(left, turned(-10.0, 0.0));
        assert_near(right, turned(-10.0, 0.0));
    }

    #[test]
    fn an_input_max_of_zero_saturates() {
        // UniVRM's runtime reads 0 as 0, which `max(0.001, …)` saturates;
        // only its editor's `OnValidate` makes it 90.
        let model = load(&with_look_at(|look_at| {
            look_at["rangeMapHorizontalInner"]["inputMaxValue"] = json!(0.0);
        }));
        let (left, _) = eyes(&model, &data(angles(1.0, 0.0), &[]));
        assert_near(left, turned(-10.0, 0.0));
    }

    #[test]
    fn the_offset_moves_the_origin_in_the_head_s_space() {
        // A head scaled by 2 at rest doubles the offset.
        let cache = TempDir::new().unwrap();
        let glb = SyntheticVrm::avatar()
            .glb_with(|root| root["nodes"][node::HEAD]["scale"] = json!([2.0, 2.0, 2.0]));
        let model = VrmModel::load_slice(&glb, cache.path()).unwrap();
        let rest = model.tree().world(&model.tree().rest_pose());
        let origin = model.look_at().origin(&rest);
        assert!(
            origin
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::new(0.0, 1.52, 0.0), 1e-6),
            "{origin}"
        );
        // Its axes are the model's.
        let (yaw, pitch) = model
            .look_at()
            .yaw_pitch(origin, Vec3::new(-1.0, 1.52, 1.0))
            .unwrap();
        assert!(close(yaw, 45.0) && close(pitch, 0.0), "{yaw}, {pitch}");
    }

    /// A change to the lookAt, and what the refusal it makes says.
    type Case = (fn(&mut Value), &'static str);

    #[test]
    fn look_at_values_are_checked() {
        let cases: [Case; 4] = [
            (
                |look_at| look_at["offsetFromHeadBone"] = json!([0.0, 0.06]),
                "lookAt offsetFromHeadBone has fewer than three numbers: [0.0, 0.06]",
            ),
            (
                |look_at| look_at["offsetFromHeadBone"] = json!([0.0, 1e39, 0.0]),
                "lookAt offsetFromHeadBone is not finite",
            ),
            (
                |look_at| look_at["rangeMapVerticalUp"]["inputMaxValue"] = json!(1e39),
                "lookAt rangeMapVerticalUp inputMaxValue is not finite",
            ),
            (
                |look_at| look_at["rangeMapHorizontalOuter"]["outputScale"] = json!(-1e39),
                "lookAt rangeMapHorizontalOuter outputScale is not finite",
            ),
        ];
        for (edit, expected) in cases {
            let message = refusal(&with_look_at(edit));
            assert!(message.contains(expected), "{message}");
        }
        // More than three numbers: the first three.
        let model = load(&with_look_at(|look_at| {
            look_at["offsetFromHeadBone"] = json!([0.0, 0.06, 0.0, 5.0]);
        }));
        let rest = model.tree().world(&model.tree().rest_pose());
        let origin = model.look_at().origin(&rest);
        assert!(origin.abs_diff_eq(Mat4::from_translation(Vec3::new(0.0, 1.46, 0.0)), 1e-6));
    }

    #[test]
    fn targets_that_are_not_finite_are_frame_errors() {
        let mut model = load(&SyntheticVrm::avatar());
        let message = frame_error(&mut model, &data(angles(f32::INFINITY, 0.0), &[]));
        assert!(message.contains("lookAt yaw is not finite"), "{message}");
        let message = frame_error(&mut model, &data(angles(0.0, f32::NAN), &[]));
        assert!(message.contains("lookAt pitch is not finite"), "{message}");
        let point = Some(LookAtData::Point([0.0, f32::NEG_INFINITY, 1.0]));
        let message = frame_error(&mut model, &data(point, &[]));
        assert!(message.contains("lookAt point is not finite"), "{message}");
        // Large but finite angles saturate.
        model
            .frame(0.0, &data(angles(f32::MAX, -f32::MAX), &[]))
            .unwrap();

        // A head scaled to 0 has no origin to look from: a point needs one,
        // angles do not.
        let cache = TempDir::new().unwrap();
        let glb = SyntheticVrm::avatar()
            .glb_with(|root| root["nodes"][node::HEAD]["scale"] = json!([0.0, 0.0, 0.0]));
        let mut model = VrmModel::load_slice(&glb, cache.path()).unwrap();
        model.frame(0.0, &data(angles(10.0, 0.0), &[])).unwrap();
        let point = Some(LookAtData::Point([0.0, 1.46, 1.0]));
        let message = frame_error(&mut model, &data(point, &[]));
        assert!(message.contains("lookAt origin"), "{message}");
    }

    #[test]
    fn look_at_data_reads_its_three_forms() {
        let read =
            |value: Value| serde_json::from_value::<PerformanceData>(json!({ "lookAt": value }));
        assert_eq!(
            read(json!("camera")).unwrap().look_at,
            Some(LookAtData::Keyword(LookAtKeyword::Camera))
        );
        assert_eq!(
            read(json!([1.0, 2.0, 3.0])).unwrap().look_at,
            Some(LookAtData::Point([1.0, 2.0, 3.0]))
        );
        assert_eq!(
            read(json!({ "yaw": 10, "pitch": -5 })).unwrap().look_at,
            angles(10.0, -5.0)
        );
        assert!(read(json!("eyes")).is_err());
        assert!(read(json!([1.0, 2.0])).is_err());
        assert!(read(json!({ "yaw": 10 })).is_err());
        assert!(read(json!({ "yaw": 10, "pitch": 0, "roll": 0 })).is_err());
        assert_eq!(read(Value::Null).unwrap().look_at, None);
    }

    #[test]
    fn look_at_frames_repeat_exactly() {
        let mut model = load(&SyntheticVrm::avatar());
        let gaze = data(Some(LookAtData::Point([0.3, 1.2, 1.0])), &[("happy", 0.4)]);
        let first = model.frame(0.5, &gaze).unwrap();
        assert_eq!(first, model.frame(0.5, &gaze).unwrap());
        let (pose, _) = model.pose_and_weights(0.5, &gaze, EYE).unwrap();
        assert_eq!(pose, model.pose_and_weights(0.5, &gaze, EYE).unwrap().0);
    }
}
