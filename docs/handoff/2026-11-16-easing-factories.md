# Easing factories (2026-11-16)

Design: [`docs/superpowers/specs/2026-11-16-animation-text-helpers-design.md`](../superpowers/specs/2026-11-16-animation-text-helpers-design.md), part 1.

## Specification

`Easings` in `@celesta.studio/react` gains four functions that return
easing curves:

| Call | Returns |
| --- | --- |
| `Easings.cubicBezier(x1, y1, x2, y2)` | The curve of CSS `cubic-bezier()`. `x1` and `x2` must be in 0–1; all four finite. |
| `Easings.steps(count)` | A staircase of `count` equal steps: `floor(t * count) / count`, and 1 at the end. `count` is a positive integer. |
| `Easings.back(overshoot = 1.70158)` | `{ easeIn, easeOut, easeInOut }` with that overshoot (finite, at least 0). |
| `Easings.elastic({ amplitude?, period? })` | `{ easeIn, easeOut, easeInOut }`; `amplitude` at least 1, `period` above 0. |

Every returned curve is exactly 0 at 0 and exactly 1 at 1. The named curves
are unchanged. The factories are JavaScript functions for `progress()`,
`interpolate()`, and any prop that takes `(t) => number`; JSON keyframes and
`frameKeyframes()` still take the named curves only, and the Rust `Easing`
enum is untouched.

## Implementation

All in `packages/react/src/animation.ts`.

- `cubicBezier` finds the curve parameter whose x is `t` by bisection (53
  halvings, the precision of a double). x never decreases while `x1` and
  `x2` are in 0–1, so this converges for any control points, including
  vertical tangents. Inputs at or below 0 return 0 and at or above 1 return
  1.
- `back(overshoot)` uses the arithmetic of the named `Back` curves with the
  constant replaced, so `Easings.back()` equals them bit for bit.
- `elastic()` without options returns the named curves themselves. With an
  option it uses Penner's form: the swing is
  `sin((t - s) * 2π / period)` with `s = period / 2π * asin(1 / amplitude)`,
  scaled by `amplitude`. A given `period` applies to `easeIn`/`easeOut` and
  `period * 1.5` to `easeInOut`, the ratio of the named curves (0.3 and
  0.45).
- `EASING_FACTORY_NAMES` (exported from `animation.ts`, not from the
  package) lists the four names for code that walks `Easings` as a map of
  curves. `packages/react/test/easings.test.mts` and
  `packages/cli/test/motion.test.mts` skip them.

## Checks

- `pnpm --filter @celesta.studio/react exec vitest run test/easing-factories.test.mts`:
  default equality with the named curves over a sweep, overshoot ordering,
  CSS `ease` at 0.5, extreme control points, step values, exact endpoints,
  and argument validation.
- `pnpm --filter @celesta.studio/react test` and
  `pnpm --filter @celesta.studio/cli exec vitest run test/motion.test.mts`.
