# Typing frames and pauses (2026-11-16)

Design: [`docs/superpowers/specs/2026-11-16-animation-text-helpers-design.md`](../superpowers/specs/2026-11-16-animation-text-helpers-design.md), part 3.

## Specification

- `useTypewriter(content, { pauses })`: `pauses` maps a character (one code
  point) to extra frames to wait after it before the next character. Values
  are finite and at least 0. The caret stays steady during a pause.
- `Typewriter.frames`: one entry per code point of the whole text, the
  first whole frame the character is shown on, on the hook's clock.
- `typewriterFrames(text, { from?, framesPerChar?, pauses? })` from
  `@celesta.studio/text`: the same list for a plain string, without a hook.
- `<TypewriterText pauses>` passes the option to the hook.
- Without `pauses`, `length`, `text`, `done`, and `caretVisible` are what
  they were on every frame, fractional frames inside `<MotionBlur>`
  included.
- A `NaN` `from` now throws (it used to give a `NaN` length). An infinite
  `from` still works as before: `Infinity` types nothing and `-Infinity`
  shows the whole text, and `frames` is then all `Infinity` or `-Infinity`.
- `frames` beyond 2^53, where a frame and the next are the same number, are
  the unrounded estimate rather than a corrected frame.

## Implementation

`packages/text/src/text-motion.ts`. One predicate decides everything:
character `i` is shown on `frame` when
`floor((frame - from - P(i)) / framesPerChar) >= i`, where `P(i)` is the sum
of the pauses of the characters before `i` (`pausesBefore`). The typed
count is the number of leading characters the predicate accepts
(`countTyped`); with `P = 0` that is today's
`floor((frame - from) / framesPerChar) + 1`, clamped. `frames[i]` starts
from `ceil(from + P(i) + i * framesPerChar)` and is moved by whole frames
until the predicate flips exactly there (`typingFrames`), so rounding in
either expression (for example `framesPerChar: 1.1`) cannot make `frames`
and `length` disagree.

`typedCount` is exported from the module for tests, not from the package.

## Checks

- `pnpm --filter @celesta.studio/text exec vitest run test/typewriter.test.mts`:
  frames with and without pauses, the invariant "`frames[i]` is the first
  whole frame on which `length > i`" over a sweep of speeds, the unchanged
  count without pauses on whole and fractional frames, code points and
  spans, the caret during a pause, a pause on the last character, and
  validation.
- `pnpm --filter @celesta.studio/text test`,
  `pnpm --filter @celesta.studio/components test` (a `pauses` case for
  `<TypewriterText>`).
- `packages/cli/examples/with-typewriter-text.tsx`: the Japanese block
  types with `pauses={{ '、': 6, '。': 12 }}`.
