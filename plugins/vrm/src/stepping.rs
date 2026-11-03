//! The spring bones' clock: a fixed 60 Hz step from the layer's 0 s, with
//! kept states, so a frame depends only on the model, its time, and its
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
//! - **Kept states** only save work; no frame depends on them. A
//!   performance (a model's motions and tracks, everything the steps read:
//!   the springs' settings are the model's) keeps a **cursor**, the state
//!   its last request ended on, which each request replaces, and **grid
//!   checkpoints**, the states after each step `k` with
//!   `(k + 1) % (30 × spacing) == 0`. A request resumes from the later of
//!   its cursor and the latest checkpoint at or before its step (else from
//!   step 0), so sequential frames step only the steps between them and a
//!   scrub back resumes at most `30 × spacing` steps before its step.
//!   Performances are keyed by a SHA-256 of their motions and tracks.
//! - **Memory:** the plugin's process keeps at most [`SPRING_CACHE_BYTES`]
//!   of states over every model and performance, a ceiling against hostile
//!   files that real compositions stay orders of magnitude below. Past it,
//!   it gives up latency, never output, in this order: the performance with
//!   the most checkpoints doubles its spacing (dropping the states off it),
//!   repeatedly; then checkpoints go, cursors stay; then the least recently
//!   used performances go. A request's own performance is out of the cache
//!   while it steps, so it is never evicted by its own request, and keeps
//!   checkpoints only within what the others leave; its cursor is always
//!   kept. So the states kept are at most the ceiling plus one cursor.
//! - **Bounds:** a request from a cold start runs every step up to its
//!   time, so a frame is refused, before any state is looked up, past
//!   [`MAX_SPRING_STEPS`] or when its steps would take more than
//!   [`MAX_STEP_WORK`]: `(n + 1) × cost`, the cost of a step being the
//!   model's ([`step_cost`], fixed when it opens) plus its data's (its
//!   motions' clip channels and its tracks' axes and keyframes, which
//!   every step reads, charged as measured). Both depend on the model, the data, and the time alone, never
//!   on what is kept, so a frame refused cold is refused warm. A model
//!   that cannot step four hours this way says how far it can when it
//!   opens ([`reach_notice`]).

use crate::model::{FrameError, VrmModel};
use crate::provider::PerformanceData;
use crate::springs::SpringState;
use crate::tracks::{TrackData, check_tracks};
use celesta_plugin::{Notice, NoticeLevel};
use glam::Vec3;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::mem::size_of;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// The spring bones' steps a second.
pub const SPRING_HZ: f64 = 60.0;

/// The steps between two checkpoints of a performance, before any thinning.
pub const CHECKPOINT_STEPS: u64 = 30;

/// The most steps a frame runs: four hours of the layer at [`SPRING_HZ`].
/// A frame further in is refused, since a request from a cold start (or one
/// whose performance was dropped) steps all the way from 0 s.
pub const MAX_SPRING_STEPS: u64 = 4 * 3600 * 60;

/// The most work a frame's steps may take, in [`step_cost`]'s units: 2^33.
///
/// A unit measured 2.8 to 5.1 ns of a laptop core (Apple M5, release
/// build) on Seed-san, UniVRM's constraint twist sample, and a VRoid
/// avatar, with and without a clip, so the worst frame accepted from a cold
/// start takes about 45 s, under a minute. It is set that high so real
/// avatars keep [`MAX_SPRING_STEPS`] with room for motions and tracks:
/// Seed-san (610 units a step) reaches 65 h alone and 27 h with a clip
/// playing, the twist sample (2,183) 18 h and 13 h, while a file of a
/// million empty nodes steps about 2.4 min of a layer.
pub const MAX_STEP_WORK: u64 = 1 << 33;

/// What any step takes beyond its nodes, joints, pairs, and climbs, in
/// [`step_cost`]'s units.
const STEP_BASE_COST: u64 = 300;

/// What a step pays for each motion, per channel of its clip and one for
/// the motion, in [`step_cost`]'s units. Measured (release build, 600
/// steps, the best of five): a looped 53-channel clip added 2.5 to 3.4 µs
/// a step to Seed-san, the twist sample, and a VRoid avatar, and a second
/// one fading with it 3.3 to 3.6 µs (sampling, blending, retargeting).
/// Divided by each model's own nanoseconds a unit without data, that is
/// 640 to 1,320 units a motion, 12 to 25 a channel: 16 keeps it within
/// 2×.
const MOTION_CHANNEL_COST: u64 = 16;

/// What a step pays for each track axis, beyond its keyframes. Measured as
/// above: a one-keyframe track added 0.07 to 0.22 µs a step, 23 to 59
/// units.
const TRACK_AXIS_COST: u64 = 32;

/// How many track keyframes a step pays one unit for. Measured as above:
/// a thousand keyframes added 0.39 to 0.48 µs a step, 0.08 to 0.17 units a
/// keyframe, since reading a track walks its keyframes.
const KEYFRAMES_PER_UNIT: u64 = 8;

/// The most bytes of spring states the plugin's process keeps, over every
/// model and performance. A ceiling against hostile files, not a size for
/// real ones: a ten-minute layer keeps about 1 MB, so it binds only for
/// spring chains of millions of joints or hundreds of hour-long
/// performances, and then costs latency only (see the module).
pub const SPRING_CACHE_BYTES: u64 = 256 << 20;

/// The step's length.
const DT: f32 = (1.0 / SPRING_HZ) as f32;

/// What a kept performance takes beyond its states: its key and its entry.
const PERFORMANCE_BYTES: u64 = 128;

/// A performance's key: the SHA-256 of its motions and tracks as JSON.
type PerformanceKey = [u8; 32];

/// The spring states the plugin's process keeps, shared by its models.
#[derive(Clone, Default)]
pub(crate) struct SharedSpringCache(Arc<Mutex<SpringCache>>);

impl SharedSpringCache {
    /// A cache keeping at most `ceiling` bytes of states (see the module).
    #[cfg(test)]
    pub(crate) fn with_ceiling(ceiling: u64) -> Self {
        Self(Arc::new(Mutex::new(SpringCache::with_ceiling(ceiling))))
    }

    /// The cache. Its states are whole whenever a lock is released, so one
    /// poisoned by a panic is still sound to read.
    fn lock(&self) -> MutexGuard<'_, SpringCache> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Every model's kept performances, by model and key.
pub(crate) struct SpringCache {
    performances: BTreeMap<(u64, PerformanceKey), Performance>,
    /// What `performances` takes, by [`Performance::bytes`].
    bytes: u64,
    /// [`SPRING_CACHE_BYTES`], or a test's.
    ceiling: u64,
    /// Counts requests; each performance's `last_used` is the request that
    /// last read it.
    clock: u64,
    /// The models that have joined: the last one's id.
    models: u64,
}

/// One performance's kept states.
struct Performance {
    /// By step: the state after it.
    grid: BTreeMap<u64, SpringState>,
    /// The step the last request ended on, and the state after it.
    cursor: Option<(u64, SpringState)>,
    /// Checkpoints are [`CHECKPOINT_STEPS`] × `spacing` steps apart.
    spacing: u64,
    last_used: u64,
    /// What one of its states takes.
    state_bytes: u64,
}

impl Default for SpringCache {
    fn default() -> Self {
        Self::with_ceiling(SPRING_CACHE_BYTES)
    }
}

impl SpringCache {
    fn with_ceiling(ceiling: u64) -> Self {
        Self {
            performances: BTreeMap::new(),
            bytes: 0,
            ceiling,
            clock: 0,
            models: 0,
        }
    }

    /// Takes `id`'s performance out, if it is kept.
    fn take(&mut self, id: &(u64, PerformanceKey)) -> Option<Performance> {
        let performance = self.performances.remove(id)?;
        self.bytes -= performance.bytes();
        Some(performance)
    }

    /// Puts `performance` back as `id`'s, the most recently used, then
    /// gives up states past the ceiling (see the module), `id`'s last.
    fn put(&mut self, id: (u64, PerformanceKey), mut performance: Performance) {
        self.clock += 1;
        performance.last_used = self.clock;
        self.bytes += performance.bytes();
        self.performances.insert(id, performance);
        while self.bytes > self.ceiling {
            // The performance with the most checkpoints: its spacing
            // doubles, or its last checkpoint goes.
            let widest = self
                .performances
                .iter()
                .filter(|(_, performance)| !performance.grid.is_empty())
                .max_by_key(|(_, performance)| performance.grid.len())
                .map(|(id, _)| *id);
            if let Some(widest) = widest {
                let performance = self.performances.get_mut(&widest).expect("a kept id");
                self.bytes -= performance.bytes();
                if performance.grid.len() > 1 {
                    performance.widen();
                } else {
                    performance.grid.clear();
                }
                self.bytes += performance.bytes();
                continue;
            }
            // Only cursors are left: the least recently used goes.
            let oldest = self
                .performances
                .iter()
                .filter(|(kept, _)| **kept != id)
                .min_by_key(|(_, performance)| performance.last_used)
                .map(|(id, _)| *id);
            match oldest {
                Some(oldest) => {
                    self.take(&oldest);
                }
                None => break,
            }
        }
    }

    /// Drops every performance of `model`.
    fn forget(&mut self, model: u64) {
        let mut bytes = 0;
        self.performances.retain(|(of, _), performance| {
            let keep = *of != model;
            if !keep {
                bytes += performance.bytes();
            }
            keep
        });
        self.bytes -= bytes;
    }
}

impl Performance {
    fn new(state_bytes: u64) -> Self {
        Self {
            grid: BTreeMap::new(),
            cursor: None,
            spacing: 1,
            last_used: 0,
            state_bytes,
        }
    }

    /// What it takes: its states, its key, and its entry.
    fn bytes(&self) -> u64 {
        let states = self.grid.len() as u64 + u64::from(self.cursor.is_some());
        PERFORMANCE_BYTES + states * self.state_bytes
    }

    /// The latest state kept at or before step `last`: the later of its
    /// cursor and its checkpoints.
    fn resume(&self, last: u64) -> Option<(u64, SpringState)> {
        let checkpoint = self.grid.range(..=last).next_back();
        let cursor = self
            .cursor
            .as_ref()
            .filter(|(step, _)| *step <= last)
            .map(|(step, state)| (step, state));
        [checkpoint, cursor]
            .into_iter()
            .flatten()
            .max_by_key(|(step, _)| **step)
            .map(|(&step, state)| (step, state.clone()))
    }

    /// Whether the state after `step` is a checkpoint at its spacing.
    fn on_grid(&self, step: u64) -> bool {
        (step + 1).is_multiple_of(CHECKPOINT_STEPS.saturating_mul(self.spacing))
    }

    /// Doubles the spacing, dropping the checkpoints off it, until at least
    /// one is dropped.
    fn widen(&mut self) {
        let before = self.grid.len();
        while self.grid.len() >= before && !self.grid.is_empty() {
            self.spacing = self.spacing.saturating_mul(2);
            let every = CHECKPOINT_STEPS.saturating_mul(self.spacing);
            self.grid.retain(|step, _| (step + 1).is_multiple_of(every));
        }
    }

    /// Widens the spacing until at most `kept` checkpoints are left.
    fn thin(&mut self, kept: usize) {
        while self.grid.len() > kept {
            self.widen();
        }
    }
}

/// The spring bones' state at any time, from the states a model keeps in
/// its process's cache.
pub(crate) struct Stepper {
    cache: SharedSpringCache,
    /// The model's id in `cache`.
    model: u64,
    /// [`MAX_SPRING_STEPS`], or a test's.
    max_steps: u64,
    /// [`MAX_STEP_WORK`], or a test's.
    max_work: u64,
    /// The steps run, for tests.
    #[cfg(test)]
    steps: std::cell::Cell<u64>,
}

impl Default for Stepper {
    /// A stepper with a cache of its own.
    fn default() -> Self {
        Self::new(&SharedSpringCache::default())
    }
}

impl Drop for Stepper {
    fn drop(&mut self) {
        self.cache.lock().forget(self.model);
    }
}

impl Stepper {
    /// A stepper keeping its states in `cache`.
    pub(crate) fn new(cache: &SharedSpringCache) -> Self {
        Self::with_limits(cache, MAX_SPRING_STEPS, MAX_STEP_WORK)
    }

    /// A stepper keeping its states in `cache`, refusing frames past
    /// `max_steps` or whose steps take more than `max_work`.
    pub(crate) fn with_limits(cache: &SharedSpringCache, max_steps: u64, max_work: u64) -> Self {
        let model = {
            let mut cache = cache.lock();
            cache.models += 1;
            cache.models
        };
        Self {
            cache: cache.clone(),
            model,
            max_steps,
            max_work,
            #[cfg(test)]
            steps: std::cell::Cell::new(0),
        }
    }

    /// The springs of `model` after the steps a frame at `time` seconds has
    /// run, for `data`'s motions and tracks. The model has springs.
    pub(crate) fn state_at(
        &self,
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
        let cost = model.step_cost().saturating_add(data_cost(model, data));
        let work = (last + 1).saturating_mul(cost);
        if work > self.max_work {
            return Err(FrameError(format!(
                "this frame is {time} s into the layer, past {}: its {} steps of {cost} work units \
                 each take {work}, more than the {} a frame may take",
                reach(cost, self.max_work).map_or_else(
                    || "any frame the VRM plugin steps this avatar's spring bones to".to_owned(),
                    |seconds| format!(
                        "the {seconds} s up to which the VRM plugin steps this avatar's spring \
                         bones with these motions and tracks"
                    )
                ),
                last + 1,
                self.max_work
            )));
        }
        // Checked once here, so the steps only read them.
        check_tracks(&data.tracks)?;
        let id = (self.model, performance_key(data)?);
        let springs = model.springs();
        let tree = model.tree();
        let pose_at = |step: u64| {
            model
                .step_pose(step as f64 / SPRING_HZ, data)
                .map_err(|error| FrameError(format!("spring bones' step {step}: {}", error.0)))
        };

        let mut cache = self.cache.lock();
        // The state to start from, found before the cache changes: a
        // performance that fails at step 0 leaves it as it was.
        let resume = cache
            .performances
            .get(&id)
            .and_then(|performance| performance.resume(last));
        let (mut step, mut state) = match resume {
            Some((kept, state)) => (kept + 1, state),
            None => (0, springs.initial_state(tree, &pose_at(0)?)),
        };
        let mut performance = cache
            .take(&id)
            .unwrap_or_else(|| Performance::new(state_bytes(&state)));
        // Checkpoints within what the other performances leave of the
        // ceiling, with room for the cursor.
        let room = cache
            .ceiling
            .saturating_sub(cache.bytes + PERFORMANCE_BYTES + performance.state_bytes);
        let kept = usize::try_from(room / performance.state_bytes.max(1)).unwrap_or(usize::MAX);
        let stepped = loop {
            if step > last {
                break Ok(());
            }
            let mut pose = match pose_at(step) {
                Ok(pose) => pose,
                Err(error) => break Err(error),
            };
            springs.step(tree, &mut pose, &mut state, DT);
            #[cfg(test)]
            self.steps.set(self.steps.get() + 1);
            if kept > 0 && performance.on_grid(step) {
                performance.grid.insert(step, state.clone());
                performance.thin(kept);
            }
            step += 1;
        };
        if stepped.is_ok() {
            performance.cursor = Some((last, state.clone()));
        }
        if performance.cursor.is_some() || !performance.grid.is_empty() {
            cache.put(id, performance);
        }
        stepped.map(|()| state)
    }

    /// The steps run so far.
    #[cfg(test)]
    pub(crate) fn steps(&self) -> u64 {
        self.steps.get()
    }

    /// The model's performances kept.
    #[cfg(test)]
    pub(crate) fn performances(&self) -> usize {
        let cache = self.cache.lock();
        cache
            .performances
            .keys()
            .filter(|(model, _)| *model == self.model)
            .count()
    }

    /// How many checkpoints `data`'s performance keeps, and its cursor's
    /// step, if it is kept.
    #[cfg(test)]
    pub(crate) fn kept(&self, data: &PerformanceData) -> Option<(usize, Option<u64>)> {
        let id = (self.model, performance_key(data).unwrap());
        let cache = self.cache.lock();
        cache.performances.get(&id).map(|performance| {
            (
                performance.grid.len(),
                performance.cursor.as_ref().map(|(step, _)| *step),
            )
        })
    }

    /// What the whole cache takes, checked against its states.
    #[cfg(test)]
    pub(crate) fn cache_bytes(&self) -> u64 {
        let cache = self.cache.lock();
        let counted: u64 = cache.performances.values().map(Performance::bytes).sum();
        assert_eq!(cache.bytes, counted, "the cache's running total drifted");
        cache.bytes
    }
}

/// A spring step's work, in units of a few nanoseconds, for a model of
/// `nodes` nodes whose springs move `joints` joints and check `pairs`
/// joint–collider pairs, and whose aim constraints climb `climbed` nodes:
/// `300 + nodes + joints + 3 × pairs + climbed`. A step clones every
/// node's pose (the springs' transforms place only the nodes they read,
/// but the pose is the whole tree's), climbs for each aim, moves each
/// joint, and checks each pair (about three units). Within a factor of two
/// of what the steps take, which is all a fence needs.
pub(crate) fn step_cost(nodes: u64, joints: u64, pairs: u64, climbed: u64) -> u64 {
    STEP_BASE_COST
        .saturating_add(nodes)
        .saturating_add(joints)
        .saturating_add(pairs.saturating_mul(3))
        .saturating_add(climbed)
}

/// What `data` adds to each step's work, as measured (see
/// [`MOTION_CHANNEL_COST`], [`TRACK_AXIS_COST`], [`KEYFRAMES_PER_UNIT`]):
/// for each motion, [`MOTION_CHANNEL_COST`] × (1 + its clip's channels),
/// since a step may sample every motion in a fade; for each track axis,
/// [`TRACK_AXIS_COST`] plus a unit per [`KEYFRAMES_PER_UNIT`] keyframes
/// (rounded up).
fn data_cost(model: &VrmModel, data: &PerformanceData) -> u64 {
    let motions = data.motions.iter().fold(0u64, |sum, motion| {
        let channels = 1 + model.clip_channels(&motion.clip);
        sum.saturating_add(channels.saturating_mul(MOTION_CHANNEL_COST))
    });
    data.tracks
        .values()
        .flat_map(TrackData::keyframe_counts)
        .fold(motions, |sum, keyframes| {
            sum.saturating_add(TRACK_AXIS_COST + keyframes.div_ceil(KEYFRAMES_PER_UNIT))
        })
}

/// The latest time, in seconds floored to the hundredth, a frame reaches
/// when each step costs `cost` and a frame may take `max_work`: steps
/// `0..=n` with `(n + 1) × cost ≤ max_work`. `None` when not even step 0
/// fits.
fn reach(cost: u64, max_work: u64) -> Option<f64> {
    let last = (max_work / cost.max(1)).checked_sub(1)?;
    Some((last * 100 / SPRING_HZ as u64) as f64 / 100.0)
}

/// A warning, for a model with springs whose steps cost `cost` before any
/// motion or track, that its frames reach less than [`MAX_SPRING_STEPS`]
/// under [`MAX_STEP_WORK`]; `None` when they reach it.
pub(crate) fn reach_notice(cost: u64) -> Option<Notice> {
    let last = (MAX_STEP_WORK / cost.max(1)).checked_sub(1);
    if last.is_some_and(|last| last >= MAX_SPRING_STEPS) {
        return None;
    }
    let reach = reach(cost, MAX_STEP_WORK).map_or_else(
        || "no frame of a layer".to_owned(),
        |seconds| format!("frames up to {seconds} s into a layer"),
    );
    Some(Notice {
        level: NoticeLevel::Warning,
        message: format!(
            "this avatar's spring bones are stepped for {reach} (less with motions and \
             tracks), short of the {} s the VRM plugin steps other avatars for: each step takes \
             {cost} work units for its nodes, spring joints, collider checks, and aim \
             constraints, and a frame may take {MAX_STEP_WORK}; later frames are refused",
            MAX_SPRING_STEPS / SPRING_HZ as u64
        ),
    })
}

/// `data`'s performance key: the SHA-256 of its motions and tracks as JSON,
/// whose numbers serde_json writes to round-trip, so equal keys are equal
/// data (and data that means the same written differently, as `-0.0` and
/// `0.0`, only misses the cache).
fn performance_key(data: &PerformanceData) -> Result<PerformanceKey, FrameError> {
    let mut hasher = Sha256::new();
    serde_json::to_writer(&mut hasher, &(&data.motions, &data.tracks))
        .map_err(|error| FrameError(format!("the motions and tracks: {error}")))?;
    Ok(hasher.finalize().into())
}

/// What one kept state takes: its tails, its vectors, and its step.
fn state_bytes(state: &SpringState) -> u64 {
    let tails = (state.prev.len() + state.current.len()) * size_of::<Vec3>();
    (tails + size_of::<SpringState>() + size_of::<u64>()) as u64
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

    /// [`open`], keeping its states in `cache`.
    fn open_sharing(cache: &SharedSpringCache) -> (VrmModel, TempDir) {
        let (mut model, files) = open();
        model.share_spring_cache(cache);
        (model, files)
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

    /// The clip, looped from `index` eighths of a second: a performance of
    /// its own for each index.
    fn starting(index: u32) -> PerformanceData {
        data(json!({
            "motions": [{ "clip": "motion0", "loop": true, "from": f64::from(index) / 8.0 }],
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
    fn states_are_kept_by_motions_and_tracks() {
        let (mut model, _files) = open();
        let first = render(&mut model, 2.0, &a());
        let turned = render(&mut model, 2.0, &b());
        assert_ne!(first.1[node::HAIR1], turned.1[node::HAIR1]);
        assert!(render(&mut model, 2.0, &a()) == first);
        assert_eq!(model.stepper().performances(), 2);
        // 120 steps: four checkpoints, and the cursor on step 120.
        assert_eq!(model.stepper().kept(&a()), Some((4, Some(120))));

        // However many performances there are, none is dropped.
        for index in 1..=20 {
            render(&mut model, 0.5, &starting(index));
        }
        assert_eq!(model.stepper().performances(), 22);
        assert!(model.stepper().kept(&b()).is_some());
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
        assert_eq!(model.stepper().performances(), 1);
    }

    #[test]
    fn a_gaze_does_not_reach_the_steps() {
        // The hair's tails are kept in the left eye's space, so a gaze the
        // steps saw would move them.
        let open_eyed = || {
            let mut spring_bone = hair();
            spring_bone["springs"][0]["center"] = json!(node::LEFT_EYE);
            let mut vrm = SyntheticVrm::avatar();
            vrm.extensions.insert(VRMC_SPRING_BONE.into(), spring_bone);
            open_vrm(&vrm)
        };
        let mut gazing = a();
        gazing.look_at = Some(crate::provider::LookAtData::Angles {
            yaw: 30.0,
            pitch: 20.0,
        });
        let (mut plain, _plain) = open_eyed();
        let (mut looking, _looking) = open_eyed();
        // The frame's eye does turn.
        let eye = |model: &mut VrmModel, data: &PerformanceData| {
            render(model, 2.0, data).1[node::LEFT_EYE]
        };
        assert_ne!(eye(&mut plain, &a()), eye(&mut looking, &gazing));
        let (fresh_plain, _a) = open_eyed();
        let (fresh_looking, _b) = open_eyed();
        let state = |model: &VrmModel, data: &PerformanceData| {
            model.stepper().state_at(model, 2.0, data).unwrap()
        };
        assert_eq!(state(&fresh_plain, &a()), state(&fresh_looking, &gazing));
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
    fn sequential_frames_step_only_the_delta() {
        let (mut model, _files) = open();
        let mut before = 0;
        for frame in 0..1800 {
            let time = f64::from(frame) / 30.0;
            render(&mut model, time, &a());
            let steps = model.stepper().steps() - before;
            before += steps;
            // The first frame runs step 0; each later one, the two since.
            assert_eq!(steps, if frame == 0 { 1 } else { 2 }, "frame {frame}");
        }
        assert_eq!(model.stepper().steps(), steps_until(1799.0 / 30.0) + 1);
        // A scrub back resumes from a checkpoint, and forward again from
        // the cursor it left.
        render(&mut model, 30.0, &a());
        assert!(model.stepper().steps() - before <= CHECKPOINT_STEPS);
        before = model.stepper().steps();
        render(&mut model, 30.0 + 1.0 / 30.0, &a());
        assert_eq!(model.stepper().steps() - before, 2);
    }

    /// Twelve performances on one model, asked in turn at every frame of
    /// 10 s at 30 fps, as an export of twelve layers of one file asks them:
    /// the total of steps each performance runs, and the most one request
    /// runs.
    fn interleaved(model: &mut VrmModel) -> (u64, u64) {
        let performances: Vec<PerformanceData> = (0..12).map(starting).collect();
        let mut most = 0;
        for frame in 0..300 {
            let time = f64::from(frame) / 30.0;
            for performance in &performances {
                let before = model.stepper().steps();
                render(model, time, performance);
                most = most.max(model.stepper().steps() - before);
            }
        }
        (model.stepper().steps(), most)
    }

    #[test]
    fn interleaved_performances_keep_requests_small() {
        let each = steps_until(299.0 / 30.0) + 1;
        let (mut model, _files) = open();
        assert_eq!(interleaved(&mut model), (12 * each, 2));
        let kept = model.stepper().cache_bytes();
        assert!(kept <= SPRING_CACHE_BYTES, "{kept}");
        assert_eq!(model.stepper().performances(), 12);

        // A ceiling that holds the twelve cursors and little else, with one
        // performance's checkpoints already filling it: the checkpoints go,
        // the cursors stay, and no request grows.
        let one = (4 * size_of::<Vec3>() + size_of::<SpringState>() + size_of::<u64>()) as u64;
        let ceiling = 12 * (PERFORMANCE_BYTES + one) + one;
        let cache = SharedSpringCache::with_ceiling(ceiling);
        let (mut tight, _tight) = open_sharing(&cache);
        render(&mut tight, 299.0 / 30.0, &starting(0));
        assert_eq!(tight.stepper().kept(&starting(0)), Some((19, Some(598))));
        // (A warm checkpoint past a cursor can save a step.)
        let (total, most) = interleaved(&mut tight);
        assert!(total <= each + 12 * each && most <= 2, "{total}, {most}");
        let kept = tight.stepper().cache_bytes();
        assert!(kept <= ceiling, "{kept} > {ceiling}");
        assert_eq!(tight.stepper().performances(), 12);
    }

    /// Frames of three performances on `model`, scrubbed back and forth.
    fn scrubbed(model: &mut VrmModel) -> Vec<(SceneFrame, Vec<Mat4>)> {
        let mut frames = Vec::new();
        for time in [3.0, 0.5, 2.0, 2.0 + 1.0 / 30.0, 9.9, 1.0, 10.0, 0.0] {
            for performance in [a(), b(), starting(3)] {
                frames.push(render(model, time, &performance));
            }
        }
        frames
    }

    #[test]
    fn the_ceiling_costs_latency_only() {
        let (mut uncapped, _uncapped) = open();
        let expected = scrubbed(&mut uncapped);
        // 4 KB: every cursor stays, the checkpoints thin. 100 B: less than
        // one performance, so each request evicts the others.
        for ceiling in [4096, 100] {
            let cache = SharedSpringCache::with_ceiling(ceiling);
            let (mut capped, _capped) = open_sharing(&cache);
            let frames = scrubbed(&mut capped);
            for (index, frame) in frames.iter().enumerate() {
                assert!(
                    *frame == expected[index],
                    "ceiling {ceiling}, frame {index}"
                );
            }
            let kept = capped.stepper().cache_bytes();
            let performances = capped.stepper().performances();
            if ceiling == 4096 {
                assert!(kept <= ceiling, "{kept}");
                assert_eq!(performances, 3);
            } else {
                // Only the last request's performance, over the ceiling by
                // its cursor.
                assert_eq!(performances, 1);
                assert_eq!(capped.stepper().kept(&starting(3)), Some((0, Some(0))));
            }
        }
    }

    #[test]
    fn models_share_the_ceiling_and_take_their_states_when_they_go() {
        let cache = SharedSpringCache::with_ceiling(SPRING_CACHE_BYTES);
        let (mut one, _one) = open_sharing(&cache);
        let (mut two, _two) = open_sharing(&cache);
        render(&mut one, 2.0, &a());
        let alone = one.stepper().cache_bytes();
        render(&mut two, 2.0, &a());
        // One performance each: the same data on two models is two.
        assert_eq!(two.stepper().cache_bytes(), 2 * alone);
        assert_eq!(two.stepper().performances(), 1);
        drop(one);
        assert_eq!(two.stepper().cache_bytes(), alone);
    }

    #[test]
    fn a_performance_failing_at_step_zero_leaves_the_cache_alone() {
        // The head's yaw overshoots `f32` around 0 s (the keys around it
        // are finite), so step 0 fails, but a frame at 2 s holds the last
        // key and renders up to its steps.
        let failing = data(json!({
            "tracks": { "head": { "yaw": { "type": "keyframes", "keyframes": [
                { "time": { "value": -30, "timescale": 30 }, "value": 0.0 },
                { "time": { "value": 30, "timescale": 30 }, "value": 3.2e38, "easing": "ease-out-back" },
            ] } } },
        }));
        let (mut probe, _probe) = open();
        render(&mut probe, 2.0, &a());
        let one = probe.stepper().cache_bytes();
        // Room for A alone.
        let cache = SharedSpringCache::with_ceiling(one);
        let (mut model, _files) = open_sharing(&cache);
        render(&mut model, 2.0, &a());
        let error = model.render(2.0, &failing).unwrap_err().0;
        assert!(error.contains("spring bones' step 0:"), "{error}");
        assert_eq!(model.stepper().performances(), 1);
        assert_eq!(model.stepper().kept(&a()), Some((4, Some(120))));
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
        *model.stepper_mut() =
            Stepper::with_limits(&SharedSpringCache::default(), 120, MAX_STEP_WORK);
        render(&mut model, 2.0, &a());
        assert!(model.render(2.0 + 1.0 / SPRING_HZ, &a()).is_err());

        // An avatar without springs has no clock to run.
        let (mut still, _still) = open_vrm(&SyntheticVrm::avatar());
        render(&mut still, past, &a());
    }

    /// Whether a model's notices warn of its springs' reach.
    fn reach_warnings(model: &VrmModel) -> Vec<&str> {
        model
            .notices()
            .iter()
            .filter(|notice| notice.message.contains("spring bones are stepped for"))
            .map(|notice| {
                assert_eq!(notice.level, NoticeLevel::Warning);
                notice.message.as_str()
            })
            .collect()
    }

    #[test]
    fn frames_past_the_work_fence_are_refused() {
        let (mut model, _files) = open();
        // 25 nodes, two moving joints, no colliders or aims.
        assert_eq!(model.step_cost(), 300 + 25 + 2);
        assert!(reach_warnings(&model).is_empty());
        // 16 units for each of the clip's channels and the motion; B's
        // one-keyframe axis 32 and a unit for up to 8 keyframes.
        let cost_a = model.step_cost() + data_cost(&model, &a());
        assert_eq!(
            cost_a,
            model.step_cost() + 16 * (1 + model.clip_channels("motion0"))
        );
        assert_eq!(data_cost(&model, &b()), cost_a - model.step_cost() + 32 + 1);
        let keyframes = |count: u64| {
            let keyframes: Vec<Value> = (0..count)
                .map(|k| json!({ "time": { "value": k, "timescale": 30 }, "value": 1.0 }))
                .collect();
            data(json!({ "tracks": {
                "head": { "yaw": { "type": "keyframes", "keyframes": keyframes } },
                "neck": { "roll": { "type": "keyframes", "keyframes": [
                    { "time": { "value": 0, "timescale": 30 }, "value": 1.0 },
                ] } },
            } }))
        };
        assert_eq!(data_cost(&model, &keyframes(8)), 2 * 32 + 1 + 1);
        assert_eq!(data_cost(&model, &keyframes(9)), 2 * 32 + 2 + 1);

        // A frame may take steps 0..=120 of A.
        let limit = 121 * cost_a;
        let fenced = |model: &mut VrmModel| {
            *model.stepper_mut() =
                Stepper::with_limits(&SharedSpringCache::default(), MAX_SPRING_STEPS, limit);
        };
        fenced(&mut model);
        render(&mut model, 2.0, &a());
        let error = model.render(2.0 + 1.0 / SPRING_HZ, &a()).unwrap_err().0;
        assert!(
            error.contains(&format!(
                "past the 2 s up to which the VRM plugin steps this avatar's spring bones with \
                 these motions and tracks: its 122 steps of {cost_a} work units each take {}, \
                 more than the {limit} a frame may take",
                122 * cost_a
            )),
            "{error}"
        );
        // B's track costs more a step: 2 s is past it.
        assert!(model.render(2.0, &b()).is_err());
        // Refused before any step, warm or cold.
        let (mut cold, _cold) = open();
        fenced(&mut cold);
        assert!(cold.render(2.0 + 1.0 / SPRING_HZ, &a()).is_err());
        assert_eq!(cold.stepper().steps(), 0);
        assert_eq!(cold.stepper().performances(), 0);
    }

    #[test]
    fn the_reach_warning_starts_short_of_four_hours() {
        // A step of 9,942 units reaches step 864,000 within 2^33 units; one
        // of 9,943 does not.
        assert_eq!(MAX_STEP_WORK / (MAX_SPRING_STEPS + 1), 9_942);
        assert!(reach_notice(9_942).is_none());
        let notice = reach_notice(9_943).unwrap();
        assert_eq!(notice.level, NoticeLevel::Warning);
        // Steps 0..=863,916: 14,398.6 s.
        assert_eq!(MAX_STEP_WORK / 9_943 - 1, 863_916);
        assert!(
            notice
                .message
                .contains("stepped for frames up to 14398.6 s into a layer"),
            "{}",
            notice.message
        );
        assert!(reach_notice(610).is_none());
        assert!(reach_notice(2_183).is_none());
        // Not even step 0 fits.
        let none = reach_notice(MAX_STEP_WORK + 1).unwrap();
        assert!(
            none.message.contains("stepped for no frame of a layer"),
            "{}",
            none.message
        );
    }

    #[test]
    fn many_nodes_shorten_the_reach() {
        // 200,000 empty nodes outside the scene: each step clones them.
        const EXTRA: u64 = 200_000;
        let mut vrm = SyntheticVrm::avatar();
        vrm.extensions.insert(VRMC_SPRING_BONE.into(), hair());
        let glb = vrm.glb_with(|root| {
            let nodes = root["nodes"].as_array_mut().unwrap();
            nodes.extend((0..EXTRA).map(|_| json!({})));
        });
        let directory = TempDir::new().unwrap();
        let mut model = VrmModel::load_slice(&glb, &directory.path().join("cache")).unwrap();
        let cost = 300 + 25 + EXTRA + 2;
        assert_eq!(model.step_cost(), cost);
        // 2^33 units: steps 0..=n with (n + 1) × cost within them.
        let last = MAX_STEP_WORK / cost - 1;
        let seconds = (last * 100 / 60) as f64 / 100.0;
        assert_eq!((last, seconds), (42_878, 714.63));
        let warnings = reach_warnings(&model);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains(&format!("frames up to {seconds} s into a layer")),
            "{}",
            warnings[0]
        );
        assert!(warnings[0].contains(&format!("each step takes {cost} work units")));

        // A frame an hour in is refused before any step.
        let still = PerformanceData::default();
        let error = model.render(3600.0, &still).unwrap_err().0;
        assert!(error.contains(&format!("past the {seconds} s")), "{error}");
        assert_eq!(model.stepper().steps(), 0);
    }
}
