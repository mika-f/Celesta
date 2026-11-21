# Hold drift and the whip easing (2026-11-22)

Modeled on Motion's `kit.Camera`: its keys' `drift: [dx/s, dy/s, dlogz/s]`
and its whip ease `kit.eox`. Builds on
[`2026-11-16-keyframes-helper.md`](2026-11-16-keyframes-helper.md) and
[`2026-11-16-easing-factories.md`](2026-11-16-easing-factories.md).

## Specification

### Drift during a hold

`keyframes(frame, keys, options?)` takes `options.fps`, and a key takes
`drift?: KeyframeDrift<T>`:

- For a number value, `drift` is one `DriftRate`; for an object value, an
  object of `DriftRate`s by field. Fields left out do not drift.
- A `DriftRate` is a number, added per second (`value + rate * s`), or
  `{ log: r }`, which multiplies (`value * e^(r * s)`), Motion's `dlogz/s`,
  so a zoom creeps at an even pace. `s` is `(frame - key.frame) / fps`.
- A field drifts only while it holds after its key: up to the next key when
  that key has the same value for the field, or after the last key for
  ever. Holds are per field, so `zoom` may drift while `x` moves.
- Where a segment starts: the first key's value; for the second key of a
  cut, that key's value; otherwise where the previous segment ended. A
  drifting hold ends where the drift reached; a move ends exactly on its
  key. So the move after a drifting hold starts from the drifted value (no
  jump at the key), a hold of the same value after it keeps that value
  (drifting on only by its own key's `drift`), and a cut discards the drift.
- Before the first key nothing drifts.
- Throws, with the key's index: `drift` without a finite positive
  `options.fps`; a rate that is not a finite number or `{ log }` with a
  finite `log` and no other field; an object drift for a number value or a
  number drift for an object value; a field the value does not have; drift
  on a field that moves to the next key; drift on the first key of a cut.
- Keys without `drift` give exactly the values they gave before (the
  carried start is then always the key's own value), with or without `fps`.

Exported types: `DriftRate`, `KeyframeDrift`, `KeyframesOptions`.

### `whip`

`Easings.whip` and the JSON / Rust easing `whip` (`Easing::Whip`):

```text
whip(t) = (1 + tanh(7 (2t - 1)) / tanh 7) / 2,  exactly 0 at 0 and 1 at 1
```

Smooth, symmetric about (0.5, 0.5), monotonic; 98% of the move happens in
the middle third, and the speed at the middle is `7 / tanh 7` ≈ 7 times the
average (`easeInOutExpo`'s is about 6.9, with longer tails). The sharpness
is `WHIP_SHARPNESS` in both `packages/react/src/animation.ts` and
`crates/composition/src/animation.rs`. There is no sharpness factory: for
another shape, `Easings.cubicBezier` covers it.

## Implementation

- `packages/react/src/keyframes.ts`: values are handled as field records (a
  number is the field `''`). `driftRates()` validates each key's drift
  against the key after it. `valueAt()` walks the segments before the
  current one to find where it starts, then evaluates held fields with
  `drifted()` and moving fields with the existing mix. The walk is linear
  in the keys, like the validation already was.
- `whip` is added to `Easings` (`animation.ts`), `Easing` in
  `crates/composition/src/keyframe.rs` (so the generated TypeScript
  `Easing` has `'whip'`), `apply_easing` (its integral uses the numeric
  fallback), `packages/web` (`KeyframeEasing`, `EASINGS`), and the shared
  fixture `crates/composition/tests/fixtures/keyframe-evaluation.json`,
  whose values came from `packages/web`'s evaluator and are pinned by the
  Rust test.
- `packages/cli/examples/with-keyframes.tsx` now has a drifting hold
  (frames 45–75), a whip pan (75–95), and a drift after the cut on 120,
  which replaces the old "slightly different last key" drift.

## Checks

- `cargo test -p celesta-composition`: the fixture and the easing unit
  tests (midpoint, finite integrals) with `Whip`.
- `pnpm --filter @celesta.studio/react run codegen`, then
  `pnpm --dir packages/react test` (after `pnpm run build:runtime`):
  `test/keyframes-drift.test.mts` (additive and `{ log }` drift, the move
  starting from the drifted value with and without easing, chained holds,
  drift after the last key and out of order frames, cuts, per-field drift
  while another field moves, unchanged results without drift, and every
  error), `test/whip-easing.test.mts`, and `test/easings.test.mts`, which
  checks `whip` against the fixture.
- `pnpm --dir packages/web test`,
  `pnpm --filter @celesta.studio/cli exec vitest run test/motion.test.mts`,
  and `pnpm --dir packages/website test`.
- `node skills/celesta/scripts/inspect.mjs packages/cli/examples/with-keyframes.tsx --frames 0,45,60,74,75,85,95,119,120,179 --no-native`:
  the camera's scale is 2.2 on 45, 2.29 on 75 (the drift's `e^0.04`), 2
  after the whip on 95, 1.8 on the cut at 120, and 1.986 on 179.
- `celesta-exporter --react packages/cli/examples/with-keyframes.tsx --frames 45,74,85,120,179`,
  looked at before and after the change.
