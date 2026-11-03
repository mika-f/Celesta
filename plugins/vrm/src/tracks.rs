//! Procedural humanoid tracks: keyframed turns of the avatar's bones, and
//! moves of its hips, on top of the motions (see `motion`).
//!
//! - **Keys** are VRM humanoid bone names (`head`, `leftUpperArm`, …),
//!   each with up to six axes, each a [`KeyframeAnimation`] on the layer's
//!   clock read at the frame's time to the microsecond, as the Live2D
//!   plugin reads its tracks. A bone the avatar lacks is skipped.
//! - **Rotations** are in degrees, in the bone's normalized frame (its turn
//!   from rest in the model's axes, as a clip's): `q ← q · Ry(−yaw) ·
//!   Rx(−pitch) · Rz(roll)`, after the motion, in glTF's axes (Unity's
//!   `Euler(pitch, yaw, roll)` order). The angles turn as the lookAt's do
//!   (UniVRM's): with the avatar facing +Z, its right toward −X, a positive
//!   yaw turns the face to the avatar's right, a positive pitch lifts it to
//!   look up, and a positive roll tilts its top toward the avatar's right.
//! - **The hips** alone take `x`, `y`, and `z`: metres added to their
//!   position in model space, which the caller has put in `pose.hips` (the
//!   rest position when no motion moves them).
//! - **Data** is checked every frame, whatever its time: bone names,
//!   axes, and each keyframe (a time with a timescale, times in order,
//!   values finite as `f32`), each refusal naming its key. The spring
//!   steps a frame runs read the tracks the frame checked, so each step
//!   costs only the reading.

use crate::humanoid::HumanBone;
use crate::model::FrameError;
use crate::vrma::ClipPose;
use celesta_composition::{KeyframeAnimation, Time, keyframe_segment};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A bone's tracks: turns in degrees, and (on the hips) moves in metres.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackData {
    #[serde(default)]
    pub pitch: Option<KeyframeAnimation<f64>>,
    #[serde(default)]
    pub yaw: Option<KeyframeAnimation<f64>>,
    #[serde(default)]
    pub roll: Option<KeyframeAnimation<f64>>,
    #[serde(default)]
    pub x: Option<KeyframeAnimation<f64>>,
    #[serde(default)]
    pub y: Option<KeyframeAnimation<f64>>,
    #[serde(default)]
    pub z: Option<KeyframeAnimation<f64>>,
}

impl TrackData {
    /// Its six axes by name, in pitch, yaw, roll, x, y, z order.
    fn axes(&self) -> [(&'static str, &Option<KeyframeAnimation<f64>>); 6] {
        [
            ("pitch", &self.pitch),
            ("yaw", &self.yaw),
            ("roll", &self.roll),
            ("x", &self.x),
            ("y", &self.y),
            ("z", &self.z),
        ]
    }

    /// For each axis it has, its keyframes, which a reading walks.
    pub(crate) fn keyframe_counts(&self) -> impl Iterator<Item = u64> + '_ {
        self.axes()
            .into_iter()
            .filter_map(|(_, animation)| animation.as_ref())
            .map(|animation| animation.keyframes.len() as u64)
    }
}

/// Applies `tracks` at `time` seconds to `pose`: each bone's turn after its
/// rotation (identity when the pose lacks the bone), and the hips' move
/// added to `pose.hips` when it is set. Every track is checked first.
pub(crate) fn apply_tracks(
    tracks: &BTreeMap<String, TrackData>,
    time: f64,
    pose: &mut ClipPose,
) -> Result<(), FrameError> {
    check_tracks(tracks)?;
    apply_checked_tracks(tracks, time, pose)
}

/// Checks `tracks` whatever the time: bone names, axes, and keyframes.
pub(crate) fn check_tracks(tracks: &BTreeMap<String, TrackData>) -> Result<(), FrameError> {
    for (name, track) in tracks {
        let Some(bone) = HumanBone::from_name(name) else {
            return Err(FrameError(format!(
                "tracks name {name:?}, which is not a humanoid bone"
            )));
        };
        for (axis, animation) in track.axes() {
            let Some(animation) = animation else {
                continue;
            };
            if bone != HumanBone::Hips && matches!(axis, "x" | "y" | "z") {
                return Err(FrameError(format!(
                    "tracks.{name}.{axis}: only hips takes x, y, z tracks"
                )));
            }
            check_keyframes(&format!("tracks.{name}.{axis}"), animation)?;
        }
    }
    Ok(())
}

/// [`apply_tracks`] for `tracks` that [`check_tracks`] passed: what a
/// spring step runs, its request having checked them once.
pub(crate) fn apply_checked_tracks(
    tracks: &BTreeMap<String, TrackData>,
    time: f64,
    pose: &mut ClipPose,
) -> Result<(), FrameError> {
    let at = track_time(time);
    for (name, track) in tracks {
        let Some(bone) = HumanBone::from_name(name) else {
            continue;
        };
        let mut values = [None; 6];
        for (slot, (axis, animation)) in values.iter_mut().zip(track.axes()) {
            if let Some(animation) = animation {
                *slot = Some(value(&format!("tracks.{name}.{axis}"), animation, at)?);
            }
        }
        let [pitch, yaw, roll, x, y, z] = values;
        if pitch.is_some() || yaw.is_some() || roll.is_some() {
            let degrees = |value: Option<f32>| value.unwrap_or(0.0).to_radians();
            // glTF's +Y and +X turn the face to the avatar's left and down,
            // so yaw and pitch turn the other way, as the lookAt's do.
            let turn = Quat::from_rotation_y(-degrees(yaw))
                * Quat::from_rotation_x(-degrees(pitch))
                * Quat::from_rotation_z(degrees(roll));
            let rotation = pose.rotations.entry(bone).or_insert(Quat::IDENTITY);
            *rotation *= turn;
        }
        if (x.is_some() || y.is_some() || z.is_some())
            && let Some(hips) = &mut pose.hips
        {
            *hips += Vec3::new(x.unwrap_or(0.0), y.unwrap_or(0.0), z.unwrap_or(0.0));
        }
    }
    Ok(())
}

/// `seconds` as an exact timeline time, to the microsecond, as the Live2D
/// plugin reads its tracks.
fn track_time(seconds: f64) -> Time {
    Time::new((seconds * 1_000_000.0).round() as i64, 1_000_000)
}

/// Checks `animation`'s keyframes (at `key`): at least one, each time with
/// a timescale and none before the one before it, each value finite as an
/// `f32`.
fn check_keyframes(key: &str, animation: &KeyframeAnimation<f64>) -> Result<(), FrameError> {
    let keyframes = &animation.keyframes;
    if keyframes.is_empty() {
        return Err(FrameError(format!("{key} has no keyframes")));
    }
    for (index, keyframe) in keyframes.iter().enumerate() {
        if keyframe.time.timescale == 0 {
            return Err(FrameError(format!(
                "{key} keyframe {index}'s time has a timescale of 0"
            )));
        }
        if !(keyframe.value.is_finite() && (keyframe.value as f32).is_finite()) {
            return Err(FrameError(format!(
                "{key} keyframe {index}'s value is not a finite number: {:?}",
                keyframe.value
            )));
        }
        if index > 0 {
            let before = keyframes[index - 1].time;
            // Both timescales are checked non-zero, so they compare.
            if keyframe
                .time
                .cmp_exact(before)
                .is_ok_and(|order| order.is_lt())
            {
                return Err(FrameError(format!(
                    "{key} keyframe {index} is before keyframe {}: keyframes are in time order",
                    index - 1
                )));
            }
        }
    }
    Ok(())
}

/// `animation` (at `key`), its keyframes checked, at `at`.
fn value(key: &str, animation: &KeyframeAnimation<f64>, at: Time) -> Result<f32, FrameError> {
    let (from, to, eased) = keyframe_segment(animation, at)
        .map_err(|error| FrameError(format!("{key} cannot be read at {at:?}: {error}")))?;
    let value = (from + (to - from) * eased) as f32;
    // An easing that overshoots can carry a value past `f32`.
    if !value.is_finite() {
        return Err(FrameError(format!(
            "{key} is not finite at {} s",
            at.value as f64 / 1e6
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VrmModel;
    use crate::provider::PerformanceData;
    use crate::synthetic::{SyntheticVrm, SyntheticVrma, node};
    use glam::Mat4;
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// The synthetic avatar, and the reference clip as `motion0`, opened
    /// from a fresh directory with a fresh texture cache.
    fn open() -> (VrmModel, TempDir) {
        let directory = TempDir::new().unwrap();
        let model = directory.path().join("avatar.vrm");
        std::fs::write(&model, SyntheticVrm::avatar().glb()).unwrap();
        let clip = directory.path().join("clip.vrma");
        std::fs::write(&clip, SyntheticVrma::clip().glb()).unwrap();
        let assets: BTreeMap<String, PathBuf> =
            [("model".to_owned(), model), ("motion0".to_owned(), clip)].into();
        let cache = directory.path().join("cache");
        (
            VrmModel::open_with_cache(&assets, &cache).unwrap(),
            directory,
        )
    }

    /// Keyframes `(seconds, value)` as `<Vrm>` sends them at 30 fps.
    fn keys(keys: &[(f64, f64)]) -> Value {
        let keyframes: Vec<Value> = keys
            .iter()
            .map(|&(seconds, value)| {
                let frame = (seconds * 30.0).round() as i64;
                json!({ "time": { "value": frame, "timescale": 30 }, "value": value })
            })
            .collect();
        json!({ "type": "keyframes", "keyframes": keyframes })
    }

    fn data(value: Value) -> PerformanceData {
        serde_json::from_value(value).unwrap()
    }

    fn worlds(model: &VrmModel, time: f64, data: &PerformanceData) -> Vec<Mat4> {
        model.tree().world(&model.base_pose(time, data).unwrap().0)
    }

    fn position(worlds: &[Mat4], node: usize) -> Vec3 {
        worlds[node].w_axis.truncate()
    }

    fn near(actual: Vec3, expected: Vec3) -> bool {
        (actual - expected).abs().max_element() < 1e-4
    }

    /// Where the head's world rotation takes `direction`.
    fn head(model: &VrmModel, time: f64, data: &PerformanceData, direction: Vec3) -> Vec3 {
        worlds(model, time, data)[node::HEAD].transform_vector3(direction)
    }

    #[test]
    fn head_tracks_turn_the_head() {
        let (model, _files) = open();
        let turn =
            data(json!({ "tracks": { "head": { "yaw": keys(&[(0.0, 0.0), (1.0, 90.0)]) } } }));
        // Yaw to the avatar's right, which is −X: 45° at 0.5 s, 90° from 1 s.
        let face = head(&model, 0.5, &turn, Vec3::Z);
        let half = std::f32::consts::FRAC_1_SQRT_2;
        assert!(near(face, Vec3::new(-half, 0.0, half)), "{face}");
        let face = head(&model, 2.0, &turn, Vec3::Z);
        assert!(near(face, Vec3::NEG_X), "{face}");
    }

    #[test]
    fn pitch_looks_up_and_roll_tilts_right() {
        let (model, _files) = open();
        let pitch = data(json!({ "tracks": { "head": { "pitch": keys(&[(0.0, 90.0)]) } } }));
        let face = head(&model, 0.0, &pitch, Vec3::Z);
        assert!(near(face, Vec3::Y), "{face}");
        let top = head(&model, 0.0, &pitch, Vec3::Y);
        assert!(near(top, Vec3::NEG_Z), "{top}");
        // The avatar's right is −X.
        let roll = data(json!({ "tracks": { "head": { "roll": keys(&[(0.0, 90.0)]) } } }));
        let top = head(&model, 0.0, &roll, Vec3::Y);
        assert!(near(top, Vec3::NEG_X), "{top}");
        // Yaw, then pitch, then roll, each in the frame the one before left:
        // yaw 90° turns the face to −X and the avatar's left side (+X) to
        // +Z, then pitch 90° lifts the face up about that side.
        let both = data(json!({ "tracks": { "head": {
            "yaw": keys(&[(0.0, 90.0)]),
            "pitch": keys(&[(0.0, 90.0)]),
        } } }));
        let face = head(&model, 0.0, &both, Vec3::Z);
        assert!(near(face, Vec3::Y), "{face}");
        let side = head(&model, 0.0, &both, Vec3::X);
        assert!(near(side, Vec3::Z), "{side}");
        let top = head(&model, 0.0, &both, Vec3::Y);
        assert!(near(top, Vec3::X), "{top}");
    }

    #[test]
    fn tracks_turn_as_the_look_at_does() {
        let (model, _files) = open();
        // A head yaw of +30 turns the face toward −X, the avatar's right.
        let turn = data(json!({ "tracks": { "head": { "yaw": keys(&[(0.0, 30.0)]) } } }));
        let face = head(&model, 0.0, &turn, Vec3::Z);
        assert!(face.x < 0.0, "{face}");
        // A lookAt yaw of +30 turns both eyes the same way.
        let gaze = data(json!({ "lookAt": { "yaw": 30, "pitch": 0 } }));
        let (pose, _) = model
            .pose_and_weights(0.0, &gaze, Vec3::new(0.0, 0.8, 3.3))
            .unwrap();
        let worlds = model.tree().world(&pose);
        for eye in [node::LEFT_EYE, node::RIGHT_EYE] {
            let forward = worlds[eye].transform_vector3(Vec3::Z);
            assert!(forward.x < 0.0, "eye {eye}: {forward}");
            assert_eq!(forward.x.signum(), face.x.signum());
        }
        // And a pitch of +20 lifts both up.
        let nod = data(json!({ "tracks": { "head": { "pitch": keys(&[(0.0, 20.0)]) } } }));
        let face = head(&model, 0.0, &nod, Vec3::Z);
        assert!(face.y > 0.0, "{face}");
        let gaze = data(json!({ "lookAt": { "yaw": 0, "pitch": 20 } }));
        let (pose, _) = model
            .pose_and_weights(0.0, &gaze, Vec3::new(0.0, 0.8, 3.3))
            .unwrap();
        let worlds = model.tree().world(&pose);
        for eye in [node::LEFT_EYE, node::RIGHT_EYE] {
            let forward = worlds[eye].transform_vector3(Vec3::Z);
            assert!(forward.y > 0.0, "eye {eye}: {forward}");
        }
    }

    #[test]
    fn tracks_compose_after_the_motion() {
        let (model, _files) = open();
        // The clip lowers the arm by Rz(−90°) at 1 s; a roll of 90° brings it
        // back up, while the hips keep the clip's travel.
        let data = data(json!({
            "motions": [{ "clip": "motion0", "from": 0 }],
            "tracks": { "leftUpperArm": { "roll": keys(&[(0.0, 90.0)]) } },
        }));
        let lower = position(&worlds(&model, 1.0, &data), node::LEFT_LOWER_ARM);
        assert!(near(lower, Vec3::new(0.40, 1.25, 0.45)), "{lower}");
    }

    #[test]
    fn hips_tracks_move_the_avatar() {
        let (model, _files) = open();
        let lifted = data(json!({ "tracks": { "hips": { "y": keys(&[(0.0, 0.1)]) } } }));
        let hips = position(&worlds(&model, 0.0, &lifted), node::HIPS);
        assert!(near(hips, Vec3::new(0.0, 1.0, 0.0)), "{hips}");
        // In the avatar's metres, on top of the clip's travel.
        let moved = data(json!({
            "motions": [{ "clip": "motion0" }],
            "tracks": { "hips": { "x": keys(&[(0.0, 0.2)]), "z": keys(&[(0.0, -0.05)]) } },
        }));
        let hips = position(&worlds(&model, 1.0, &moved), node::HIPS);
        assert!(near(hips, Vec3::new(0.2, 0.9, 0.4)), "{hips}");
    }

    #[test]
    fn bones_the_avatar_lacks_are_skipped() {
        let (model, _files) = open();
        // The synthetic avatar has no jaw.
        let jaw = data(json!({ "tracks": { "jaw": { "pitch": keys(&[(0.0, 30.0)]) } } }));
        let still = worlds(&model, 0.0, &PerformanceData::default());
        let posed = worlds(&model, 0.0, &jaw);
        for (node, (posed, still)) in posed.iter().zip(&still).enumerate() {
            assert!(posed.abs_diff_eq(*still, 1e-5), "node {node}");
        }
    }

    fn refusal(model: &VrmModel, tracks: Value) -> String {
        match model.base_pose(0.0, &data(json!({ "tracks": tracks }))) {
            Ok(_) => panic!("the tracks were accepted"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn bad_tracks_are_refused() {
        let (model, _files) = open();
        let message = refusal(&model, json!({ "tail": { "yaw": keys(&[(0.0, 1.0)]) } }));
        assert!(
            message.contains("tracks name \"tail\", which is not a humanoid bone"),
            "{message}"
        );
        let message = refusal(&model, json!({ "head": { "x": keys(&[(0.0, 1.0)]) } }));
        assert!(
            message.contains("tracks.head.x: only hips takes x, y, z tracks"),
            "{message}"
        );
        let empty = json!({ "type": "keyframes", "keyframes": [] });
        let message = refusal(&model, json!({ "head": { "yaw": empty } }));
        assert!(
            message.contains("tracks.head.yaw has no keyframes"),
            "{message}"
        );
        let no_timescale = json!({ "type": "keyframes", "keyframes": [
            { "time": { "value": 0, "timescale": 0 }, "value": 0 },
        ] });
        let message = refusal(&model, json!({ "head": { "yaw": no_timescale } }));
        assert!(
            message.contains("tracks.head.yaw keyframe 0's time has a timescale of 0"),
            "{message}"
        );
        let message = refusal(
            &model,
            json!({ "head": { "pitch": keys(&[(0.0, 0.0), (1.0, 1e39)]) } }),
        );
        assert!(
            message.contains("tracks.head.pitch keyframe 1's value is not a finite number: 1e39"),
            "{message}"
        );
        let message = refusal(
            &model,
            json!({ "hips": { "y": keys(&[(1.0, 0.0), (0.5, 0.0)]) } }),
        );
        assert!(
            message.contains("tracks.hips.y keyframe 1 is before keyframe 0"),
            "{message}"
        );
        // Whatever the frame's time: a track past it is checked too.
        let message = refusal(
            &model,
            json!({ "head": { "roll": keys(&[(0.0, 0.0), (9.0, f64::MAX)]) } }),
        );
        assert!(
            message.contains("tracks.head.roll keyframe 1's value"),
            "{message}"
        );
        // An axis the track does not have is not data at all.
        let twist = serde_json::from_value::<PerformanceData>(
            json!({ "tracks": { "head": { "twist": keys(&[(0.0, 1.0)]) } } }),
        );
        assert!(twist.is_err());
    }

    #[test]
    fn data_reads_what_the_component_sends() {
        // As `<Vrm>`'s `vrmData` writes tracks at 30 fps.
        let data = data(json!({ "tracks": {
            "head": { "yaw": {
                "type": "keyframes",
                "keyframes": [
                    { "time": { "value": 0, "timescale": 30 }, "value": 0 },
                    { "time": { "value": 30, "timescale": 30 }, "value": 20, "easing": "ease-in-out" },
                ],
            } },
            "hips": { "y": { "type": "keyframes", "keyframes": [{ "time": { "value": 0, "timescale": 30 }, "value": 0.1 }] } },
        } }));
        let yaw = data.tracks["head"].yaw.as_ref().unwrap();
        assert_eq!(yaw.keyframes.len(), 2);
        assert_eq!(yaw.keyframes[1].time, Time::new(30, 30));
        assert_eq!(yaw.keyframes[1].value, 20.0);
        let hips = &data.tracks["hips"];
        assert_eq!(hips.y.as_ref().unwrap().keyframes[0].value, 0.1);
        assert_eq!(hips.x, None);
        // Times to the microsecond, as at 29.97 fps: half way at 0.5005 s.
        let micro = data_at_micro();
        let mut pose = ClipPose::default();
        apply_tracks(&micro, 0.5005005, &mut pose).unwrap();
        // A yaw of 10° to the avatar's right: −10° about glTF's +Y.
        let expected = Quat::from_rotation_y(-10_f32.to_radians());
        let turn = pose.rotations[&HumanBone::Head];
        assert!(turn.abs_diff_eq(expected, 1e-6), "{turn}");
    }

    fn data_at_micro() -> BTreeMap<String, TrackData> {
        let tracks = json!({ "head": { "yaw": {
            "type": "keyframes",
            "keyframes": [
                { "time": { "value": 0, "timescale": 1_000_000 }, "value": 0 },
                { "time": { "value": 1_001_001, "timescale": 1_000_000 }, "value": 20 },
            ],
        } } });
        serde_json::from_value(tracks).unwrap()
    }
}
