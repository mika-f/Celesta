# Animation and Text Helpers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add easing factories, `keyframes()`, typing times for `useTypewriter`, and `<TextReveal unit>` to the React runtime packages.

**Architecture:** Four independent TypeScript-only additions, one per task, each on its own branch of one `gh stack` stack (bottom to top in task order). Every default reproduces today's output; no Rust or project-format change.

**Tech Stack:** TypeScript, React (custom reconciler), Vitest with `node:assert/strict`, pnpm + Turborepo, `gh stack`.

**Spec:** `docs/superpowers/specs/2026-11-16-animation-text-helpers-design.md`

## Global Constraints

- TypeScript only. `frameKeyframes()`, the Rust `Easing` enum, and JSON projects are not touched.
- Anti-goal: no existing composition draws differently. `useTypewriter` without `pauses`, `<TextReveal>` without `unit`, and every existing `Easings.*` curve keep their output exactly.
- Tests: Vitest, TypeScript (`.test.mts`), `import assert from 'node:assert/strict'`, importing `../src`. During red/green run only the new test file; run the package's full `pnpm --filter <pkg> test` once at the end of the task. Never chain a commit and a test run in one command.
- Run `pnpm install && pnpm run build:runtime` once before the first test run (packages resolve each other's `dist`).
- Each task updates, in the same branch: the skill reference(s) under `skills/celesta/references/` (and `skills/celesta/SKILL.md`'s package table when a new name is exported), the user guide in both `packages/website/content/en/` and `packages/website/content/ja/` (the pages that already document the neighbouring API), and a new note `docs/handoff/2026-11-16-<task>.md` (specification, implementation details, checks run).
- Committed text is public: no scratch paths and no cleanup narration.
- Never amend.
- Branches: Task 1 `claude/easing-factories` (exists, checked out), Task 2 `claude/keyframes-helper`, Task 3 `claude/typewriter-frames`, Task 4 `claude/text-reveal-units`. Start each later task with `gh stack add <branch>` from the previous task's branch. Do not push or open PRs inside a task.

## Review Focus

1. A fractional frame just below a cut in `keyframes()` (a `<MotionBlur>` sample such as 119.75 with two keys on 120) still reads the first key's side. Test in Task 2.
2. `Easings.cubicBezier` with control points that overshoot (`y` outside 0–1) or are extreme (`0, 0, 1, 1`-like steep curves, `x1 = 0`, `x2 = 1`) returns finite values and exact endpoints. Test in Task 1.
3. `useTypewriter` text containing emoji / surrogate pairs and `<Span>`s: `frames.length` equals the code-point count, and a pause on the last character delays nothing visible but keeps `done` consistent with `length`. Test in Task 3.
4. `<TextReveal unit>` with leading, trailing, and repeated spaces and with an empty line (`'a\n\nb'`): whitespace makes no unit, an empty line makes no unit and consumes no stagger step. Test in Task 4.
5. `<TextReveal unit="word">` at rest (after the last unit finishes) with `align` 0, 0.5, and 1, and with `direction="out"` on its first frame: every unit sits where the measured line puts it. Test in Task 4.

---

### Task 1: Easing factories

**Files:**
- Modify: `packages/react/src/animation.ts` (the `Easings` object, near line 230)
- Modify: `packages/react/test/easings.test.mts`, `packages/cli/test/motion.test.mts:73-77`
- Test: `packages/react/test/easing-factories.test.mts` (new)
- Docs: `skills/celesta/references/animation.md` (`## Easings`), `skills/celesta/references/helpers.md` (one table row), `packages/website/content/{en,ja}/docs/02-(guides)/01-animation.md`, `packages/website/content/{en,ja}/docs/03-(reference)/01-reference.md` if it lists `Easings`, `docs/handoff/2026-11-16-easing-factories.md`

**Interfaces:**
- Produces, on the `Easings` object:
  - `cubicBezier(x1: number, y1: number, x2: number, y2: number): (t: number) => number`
  - `steps(count: number): (t: number) => number`
  - `back(overshoot?: number): EasingFamily`
  - `elastic(options?: { amplitude?: number; period?: number }): EasingFamily`
  - exported type `EasingFamily = { easeIn: (t: number) => number; easeOut: (t: number) => number; easeInOut: (t: number) => number }`
  - exported const `EASING_FACTORY_NAMES = ['cubicBezier', 'steps', 'back', 'elastic'] as const` from `animation.ts` (tests use it to skip factories when iterating `Easings`; not re-exported from the package index).

- [ ] **Step 1: Write the failing tests** in `easing-factories.test.mts`:

```ts
const sweep = Array.from({ length: 101 }, (_, i) => i / 100);

test('back() and elastic() defaults are the named curves, bit for bit', () => {
  for (const t of sweep) {
    assert.equal(Easings.back().easeIn(t), Easings.easeInBack(t));
    assert.equal(Easings.back().easeOut(t), Easings.easeOutBack(t));
    assert.equal(Easings.back().easeInOut(t), Easings.easeInOutBack(t));
    assert.equal(Easings.elastic().easeIn(t), Easings.easeInElastic(t));
    assert.equal(Easings.elastic().easeOut(t), Easings.easeOutElastic(t));
    assert.equal(Easings.elastic().easeInOut(t), Easings.easeInOutElastic(t));
  }
});

test('back(overshoot) overshoots by more with a larger value, and 0 does not overshoot', () => {
  const peak = (f: (t: number) => number) => Math.max(...sweep.map(f));
  assert.ok(peak(Easings.back(3).easeOut) > peak(Easings.back(1.4).easeOut));
  assert.ok(peak(Easings.back(0).easeOut) <= 1);
  assert.throws(() => Easings.back(-1), /overshoot/);
  assert.throws(() => Easings.back(NaN), /overshoot/);
});

test('cubicBezier follows CSS cubic-bezier()', () => {
  const ease = Easings.cubicBezier(0.25, 0.1, 0.25, 1);       // CSS `ease`
  assert.ok(Math.abs(ease(0.5) - 0.8024033877) < 1e-6);
  const linear = Easings.cubicBezier(0.3, 0.3, 0.7, 0.7);
  for (const t of sweep) assert.ok(Math.abs(linear(t) - t) < 1e-6);
  assert.throws(() => Easings.cubicBezier(-0.1, 0, 1, 1), /x1/);
  assert.throws(() => Easings.cubicBezier(0, 0, 1.1, 1), /x2/);
  assert.throws(() => Easings.cubicBezier(0, NaN, 1, 1), /finite/);
});

test('cubicBezier stays finite and exact at the ends for extreme control points', () => {
  for (const args of [[0, 0, 1, 1], [0, 1.8, 1, -0.8], [1, 0, 0, 1], [0.5, -2, 0.5, 3]] as const) {
    const f = Easings.cubicBezier(...args);
    assert.ok(f(0) === 0 && f(1) === 1);
    for (const t of sweep) assert.ok(Number.isFinite(f(t)));
  }
});

test('steps(count) holds each step and ends on 1', () => {
  const f = Easings.steps(4);
  assert.deepEqual([0, 0.24, 0.25, 0.5, 0.99, 1].map(f), [0, 0, 0.25, 0.5, 0.75, 1]);
  for (const bad of [0, -1, 1.5, NaN]) assert.throws(() => Easings.steps(bad), /positive integer/);
});

test('every factory curve starts at exactly 0 and ends at exactly 1', () => {
  const curves = [Easings.steps(3), Easings.cubicBezier(0.4, 0, 0.2, 1),
    ...Object.values(Easings.back(2.5)), ...Object.values(Easings.elastic({ amplitude: 1.5, period: 0.5 }))];
  for (const f of curves) assert.ok(f(0) === 0 && f(1) === 1);
});

test('elastic validates its options', () => {
  assert.throws(() => Easings.elastic({ amplitude: 0.5 }), /amplitude/);   // below 1
  assert.throws(() => Easings.elastic({ period: 0 }), /period/);
});
```

- [ ] **Step 2: Run** `pnpm --filter @celesta.studio/react exec vitest run test/easing-factories.test.mts` — expect FAIL (`Easings.back is not a function`).
- [ ] **Step 3: Implement** the four factories and `EASING_FACTORY_NAMES` in `animation.ts`. `amplitude` must be finite and at least 1 (default 1); `period` finite and above 0 (default: the value that reproduces today's curve — `easeIn`/`easeOut` use 0.3 and `easeInOut` 0.45 today, so an omitted `period` keeps that split and a given `period` applies `period` to in/out and `period * 1.5` to in-out). The existing six named curves must keep their exact arithmetic; if you route them through the factories, the first test proves it, otherwise leave them as they are and make the factory defaults call them.
- [ ] **Step 4: Update the two iterating tests** to skip `EASING_FACTORY_NAMES` (`easings.test.mts`: remove them from `names`; `motion.test.mts:74`: `continue` on them). Import the constant from `../src/animation` (react) and from the path `motion.test.mts` already uses for internals, or inline the four names there if it only imports the public package.
- [ ] **Step 5: Run** the new file again — PASS. Then `pnpm --filter @celesta.studio/react test` and `pnpm --filter @celesta.studio/cli exec vitest run test/motion.test.mts` — PASS.
- [ ] **Step 6: Docs.** Add the factories to `animation.md` `## Easings` (signature table + one example using `progress(frame, 0, 20, Easings.back(2.2).easeOut)` and `Easings.cubicBezier(0.4, 0, 0.2, 1)`), one row in `helpers.md` ("an easeOutBack copy with a different constant / a bezier solved by hand" → the factories), the website animation guide in en and ja, and the handoff note. State that the factories are functions for React only: JSON keyframes and `frameKeyframes()` take names.
- [ ] **Step 7: Commit** in two commits: `feat(react): add Easings.cubicBezier, steps, back, and elastic` (source + tests), then `docs: document the easing factories`.

### Task 2: `keyframes()`

**Files:**
- Modify: `packages/react/src/keyframes.ts`, `packages/react/src/index.ts:129`, `packages/react/src/browser.ts:18`
- Test: `packages/react/test/keyframes-value.test.mts` (new; `keyframes.test.mts` already covers `frameKeyframes`)
- Example: `packages/cli/examples/with-keyframes.tsx` (new: a `<Camera>` moving through four keys with a hold and a cut over a simple world of shapes)
- Docs: `skills/celesta/references/animation.md` (new `## keyframes` section after `## progress`, a row in "Pick a helper", and a sentence in `## Camera` pointing to it), `helpers.md` (row: "segments chained with several `interpolate` calls or `useCue` + `progress`" → `keyframes()`), `skills/celesta/SKILL.md` package table, website `02-(guides)/01-animation.md` en + ja (and `03-text-camera-lines.md` where it animates the camera), `docs/handoff/2026-11-16-keyframes-helper.md`

**Interfaces:**
- Consumes: nothing from Task 1 beyond `Easings` existing.
- Produces:

```ts
export interface ValueKeyframe<T extends number | Record<string, number>> {
  frame: number;
  value: T;
  /** Curve of the segment that ends at this key: a function or an `Easing` name. Default linear. */
  easing?: Easing | ((t: number) => number);
}
export function keyframes<T extends number | Record<string, number>>(
  frame: number, keys: readonly ValueKeyframe<T>[],
): T;
```

- [ ] **Step 1: Write the failing tests:**

```ts
const cam = [
  { frame: 0, value: { x: 0, zoom: 1 } },
  { frame: 100, value: { x: 200, zoom: 2 }, easing: Easings.easeInQuad },
  { frame: 120, value: { x: 200, zoom: 2 } },
  { frame: 120, value: { x: 900, zoom: 1 } },
  { frame: 150, value: { x: 1000, zoom: 1 }, easing: 'ease-out-cubic' as const },
];

test('interpolates each field with the easing of the key a segment ends on', () => {
  assert.deepEqual(keyframes(50, cam), { x: 200 * 0.25, zoom: 1.25 });
  assert.equal(keyframes(135, cam).x, 900 + 100 * Easings.easeOutCubic(0.5));
  assert.equal(keyframes(5, [{ frame: 0, value: 0 }, { frame: 10, value: 4 }]), 2);
});

test('holds before the first key, after the last, and between equal keys', () => {
  assert.deepEqual(keyframes(-10, cam), { x: 0, zoom: 1 });
  assert.deepEqual(keyframes(999, cam), { x: 1000, zoom: 1 });
  assert.deepEqual(keyframes(110, cam), { x: 200, zoom: 2 });
  assert.equal(keyframes(7, [{ frame: 3, value: 5 }]), 5);
});

test('two keys on one frame cut: the second value is on screen on that frame', () => {
  assert.deepEqual(keyframes(119, cam), { x: 200, zoom: 2 });
  assert.deepEqual(keyframes(119.75, cam), { x: 200, zoom: 2 });
  assert.deepEqual(keyframes(120, cam), { x: 900, zoom: 1 });
  // A cut on the first frame: the second key wins from that frame on, the first before it.
  const lead = [{ frame: 0, value: 1 }, { frame: 0, value: 2 }, { frame: 10, value: 3 }];
  assert.equal(keyframes(-1, lead), 1);
  assert.equal(keyframes(0, lead), 2);
});

test('an overshooting easing overshoots, and returns a new object each call', () => {
  const keys = [{ frame: 0, value: { y: 0 } }, { frame: 10, value: { y: 100 }, easing: Easings.easeOutBack }];
  assert.ok(keyframes(7, keys).y > 100);
  assert.notEqual(keyframes(10, keys), keys[1].value);
});

test('rejects malformed keys with the key index', () => {
  assert.throws(() => keyframes(0, []), /at least one key/);
  assert.throws(() => keyframes(NaN, [{ frame: 0, value: 0 }]), /finite frame/);
  assert.throws(() => keyframes(0, [{ frame: NaN, value: 0 }]), /finite frame \(key 0\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: Infinity }]), /finite value \(key 0\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: { x: NaN } }]), /finite value \(key 0\)/);
  assert.throws(() => keyframes(0, [{ frame: 5, value: 0 }, { frame: 4, value: 1 }]), /in order of frame \(key 1\)/);
  assert.throws(() => keyframes(0, [0, 0, 0].map(() => ({ frame: 1, value: 0 }))), /at most two keys on one frame \(key 2\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: 0 }, { frame: 1, value: { x: 1 } }] as never), /same shape \(key 1\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: { x: 0 } }, { frame: 1, value: { y: 1 } }] as never), /same shape \(key 1\)/);
  assert.throws(() => keyframes(0, [{ frame: 0, value: 0 }, { frame: 1, value: 1, easing: 'ease-sideways' as never }]), /unknown easing "ease-sideways" \(key 1\)/);
});

test('every Easing name resolves to the Easings curve of the same name', () => {
  // Names come from the fixture easings.test.mts reads (crates/composition/tests/fixtures/keyframe-evaluation.json).
  for (const name of easingNamesFromFixture) {
    const keys = [{ frame: 0, value: 0 }, { frame: 10, value: 1, easing: name }];
    assert.equal(keyframes(3, keys), Easings[camelCase(name)](0.3));
  }
});
```

- [ ] **Step 2: Run** `pnpm --filter @celesta.studio/react exec vitest run test/keyframes-value.test.mts` — FAIL (`keyframes` is not exported).
- [ ] **Step 3: Implement** `keyframes()` in `keyframes.ts` with messages prefixed `keyframes() requires …` / `keyframes() allows …` in the wording of `frameKeyframes()`. Segment choice: the segment for `frame` is the last pair `(a, b)` with `a.frame <= frame`, so on a shared frame the second key is the segment start; before the first key return the first value (for a leading cut, the first of the two). Names map to `Easings` by kebab → camel case. Export `keyframes` and `ValueKeyframe` from `index.ts` and `browser.ts` beside `frameKeyframes`.
- [ ] **Step 4: Run** the new file — PASS; then `pnpm --filter @celesta.studio/react test` — PASS.
- [ ] **Step 5: Example and docs.** Write `with-keyframes.tsx`; check it with `node skills/celesta/scripts/inspect.mjs packages/cli/examples/with-keyframes.tsx --frames 0,50,119,120,-1` (the camera's `x`/`zoom` on 119 and 120 show the cut) and render frames 119 and 120 to PNG with the exporter if one is built, looking at them. Write the docs listed above; the `animation.md` section states the cut rule, the hold rule, that `easing` belongs to the key a segment ends on, and that `frameKeyframes()` is the one for `<Audio volume>`.
- [ ] **Step 6: Commit** `feat(react): add keyframes() for keyed values with per-key easing`, then `docs: document keyframes() and add an example`.

### Task 3: Typing times

**Files:**
- Modify: `packages/text/src/text-motion.ts:168-228`, `packages/text/src/index.ts`, `packages/components/src/typewriter-text.ts` (props near line 42, call at line 144)
- Test: `packages/text/test/typewriter.test.mts` (new), `packages/components/test/typewriter-text.test.mts` (one added test)
- Example: `packages/cli/examples/with-typewriter-text.tsx` (add a line with `pauses`)
- Docs: `skills/celesta/references/text.md` (`useTypewriter` and `<TypewriterText>` sections), `helpers.md` (row: "`from + i * framesPerChar` computed by hand to place a sound per character" → `typewriterFrames()`), `SKILL.md` package table (`typewriterFrames`), website `02-(guides)/05-text-fonts.md` or `03-text-camera-lines.md` (whichever documents `useTypewriter`) en + ja, `docs/handoff/2026-11-16-typewriter-frames.md`

**Interfaces:**
- Produces:

```ts
export interface TypewriterOptions { from?: number; framesPerChar?: number; blinkFrames?: number;
  /** Extra frames to wait after a character (one code point per name) before the next. */
  pauses?: Readonly<Record<string, number>>; }
export interface Typewriter { text: string; length: number; done: boolean; caretVisible: boolean;
  /** The first whole frame each character is shown on, one entry per code point, on the hook's clock. */
  frames: number[]; }
export function typewriterFrames(text: string,
  options?: Pick<TypewriterOptions, 'from' | 'framesPerChar' | 'pauses'>): number[];
```
- `<TypewriterText pauses>`: same type, passed straight to the hook.

- [ ] **Step 1: Write the failing tests** (render the hook through the helpers `text-reveal.test.mts` already uses to evaluate a component at a frame):

```ts
test('typewriterFrames without pauses is from + ceil(i * framesPerChar) on whole frames', () => {
  assert.deepEqual(typewriterFrames('abcd', { from: 10, framesPerChar: 2 }), [10, 12, 14, 16]);
  assert.deepEqual(typewriterFrames('abcd', { framesPerChar: 0.5 }), [0, 1, 1, 2]);
  assert.deepEqual(typewriterFrames(''), []);
});

test('a pause delays every character after it', () => {
  assert.deepEqual(
    typewriterFrames('はい、できました。', { from: 10, framesPerChar: 2, pauses: { '、': 6, '。': 12 } }),
    [10, 12, 14, 22, 24, 26, 28, 30, 32],
  );
});

test('frames[i] is the first whole frame on which length > i, for awkward speeds', () => {
  for (const framesPerChar of [0.3, 0.7, 1, 1.1, 1.5, 2.2, 3, 7.3]) {
    const text = 'x'.repeat(200);
    const frames = typewriterFrames(text, { from: 3, framesPerChar });
    for (const i of [0, 1, 29, 30, 169, 170, 199]) {
      assert.ok(lengthAt(text, frames[i], { from: 3, framesPerChar }) > i);
      assert.ok(lengthAt(text, frames[i] - 1, { from: 3, framesPerChar }) <= i);
    }
  }
});   // lengthAt renders useTypewriter at a frame and returns .length

test('without pauses, length is unchanged on whole and fractional frames', () => {
  for (const frame of [0, 1, 1.5, 2.25, 33, 34, 187, 188])
    for (const framesPerChar of [1, 1.1, 1.5])
      assert.equal(lengthAt('x'.repeat(200), frame, { framesPerChar }),
        Math.min(200, Math.max(0, Math.floor(frame / framesPerChar) + 1)));
});

test('the hook reports the same frames, counts code points, and keeps the caret steady in a pause', () => {
  const typed = typewriterAt(<>a😀<Span color="#ff0000">b。</Span>c</>, 9, { framesPerChar: 2, pauses: { '。': 10 } });
  assert.deepEqual(typed.frames, [0, 2, 4, 6, 18]);
  assert.equal(typed.length, 4);            // frame 9: inside the pause after 。
  assert.equal(typed.caretVisible, true);   // still typing
  assert.equal(typed.done, false);
  const end = typewriterAt('ab。', 4, { framesPerChar: 2, pauses: { '。': 30 } });
  assert.equal(end.done, true);             // a pause after the last character delays nothing
});

test('pauses are validated', () => {
  assert.throws(() => typewriterFrames('a', { pauses: { ab: 3 } }), /one character/);
  assert.throws(() => typewriterFrames('a', { pauses: { a: -1 } }), /pauses/);
  assert.throws(() => typewriterFrames('a', { pauses: { a: NaN } }), /pauses/);
  assert.throws(() => typewriterFrames('a', { framesPerChar: 0 }), /positive framesPerChar/);
});
```

- [ ] **Step 2: Run** `pnpm --filter @celesta.studio/text exec vitest run test/typewriter.test.mts` — FAIL.
- [ ] **Step 3: Implement.** One shared predicate decides everything: character `i` is shown on `frame` when `Math.floor((frame - from - P(i)) / framesPerChar) >= i`, `P(i)` the sum of pauses of characters before `i`. `length` is the count of shown characters (they are shown in order, so stop at the first hidden one); without `pauses` this must equal today's expression on every frame. `frames[i]` starts from `Math.ceil(from + P(i) + i * framesPerChar)` and is corrected by ±1 with the predicate so the invariant in the third test holds. `typing` stays `frame >= from && length < characters.length`. Add the `<TypewriterText pauses>` prop and its test: a `pauses` value reaches the hook (the visible length at a frame inside a pause is the paused one).
- [ ] **Step 4: Run** the new file — PASS; then `pnpm --filter @celesta.studio/text test` and `pnpm --filter @celesta.studio/components test` — PASS.
- [ ] **Step 5: Example and docs.** Add `pauses` to one line of `with-typewriter-text.tsx` and inspect it at a frame inside the pause. Document the option, `frames`, `typewriterFrames()` with the key-click `<Sequence from={at}><Audio …/></Sequence>` example, and that pause names are single characters counted by code point.
- [ ] **Step 6: Commit** `feat(text): report typing frames and pause after chosen characters`, then `docs: document typing frames and pauses`.

### Task 4: Reveal units

**Files:**
- Modify: `packages/text/src/text-motion.ts:22-166` (`TextRevealProps`, `TextReveal`, a new helper beside `useLineRanges`)
- Test: `packages/text/test/text-reveal.test.mts` (add cases; reuse its measurer stub, extending it with glyph positions if it has none)
- Example: `packages/cli/examples/` — the example that already shows `<TextReveal>` (find it with `grep -l TextReveal packages/cli/examples`); add a `unit="word"` and a `unit="grapheme"` block
- Docs: `skills/celesta/references/text.md` (`TextReveal` section), `verify.md` only if its list of APIs needing `--native` is explicit, website `02-(guides)/03-text-camera-lines.md` or `05-text-fonts.md` (where `TextReveal` is) en + ja, `docs/handoff/2026-11-16-text-reveal-units.md`

**Interfaces:**
- Produces: `TextRevealProps.unit?: 'line' | 'word' | 'phrase' | 'grapheme'` (default `'line'`).
- Consumes: `phrases()` from `./phrases`, `graphemes()` from `./emphasis`, the measurer path `useLineRanges` uses, and `textCaret` from `@celesta.studio/react` for a unit's x.

- [ ] **Step 1: Write the failing tests.** With a stub measurer of fixed advance `A` per code point (so x of offset `n` in a line is `n * A`) and the test file's existing way of reading the rendered layer tree:

```ts
test("unit='line' and no unit render the same tree as before", …);   // deepEqual of the two trees, and against a snapshot taken before the change
test("unit='word' makes one clipped Text per word at its measured x", () => {
  // 'ab cd\nef' → units 'ab' x=0, 'cd' x=3A on line 0; 'ef' x=0 on line 1 (y = lineHeight)
});
test('stagger counts units in reading order across lines', () => {
  // from=0, stagger=4, durationInFrames=20: at frame 4 unit 0 is partly shown, unit 1 just starts, unit 2 (line 1) is hidden
});
test("unit='grapheme' splits by grapheme cluster, not code point", () => {
  // '👨‍👩‍👧a' → 2 units
});
test("unit='phrase' uses phrases()", () => {
  // units' texts deepEqual phrases(lineText) for a Japanese line
});
test('whitespace and empty lines make no unit and use no stagger step', () => {
  // '  ab   cd  \n\nef' with unit='word' → 3 units ('ab', 'cd', 'ef'), start frames 0, 4, 8; 'cd' x = 7A
});
test('align pivots the whole line', () => {
  // align 0.5: unit x = measuredX - lineWidth / 2; align 1: measuredX - lineWidth; at rest positions equal the line's
});
test("direction='out' starts at rest and hides units in order", …);
test('spans keep their style inside the unit they fall in, and a span crossing units is sliced', …);
```

Write each with concrete numbers from the stub (no placeholders in the committed test).

- [ ] **Step 2: Run** `pnpm --filter @celesta.studio/text exec vitest run test/text-reveal.test.mts` — the new cases FAIL, the existing ones PASS.
- [ ] **Step 3: Implement.** Keep the `'line'` path byte-for-byte as it is (branch early). For other units: per line, compute unit code-point ranges (`'word'`: maximal runs of non-whitespace, `/\s/u`; `'phrase'`: `phrases(lineText)` laid end to end, trimmed of surrounding whitespace, empty ones dropped; `'grapheme'`: `graphemes(lineText)` minus whitespace-only clusters); measure the line once as a single-line text with the line's style and runs; a unit's x is the caret x at its start offset (`textCaret`) where glyph positions exist, else the measured width of the line's text before it (the fallback `<TypewriterText>`'s caret uses — follow `packages/components/src/typewriter-text.ts`). Draw each unit as today's line is drawn (`Group clip` + sliding `Text` with `anchorY: 'baseline'`), left-anchored at `x - align * lineWidth`, with runs from `sliceTextRuns(flat, start, end)`. The clip is the unit's own box widened by a margin of one `fontSize` on each side so italics and overshoot are not cut horizontally. Unit index `n` runs across lines; progress is `progress(frame, from + n * stagger, durationInFrames, easing)`. Memoise ranges and measurements on the same key `useLineRanges` uses plus `unit`.
- [ ] **Step 4: Run** the file — PASS; then `pnpm --filter @celesta.studio/text test` and `pnpm run test:runtime` — PASS.
- [ ] **Step 5: Example, screenshots, docs.** Update the example; render the same frame of the unchanged `'line'` block before (on the Task 3 branch) and after to show it did not move, and a mid-reveal frame of the `word` and `grapheme` blocks; keep the PNGs outside the repository for the PR. Document `unit` in `text.md` with: the default, what each unit splits on, that Japanese wants `'phrase'` or `'grapheme'` (a line without spaces is one `'word'`), stagger across lines, the measurement requirement (`inspect.mjs --native`, browser preview approximation), and the limit that units are shaped separately (kerning, ligatures, joined scripts).
- [ ] **Step 6: Commit** `feat(text): reveal TextReveal by word, phrase, or grapheme`, then `docs: document TextReveal units`.

---

## After the last task

- `gh stack view --json`: four branches, none needing a rebase. Run `pnpm run test:runtime` on the top branch only.
- `gh stack submit --auto --remote origin`, then `gh pr edit` each PR: a body a reviewer can reproduce from (what the API is, the test command, the example to open), screenshots attached with `gh pr edit --attach` for Tasks 2 and 4.
