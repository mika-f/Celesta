# Intro Video Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `examples/intro/`, a 60-second Celesta composition that introduces Celesta on YouTube by revealing that the video is its own source code.

**Architecture:** A React composition in the shape of `examples/reel`: `film.tsx` maps a scene list to `<Sequence>`s, one file per scene under `scenes/`, shared drawing parts under `components/`. Every value that is both drawn and shown as code lives in a pure module under `shared/` (no Celesta imports), so `node --test` can prove the snippet and the picture agree.

**Tech Stack:** Celesta React runtime (`@celesta/react`, `@celesta/shapes`, `@celesta/code`, `@celesta/text`, `@celesta/math`), TypeScript, Node 26 built-in test runner (native type stripping), Python 3 standard library for the score.

**Spec:** `docs/superpowers/specs/2026-10-14-intro-video-design.md`

## Global Constraints

- 1920×1080, 30 fps, `DURATION = 1800` frames; `BEAT = 15`, `BAR = 60`; every scene starts on a bar line.
- Scene starts (frames): ColdOpen 0, Reveal 180, Problem 300, Idea 540, Components 780, LiveEdit 1020, Export 1320, Outro 1560. End-screen hold from `HOLD = 1650` (55 s) to the last frame: nothing moves.
- Palette, exactly: `bg #000000`, `fg #F5F5F7`, `accent #3D7BFF`, `muted #6B6B70`, `line #1C1C1E`, `panel #0D0D0F`. No other colors except alpha variants of these.
- No gradients. Every `Rect` square-cornered. Hairlines 1 px.
- Only the `CELESTA` wordmark is uppercase; no wide-tracked uppercase labels.
- Never draw `→ ◆ ✓ ⌘` or similar symbols as text; draw them with `Path`/`Arrow`.
- Fonts: Space Grotesk 700 for headlines, JetBrains Mono 400/700 for code and UI, both from one Google Fonts CSS URL via `<Font>`.
- On-screen copy is the spec's storyboard text, verbatim.
- Frames are pure functions of the frame number: no `Math.random`, `Date`, state, or effects; seeded `random`/`noise` from `@celesta/math` only.
- Text whose width decides layout (headlines next to panels, the terminal line) is measured with `useTextMetrics` or `fitText`, not tuned by hand.
- Honesty rule: a snippet shown on screen takes every value it displays from the same constant or function the picture uses.
- Files under `shared/` and `constants.ts` import nothing except each other, always with explicit `.ts` extensions, so `node --test` runs them directly. (The staged tsconfig sets `allowImportingTsExtensions`; esbuild accepts them.) Other files import without extensions, like the other examples.

## Review Focus

1. **Missing glyphs.** `·`, `×`, `—`, and quotes in copy may be absent from a font and render as tofu or a fallback. Every PNG/MP4 export's `--json` `warnings` must contain no missing-glyph or font warning (checked in Task 11, and in each scene task's render step).
2. **Scene boundaries off by one.** The frame before and the frame at each cut must belong to the outgoing and incoming scene. Task 1 tests the boundary table; Task 11 renders frames `S.x - 1` and `S.x` for every cut.
3. **Out-of-order rendering.** Exporter and scrubbing evaluate frames in any order; a scene that keeps state would differ. Task 11 inspects frames `900,10,900` and requires both 900s to be identical.
4. **Snippet drift.** Someone edits a drawing constant but not the displayed code. Tasks 3, 7, and 8 pin each snippet to its source with a unit test.
5. **End-screen hold actually still.** A counter, caret blink, or noise term left running after frame 1650 would distract under YouTube's end-screen cards. Task 10 requires the Scene JSON of frames 1650 and 1799 to be identical.

---

## Shared commands

All commands run from the worktree root. Use absolute paths in practice; they are shortened here.

```bash
export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"
EXPORTER=target/release/celesta-exporter
INSPECT=skills/celesta/scripts/inspect.mjs
TSC=$(ls -d node_modules/.pnpm/typescript@*/node_modules/typescript/bin/tsc | head -1)
OUT=<session scratchpad>/intro        # PNGs and MP4s; never inside the repo
```

- **Unit:** `node --test examples/intro/test/*.test.ts`
- **Types:** `node $TSC --noEmit -p examples/intro`
- **Inspect:** `node $INSPECT examples/intro/film.tsx --frames <list>` → ends with no `problem(s) found` line and no `MISSING FILE`.
- **Render:** `$EXPORTER --json --react examples/intro/film.tsx --frames <list> $OUT/<name>.png` → JSON has no `code`; `warnings` is empty; then open every PNG listed in `outputs` and compare it with the step's expected picture.

---

### Task 1: Scaffold, timebase, and frame counter

**Files:**
- Create: `examples/intro/package.json`, `examples/intro/tsconfig.json`, `examples/intro/.gitignore`
- Create: `examples/intro/constants.ts`, `examples/intro/shared/cues.ts`, `examples/intro/timeline.ts`, `examples/intro/film.tsx`
- Create: `examples/intro/components/FrameCounter.tsx`, `examples/intro/components/Placeholder.tsx`
- Test: `examples/intro/test/cues.test.ts`

**Interfaces:**
- Produces, `constants.ts`: `W = 1920`, `H = 1080`, `FPS = 30`, `BEAT = 15`, `BAR = 60`, `DURATION = 1800`, `C = { bg, fg, accent, muted, line, panel }` (values from Global Constraints), `FONT = { display: 'Space Grotesk', mono: 'JetBrains Mono' }`, `FONT_SRC = 'https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@700&family=JetBrains+Mono:wght@400;700'`.
- Produces, `shared/cues.ts`: `S = { coldOpen: 0, reveal: 180, problem: 300, idea: 540, components: 780, liveEdit: 1020, export: 1320, outro: 1560 } as const`, `HOLD = 1650`, `SCENE_ORDER: (keyof typeof S)[]` in that order, `sceneAt(frame: number): keyof typeof S`.
- Produces, `timeline.ts`: `SCENES: { from: number; to: number; Scene: () => ReactNode }[]` built from `SCENE_ORDER`; `to` is the next start or `DURATION`. Each later task swaps its `Placeholder` entry for the real scene.
- Produces, `components/FrameCounter.tsx`: `FrameCounter()` — `frame 0000` (four digits, zero-padded, composition frame), JetBrains Mono 20 px, `C.muted`, right-aligned at 56 px from the right and bottom edges. `film.tsx` mounts it for frames `0..HOLD-1`.
- `components/Placeholder.tsx`: `placeholder(name: string): () => ReactNode`, which draws the scene name centered in `C.muted`. Removed in Task 10 when the last scene lands.

- [ ] **Step 1: Build the toolchain** (the worktree has no build output yet)

Run: `pnpm install && pnpm --dir packages/react run codegen && pnpm run build:runtime && cargo build -p celesta-exporter --release`
Expected: all succeed; `$EXPORTER` and `packages/cli/dist/project-types/` exist.

- [ ] **Step 2: Write the failing test** `test/cues.test.ts`

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import { BAR, DURATION } from '../constants.ts';
import { HOLD, S, SCENE_ORDER, sceneAt } from '../shared/cues.ts';

test('scenes start on bar lines in storyboard order', () => {
  assert.deepEqual(SCENE_ORDER.map((k) => S[k]), [0, 180, 300, 540, 780, 1020, 1320, 1560]);
  for (const k of SCENE_ORDER) assert.equal(S[k] % BAR, 0);
  assert.equal(DURATION, 1800);
  assert.equal(HOLD, 1650);
});

test('sceneAt switches exactly on the cut', () => {
  assert.equal(sceneAt(179), 'coldOpen');
  assert.equal(sceneAt(180), 'reveal');
  assert.equal(sceneAt(1559), 'export');
  assert.equal(sceneAt(1799), 'outro');
});
```

- [ ] **Step 3: Run it to see it fail**

Run: unit command. Expected: FAIL, cannot find `../constants.ts`.

- [ ] **Step 4: Implement** `constants.ts`, `shared/cues.ts`, then `timeline.ts`, `FrameCounter`, `Placeholder`, and `film.tsx` (Composition, `<Assets><Font src={FONT_SRC} /></Assets>`, a full-frame `C.bg` Rect, the `SCENES` Sequences, and `FrameCounter` in a `<Sequence from={0} durationInFrames={HOLD}>`). `package.json` is named `@celesta/example-intro` with the workspace deps listed in Tech Stack plus `react`/`@types/react` like `examples/versus/package.json`. `tsconfig.json` is `{ "extends": "./.celesta/tsconfig.json" }`; `.gitignore` holds `.celesta/`.

- [ ] **Step 5: Verify**

Run: unit command → PASS. `cp -R packages/cli/dist/project-types examples/intro/.celesta` then the types command → no errors. Inspect `--frames 0,179,180,1649,1650,-1` → each frame shows the right placeholder name; the counter text is present at 1649 and absent at 1650. Render `--frames 0,1649` → black frame, muted name centered, `frame 0000` / `frame 1649` bottom right.

- [ ] **Step 6: Commit**

```bash
git add examples/intro pnpm-lock.yaml
git commit -m "feat(examples): scaffold the intro video's timebase and frame counter"
```

### Task 2: Score

**Files:**
- Create: `examples/intro/make-score.py`
- Modify: `examples/intro/film.tsx` (add `<Audio src="./music.wav" />`)

**Interfaces:**
- Produces: `examples/intro/music.wav`, 44.1 kHz stereo 16-bit, exactly `60 * 44100` frames (gitignored by the root `*.wav` rule).

- [ ] **Step 1: Write the score script.** Start from a copy of `examples/feature-tour/make-score.py` (same 120 BPM building blocks), set `BARS = 30`, write `music.wav`, and re-arrange to the picture. Cue constants at the top mirror `shared/cues.ts` in seconds, with a "keep in sync" comment:
  - bars 0–2 Cold open: kick + bell per beat, sparse.
  - bar 3 Reveal (6 s): impact and the groove drops out under a held pad.
  - bars 5–8 Problem: dry ticks on every eighth, a stumbling hit when the diamonds misalign (bar 7, beat 3), a reverse swell into bar 9.
  - bars 9–12 Idea: half-time pulse; a soft pluck each time the scrub bar turns.
  - bars 13–21 Components and Live edit: full groove; a click on each ⌘S (frames from `shared/live-edit.ts` in Task 8, mirrored here).
  - bars 22–25 Export: groove plus a rising arpeggio while the counter climbs; bell chord at the check mark.
  - bars 26–29 Outro: impact at 52 s, pad and bell; from 55 s only a held pad fading to silence by 60 s.

- [ ] **Step 2: Verify length and determinism**

Run: `python3 examples/intro/make-score.py && shasum examples/intro/music.wav && python3 examples/intro/make-score.py && shasum examples/intro/music.wav && python3 -c "import wave;w=wave.open('examples/intro/music.wav');print(w.getnframes(),w.getframerate(),w.getnchannels())"`
Expected: identical hashes; `2646000 44100 2`.

- [ ] **Step 3: Verify the entry** — inspect `--frames 0` lists `audio ./music.wav` and no missing file.

- [ ] **Step 4: Commit** — `feat(examples): add the intro video's score`

### Task 3: Ball and Cold open (0–6 s)

**Files:**
- Create: `examples/intro/shared/ball.ts`, `examples/intro/components/BallStage.tsx`, `examples/intro/components/Headline.tsx`, `examples/intro/scenes/ColdOpen.tsx`
- Modify: `examples/intro/timeline.ts`
- Test: `examples/intro/test/ball.test.ts`

**Interfaces:**
- Produces, `shared/ball.ts`: `BALL = { size: 120, floor: 760, height: 420, period: 30 } as const`; `ballY(frame: number): number` (top of the square; one bounce per two beats, landing on every other beat); `BALL_SOURCE: string`, the JavaScript source of `ballY` as displayed in Reveal and Idea, with `BALL`'s numbers interpolated from the constant.
- Produces, `components/BallStage.tsx`: `BallStage({ frame }: { frame: number })` draws the 1920×1080 cold-open stage (the accent square at `ballY(frame)`, centered horizontally, plus a 1 px `C.line` floor) without background, so Reveal can scale it.
- Produces, `components/Headline.tsx`: `Headline({ x, y, children, size = 120, color = C.fg, anchorX = 0, opacity = 1 })`, Space Grotesk 700, letter-spacing as set by the font.

- [ ] **Step 1: Write the failing test** `test/ball.test.ts`

```ts
test('BALL_SOURCE computes exactly what ballY draws', () => {
  const shown = new Function('BALL', `${BALL_SOURCE}\nreturn ballY;`)(BALL);
  for (let f = 0; f < 1800; f += 1) assert.equal(shown(f), ballY(f));
});

test('the ball lands on the floor every period', () => {
  for (const f of [0, 30, 60, 900]) assert.equal(ballY(f), BALL.floor - BALL.size);
  assert.equal(ballY(15), BALL.floor - BALL.size - BALL.height);
});
```

- [ ] **Step 2: Run it** — FAIL, missing module.
- [ ] **Step 3: Implement `shared/ball.ts`.** A parabola per period: `t = (frame % period) / period`, `y = floor - size - height * 4 * t * (1 - t)`. `BALL_SOURCE` is a template literal holding that function's source (plain JS, reading `BALL.*`).
- [ ] **Step 4: Run it** — PASS.
- [ ] **Step 5: Implement `ColdOpen`.** Bar 1 (0–59): `BallStage` alone. Bars 2–3, one beat each from frame 60: `Titles.` (a title card: Headline sliding in on a 1 px rule), `Charts.` (five accent bars growing with `spring`), `Captions.` (a `C.panel` band with a caption line typed by `useTypewriter`), repeating once to fill bars 2–3, then on the last beat (165–179) the ball stage returns full frame so Reveal starts from it. Each word is a `Headline` at size 120, left at x=160, y=160. Register in `timeline.ts`.
- [ ] **Step 6: Verify** — types; inspect `--frames 0,15,60,75,90,170`; render the same frames. Expected: frame 15 ball at the top of its arc; 60/75/90 show `Titles.`, `Charts.`, `Captions.` each with its vignette; 170 ball stage only; nothing but the six palette colors.
- [ ] **Step 7: Commit** — `feat(examples): add the intro's cold open`

### Task 4: Reveal (6–10 s)

**Files:**
- Create: `examples/intro/components/CodePanel.tsx`, `examples/intro/scenes/Reveal.tsx`
- Modify: `examples/intro/timeline.ts`

**Interfaces:**
- Consumes: `BallStage`, `ballY`, `BALL_SOURCE`, `Headline`.
- Produces, `components/CodePanel.tsx`: `noirTheme` (from `codeThemes.dark`: `foreground: C.fg`, `highlightLine: C.accent + '33'`, keyword/number/boolean/constant → `C.accent`, string/template → `C.fg`, punctuation/comment/operator → `C.muted`); `CodePanel({ x, y, width, height, source, language = 'tsx', fontSize = 26, highlightLines, visibleCharacters, title })` drawing a `C.panel` Rect, a 1 px `C.line` border, an optional file-name tab (`title`, mono 18 px `C.muted`), and `<Code>` inset 32 px.

- [ ] **Step 1: Implement `CodePanel` and `Reveal`.** Reveal's source shown on the left is a short `film.tsx` excerpt: imports, then `BALL_SOURCE`, then a `<Rect y={ballY(frame)} … />` line. Beat 1 (180–194): the full-frame `BallStage` scales with `spring` to a 860×484 preview frame (1 px `C.line` border) at x=980, y=300, still playing `ballY` of the composition frame. From beat 2 the `CodePanel` (x=80, y=240, 860×600, title `film.tsx`) types in with `useTypewriter` and the line containing `ballY(frame)` is highlighted while the ball is in the air. `This video is code.` Headline at x=80, y=96, size 96, appears on bar 5's downbeat (240).
- [ ] **Step 2: Verify** — types; render `--frames 180,195,240,285`. Expected: 180 identical in layout to ColdOpen's last frame (no jump at the cut); 195 preview at final size; 240 headline visible; 285 code fully typed and the highlighted line matching the ball's motion. Keep frame 285's PNG; Task 11 uses it for the poster.
- [ ] **Step 3: Commit** — `feat(examples): add the intro's reveal`

### Task 5: Problem (10–18 s)

**Files:**
- Create: `examples/intro/components/Diamond.tsx`, `examples/intro/scenes/Problem.tsx`
- Modify: `examples/intro/timeline.ts`

**Interfaces:**
- Produces: `Diamond({ x, y, size = 16, color = C.fg })`, a filled `Path` rhombus centered on (x, y).

- [ ] **Step 1: Implement.** A mock timeline editor: `C.panel` surface 1600×520 at (160, 360), 8 track rows with 1 px `C.line` separators, a ruler with frame ticks, ~48 diamonds placed by seeded `random`. Bar 6–7: a `C.accent` selection box hops to one diamond per eighth note and nudges it; at bar 7 beat 3 (frames 450–464) every later diamond shifts out of its column. Bar 8: `Again.`. Bar 9 (480–539): every diamond travels with `spring` into a single code line `y = spring(frame)` (mono 64 px, centered at y=720), then the editor fades to `C.bg`. Copy: `Keyframes. By hand.` (300), `Again.` (420), `Or one line.` (480), Headline size 96 at x=160, y=160.
- [ ] **Step 2: Verify** — render `--frames 300,400,455,500,535`. Expected: diamonds on the grid, then misaligned at 455, converging at 500, a single line at 535; no `◆` text layers in inspect output.
- [ ] **Step 3: Commit** — `feat(examples): add the intro's keyframe problem scene`

### Task 6: Idea (18–26 s)

**Files:**
- Create: `examples/intro/scenes/Idea.tsx`, `examples/intro/shared/scrub.ts`
- Modify: `examples/intro/timeline.ts`
- Test: `examples/intro/test/scrub.test.ts`

**Interfaces:**
- Produces, `shared/scrub.ts`: `scrubFrame(local: number): number`, the frame the scrub bar points to at the scene's local frame `0..239`; sweeps 0→89→30→120→60 with eased turns, integer results, and the same value repeats at least three times across the scene.

- [ ] **Step 1: Write the failing test**

```ts
test('scrub revisits frames, so the ball must repeat', () => {
  const seen = new Map<number, number>();
  for (let l = 0; l < 240; l += 1) seen.set(scrubFrame(l), (seen.get(scrubFrame(l)) ?? 0) + 1);
  assert.ok([...seen.values()].some((n) => n >= 3));
  for (let l = 0; l < 240; l += 1) assert.ok(Number.isInteger(scrubFrame(l)));
});
```

- [ ] **Step 2: Run it** — FAIL. **Step 3:** implement with `interpolate`-style piecewise easing written inline (no Celesta imports). **Step 4:** PASS.
- [ ] **Step 5: Implement the scene.** Top: `frame` (mono 120 px) — an `Arrow` in `C.accent` — `picture` (Headline 120). Middle: `BallStage` scaled to 0.4 at its frame `scrubFrame(local)`, with a ghost of the same ball at 25% opacity marking that frame's position whenever it has been visited before (computed from `scrubFrame` over earlier local frames, still pure). Bottom: a 1200 px scrub bar with a `C.accent` playhead and the current `frame` number. Copy `A frame is a function of time.` at bar 12 (720).
- [ ] **Step 6: Verify** — unit; render `--frames 540,600,660,720,770`. Expected: playhead moves both ways across the PNGs; the ball sits on its ghost when the frame number repeats.
- [ ] **Step 7: Commit** — `feat(examples): add the intro's frame-as-function scene`

### Task 7: Components (26–34 s)

**Files:**
- Create: `examples/intro/shared/cards.ts`, `examples/intro/components/Card.tsx`, `examples/intro/scenes/Components.tsx`
- Modify: `examples/intro/timeline.ts`
- Test: `examples/intro/test/cards.test.ts`

**Interfaces:**
- Produces, `shared/cards.ts`: `CardDatum = { title: string; value: number }`; `cardsAt(local: number): CardDatum[]` (12 items; one card's value changes every two beats, at local frames 120, 150, 180, 210); `cardsSource(data: CardDatum[]): string`, the array literal shown on screen; `CARD_SOURCE: string`, the `<Card title value />` component snippet.
- Produces, `components/Card.tsx`: `Card({ x, y, datum })`, 260×150, `C.panel` with 1 px `C.line`, title mono 20 `C.muted`, value Headline 56, and a value bar in `C.accent` whose width is `datum.value`% of 220 px.

- [ ] **Step 1: Write the failing test**

```ts
test('the array on screen is the array the cards draw', () => {
  for (const l of [0, 119, 120, 239]) {
    const shown = new Function(`return ${cardsSource(cardsAt(l))};`)();
    assert.deepEqual(shown, cardsAt(l));
  }
  assert.notDeepEqual(cardsAt(119), cardsAt(120));
  assert.equal(cardsAt(0).length, 12);
});
```

- [ ] **Steps 2–4:** FAIL, implement `cards.ts`, PASS.
- [ ] **Step 5: Implement the scene.** Left: `CodePanel` (x=80, y=200, 760×760) with `CARD_SOURCE` then `cardsSource(cardsAt(local))`; the changed line is highlighted for one beat. Right: a 3×4 grid of `Card`s from x=900, stagger-in over bar 14 with `Stagger`. Copy `Write it once.` (780), `Feed it data.` (900) at x=80, y=96, size 80.
- [ ] **Step 6: Verify** — unit; render `--frames 790,880,905,960,1010`. Expected: the number in the code and the matching card's value and bar agree in every PNG.
- [ ] **Step 7: Commit** — `feat(examples): add the intro's components scene`

### Task 8: Live edit (34–44 s)

**Files:**
- Create: `examples/intro/shared/live-edit.ts`, `examples/intro/components/KeyCap.tsx`, `examples/intro/scenes/LiveEdit.tsx`
- Modify: `examples/intro/timeline.ts`, `examples/intro/make-score.py` (save-click cue times)
- Test: `examples/intro/test/live-edit.test.ts`

**Interfaces:**
- Produces, `shared/live-edit.ts`: `TitleProps = { fontSize: number; fill: string }`; `EDITS: { at: number; save: number; props: TitleProps }[]` — edit 1 types `96`→`160` over local 30–59 and saves at local 75; edit 2 swaps `fg`→`accent` over local 120–149 and saves at local 165; `typedAt(local): { source: string; caret: { line: number; column: number } }` (the source as typed so far); `savedAt(local): TitleProps` (props as of the last save, `{ fontSize: 96, fill: '#F5F5F7' }` before any); `SAVE_FRAMES: number[]` in local frames.
- Produces, `components/KeyCap.tsx`: `KeyCap({ x, y, pressed })`, a 120×120 square cap with a ⌘ `Path` and `S` text; `pressed` moves it 6 px down and fills `C.accent`.

- [ ] **Step 1: Write the failing test**

```ts
test('the preview changes only on save', () => {
  assert.equal(savedAt(60).fontSize, 96);       // typed 160, not saved yet
  assert.match(typedAt(60).source, /fontSize: 160/);
  assert.equal(savedAt(75).fontSize, 160);
  assert.equal(savedAt(164).fill, '#F5F5F7');
  assert.equal(savedAt(165).fill, '#3D7BFF');
  assert.deepEqual(SAVE_FRAMES, [75, 165]);
});
```

- [ ] **Steps 2–4:** FAIL, implement, PASS.
- [ ] **Step 5: Implement the scene.** Left `CodePanel` (x=80, y=200, 860×640, title `film.tsx`) showing `typedAt(local).source` with a 2 px `C.accent` caret placed by `useCodePoint`, blinking every 15 frames while idle. Right: a preview frame (x=980, y=200, 860×484) rendering a `Hello, Celesta.` title with `savedAt(local)` props, scaled into the frame. `KeyCap` at (980, 760), pressed for 6 frames from each save; the preview frame flashes its border to `C.accent` for 4 frames on save. Copy: `Save.` (local 75), `It reloads.` (165), `Frame-exact.` (240) at x=80, y=96, size 80. Mirror `SAVE_FRAMES` (+1020, in seconds) into `make-score.py`'s click cues and regenerate `music.wav`.
- [ ] **Step 6: Verify** — unit; render `--frames 1080,1094,1095,1185,1260`. Expected: at 1094 code says 160 but preview is small; 1095 preview large and key pressed; 1185 accent title; 1260 `Frame-exact.`
- [ ] **Step 7: Commit** — `feat(examples): add the intro's live-edit scene`

### Task 9: Export and agents (44–52 s)

**Files:**
- Create: `examples/intro/shared/export.ts`, `examples/intro/components/Check.tsx`, `examples/intro/scenes/Export.tsx`
- Modify: `examples/intro/timeline.ts`
- Test: `examples/intro/test/export.test.ts`

**Interfaces:**
- Produces, `shared/export.ts`: `EXPORT_COMMAND = 'celesta-exporter --react film.tsx intro.mp4'`; `framesDone(local: number): number` — 0 until the command is typed (local 30), then rises to `DURATION` by local 120 and stays; `AGENT_LOG: string[]`, four short lines (reads the Celesta skill, writes `film.tsx`, renders a contact sheet, exports `intro.mp4`).
- Produces, `components/Check.tsx`: `Check({ x, y, size = 48, color = C.accent })`, a stroked `Path`.

- [ ] **Step 1: Write the failing test**

```ts
test('the export counter fills to the full length and holds', () => {
  assert.equal(framesDone(0), 0);
  assert.equal(framesDone(120), 1800);
  assert.equal(framesDone(239), 1800);
  for (let l = 1; l < 240; l += 1) assert.ok(framesDone(l) >= framesDone(l - 1));
});
```

- [ ] **Steps 2–4:** FAIL, implement, PASS.
- [ ] **Step 5: Implement the scene.** Bars 23–24: a terminal panel (x=160, y=240, 1600×280, mono 30) types `$ ` + `EXPORT_COMMAND`; below it a 30×10 mosaic of frame tiles, tile *i* lit once `framesDone ≥ i * 6`, each tile filled `C.accent` when the ball is in the upper half of its arc at that tile's frame and `C.muted` otherwise (from `ballY`, so the mosaic is this video's own frames); a counter `framesDone / 1800`; on completion `intro.mp4` and `Check`. Bars 25–26: the terminal clears to the agent log, one line per beat. Copy: `One command to MP4.` (1320), `Your agent can write it too.` (1440) at x=160, y=120, size 80.
- [ ] **Step 6: Verify** — unit; render `--frames 1340,1400,1440,1500,1555`. Expected: mosaic partly lit at 1400, check mark at 1440, agent lines appearing by 1500.
- [ ] **Step 7: Commit** — `feat(examples): add the intro's export scene`

### Task 10: Outro and end-screen hold (52–60 s)

**Files:**
- Create: `examples/intro/scenes/Outro.tsx`
- Modify: `examples/intro/timeline.ts`; delete `examples/intro/components/Placeholder.tsx`

**Interfaces:**
- Consumes: `HOLD`, `Headline`, `C`.

- [ ] **Step 1: Write the failing check.** Run: `node $INSPECT examples/intro/film.tsx --frames 1650,1799 --json > $OUT/hold.json` and a `node -e` comparison that the two frames' layer lists are deep-equal (excluding the frame index). Expected now: FAIL or placeholder-equal; rerun after Step 2 must PASS with the real outro.
- [ ] **Step 2: Implement.** Frame 1560: impact — `CELESTA` wordmark (Space Grotesk 700, 200 px, `C.fg`) settles with `spring` at center-left (x=160, y=360); 1590: tagline `Video, written in code.` (Headline 64, `C.fg`); 1605: `React · TypeScript · macOS / Windows` (mono 28, `C.muted`); 1620: `github.com/mika-f/celesta` (mono 36, `C.accent`) with a 1 px underline drawing in. Every animation is clamped to finish by 1649; the right half (x ≥ 1100) stays empty for YouTube end-screen elements. Remove `Placeholder` and confirm no scene still uses it.
- [ ] **Step 3: Verify** — the Step 1 comparison PASSES; render `--frames 1560,1600,1650,1799`. Expected: 1650 and 1799 pixel-identical layouts with no frame counter.
- [ ] **Step 4: Commit** — `feat(examples): add the intro's outro and end-screen hold`

### Task 11: README, poster, and full verification

**Files:**
- Create: `examples/intro/README.md`, `examples/intro/poster.jpg`
- Modify: `docs/gallery.md` only if the user asks (not by default).

- [ ] **Step 1: Whole-video checks**
  - Unit: all tests PASS. Types: clean.
  - `node $INSPECT examples/intro/film.tsx --every 15` → no problems.
  - Out-of-order: inspect `--frames 900,10,900 --json`; the two 900 frames are deep-equal.
  - Cuts: render `--frames 179,180,299,300,539,540,779,780,1019,1020,1319,1320,1559,1560`; each pair shows outgoing then incoming scene, and 179/180 match in layout.
  - Contact sheet: `$EXPORTER --json --react examples/intro/film.tsx --every 30 --contact-sheet $OUT/sheet.png` → open it; pacing follows the storyboard; only palette colors; no uppercase except `CELESTA`.
  - Every export's `warnings` empty (no missing glyph or font).
- [ ] **Step 2: Poster.** `sips -Z 1280 -s format jpeg $OUT/<frame 285 png> --out examples/intro/poster.jpg` → 1280×720 JPEG.
- [ ] **Step 3: README** in the format of `examples/feature-tour/README.md` (Japanese, like the other example READMEs): one-paragraph summary, file list, the storyboard timing table, how to regenerate the score and export (`celesta-exporter --react examples/intro/film.tsx intro.mp4`), and a "real vs. staged" section: the code panels show the code that draws them (pinned by `test/`); the timeline editor, terminal, and agent log are drawn mock-ups.
- [ ] **Step 4: Final MP4.** `$EXPORTER --json --react examples/intro/film.tsx $OUT/intro.mp4`, then `ffprobe -v error -count_frames -show_entries stream=codec_type,nb_read_frames -of csv $OUT/intro.mp4` → a video stream with 1800 frames and an audio stream. Hand the path to the user to listen to; do not claim the music sounds right.
- [ ] **Step 5: Commit** — `docs(examples): describe the intro video and add its poster`
