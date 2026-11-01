# beep-voice

A sample content plugin with components only: a speech channel. It voices
dialogue lines with game-style beeps, one per character, where
`@celesta/voicevox` would ask a speech engine, and returns the lines ready
for `planDialogue()` with `audio` and `lipSync` set.

```tsx
import { planDialogue } from '@celesta/character';
import { prepareBeepNarration } from '@celesta-plugin/beep-voice';

export async function prepare() {
  const lines = await prepareBeepNarration(
    [{ id: 'hi', speaker: 'shizuku', text: 'こんにちは！' }],
    { speakers: { shizuku: { pitch: 330, speed: 13, waveform: 'square' } } },
  );
  plan = await planDialogue(lines, { fps: 30 });
}
```

| Voice setting | Meaning |
| --- | --- |
| `pitch` | The base pitch in hertz. Default 220. |
| `speed` | Characters spoken each second. Default 12. |
| `volume` | Loudness from 0 to 1. Default 0.3. |
| `waveform` | `square` (buzzy), `triangle` (soft), or `sine` (pure). Default `square`. |

`、` and `。` and the like are pauses, and a line ending in a question rises.
The WAVs are kept in `cacheDir` (default `./.celesta/beep-voice`, from the
entry) under a hash of their text and voice, so each is written once.
[`demo.tsx`](demo.tsx) has two portraits talk in beeps.

## Making a speech channel for another engine

The plugin has no program: its manifest names only a module, which the
runtime bundles into the entry. The module runs in `prepare()` with Node.js,
so it can read and write files, make HTTP requests, or start programs.

[`js/index.ts`](js/index.ts) keeps the engine in one function,
`synthesize(text, voice)`, which returns samples and the times the mouth
opens. For another engine, replace it with a request to that engine (an HTTP
API, or a program started with `node:child_process`) that returns its WAV
and, if the engine reports them, phoneme times; keep the rest:

- validate the script before synthesizing anything;
- cache each WAV under a hash of everything that changes its sound, written
  through a temporary file;
- build the lip sync with `lipSyncFromKeyframes()` from the engine's timing,
  or with `loadLipSync({ src, text })` from the WAV when it reports none;
- return the lines with `audio` and `lipSync`, every other field as given.

Synthesis must give the same sound for the same input, or exports change
between runs; key the cache on anything else that does (an engine version,
a user dictionary).
