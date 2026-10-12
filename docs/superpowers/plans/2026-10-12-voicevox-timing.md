# `timingFromVoicevox` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose where every mora, phoneme, and pause of a VOICEVOX AudioQuery sits in the synthesized WAV (`timingFromVoicevox`), and rebuild `lipSyncFromVoicevox` on it so the two always agree.

**Architecture:** `@celesta/voicevox` gains `src/query.ts` (AudioQuery types), `src/timing.ts` (the engine's timing arithmetic, returning plain data shaped like the query), and a slimmer `src/lipsync.ts` that turns a timing into keyframes for `lipSyncFromKeyframes`. The shorts-explainer example drops its hand-copied `timelineOf()` and uses the new API. Docs (package README, skill references, website) describe it.

**Tech Stack:** TypeScript 5.7 (`tsc`, NodeNext ESM), `node --test` against built `dist`, pnpm workspace, VOICEVOX Engine (docker) for the example check.

**Spec:** `docs/superpowers/specs/2026-10-12-voicevox-timing-design.md`

## Global Constraints

- All times are seconds from the WAV's first sample, in fields named `start` / `end`; never `from` / `to` (Celesta's `from` is a frame).
- With a `frameRate` (default `24000 / 256` = 93.75), each phoneme's frame count is `roundHalfToEven((length / speedScale) * frameRate)` in that order; boundaries are accumulated as integer frames and converted with `frames / frameRate`. With `frameRate: null`, unrounded seconds are accumulated.
- Pause length is `(query.pauseLength ?? pause_mora.vowel_length) * pauseLengthScale`, then the rule above. `prePhonemeLength` / `postPhonemeLength` (`sil`) are not pauses.
- Upspeak mora: appended when `interrogativeUpspeak` (default true) and `phrase.is_interrogative` is truthy and the phrase's last mora has `(pitch ?? 0) > 0`; vowel = last mora's vowel, `vowel_length` 0.15, no consonant, `text` = `{ a: 'ア', i: 'イ', u: 'ウ', e: 'エ', o: 'オ', N: 'ン' }[vowel] ?? vowel`, `upspeak: true`.
- Zero-length phonemes and moras are kept with `start === end`; lip sync emits no keyframe for them.
- Error messages from timing start with `timingFromVoicevox:`; the rule texts stay as today (`speedScale must be a positive number`, `frameRate must be a positive number or null`, `<field> must be a non-negative number`).
- `lipSyncFromVoicevox(timing, options)` throws `lipSyncFromVoicevox: options apply when the timing is computed; pass them to timingFromVoicevox`.
- No changes to `@celesta/character`. `VoicevoxLipSyncOptions` stays exported as an alias of `VoicevoxTimingOptions`.
- Code comments and docs are English except Japanese docs/locales; match the surrounding comment density.
- Edit files with the Edit tool, not `sed`/`python` rewrites.
- Scratch files (baselines, renders) go in a temporary directory outside the repository, written `<scratch>` below; substitute its path in each command.

## Review Focus

1. **A copied timing** (`structuredClone`, JSON round trip from a cache) passed to `lipSyncFromVoicevox` must give the same mouth as the original, consonants included. → Task 2, Step 1 test `a cloned timing gives the same track`.
2. **A minimal query** (no `text`, no `pitch`, no `pre/postPhonemeLength`, `pause_mora` absent rather than null, `consonant: ''`) must work: empty `text`, zero-length `sil`, no consonant phoneme. → Task 1, Step 1 test `minimal query`.
3. **No accent phrases** gives only the two `sil` phonemes, `speech` collapsed at the end of the leading silence, a closed mouth. → Task 1 test `no accent phrases`; Task 2 test `empty query is closed`.
4. **An edited query** whose interrogative phrase ends in a devoiced vowel with a positive pitch gets an upspeak mora whose `text` is the vowel string, not a throw. → Task 1 test `upspeak text falls back to the vowel`.
5. **A non-default `frameRate`** (e.g. 100) still yields contiguous phonemes with exactly equal boundaries and a duration of `frames / 100`. → Task 1 test `custom frameRate`.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `packages/voicevox/src/query.ts` (create) | `VoicevoxMora`, `VoicevoxAccentPhrase`, `VoicevoxAudioQuery` |
| `packages/voicevox/src/timing.ts` (create) | `timingFromVoicevox`, span/timed types, `VoicevoxTimingOptions`, rounding, upspeak kana, validation, the engine-mirroring header comment |
| `packages/voicevox/src/lipsync.ts` (rewrite) | `lipSyncFromVoicevox` overloads, `voicevoxVowelShape`, bilabial set, `VoicevoxLipSyncOptions` alias |
| `packages/voicevox/src/index.ts` (modify) | exports |
| `packages/voicevox/test/timing.test.mjs` (create) | timing tests |
| `packages/voicevox/test/lipsync.test.mjs` (modify) | overload, clone, empty-query tests |
| `packages/voicevox/test/runtime.test.mjs` (modify) | composition imports `timingFromVoicevox` |
| `examples/shorts-explainer/voice.ts`, `scenes/VoiceScene.tsx`, `README.md` (modify) | use the API |
| `packages/voicevox/README.md`, `skills/celesta/SKILL.md`, `skills/celesta/references/dialogue.md`, `packages/website/src/pages/docs/content/packages.tsx`, `packages/website/src/locales/{en,ja}.json` (modify) | docs |

Build once before Task 1 (from the worktree root):

```bash
pnpm install
pnpm --dir packages/react run codegen
pnpm --filter "@celesta/cli..." run build
```

After that, `pnpm --dir packages/voicevox run build` rebuilds just this package.

---

### Task 1: `timingFromVoicevox`

**Files:**
- Create: `packages/voicevox/src/query.ts`
- Create: `packages/voicevox/src/timing.ts`
- Modify: `packages/voicevox/src/lipsync.ts` (only: import the query types from `./query.js` instead of declaring them)
- Modify: `packages/voicevox/src/index.ts`
- Test: `packages/voicevox/test/timing.test.mjs`

**Interfaces:**
- Consumes: nothing new.
- Produces (exported from `dist/index.js`):
  - `timingFromVoicevox(query: VoicevoxAudioQuery, options?: VoicevoxTimingOptions): VoicevoxTiming`
  - types `VoicevoxSpan { start; end }`, `VoicevoxTimedPhoneme { phoneme; start; end }`, `VoicevoxTimedMora { text; start; end; consonant: VoicevoxTimedPhoneme | null; vowel: VoicevoxTimedPhoneme; accentPhrase: number; upspeak: boolean }`, `VoicevoxTimedPause { text; start; end; accentPhrase }`, `VoicevoxTimedAccentPhrase { text; start; end; moras; pause: VoicevoxTimedPause | null }`, `VoicevoxTiming { durationInSeconds; speech: VoicevoxSpan; accentPhrases; moras; pauses; phonemes }`, `VoicevoxTimingOptions { interrogativeUpspeak?: boolean; frameRate?: number | null }`.

- [ ] **Step 1: Write the failing tests**

Create `packages/voicevox/test/timing.test.mjs`:

```js
// Unit tests for timingFromVoicevox. Run after building @celesta/character
// and @celesta/voicevox: pnpm --dir packages/voicevox test
//
// Fixtures are described in lipsync.test.mjs.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { lipSyncFromVoicevox, timingFromVoicevox } from '../dist/index.js';

const read = (name) => JSON.parse(readFileSync(new URL(`./fixtures/${name}`, import.meta.url), 'utf8'));
const fixture = read('voicevox-zundamon-konnichiwa.json');
const synthesized = read('voicevox-engine-0.26-synthesis.json');
const clone = () => structuredClone(fixture);
const exact = { frameRate: null };
const FRAME = 1 / 93.75;
const close = (actual, expected, what) =>
  assert.ok(Math.abs(actual - expected) < 1e-9, `${what}: ${actual} vs ${expected}`);
const allQueries = [['zundamon', fixture], ...synthesized.map(({ name, query }) => [name, query])];

function assertContiguous(timing, what) {
  const { phonemes } = timing;
  assert.equal(phonemes[0].start, 0, `${what}: starts at 0`);
  for (let i = 1; i < phonemes.length; i += 1) {
    assert.equal(phonemes[i].start, phonemes[i - 1].end, `${what}: phoneme ${i}`);
  }
  assert.equal(phonemes[phonemes.length - 1].end, timing.durationInSeconds, `${what}: ends at duration`);
}

test('timingFromVoicevox: duration matches the WAV and the lip-sync track', () => {
  close(timingFromVoicevox(fixture).durationInSeconds, 2.56, 'zundamon');
  for (const { name, query, wavSamples, sampleRate } of synthesized) {
    const timing = timingFromVoicevox(query);
    close(timing.durationInSeconds, wavSamples / sampleRate, name);
    assert.equal(timing.durationInSeconds, lipSyncFromVoicevox(query).durationInSeconds, name);
  }
});

test('timingFromVoicevox: phonemes cover the WAV without gaps', () => {
  for (const [name, query] of allQueries) {
    assertContiguous(timingFromVoicevox(query), name);
    assertContiguous(timingFromVoicevox(query, exact), `${name} unrounded`);
  }
});

test('timingFromVoicevox: mirrors the query', () => {
  for (const [name, query] of allQueries) {
    const timing = timingFromVoicevox(query, { interrogativeUpspeak: false });
    assert.equal(timing.accentPhrases.length, query.accent_phrases.length, name);
    query.accent_phrases.forEach((phrase, i) => {
      const timed = timing.accentPhrases[i];
      assert.deepEqual(timed.moras.map((m) => m.text), phrase.moras.map((m) => m.text), `${name} phrase ${i}`);
      assert.equal(timed.text, phrase.moras.map((m) => m.text).join(''));
      assert.ok(timed.moras.every((m) => m.accentPhrase === i && !m.upspeak));
      assert.equal(timed.pause === null, !phrase.pause_mora);
      if (phrase.pause_mora) assert.equal(timed.pause.text, phrase.pause_mora.text);
    });
    assert.deepEqual(timing.moras, timing.accentPhrases.flatMap((p) => p.moras));
    timing.moras.forEach((m, i) => assert.equal(m, timing.accentPhrases.flatMap((p) => p.moras)[i]));
    timing.pauses.forEach((p) => assert.equal(p, timing.accentPhrases[p.accentPhrase].pause));
    assert.equal(timing.pauses.length, query.accent_phrases.filter((p) => p.pause_mora).length);
  }
});

test('timingFromVoicevox: hand-computed times (unrounded)', () => {
  const timing = timingFromVoicevox(fixture, exact);
  const moras = fixture.accent_phrases.flatMap((p) => p.moras);
  const lengthOf = (m) => (m.consonant_length ?? 0) + m.vowel_length;
  const firstPhrase = fixture.accent_phrases[0].moras.reduce((s, m) => s + lengthOf(m), 0);
  const pause = fixture.accent_phrases[0].pause_mora.vowel_length;

  assert.equal(timing.phonemes[0].phoneme, 'sil');
  close(timing.phonemes[0].end, 0.1, 'pre silence');
  const ko = timing.moras[0];
  assert.equal(ko.text, 'コ');
  close(ko.start, 0.1, 'コ start');
  assert.equal(ko.consonant.phoneme, 'k');
  close(ko.consonant.end, 0.1 + moras[0].consonant_length, 'コ consonant end');
  assert.equal(ko.vowel.phoneme, 'o');
  assert.equal(ko.end, ko.vowel.end);
  assert.equal(timing.moras[1].consonant, null); // ン

  assert.equal(timing.pauses.length, 1);
  assert.equal(timing.pauses[0].text, '、');
  close(timing.pauses[0].start, 0.1 + firstPhrase, 'pause start');
  close(timing.pauses[0].end, 0.1 + firstPhrase + pause, 'pause end');
  close(timing.accentPhrases[0].end, timing.pauses[0].start, 'phrase end excludes the pause');

  const mo = timing.moras[8];
  assert.equal(mo.text, 'モ');
  const moStart = 0.1 + pause + moras.slice(0, 8).reduce((s, m) => s + lengthOf(m), 0);
  close(mo.start, moStart, 'モ start');
  close(mo.vowel.start, moStart + moras[8].consonant_length, 'モ vowel start');

  assert.equal(timing.speech.start, timing.moras[0].start);
  assert.equal(timing.speech.end, timing.moras[timing.moras.length - 1].end);
  assert.equal(timing.speech.end, timing.phonemes[timing.phonemes.length - 1].start);
  close(timing.durationInSeconds - timing.speech.end, 0.1, 'post silence');
});

test('timingFromVoicevox: half-to-even per phoneme; a mora is the sum of its rounded phonemes', () => {
  // 0.016 s = 1.5 frames -> 2, so k + a = 4 frames, although 0.032 s rounds to 3.
  const query = {
    accent_phrases: [{ moras: [{ text: 'カ', consonant: 'k', consonant_length: 0.016, vowel: 'a', vowel_length: 0.016, pitch: 5 }] }],
    prePhonemeLength: 0,
    postPhonemeLength: 0,
  };
  const timing = timingFromVoicevox(query);
  assert.equal(timing.moras[0].consonant.end, 2 / 93.75);
  assert.equal(timing.moras[0].end, 4 / 93.75);
  assert.equal(timing.durationInSeconds, 4 / 93.75);

  // 0.048 s = 4.5 frames -> 4.
  query.accent_phrases[0].moras[0].vowel_length = 0.048;
  assert.equal(timingFromVoicevox(query).moras[0].vowel.end, 6 / 93.75);
});

test('timingFromVoicevox: interrogative upspeak', () => {
  const query = {
    accent_phrases: [{
      moras: [
        { text: 'ス', consonant: 's', consonant_length: 0.1, vowel: 'U', vowel_length: 0.1, pitch: 0 },
        { text: 'ッ', consonant: null, consonant_length: null, vowel: 'cl', vowel_length: 0.1, pitch: 0 },
        { text: 'カ', consonant: 'k', consonant_length: 0.1, vowel: 'a', vowel_length: 0.1, pitch: 5.5 },
      ],
      pause_mora: null,
      is_interrogative: true,
    }],
    prePhonemeLength: 0,
    postPhonemeLength: 0,
  };
  const timing = timingFromVoicevox(query, exact);
  assert.equal(timing.moras.length, 4);
  const up = timing.moras[3];
  assert.deepEqual(
    { text: up.text, upspeak: up.upspeak, consonant: up.consonant, vowel: up.vowel.phoneme, accentPhrase: up.accentPhrase },
    { text: 'ア', upspeak: true, consonant: null, vowel: 'a', accentPhrase: 0 },
  );
  close(up.start, 0.5, 'upspeak start');
  close(up.end, 0.65, 'upspeak end');
  assert.equal(timing.accentPhrases[0].text, 'スッカア');
  assert.equal(timing.accentPhrases[0].end, up.end);

  assert.equal(timingFromVoicevox(query, { ...exact, interrogativeUpspeak: false }).moras.length, 3);
  const lowered = structuredClone(query);
  lowered.accent_phrases[0].moras[2].pitch = 0;
  assert.equal(timingFromVoicevox(lowered, exact).moras.length, 3);
  const ending = structuredClone(query);
  ending.accent_phrases[0].moras.push({ text: 'ン', vowel: 'N', vowel_length: 0.1, pitch: 5 });
  const endingUp = timingFromVoicevox(ending, exact).moras.at(-1);
  assert.deepEqual({ text: endingUp.text, upspeak: endingUp.upspeak }, { text: 'ン', upspeak: true });
});

test('timingFromVoicevox: upspeak text falls back to the vowel', () => {
  // An edited query: a devoiced vowel with a positive pitch. The engine's
  // kana lookup would fail; the timing keeps the vowel string.
  const query = {
    accent_phrases: [{ moras: [{ text: 'ス', consonant: 's', consonant_length: 0.1, vowel: 'U', vowel_length: 0.1, pitch: 5 }], is_interrogative: true }],
  };
  const timing = timingFromVoicevox(query, exact);
  assert.equal(timing.moras[1].text, 'U');
  assert.equal(timing.moras[1].upspeak, true);
});

test('timingFromVoicevox: pause settings move pauses only; speedScale divides everything', () => {
  const query = clone();
  query.pauseLength = 0.5;
  query.pauseLengthScale = 2;
  query.speedScale = 2;
  const timing = timingFromVoicevox(query, exact);
  close(timing.phonemes[0].end, 0.05, 'pre silence is not a pause');
  close(timing.pauses[0].end - timing.pauses[0].start, 0.5, 'pause length');
  close(timing.moras[0].vowel.end - timing.moras[0].vowel.start, fixture.accent_phrases[0].moras[0].vowel_length / 2, 'vowel');
});

test('timingFromVoicevox: zero-length phonemes stay in place', () => {
  const query = {
    accent_phrases: [{
      moras: [
        { text: 'カ', consonant: 'k', consonant_length: null, vowel: 'a', vowel_length: 0.1 },
        { text: 'イ', vowel: 'i', vowel_length: 0.001 }, // 0.09 frames -> 0
      ],
    }],
  };
  const timing = timingFromVoicevox(query);
  assert.equal(timing.moras[0].consonant.phoneme, 'k');
  assert.equal(timing.moras[0].consonant.start, timing.moras[0].consonant.end);
  assert.equal(timing.moras[1].start, timing.moras[1].end);
  assert.equal(timing.moras.length, 2);
  assertContiguous(timing, 'zero-length');
});

test('timingFromVoicevox: minimal query', () => {
  const query = { accent_phrases: [{ moras: [{ consonant: '', vowel: 'a', vowel_length: 0.2 }] }] };
  const timing = timingFromVoicevox(query, exact);
  assert.equal(timing.moras[0].text, '');
  assert.equal(timing.moras[0].consonant, null);
  assert.equal(timing.accentPhrases[0].pause, null);
  assert.deepEqual(timing.phonemes.map((p) => p.phoneme), ['sil', 'a', 'sil']);
  assert.equal(timing.phonemes[0].end, 0);
  close(timing.durationInSeconds, 0.2, 'duration');
});

test('timingFromVoicevox: no accent phrases', () => {
  const timing = timingFromVoicevox({ accent_phrases: [], prePhonemeLength: 0.1, postPhonemeLength: 0.1 }, exact);
  assert.deepEqual(timing.phonemes.map((p) => p.phoneme), ['sil', 'sil']);
  assert.deepEqual([timing.moras, timing.pauses, timing.accentPhrases], [[], [], []]);
  assert.deepEqual(timing.speech, { start: timing.phonemes[0].end, end: timing.phonemes[0].end });
  close(timing.durationInSeconds, 0.2, 'duration');
});

test('timingFromVoicevox: custom frameRate', () => {
  const query = {
    accent_phrases: [{ moras: [{ consonant: 'k', consonant_length: 0.034, vowel: 'a', vowel_length: 0.105 }] }],
    prePhonemeLength: 0.1,
    postPhonemeLength: 0.1,
  };
  const timing = timingFromVoicevox(query, { frameRate: 100 });
  assertContiguous(timing, 'frameRate 100');
  // pre 10, k 3.4 -> 3, a exactly 10.5 -> 10 (half to even), post 10.
  assert.equal(timing.durationInSeconds, (10 + 3 + 10 + 10) / 100);
});

test('timingFromVoicevox: rejects invalid input', () => {
  const query = clone();
  query.speedScale = 0;
  assert.throws(() => timingFromVoicevox(query), /^Error: timingFromVoicevox: speedScale must be a positive number/);
  assert.throws(() => timingFromVoicevox(fixture, { frameRate: 0 }), /timingFromVoicevox: frameRate must be a positive number or null/);
  const negative = clone();
  negative.accent_phrases[0].moras[0].vowel_length = -1;
  assert.throws(() => timingFromVoicevox(negative), /timingFromVoicevox: vowel_length must be a non-negative number/);
  const nan = clone();
  nan.prePhonemeLength = Number.NaN;
  assert.throws(() => timingFromVoicevox(nan), /prePhonemeLength/);
  const pause = clone();
  pause.pauseLengthScale = -1;
  assert.throws(() => timingFromVoicevox(pause), /pause length/);
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --dir packages/voicevox run build && node --test packages/voicevox/test/timing.test.mjs`
Expected: FAIL — `SyntaxError: The requested module '../dist/index.js' does not provide an export named 'timingFromVoicevox'`.

- [ ] **Step 3: Move the query types to `src/query.ts`**

Create `packages/voicevox/src/query.ts`:

```ts
// The parts of a VOICEVOX Engine `audio_query` result that Celesta reads.
// Engines that share VOICEVOX's API (AivisSpeech, COEIROINK v2, ...) return
// the same shape. Fields not listed here are allowed and ignored.

/** One mora of a VOICEVOX AudioQuery. */
export interface VoicevoxMora {
  text?: string;
  consonant?: string | null;
  consonant_length?: number | null;
  vowel: string;
  vowel_length: number;
  pitch?: number;
}

export interface VoicevoxAccentPhrase {
  moras: readonly VoicevoxMora[];
  pause_mora?: VoicevoxMora | null;
  is_interrogative?: boolean;
}

/**
 * The JSON returned by VOICEVOX Engine's `POST /audio_query` (and passed to
 * `POST /synthesis`).
 */
export interface VoicevoxAudioQuery {
  accent_phrases: readonly VoicevoxAccentPhrase[];
  speedScale?: number;
  prePhonemeLength?: number;
  postPhonemeLength?: number;
  /** Newer engines: when not null, replaces the length of every pause mora. */
  pauseLength?: number | null;
  /** Newer engines: multiplies the length of every pause mora. */
  pauseLengthScale?: number;
}
```

In `packages/voicevox/src/lipsync.ts`, delete the three interfaces `VoicevoxMora`, `VoicevoxAccentPhrase`, `VoicevoxAudioQuery` (from `/** One mora of a VOICEVOX AudioQuery. Only the fields lip sync reads. */` through the closing `}` of `VoicevoxAudioQuery`) and add below the existing `@celesta/character` import:

```ts
import type { VoicevoxAudioQuery } from './query.js';
```

Run `pnpm --dir packages/voicevox run build && node --test packages/voicevox/test/lipsync.test.mjs` — expected PASS (pure move).

- [ ] **Step 4: Write `src/timing.ts`**

Create `packages/voicevox/src/timing.ts`:

```ts
// Where every phoneme of a VOICEVOX AudioQuery falls in the synthesized WAV.
//
// The arithmetic mirrors `_query_to_decoder_feature` in voicevox_engine's
// `tts_pipeline/tts_engine.py`: optional interrogative upspeak (a 0.15 s
// mora repeating the last vowel, its `text` the vowel's kana), then a
// `prePhonemeLength` silence, every accent phrase's moras followed by its
// `pause_mora`, a `postPhonemeLength` silence; `pauseLength` /
// `pauseLengthScale` adjust only the pause moras, and `speedScale` divides
// everything. Each phoneme is then rounded (half to even, like numpy) to the
// engine's 93.75 Hz frame grid, so a mora lasts the sum of its rounded
// phonemes. That rounding is what makes the times match the WAV to the
// sample; without it the error accumulates (about 50 ms over a 1.7 s line at
// `speedScale: 1.5`).

import type { VoicevoxAudioQuery, VoicevoxMora } from './query.js';

/** A stretch of the WAV, in seconds from its first sample. `end >= start`. */
export interface VoicevoxSpan {
  readonly start: number;
  readonly end: number;
}

export interface VoicevoxTimedPhoneme extends VoicevoxSpan {
  /**
   * As in the query: 'k', 'o', 'N', 'cl', 'pau'. The pre/post silence is
   * 'sil', the engine's mora name for it (its decoder folds it into 'pau');
   * keeping 'sil' tells the surrounding silence apart from a pause.
   */
  readonly phoneme: string;
}

export interface VoicevoxTimedMora extends VoicevoxSpan {
  /** The query's `text` ('コ'); for an upspeak mora, its vowel's kana like the engine ('ア'). */
  readonly text: string;
  readonly consonant: VoicevoxTimedPhoneme | null;
  readonly vowel: VoicevoxTimedPhoneme;
  /** Index into `query.accent_phrases`. */
  readonly accentPhrase: number;
  /** True for the mora interrogative upspeak appended; it is not in the query. */
  readonly upspeak: boolean;
}

export interface VoicevoxTimedPause extends VoicevoxSpan {
  /** The pause mora's `text`, usually the punctuation ('、'). Empty when absent. */
  readonly text: string;
  readonly accentPhrase: number;
}

/**
 * From the first mora's start to the last mora's end; the pause after it is
 * not included. A phrase without moras has `start === end`.
 */
export interface VoicevoxTimedAccentPhrase extends VoicevoxSpan {
  /** Its moras' text joined. */
  readonly text: string;
  /** `moras[j]` is `query.accent_phrases[i].moras[j]`; an upspeak mora comes last. */
  readonly moras: readonly VoicevoxTimedMora[];
  readonly pause: VoicevoxTimedPause | null;
}

export interface VoicevoxTiming {
  /** The WAV's length; equal to the `LipSyncTrack`'s. */
  readonly durationInSeconds: number;
  /**
   * From the first mora's start to the last mora's end: the voice without
   * pre/post silence. Without moras, both are the end of the pre silence.
   */
  readonly speech: VoicevoxSpan;
  /** One per `query.accent_phrases`, same order. */
  readonly accentPhrases: readonly VoicevoxTimedAccentPhrase[];
  /** Every mora in order, upspeak moras included. The same objects as in `accentPhrases`. */
  readonly moras: readonly VoicevoxTimedMora[];
  /** Every pause in order. The same objects as in `accentPhrases`. */
  readonly pauses: readonly VoicevoxTimedPause[];
  /**
   * Every phoneme the engine synthesizes, in order, `sil` and `pau`
   * included; together they cover [0, durationInSeconds] without gaps.
   */
  readonly phonemes: readonly VoicevoxTimedPhoneme[];
}

export interface VoicevoxTimingOptions {
  /**
   * Whether the voice was synthesized with interrogative upspeak, which
   * appends a 0.15 s mora to accent phrases marked `is_interrogative`.
   * Matches `/synthesis`'s `enable_interrogative_upspeak`, default `true`.
   */
  interrogativeUpspeak?: boolean;
  /**
   * Frames per second the engine quantizes each phoneme length to.
   * VOICEVOX synthesizes at 24 kHz with a 256-sample hop: 93.75. Pass `null`
   * to keep the unrounded lengths, e.g. for an engine that does not use
   * this grid.
   */
  frameRate?: number | null;
}

const VOICEVOX_FRAME_RATE = 24000 / 256;
const UPSPEAK_LENGTH = 0.15;

// `mora_phonemes_to_mora_kana` in voicevox_engine's
// `tts_pipeline/mora_mapping.py`, for the vowels an upspeak mora can repeat
// (the others have pitch 0).
const UPSPEAK_KANA: Readonly<Record<string, string>> = { a: 'ア', i: 'イ', u: 'ウ', e: 'エ', o: 'オ', N: 'ン' };

// numpy's `np.round`: halves go to the even neighbour.
function roundHalfToEven(value: number): number {
  const floor = Math.floor(value);
  const fraction = value - floor;
  if (fraction > 0.5) return floor + 1;
  if (fraction < 0.5) return floor;
  return floor % 2 === 0 ? floor : floor + 1;
}

function lengthOf(value: number | null | undefined, what: string): number {
  const length = value ?? 0;
  if (!Number.isFinite(length) || length < 0) {
    throw new Error(`timingFromVoicevox: ${what} must be a non-negative number, got ${value}`);
  }
  return length;
}

/**
 * Where each mora, phoneme, and pause of a VOICEVOX AudioQuery falls in the
 * WAV synthesized from it, in seconds from its first sample (including
 * `prePhonemeLength`). Pass the same query and options that synthesized the
 * voice. The result is plain data shaped like the query:
 * `accentPhrases[i].moras[j]` is `query.accent_phrases[i].moras[j]`.
 */
export function timingFromVoicevox(
  query: VoicevoxAudioQuery,
  options: VoicevoxTimingOptions = {},
): VoicevoxTiming {
  const speedScale = query.speedScale ?? 1;
  if (!Number.isFinite(speedScale) || speedScale <= 0) {
    throw new Error(`timingFromVoicevox: speedScale must be a positive number, got ${query.speedScale}`);
  }
  const pauseLengthScale = query.pauseLengthScale ?? 1;
  const upspeak = options.interrogativeUpspeak ?? true;
  const frameRate = options.frameRate === undefined ? VOICEVOX_FRAME_RATE : options.frameRate;
  if (frameRate !== null && (!Number.isFinite(frameRate) || frameRate <= 0)) {
    throw new Error(`timingFromVoicevox: frameRate must be a positive number or null, got ${frameRate}`);
  }
  // The clock counts engine frames when quantizing, so boundaries stay exact
  // integers until they become seconds; otherwise it counts seconds.
  const ticks = frameRate === null
    ? (length: number): number => length / speedScale
    : (length: number): number => roundHalfToEven((length / speedScale) * frameRate);
  const seconds = frameRate === null ? (clock: number): number => clock : (clock: number): number => clock / frameRate;

  let clock = 0;
  const phonemes: VoicevoxTimedPhoneme[] = [];
  const phoneme = (name: string, length: number): VoicevoxTimedPhoneme => {
    const start = seconds(clock);
    clock += ticks(length);
    const timed = { phoneme: name, start, end: seconds(clock) };
    phonemes.push(timed);
    return timed;
  };
  const mora = (source: VoicevoxMora, text: string, accentPhrase: number, isUpspeak: boolean): VoicevoxTimedMora => {
    const start = seconds(clock);
    const consonant = source.consonant
      ? phoneme(source.consonant, lengthOf(source.consonant_length, 'consonant_length'))
      : null;
    const vowel = phoneme(source.vowel, lengthOf(source.vowel_length, 'vowel_length'));
    return { text, start, end: vowel.end, consonant, vowel, accentPhrase, upspeak: isUpspeak };
  };

  const leading = phoneme('sil', lengthOf(query.prePhonemeLength, 'prePhonemeLength'));
  const accentPhrases = query.accent_phrases.map((phrase, index): VoicevoxTimedAccentPhrase => {
    const start = seconds(clock);
    const moras = phrase.moras.map((source) => mora(source, source.text ?? '', index, false));
    const last = phrase.moras[phrase.moras.length - 1];
    if (upspeak && phrase.is_interrogative && last !== undefined && (last.pitch ?? 0) > 0) {
      const text = UPSPEAK_KANA[last.vowel] ?? last.vowel;
      moras.push(mora({ vowel: last.vowel, vowel_length: UPSPEAK_LENGTH }, text, index, true));
    }
    const end = seconds(clock);
    let pause: VoicevoxTimedPause | null = null;
    if (phrase.pause_mora) {
      const base = query.pauseLength ?? lengthOf(phrase.pause_mora.vowel_length, 'pause_mora.vowel_length');
      const timed = phoneme('pau', lengthOf(base * pauseLengthScale, 'pause length'));
      pause = { text: phrase.pause_mora.text ?? '', start: timed.start, end: timed.end, accentPhrase: index };
    }
    return { text: moras.map((m) => m.text).join(''), start, end, moras, pause };
  });
  phoneme('sil', lengthOf(query.postPhonemeLength, 'postPhonemeLength'));

  const moras = accentPhrases.flatMap((phrase) => phrase.moras);
  const pauses = accentPhrases.flatMap((phrase) => (phrase.pause ? [phrase.pause] : []));
  const speech = moras.length > 0
    ? { start: moras[0].start, end: moras[moras.length - 1].end }
    : { start: leading.end, end: leading.end };
  return { durationInSeconds: seconds(clock), speech, accentPhrases, moras, pauses, phonemes };
}
```

- [ ] **Step 5: Export it**

Replace `packages/voicevox/src/index.ts` with:

```ts
export { lipSyncFromVoicevox, voicevoxVowelShape } from './lipsync.js';
export type { VoicevoxLipSyncOptions } from './lipsync.js';
export type { VoicevoxAccentPhrase, VoicevoxAudioQuery, VoicevoxMora } from './query.js';
export { timingFromVoicevox } from './timing.js';
export type {
  VoicevoxSpan,
  VoicevoxTimedAccentPhrase,
  VoicevoxTimedMora,
  VoicevoxTimedPause,
  VoicevoxTimedPhoneme,
  VoicevoxTiming,
  VoicevoxTimingOptions,
} from './timing.js';
```

`VoicevoxLipSyncOptions` is still declared in `lipsync.ts` at this point; Task 2 turns it into an alias.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `pnpm --dir packages/voicevox run build && pnpm --dir packages/voicevox run test`
Expected: PASS — all of `timing.test.mjs`, and `lipsync.test.mjs` / `runtime.test.mjs` unchanged and passing.

- [ ] **Step 7: Commit**

```bash
git add packages/voicevox/src packages/voicevox/test/timing.test.mjs
git commit -m "feat(voicevox): expose mora and phoneme timing with timingFromVoicevox (#226)"
```

---

### Task 2: Rebuild `lipSyncFromVoicevox` on the timing

**Files:**
- Modify: `packages/voicevox/src/lipsync.ts` (rewrite)
- Modify: `packages/voicevox/src/index.ts` (no change expected; verify)
- Test: `packages/voicevox/test/lipsync.test.mjs`, `packages/voicevox/test/runtime.test.mjs`

**Interfaces:**
- Consumes: `timingFromVoicevox`, `VoicevoxTiming`, `VoicevoxTimingOptions` from `./timing.js`; `VoicevoxAudioQuery` from `./query.js`.
- Produces: `lipSyncFromVoicevox(query: VoicevoxAudioQuery, options?: VoicevoxTimingOptions): LipSyncTrack`, `lipSyncFromVoicevox(timing: VoicevoxTiming): LipSyncTrack`, `type VoicevoxLipSyncOptions = VoicevoxTimingOptions`.

- [ ] **Step 1: Capture the current mouth output as a baseline**

Before editing `lipsync.ts`, with Task 1's build in `dist`, write the current track for every fixture to `<scratch>` (not committed). Sample at half-millisecond offsets so no sample lands exactly on a frame boundary (k / 93.75 s hits whole milliseconds every 32 ms):

```bash
node --input-type=module -e "
import { readFileSync, writeFileSync } from 'node:fs';
import { lipSyncFromVoicevox } from './packages/voicevox/dist/index.js';
const read = (n) => JSON.parse(readFileSync('packages/voicevox/test/fixtures/' + n, 'utf8'));
const queries = [['zundamon', read('voicevox-zundamon-konnichiwa.json')], ...read('voicevox-engine-0.26-synthesis.json').map((s) => [s.name, s.query])];
const out = {};
for (const [name, q] of queries) for (const [label, opts] of [['grid', {}], ['exact', { frameRate: null }], ['flat', { interrogativeUpspeak: false }]]) {
  const t = lipSyncFromVoicevox(q, opts);
  const shapes = [];
  for (let i = 0; i < Math.ceil(t.durationInSeconds * 1000) + 5; i += 1) shapes.push(t.mouthAtSeconds((i + 0.5) / 1000));
  out[name + '/' + label] = { duration: t.durationInSeconds, shapes: shapes.join(',') };
}
writeFileSync(process.argv[1], JSON.stringify(out));
" "<scratch>/lipsync-baseline.json"
```

Expected: the file exists with 18 entries.

- [ ] **Step 2: Write the failing tests**

Append to `packages/voicevox/test/lipsync.test.mjs`, and change its import line to
`import { lipSyncFromVoicevox, timingFromVoicevox, voicevoxVowelShape } from '../dist/index.js';`:

```js
// Every 0.5 ms offset, so samples never sit exactly on a frame boundary.
function shapesOf(track) {
  const shapes = [];
  for (let i = 0; i < Math.ceil(track.durationInSeconds * 1000) + 5; i += 1) {
    shapes.push(track.mouthAtSeconds((i + 0.5) / 1000));
  }
  return shapes;
}

test('lipSyncFromVoicevox: a timing gives the same track as its query', () => {
  const queries = [fixture, ...synthesized.map((s) => s.query)];
  for (const query of queries) {
    for (const options of [{}, exact, { interrogativeUpspeak: false }]) {
      const fromQuery = lipSyncFromVoicevox(query, options);
      const fromTiming = lipSyncFromVoicevox(timingFromVoicevox(query, options));
      assert.equal(fromTiming.durationInSeconds, fromQuery.durationInSeconds);
      assert.deepEqual(shapesOf(fromTiming), shapesOf(fromQuery));
    }
  }
});

test('lipSyncFromVoicevox: a cloned timing gives the same track', () => {
  const timing = timingFromVoicevox(fixture);
  const copied = JSON.parse(JSON.stringify(timing));
  assert.deepEqual(shapesOf(lipSyncFromVoicevox(copied)), shapesOf(lipSyncFromVoicevox(timing)));
  assert.deepEqual(shapesOf(lipSyncFromVoicevox(structuredClone(timing))), shapesOf(lipSyncFromVoicevox(timing)));
  // The bilabial m of モ still closes the lips in the copy.
  const mo = timing.moras[8];
  const track = lipSyncFromVoicevox(copied);
  assert.equal(track.mouthAtSeconds((mo.consonant.start + mo.consonant.end) / 2), 'closed');
  assert.equal(track.mouthAtSeconds((mo.vowel.start + mo.vowel.end) / 2), 'o');
});

test('lipSyncFromVoicevox: options with a timing throw', () => {
  const timing = timingFromVoicevox(fixture);
  assert.throws(
    () => lipSyncFromVoicevox(timing, { frameRate: null }),
    /lipSyncFromVoicevox: options apply when the timing is computed; pass them to timingFromVoicevox/,
  );
});

test('lipSyncFromVoicevox: empty query is closed', () => {
  const track = lipSyncFromVoicevox({ accent_phrases: [], prePhonemeLength: 0.1, postPhonemeLength: 0.1 }, exact);
  assert.ok(Math.abs(track.durationInSeconds - 0.2) < 1e-9);
  assert.ok(shapesOf(track).every((shape) => shape === 'closed'));
});
```

In `packages/voicevox/test/runtime.test.mjs`, change the composition's import and track lines to:

```ts
        import { lipSyncFromVoicevox, timingFromVoicevox, type VoicevoxAudioQuery, type VoicevoxTiming } from '@celesta/voicevox';
```

```ts
        const timing: VoicevoxTiming = timingFromVoicevox(query, { frameRate: null });
        const track: LipSyncTrack = lipSyncFromVoicevox(timing);
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `pnpm --dir packages/voicevox run build && node --test packages/voicevox/test/lipsync.test.mjs`
Expected: FAIL — `a timing gives the same track as its query` and the clone test fail (the current function reads `timing.accent_phrases`, which is undefined: `TypeError: query.accent_phrases is not iterable`), and the options test fails for the same reason instead of the expected message.

- [ ] **Step 4: Rewrite `src/lipsync.ts`**

Replace the whole file with:

```ts
// Builds a lip-sync track from a VOICEVOX Engine `audio_query` result.
//
// Unlike `loadLipSync`, which guesses where the vowels fall inside a WAV,
// an AudioQuery states the length of every consonant and vowel the engine
// will synthesize, so the mouth lines up with the voice exactly. The times
// come from `timingFromVoicevox`, so the mouth and any other use of the
// timing (lit moras, a face change at a pause) agree.

import { lipSyncFromKeyframes, type LipSyncTrack, type MouthKeyframe, type MouthShape } from '@celesta/character';

import type { VoicevoxAudioQuery } from './query.js';
import { timingFromVoicevox } from './timing.js';
import type { VoicevoxSpan, VoicevoxTiming, VoicevoxTimingOptions } from './timing.js';

/** The options of `lipSyncFromVoicevox`; the same as `timingFromVoicevox`'s. */
export type VoicevoxLipSyncOptions = VoicevoxTimingOptions;

// Consonants made by closing both lips. Showing the next vowel's open mouth
// during them looks like the lips never meet, so they stay `closed`; every
// other consonant anticipates its vowel, which is how the mouth actually
// moves into a mora.
const BILABIAL_CONSONANTS = new Set(['m', 'my', 'b', 'by', 'p', 'py']);

/**
 * Mouth shape for a VOICEVOX vowel phoneme. Devoiced vowels (`A I U E O`)
 * keep their vowel's shape; `N` (ん), `cl` (っ), `pau`, `sil` and anything
 * unknown are `closed`.
 */
export function voicevoxVowelShape(vowel: string): MouthShape {
  switch (vowel) {
    case 'a':
    case 'A':
      return 'a';
    case 'i':
    case 'I':
      return 'i';
    case 'u':
    case 'U':
      return 'u';
    case 'e':
    case 'E':
      return 'e';
    case 'o':
    case 'O':
      return 'o';
    default:
      return 'closed';
  }
}

/**
 * Converts a VOICEVOX AudioQuery, or the `timingFromVoicevox` result for
 * it, into a `LipSyncTrack` for `<Dialogue lipSync>`, `<CharacterView
 * lipSync>` or `useLipSync`. Pass the same query that synthesized the
 * voice, so speed and pause edits are reflected. Time 0 is the start of the
 * WAV, including `prePhonemeLength`.
 */
export function lipSyncFromVoicevox(query: VoicevoxAudioQuery, options?: VoicevoxTimingOptions): LipSyncTrack;
export function lipSyncFromVoicevox(timing: VoicevoxTiming): LipSyncTrack;
export function lipSyncFromVoicevox(
  input: VoicevoxAudioQuery | VoicevoxTiming,
  options?: VoicevoxTimingOptions,
): LipSyncTrack {
  if ('accent_phrases' in input) {
    return trackOf(timingFromVoicevox(input, options));
  }
  if (options !== undefined) {
    throw new Error('lipSyncFromVoicevox: options apply when the timing is computed; pass them to timingFromVoicevox');
  }
  return trackOf(input);
}

// Walks the phonemes in order through the timing's structure rather than by
// object identity, so a copied timing (structuredClone, JSON) works too.
function trackOf(timing: VoicevoxTiming): LipSyncTrack {
  const keyframes: MouthKeyframe[] = [];
  const push = (span: VoicevoxSpan, mouth: MouthShape): void => {
    if (span.end > span.start) keyframes.push({ seconds: span.start, mouth });
  };
  const { phonemes } = timing;
  push(phonemes[0], 'closed');
  for (const phrase of timing.accentPhrases) {
    for (const mora of phrase.moras) {
      const vowel = voicevoxVowelShape(mora.vowel.phoneme);
      if (mora.consonant) {
        push(mora.consonant, BILABIAL_CONSONANTS.has(mora.consonant.phoneme) ? 'closed' : vowel);
      }
      push(mora.vowel, vowel);
    }
    if (phrase.pause) push(phrase.pause, 'closed');
  }
  push(phonemes[phonemes.length - 1], 'closed');
  return lipSyncFromKeyframes(keyframes, timing.durationInSeconds);
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `pnpm --dir packages/voicevox run build && pnpm --dir packages/voicevox run test`
Expected: PASS — every test in `lipsync.test.mjs` (the pre-existing ones unchanged), `timing.test.mjs`, and `runtime.test.mjs` (both the source and packaged CLI variants; the packaged one type-checks the composition that now uses `VoicevoxTiming`).

If `runtime.test.mjs` fails with a missing `cli/dist`, rerun `pnpm --filter "@celesta/cli..." run build` first.

- [ ] **Step 6: Compare against the baseline**

```bash
node --input-type=module -e "
import { readFileSync } from 'node:fs';
import { lipSyncFromVoicevox } from './packages/voicevox/dist/index.js';
const base = JSON.parse(readFileSync(process.argv[1], 'utf8'));
const read = (n) => JSON.parse(readFileSync('packages/voicevox/test/fixtures/' + n, 'utf8'));
const queries = [['zundamon', read('voicevox-zundamon-konnichiwa.json')], ...read('voicevox-engine-0.26-synthesis.json').map((s) => [s.name, s.query])];
let diffs = 0;
for (const [name, q] of queries) for (const [label, opts] of [['grid', {}], ['exact', { frameRate: null }], ['flat', { interrogativeUpspeak: false }]]) {
  const t = lipSyncFromVoicevox(q, opts);
  const shapes = [];
  for (let i = 0; i < Math.ceil(t.durationInSeconds * 1000) + 5; i += 1) shapes.push(t.mouthAtSeconds((i + 0.5) / 1000));
  const b = base[name + '/' + label];
  if (Math.abs(b.duration - t.durationInSeconds) > 1e-9 || b.shapes !== shapes.join(',')) { diffs += 1; console.log('DIFF', name, label); }
}
console.log(diffs === 0 ? 'identical' : diffs + ' differences');
" "<scratch>/lipsync-baseline.json"
```

Expected: `identical`. Any `DIFF` is a regression: stop and find which keyframe changed.

- [ ] **Step 7: Commit**

```bash
git add packages/voicevox/src packages/voicevox/test/lipsync.test.mjs packages/voicevox/test/runtime.test.mjs
git commit -m "refactor(voicevox): build lipSyncFromVoicevox on timingFromVoicevox (#226)"
```

---

### Task 3: Use the timing in the shorts-explainer example

**Files:**
- Modify: `examples/shorts-explainer/voice.ts`
- Modify: `examples/shorts-explainer/scenes/VoiceScene.tsx:36-37`
- Modify: `examples/shorts-explainer/README.md:11`

**Interfaces:**
- Consumes: `timingFromVoicevox`, `lipSyncFromVoicevox(timing)`, `voicevoxVowelShape` from `@celesta/voicevox`.
- Produces: `Mora = { text: string; start: number; end: number; mouth: MouthShape }` and `VoicedLine.faces: { at: number; face: Face }[]` (seconds into the voice); `faceAt`, `lineAt`, `lineFrom`, `plan` keep their signatures.

- [ ] **Step 1: Render the voice and PSD scenes before the change**

Start VOICEVOX Engine and generate the assets (the voices are not committed):

```bash
docker run --rm -d -p 50021:50021 --name celesta-voicevox voicevox/voicevox_engine:cpu-ubuntu24.04-latest
node examples/shorts-explainer/make-voices.ts
node examples/shorts-explainer/prepare-assets.ts
node skills/celesta/scripts/inspect.mjs examples/shorts-explainer/film.tsx --every 6 > "<scratch>/explainer-before.txt"
```

Wait for the engine (`curl -s http://127.0.0.1:50021/version`) before `make-voices.ts`. Keep the frame PNGs/contact sheet inspect.mjs reports for the 03 (voice) and 04 (psd) scenes. If docker or the engine image is unavailable, skip Steps 1 and 4's rendering, say so in the PR, and rely on Step 4's evaluation of `prepareVoices()` with a hand-written query instead (see Step 4).

- [ ] **Step 2: Replace `timelineOf()` in `voice.ts`**

Change the `@celesta/voicevox` import to:

```ts
import { lipSyncFromVoicevox, timingFromVoicevox, voicevoxVowelShape } from '@celesta/voicevox';
```

Change the header comment's last clause and the two types:

```ts
// The voiced script, timed from the WAVs make-voices.ts wrote, with each
// line's lip sync, mora times and face changes taken from the timing of the
// AudioQuery next to it.

export type Mora = { text: string; start: number; end: number; mouth: MouthShape };
```

```ts
  // The speaker's face from `at` seconds into the voice on.
  faces: { at: number; face: Face }[];
```

In `prepareVoices()`, replace

```ts
    const { moras, pauses } = timelineOf(query);
    // One face per clause; the next clause's face starts with the pause before it.
    const faces = [line.expression ?? 'normal'].flat();
    if (faces.length > pauses.length + 1) {
      throw new Error(`script line "${line.id}" has ${faces.length} faces but its voice has only ${pauses.length + 1} clauses`);
    }
```

with

```ts
    const timing = timingFromVoicevox(query);
    // One face per clause; the next clause's face starts with the pause before it.
    const faces = [line.expression ?? 'normal'].flat();
    if (faces.length > timing.pauses.length + 1) {
      throw new Error(`script line "${line.id}" has ${faces.length} faces but its voice has only ${timing.pauses.length + 1} clauses`);
    }
```

and the three returned fields

```ts
      lipSync: lipSyncFromVoicevox(query),
      moras,
      faces: faces.map((face, i) => ({ from: i === 0 ? 0 : pauses[i - 1], face })),
```

with

```ts
      lipSync: lipSyncFromVoicevox(timing),
      moras: timing.moras.map((mora) => ({
        text: mora.text, start: mora.start, end: mora.end, mouth: voicevoxVowelShape(mora.vowel.phoneme),
      })),
      faces: faces.map((face, i) => ({ at: i === 0 ? 0 : timing.pauses[i - 1].start, face })),
```

Delete the whole `timelineOf` function and its comment (`// Where each mora and each pause starts in the WAV, ...` through its closing `}`).

In `faceAt`, change `if (cue.from <= seconds) face = cue.face;` to `if (cue.at <= seconds) face = cue.face;`.

- [ ] **Step 3: Update `VoiceScene.tsx` and the README**

In `examples/shorts-explainer/scenes/VoiceScene.tsx`, change

```tsx
        const now = seconds >= mora.from && seconds < mora.to;
        const past = seconds >= mora.to;
```

to

```tsx
        const now = seconds >= mora.start && seconds < mora.end;
        const past = seconds >= mora.end;
```

In `examples/shorts-explainer/README.md` line 11, change `AudioQuery から口パク（`lipSyncFromVoicevox`）とモーラの時刻を作ります。` to `AudioQuery からモーラの時刻（`timingFromVoicevox`）と口パク（`lipSyncFromVoicevox`）を作ります。`

Then check nothing else reads the old names:

```bash
grep -rn "timelineOf\|mora\.from\|mora\.to\|cue\.from\|faces.*from:" examples/shorts-explainer
```

Expected: no output.

- [ ] **Step 4: Render again and compare**

```bash
node skills/celesta/scripts/inspect.mjs examples/shorts-explainer/film.tsx --every 6 > "<scratch>/explainer-after.txt"
```

Expected: no errors; the 03 panel's lit chip follows the voice, and the question line `じゃあ、わたしたちの声は？` (`voice-1`) shows one extra trailing chip (the upspeak mora, the vowel's kana) lit during its last 0.15 s. Differences from the "before" render are limited to chip highlight changes of at most a frame at mora boundaries, the upspeak chip, and the `N moras` label of question lines. The 04 (psd) scene's face changes at the same frames, give or take one frame. Look at the contact sheet / frames for both scenes rather than only diffing the text output.

Without docker: write a one-line query to `<scratch>/q.json` and evaluate `timingFromVoicevox` / `voicevoxVowelShape` mapping the same way `prepareVoices()` does in a `node --input-type=module -e` snippet against `packages/voicevox/dist/index.js`, and state in the PR that the example itself was not rendered.

Stop the engine: `docker stop celesta-voicevox`.

- [ ] **Step 5: Commit**

```bash
git add examples/shorts-explainer/voice.ts examples/shorts-explainer/scenes/VoiceScene.tsx examples/shorts-explainer/README.md
git commit -m "refactor(examples): time moras and faces with timingFromVoicevox (#226)"
```

---

### Task 4: Documentation

**Files:**
- Modify: `packages/voicevox/README.md`
- Modify: `skills/celesta/SKILL.md:188`
- Modify: `skills/celesta/references/dialogue.md` (after the "Lip sync from VOICEVOX timing" bullets, before `### Image mouths`)
- Modify: `packages/website/src/pages/docs/content/packages.tsx:21-24`
- Modify: `packages/website/src/locales/en.json` and `ja.json` (`docs.chapters.voicevox`)

**Interfaces:**
- Consumes: the API from Tasks 1–2, exactly as named there.
- Produces: docs only.

- [ ] **Step 1: Check the compatible-engine claim**

The README and `dialogue.md` say engines sharing the API (AivisSpeech, COEIROINK v2) return the same shape. Check whether AivisSpeech Engine honors the query's `consonant_length` / `vowel_length` when synthesizing: read the AivisSpeech Engine README / API docs (https://github.com/Aivis-Project/AivisSpeech-Engine) for how it treats phoneme lengths in `/synthesis`.

- If it documents that phoneme lengths (or `pitch`) are ignored / not supported, the timing is not exact for it. Use wording (a) below.
- If it honors them, or nothing says otherwise, use wording (b).

Record what you found (link + one sentence) for the PR description.

- [ ] **Step 2: Update `packages/voicevox/README.md`**

Replace the first paragraph and add a timing section after the lip-sync example's paragraph that ends `` `{ interrogativeUpspeak: false }` when synthesis disables upspeak. ``:

First paragraph, wording (b):

```md
VOICEVOX AudioQuery timing and lip sync for Celesta React compositions,
including engines that use the same query format. Dialogue components, WAV lip
sync, and `LipSyncTrack` are in `@celesta/character`.
```

Wording (a) instead ends the first sentence with `..., for VOICEVOX Engine. Engines that accept the same query but choose their own phoneme lengths (such as AivisSpeech) are not timed exactly.`

New section:

````md
## Mora and phoneme timing

`timingFromVoicevox(query)` returns where every mora, phoneme, and pause of
the query falls in the WAV, in seconds from its first sample. It is the same
timing the lip-sync track uses, so lit moras or a face change at a pause stay
in step with the mouth.

```ts
import { lipSyncFromVoicevox, timingFromVoicevox } from '@celesta/voicevox';

const timing = timingFromVoicevox(helloQuery);
const hello = lipSyncFromVoicevox(timing);

timing.moras;           // [{ text: 'コ', start, end, consonant, vowel, accentPhrase, upspeak }, ...]
timing.pauses[0].start; // where the 、 pause begins
timing.speech;          // { start, end } without the leading and trailing silence
```

`timing.accentPhrases[i].moras[j]` is `query.accent_phrases[i].moras[j]`, so
fields such as `pitch` are read from the query at the same index. With
interrogative upspeak, the 0.15 s mora the engine appends comes last in its
phrase with `upspeak: true` and the vowel's kana as `text`. Phonemes rounded to
zero frames stay in the lists with `start === end`. Pass options to
`timingFromVoicevox`, not to `lipSyncFromVoicevox(timing)`, which throws when
given both.
````

Replace the `Exports:` paragraph with:

```md
Exports: `timingFromVoicevox`, `lipSyncFromVoicevox`, `voicevoxVowelShape`,
and the types `VoicevoxAudioQuery`, `VoicevoxAccentPhrase`, `VoicevoxMora`,
`VoicevoxTiming`, `VoicevoxTimedAccentPhrase`, `VoicevoxTimedMora`,
`VoicevoxTimedPause`, `VoicevoxTimedPhoneme`, `VoicevoxSpan`,
`VoicevoxTimingOptions`, and `VoicevoxLipSyncOptions` (the same options).
```

- [ ] **Step 3: Update the skill references**

`skills/celesta/SKILL.md` line 188: `| `@celesta/voicevox` | `lipSyncFromVoicevox` |` → `| `@celesta/voicevox` | `lipSyncFromVoicevox`, `timingFromVoicevox`, `voicevoxVowelShape`, and their types |`.

`skills/celesta/references/dialogue.md`: in the "Lip sync from VOICEVOX timing" intro, wording (a) changes `(or a compatible engine such as\nAivisSpeech)` to `` (engines that take the same query but choose their own phoneme lengths, such as AivisSpeech, are not timed exactly)``; wording (b) leaves it. Then insert before `### Image mouths`:

````md
### Mora and phoneme timing from VOICEVOX

`timingFromVoicevox(query, options?)` returns the timing the VOICEVOX lip-sync
track is built from: seconds from the start of the WAV (`start` / `end`, not
frames) for every phoneme, mora, pause, and accent phrase. Use it to light
moras as they are spoken, time an effect to a mora, or change a face at a
pause, and build the mouth from the same timing so they agree:

```ts
import { lipSyncFromVoicevox, timingFromVoicevox, voicevoxVowelShape } from '@celesta/voicevox';

const timing = timingFromVoicevox(query);
const line = {
  id: 'voice-1', audio: './voices/voice-1.wav', text: 'じゃあ、わたしたちの声は？',
  lipSync: lipSyncFromVoicevox(timing),
};

// Per frame, inside the line: seconds since the voice started.
const seconds = (frame - planned.from) / fps;
const current = timing.moras.find((m) => m.start <= seconds && seconds < m.end);
const secondClause = timing.pauses[0]?.start ?? Infinity; // face change point
```

- `timing.moras` lists every mora in order; `timing.accentPhrases[i].moras[j]`
  is `query.accent_phrases[i].moras[j]` (read `pitch` and other fields there).
- `timing.pauses[k]` is the k-th pause (`、`); a clause boundary is its `start`.
- `timing.speech` spans the voice without `prePhonemeLength` /
  `postPhonemeLength`; `timing.durationInSeconds` is the WAV length.
- A question's upspeak mora (0.15 s, `upspeak: true`, `text` the vowel's
  kana) comes last in its phrase; skip it with `!m.upspeak` for display.
- Moras or phonemes rounded to zero frames have `start === end` and are never
  current.
- Options (`frameRate`, `interrogativeUpspeak`) go to `timingFromVoicevox`;
  `lipSyncFromVoicevox(timing, options)` throws.

`examples/shorts-explainer/voice.ts` uses this for its lit mora panel and its
in-line face changes.
````

- [ ] **Step 4: Update the website chapter**

In `packages/website/src/pages/docs/content/packages.tsx`, replace the `@celesta/voicevox` `Api` rows with:

```tsx
    <Api caption="@celesta/voicevox" rows={[
      [<code>lipSyncFromVoicevox(query, options?)</code>, text('docs.chapters.voicevox.api.voicevox-convert')],
      [<code>lipSyncFromVoicevox(timing)</code>, text('docs.chapters.voicevox.api.voicevox-convert-timing')],
      [<code>timingFromVoicevox(query, options?)</code>, text('docs.chapters.voicevox.api.voicevox-timing')],
      [<code>voicevoxVowelShape(vowel)</code>, text('docs.chapters.voicevox.api.voicevox-vowel')],
    ]} />
```

In `en.json` under `docs.chapters.voicevox`:

- `metadata.description`: `"Mora timing and lip sync from the AudioQuery used to synthesize a voice."`
- `metadata.keywords`: append ` timingFromVoicevox mora timing karaoke pause`
- `intro`: `"<slot0/> reads the AudioQuery used by VOICEVOX synthesis: where each mora, phoneme, and pause falls in the WAV, and a LipSyncTrack built from that timing. Dialogue components, WAV-based lip sync, and LipSyncTrack are in <slot1/>. Import the adapter from its own package; @celesta/character does not re-export it."`
- add `api.voicevox-timing`: `"Seconds from the start of the WAV for every mora, phoneme, pause, and accent phrase, shaped like the query (accentPhrases[i].moras[j] is accent_phrases[i].moras[j]). Use it to light moras as they are spoken or change a face at a pause."`
- add `api.voicevox-convert-timing`: `"The same track from a timing already computed, so the mouth and the timing cannot disagree. Pass options to timingFromVoicevox instead."`
- `types`: `"Exports also include VoicevoxAudioQuery, VoicevoxAccentPhrase, VoicevoxMora, VoicevoxTiming, VoicevoxTimedAccentPhrase, VoicevoxTimedMora, VoicevoxTimedPause, VoicevoxTimedPhoneme, VoicevoxSpan, VoicevoxTimingOptions, and VoicevoxLipSyncOptions. LipSyncTrack is imported from @celesta/character."`

In `ja.json` under the same keys:

- `metadata.description`: `"音声合成に使った AudioQuery から、モーラの時刻と口パクを作る。"`
- `metadata.keywords`: append ` モーラ 時刻 カラオケ 間 timingFromVoicevox`
- `intro`: `"<slot0/> は、VOICEVOX の音声合成に使った AudioQuery から、各モーラ・音素・間が WAV のどこにあるかを求め、その時刻から LipSyncTrack を作ります。会話のコンポーネント、WAV から作る口パク、LipSyncTrack は <slot1/> にあります。変換機能は独立したパッケージから読み込んでください。@celesta/character からは再エクスポートされません。"`
- add `api.voicevox-timing`: `"すべてのモーラ・音素・間・アクセント句について、WAV の開始からの秒数を返します。形はクエリと同じで、accentPhrases[i].moras[j] が accent_phrases[i].moras[j] に対応します。声に合わせてモーラを点灯させたり、間で表情を変えたりするのに使います。"`
- add `api.voicevox-convert-timing`: `"計算済みの時刻から同じトラックを作ります。口と時刻が食い違いません。オプションは timingFromVoicevox に渡してください。"`
- `types`: `"VoicevoxAudioQuery、VoicevoxAccentPhrase、VoicevoxMora、VoicevoxTiming、VoicevoxTimedAccentPhrase、VoicevoxTimedMora、VoicevoxTimedPause、VoicevoxTimedPhoneme、VoicevoxSpan、VoicevoxTimingOptions、VoicevoxLipSyncOptions もエクスポートされます。LipSyncTrack は @celesta/character から読み込みます。"`

Keep the key order of the existing `api` object (insert `voicevox-convert-timing` after `voicevox-convert`, `voicevox-timing` after it).

- [ ] **Step 5: Run the website tests**

Run: `pnpm --dir packages/website run test`
Expected: PASS (locale key parity, docs nav, routes).

Then type-check the page: `pnpm --dir packages/website exec tsc --noEmit`
Expected: no errors. (If it fails only because `wrangler types` has not generated its declarations, run `pnpm --dir packages/website exec wrangler types` first.)

- [ ] **Step 6: Commit**

```bash
git add packages/voicevox/README.md skills/celesta packages/website/src
git commit -m "docs(voicevox): document timingFromVoicevox (#226)"
```

---

### Task 5: Final verification

- [ ] **Step 1: Full build and tests**

```bash
pnpm --filter "@celesta/cli..." run build
pnpm run test:runtime
pnpm --dir packages/website run test
```

Expected: all pass. `test:runtime` covers `packages/voicevox` (unit and packaged-runtime tests) and `scripts/test-package.mjs`'s VOICEVOX import.

- [ ] **Step 2: Check the public surface**

```bash
grep -n "export" packages/voicevox/dist/index.d.ts
```

Expected: `lipSyncFromVoicevox`, `voicevoxVowelShape`, `timingFromVoicevox`, and the types `VoicevoxLipSyncOptions`, `VoicevoxAccentPhrase`, `VoicevoxAudioQuery`, `VoicevoxMora`, `VoicevoxSpan`, `VoicevoxTimedAccentPhrase`, `VoicevoxTimedMora`, `VoicevoxTimedPause`, `VoicevoxTimedPhoneme`, `VoicevoxTiming`, `VoicevoxTimingOptions`.

- [ ] **Step 3: Note for the PR**

The PR description states: the baseline comparison result from Task 2 Step 6, whether the example was rendered with a real engine (Task 3), the AivisSpeech finding (Task 4 Step 1), and that #239 / #178 consumers are out of scope (they read `timing.pauses[i].start` and `timing.accentPhrases`).
