# @celesta/aivis-speech

AivisSpeech AudioQuery timing and lip sync for Celesta React compositions, for
AivisSpeech Engine. Dialogue components, WAV lip sync, and `LipSyncTrack` are
in `@celesta/character`; VOICEVOX Engine voices use `@celesta/voicevox`.

AivisSpeech serves VOICEVOX's API, but its model chooses every phoneme's
length itself: the query's lengths are dummy zeros. So the timing is read from
the query **and the WAV** synthesized from it. The query gives the moras in
order (consonants, ん, っ, and punctuation included); the WAV gives where they
fall. The voice's start and end are exact (the pre/post silence the engine
adds); the times inside it are estimated by aligning the moras to the WAV's
loudness, so pauses land on the silences the model made.

```tsx
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { Dialogue, type LipSyncTrack } from '@celesta/character';
import { lipSyncFromAivisSpeech } from '@celesta/aivis-speech';
import helloQuery from './voices/hello.json';

let hello: LipSyncTrack;
export async function prepare() {
  const wav = await readFile(join(import.meta.dirname, 'voices/hello.wav'));
  hello = lipSyncFromAivisSpeech(helloQuery, wav);
}
// Inside the sequence that starts the voice:
<Dialogue character={view} audio="./voices/hello.wav" lipSync={hello}>
  こんにちは、今日はいい天気ですね。
</Dialogue>;
```

Use the query that produced the WAV: its `speedScale`, `prePhonemeLength`, and
`postPhonemeLength` locate the voice inside the WAV. Pass the WAV's bytes (an
uncompressed WAV, as AivisSpeech returns) or `decodeWav`'s result.

```sh
# Save the query, then synthesize the WAV from that same query.
curl -s -X POST -G "http://127.0.0.1:10101/audio_query" --data-urlencode "speaker=888753760" \
  --data-urlencode "text=こんにちは、今日はいい天気ですね。" -o voices/hello.json
curl -s -X POST "http://127.0.0.1:10101/synthesis?speaker=888753760" \
  -H "Content-Type: application/json" -d @voices/hello.json -o voices/hello.wav
```

## Mora and phoneme timing

`timingFromAivisSpeech(query, wav)` returns where every mora, phoneme, and
pause falls in the WAV, in seconds from its first sample. It is the timing the
lip-sync track is built from, so lit moras or a face change at a pause stay in
step with the mouth. It takes about 10 ms per second of voice; call it
once per line in `prepare()`.

```ts
import { lipSyncFromAivisSpeech, timingFromAivisSpeech } from '@celesta/aivis-speech';

const timing = timingFromAivisSpeech(helloQuery, wav);
const hello = lipSyncFromAivisSpeech(timing);

timing.moras;           // [{ text: 'コ', start, end, consonant, vowel, accentPhrase }, ...]
timing.pauses[0].start; // where the first punctuation mark's pause begins
timing.speech;          // { start, end } without the leading and trailing silence
```

`timing.accentPhrases[i].moras[j]` is `query.accent_phrases[i].moras[j]`.
AivisSpeech keeps punctuation as moras whose vowel is `pau` and whose `text`
is the normalized mark (`,` `.` `!` `?` `…`); they stay in `moras` and are
also listed in `pauses`, together with any `pause_mora`, sentence-final marks
included. A mark the model did not pause at is short or has
`start === end`. Skip punctuation for display with
`m.vowel.phoneme !== 'pau'`.

Mouths follow the same rules as `@celesta/voicevox`: vowels `a i u e o` (and
devoiced `A I U E O`) map to their shapes; `N` (ん), `cl` (っ), pauses and
silence are `closed`; a consonant shows its mora's vowel, except `m`/`b`/`p`,
which close the lips.

Measured against VOICEVOX Engine voices, whose true phoneme times are known
(the queries' lengths removed and pauses turned into `,` moras, as AivisSpeech
phrases them), mora starts were within 13 ms at the median and 44 ms at the
90th percentile, and the mouth matched the true one 86% of the time, where
`loadLipSync` matched 52%.

Exports: `timingFromAivisSpeech`, `lipSyncFromAivisSpeech`,
`aivisSpeechVowelShape`, and the types `AivisSpeechAudioQuery`,
`AivisSpeechAccentPhrase`, `AivisSpeechMora`, `AivisSpeechTiming`,
`AivisSpeechTimedAccentPhrase`, `AivisSpeechTimedMora`,
`AivisSpeechTimedPause`, `AivisSpeechTimedPhoneme`, and `AivisSpeechSpan`.

Building `@celesta/cli` also builds this package and stages its TypeScript
support. Celesta's desktop runtime ships it separately and the CLI resolves it
when a composition imports it. **File > Set Up TypeScript** includes its types
and import mapping, without requiring an install in the composition's project.
The package is private and has not been published to npm.

```sh
pnpm install
pnpm --dir packages/react run codegen
pnpm --filter "@celesta/cli..." run build
pnpm --dir packages/aivis-speech run test
```

`@celesta/character` does not depend on or re-export the adapter.
