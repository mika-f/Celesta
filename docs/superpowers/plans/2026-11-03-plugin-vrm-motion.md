# Plugin VRM motion (phase 3, part 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The VRM plugin's avatars move: VRM animation clips (`.vrma`) play on the composition's timeline, retargeted as UniVRM retargets them; keyframed humanoid tracks add to them; the avatar's node constraints follow; and its spring bones swing deterministically, so any frame, asked for in any order and at any frame rate, is the same.

**Architecture:** Builds on `2026-11-03-plugin-vrm-core.md` and fills the stages it leaves empty in `VrmModel::frame`: `base_pose` (clips in `vrma.rs` and `motion.rs`, tracks in `tracks.rs`), `apply_constraints` (`constraints.rs`), `expression_inputs` (clip expressions), and `apply_springs` (`springs.rs` for UniVRM's step, `stepping.rs` for the fixed 60 Hz clock and its checkpoints). `PerformanceData` gains `motions` and `tracks`; `<Vrm>` gains the props that send them and the clips as assets.

**Tech Stack:** Rust 2024 (the core plan's crates, plus `celesta-composition` for `KeyframeAnimation`), TypeScript (`@celesta/react`, Vitest).

**Spec:** `docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md` (The VRM plugin → Loading (`.vrma`), Per frame, Spring bones deterministic, `<Vrm>`; Checks → Plugin; Phases → 3). Core types consumed here: `NodeTree`, `Trs`, `Pose`, `Humanoid`, `HumanBone`, `ExpressionKey`, `PerformanceData`, `VrmModel` and its stages, `FrameError`, `LoadError`, `Document`, `SyntheticVrm` (all from the core plan; never redefined).

## Global Constraints

- Everything in the core plan's Global Constraints holds (paths, commands, licence headers, glTF axes, determinism, the samples variable, the task checks).
- `PerformanceData` keeps `deny_unknown_fields` and gains `motions: Vec<MotionData>` and `tracks: BTreeMap<String, TrackData>`, both `#[serde(default)]`.
- Assets: `model` plus `motion0`, `motion1`, … (decimal, one `.vrma` each); any other name is refused.
- Per frame, in UniVRM's order (`Vrm10Runtime.Process`): clips and tracks → constraints → lookAt → expressions → spring bones.
- Spring bones step at `SPRING_HZ = 60`, `Δt = 1/60` s; each performance (motions and tracks) keeps a cursor and checkpoints every `CHECKPOINT_STEPS = 30` steps, within one `SPRING_CACHE_BYTES` ceiling per process and no cap on how many performances are kept; a frame is refused past `MAX_SPRING_STEPS` (4 h) or past `MAX_STEP_WORK` units of work.
- Track times are read to the microsecond (`Time::new((seconds × 1e6).round(), 1_000_000)`), as the Live2D plugin reads them.
- `VRMC_springBone_limit` (a warning says when a joint carries it), the VRMA `lookAt` animation, and glTF animations other than the clip's first are not read.

## Review Focus

- **Scrubbing an avatar with springs**: frames asked for out of order, after another performance warmed the model, equal frames played in order on a fresh model, bit for bit. Task 6.
- **Clips on avatars whose bones rest rotated** (VRoid's are not identity): a clip turns each bone by its normalized rotation, so an arm lowered in the clip lowers on the avatar instead of twisting. Task 1.
- **Clips recorded on a taller or shorter rig**: the hips' travel scales by the avatar's hips height over the clip's, so feet stay on the floor. Task 1.
- **30 fps and 60 fps exports**: spring bones step at a fixed 60 Hz, so the frames both exports share are equal. Task 6.
- **Chained constraints**: they run in node pre-order and read sources already constrained this frame, as UniVRM's `GetComponentsInChildren` order does. Task 4.

---

### Task 1: VRMA clips and UniVRM's retarget

**Files:**
- Create: `plugins/vrm/src/vrma.rs` (ported: `Packages/VRM10/Runtime/IO/VrmAnimationImporter.cs`, `Packages/VRM10/Runtime/ControlRig/InitialRotationPoseProvider.cs`, `BoneInitialRotation.cs`, `Vrm10Retarget.cs`, `Packages/VRM10/Runtime/Components/Vrm10Runtime/ControlRig/Vrm10ControlBone.cs`, `Vrm10RuntimeControlRig.cs`)
- Modify: `src/synthetic.rs` (`SyntheticVrma`), `src/lib.rs`

**Interfaces — Produces:**

```rust
pub struct Clip { pub duration: f64, /* bone channels, rest data, expression channels */ }
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ClipPose { pub rotations: BTreeMap<HumanBone, Quat> /* normalized local */, pub hips: Option<Vec3> /* raw, clip root space */, pub expressions: BTreeMap<ExpressionKey, f32> }
impl Clip { pub fn load(bytes: &[u8]) -> Result<Self, LoadError>; pub fn sample(&self, seconds: f64) -> ClipPose; pub fn hips_rest(&self) -> Vec3 }
pub(crate) struct Retarget { /* per avatar bone: rest local and world rotation; hips rest world position */ }
impl Retarget {
    pub(crate) fn new(tree: &NodeTree, humanoid: &Humanoid) -> Self;
    pub(crate) fn apply(&self, pose: &ClipPose, source_hips_rest: Vec3, tree: &NodeTree, out: &mut Pose);
}
pub struct SyntheticVrma { pub hips_rest_y: f32, pub arm: Vec<(f32, Quat)>, pub hips: Vec<(f32, Vec3)>, pub happy: Vec<(f32, f32)>, pub interpolation: &'static str }
impl SyntheticVrma { pub fn clip() -> Self; pub fn glb(&self) -> Vec<u8> }
```

**Decisions** (UniVRM's math, written in glTF's axes):
- **Loading:** `VRMC_vrm_animation` through `vrm-spec` (its `specVersion` required by the schema but not checked, as UniVRM's importer does not check it); a file without it: `"this glTF file has no VRMC_vrm_animation extension: it is not a VRM animation"`. Only `animations[0]`. Bone channels: `rotation` of every humanoid bone's node, `translation` of the hips' node. An expression is the `translation` x of its node (`VrmAnimationImporter.CreateCurve`), keyed by `ExpressionKey::parse(name)`. `duration` is the last input time over those channels.
- **Sampling:** times clamp to the channel's first and last key; `STEP` holds; `LINEAR` lerps translations and slerps rotations along the shorter arc (glTF; UniVRM's Unity curves lerp components then normalise, a difference far below a degree at a VRMA's key rate; recorded); `CUBICSPLINE` is glTF's Hermite with tangents scaled by the key interval, rotations normalised.
- **Normalized rotation of the clip** (`BoneInitialRotation.NormalizedLocalRotation`): with the clip's rest local rotation `L₀` and rest world rotation `G₀` of the bone's node, `q = G₀ · L₀⁻¹ · L(t) · G₀⁻¹`. Raw hips: the hips node's world position at `t` in the clip's root space.
- **Onto the avatar** (`Vrm10ControlBone.ProcessRecursively`): with the avatar bone's rest local rotation `L₀'` and rest world rotation `G₀'`, `local(b) = L₀' · (G₀'⁻¹ · q · G₀')`. A bone the clip lacks takes `q = identity` (its rest). Non-humanoid nodes keep the pose's values.
- **Hips** (`Vrm10Retarget.Retarget`): `world = H₀' + (raw − H₀) × (H₀'.y / H₀.y)`, `H₀`/`H₀'` the clip's and avatar's rest hips world positions; written as the hips' local translation through its parent's current world. A clip without hips translation leaves the hips at rest.
- **Synthetic clip:** a humanoid with identity rest rotations and the hips at (0, `h`, 0), `h = hips_rest_y` (default 1.0; the core avatar's hips are at 0.9, so the travel scales by 0.9 / `h`); 1 s; `leftUpperArm` from identity to Rz(−90°) (lowered); `hips` keys default to `[(0, (0, h, 0)), (1, (0, h, 0.5))]`; `happy` 0 → 1; `LINEAR`.

- [ ] **Step 1:** Tests (`vrma.rs`, synthetic avatar and clip; positions within 1e-4):
  - `clip_reads_bones_hips_and_expressions`: `duration == 1.0`; `sample(0.5)` has the arm's `q` = Rz(−45°), `hips` (0, 1, 0.25), `happy` 0.5; `sample(2.0)` equals `sample(1.0)`.
  - `retarget_uses_normalized_rotations` (Review Focus): at 1 s the avatar's `leftLowerArm` world position is (0.15, 1.0, 0.45) (copying the local rotation instead would put it near (−0.10, 1.25, 0.45)); at 0.5 s (0.3267767, 1.0732233, 0.225).
  - `hips_travel_scales_by_hips_height` (Review Focus): at 1 s the hips are at (0, 0.9, 0.45); with `hips_rest_y: 0.45` (a rig half the size, its keys following `h`) at (0, 0.9, 1.0).
  - `step_and_cubic_interpolation`: `"STEP"` at 0.5 s gives identity; a `"CUBICSPLINE"` clip with zero tangents gives Rz(−45°) at 0.5 s.
  - `not_a_vrma_is_refused`: the core avatar's glb → `"not a VRM animation"`.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml vrma` → fails to compile.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM animation clips retargeted as UniVRM does`.

### Task 2: Motions on the timeline

**Files:**
- Create: `plugins/vrm/src/motion.rs`
- Modify: `src/provider.rs` (`motions`, clip assets), `src/model.rs` (`VrmModel::open`, `base_pose`, `expression_inputs`)

**Interfaces — Produces:**

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MotionData { pub clip: String /* "motion0"… */, #[serde(default)] pub from: f64 /* s */, #[serde(default, rename = "loop")] pub looped: bool, #[serde(default)] pub fade_in: f64 /* s */ }
impl VrmModel { pub fn open(assets: &BTreeMap<String, PathBuf>) -> Result<Self, LoadError> }   // replaces the provider's direct `load`
pub(crate) fn motion_pose(clips: &BTreeMap<String, Clip>, motions: &[MotionData], time: f64) -> Result<Option<(ClipPose, Vec3 /* source hips rest */)>, FrameError>
```

**Decisions:**
- Motions run in `from` order (stable for equal `from`). At `t` the active motion is the last with `from ≤ t`; before the first, the avatar rests and no clip expression applies. Local time `τ = t − from`: looped `τ mod duration` (0 for a zero-length clip), else `min(τ, duration)` (the last pose holds until the next motion).
- **Fade:** while `τ < fade_in`, the pose blends from the previous motion (evaluated at `t` by these same rules, or the rest pose when there is none) with `w = τ / fade_in`: each bone `slerp(q_prev, q_cur, w)` (identity where one lacks the bone), hips `lerp` (rest where one lacks it), expressions `lerp` (0 where one lacks it). Both motions retarget with their own clip's hips rest.
- `expression_inputs` = the clip's expressions, then `data.expressions` replacing them key by key: what the layer says wins over what the clip says (UniVRM sets the clip's weights each frame; a composition's explicit values are the author's intent).
- A motion naming a clip the layer did not open → `FrameError` `"motion {i} names clip \"{clip}\", which the layer did not open"`; non-finite `from` or negative `fade_in` → `"motion {i}'s from/fadeIn …"`.

- [ ] **Step 1:** Tests (`motion.rs`, the synthetic clip as `motion0`):
  - `motions_start_at_from`: `[{clip: motion0, from: 2}]`: at 1.5 s the pose is rest; at 3 s the arm is down; at 2.5 s half-way (the Task 1 values).
  - `looping_and_holding`: `loop: true, from: 0`: 1.25 s equals 0.25 s; without `loop` 5 s equals 1 s.
  - `fade_blends_from_the_previous_motion`: `[{motion0, from 0}, {motion0, from 1, fadeIn: 0.5}]` at 1.25 s: the previous holds Rz(−90°), the new is at Rz(−22.5°), `w` 0.5 → the arm's normalized rotation Rz(−56.25°) within 1e-4 rad.
  - `clip_expressions_yield_to_the_layer`: `[{clip: motion0}]` at 0.5 s: morph target 0 of draw 0 is 0.5 (happy from the clip); with `expressions: {happy: 0.2}` it is 0.2.
  - `unknown_clip_is_refused`: `clip: "motion3"` → `"names clip \"motion3\""`; provider: asset `"walk"` → `"unknown asset"`.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml motion` → fails.
- [ ] **Step 3:** Implement. **Step 4:** Pass; core's tests still pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM motions on the composition's timeline`.

### Task 3: Procedural humanoid tracks

**Files:**
- Create: `plugins/vrm/src/tracks.rs`
- Modify: `plugins/vrm/Cargo.toml` (`celesta-composition` by path as a dependency), `src/provider.rs`, `src/model.rs`

**Interfaces — Produces:**

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackData { #[serde(default)] pub pitch: Option<KeyframeAnimation<f64>>, #[serde(default)] pub yaw: Option<KeyframeAnimation<f64>>,
    #[serde(default)] pub roll: Option<KeyframeAnimation<f64>>, #[serde(default)] pub x: Option<KeyframeAnimation<f64>>,
    #[serde(default)] pub y: Option<KeyframeAnimation<f64>>, #[serde(default)] pub z: Option<KeyframeAnimation<f64>> }
pub(crate) fn apply_tracks(tracks: &BTreeMap<String, TrackData>, time: f64, pose: &mut ClipPose) -> Result<(), FrameError>
```

**Decisions:**
- Keys are VRM humanoid bone names (`head`, `leftUpperArm`, …); another name → `FrameError` `"tracks name \"{name}\", which is not a humanoid bone"`; a bone the avatar lacks is skipped. `x`, `y`, `z` only on `hips` (`"only hips takes x, y, z tracks"`).
- Each value is `keyframe_segment` at the track time, `from + (to − from) × eased` (an empty track is refused when read).
- **Rotation:** degrees, in the normalized (model-aligned) frame: `q ← q · Ry(−yaw) · Rx(−pitch) · Rz(roll)`, after the motion. The angles turn as `lookAt`'s do (UniVRM's): with the avatar facing +Z, its right toward −X, yaw turns the face to the avatar's right, pitch looks up, roll tilts its top toward the avatar's right.
- **Hips:** `(x, y, z)` metres added to the hips' retargeted world position (from the rest position when no motion moves it).
- Tracks are applied to the clip pose before `Retarget::apply`, so the rest of the pipeline sees one pose.

- [ ] **Step 1:** Tests (`tracks.rs`):
  - `head_tracks_turn_the_head`: `head: {yaw: [{0 s, 0}, {1 s, 90}]}` at 0.5 s: the head's world rotation × (0, 0, 1) is (−0.7071068, 0, 0.7071068); at 2 s (−1, 0, 0).
  - `tracks_compose_after_the_motion`: the synthetic clip at 1 s plus `leftUpperArm: {roll: 90}` (one keyframe) brings the arm back up: `leftLowerArm` at (0.40, 1.25, 0.45).
  - `hips_tracks_move_the_avatar`: `hips: {y: 0.1}` → hips at (0, 1.0, 0) with no motion.
  - `bad_tracks_are_refused`: `{"tail": {...}}` → `"not a humanoid bone"`; `head: {x: …}` → `"only hips"`.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml tracks` → fails.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): keyframed humanoid tracks for VRM avatars`.

### Task 4: Node constraints

**Files:**
- Create: `plugins/vrm/src/constraints.rs` (ported: `Packages/VRM10/Runtime/Components/Constraint/Vrm10RollConstraint.cs`, `Vrm10AimConstraint.cs`, `Vrm10RotationConstraint.cs`; the order from `Vrm10Runtime.cs`)
- Modify: `src/model.rs` (`apply_constraints`), `src/synthetic.rs` (`node_extensions` already exist)

**Interfaces — Produces** (own serde: `vrm-spec` has no `VRMC_node_constraint`):

```rust
#[derive(Deserialize)] #[serde(rename_all = "camelCase")] struct NodeConstraintExtension { spec_version: String, constraint: ConstraintJson }
#[derive(Deserialize)] #[serde(rename_all = "camelCase")] struct ConstraintJson { roll: Option<RollJson>, aim: Option<AimJson>, rotation: Option<RotationJson> }
#[derive(Deserialize)] #[serde(rename_all = "camelCase")] struct RollJson { source: usize, roll_axis: Axis /* "X" | "Y" | "Z" */, #[serde(default = "one")] weight: f32 }
#[derive(Deserialize)] #[serde(rename_all = "camelCase")] struct AimJson { source: usize, aim_axis: AimAxis /* "PositiveX" … "NegativeZ" */, #[serde(default = "one")] weight: f32 }
#[derive(Deserialize)] #[serde(rename_all = "camelCase")] struct RotationJson { source: usize, #[serde(default = "one")] weight: f32 }
pub(crate) struct Constraints { /* by destination, in node pre-order */ }
impl Constraints { pub(crate) fn read(doc: &Document, tree: &NodeTree) -> Result<Self, LoadError>; pub(crate) fn apply(&self, tree: &NodeTree, pose: &mut Pose) }
pub(crate) fn from_to_rotation(from: Vec3, to: Vec3) -> Quat
```

**Decisions:**
- Read from each node's `extensions.VRMC_node_constraint` (its `specVersion` required by the schema but not checked, as UniVRM's importer does not check it); exactly one of roll, aim, rotation, else `LoadError` `"node {n}'s constraint has {k} kinds; it needs one"`; a source out of range or equal to the node → `"node {n}'s constraint source {s} …"`; `weight` used as given (the schema bounds it to 0..1, but UniVRM's importer copies it unchecked and `Process` blends with `SlerpUnclamped`, so a weight past 0..1 extrapolates).
- **Order:** destinations in node pre-order (UniVRM's `GetComponentsInChildren`); each reads the current pose, so a constraint whose source an earlier one moved sees the move. World matrices are recomputed after each constraint.
- With rest (initial) local rotations `D₀` (destination) and `S₀` (source), and `slerp` along the shorter arc:
  - **Rotation:** `local = slerp(D₀, D₀ · (S₀⁻¹ · S), w)`.
  - **Roll:** `Δ = S₀⁻¹ · S`; `Δp = S₀ · Δ · S₀⁻¹`; `Δd = D₀⁻¹ · Δp · D₀`; `to = Δd · axis`; `local = slerp(D₀, D₀ · from_to(axis, to)⁻¹ · Δd, w)`.
  - **Aim:** `P` = the destination parent's current world rotation; `from = P · D₀ · axis`; `to = normalize(source world position − destination world position)`; `local = slerp(D₀, P⁻¹ · from_to(from, to) · P · D₀, w)`.
- `from_to_rotation` (Unity's `FromToRotation`): identity when the unit vectors' dot ≥ 1 − 1e-6; a half turn about `normalize(cross(from, +X))` (or `cross(from, +Y)` when that is shorter than 1e-6) when ≤ −1 + 1e-6; else the shortest arc. Coincident aim positions leave `to` as `from`.

- [ ] **Step 1:** Unit tests (`constraints.rs`, poses built by hand on the synthetic tree, angles within 1e-4 rad):
  - `rotation_copies_the_source_delta`: rotation constraint on `leftLowerLeg` from `leftUpperLeg`, weight 1; upper leg local Rx(20°) → lower leg local Rx(20°); weight 0.5 → Rx(10°).
  - `roll_keeps_only_the_twist`: roll on `rightLowerArm` from `rightHand`, axis X, weight 0.5: hand Rx(60°) → Rx(30°); hand Rz(40°) → identity.
  - `aim_points_the_axis_at_the_source`: aim on `leftEye`, source `FirstPersonOnly`, `PositiveZ`, weight 1 → the eye's world forward (−0.5144958, 0, 0.8574929).
  - `chained_constraints_run_in_pre_order` (Review Focus): rotation on `leftUpperArm` from `spine` and on `leftLowerArm` from `leftUpperArm`; spine local Ry(30°) → `leftUpperArm` local Rz(90°)·Ry(30°) and `leftLowerArm` local Ry(30°) (reversed order would leave it identity).
  - `aim_at_the_opposite_direction_is_finite`: a source exactly behind the aim axis → a finite half turn.
  - `bad_constraints_are_refused`: a source equal to the node → message contains `"constraint source"`.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml constraints` → fails.
- [ ] **Step 3:** Implement; `apply_constraints` calls it. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM node constraints in UniVRM's order`.

### Task 5: Spring bones, one step

**Files:**
- Create: `plugins/vrm/src/springs.rs` (ported: `Packages/UniGLTF/Runtime/SpringBoneJobs/UpdateFastSpringBoneJob.cs`, `SpringBoneCollision.cs`, `InputPorts/FastSpringBoneBuffer.cs` (`LogicFromTransform`), `FastSpringBoneConbinedBuffer.cs` (`InitCurrentTailsJob`, the tail swap); `Packages/VRM10/Runtime/Components/Vrm10Runtime/Springbone/FastSpringBoneBufferFactory.cs`)

**Interfaces — Produces:**

```rust
pub(crate) enum ColliderShape { Sphere { offset: Vec3, radius: f32 }, Capsule { offset: Vec3, radius: f32, tail: Vec3 }, Plane { offset: Vec3, normal: Vec3 },
    SphereInside { offset: Vec3, radius: f32 }, CapsuleInside { offset: Vec3, radius: f32, tail: Vec3 } }
pub(crate) struct JointLogic { pub head: usize, pub tail: usize, pub parent: Option<usize>, pub local_rotation: Quat, pub bone_axis: Vec3, pub length: f32,
    pub radius: f32, pub stiffness: f32, pub gravity_power: f32, pub gravity_dir: Vec3, pub drag: f32 }
pub(crate) struct Spring { pub joints: Vec<JointLogic>, pub colliders: Vec<(usize /* node */, ColliderShape)>, pub center: Option<usize> }
#[derive(Clone, Debug, PartialEq)] pub(crate) struct SpringState { pub prev: Vec<Vec3>, pub current: Vec<Vec3> }   // centre space, all joints in spring order
pub(crate) struct Springs { pub springs: Vec<Spring> }
impl Springs {
    pub(crate) fn read(doc: &Document, tree: &NodeTree) -> Result<Self, LoadError>;
    pub(crate) fn initial_state(&self, tree: &NodeTree, pose: &Pose) -> SpringState;
    pub(crate) fn step(&self, tree: &NodeTree, pose: &mut Pose, state: &mut SpringState, dt: f32);
    pub(crate) fn settle(&self, tree: &NodeTree, pose: &mut Pose, state: &SpringState);   // rotations toward the current tails, no integration
}
```

**Decisions:**
- **Reading** `VRMC_springBone` through `vrm-spec`: springs in file order, each with its joints (the last is a tail only: a spring of `k` joints has `k − 1` logics), its collider groups' colliders in group then collider order, and its `center` node. Joint defaults: `hitRadius` 0, `stiffness` 1, `dragForce` 0.5, `gravityPower` 0, `gravityDir` (0, −1, 0). A collider's `extensions.VRMC_springBone_extended_collider` (`specVersion` not checked, as in UniVRM; own serde: `shape.sphere {offset, radius, inside}`, `shape.capsule {offset, radius, tail, inside}`, `shape.plane {offset, normal}`) replaces its base shape; `inside` picks the inside kinds. A joint node out of range or a spring with fewer than 2 joints → `LoadError` naming it.
- **Logic** (`LogicFromTransform`, at rest): `local_rotation` the head's rest local rotation; `c = tail rest local translation × tail rest world scale` (component-wise); `bone_axis = normalize(c)`, `length = |c|`; `parent` the head's parent.
- **Initial state** (`InitCurrentTailsJob`): `prev = current =` each tail's world position in the given pose, in centre space (`center⁻¹ ×`, the centre's world in that pose).
- **Step** (`UpdateFastSpringBoneJob.Execute`), springs in order, joints in order, `scaling` 1 (UniVRM's default):
  1. Head world from its parent's current world (which an earlier joint of this step may have turned) × its local.
  2. `cur`, `prev` to world through the centre's world in the step's pose (before any spring turned it).
  3. `next = cur + (cur − prev) × (1 − drag) + parentRot · localRot · axis × stiffness × dt + gravityDir × gravityPower × dt` (inertia not scaled by `dt`).
  4. `next = head + normalize(next − head) × length` (a zero vector stays at the head).
  5. Each collider in order, its world from the step's pose before spring turns: sphere `r = hitRadius + radius × max|world scale|`, inside `|next − c|² ≤ r²` pushes to `c + normalize(next − c) × r`, then renormalised to `length`; capsule: the closest point to the **head** on the segment (`P = normalize(tail − c)`, `d = P · (head − c)`: `d ≤ 0` the start sphere, `d ≥ |tail − c|` the end sphere, else `c + P × d`), then the sphere rule; plane `dist = (next − p) · n − hitRadius`, below 0 `next −= n × dist`, not renormalised; sphere inside `dist = radius − hitRadius − |next − c|`, below 0 `next += normalize(next − c) × dist`, not renormalised; capsule inside the same against the segment's closest point to `next`. The inside shapes' radius is not scaled.
  6. Store `next` in centre space; the head's rotation `from_to(parentRot · localRot · axis, next − head) · parentRot · localRot`, written back as its local rotation.
  After all springs: `prev = current`, `current = next`.
- **Settle:** steps 1, 2, and 6 with `next = current` (the frame's pose, the last step's tails).

- [ ] **Step 1:** Unit tests (`springs.rs`; a spring of joints `[hair0, hair1]` on the synthetic avatar, rest pose; positions within 1e-6):
  - `logic_from_rest`: `length` 0.1118034, `bone_axis` (0, −0.8944272, −0.4472136).
  - `a_spring_at_rest_stays_at_rest`: stiffness 1, drag 0.5, no gravity: after 10 steps the tail is at (0, 1.45, −0.1).
  - `inertia_is_not_scaled_by_the_step`: stiffness 0, drag 0, gravity power 1: after one step (0, 1.4472365, −0.0940415), after two (0, 1.4432604, −0.0832666).
  - `sphere_colliders_push_out_and_renormalise`: a sphere on `head` with offset (0, 0.05, −0.15), radius 0.06; stiffness 0, no gravity → (0, 1.4461932, −0.0915227).
  - `capsules_measure_from_the_head`: a capsule on `head` from offset (−1, 0.05, −0.15) to (1, 0.05, −0.15), radius 0.06 → the sphere case's tail.
  - `planes_push_without_renormalising`: a plane on `head`, offset (0, 0.05, −0.08), normal (0, 0, 1) → exactly (0, 1.45, −0.08).
  - `inside_spheres_hold_the_tail_in`: an inside sphere on `hair0`, radius 0.08 → (0, 1.4784458, −0.0857771).
  - `centre_space_moves_tails_with_the_centre`: centre `hips`; between two steps the hips move by (1, 0, 0): the tail moves by exactly (1, 0, 0); without a centre it lags (x < 1).
  - `extended_colliders_replace_the_base_shape`: a base sphere with an extended plane → the plane case's result.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml springs` → fails.
- [ ] **Step 3:** Implement. **Step 4:** Pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): VRM spring bones with UniVRM's step`.

### Task 6: Deterministic stepping and checkpoints

**Files:**
- Create: `plugins/vrm/src/stepping.rs`
- Modify: `src/model.rs` (`apply_springs`, the model's step cost and notices), `src/provider.rs` (one spring cache for the process), `src/springs.rs` (pairs, the nodes a step reads), `src/constraints.rs` (the aims' climb), `src/vrma.rs` (a clip's channels), `src/tracks.rs` (checking apart from reading)

**Interfaces — Produces:**

```rust
pub const SPRING_HZ: f64 = 60.0; pub const CHECKPOINT_STEPS: u64 = 30;
pub const MAX_SPRING_STEPS: u64 = 4 * 3600 * 60; pub const MAX_STEP_WORK: u64 = 1 << 33; pub const SPRING_CACHE_BYTES: u64 = 256 << 20;
#[derive(Clone, Default)] pub(crate) struct SharedSpringCache(Arc<Mutex<SpringCache>>);   // one per provider, so per process
pub(crate) struct SpringCache { performances: BTreeMap<(u64 /* model */, [u8; 32] /* SHA-256 */), Performance>, bytes: u64, ceiling: u64, clock: u64, models: u64 }
struct Performance { grid: BTreeMap<u64, SpringState>, cursor: Option<(u64, SpringState)>, spacing: u64, last_used: u64, state_bytes: u64 }
pub(crate) struct Stepper { cache: SharedSpringCache, model: u64, max_steps: u64, max_work: u64 }   // Drop forgets the model's performances
impl Stepper { pub(crate) fn state_at(&self, model: &VrmModel /* pose stages and springs */, time: f64, data: &PerformanceData) -> Result<SpringState, FrameError> }
pub(crate) fn steps_until(time: f64) -> u64   // floor(max(time, 0) × 60 + 1e-6)
pub(crate) fn step_cost(nodes: u64, joints: u64, pairs: u64, climbed: u64) -> u64   // 300 + nodes + joints + 3 × pairs + climbed
pub(crate) fn reach_notice(cost: u64) -> Option<Notice>
```

**Decisions:**
- **Clock:** a frame at `t` has run steps `0..=n`, `n = steps_until(t)`, step `k` at `k / 60` s with `Δt = 1/60`. Before step 0 the tails start from the pose at 0 s (velocity zero); step 0 integrates once, as UniVRM's first frame does. The frame then takes its own pose at `t` (clips, tracks, constraints, lookAt, expressions) and `settle`s on step `n`'s tails, so a time between steps shows the last step's tails on the current pose.
- **What a step sees:** the pose at `k / 60` from `base_pose` and `apply_constraints` only. LookAt and expressions are not in the steps: in a VRM 1.0 avatar they turn no bone but the eyes, and a frame's gaze and expression values are that frame's alone, which earlier steps cannot be given. A spring hanging from an eye bone swings as if the eye rested; recorded. A step's transforms place only the nodes it reads (heads and their parents, centres, collider nodes) and their ancestors, by the same operations as the whole tree's walk, so the result is the same bit for bit.
- **Kept states** only save work; no frame depends on them. A performance is a model's motions and tracks, everything the steps read (the springs' settings are the model's), keyed by the SHA-256 of `serde_json` of `(motions, tracks)`. It keeps a **cursor**, the state its last request ended on, which each request replaces, and **grid checkpoints** after each step `k` with `(k + 1) % (30 × spacing) == 0`. A request resumes from the later of its cursor and the latest checkpoint at or before `n` (else from step 0), so sequential frames step only the steps between them and a scrub back resumes at most `30 × spacing` steps early.
- **Memory:** the process (one `SharedSpringCache` in `VrmProvider`, which every model it opens joins; a model opened alone has its own) keeps at most `SPRING_CACHE_BYTES` of states, a ceiling against hostile files: real performances take about 1 MB per ten minutes of layer. Nothing counts performances. Past the ceiling, latency gives way, never output: the performance with the most checkpoints doubles its spacing (dropping the states off it), repeatedly; then a performance's last checkpoint goes; then the least recently used performances (cursors) go. A request takes its own performance out of the cache while it steps, so it is never evicted by its own request and keeps checkpoints only within what the others leave; its cursor is always kept, so the cache holds at most the ceiling plus one cursor. A performance's initial state is built before the cache changes, so one failing at step 0 leaves the cache as it was; `close` drops a model's performances.
- **Bounds:** before any state is looked up, a frame is refused past `MAX_SPRING_STEPS` (4 h), or when `(n + 1) × cost > MAX_STEP_WORK`, with `cost` the model's `step_cost` (computed when it opens, from its node count, moving joints, joint–collider pairs, and aim climb) plus the data's, charged as measured: each motion `16 × (1 + its clip's channels)`, since a step in a fade samples every motion; each track axis 32 plus a unit per 8 keyframes rounded up, since reading a track walks them. A unit measured a few nanoseconds of one laptop core (release build) on Seed-san and UniVRM's constraint twist sample, with and without a clip, so the worst frame accepted from a cold start (2^33 units) takes under a minute. Each data charge (16 a clip channel and one for the motion, 32 a track axis, one per 8 keyframes) is within 2× of what those two samples measured. Seed-san (610 units a step) reaches 65 h alone and 27 h with a clip; the twist sample (2,183) 18 h and 13 h; a file of a million empty nodes about 2.4 min. Refusals depend on the model, the data, and the time alone. The tracks are checked once per request; each step only reads them. A model with springs whose cost alone cannot reach `MAX_SPRING_STEPS` (a cost above 9,942 units) gets a `Warning` notice when it opens, naming the latest time it reaches.
- A model without springs skips all of it.

- [ ] **Step 1:** Tests (`stepping.rs`; the synthetic avatar with a hair spring `[hair0, hair1, hair2]`, stiffness 0.5, drag 0.4, gravity power 1; data A = `motions: [{motion0, loop: true}]`, B = the same plus `tracks: {head: {yaw: 30}}`; frames compared with `==`; a test-only step counter on `Stepper`):
  - `springs_do_not_depend_on_frame_order` (Review Focus): on a model that first rendered B at 4 s, frames of A at `[2.0, 0.5, 3.3, 0.5, 1.0]` equal those of a fresh model asked in increasing order.
  - `thirty_and_sixty_fps_agree` (Review Focus): A at `k / 30` for k in 0..60 on one model equals A at the same times on another that is also asked every `k / 60` in between.
  - `states_are_kept_by_motions_and_tracks`: A at 2 s, B at 2 s, A at 2 s → the last equals the first; 2 performances, A's with 4 checkpoints and its cursor on step 120; after 20 more performances all 22 are kept.
  - `per_frame_values_leave_the_springs_alone`: A at 2 s with `expressions: {happy: 1}` and `lookAt: {yaw: 30, pitch: 0}` has the same hair node worlds as A at 2 s without them.
  - `a_gaze_does_not_reach_the_steps`: with the hair's centre on `leftEye`, the springs' state at 2 s is the same with and without a gaze (which does turn the frame's eye).
  - `between_steps_the_last_step_holds`: with no motion (a still avatar), the hair nodes at `1/60 + 0.004` s equal those at `1/60` s.
  - `sequential_frames_step_only_the_delta`: 60 s at 30 fps runs 1 step, then 2 per frame (3,599 in all); a scrub back to 30 s runs at most 30, and the next frame 2.
  - `interleaved_performances_keep_requests_small`: 12 performances asked in turn at every frame of 10 s at 30 fps run 12 × 599 steps, at most 2 per request, within the ceiling; with a ceiling of 12 cursors and one state, one performance's checkpoints filling it first, no request runs more than 2.
  - `the_ceiling_costs_latency_only`: three performances scrubbed back and forth under ceilings of 4 KB (cursors kept, checkpoints thinned) and 100 B (each request evicts the others) give frames equal to an uncapped model's.
  - `models_share_the_ceiling_and_take_their_states_when_they_go`, `a_performance_failing_at_step_zero_leaves_the_cache_alone`.
  - `frames_past_the_last_step_are_refused`, `frames_past_the_work_fence_are_refused` (an injected budget of 121 steps of A: 2 s renders, the next step is refused, B's extra track makes 2 s too much, the motion, axis, and keyframe charges, and a cold model refuses with no step run), `the_reach_warning_starts_short_of_four_hours` (no warning at 9,942 units a step, one at 9,943), `many_nodes_shorten_the_reach` (200,000 empty nodes: the cost, the warning's 714.63 s, a frame an hour in refused with no step run).
  - `steps_until_counts_whole_steps`: `steps_until(0.0) == 0`, `(1.0) == 60`, `(1.0 / 3.0) == 20`, `(-1.0) == 0`.
  - `springs.rs`: `steps_read_their_nodes_alone_and_exactly` (240 swaying steps equal to a whole-tree reading, on the synthetic avatar and the samples); `samples.rs`: Seed-san and the twist sample open without the reach warning.
- [ ] **Step 2:** `cargo test --manifest-path plugins/vrm/Cargo.toml stepping` → fails.
- [ ] **Step 3:** Implement. **Step 4:** Pass; the core plan's `frames_repeat_exactly` and process test still pass; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): deterministic VRM spring bones at 60 Hz with checkpoints`.

### Task 7: `<Vrm motions tracks>`

**Files:**
- Modify: `plugins/vrm/js/src/index.ts`, `js/test/vrm.test.mts`, `plugins/vrm/demo.tsx`, `plugins/vrm/README.md`

**Interfaces — Produces:**

```ts
export interface VrmMotion { src: AssetInput; from?: number /* frame of the enclosing sequence, 0 */; loop?: boolean; fadeIn?: number /* frames */ }
export interface VrmKeyframe { frame: number; value: number; easing?: Easing }
export interface VrmTrack { pitch?: VrmKeyframe[]; yaw?: VrmKeyframe[]; roll?: VrmKeyframe[]; x?: VrmKeyframe[]; y?: VrmKeyframe[]; z?: VrmKeyframe[] }
// VrmProps gains:  motions?: VrmMotion[];  tracks?: Partial<Record<VrmHumanBone, VrmTrack>>;
export type VrmHumanBone = 'hips' | 'spine' | /* … every VRM 1.0 humanoid bone name */ 'rightLittleDistal';
```

**Decisions:**
- `vrmData` sends `motions: [{ clip: 'motion<i>', from: from / fps, loop?, fadeIn?: fadeIn / fps }]`, `i` the index of the first motion with the same `src` (`===`); `content` resolves each distinct `src` once as asset `motion<i>`.
- `tracks` become `KeyframeAnimation<number>` on the layer's clock as `<Live2D>`'s `keyframeAnimation` does (frame-rate timescale for whole rates, microseconds otherwise), sorted by frame. A fractional frame is rounded to the nearest frame at every rate (as a whole rate's timescale rounds it), so 29.97 fps places it where 30 fps does.
- The demo adds `motions={[{ src: './assets/idle.vrma', loop: true }]}` and a head nod track; the README says where to get a `.vrma` (any VRM animation, for example the one three-vrm's repository tests with) and that it goes in `plugins/vrm/assets/`.

- [ ] **Step 1:** Tests in `vrm.test.mts`:
  - `motions_name_their_clips`: two motions with `src: './a.vrma'` and one `'./b.vrma'`, `from: 30` at 30 fps → clips `motion0`, `motion0`, `motion2`, `from` 1; `fadeIn: 15` → 0.5.
  - `tracks_are_keyframes_on_the_layer_clock`: `head: { yaw: [{frame: 30, value: 20}, {frame: 0, value: 0}] }` at 30 fps → keyframes sorted, times `{value: 0, timescale: 30}` and `{value: 30, timescale: 30}`; at 29.97 fps timescale 1 000 000; frames 10.4 and 20.6 are frames 10 and 21 at 30 fps and at 29.97 fps.
- [ ] **Step 2:** `pnpm --filter celesta-plugin-vrm-js test` → fails.
- [ ] **Step 3:** Implement. **Step 4:** Pass; `node plugins/vrm/build.mjs`; then the task checks.
- [ ] **Step 5:** Commit `feat(plugins): <Vrm> motions and tracks`.

### Task 8: The official samples in motion

**Files:**
- Modify: `plugins/vrm/tests/samples.rs`, `plugins/vrm/tests/process.rs`, `plugins/vrm/tests/render.rs`

**Decisions:** as in the core plan, sample tests skip without `CELESTA_VRM_SAMPLES_DIR` and GPU tests without an adapter (unless `CELESTA_REQUIRE_GPU=1`).

- [ ] **Step 1:** Tests:
  - `samples.rs` `twist_sample_constraints_follow_their_sources`: `VRM1_Constraint_Twist_Sample.vrm` with `tracks: {leftLowerArm: {roll: [{0, 0}, {1 s, 90}]}}` at 1 s → some node carrying a roll constraint is turned from its rest, and none is non-finite.
  - `samples.rs` `vrma_moves_seed_san`: `test.vrma` as `motion0` on `Seed-san.vrm`, `loop: true`: at 1 s some humanoid bone's local rotation differs from rest by more than 1°; frames at `[3.0, 0.5, 2.0, 0.5]` on a model warmed with another performance equal fresh in-order frames; some spring joint's world position at 2 s differs from rest.
  - `process.rs` `motions_cross_the_process`: the synthetic avatar (with a hair spring) and clip as assets `model` and `motion0` through `PluginHost` and in process; frames at `[2.0, 0.25, 2.0]` with `motions` equal; a second `PluginHost` (a fresh process, as after a restart) asked `[0.25, 2.0]` answers the same frames.
  - `render.rs` `the_avatar_moves`: the synthetic avatar with the clip at 0 s and 1 s draws different pixels; 1 s drawn twice is equal.
- [ ] **Step 2:** `env CELESTA_REQUIRE_GPU=1 cargo test --manifest-path plugins/vrm/Cargo.toml --test samples --test process --test render` with the samples → the new tests fail.
- [ ] **Step 3:** Fix what they find, each fix with a unit test in its module. **Step 4:** Pass with and without the samples; then the task checks.
- [ ] **Step 5:** Commit `test(plugins): VRM samples with clips, constraints, and springs`.

### Task 9: Docs

**Files:**
- Create: `docs/handoff/2026-11-03-plugin-vrm-motion.md` (clips and retargeting, motions and fades, tracks and their axes, constraints and their order, spring bones (the step, colliders, centre space), the 60 Hz clock and checkpoints, what steps do not see, the component's new props; What landed; Not done; Where it knowingly differs from UniVRM; Checks)
- Modify: `docs/handoff/2026-11-03-plugin-vrm.md` (Not done: motion, constraints, springs now point to the new note), `packages/website/content/en/docs/05-(more)/05-plugins.md`, `packages/website/content/ja/docs/05-(more)/05-plugins.md` (the VRM section: `motions`, `tracks` with a head-turn example, spring bones and that any frame is the same however it is reached), `skills/celesta/references/plugins.md` (the same)

**Not done** lists at least: `VRMC_springBone_limit`; the VRMA `lookAt` animation; springs that ignore gaze and expressions; spring recordings against UniVRM not yet compared; IK and arbitrary-skeleton retargeting (non-goals). **Knowingly differs:** slerp instead of Unity's per-component quaternion curves; explicit expressions winning over a clip's; a fixed 60 Hz clock instead of per rendered frame; `FromToRotation`'s half-turn axis.

- [ ] **Step 1:** Write the docs. **Step 2:** `pnpm --dir packages/website run build` passes.
- [ ] **Step 3:** Commit `docs: VRM motion, constraints, and spring bones`.

### Finish

- [ ] Full checks once: the plugin's fmt, clippy, and `env CELESTA_REQUIRE_GPU=1 cargo test --manifest-path plugins/vrm/Cargo.toml --locked` with and without `CELESTA_VRM_SAMPLES_DIR`; `pnpm --filter celesta-plugin-vrm-js test`.
- [ ] Export the demo with a clip (`cargo run -p celesta-exporter -- --react plugins/vrm/demo.tsx demo.mp4 --plugin-dir plugins/vrm/dist`) and a still at frame 60 before (the core layer's branch) and after.
- [ ] Push and open the PR stacked on the core plan's branch (`gh stack`), attaching both stills with `gh pr create --attach`.
