# VRM plugin: refuse degenerate data at load

Issue #678, from two review threads on #668. Both cases failed silently on
malformed files; they now refuse the file when it opens, as #668 already does
for quaternion keys whose length is not finite.

## Specification

- **Spring centre without an inverse.** `Springs::read` refuses a spring
  whose `center` node's rest world matrix has a zero determinant or a
  non-finite inverse: `VRMC_springBone spring {s} center, node {n}, cannot be
  inverted at rest: its world matrix is singular (a zero scale?)`. Refusing
  was chosen over treating the spring as centreless: the file is malformed,
  and a silent fallback would swing the joints differently from UniVRM's
  reading of the same file. Nothing animates a node's scale, so the rest
  decides.
- **Clip keys past a bound.** `vrma::Reader::read` refuses a sampler whose
  key times, values, or `CUBICSPLINE` tangents have a magnitude past
  `MAX_KEY` (1e6): `animation {a} sampler {i} has a {path} past ±1e6, the
  most a clip's keys may be` (key times: `... has a key time past ±1e6
  seconds ...`). Within that bound neither `LINEAR` (`a + (b − a)·s`) nor
  the Hermite spline (tangents scaled by the key interval, also within 1e6)
  overflows `f32`. Key times are bounded too because the spline multiplies
  tangents by the interval. The bound also makes the old overflow case of
  `rotations` (finite components whose squared length overflows) refused
  earlier, by the bound.

## Implementation

- `plugins/vrm/src/springs.rs`: the check after the centre's node index is
  read.
- `plugins/vrm/src/vrma.rs`: `MAX_KEY` and the two checks in `Reader::read`.
- `plugins/vrm/src/synthetic.rs`: `SyntheticVrma::tangent`, every
  `CUBICSPLINE` tangent component (0 in the reference clip).
- `plugins/vrm/src/motion.rs`: `hips_past_f32_are_refused` now reaches the
  frame-time hips overflow through an avatar whose hips rest 3e32 m high,
  which scales a clip's 1e6 m of travel to 3e38 m, since a clip alone can no
  longer key 3e38.
- User guide (en and ja) and `skills/celesta/references/plugins.md` list
  the new refusals.

## Checks

- `cargo test -p celesta-plugin-vrm` in `plugins/`: all pass, including
  `springs::tests::a_center_that_cannot_be_inverted_is_refused` and
  `vrma::tests::keys_past_the_bound_are_refused` (values, translations,
  rotations, key times, `CUBICSPLINE` tangents, and keys at the bound that
  still load).
- `cargo clippy -p celesta-plugin-vrm --all-targets`: no warnings.
