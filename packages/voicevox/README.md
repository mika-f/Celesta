# @celesta/voicevox

VOICEVOX AudioQuery timing and lip sync for Celesta React compositions, for
VOICEVOX Engine. Engines that accept the same query but choose their own phoneme
lengths are not timed exactly; for AivisSpeech, use `@celesta/aivis-speech`.
Dialogue components, WAV lip sync, and `LipSyncTrack` are in `@celesta/character`.

```tsx
import { Dialogue } from '@celesta/character';
import { lipSyncFromVoicevox } from '@celesta/voicevox';
import helloQuery from './voices/hello.json';

const hello = lipSyncFromVoicevox(helloQuery);
// Inside the sequence that starts the voice:
<Dialogue character={view} audio="./voices/hello.wav" lipSync={hello}>
  こんにちは、ずんだもんなのだ。
</Dialogue>;
```

Use the query that produced the WAV. The track reflects speed, pauses, and
interrogative upspeak, with phoneme lengths rounded to the engine's frame grid.
Pass `{ frameRate: null }` to use unrounded lengths, or
`{ interrogativeUpspeak: false }` when synthesis disables upspeak.

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

The timing is a `SpeechTiming` from `@celesta/character` with `exact: true`,
the shape `@celesta/aivis-speech` returns too (with `exact: false`), so code
that reads timings takes either engine's. Its mouth rules are
`lipSyncFromSpeechTiming`'s, and `voicevoxVowelShape` is `speechVowelShape`.

Exports: `timingFromVoicevox`, `lipSyncFromVoicevox`, `voicevoxVowelShape`,
and the types `VoicevoxAudioQuery`, `VoicevoxAccentPhrase`, `VoicevoxMora`,
`VoicevoxTiming`, `VoicevoxTimedAccentPhrase`, `VoicevoxTimedMora`,
`VoicevoxTimedPause`, `VoicevoxTimedPhoneme`, `VoicevoxSpan`,
`VoicevoxTimingOptions`, and `VoicevoxLipSyncOptions` (the same options).

Building `@celesta/cli` also builds this package and stages its TypeScript
support. Celesta's desktop runtime ships it separately and the CLI resolves it
when a composition imports it. **File > Set Up TypeScript** includes its types
and import mapping, without requiring an install in the composition's project.
The package is private and has not been published to npm.

```sh
pnpm install
pnpm --dir packages/react run codegen
pnpm --filter "@celesta/cli..." run build
pnpm --dir packages/voicevox run test
```

`@celesta/character` does not depend on or re-export the adapter.
