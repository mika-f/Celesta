//! Motions: VRM animation clips (see `vrma`) placed on the composition's
//! timeline.
//!
//! - **Clips** are the layer's assets `motion0`, `motion1`, … (decimal,
//!   without leading zeros), opened with the model. A layer opens at most
//!   [`MAX_CLIPS`] of them, whose files take at most [`CLIP_FILES_LIMIT`]
//!   bytes and whose keys at most [`MOTION_KEYS_BUDGET`] bytes together.
//! - **Order.** Motions run in `from` order (stable for equal `from`). At
//!   `t` the active motion is the last with `from ≤ t`; before the first,
//!   the avatar rests and no clip expression applies. A motion's local time
//!   is `τ = t − from`: looped, `τ mod duration` (0 for a clip of no
//!   length); otherwise `min(τ, duration)`, so its last pose holds until the
//!   next motion.
//! - **Fades.** While `τ < fadeIn`, the pose blends from the motion before
//!   it (evaluated at `t` by these same rules, or the rest pose when there
//!   is none) with `w = τ / fadeIn`: each bone `slerp(q_prev, q_cur, w)`
//!   (identity where one lacks the bone), the hips `lerp` (rest where one
//!   lacks them), expressions `lerp` (0 where one lacks them).
//! - **Spring steps** (see `stepping`) read the bones and the hips alone:
//!   their poses ([`Sampling::Motion`]) neither sample nor blend a clip's
//!   expressions, which only the frame's own pose reads.
//! - **Hips.** Each motion's hips are its clip's travel over its clip's
//!   hips height, so clips recorded on rigs of different sizes blend: the
//!   pose's hips are in a rig whose hips rest at (0, 1, 0), which the
//!   retarget scales to the avatar's.
//! - **Data** is checked every frame, whatever its time: each motion names
//!   a clip the layer opened, its `from` is finite, and its `fadeIn` is
//!   finite and not negative.

use crate::document::{LoadError, read_file};
use crate::model::FrameError;
use crate::vrma::{Clip, ClipPose};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The most clips a layer opens: far past the motions of any video.
pub(crate) const MAX_CLIPS: usize = 256;

/// The most bytes a layer's clip files take together, as the plugin reads
/// a single file.
pub(crate) const CLIP_FILES_LIMIT: u64 = 1 << 30;

/// The most bytes of keys a layer's clips hold together, each clip within
/// its own `CLIP_BUDGET` (see `vrma`).
pub(crate) const MOTION_KEYS_BUDGET: u64 = 1 << 30;

/// The prefix of a clip's asset name.
const CLIP_PREFIX: &str = "motion";

/// A clip placed on the layer's clock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MotionData {
    /// The clip's asset name: `motion0`, `motion1`, ….
    pub clip: String,
    /// When it starts, in seconds of the layer's clock.
    #[serde(default)]
    pub from: f64,
    /// It repeats until the next motion starts; otherwise its last pose
    /// holds.
    #[serde(default, rename = "loop")]
    pub looped: bool,
    /// The seconds over which it blends in from the motion before it.
    #[serde(default)]
    pub fade_in: f64,
}

/// Whether `name` is a clip's asset name: `motion` and a decimal without
/// leading zeros.
pub(crate) fn is_clip_name(name: &str) -> bool {
    name.strip_prefix(CLIP_PREFIX).is_some_and(|digits| {
        !digits.is_empty()
            && digits.bytes().all(|byte| byte.is_ascii_digit())
            && (digits == "0" || !digits.starts_with('0'))
    })
}

/// What a layer's clips may take together.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClipLimits {
    pub count: usize,
    pub file_bytes: u64,
    pub key_bytes: u64,
}

impl ClipLimits {
    /// A layer's: [`MAX_CLIPS`], [`CLIP_FILES_LIMIT`], and
    /// [`MOTION_KEYS_BUDGET`].
    pub(crate) const LAYER: Self = Self {
        count: MAX_CLIPS,
        file_bytes: CLIP_FILES_LIMIT,
        key_bytes: MOTION_KEYS_BUDGET,
    };
}

/// The clips among `assets` (those [`is_clip_name`] accepts) by asset name,
/// within `limits`. The count is checked before any file is read; a file or
/// clip that fails is named by its path.
pub(crate) fn open_clips(
    assets: &BTreeMap<String, PathBuf>,
    limits: ClipLimits,
) -> Result<BTreeMap<String, Clip>, LoadError> {
    let named: Vec<_> = assets
        .iter()
        .filter(|(name, _)| is_clip_name(name))
        .collect();
    if named.len() > limits.count {
        return Err(LoadError::Invalid(format!(
            "the layer opens {} clips, past the {} a VRM layer may",
            named.len(),
            limits.count
        )));
    }
    let (mut file_bytes, mut key_bytes) = (0u64, 0u64);
    let mut clips = BTreeMap::new();
    for (name, path) in named {
        let at = |message: String| LoadError::Invalid(format!("{}: {message}", path.display()));
        let bytes = read_file(path).map_err(|error| at(error.to_string()))?;
        file_bytes = file_bytes.saturating_add(bytes.len() as u64);
        if file_bytes > limits.file_bytes {
            return Err(at(format!(
                "the layer's clip files take {file_bytes} bytes, past the {} a VRM layer's clips may take",
                limits.file_bytes
            )));
        }
        let clip = Clip::load(&bytes).map_err(|error| at(error.to_string()))?;
        key_bytes = key_bytes.saturating_add(clip.key_bytes());
        if key_bytes > limits.key_bytes {
            return Err(at(format!(
                "the layer's clips hold {key_bytes} bytes of keys, past the {} a VRM layer's clips may hold",
                limits.key_bytes
            )));
        }
        clips.insert(name.clone(), clip);
    }
    Ok(clips)
}

/// The rig the motions' poses are in: its hips rest 1 m up at the origin,
/// so a pose's hips are the clip's travel over its hips height.
const UNIT_HIPS: Vec3 = Vec3::Y;

/// What a pose reads of its clips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sampling {
    /// The bones, the hips, and the expressions: a frame's own pose.
    All,
    /// The bones and the hips alone: a spring step's, which reads no
    /// expression, so a clip's expressions cost its steps nothing.
    Motion,
}

/// The pose `motions` give at `time` seconds, as `sampling` reads their
/// clips, and the rest position of the hips of the rig it is in (to
/// retarget it from); `None` before the first motion (or with none), when
/// the avatar rests. Every motion is checked, whatever `time` is.
pub(crate) fn motion_pose(
    clips: &BTreeMap<String, Clip>,
    motions: &[MotionData],
    time: f64,
    sampling: Sampling,
) -> Result<Option<(ClipPose, Vec3)>, FrameError> {
    for (index, motion) in motions.iter().enumerate() {
        if !clips.contains_key(&motion.clip) {
            return Err(FrameError(format!(
                "motion {index} names clip {:?}, which the layer did not open",
                motion.clip
            )));
        }
        if !motion.from.is_finite() {
            return Err(FrameError(format!(
                "motion {index}'s from is not finite: {}",
                motion.from
            )));
        }
        if !(motion.fade_in.is_finite() && motion.fade_in >= 0.0) {
            return Err(FrameError(format!(
                "motion {index}'s fadeIn is {}, not a finite number of seconds, 0 or more",
                motion.fade_in
            )));
        }
    }
    // By `from`, stable: the later of two equal `from`s runs second.
    let mut order: Vec<&MotionData> = motions.iter().collect();
    order.sort_by(|a, b| a.from.partial_cmp(&b.from).expect("from is checked finite"));
    let started = order.partition_point(|motion| motion.from <= time);
    let Some(active) = started.checked_sub(1) else {
        return Ok(None);
    };
    // The motions the active one fades from, down to one that is not
    // fading (or the first, fading from rest): walked, not recursed, so
    // many motions cannot exhaust the stack.
    let mut bottom = active;
    while bottom > 0 && fading(order[bottom], time) {
        bottom -= 1;
    }
    let mut pose: Option<ClipPose> = None;
    for motion in &order[bottom..=active] {
        let current = local_pose(&clips[&motion.clip], motion, time, sampling);
        let tau = time - motion.from;
        pose = Some(if tau < motion.fade_in {
            let previous = pose.unwrap_or_default();
            blend(&previous, &current, (tau / motion.fade_in) as f32)
        } else {
            current
        });
    }
    Ok(pose.map(|pose| (pose, UNIT_HIPS)))
}

/// Whether `motion` is still fading in at `time`.
fn fading(motion: &MotionData, time: f64) -> bool {
    time - motion.from < motion.fade_in
}

/// `motion`'s clip at `time`, as `sampling` reads it, its hips in the unit
/// rig.
fn local_pose(clip: &Clip, motion: &MotionData, time: f64, sampling: Sampling) -> ClipPose {
    let tau = time - motion.from;
    let at = if !motion.looped {
        tau.min(clip.duration)
    } else if clip.duration > 0.0 {
        tau.rem_euclid(clip.duration)
    } else {
        0.0
    };
    let mut pose = match sampling {
        Sampling::All => clip.sample(at),
        Sampling::Motion => clip.sample_motion(at),
    };
    // A clip that moves its hips rests them at least `MIN_HIPS_HEIGHT` up.
    let rest = clip.hips_rest();
    pose.hips = pose.hips.map(|raw| UNIT_HIPS + (raw - rest) / rest.y);
    pose
}

/// `w` of the way from `previous` to `current`: rotations slerped (identity
/// where one lacks a bone), the hips lerped (rest where one lacks them),
/// expressions lerped (0 where one lacks one).
fn blend(previous: &ClipPose, current: &ClipPose, w: f32) -> ClipPose {
    let mut rotations = BTreeMap::new();
    for bone in previous.rotations.keys().chain(current.rotations.keys()) {
        let rotation =
            |pose: &ClipPose| pose.rotations.get(bone).copied().unwrap_or(Quat::IDENTITY);
        rotations.insert(*bone, rotation(previous).slerp(rotation(current), w));
    }
    let hips = match (previous.hips, current.hips) {
        (None, None) => None,
        (from, to) => Some(from.unwrap_or(UNIT_HIPS).lerp(to.unwrap_or(UNIT_HIPS), w)),
    };
    let mut expressions = BTreeMap::new();
    for key in previous
        .expressions
        .keys()
        .chain(current.expressions.keys())
    {
        let weight = |pose: &ClipPose| pose.expressions.get(key).copied().unwrap_or(0.0);
        let (from, to) = (weight(previous), weight(current));
        expressions.insert(key.clone(), from + (to - from) * w);
    }
    ClipPose {
        rotations,
        hips,
        expressions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::humanoid::HumanBone;
    use crate::model::VrmModel;
    use crate::nodes::Pose;
    use crate::provider::PerformanceData;
    use crate::synthetic::{SyntheticVrm, SyntheticVrma, node};
    use glam::Mat4;
    use serde_json::json;
    use std::f32::consts::FRAC_PI_2;
    use tempfile::TempDir;

    /// The synthetic avatar with `clips` (asset name, clip) as files of a
    /// fresh directory, opened with a fresh texture cache.
    fn open(clips: &[(&str, &SyntheticVrma)]) -> (VrmModel, TempDir) {
        let directory = TempDir::new().unwrap();
        let mut assets = BTreeMap::new();
        let path = directory.path().join("avatar.vrm");
        std::fs::write(&path, SyntheticVrm::avatar().glb()).unwrap();
        assets.insert("model".to_owned(), path);
        for (name, clip) in clips {
            let path = directory.path().join(format!("{name}.vrma"));
            std::fs::write(&path, clip.glb()).unwrap();
            assets.insert((*name).to_owned(), path);
        }
        let cache = directory.path().join("cache");
        let model = VrmModel::open_with_cache(&assets, &cache).unwrap();
        (model, directory)
    }

    fn reference() -> (VrmModel, TempDir) {
        open(&[("motion0", &SyntheticVrma::clip())])
    }

    fn data(value: serde_json::Value) -> PerformanceData {
        serde_json::from_value(value).unwrap()
    }

    fn base(model: &VrmModel, time: f64, data: &PerformanceData) -> Pose {
        model.base_pose(time, data).unwrap().0
    }

    fn worlds(model: &VrmModel, time: f64, data: &PerformanceData) -> Vec<Mat4> {
        model.tree().world(&base(model, time, data))
    }

    fn position(worlds: &[Mat4], node: usize) -> Vec3 {
        worlds[node].w_axis.truncate()
    }

    fn near(actual: Vec3, expected: Vec3) -> bool {
        (actual - expected).abs().max_element() < 1e-4
    }

    fn clips(model: &VrmModel) -> &BTreeMap<String, Clip> {
        model.clips()
    }

    /// The pose a frame reads ([`Sampling::All`]).
    fn motion_pose(
        clips: &BTreeMap<String, Clip>,
        motions: &[MotionData],
        time: f64,
    ) -> Result<Option<(ClipPose, Vec3)>, FrameError> {
        super::motion_pose(clips, motions, time, Sampling::All)
    }

    fn arm(pose: &ClipPose) -> Quat {
        pose.rotations[&HumanBone::LeftUpperArm]
    }

    /// The angle between two rotations, in radians: through `asin` of the
    /// difference's vector part, which keeps its precision near zero (f32's
    /// `acos` of a dot product near 1 does not).
    fn angle(a: Quat, b: Quat) -> f32 {
        2.0 * (a * b.inverse()).xyz().length().min(1.0).asin()
    }

    #[test]
    fn motions_start_at_from() {
        let (model, _files) = reference();
        let data = data(json!({ "motions": [{ "clip": "motion0", "from": 2.0 }] }));
        // Before the first motion, the rest pose exactly.
        assert_eq!(base(&model, 1.5, &data), model.tree().rest_pose());
        let lower = position(&worlds(&model, 3.0, &data), node::LEFT_LOWER_ARM);
        assert!(near(lower, Vec3::new(0.15, 1.0, 0.45)), "{lower}");
        let lower = position(&worlds(&model, 2.5, &data), node::LEFT_LOWER_ARM);
        assert!(
            near(lower, Vec3::new(0.3267767, 1.0732233, 0.225)),
            "{lower}"
        );
        // The hips travel as the clip's, scaled to the avatar's height.
        let hips = position(&worlds(&model, 3.0, &data), node::HIPS);
        assert!(near(hips, Vec3::new(0.0, 0.9, 0.45)), "{hips}");
        // No motion at all: the rest pose exactly.
        let still = PerformanceData::default();
        assert_eq!(base(&model, 1.0, &still), model.tree().rest_pose());
    }

    #[test]
    fn looping_and_holding() {
        let (model, _files) = reference();
        let looped = data(json!({ "motions": [{ "clip": "motion0", "from": 0.0, "loop": true }] }));
        assert_eq!(base(&model, 1.25, &looped), base(&model, 0.25, &looped));
        let held = data(json!({ "motions": [{ "clip": "motion0", "from": 0.0 }] }));
        assert_eq!(base(&model, 5.0, &held), base(&model, 1.0, &held));
        assert_ne!(base(&model, 1.25, &held), base(&model, 0.25, &held));
    }

    #[test]
    fn a_clip_of_no_length_holds_its_only_pose() {
        let still = SyntheticVrma {
            arm: vec![(0.0, Quat::from_rotation_z(-FRAC_PI_2))],
            hips: Vec::new(),
            happy: Vec::new(),
            ..SyntheticVrma::clip()
        };
        let (model, _files) = open(&[("motion0", &still)]);
        assert_eq!(clips(&model)["motion0"].duration, 0.0);
        let motions = [MotionData {
            clip: "motion0".into(),
            from: 0.0,
            looped: true,
            fade_in: 0.0,
        }];
        let (pose, _) = motion_pose(clips(&model), &motions, 3.5).unwrap().unwrap();
        let expected = Quat::from_rotation_z(-FRAC_PI_2);
        assert!(angle(arm(&pose), expected) < 1e-4, "{}", arm(&pose));
    }

    #[test]
    fn fade_blends_from_the_previous_motion() {
        let (model, _files) = reference();
        let motions = [
            MotionData {
                clip: "motion0".into(),
                from: 0.0,
                looped: false,
                fade_in: 0.0,
            },
            MotionData {
                clip: "motion0".into(),
                from: 1.0,
                looped: false,
                fade_in: 0.5,
            },
        ];
        // The previous holds Rz(−90°); the new is at Rz(−22.5°); w = 0.5.
        let (pose, _) = motion_pose(clips(&model), &motions, 1.25).unwrap().unwrap();
        let expected = Quat::from_rotation_z(-56.25_f32.to_radians());
        assert!(angle(arm(&pose), expected) < 1e-4, "{}", arm(&pose));
        // Expressions blend too: 1 and 0.25, half way.
        let happy = pose.expressions[&crate::expressions::ExpressionKey::parse("happy")];
        assert!((happy - 0.625).abs() < 1e-6, "{happy}");
        // Once the fade is over, the new motion alone.
        let (after, _) = motion_pose(clips(&model), &motions, 1.5).unwrap().unwrap();
        let (alone, _) = motion_pose(clips(&model), &motions[1..], 1.5)
            .unwrap()
            .unwrap();
        assert_eq!(after, alone);
        // The first motion fades in from rest.
        let from_rest = [MotionData {
            fade_in: 1.0,
            ..motions[0].clone()
        }];
        let (pose, _) = motion_pose(clips(&model), &from_rest, 0.5)
            .unwrap()
            .unwrap();
        let expected = Quat::from_rotation_z(-22.5_f32.to_radians());
        assert!(angle(arm(&pose), expected) < 1e-4, "{}", arm(&pose));
    }

    #[test]
    fn fades_chain_through_motions_that_are_still_fading() {
        let (model, _files) = reference();
        let motion = |from: f64, fade_in: f64| MotionData {
            clip: "motion0".into(),
            from,
            looped: false,
            fade_in,
        };
        // At 1 s: the first fades in from rest (w 0.5, at Rz(−90°)) →
        // Rz(−45°); the second fades from that (w 0.5, at Rz(−22.5°)) →
        // Rz(−33.75°). From the first's own pose it would be Rz(−56.25°).
        let motions = [motion(0.0, 2.0), motion(0.75, 0.5)];
        let (pose, _) = motion_pose(clips(&model), &motions, 1.0).unwrap().unwrap();
        let expected = Quat::from_rotation_z(-33.75_f32.to_radians());
        assert!(angle(arm(&pose), expected) < 1e-4, "{}", arm(&pose));
    }

    #[test]
    fn a_steps_pose_is_the_frames_without_expressions() {
        let (model, _files) = reference();
        let motion = |from: f64, fade_in: f64| MotionData {
            clip: "motion0".into(),
            from,
            looped: true,
            fade_in,
        };
        let motions = [motion(0.0, 0.0), motion(0.3, 1.0), motion(0.6, 1.0)];
        for time in [0.1, 0.45, 0.8, 1.2, 2.0] {
            let (frame, hips) = motion_pose(clips(&model), &motions, time).unwrap().unwrap();
            let (step, step_hips) =
                super::motion_pose(clips(&model), &motions, time, Sampling::Motion)
                    .unwrap()
                    .unwrap();
            assert!(!frame.expressions.is_empty());
            assert!(step.expressions.is_empty(), "at {time} s");
            // The bones and hips bit for bit.
            assert_eq!(step.rotations, frame.rotations, "at {time} s");
            assert_eq!(step.hips, frame.hips, "at {time} s");
            assert_eq!(step_hips, hips);
        }
    }

    #[test]
    fn fades_blend_hips_of_rigs_of_different_sizes() {
        let h = 0.45;
        let small = SyntheticVrma {
            hips_rest_y: h,
            hips: vec![(0.0, Vec3::new(0.0, h, 0.0)), (1.0, Vec3::new(0.0, h, 0.5))],
            ..SyntheticVrma::clip()
        };
        let (model, _files) = open(&[("motion0", &SyntheticVrma::clip()), ("motion1", &small)]);
        // At 2 s motion0 holds 0.5 m of a 1 m rig, motion1 is at 0.5 m of a
        // 0.45 m rig, half faded: (0.5 + 0.5 / 0.45) / 2 of the avatar's
        // 0.9 m.
        let data = data(json!({ "motions": [
            { "clip": "motion0", "from": 0.0 },
            { "clip": "motion1", "from": 1.0, "fadeIn": 2.0 },
        ] }));
        let hips = position(&worlds(&model, 2.0, &data), node::HIPS);
        let travel = (0.5 + 0.5 / h) / 2.0 * 0.9;
        assert!(near(hips, Vec3::new(0.0, 0.9, travel)), "{hips}");
    }

    #[test]
    fn equal_froms_keep_their_order() {
        let (model, _files) = open(&[
            ("motion0", &SyntheticVrma::clip()),
            (
                "motion1",
                &SyntheticVrma {
                    arm: vec![(0.0, Quat::IDENTITY)],
                    ..SyntheticVrma::clip()
                },
            ),
        ]);
        let motion = |clip: &str, from: f64| MotionData {
            clip: clip.into(),
            from,
            looped: false,
            fade_in: 0.0,
        };
        // Sorted by `from`: the later of two equal `from`s is active.
        let motions = [motion("motion1", 0.0), motion("motion0", 0.0)];
        let (pose, _) = motion_pose(clips(&model), &motions, 1.0).unwrap().unwrap();
        let expected = Quat::from_rotation_z(-FRAC_PI_2);
        assert!(angle(arm(&pose), expected) < 1e-4);
        let motions = [
            motion("motion0", 0.5),
            motion("motion1", 0.0),
            motion("motion0", 0.0),
        ];
        let (pose, _) = motion_pose(clips(&model), &motions, 0.25).unwrap().unwrap();
        // motion0 from 0 is active at 0.25 s (motion0 from 0.5 is not yet).
        let expected = Quat::from_rotation_z(-FRAC_PI_2 / 4.0);
        assert!(angle(arm(&pose), expected) < 1e-4, "{}", arm(&pose));
    }

    #[test]
    fn hips_past_f32_are_refused() {
        // The clip's hips 3e38 m out, and a hips track 3e38 m further: each
        // finite, their sum not.
        let far = SyntheticVrma {
            hips: vec![(0.0, Vec3::new(3e38, 1.0, 0.0))],
            ..SyntheticVrma::clip()
        };
        let (model, _files) = open(&[("motion0", &far)]);
        let x = json!({
            "type": "keyframes",
            "keyframes": [{ "time": { "value": 0, "timescale": 30 }, "value": 3e38 }],
        });
        let motions = json!([{ "clip": "motion0" }]);
        // Each alone places the hips.
        model
            .base_pose(0.5, &data(json!({ "motions": motions })))
            .unwrap();
        let track = json!({ "hips": { "x": x } });
        model
            .base_pose(0.5, &data(json!({ "tracks": track })))
            .unwrap();
        let both = data(json!({ "motions": motions, "tracks": track }));
        let message = match model.base_pose(0.5, &both) {
            Ok(_) => panic!("the hips were placed"),
            Err(error) => error.to_string(),
        };
        assert!(
            message
                .contains("the hips' position (the motions' with the hips tracks) is not finite"),
            "{message}"
        );
    }

    #[test]
    fn clip_expressions_yield_to_the_layer() {
        let (mut model, _files) = reference();
        let from_clip = data(json!({ "motions": [{ "clip": "motion0" }] }));
        let frame = model.frame(0.5, &from_clip).unwrap();
        let weight = frame.draws[0].morph_weights[0];
        assert!((weight - 0.5).abs() < 1e-6, "{weight}");
        let from_layer = data(json!({
            "motions": [{ "clip": "motion0" }],
            "expressions": { "happy": 0.2 },
        }));
        let frame = model.frame(0.5, &from_layer).unwrap();
        let weight = frame.draws[0].morph_weights[0];
        assert!((weight - 0.2).abs() < 1e-6, "{weight}");
        // Before the motion, no clip expression applies.
        let later = data(json!({ "motions": [{ "clip": "motion0", "from": 1.0 }] }));
        let frame = model.frame(0.5, &later).unwrap();
        assert_eq!(frame.draws[0].morph_weights[0], 0.0);
    }

    fn refusal(model: &VrmModel, motions: serde_json::Value, time: f64) -> String {
        match model.base_pose(time, &data(json!({ "motions": motions }))) {
            Ok(_) => panic!("the motions were accepted"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn unknown_clip_is_refused() {
        let (model, _files) = reference();
        let message = refusal(&model, json!([{ "clip": "motion3" }]), 0.0);
        assert!(
            message.contains("motion 0 names clip \"motion3\", which the layer did not open"),
            "{message}"
        );
        // Whatever the time: a motion that has not started is checked too.
        let message = refusal(
            &model,
            json!([{ "clip": "motion0" }, { "clip": "walk", "from": 10.0 }]),
            0.0,
        );
        assert!(
            message.contains("motion 1 names clip \"walk\""),
            "{message}"
        );
    }

    #[test]
    fn bad_times_are_refused() {
        let (model, _files) = reference();
        let motion = |from: f64, fade_in: f64| {
            vec![MotionData {
                clip: "motion0".into(),
                from,
                looped: false,
                fade_in,
            }]
        };
        let message_of = |motions: Vec<MotionData>| match motion_pose(clips(&model), &motions, 0.0)
        {
            Ok(_) => panic!("the motions were accepted"),
            Err(error) => error.to_string(),
        };
        for from in [f64::NAN, f64::INFINITY] {
            let message = message_of(motion(from, 0.0));
            assert!(
                message.contains("motion 0's from is not finite"),
                "{message}"
            );
        }
        for fade_in in [-0.5, f64::NAN, f64::INFINITY] {
            let message = message_of(motion(0.0, fade_in));
            assert!(message.contains("motion 0's fadeIn is"), "{message}");
        }
    }

    #[test]
    fn data_reads_what_the_component_sends() {
        // As `<Vrm>`'s `vrmData` writes motions: `loop` and `fadeIn` only
        // when set.
        let data = data(json!({ "motions": [
            { "clip": "motion0", "from": 1 },
            { "clip": "motion2", "from": 2.5, "loop": true, "fadeIn": 0.5 },
        ] }));
        assert_eq!(
            data.motions,
            [
                MotionData {
                    clip: "motion0".into(),
                    from: 1.0,
                    looped: false,
                    fade_in: 0.0,
                },
                MotionData {
                    clip: "motion2".into(),
                    from: 2.5,
                    looped: true,
                    fade_in: 0.5,
                },
            ]
        );
        let unknown = serde_json::from_value::<PerformanceData>(
            json!({ "motions": [{ "clip": "motion0", "speed": 2 }] }),
        );
        assert!(unknown.is_err());
    }

    #[test]
    fn clip_names_are_motion_and_a_decimal() {
        for name in ["motion0", "motion1", "motion12", "motion255"] {
            assert!(is_clip_name(name), "{name}");
        }
        for name in [
            "motion", "motion00", "motion01", "motion-1", "motion1 ", "Motion1", "motion+1",
            "walk", "model",
        ] {
            assert!(!is_clip_name(name), "{name}");
        }
    }

    /// The avatar and `count` copies of the reference clip as assets.
    fn assets(directory: &TempDir, count: usize) -> BTreeMap<String, PathBuf> {
        let model = directory.path().join("avatar.vrm");
        std::fs::write(&model, SyntheticVrm::avatar().glb()).unwrap();
        let clip = directory.path().join("clip.vrma");
        std::fs::write(&clip, SyntheticVrma::clip().glb()).unwrap();
        let mut assets: BTreeMap<String, PathBuf> = (0..count)
            .map(|index| (format!("motion{index}"), clip.clone()))
            .collect();
        assets.insert("model".into(), model);
        assets
    }

    fn open_refusal(assets: &BTreeMap<String, PathBuf>, limits: ClipLimits) -> String {
        match open_clips(assets, limits) {
            Ok(_) => panic!("the clips opened"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn clips_past_their_count_are_refused() {
        let directory = TempDir::new().unwrap();
        let limits = ClipLimits::LAYER;
        assert_eq!(open_clips(&assets(&directory, 3), limits).unwrap().len(), 3);
        let message = open_refusal(&assets(&directory, MAX_CLIPS + 1), limits);
        assert!(
            message.contains(&format!(
                "the layer opens {} clips, past the {MAX_CLIPS} a VRM layer may",
                MAX_CLIPS + 1
            )),
            "{message}"
        );
        // Through the model, with nothing written to the cache.
        let cache = directory.path().join("cache");
        let error = VrmModel::open_with_cache(&assets(&directory, MAX_CLIPS + 1), &cache);
        assert!(error.is_err());
        assert!(!cache.exists());
    }

    #[test]
    fn clips_past_their_bytes_are_refused() {
        let directory = TempDir::new().unwrap();
        let assets = assets(&directory, 2);
        let file = SyntheticVrma::clip().glb().len() as u64;
        let keys = clips_of(&assets)["motion0"].key_bytes();
        let fits = ClipLimits {
            count: 2,
            file_bytes: 2 * file,
            key_bytes: 2 * keys,
        };
        open_clips(&assets, fits).unwrap();
        let message = open_refusal(
            &assets,
            ClipLimits {
                file_bytes: 2 * file - 1,
                ..fits
            },
        );
        assert!(
            message.contains(&format!(
                "the layer's clip files take {} bytes, past the {} a VRM layer's clips may take",
                2 * file,
                2 * file - 1
            )),
            "{message}"
        );
        assert!(message.contains("clip.vrma"), "{message}");
        let message = open_refusal(
            &assets,
            ClipLimits {
                key_bytes: 2 * keys - 1,
                ..fits
            },
        );
        assert!(
            message.contains(&format!(
                "the layer's clips hold {} bytes of keys, past the {} a VRM layer's clips may hold",
                2 * keys,
                2 * keys - 1
            )),
            "{message}"
        );
    }

    fn clips_of(assets: &BTreeMap<String, PathBuf>) -> BTreeMap<String, Clip> {
        open_clips(assets, ClipLimits::LAYER).unwrap()
    }

    #[test]
    fn a_broken_clip_names_its_file() {
        let directory = TempDir::new().unwrap();
        let mut assets = assets(&directory, 1);
        let broken = directory.path().join("broken.vrma");
        std::fs::write(&broken, SyntheticVrm::avatar().glb()).unwrap();
        assets.insert("motion1".into(), broken);
        let message = open_refusal(&assets, ClipLimits::LAYER);
        assert!(message.contains("broken.vrma: "), "{message}");
        assert!(message.contains("not a VRM animation"), "{message}");
    }
}
