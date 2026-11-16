`@celesta-plugin/aivis-speech` reads an AivisSpeech AudioQuery together with the WAV synthesized from it: where each mora, phoneme, and pause falls in the WAV, and a LipSyncTrack built from that timing. Dialogue components and LipSyncTrack are in `@celesta.studio/character`; VOICEVOX voices use [@celesta-plugin/voicevox](/en/docs/voicevox/).

AivisSpeech serves the VOICEVOX API, but its model chooses every phoneme length itself, so the query’s lengths are dummy zeros. The query gives the moras in order, including ん, っ, and punctuation; the WAV gives where they fall. The voice’s start and end are exact; the times inside it are estimated from the WAV’s loudness, so pauses land on the silences the model made.

> [!NOTE]
> **Add the plugin**
> `@celesta-plugin/aivis-speech` is a first-party plugin from Celesta's [plugin registry](/en/docs/plugins/#registry-plugins). Add it to the project with the `celesta` command, which declares it in `celesta.config.json`, locks it in `celesta.lock`, and downloads it:
>
> ```sh
> celesta plugin add aivis-speech
> ```
>
> The plugin's versions follow Celesta's. On a pre-release of Celesta, name that version, as in `celesta plugin add aivis-speech@0.10.0-alpha.4`: a requirement without a pre-release matches none. The desktop app reads the `celesta.lock` of a project you trust (saved trust, with `celesta.config.json`) and runs the locked versions; when you open such a project and its locked plugins are not downloaded, it offers to download them.
>
> A composition that imports `@celesta.studio/aivis-speech` keeps working: Celesta still includes that package, and warns with the import to use and the `celesta plugin add` command to run.

The browser playground does not support it.

## Drive a dialogue line from its query and WAV

Use the AudioQuery that produced the WAV: its speedScale, prePhonemeLength, and postPhonemeLength locate the voice in the WAV. Read the WAV in prepare() and pass its bytes. The package does not synthesize audio or call an AivisSpeech server. Save the AudioQuery as aivis-speech-query.json beside aivis-speech.tsx, and use your own voice and mouth images in the example below.

```tsx file=examples/aivis-speech.tsx title="aivis-speech.tsx"
```

**@celesta-plugin/aivis-speech**

| API | Use it for |
| --- | --- |
| `lipSyncFromAivisSpeech(query, wav)` | Convert an AudioQuery and its WAV (bytes or the result of decodeWav) into a LipSyncTrack, with time zero at the start of the WAV, including leading silence. |
| `lipSyncFromAivisSpeech(timing)` | The same track from a timing already computed, so the mouth and the timing cannot disagree. |
| `timingFromAivisSpeech(query, wav)` | Seconds from the start of the WAV for every mora, phoneme, pause, and accent phrase, shaped like the query (accentPhrases[i].moras[j] is accent_phrases[i].moras[j]). Use it to light moras as they are spoken or change a face at a pause. |

AivisSpeech keeps punctuation as moras whose vowel is pau and whose text is the normalized mark (, . ! ? …). They stay in moras and are also listed in pauses, sentence-final marks included; a mark the model did not pause at is short or empty.

Both engines’ timings are a `SpeechTiming` from `@celesta.studio/character`: exact: true from timingFromVoicevox, exact: false from timingFromAivisSpeech. Code that reads timings, such as lit moras or a face change at a pause, takes either, and `lipSyncFromSpeechTiming` builds the mouth from any of them.

Against VOICEVOX voices with known phoneme times (138 lines, six voices, three speeds), mora starts were within 14 ms at the median and 71 ms at the 90th percentile, and the mouth matched 83% of the time, against 51% for loadLipSync. Pauses anchor the estimate; in the middle of a long clause voiced without a break, errors grow. For VOICEVOX voices use timingFromVoicevox, which is exact.

Exports also include AivisSpeechTiming, AivisSpeechAudioQuery, AivisSpeechAccentPhrase, and AivisSpeechMora. SpeechTiming, its parts, speechVowelShape, and LipSyncTrack are imported from @celesta.studio/character.
