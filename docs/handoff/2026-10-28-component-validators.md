# Shared prop checks in @celesta/components (2026-10-28)

The number checks and padding expansion that `@celesta/components` copied
into each component now come from one helper, and the local `clamp01`
copies use `@celesta/math` (issue #575).

## The helper

`packages/components/src/validate.ts` (internal, not exported from the
package): `validators(component)` returns `{ fail, finite, nonNegative,
positive, insets }`, each throwing `<component> ...` with the value it was
given:

| Check | Message |
| --- | --- |
| `fail(message)` | `<C> message` |
| `finite(name, v)` | `<C> name must be a finite number, not v` |
| `nonNegative(name, v)` | `<C> name must be a finite number of 0 or more, not v` |
| `positive(name, v)` | `<C> name must be a finite number greater than 0, not v` |
| `insets(padding)` | a number on every edge, missing edges 0, each `nonNegative('padding <edge>', ...)` |

Components call it once at module scope
(`const { fail, nonNegative, insets } = validators('Panel')`); `BarChart` /
`Ranking` call it per render with the component name they were given, as the
old `check(component)` did.

## Callers

- Replaced local copies: `badge`, `callout`, `panel` (`paddingInsets` is now
  `insets`), `media-frame`, `stat-card`, `caption-track` (`fail`,
  `positive`), `pattern-background` (`requirePositive` / `requireFinite`),
  `bar-chart` (`check`).
- `text-block.ts` no longer exports `positive` / `nonNegative`; `title-card`
  and `lower-third` use `validators` instead.
- `clamp01`: `components/src/motion.ts`, `transitions/src/transition.ts`,
  `transitions/src/built-in-transitions.ts` import it from `@celesta/math`
  (same formula).

## Message changes (intentional)

- `<TitleCard>` / `<LowerThird>`: `... must be a finite number greater than
  0` and `... must be a finite non-negative number` now read like the other
  components: `greater than 0, not v` and `of 0 or more, not v`.
- `<PatternBackground>`: `must be a finite number above 0` is now `greater
  than 0`.
- `<Callout>` padding / sizes given `NaN` or `Infinity` now report `of 0 or
  more` (as the other components did) rather than the bare `finite number`.

Only `test/pattern-background.test.mts` quoted the old wording; it now
expects `greater than 0`. The docs and the Agent Skill quote none of it.

## Left alone

- `caption-track`'s `edges()` spreads `margin` without range checks.
- `typewriter-text`'s `positive` (optional value, different message),
  `motion`'s envelope checks, and `count.ts`'s `${caller}`-prefixed checks
  keep their own contracts.
- `@celesta/layout` `safeAreaInsets()` also takes size presets and has its
  own errors, so padding does not route through it.
- The other packages' "must be a finite" checks (`layout`, `shapes`,
  `transitions`, `character`, `react`, `math`) are out of scope.

## Checks

```bash
pnpm run build:runtime
pnpm --dir packages/transitions test
pnpm --dir packages/components test
```

`test/validate.test.mts` covers the helper's messages and padding expansion.
No user-facing behavior beyond error wording changed, so the docs site and
the Agent Skill are untouched.
