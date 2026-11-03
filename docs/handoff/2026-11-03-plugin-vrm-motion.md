# VRM motion, constraints, and spring bones (2026-11-03)

The VRM plugin's avatars move. VRM animation clips (`.vrma`) play on the
composition's timeline, retargeted as UniVRM retargets them; keyframed
humanoid tracks add to them; the avatar's node constraints follow; and its
spring bones swing on a fixed 60 Hz clock, so any frame, asked for in any
order and at any frame rate, is the same.

This is the second part of phase 3 of
`docs/superpowers/specs/2026-11-03-plugin-3d-content-vrm-design.md`, built
on `2026-11-03-plugin-vrm.md` (loading, materials, expressions, the lookAt,
the camera, `<Vrm>`). The plan is
`docs/superpowers/plans/2026-11-03-plugin-vrm-motion.md`. As there, every
rule is written in glTF's axes (right-handed, +Y up, metres, the avatar
facing +Z with its left at +X), UniVRM decides wherever the specification
leaves room, and files ported from UniVRM carry its MIT notice and name
their source.

## Each frame

`VrmModel::frame(time, data)` runs UniVRM's order
(`Vrm10Runtime.Process`):

1. **The base pose:** the motions at `time`, the tracks on top, retargeted
   onto the avatar (`model.rs` `base_pose`).
2. **Constraints** on that pose (`constraints.rs`).
3. **The lookAt**, then **expressions**, as in `2026-11-03-plugin-vrm.md`;
   a clip's expression weights are the ones `data.expressions` does not
   set.
4. **Spring bones:** each head turned to its tail after the last 60 Hz
   step at or before `time` (`springs.rs`, `stepping.rs`).
5. World matrices, skinning matrices, and draw states.

A frame depends only on the model, `time`, and `data`. Nothing in it
depends on frames asked for before, so a frame scrubbed to, exported at
30 fps, or exported at 60 fps is the same, bit for bit.

## Clips and the retarget (`vrma.rs`)

Ported from `VrmAnimationImporter.cs`, `InitialRotationPoseProvider.cs`,
`BoneInitialRotation.cs`, `Vrm10Retarget.cs`, `Vrm10ControlBone.cs`, and
`Vrm10RuntimeControlRig.cs`.

- **Reading.** A `.vrma` is binary glTF (or glTF JSON with `data:`
  buffers) with `VRMC_vrm_animation` `specVersion` 1.0 (or a `1.0-`
  pre-release such as `1.0-draft`, as for `VRMC_vrm`), read through the
  model's file and JSON limits. Without the extension:
  `this glTF file has no VRMC_vrm_animation extension: it is not a VRM
  animation`; another version: `VRMC_vrm_animation specVersion {v} is not
  1.0`. Only `animations[0]` is read: the `rotation` of each humanoid
  bone's node, the `translation` of the hips' node, and for each
  expression the `translation` x of its node. Other channels, the eye
  bones (UniVRM's importer leaves them out), the look expressions (UniVRM's
  clip has no field for them), and the lookAt are not read. A humanoid
  with bones but no hips is refused. `duration` is the last key time over
  the channels read.
- **Hostile clips.** Key times must be finite and rise, values finite,
  rotations of a non-zero length whose square is finite (finite components
  past about 1e19 overflow it and would normalize to 0); two channels of
  one path on one node are refused. A clip keys at most `MAX_CHANNELS` (1024) bones and expressions
  and holds at most `CLIP_BUDGET` (256 MiB) of keys. A clip that moves the
  hips must rest them at least 1 mm up (its travel is divided by that
  height).
- **Sampling.** Times clamp to a channel's first and last key (the keys
  themselves, never a blend). `STEP` holds; `LINEAR` lerps translations
  and weights and slerps rotations along the shorter arc; `CUBICSPLINE` is
  glTF's Hermite spline with tangents scaled by the key interval, a
  rotation normalised (its start key when it degenerates).
- **Normalized rotations** (`BoneInitialRotation.NormalizedLocalRotation`):
  a clip bone resting at local `L₀` and world `G₀` turns by
  `q = G₀ · L₀⁻¹ · L(t) · G₀⁻¹`, its turn from rest in the clip's root
  axes, whatever its rest. So a clip recorded on a rig whose bones rest
  rotated (Blender's, VRoid's) lowers an arm instead of twisting it.
- **Onto the avatar** (`Vrm10ControlBone.ProcessRecursively`): an avatar
  bone resting at local `L₀'` and world `G₀'` takes
  `local = L₀' · (G₀'⁻¹ · q · G₀')`. A bone the clip does not key rests
  (`q = 1`); nodes that are not humanoid bones keep the pose's values.
- **The hips** (`Vrm10Retarget.Retarget`): with `H₀` the clip's hips at
  rest in its root space and `H₀'` the avatar's,
  `world = H₀' + (raw − H₀) · (H₀'.y / H₀.y)`: the clip's travel from its
  own rest, scaled by the avatar's hips height over the clip's. It is
  written as the hips' local translation under their parent's current
  world. A clip without hips translation leaves the hips at rest. UniVRM
  takes `H₀` from the clip's hierarchy when it is loaded, before it plays
  (`Vrm10AnimationInstance.Initialize`, which expects that hierarchy in
  T-pose), so a clip whose rest hips stand higher or lower than its
  standing animation moves every avatar by that offset, in UniVRM as here
  (see Not done).

## Motions (`motion.rs`)

- **Assets.** A layer's clips are its assets `motion0`, `motion1`, …
  (decimal, no leading zeros), each one `.vrma`, opened before the model,
  so a refused clip writes no texture. Besides `model`, any other asset
  name is refused (`unknown asset "{name}"`). A layer opens at most
  `MAX_CLIPS` (256) clips, whose files take at most 1 GiB and whose keys
  at most 1 GiB together; the count is checked before any file is read,
  and a clip that fails is named by its path.
- **`data.motions`:** `[{ clip, from, loop?, fadeIn? }]`, `from` and
  `fadeIn` in seconds of the layer's clock. Every motion is checked every
  frame, whatever its time: its clip must be one the layer opened, `from`
  finite, `fadeIn` finite and not negative.
- **Order.** Motions run in `from` order, stable for equal `from`. At `t`
  the active motion is the last with `from ≤ t`; before the first, the
  avatar rests and no clip expression applies. A motion's local time is
  `τ = t − from`: looped, `τ mod duration` (0 for a clip of no length);
  otherwise `min(τ, duration)`, so its last pose holds until the next
  motion.
- **Fades.** While `τ < fadeIn`, the pose blends from the motion before it
  (evaluated at `t` by these same rules, so fades chain, or the rest pose
  when there is none) with `w = τ / fadeIn`: each bone
  `slerp(q_prev, q_cur, w)` (identity where one lacks the bone), the hips
  `lerp` (rest where one lacks them), expressions `lerp` (0 where one lacks
  them).
- **Hips of different rigs.** Each motion's hips are put in a unit rig,
  `(0, 1, 0) + (raw − H₀) / H₀.y`, before blending, so clips recorded on
  rigs of different sizes blend; the retarget scales the result to the
  avatar's hips height.
- **Expressions.** A clip's expression weights fill the keys
  `data.expressions` leaves out; a key the layer sets wins.

## Tracks (`tracks.rs`)

- **`data.tracks`:** `{ bone: { pitch?, yaw?, roll?, x?, y?, z? } }`, each
  axis a `KeyframeAnimation` on the layer's clock, read at the frame's time
  to the microsecond (`Time::new((seconds × 1e6).round(), 1_000_000)`), as
  the Live2D plugin reads its tracks. Keys are VRM humanoid bone names; a
  bone the avatar lacks is skipped.
- **Rotations** are degrees, applied after the motion in the bone's
  normalized frame (its turn from rest in the model's axes, as a clip's):
  `q ← q · Ry(−yaw) · Rx(−pitch) · Rz(roll)` in glTF's axes (Unity's
  `Euler(pitch, yaw, roll)` order). The angles turn as the `lookAt`
  prop's do (UniVRM's): with the avatar facing +Z, its right toward −X,
  positive yaw turns the face to the avatar's right, positive pitch looks
  up, and positive roll tilts its top toward the avatar's right.
- **The hips alone** take `x`, `y`, and `z`: metres added to their
  position in model space after the retarget (the rest position when no
  motion moves them). Tracks apply without any motion too.
- **Checks**, every frame whatever its time, each naming its key
  (`tracks.{bone}.{axis}`): the bone name, `x`/`y`/`z` on the hips only,
  at least one keyframe, a non-zero timescale, keyframes in time order,
  values finite as `f32`, and the eased value finite at the frame's time.
  A spring step reads the tracks its request checked once.

## Constraints (`constraints.rs`)

Ported from `Vrm10RollConstraint.cs`, `Vrm10AimConstraint.cs`,
`Vrm10RotationConstraint.cs`, `Vrm10Runtime.cs`, and `Vrm10Importer.cs`.

- **Reading** (`LoadConstraintAsync`): each node's
  `VRMC_node_constraint` 1.0 with exactly one of `roll`, `aim`, and
  `rotation`. A source that is not a node of the file or is the node
  itself, and a weight that is not finite, are refused. The weight
  defaults to 1 and is used as it is: the schema bounds it to 0..1, but
  UniVRM's importer copies it unchecked and `Process` calls
  `SlerpUnclamped`, so a weight past 0..1 extrapolates there and here
  alike.
- **Order:** each constrained node once, in node pre-order (UniVRM's
  `GetComponentsInChildren`), reading the pose as the constraints before
  it left it. A destination whose source an earlier constraint moved sees
  the move; a cycle through sources is no loop.
- **Rules,** with `D₀` and `S₀` the destination's and source's local rest
  rotations and `w` the weight:
  - rotation: `slerp(D₀, D₀ · (S₀⁻¹ · S), w)`;
  - roll: the source's turn from rest in the destination's rest frame,
    less its swing of the roll axis;
  - aim: the destination's rest axis, under its parent's current world
    rotation, turned toward the source's current world position (`from`
    when they coincide).
- **Work.** Only an aim reads world transforms, climbing from its
  destination's parent and its source to the root. The climbs' sum is
  fixed at load; a model whose sum passes `AIM_CLIMB_LIMIT` (2^20) is
  refused.
- Seed-san's robot arm follows her left arm through rotation constraints
  in every clip; the twist sample's roll nodes take their sources' twist
  at their weights (1 and 0.5).

## Spring bones (`springs.rs`)

Ported from UniVRM's FastSpringBone (`UpdateFastSpringBoneJob.cs`,
`SpringBoneCollision.cs`, `FastSpringBoneBuffer.cs`,
`FastSpringBoneConbinedBuffer.cs`, `BlittableTransform.cs`,
`MathHelper.cs`, `FastSpringBoneBufferFactory.cs`) and
`Vrm10Importer.LoadSpringBoneAsync`.

- **Reading,** as UniVRM's importer: a joint, collider, or centre naming a
  node the file lacks is refused; a joint on a node an earlier joint took
  is skipped; a spring without joints, and an index to a collider group or
  collider the file lacks, are skipped; a spring with fewer than two
  joints moves nothing and is dropped. A number not finite in `f32` is
  refused. A step checks at most `SPRING_WORK_BUDGET` (2^20)
  joint–collider pairs over every spring, or the model is refused.
- **The step** (`UpdateFastSpringBoneJob.Execute`), springs in order and
  each spring's joints in order:
  `tail = cur + (cur − prev) · (1 − drag) + (parent rotation · rest
  rotation · bone axis) · stiffness · Δt + gravityDir · gravityPower · Δt`,
  the inertia not scaled by the step; the tail kept at the bone's length;
  then each collider in group and collider order; then the head turned so
  its bone points at the tail (`FromToRotation`, ported with its edge
  cases), written as its local rotation.
- **Colliders:** spheres and capsules (a capsule's closest point to the
  joint's *head*), pushed out by `hitRadius + radius × the node's largest
  scale` and back to the bone's length; and
  `VRMC_springBone_extended_collider`'s planes and inside spheres and
  capsules, which replace the base shape and push without renormalising.
- **Centre space.** A spring with a `center` keeps its tails in that
  node's space, so the centre's own motion does not swing them.
- **What a step reads:** the world matrix and rotation of each head, its
  parent, each centre, each collider's node, and their ancestors, taken
  once from the step's pose (UniVRM's `Transforms` array); a head is
  updated in place once it turns. Only those nodes are placed, which is
  bit-identical to placing every node (tested).
- **Not read:** `VRMC_springBone_limit` (UniVRM's angle limits), so a
  joint carrying it swings past its limits, and the model warns: `N
  spring joint(s) carry VRMC_springBone_limit, which the VRM plugin does
  not read: they swing past their angle limits`. UniVRM's runtime options
  (external force, scaling with the model, stopping the write-back) stay
  at their defaults.
- **Versions:** as in UniVRM, neither `VRMC_springBone`'s nor
  `VRMC_springBone_extended_collider`'s `specVersion` is checked (UniVRM's
  importer checks only `VRMC_springBone_limit`'s, with a warning).

## The clock (`stepping.rs`)

- **60 Hz from 0 s.** A frame at `t` has run steps `0..=n`,
  `n = ⌊max(t, 0) × 60 + 10⁻⁶⌋` (the leeway puts a frame at `k / 30` s on
  step `2k`). Step `k` is at `k / 60` s with `Δt = 1/60`. Before step 0 the
  tails start still from the pose at 0 s (`InitCurrentTailsJob`), and step
  0 integrates once, as UniVRM's first frame does. The frame then takes its
  own pose at `t` and turns the heads to step `n`'s tails, so a time
  between steps shows the last step's tails on the current pose.
- **What a step sees:** the pose at `k / 60` from the motions, tracks, and
  constraints alone, never the step before's turned pose. The lookAt and
  expressions are a frame's own values, which earlier steps cannot be
  given; in a VRM 1.0 avatar they turn no bone but the eyes, so a spring
  hanging from an eye bone swings as if the eye rested (tested).
- **The frame contract.** A frame depends on the model, `t`, and `data`
  alone. Kept states only save work; no frame depends on them, and a
  frame refused cold is refused warm.

### Kept states

- **A performance** is what the steps read beyond the model: the layer's
  `motions` and `tracks`, keyed by a SHA-256 of their JSON (equal keys are
  equal data; data that means the same written differently, as `-0.0` and
  `0.0`, only misses). Each keeps a **cursor**, the state its last request
  ended on, which every request replaces, and **grid checkpoints**, the
  states after each step `k` with `(k + 1) % (30 × spacing) == 0`
  (`CHECKPOINT_STEPS` 30). A request resumes from the later of its cursor
  (if at or before its step) and the latest checkpoint at or before its
  step, else from step 0. So an export steps each performance once (two
  steps a frame at 30 fps), and a scrub back resumes at most 30 steps
  early (`30 × spacing` once the ceiling has thinned the checkpoints).
- **One ceiling per process.** `SPRING_CACHE_BYTES` (256 MiB) over every
  model and performance; nothing counts performances. It is a ceiling
  against hostile files (a ten-minute layer keeps about 1 MB) and costs
  latency only. Past it, in order: the performance with the most
  checkpoints doubles its spacing, repeatedly; then checkpoints go and
  cursors stay; then the least recently used performances go. A request's
  own performance is out of the cache while it steps, so its own request
  never evicts it; it keeps checkpoints only within what the others leave,
  and its cursor always. The states kept are therefore at most the
  ceiling plus one cursor.
- The provider's models share one cache (`SharedSpringCache`); a model
  that closes takes its states with it. A failed request puts back the
  checkpoints it made before failing and keeps its previous cursor.
- **Keep `motions` and `tracks` constant across a layer.** Data that
  changes every frame (computed from the frame's time) is a new
  performance each frame: every frame then steps cold from 0 s and an
  export becomes quadratic in its length, bounded per frame only by the
  work fence. Per-frame values belong in `expressions`, `mouth`, and
  `lookAt`, which the steps never read.

### The work fence

A request from a cold start runs every step up to its time, so a frame is
refused before any state is looked up:

- **Past `MAX_SPRING_STEPS`** (4 h at 60 Hz, 864,000 steps): `this frame
  is {t} s into the layer, past the 14400 s up to which the VRM plugin
  steps spring bones (864000 steps at 60 Hz)`.
- **Past `MAX_STEP_WORK`** (2^33 units): when `(n + 1) × cost` passes it,
  the cost of a step being
  - the model's, fixed at load: `300 + nodes + moving joints + 3 ×
    joint–collider pairs + aim climbs`;
  - plus its data's, charged as measured: `16 × (1 + the clip's channels)`
    for each motion (a step in a fade may sample every motion), and
    `32 + ⌈keyframes / 8⌉` for each track axis.
  The refusal names the latest time reachable with these motions and
  tracks, the steps, the cost per step, the work, and the budget.
- A unit measured a few nanoseconds of one laptop core (release build) on
  Seed-san and UniVRM's constraint twist sample, with and without a clip,
  so the worst frame accepted from a cold start (2^33 units) takes under a
  minute. Each data charge (16 a clip channel and one for the motion, 32 a
  track axis, one per 8 keyframes) is within 2× of what those two samples
  measured. Seed-san (610 units a step) reaches 65 h alone and 27 h with a
  clip; the twist sample (2,183) 18 h and 13 h; a file of a million empty
  nodes about 2.4 min.
- **The reach warning.** A model with springs whose own cost per step
  reaches less than 4 h (from 9,943 units a step) says so when it opens:
  `this avatar's spring bones are stepped for frames up to {s} s into a
  layer (less with motions and tracks), short of the 14400 s …`. A model
  without springs has neither limit.
- Both refusals depend on the model, the data, and the time alone.

## `<Vrm>`

New props of `@celesta-plugin/vrm`'s `Vrm`:

| Prop | What it does |
| --- | --- |
| `motions` | `[{ src, from?, loop?, fadeIn? }]`: `src` a `.vrma`; `from` (default 0) and `fadeIn` (default 0) in frames of the enclosing sequence, sent as seconds. Each distinct `src` (`===`) is sent once, as asset `motion<i>`, `i` the index of the first motion with that `src`. `from` not finite, or `fadeIn` not finite or negative, throws. |
| `tracks` | `{ bone: { pitch?, yaw?, roll?, x?, y?, z? } }`, each axis a list of `{ frame, value, easing? }`. Degrees for the turns, metres for `x`/`y`/`z` (hips only). Sent as `KeyframeAnimation`s: at whole frame rates with the frame rate as the timescale, otherwise in microseconds. A fractional `frame` is rounded to the nearest frame at any rate. An unknown bone or axis, `x`/`y`/`z` off the hips, an empty axis, or a frame or value not finite throws. |

`demo.tsx` plays `test.vrma` on a loop with a nod track on top.

## What landed

| Where | What |
| --- | --- |
| `plugins/vrm/src/vrma.rs` | Clips, their limits, sampling, and UniVRM's retarget. |
| `plugins/vrm/src/motion.rs` | Clip assets and their limits; motions, loops, holds, and fades. |
| `plugins/vrm/src/tracks.rs` | Tracks: checks, turns in the normalized frame, hips moves. |
| `plugins/vrm/src/constraints.rs` | Roll, aim, and rotation constraints in pre-order, and the climb limit. |
| `plugins/vrm/src/springs.rs` | Spring bones: reading, the step, colliders, centre space, the node closure, the limit warning. |
| `plugins/vrm/src/stepping.rs` | The 60 Hz clock, the shared cache of cursors and checkpoints, the work fence, the reach warning. |
| `plugins/vrm/src/model.rs`, `provider.rs` | The frame's stages; `PerformanceData.motions` and `.tracks`; clip assets; the provider's shared spring cache. |
| `plugins/vrm/src/synthetic.rs` | `SyntheticVrma`, a clip built in code; the synthetic avatar's hair spring. |
| `plugins/vrm/js`, `demo.tsx`, `README.md` | `motions` and `tracks`; the demo's clip and nod; where to get clips. |
| `plugins/vrm/tests/samples.rs`, `process.rs`, `render.rs` | Real clips and constraints on the official samples; motions across the process; a moving avatar drawn. |

## Not done

- **`VRMC_springBone_limit`** is not read; the model warns when a joint
  carries it.
- **The VRMA `lookAt` animation** is not read, nor glTF animations other
  than a clip's first.
- **Springs ignore the gaze and expressions:** a step sees no lookAt and
  no expression, so a spring hanging from an eye bone swings as if the eye
  rested.
- **Spring recordings against UniVRM** (a reference Unity scene stepping
  the same clip at 60 Hz) have not been compared.
- **IK and arbitrary-skeleton retargeting** are non-goals: clips are VRM
  humanoid clips, retargeted by rotation.
- **No foot IK.** The hips' travel scales by hips height while the legs
  fold by the clip's knee angles, so a squat sinks an avatar whose leg
  length over hips height differs from the clip rig's: with VRoid
  Project's calisthenics clip, Seed-san sinks about 1.5 cm at the bottom
  of the squat and the twist sample about 3.4 cm.
- **A clip's rest hips offset.** A clip whose rest hips stand higher or
  lower than its standing animation moves every avatar by that offset:
  VRoid Project's motion pack clips 01–04 rest their hips 3.9 cm above
  their standing pose, so the feet sit 3–4 cm into the floor, and clips
  05–07 1–2 cm above it. This is UniVRM's arithmetic
  (`Vrm10Retarget.Retarget` measures the travel from the clip's rest),
  kept for parity; a grounding option would be a deviation.
- **Model colliders.** Springs pass through the body where a model has no
  collider (the twist sample's front hair into its shirt), and a torso
  straightening through a hair spring's colliders flicks it out (Seed-san's
  tail hair in a jump). Both are the models' collider data, stepped as
  UniVRM steps it.
- **A renderer nondeterminism** seen once: the same exporter still of one
  pose differed in 64 pixels between runs where two near-coplanar surfaces
  meet, while the plugin's scene data was bit-identical. It is Celesta
  core's, tracked separately.
- **Cold frames cost O(t).** Springs are defined as stepped from the
  layer's 0 s, so a cold frame far into a long layer steps everything
  before it: at the 4 h ceiling, about 6% of the fence's work for Seed-san
  and about 22% for the twist sample (before any motion or track), a
  fraction of the minute the fence allows. An opt-in warm-up window, stepping from a fixed few seconds
  before the requested time and from that time's pose, would make a cold
  frame's cost constant, since damped springs forget their history within
  a second or two; the price is exact continuity, as adjacent frames'
  windows differ and could leave a small seam. Recorded as a trade-off,
  not planned.
- The cost constants of the work fence are estimates within a factor of
  two, measured on one machine.

## Where it knowingly differs from UniVRM

- **Rotation keys are slerped** along the shorter arc, as glTF asks;
  UniVRM's Unity curves lerp a quaternion's components and normalise, a
  difference far below a degree at a clip's key rate.
- **Expressions the layer sets win over a clip's,** key by key; UniVRM
  sets a clip's weights every frame.
- **A fixed 60 Hz clock.** UniVRM steps springs once per rendered frame,
  so its motion depends on the frame rate; here a 30 fps and a 60 fps
  export show the same motion.
- **`FromToRotation`'s half-turn axis.** For opposite vectors the
  constraints turn half a turn about `normalize(cross(from, +X))`, else
  `cross(from, +Y)`; Unity's `Quaternion.FromToRotation` does not document
  its axis there. (The springs port UniVRM's own `MathHelper` version.)
- **`PushTransformJob` is not emulated.** UniVRM writes each spring head's
  *world* rotation back; the plugin writes its *local* rotation, which is
  the same wherever a head's parent is the previous joint or a node no
  spring turns.
- **The steps ignore the lookAt and expressions** (above).
- **A clip whose hips rest below 1 mm** is refused; UniVRM would divide by
  that height.
- **`<Vrm>` rounds a fractional keyframe frame to the nearest frame at any
  rate**, while `<Live2D>` keeps it to the microsecond at frame rates that
  are not whole numbers.

## Checks

- `cargo test --manifest-path plugins/vrm/Cargo.toml --locked`, with
  `CELESTA_REQUIRE_GPU=1`, with and without `CELESTA_VRM_SAMPLES_DIR`;
  the plugin's `cargo fmt --check` and `cargo clippy --all-targets
  --locked -- -D warnings`; `pnpm --filter celesta-plugin-vrm-js test`.
  Lib 303, `mtoon` 55, `process` 2, `render` 2, `samples` 6, JS 16.
- Unit tests, on the synthetic avatar and clip:
  - `vrma`: bones, hips, and expressions read; normalized rotations onto
    the avatar (copying local rotations would put the forearm elsewhere);
    rest rotations normalised away; hips travel scaled by hips height;
    `STEP` and `CUBICSPLINE`; not a VRMA; bad times, values, samplers,
    bones, hips heights, channel counts, and key budgets refused.
  - `motion`: starts, loops and holds, a clip of no length, fades, chained
    fades, hips of rigs of different sizes, equal `from`s, clip
    expressions yielding to the layer, unknown clips, bad times, clip
    names, and the count, bytes, and broken-file refusals.
  - `tracks`: a head turn, pitch and roll directions, the same turn as
    `lookAt`'s for positive yaw and pitch, composition after the motion
    (a roll on the clip's turn, and a yaw that does not commute with it),
    hips moves, bones the avatar lacks, bad tracks, the data shape.
  - `constraints`: each kind, weights defaulting to 1 and extrapolating,
    twist through rest rotations, aims following a parent turned this
    frame, opposite and coincident aims, `from_to_rotation`, pre-order
    chains, cycles, a 100,000-deep chain in one pass, the climb limit,
    bad constraints; Seed-san's robot arm and the twist sample at rest.
  - `springs`: logic at rest, inertia not scaled by the step, each
    collider shape, centre space, extended colliders, head turns and
    chains, the node closure bit-identical to every node, determinism,
    UniVRM's skip and refusal rules, the pair budget.
  - `stepping`: frames equal in any order; 30 and 60 fps agree; states
    kept by motions and tracks; per-frame values and a gaze leave the
    springs alone; between steps the last step holds; sequential frames
    step only the delta (60 s at 30 fps: 3,599 steps); twelve interleaved
    performances keep each request at two steps; the ceiling costs
    latency only; models share the ceiling and release it; a performance
    failing at step 0 leaves the cache alone, and one failing mid-run
    keeps its cursor and fails warm as cold; the 4 h and work-fence
    refusals; the reach warning; many nodes shorten the reach.
  - `model`: the spring-limit warning.
- `tests/samples.rs` (with the samples folder): `test.vrma` moves
  Seed-san, and frames asked out of order on a model warmed by another
  performance equal fresh in-order frames; the twist sample's constraints
  follow their sources.
- `tests/process.rs`: motions cross the plugin's process, frames equal to
  the provider's. `tests/render.rs`: the avatar drawn with a clip moves.
- JS: motions name their clips once, their times are frames, tracks are
  keyframes on the layer's clock (fractional frames rounded at 30 and
  29.97 fps), bad tracks throw.
- By hand: VRoid Project's motion pack (seven clips) on Seed-san and the
  twist sample, at 30 and 60 fps, every frame probed: no frame error or
  non-finite vertex, natural limbs and fingers, the spin's yaw following
  the clip within about 1°, springs settling within 12 to 15 frames of a
  loop's jump, and the plugin's frame data bit-identical across frame
  rates and request orders.
