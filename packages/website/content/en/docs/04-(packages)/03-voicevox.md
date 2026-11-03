`@celesta.studio/voicevox` reads the AudioQuery used by VOICEVOX synthesis: where each mora, phoneme, and pause falls in the WAV, and a LipSyncTrack built from that timing. It can also synthesize a whole script with a local VOICEVOX Engine and cache the voices. Dialogue components, WAV-based lip sync, and LipSyncTrack are in `@celesta.studio/character`. Import the adapter from its own package; @celesta.studio/character does not re-export it.

The desktop app and CLI bundle this package separately. Building the runtime builds it too, and File → Set Up TypeScript includes its declarations and import mapping. It is private and is not published to npm. The browser playground does not support it.

## Drive a dialogue line from its synthesis query

Use the exact AudioQuery that produced the WAV, including speed and pause settings. Pass the track to Dialogue or CharacterView as lipSync, or read it with useLipSync(). These functions do not synthesize audio or call a VOICEVOX server. Save that AudioQuery as voicevox-query.json beside voicevox.tsx, and use your own voice and mouth images in the example below.

```tsx file=examples/voicevox.tsx title="voicevox.tsx"
```

## Synthesize a script

Call prepareNarration(lines, options) in prepare() to have a running local VOICEVOX Engine read each line. It returns the lines with audio, lipSync, and narration set, ready for planDialogue(). Celesta never installs or starts the engine; start it yourself, for example with `docker run --rm -p 127.0.0.1:50021:50021 voicevox/voicevox_engine:cpu-ubuntu24.04-latest`.

```tsx
const lines = await prepareNarration([
  { id: 'intro', speaker: 'zunda', text: 'HPが1です。', spokenText: 'エイチピーがいちです。' },
  { id: 'reply', speaker: 'metan', text: '危ないわね。', expression: 'surprised' },
], {
  speakers: {
    zunda: { baseUrl: 'http://127.0.0.1:50021', styleId: 3 },
    metan: { baseUrl: 'http://127.0.0.1:50021', styleId: 2, query: { speedScale: 1.1 } },
  },
  cacheDir: './.celesta/voices',
});
plan = await planDialogue(lines, { fps: 30 });
```

Voices are cached in cacheDir, relative to the entry file. A line is synthesized again only when its spoken text, style, query settings, synthesis flags, or revision change; editing the subtitle while spokenText stays the same, or changing the expression, order, or gaps, reuses the voice. Cached lines are read from disk without the engine, so an unchanged script previews and exports while the engine is off. The cache cannot see changes to the engine’s user dictionary, the engine, or its voice models: after one, change revision or pass refresh. Lines that already have audio are never synthesized. Failures throw a NarrationError naming the line, its speaker, and the stage that failed. The repository’s packages/cli/examples/with-narration.tsx is a runnable two-speaker example.

| Option | Use it for |
| --- | --- |
| `speakers` | Voices by speaker key: `baseUrl`, `styleId` (from the engine’s `GET /speakers`), and optional `query` settings (`speedScale`, `pitchScale`, `pauseLengthScale`, …), `interrogativeUpspeak`, `katakanaEnglish`, and `revision`. A line’s `voice` overrides them for that line. |
| `cacheDir` | Where the WAV and the final query of each line are kept. Defaults to `./.celesta/voices`. |
| `concurrency`, `timeout`, `signal` | Lines synthesized at once (default 2), milliseconds per engine request (default 60000), and an AbortSignal to cancel. |
| `revision`, `refresh` | Invalidate every line, or synthesize again regardless of the cache (`true` or a list of line ids). |

**@celesta.studio/voicevox**

| API | Use it for |
| --- | --- |
| `prepareNarration(lines, options)` | Synthesize and cache a script’s voices, returning lines for planDialogue(). |
| `lipSyncFromVoicevox(query, options?)` | Convert an AudioQuery into a LipSyncTrack, with time zero at the start of the WAV, including leading silence. |
| `lipSyncFromVoicevox(timing)` | The same track from a timing already computed, so the mouth and the timing cannot disagree. Pass options to timingFromVoicevox instead. |
| `timingFromVoicevox(query, options?)` | Seconds from the start of the WAV for every mora, phoneme, pause, and accent phrase, shaped like the query (accentPhrases[i].moras[j] is accent_phrases[i].moras[j]). Use it to light moras as they are spoken or change a face at a pause. |
| `voicevoxVowelShape(vowel)` | Map a phoneme to a/i/u/e/o/closed. Uppercase devoiced vowels keep their shape; N, cl, pau, sil, and unknown values map to closed. |

**Timing options**

| API | Use it for |
| --- | --- |
| `frameRate` | Defaults to 93.75 Hz, the VOICEVOX phoneme frame grid. Set null for unrounded lengths or the engine’s own rate; this is not the composition’s fps. |
| `interrogativeUpspeak` | Defaults to true. Set false when the WAV was synthesized with interrogative upspeak disabled. |

Exports also include NarrationError, NarrationLine, NarratedLine, NarrationAsset, NarrationStage, PrepareNarrationOptions, VoicevoxVoice, VoicevoxVoiceSettings, VoicevoxQuerySettings, VoicevoxAudioQuery, VoicevoxAccentPhrase, VoicevoxMora, VoicevoxTiming, VoicevoxTimedAccentPhrase, VoicevoxTimedMora, VoicevoxTimedPause, VoicevoxTimedPhoneme, VoicevoxSpan, VoicevoxTimingOptions, and VoicevoxLipSyncOptions. LipSyncTrack is imported from @celesta.studio/character.
