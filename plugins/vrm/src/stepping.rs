//! The spring bones' clock: a fixed 60 Hz step from the layer's 0 s, with
//! checkpoints, so a frame depends only on the model, its time, and its
//! `data`, whatever was asked before it and at whatever frame rate.
//!
//! - **Clock:** a frame at `t` has run steps `0..=n`, `n = steps_until(t)`;
//!   step `k` is at `k / 60` s with `Δt = 1/60`. Before step 0 the tails
//!   start still from the pose at 0 s; step 0 integrates once, as UniVRM's
//!   first frame does. The frame then takes its own pose at `t` and turns
//!   the heads to step `n`'s tails (`Springs::settle`), so a time between
//!   steps shows the last step's tails on the current pose.
//! - **What a step sees:** the pose at `k / 60` from the motions and tracks,
//!   and the constraints on it, never the step before's turned pose. The
//!   lookAt and expressions are a frame's own values, which earlier steps
//!   cannot be given; in a VRM 1.0 avatar they turn no bone but the eyes,
//!   so a spring hanging from an eye bone swings as if the eye rested.
//! - **Checkpoints** only save work: a request resumes from the latest
//!   state kept at or before its step (else from step 0) and keeps the ones
//!   it passes, after each step `k` with `(k + 1) % (30 × spacing) == 0`.
//!   They are keyed by `data`'s motions and tracks, everything the steps
//!   read (the springs' settings are the model's). A model keeps at most
//!   [`CHECKPOINT_KEYS`] keys, dropping the least recently used.
//! - **Bounds:** a key keeps states within [`CHECKPOINT_BYTES`]; past them
//!   its spacing doubles and the states off it are dropped, so the kept
//!   ones still cover every time asked for. A frame past
//!   [`MAX_SPRING_STEPS`] is refused: a request from a cold start runs every
//!   step up to its time.

use crate::model::{FrameError, VrmModel};
use crate::provider::PerformanceData;
use crate::springs::SpringState;
use glam::Vec3;
use std::collections::BTreeMap;
use std::mem::size_of;

/// The spring bones' steps a second.
pub const SPRING_HZ: f64 = 60.0;

/// The steps between two kept states of a performance.
pub const CHECKPOINT_STEPS: u64 = 30;

/// The performances (motions and tracks) a model keeps states of.
pub const CHECKPOINT_KEYS: usize = 8;

/// The most steps a frame runs: four hours of the layer at [`SPRING_HZ`].
/// A frame further in is refused, since a request from a cold start (or one
/// whose performance was dropped) steps all the way from 0 s.
pub const MAX_SPRING_STEPS: u64 = 4 * 3600 * 60;

/// The most bytes of states one performance keeps: past them, its states
/// thin out (see the module). Eight performances keep 256 MiB at most.
pub const CHECKPOINT_BYTES: u64 = 32 << 20;

/// The step's length.
const DT: f32 = (1.0 / SPRING_HZ) as f32;

/// The spring bones' state at any time, from the states kept.
pub(crate) struct Stepper {
    keys: BTreeMap<String, Checkpoints>,
    /// Counts requests; each key's `last_used` is the request that last
    /// read it.
    clock: u64,
    /// [`MAX_SPRING_STEPS`], or a test's.
    max_steps: u64,
    /// [`CHECKPOINT_BYTES`], or a test's.
    checkpoint_bytes: u64,
}

/// One performance's kept states.
struct Checkpoints {
    /// By step: the state after it.
    after_step: BTreeMap<u64, SpringState>,
    last_used: u64,
    /// Kept states are [`CHECKPOINT_STEPS`] × `spacing` steps apart.
    spacing: u64,
}

impl Default for Stepper {
    fn default() -> Self {
        Self::with_limits(MAX_SPRING_STEPS, CHECKPOINT_BYTES)
    }
}

impl Stepper {
    /// A stepper refusing frames past `max_steps` and keeping at most
    /// `checkpoint_bytes` of states per performance.
    pub(crate) fn with_limits(max_steps: u64, checkpoint_bytes: u64) -> Self {
        Self {
            keys: BTreeMap::new(),
            clock: 0,
            max_steps,
            checkpoint_bytes,
        }
    }

    /// The springs of `model` after the steps a frame at `time` seconds has
    /// run, for `data`'s motions and tracks. The model has springs.
    pub(crate) fn state_at(
        &mut self,
        model: &VrmModel,
        time: f64,
        data: &PerformanceData,
    ) -> Result<SpringState, FrameError> {
        let last = steps_until(time);
        if last > self.max_steps {
            return Err(FrameError(format!(
                "this frame is {time} s into the layer, past the {} s up to which the VRM plugin \
                 steps spring bones ({} steps at {SPRING_HZ} Hz)",
                self.max_steps as f64 / SPRING_HZ,
                self.max_steps
            )));
        }
        let key = serde_json::to_string(&(&data.motions, &data.tracks))
            .map_err(|error| FrameError(format!("the motions and tracks: {error}")))?;
        let budget = self.checkpoint_bytes;
        let springs = model.springs();
        let tree = model.tree();
        let pose_at = |step: u64| {
            model
                .step_pose(step as f64 / SPRING_HZ, data)
                .map_err(|error| FrameError(format!("spring bones' step {step}: {}", error.0)))
        };

        let checkpoints = self.checkpoints(key);
        let (mut step, mut state) = match checkpoints.after_step.range(..=last).next_back() {
            Some((&kept, state)) => (kept + 1, state.clone()),
            None => (0, springs.initial_state(tree, &pose_at(0)?)),
        };
        let state_bytes = (state.current.len() * 2 * size_of::<Vec3>()) as u64;
        let kept = usize::try_from(budget / state_bytes.max(1)).unwrap_or(usize::MAX);
        while step <= last {
            let mut pose = pose_at(step)?;
            springs.step(tree, &mut pose, &mut state, DT);
            if kept > 0 && (step + 1) % CHECKPOINT_STEPS.saturating_mul(checkpoints.spacing) == 0 {
                checkpoints.after_step.insert(step, state.clone());
                checkpoints.thin(kept);
            }
            step += 1;
        }
        Ok(state)
    }

    /// The states kept for `key`, made the most recently used; a new key
    /// drops the least recently used past [`CHECKPOINT_KEYS`].
    fn checkpoints(&mut self, key: String) -> &mut Checkpoints {
        self.clock += 1;
        if !self.keys.contains_key(&key) && self.keys.len() >= CHECKPOINT_KEYS {
            let oldest = self
                .keys
                .iter()
                .min_by_key(|(_, checkpoints)| checkpoints.last_used)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.keys.remove(&oldest);
            }
        }
        let checkpoints = self.keys.entry(key).or_insert_with(|| Checkpoints {
            after_step: BTreeMap::new(),
            last_used: 0,
            spacing: 1,
        });
        checkpoints.last_used = self.clock;
        checkpoints
    }

    /// The performances whose states are kept.
    #[cfg(test)]
    pub(crate) fn keys(&self) -> usize {
        self.keys.len()
    }

    /// How many states `data`'s performance keeps, if it is kept.
    #[cfg(test)]
    pub(crate) fn kept(&self, data: &PerformanceData) -> Option<usize> {
        let key = serde_json::to_string(&(&data.motions, &data.tracks)).unwrap();
        self.keys
            .get(&key)
            .map(|checkpoints| checkpoints.after_step.len())
    }
}

impl Checkpoints {
    /// Doubles the spacing, dropping the states off it, until at most
    /// `kept` states are left.
    fn thin(&mut self, kept: usize) {
        while self.after_step.len() > kept {
            self.spacing = self.spacing.saturating_mul(2);
            let every = CHECKPOINT_STEPS.saturating_mul(self.spacing);
            self.after_step.retain(|step, _| (step + 1) % every == 0);
        }
    }
}

/// The last step a frame at `time` seconds has run: `⌊max(time, 0) × 60⌋`,
/// with a microsecond's leeway so a frame at `k / 30` s runs step `2k`.
pub(crate) fn steps_until(time: f64) -> u64 {
    (time.max(0.0) * SPRING_HZ + 1e-6).floor() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{SyntheticVrm, SyntheticVrma, node};
    use celesta_plugin::SceneFrame;
    use glam::Mat4;
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use tempfile::TempDir;
    use vrm_spec::vrmc_spring_bone_1_0::VRMC_SPRING_BONE;

    /// The hair spring: `[hair0, hair1, hair2]`, each joint with stiffness
    /// 0.5, drag 0.4, and gravity power 1.
    fn hair() -> Value {
        let joint = |node: usize| json!({ "node": node, "stiffness": 0.5, "dragForce": 0.4, "gravityPower": 1.0 });
        json!({
            "specVersion": "1.0",
            "springs": [{ "joints": [joint(node::HAIR0), joint(node::HAIR1), joint(node::HAIR2)] }],
        })
    }

    /// `vrm` written to a fresh directory with the reference clip as
    /// `motion0`, opened with a fresh texture cache.
    fn open_vrm(vrm: &SyntheticVrm) -> (VrmModel, TempDir) {
        let directory = TempDir::new().unwrap();
        let model = directory.path().join("avatar.vrm");
        std::fs::write(&model, vrm.glb()).unwrap();
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

    /// The synthetic avatar with the hair spring.
    fn open() -> (VrmModel, TempDir) {
        let mut vrm = SyntheticVrm::avatar();
        vrm.extensions.insert(VRMC_SPRING_BONE.into(), hair());
        open_vrm(&vrm)
    }

    fn data(value: Value) -> PerformanceData {
        serde_json::from_value(value).unwrap()
    }

    /// A: the clip, looped.
    fn a() -> PerformanceData {
        data(json!({ "motions": [{ "clip": "motion0", "loop": true }] }))
    }

    /// B: A with the head turned 30° by a track.
    fn b() -> PerformanceData {
        let yaw = json!({
            "type": "keyframes",
            "keyframes": [{ "time": { "value": 0, "timescale": 30 }, "value": 30.0 }],
        });
        data(json!({
            "motions": [{ "clip": "motion0", "loop": true }],
            "tracks": { "head": { "yaw": yaw } },
        }))
    }

    /// The frame and every node's world, springs included.
    fn render(model: &mut VrmModel, time: f64, data: &PerformanceData) -> (SceneFrame, Vec<Mat4>) {
        model.render(time, data).unwrap()
    }

    /// The hair nodes' worlds.
    fn hair_worlds(model: &mut VrmModel, time: f64, data: &PerformanceData) -> [Mat4; 3] {
        let (_, worlds) = render(model, time, data);
        [node::HAIR0, node::HAIR1, node::HAIR2].map(|node| worlds[node])
    }

    #[test]
    fn springs_do_not_depend_on_frame_order() {
        let times = [2.0, 0.5, 3.3, 0.5, 1.0];
        let (mut warmed, _warmed) = open();
        render(&mut warmed, 4.0, &b());
        let scrubbed: Vec<_> = times
            .iter()
            .map(|&time| render(&mut warmed, time, &a()))
            .collect();

        let (mut fresh, _fresh) = open();
        let mut increasing = times;
        increasing.sort_by(f64::total_cmp);
        let in_order: BTreeMap<u64, _> = increasing
            .iter()
            .map(|&time| (time.to_bits(), render(&mut fresh, time, &a())))
            .collect();
        for (time, frame) in times.iter().zip(&scrubbed) {
            assert!(*frame == in_order[&time.to_bits()], "at {time} s");
        }
        // A model asked for one frame only agrees too.
        let (mut cold, _cold) = open();
        assert!(render(&mut cold, 3.3, &a()) == scrubbed[2]);

        // The hair did swing.
        let rest = warmed.tree().world(&warmed.tree().rest_pose());
        assert_ne!(scrubbed[0].1[node::HAIR1], rest[node::HAIR1]);
    }

    #[test]
    fn thirty_and_sixty_fps_agree() {
        let (mut thirty, _thirty) = open();
        let (mut sixty, _sixty) = open();
        let at_thirty: Vec<_> = (0..60)
            .map(|k| render(&mut thirty, f64::from(k) / 30.0, &a()))
            .collect();
        let at_sixty: Vec<_> = (0..120)
            .map(|k| render(&mut sixty, f64::from(k) / 60.0, &a()))
            .collect();
        for k in 0..60 {
            assert_eq!(f64::from(k) / 30.0, f64::from(2 * k) / 60.0);
            assert!(
                at_thirty[k as usize] == at_sixty[2 * k as usize],
                "frame {k}"
            );
        }
    }

    #[test]
    fn checkpoints_are_keyed_by_motions_and_tracks() {
        let (mut model, _files) = open();
        let first = render(&mut model, 2.0, &a());
        let turned = render(&mut model, 2.0, &b());
        assert_ne!(first.1[node::HAIR1], turned.1[node::HAIR1]);
        assert!(render(&mut model, 2.0, &a()) == first);
        assert_eq!(model.stepper().keys(), 2);

        // Nine performances: the least recently used, B, is dropped.
        let starting = |quarter: u32| {
            data(json!({
                "motions": [{ "clip": "motion0", "loop": true, "from": f64::from(quarter) / 4.0 }],
            }))
        };
        for quarter in 1..=7 {
            render(&mut model, 0.5, &starting(quarter));
        }
        assert_eq!(model.stepper().keys(), CHECKPOINT_KEYS);
        assert!(model.stepper().kept(&b()).is_none());
        assert!(model.stepper().kept(&a()).is_some());
        // A used again outlives the next oldest.
        render(&mut model, 0.5, &a());
        render(&mut model, 0.5, &starting(8));
        assert_eq!(model.stepper().keys(), CHECKPOINT_KEYS);
        assert!(model.stepper().kept(&a()).is_some());
        assert!(model.stepper().kept(&starting(1)).is_none());
    }

    #[test]
    fn per_frame_values_leave_the_springs_alone() {
        let (mut model, _files) = open();
        let plain = hair_worlds(&mut model, 2.0, &a());
        let mut gazing = a();
        gazing.expressions = [("happy".to_owned(), 1.0)].into();
        gazing.look_at = Some(crate::provider::LookAtData::Angles {
            yaw: 30.0,
            pitch: 0.0,
        });
        let (mut other, _other) = open();
        assert_eq!(hair_worlds(&mut other, 2.0, &gazing), plain);
        // They share the performance's states.
        assert_eq!(hair_worlds(&mut model, 2.0, &gazing), plain);
        assert_eq!(model.stepper().keys(), 1);
    }

    #[test]
    fn between_steps_the_last_step_holds() {
        let (mut model, _files) = open();
        let still = PerformanceData::default();
        let one = hair_worlds(&mut model, 1.0 / 60.0, &still);
        assert_eq!(hair_worlds(&mut model, 1.0 / 60.0 + 0.004, &still), one);
        // Gravity moves it from one step to the next.
        assert_ne!(hair_worlds(&mut model, 2.0 / 60.0, &still), one);
    }

    #[test]
    fn steps_until_counts_whole_steps() {
        assert_eq!(steps_until(0.0), 0);
        assert_eq!(steps_until(1.0), 60);
        assert_eq!(steps_until(1.0 / 3.0), 20);
        assert_eq!(steps_until(-1.0), 0);
        assert_eq!(steps_until(f64::NAN), 0);
        assert_eq!(steps_until(f64::INFINITY), u64::MAX);
    }

    #[test]
    fn frames_past_the_last_step_are_refused() {
        let (mut model, _files) = open();
        let past = MAX_SPRING_STEPS as f64 / SPRING_HZ + 1.0 / SPRING_HZ;
        for time in [past, f64::INFINITY] {
            let error = model.render(time, &a()).unwrap_err().0;
            assert!(
                error.contains("past the 14400 s up to which the VRM plugin steps spring bones"),
                "{error}"
            );
        }
        *model.stepper_mut() = Stepper::with_limits(120, CHECKPOINT_BYTES);
        render(&mut model, 2.0, &a());
        assert!(model.render(2.0 + 1.0 / SPRING_HZ, &a()).is_err());

        // An avatar without springs has no clock to run.
        let (mut still, _still) = open_vrm(&SyntheticVrm::avatar());
        render(&mut still, past, &a());
    }

    #[test]
    fn kept_states_per_performance_are_bounded() {
        // Two moving joints: 48 bytes a state, so four states.
        let (mut bounded, _bounded) = open();
        *bounded.stepper_mut() = Stepper::with_limits(MAX_SPRING_STEPS, 4 * 48);
        let late = render(&mut bounded, 10.0, &a());
        let kept = bounded.stepper().kept(&a()).unwrap();
        assert!((1..=4).contains(&kept), "{kept}");

        let (mut fresh, _fresh) = open();
        assert!(late == render(&mut fresh, 10.0, &a()));
        for time in [7.3, 0.5, 9.9] {
            assert!(
                render(&mut bounded, time, &a()) == render(&mut fresh, time, &a()),
                "at {time} s"
            );
        }
        assert!(bounded.stepper().kept(&a()).unwrap() <= 4);
        // Unbounded, the 600 steps keep 20.
        assert_eq!(fresh.stepper().kept(&a()), Some(20));
    }
}
