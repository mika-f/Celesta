# `timingFromVoicevox` design (issue #226)

## Goal

`lipSyncFromVoicevox()` turns a VOICEVOX AudioQuery into a mouth track that
matches the synthesized WAV to the sample. The times it computes on the way
(where each mora, phoneme, and pause sits in the WAV) are not exposed, so
`examples/shorts-explainer/voice.ts` copies the arithmetic in `timelineOf()`
to light moras as they are spoken and to change the face at a pause. The copy
already drifts from the original:

- it rounds with `Math.round`, the engine and `lipSyncFromVoicevox` round half
  to even;
- it ignores the 0.15 s interrogative upspeak mora;
- any change to the engine's timing has to be made twice.

`timingFromVoicevox(query, options?)` returns that timing as data, and
`lipSyncFromVoicevox` is rebuilt on top of it so the two cannot disagree.

```ts
import { lipSyncFromVoicevox, timingFromVoicevox } from '@celesta/voicevox';

const timing = timingFromVoicevox(query);
const lipSync = lipSyncFromVoicevox(timing);

timing.moras;            // [{ text: 'コ', start, end, consonant, vowel, accentPhrase: 0, upspeak: false }, ...]
timing.pauses[0].start;  // where the first 、 begins, e.g. to change the face (#239)
```

Consumers in later work: karaoke-style subtitles and explainers (the
shorts-explainer panel), effects timed to a mora, #178's reading-based subtitle
page timing, and #239's in-line expression changes. Those consumers are out of
scope here; they take plain seconds from this API.

## Engine behavior this mirrors

From `_query_to_decoder_feature` and its helpers in voicevox_engine's
`tts_pipeline/tts_engine.py`:

1. With upspeak enabled, an accent phrase marked `is_interrogative` whose last
   mora has `pitch > 0` gets one more mora: no consonant, the last mora's
   vowel, `vowel_length` 0.15, and `text` set to that vowel's kana
   (`mora_phonemes_to_mora_kana[vowel]`).
2. Moras are flattened, each accent phrase followed by its `pause_mora`.
3. A `sil` mora of `prePhonemeLength` is prepended and one of
   `postPhonemeLength` appended.
4. `pauseLength` (when not null) replaces, then `pauseLengthScale` multiplies,
   the length of every `pau` mora. The `sil` moras are not pauses.
5. `speedScale` divides every length.
6. Each phoneme's length is rounded to the 93.75 Hz frame grid with numpy's
   half-to-even `np.round`. A mora's length is the sum of its rounded
   phonemes, not the rounded sum.

The current `lipSyncFromVoicevox` already follows 1–6 except the upspeak
mora's `text`, which it never needed.

## API

All times are seconds from the WAV's first sample, named `start` / `end`.
`from` is not used because everywhere else in Celesta `from` is a frame
(`<Sequence from>`, `PlannedDialogueLine.from`, `SeriesTiming`).

```ts
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
  /** Every phoneme the engine synthesizes, `sil` and `pau` included; together they cover [0, durationInSeconds]. */
  readonly phonemes: readonly VoicevoxTimedPhoneme[];
}

export interface VoicevoxTimingOptions {
  /** Same as today's `VoicevoxLipSyncOptions.interrogativeUpspeak`. Default true. */
  interrogativeUpspeak?: boolean;
  /** Same as today's `VoicevoxLipSyncOptions.frameRate`. Default 93.75; null keeps unrounded lengths. */
  frameRate?: number | null;
}
/** Alias kept for existing imports. */
export type VoicevoxLipSyncOptions = VoicevoxTimingOptions;

export function timingFromVoicevox(query: VoicevoxAudioQuery, options?: VoicevoxTimingOptions): VoicevoxTiming;

export function lipSyncFromVoicevox(query: VoicevoxAudioQuery, options?: VoicevoxTimingOptions): LipSyncTrack;
export function lipSyncFromVoicevox(timing: VoicevoxTiming): LipSyncTrack;
```

`VoicevoxLipSyncOptions` stays as a plain alias (no deprecation warning); the
docs name `VoicevoxTimingOptions`.

### Shape decisions

- **Mirrors the query.** `timing.accentPhrases[i].moras[j]` corresponds to
  `query.accent_phrases[i].moras[j]`, so fields this API does not copy
  (`pitch`, `accent`) are read from the query at the same index, and #178 can
  map reading segments to display text by position. The upspeak mora is
  appended after the phrase's own moras and flagged, so it never shifts an
  index.
- **`consonant` / `vowel` instead of `phonemes[]`.** A VOICEVOX mora is at
  most one consonant and one vowel; the fields match `VoicevoxMora`.
  `consonant` is non-null exactly when the query mora's `consonant` is a
  non-empty string, like the engine's `if mora.consonant`; a null
  `consonant_length` then gives it zero length.
- **Silence is always listed.** `phonemes` always starts with the `sil` of
  `prePhonemeLength` and ends with the `sil` of `postPhonemeLength`, even when
  they are zero-length, as the engine always adds both moras.
- **Flat views are the same objects.** `moras`, `pauses`, and `phonemes` are
  for the common loops (light the current mora, find the n-th pause, build the
  mouth track) and cost one array each.
- **Zero-length entries stay.** A phoneme whose rounded length is 0 frames
  keeps `start === end`, so indexes still match the query. Consumers testing
  "is this mora current" with `start <= t && t < end` naturally skip it.
- **The result is plain data.** No methods; it can be computed in `prepare()`
  or at module scope and read per frame.

### Arithmetic

With a `frameRate`, lengths are converted to whole frames per phoneme (half to
even), accumulated as integers, and converted to seconds once per boundary
(`frames / frameRate`). Adjacent spans therefore share the exact same number
(`moras[i].end === moras[i + 1].start` when no pause is between them), and
`durationInSeconds` is `totalFrames / frameRate`. With `frameRate: null`,
unrounded seconds are accumulated directly.

Each phoneme's frame count is `roundHalfToEven((length / speedScale) * frameRate)`,
in that order, as the engine does and as today's code does, so values that
land exactly on .5 round the same way. Pause lengths are
`(pauseLength ?? pause_mora.vowel_length) * pauseLengthScale` before that.

Accumulating integers can move a boundary by a few ulps compared with today's
`cursor += seconds`; frame counts and durations do not change, and the
existing tests sample midpoints or compare with a 1e-9 tolerance.

### Validation

Same rules and messages as today, with the prefix `timingFromVoicevox:`:
`speedScale` must be a positive finite number, `frameRate` positive finite or
null, every length non-negative and finite. `lipSyncFromVoicevox(query)`
reports errors through `timingFromVoicevox`, so the prefix of its messages
changes; the existing tests match on the field name only.

### `lipSyncFromVoicevox`

Both overloads go through one function that walks `timing.phonemes`:

- `sil` and `pau` → `closed`;
- a vowel → `voicevoxVowelShape(vowel)`;
- a consonant → `closed` if bilabial (`m my b by p py`), otherwise its
  mora's vowel shape (anticipation), as today;
- phonemes with `end <= start` emit no keyframe, as today, so
  `lipSyncFromKeyframes`'s "later keyframe at the same time wins" rule sees
  the same keyframes as before.

`lipSyncFromVoicevox(query, options)` is `lipSyncFromVoicevox(timingFromVoicevox(query, options))`.
The overloads are told apart by `'accent_phrases' in input`. Passing options
together with a timing throws (`lipSyncFromVoicevox: options apply when the
timing is computed; pass them to timingFromVoicevox`) instead of silently
ignoring them.

## Not included

- **An engine-neutral `SpeechTiming` in `@celesta/character`.** VOICEVOX is
  the only producer; #178 and #239 consume seconds, not this type; the
  engine-neutral seam that exists, `lipSyncFromKeyframes`, already covers the
  mouth. Extract a shared type when a second engine or an aligner needs it.
- **`clauses`** (accent phrases grouped between pauses). #239 needs only
  `pauses[i].start`; add a derived view when its implementation settles the
  shape.
- **A lookup helper** (`moraAt(seconds)`). A loop over `moras` is enough.
- **Mouth shapes on moras.** That is the lip-sync mapping;
  `voicevoxVowelShape(mora.vowel.phoneme)` gives the vowel's shape.

## Files

`packages/voicevox/src`:

- `query.ts` — `VoicevoxAudioQuery`, `VoicevoxAccentPhrase`, `VoicevoxMora`
  (moved from `lipsync.ts`; `VoicevoxMora`'s comment no longer says "only the
  fields lip sync reads").
- `timing.ts` — `timingFromVoicevox`, the span types, `VoicevoxTimingOptions`,
  half-to-even rounding, the upspeak kana table (`a i u e o N` →
  `ア イ ウ エ オ ン`, checked against `mora_phonemes_to_mora_kana` in
  voicevox_engine's `tts_pipeline/mora_mapping.py`). Devoiced vowels, `cl`,
  and `pau` have pitch 0 and never qualify; if an edited query gives one a
  positive pitch, the engine's lookup would fail, so here the mora's `text`
  falls back to the vowel string instead of throwing.
- `lipsync.ts` — `lipSyncFromVoicevox` (both overloads), `voicevoxVowelShape`,
  the bilabial set.
- `index.ts` — exports `timingFromVoicevox`, `lipSyncFromVoicevox`,
  `voicevoxVowelShape`, and every type above, plus the
  `VoicevoxLipSyncOptions` alias.

Elsewhere:

- `examples/shorts-explainer/voice.ts` — delete `timelineOf()`; build `moras`
  and `faces` from `timingFromVoicevox(query)` and pass the timing to
  `lipSyncFromVoicevox`. `Mora` keeps its `mouth` via `voicevoxVowelShape`.
  `scenes/VoiceScene.tsx` reads `start`/`end` instead of `from`/`to`.
- `packages/voicevox/README.md` — a timing section and the export list.
- `skills/celesta/SKILL.md` — the `@celesta/voicevox` row of the export table.
- `skills/celesta/references/dialogue.md` — a "Mora and phoneme timing"
  subsection after "Lip sync from VOICEVOX timing", with the shorts-explainer
  pattern (light the current mora; change the face at a pause).
- `packages/website/src/pages/docs/content/packages.tsx` and
  `src/locales/{en,ja}.json` — API table rows for `timingFromVoicevox` and the
  timing overload.

## Testing

`packages/voicevox/test/timing.test.mjs` (built `dist`, `node --test`, the
existing fixtures):

- `durationInSeconds` equals each fixture WAV's sample count over its rate,
  and equals `lipSyncFromVoicevox(query).durationInSeconds`.
- `phonemes` cover [0, duration]: the first starts at 0, each starts where the
  previous ends, the last ends at `durationInSeconds`.
- Index alignment: `accentPhrases.length` and each phrase's non-upspeak mora
  count match the query; `text` matches; `moras`/`pauses` are the same objects
  as the nested ones.
- Hand-computed times for the Zundamon fixture with `frameRate: null`
  (コ starts at 0.1, the 、 pause, モ's consonant).
- Half-to-even per phoneme (1.5 → 2, 4.5 → 4 frames) and a mora length equal
  to the sum of its rounded phonemes.
- Upspeak: appended with `upspeak: true`, `text` 'ア' for an `a` vowel, 0.15 s,
  absent with `interrogativeUpspeak: false` and when the last mora's pitch is 0.
- `pauseLength`, `pauseLengthScale`, `speedScale` move pauses and moras as the
  engine does; `sil` is unaffected by pause settings.
- Zero-length phonemes keep `start === end`.
- `speech` spans the first mora's start to the last mora's end.
- Validation errors for `speedScale`, `frameRate`, negative lengths.
- A consonant with a null `consonant_length` is listed with zero length.

`lipsync.test.mjs`: the existing tests pass unchanged; add one that samples
`lipSyncFromVoicevox(query)` and `lipSyncFromVoicevox(timingFromVoicevox(query))`
every 1 ms over each fixture and asserts identical shapes, and one that
`lipSyncFromVoicevox(timing, options)` throws.

`runtime.test.mjs`: the composition it writes also imports
`timingFromVoicevox`, so the packaged runtime and its `.d.ts` are checked.

The shorts-explainer example is checked by rendering frames of the voice and
PSD scenes before and after the change; differences are limited to the fixed
rounding and upspeak.

## Open question to verify during implementation

The README says engines sharing the API (AivisSpeech, COEIROINK v2) return the
same shape. If such an engine does not honor the query's phoneme lengths when
synthesizing, neither the mouth track nor the timing matches its WAV. Check
before keeping that claim; if it does not hold, the README states which
engines the timing is exact for.
