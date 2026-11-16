# Animation and text helpers: keyframes, easing factories, typing times, reveal units

## Intent

People writing compositions in TSX keep rebuilding four small things by
hand: a value that moves through several keys with a different curve on
each leg, an easing with a tuned overshoot or a bezier copied from a design
tool, the frame each typed character appears on (to place a key click), and
a text reveal finer than one line. Each becomes one documented helper.

The four are independent and ship as four pull requests, in this order:
easing factories, `keyframes()`, typing times, reveal units. `keyframes()`
uses nothing from the factories beyond accepting any `(t) => number`.

## Goals

- `keyframes(frame, keys)` in `@celesta.studio/react`: the value at a frame
  from a list of keys with per-key easing, holds, and cuts.
- `Easings.cubicBezier`, `Easings.steps`, `Easings.back`, `Easings.elastic`:
  functions that return easing curves.
- `useTypewriter` reports the frame each character appears on and can pause
  after chosen characters; `typewriterFrames()` gives the same frames
  outside a component.
- `<TextReveal unit>` reveals by line (as today), word, phrase, or grapheme.

## Non-goals

- Parametric easings in JSON projects, `frameKeyframes()`, or the Rust
  `Easing` enum. They stay named curves. The factories are JavaScript
  functions for `progress`, `interpolate`, `keyframes`, and any prop that
  takes `(t) => number`.
- A camera-specific key API, or a `drift` setting: a slow move during a hold
  is two keys with slightly different values.
- Keyframed colors or strings in `keyframes()`. Use `interpolateColor`.
- Sound. `typewriterFrames()` only returns frames; the caller places
  `<Audio>`.

## Anti-goals

- Any change to what existing compositions draw. Every default reproduces
  today's output exactly: `useTypewriter` without `pauses`, `<TextReveal>`
  without `unit`, and every existing `Easings.*` curve.

## Constraints

- Pure functions of the frame, like everything else a composition calls.
- TypeScript only. No Rust or project-format change.
- Tests are Vitest, in TypeScript, importing `../src`.
- Each pull request updates the user guide in
  `packages/website/content/en/` and `ja/`, and the Celesta Agent Skill
  (`skills/celesta/references/animation.md`, `text.md`, `helpers.md`, and
  the package table in `SKILL.md` where a new name is exported), and records
  its note in `docs/handoff/`.

## Approaches

For the keyed value, three shapes were considered:

1. **A new function `keyframes(frame, keys)`** whose keys have the shape
   `frameKeyframes()` already uses. Chosen: one call moves several numbers
   together (a camera's `x`, `y`, `zoom`), it expresses a cut, and anyone
   who knows `frameKeyframes()` can already write it.
2. **An array for `interpolate`'s `easing` option.** Smallest change, but
   one number per call and no cut.
3. **`<Camera keys>`.** Closest to a camera rig, but useless for anything
   that is not a camera.

For parametric easings, extending the Rust `Easing` enum (so JSON projects
and `frameKeyframes()` take them too) was considered and rejected for now:
it changes the project format and needs an integral for each curve
(`easing_integral`, used by keyframed `playbackRate`). The helpers are for
people writing TSX; the enum can grow later.

The other three have one sensible shape each, following the code they
extend.

## Design

### 1. Easing factories (`packages/react/src/animation.ts`)

| Call | Returns | Notes |
| --- | --- | --- |
| `Easings.cubicBezier(x1, y1, x2, y2)` | `(t) => number` | CSS `cubic-bezier()` semantics. `x1` and `x2` must be in 0–1 and all four finite, or it throws. |
| `Easings.steps(count)` | `(t) => number` | `floor(t * count) / count`, and exactly 1 at `t = 1`. `count` is a positive integer, or it throws. |
| `Easings.back(overshoot?)` | `{ easeIn, easeOut, easeInOut }` | `overshoot` defaults to `1.70158`, must be finite and at least 0. |
| `Easings.elastic({ amplitude?, period? })` | `{ easeIn, easeOut, easeInOut }` | Penner's elastic with its usual parameters. |

- Every returned curve is exactly 0 at 0 and 1 at 1, as the named curves
  are.
- `Easings.easeInBack` / `easeOutBack` / `easeInOutBack` and the three
  elastic curves keep their current output bit for bit. Whether they are
  reimplemented through the factories is the builder's choice; a test pins
  the equality either way: `Easings.back().easeOut(t) ===
  Easings.easeOutBack(t)` over a sweep of `t`, and the same for
  `elastic()`.
- `packages/cli/test/motion.test.mts` and
  `packages/react/test/easings.test.mts` iterate `Easings` as a map of
  curves. They skip the four factory names; the parity test against the
  Rust enum is otherwise unchanged.

### 2. `keyframes()` (`packages/react/src/keyframes.ts`)

```tsx
import { keyframes, Easings, useCurrentFrame } from '@celesta.studio/react';

const cam = keyframes(useCurrentFrame(), [
  { frame: 0,   value: { x: 960, y: 540, zoom: 1 } },
  { frame: 100, value: { x: 880, y: 520, zoom: 1.12 }, easing: Easings.easeInOutCubic },
  { frame: 120, value: { x: 880, y: 520, zoom: 1.12 } },  // same value: a hold
  { frame: 120, value: { x: 300, y: 400, zoom: 1 } },     // same frame: a cut
]);
<Camera {...cam}>…</Camera>
```

- `keyframes<T extends number | Record<string, number>>(frame, keys): T`.
- A key is `{ frame, value, easing? }`. `easing` shapes the segment that
  **ends** at the key, as in `frameKeyframes()` and JSON keyframes. It is a
  function or one of the kebab-case names of the `Easing` type
  (`'ease-out-back'`), which map to the `Easings` curve of the same name.
  Default linear.
- Before the first key the value is the first key's; after the last, the
  last key's. `frame` may be fractional (inside `<MotionBlur>`).
- **Cut.** Two keys on one frame `f`: frames below `f` end on the first
  key's value, and frame `f` itself and later use the second. The new value
  is on screen on frame `f`, as a `<Sequence from={f}>` would show it. (At
  the single instant `t = f` the Rust evaluator returns the first value;
  that instant matters for audio, not for a frame.)
- An object value is interpolated field by field. An easing that overshoots
  overshoots every field.
- Validation, each throwing with the key's index, in the wording of
  `frameKeyframes()`: no keys; a non-finite `frame`, `frame` argument, or
  value; keys out of order; a third key on one frame; mixed number and
  object values; object values whose field names differ; an unknown easing
  name.
- `frameKeyframes()` is unchanged, and its `easing` stays names only.

### 3. Typing times (`packages/text/src/text-motion.ts`)

```tsx
const typed = useTypewriter('はい、できました。', { from: 10, framesPerChar: 2, pauses: { '、': 6, '。': 12 } });
typed.frames; // [10, 12, 14, 22, 24, …]: the frame each character appears on

const clicks = typewriterFrames('npm run build', { from: 10, framesPerChar: 2 });
clicks.map((at) => <Sequence key={at} from={at}><Audio src="./key.wav" /></Sequence>);
```

- New option `pauses?: Record<string, number>`: extra frames to wait
  **after** that character before the next one. Each name must be exactly
  one code point and each value finite and at least 0, or it throws.
- Today's division stays the source of truth. Character `i` is shown on a
  frame when `floor((frame - from - P(i)) / framesPerChar) >= i`, where
  `P(i)` is the sum of the pause frames of the characters before `i`.
  Without `pauses`, `P` is 0 and `length` is today's
  `floor((frame - from) / framesPerChar) + 1` on every frame, fractional
  frames inside `<MotionBlur>` included.
- `Typewriter` gains `frames: number[]`, one entry per code point, on the
  same clock as the hook's `frame` (so `from` is included). `frames[i]` is
  the first **whole** frame on which `length > i`, found with that same
  test rather than a separate formula, so float rounding (for example
  `framesPerChar: 1.1`) cannot make the two disagree. A test pins this
  invariant over a sweep of `framesPerChar` values. With `framesPerChar`
  below 1 several characters share a frame.
- During a pause the caret stays steady: the text is still being typed.
- `typewriterFrames(text: string, options?)` is the same calculation as a
  plain function for a plain string, for use where hooks cannot run or to
  place sounds. Options: `from`, `framesPerChar`, `pauses`.
- `<TypewriterText>` (`@celesta.studio/components`) and any other caller
  that forwards `framesPerChar` to the hook gains a `pauses` prop passed
  straight through.

### 4. Reveal units (`packages/text/src/text-motion.ts`)

```tsx
<TextReveal unit="phrase" stagger={3} maxWidth={880}
  style={{ fontFamily: 'Noto Sans JP', fontSize: 96, lineBreak: 'phrase' }}>
  速くて、カンタンで、コードで書ける。
</TextReveal>
```

- `unit?: 'line' | 'word' | 'phrase' | 'grapheme'`, default `'line'`, which
  renders exactly as today.
- Lines are found first, as today (`\n` and `maxWidth`). Each line is then
  split into units:
  - `'word'`: runs separated by whitespace; punctuation stays with its
    word. Text without spaces (Japanese) is one unit per line; use
    `'phrase'` for it.
  - `'phrase'`: `phrases()` (BudouX) on the line's text.
  - `'grapheme'`: `graphemes()`.
- Each unit is its own clipped, sliding `Text`, as each line is today,
  placed at its measured x inside the line. Whitespace between units is not
  drawn but keeps its width. `align` pivots the whole line, as today.
- `stagger` counts units in reading order across all lines:
  unit `n` starts at `from + n * stagger`. The default stays 4.
- Spans and color runs are sliced per unit the way they are sliced per line
  now.
- A unit other than `'line'` measures every line during render, even
  without `maxWidth`, so it needs the editor, the exporter, or the browser
  preview runtime, like `maxWidth` does; `inspect.mjs` needs `--native`.
  The browser measurer reports no glyph positions, so there the x of a
  unit is the measured width of the line's text before it, as
  `<TypewriterText>` places its caret.
- Known limit, to document: units are shaped separately, so kerning,
  ligatures, and joining forms across a unit boundary are lost. Not for
  Arabic or other joined scripts with `'grapheme'`.

## Left to the builder

- The bezier solver (Newton with bisection fallback, or a table) and its
  tolerance, as long as the endpoints are exact.
- Exact parameterisation of `elastic` for `easeInOut`, as long as the
  defaults equal today's curves.
- Internal structure: whether named easings are looked up through a shared
  kebab-to-camel helper, how unit ranges are computed and memoised.
- Which existing example scenes gain the new props. A new feature that
  supersedes an example's content updates that example in the same pull
  request: `with-motion.tsx` or a camera example for `keyframes()`, and the
  typewriter and text-reveal examples for 3 and 4. Rendering changes attach
  before/after screenshots of the same scene; for 3 and 4 the "before" is
  the default (unchanged) output.
