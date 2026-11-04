# Character dialogue, portraits, and lip sync

The components and helpers here are imported from `@celesta.studio/character`; the
core layers (`Composition`, `Assets`, `Sequence`, …) from `@celesta.studio/react`.
Voice, portrait, and
PSD paths are relative to the entry file (JSON: asset ids declared in
`assets`).

Dialogue scenes combine three pieces:

- a **character**: a portrait (one image per expression, or a layered PSD),
  optional lip-sync mouths, and the character's subtitle style;
- a **view** that places the portrait on screen (React only; JSON places the
  portrait from the character definition);
- **lines**: a subtitle, an optional voice recording, and optional
  expression or mouth changes while the line is active.

## Contents

- [React: a two-line conversation](#react-a-two-line-conversation)
- [Timing a script from its voices](#timing-a-script-from-its-voices), with
  [spoken text](#spoken-text-and-subtitle-text) apart from the subtitle
- [Importing a voice tool's export folder](#importing-a-voice-tools-export-folder)
- [Matching voice levels and ducking background audio](#matching-voice-levels-and-ducking-background-audio)
- [Synthesizing a script with VOICEVOX](#synthesizing-a-script-with-voicevox)
- [Changing the face within a line](#changing-the-face-within-a-line)
- [Drawing the subtitle yourself](#drawing-the-subtitle-yourself)
- [Subtitle pages and emphasis](#subtitle-pages-and-emphasis): subtitlePages, emphasis, paginateSubtitles
- [React props](#react-props)
- [Automatic lip sync](#automatic-lip-sync)
- [PSD portraits](#psd-portraits)
- [Blinking](#blinking)
- [JSON projects](#json-projects)
- [Troubleshooting](#troubleshooting)

## React: a two-line conversation

```tsx
import * as React from 'react';
import { Assets, Composition, Rect, Sequence } from '@celesta.studio/react';
import { Character, CharacterView, Dialogue } from '@celesta.studio/character';
import type { AssetReference } from '@celesta.studio/react';
import type { CharacterViewReference } from '@celesta.studio/character';

const mira = React.createRef<AssetReference>();          // the character
const miraView = React.createRef<CharacterViewReference>(); // where it is drawn

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={180}>
      <Rect width={1920} height={1080} fill="#20243a" />
      <Assets>
        <Character
          ref={mira}
          name="Mira"
          portrait={{
            defaultExpression: 'calm',
            expressions: { calm: './mira/calm.png', smile: './mira/smile.png' },
          }}
          subtitle={{
            x: 960, y: 940, anchorX: 0.5, anchorY: 0.5, maxWidth: 1600,
            style: {
              fontSize: 56, align: 'center',
              fill: { type: 'solid', color: '#ffffff' },
              stroke: { paint: { type: 'solid', color: '#3a2d52' }, width: 6 },
            },
          }}
        />
      </Assets>

      <CharacterView ref={miraView} character={mira} x={1400} y={200} />

      <Sequence durationInFrames={90}>
        <Dialogue character={miraView} audio="./voices/line-01.wav">
          Welcome back. Shall we begin?
        </Dialogue>
      </Sequence>
      <Sequence from={90} durationInFrames={90}>
        <Dialogue character={miraView} expression="smile" audio="./voices/line-02.wav">
          Every scene starts with a single line.
        </Dialogue>
      </Sequence>
    </Composition>
  );
}
```

Key points:

- `<Character>` goes inside `<Assets>` and is referenced through its ref.
- `<CharacterView>` draws the portrait for the whole time it is rendered, in
  its `defaultExpression` unless a line overrides it.
- `<Dialogue character={viewRef}>` points at the **view**, not the
  character, so the portrait stays put while lines change. Its children are
  the subtitle text (strings only).
- Time each line with a `<Sequence>`. While active, the line shows its
  subtitle, plays `audio`, and applies `expression`/`mouth`/`lipSync` to its
  view; afterwards the view returns to its defaults.
- For two speakers, declare two characters and two views.
- A script of many lines is easiest as data:

```tsx
const lines = [
  { at: 0,   len: 90, text: 'Welcome back.', voice: './voices/01.wav' },
  { at: 90,  len: 75, text: 'Shall we begin?', voice: './voices/02.wav', expression: 'smile' },
];
// inside the Composition:
{lines.map((l, i) => (
  <Sequence key={i} from={l.at} durationInFrames={l.len}>
    <Dialogue character={miraView} audio={l.voice} expression={l.expression}>{l.text}</Dialogue>
  </Sequence>
))}
```

To fit lines to their recordings, let `planDialogue()` measure them (next
section) instead of writing `at`/`len` by hand.

## Timing a script from its voices

`planDialogue(lines, options)` measures every line's voice in `prepare()` and
places the lines back to back: each starts after the previous line and its
gap. `<DialogueSeries>` then renders the plan as `<Sequence>` + `<Dialogue>`
pairs, and the plan tells the rest of the video when each line starts.

```tsx
import * as React from 'react';
import { Composition, Rect, Sequence } from '@celesta.studio/react';
import { CharacterView, DialogueSeries, planDialogue } from '@celesta.studio/character';
import type { AssetReference } from '@celesta.studio/react';
import type { CharacterViewReference, DialoguePlan } from '@celesta.studio/character';

const zunda = React.createRef<AssetReference>();
const metan = React.createRef<AssetReference>();
const zundaView = React.createRef<CharacterViewReference>();
const metanView = React.createRef<CharacterViewReference>();

let plan: DialoguePlan;
export async function prepare() {
  plan = await planDialogue(
    [
      { id: 'hello', scene: 'intro', speaker: 'zunda', audio: './voices/01.wav', text: 'ずんだもんなのだ。' },
      { id: 'topic', scene: 'intro', speaker: 'metan', audio: './voices/02.wav', text: '今日は音声合成の話よ。', expression: 'smile' },
      { id: 'how',   scene: 'body',  speaker: 'zunda', audio: './voices/03.wav', text: 'どうやって喋っているのだ？', gap: 0.6 },
    ],
    { fps: 30, sceneLeadIn: 1 },  // a 1 s pause before each new scene
  );
}

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={plan.durationInFrames}>
      {/* …<Assets> with the two <Character>s… */}
      <Sequence {...plan.scene('intro')}><Rect width={1920} height={1080} fill="#20243a" /></Sequence>
      <Sequence {...plan.scene('body')}><Rect width={1920} height={1080} fill="#2f3b2a" /></Sequence>
      <CharacterView ref={zundaView} character={zunda} x={1400} y={200} />
      <CharacterView ref={metanView} character={metan} x={100} y={200} />
      <DialogueSeries plan={plan} views={{ zunda: zundaView, metan: metanView }} />
    </Composition>
  );
}
```

Each line:

| Field | Notes |
| --- | --- |
| `text` | Subtitle; may hold [`<Span>`](text.md#rich-text-with-span), as `<Dialogue>` children may. |
| `audio` | Voice file, relative to the entry file. Its length (rounded up to whole frames) is the line's length. |
| `durationInFrames` | Explicit length instead of measuring; required for a line without `audio`. |
| `id` | Name for looking the line up; defaults to its index (`"0"`, `"1"`, …). Unique. |
| `gap` | Seconds of silence after the line; defaults to the plan's `gap` (0.25 s). |
| `leadIn` | Seconds of silence before the line. Defaults to `sceneLeadIn` on the first line of a new `scene` (not the script's first line), else 0. |
| `scene` | Groups consecutive lines into a scene. |
| `speaker` / `character` | Who speaks: a key of `<DialogueSeries views>`, or the `<CharacterView>` ref itself. |
| `expression` | The speaker's face: a name, or `{ at, expression }` cues to change it partway through ([below](#changing-the-face-within-a-line)). |
| `mouth`, `lipSync`, `volume`, `muted` | Passed to the line's `<Dialogue>`. |

Lines may carry extra fields of your own; the plan keeps the original line
as `planned.line`.

Options: `fps` (required), `gap` (default seconds after each line, 0.25),
`sceneLeadIn` (default seconds before each new scene, 0). Seconds are
rounded to whole frames.

The plan:

- `plan.durationInFrames`: the total, including the last gap. Use it as the
  `<Composition durationInFrames>`.
- `plan.lines`: `{ id, index, line, from, durationInFrames, gapInFrames, leadInFrames, spanInFrames }`
  per line; `spanInFrames` is the line plus its gap.
- `plan.startOf(id)`: the frame a line starts on, e.g. a camera move
  `interpolate(frame, [plan.startOf('how'), plan.startOf('how') + 20], …)`.
- `plan.range(firstId, lastId?)`: `{ from, durationInFrames }` from one line's
  start to the end of another's gap, ready to spread onto a `<Sequence>`.
- `plan.scenes` / `plan.scene(id)`: each scene's `{ id, from, durationInFrames, lines }`.
  A scene starts at its first line's lead-in and lasts until the next scene
  starts (the last one until the end), so backgrounds cut with no hole.

`<DialogueSeries>` props:

| Prop | Notes |
| --- | --- |
| `plan` | The result of `planDialogue()`. |
| `views` | `{ speaker: viewRef }` for lines that use `speaker`. |
| `holdThroughGap` | Keep each subtitle (and expression) through the gap after it. Default false: the line clears when its voice ends. |
| `holdSubtitle` | Keep a `subtitle.render` band up through the gap after each line (see [Drawing the subtitle yourself](#drawing-the-subtitle-yourself)). Default false. |
| `dialogueProps` | `(planned) => props` merged into each `<Dialogue>`, e.g. to move one subtitle. |

### Spoken text and subtitle text

A line's `text` is what its subtitle shows; `spokenText` is what it says,
when that differs: a reading for `HPが1` (`'えいちぴーがいち'`), or the text
without its `<Span>`s. Speech engines (`prepareNarration()`) read it and
waveform lip sync takes its vowels from it. It is never drawn and never
goes into the `--subtitles` file. Without it the line says its `text`,
which must then be a string. `spokenTextOf(line)` returns it the same way.

For waveform lip sync from local WAVs, call `loadDialogueLipSync(lines)` in
`prepare()` before `planDialogue()`. Each line with an `audio` file and no
`lipSync` of its own gets the track `loadLipSync()` would build from its
WAV and its spoken text:

```tsx
const lines = await loadDialogueLipSync([
  { id: 'danger', speaker: 'zunda', audio: './voices/danger.wav',
    text: 'HPが1です。', spokenText: 'えいちぴーがいちです。' },
]);
plan = await planDialogue(lines, { fps: 30 });
```

- Write `spokenText` in kana or romaji: only those give vowels. A line
  whose text has kanji and no `spokenText`, or whose spoken text has no
  vowels, only opens and closes its mouth, with a warning to `onWarning`
  (default `console.warn`). `hopHz` is passed to the analysis.
- Lines without `audio`, or with a `lipSync`, come back as they are. A
  remote URL or a file that is not PCM/float WAV fails with the line named.
- The voice, its lip sync, and its subtitle stay one line: one clip and one
  mouth clock, however the subtitle is shown.

See `packages/cli/examples/with-spoken-text.tsx`.

A missing voice file fails `prepare()` with
`planDialogue(): voice file for line "how" not found: ./voices/03.wav (looked at …)`.

## Importing a voice tool's export folder

VOICEROID, A.I.VOICE, and AquesTalk front ends (Yukkuri voices) can save each
line as `name.wav` with its text beside it as `name.txt`. `importVoiceFolder()`
turns such a folder into `planDialogue()` lines, so the script lives in the
folder rather than in code:

```tsx
import { importVoiceFolder, planDialogue } from '@celesta.studio/character';
import type { DialoguePlan, VoiceLine } from '@celesta.studio/character';

let plan: DialoguePlan<VoiceLine>;
export async function prepare() {
  const lines = await importVoiceFolder({
    directory: './voices/akane',              // relative to the entry file; may be outside its folder
    speaker: 'akane',                         // every line's speaker (never guessed from the text)
    manifest: './voices/akane/manifest.json', // optional overrides, below
  });
  plan = await planDialogue(lines, { fps: 30 });
}
```

Each line gets `id` (the file name without extension), `text` (the
transcript: line breaks as `\n`, surrounding whitespace trimmed), `audio`,
`speaker`, `lipSync`, `durationInSeconds`, and `source: { wav, txt }`.

- **Order**: natural order of the names (`voice-2` before `voice-10`), the
  same on every OS: digit runs compare by value, other text ignoring case;
  names equal that way fall back to code-unit order. Ids are names, not
  positions, so adding a file does not renumber the others.
- **Pairing**: `.wav`/`.txt` extensions in any case; names pair ignoring case
  (`Hello.wav` + `hello.txt`), and the WAV's spelling is the id. Two WAVs (or
  TXTs) whose names differ only in case fail as ambiguous. Files starting
  with `.` are ignored. Subfolders only with `recursive: true`; their ids are
  `sub/name`.
- **Encoding**: `encoding: 'utf-8'` (default, BOM optional), `'utf-16le'`, or
  `'shift_jis'`, or a function `(bytes, file) => string`. A transcript that
  does not decode cleanly fails with its path; it never shows as `�`.
- **Lip sync** (`lipSync`): `'waveform'` (default) spreads the vowels of the
  line's kana/romaji `spokenText` (else the transcript) over the voiced span.
  Kanji give no vowels: a line with kanji and no `spokenText`, or with no vowels,
  falls back to `'amplitude'` (mouth `a` while loud, closed otherwise) and
  warns. `false` makes no track. This is an approximation, not phoneme timing.
- **Incomplete pairs** (`incomplete`): a WAV without its TXT (or the reverse),
  an empty TXT, or a WAV cut short fail the import with every file named
  (`'error'`, default) or are left out with a warning (`'skip'`). Only
  uncompressed PCM/float WAV is read.
- Warnings go to `onWarning` (default `console.warn`).

`manifest.json` lists lines by `file` (the name without extension); listed
lines come first in the listed order, the rest follow in natural order.
Every field but `file` is optional; unknown keys fail. `spokenText` is the
line's [spoken text](#spoken-text-and-subtitle-text) (the key was `reading`
before; a manifest that still says `reading` fails and says so):

```json
{
  "lines": [
    { "file": "voice-1", "id": "hello", "speaker": "aoi", "text": "表示する字幕", "spokenText": "ひょうじするじまく",
      "scene": "intro", "expression": "smile", "gap": 0.5 },
    { "file": "voice-2", "expression": [{ "at": 1.2, "expression": "smile" }] }
  ]
}
```

In the editor, resaving a WAV or TXT (same name or not), adding, deleting, or
renaming a file, or editing the manifest reloads the preview, also for a folder
outside the entry's directory. A save is picked up once the files stop
changing; while a pair is half written the import fails and the last good
preview stays, recovering when the save completes. Each WAV is used through a
snapshot in Celesta's cache, so export and preview always pair the
transcript with the recording read alongside it. Reload and export always
reread the folder.

Example: `packages/cli/examples/with-voice-folder.tsx`.

## Matching voice levels and ducking background audio

Recordings from different speakers or engines can sit far apart in level.
`matchVoiceLevels(lines, options)` measures each line's local WAV in
`prepare()` and multiplies the line's `volume` by a constant gain toward a
target. It is opt-in: nothing changes unless you call it. Call it before
`planDialogue()` and pass its `lines` on.

```tsx
import { matchVoiceLevels, planDialogue } from '@celesta.studio/character';
import type { VoiceLevelReport } from '@celesta.studio/character';

let plan: DialoguePlan<Line>;
let levels: VoiceLevelReport;
export async function prepare() {
  const matched = await matchVoiceLevels(script, {
    targetRmsDbfs: -20,   // default -20
    peakCeilingDbfs: -1,  // default -1
    maxBoostDb: 12,       // default 12
    maxCutDb: 12,         // default 12
    speakers: { aoi: { offsetDb: -1 }, kotone: { targetRmsDbfs: -22 } },
  });
  levels = matched.report;
  plan = await planDialogue(matched.lines, { fps: 30 });
}
```

The metric (a controllable first step, not perceived loudness):

- Speech RMS: 20 ms blocks, each block's power the mean of its channels'
  mean squares (opposite-polarity stereo does not cancel). Blocks under
  -60 dBFS are silence; of the rest, blocks more than 20 dB under their
  average are dropped (breaths, room tone). The remaining blocks' average
  power in dBFS is the speech RMS. It is not LUFS (no K-weighting).
- Peak: the largest absolute sample over every original channel (sample
  peak, not true peak).
- Less than 0.1 s passing the gates is `'insufficient-speech'`; nothing over
  -60 dBFS is `'silent'`. Both get 0 dB (never extreme amplification).

Gain = `target - rms`, clamped to `maxBoostDb` / `maxCutDb`, then lowered so
the sample peak stays under `peakCeilingDbfs`. The ceiling is the hardest
bound: it cuts past `maxCutDb` when a file already peaks above it. The
target is the speaker's `targetRmsDbfs` (or the global one) plus the
speaker's `offsetDb`.

Per line, set `voiceLevel` on the `DialogueLine`:

| `voiceLevel` | Effect |
| --- | --- |
| omitted | Matched with the speaker's target. |
| `false` | Left exactly as authored; not analyzed. |
| `{ offsetDb: 2 }` | Added to the target. |
| `{ targetRmsDbfs: -24 }` | Replaces the speaker's target and offset for this line. |
| `{ gainDb: -3 }` | Fixed gain; not analyzed, no bounds. |

Muted lines and lines without `audio` are left unchanged. The gain multiplies
a numeric `volume` or every keyframe value of a keyframed one; key times,
easings, same-time cuts, and sequence-local times stay as they were
(`scaleVolume(volume, gain)` does the same for any `AnimatedNumber`). Only
gain changes: the source file, its duration, `lipSync`, and the plan's timing
do not.

`matched.report.lines[i]` has `status` (`'matched'`, `'limited'`, `'silent'`,
`'insufficient-speech'`, `'fixed-gain'`, `'opted-out'`, `'no-audio'`,
`'muted'`), `analysis` (`rmsDbfs`, `peakDbfs`, `speechSeconds`,
`speechStartSeconds`, `speechEndSeconds`, `durationSeconds`, …),
`targetRmsDbfs`, `desiredGainDb`, `gainDb`, `gain`, `limitedBy`
(`'max-boost'`, `'max-cut'`, `'peak-ceiling'`), `rmsAfterDbfs`, and
`peakAfterDbfs`. The before/after values describe the source and the
matching gain only; an authored `volume` above 1, BGM, and any master gain
come on top, so the final mix can exceed the ceiling.

Analysis runs once per file content in `prepare()`, never per frame. It is
cached by the file's SHA-256 and the analysis version: re-saving a WAV at the
same path is measured again; editing text or expressions is not.
`analyzeVoiceLevel(src)` returns the measurement for one file.

Only local uncompressed PCM (8/16/24/32-bit) and IEEE float WAV are read. A
URL, a compressed WAV, or another format fails `prepare()` with
`matchVoiceLevels(): line "x": could not analyze …` and suggests
`voiceLevel: false`.

### Ducking BGM and game audio under speech

Every dialogue voice (`<Dialogue audio>`, and `DialogueSeries` lines) belongs
to the duck group `"voice"`, unless its `duckGroup` names another group, or
is `null` for none. A voice is never ducked itself. So a bed needs one prop:

```tsx
<Audio src="./bgm.wav" volume={0.8} duck={{ under: 'voice' }} />
<Audio src="./game.wav" duck={{ under: 'voice', to: 0.5, attack: 0.1, release: 0.3 }} />
```

The bed dips while a voice clip plays: from the clip's start to where its
file runs out (or its sequence ends), so a subtitle held through the gap
does not count. See [react-core.md](react-core.md#ducking) for `to`,
`attack`, `release`, and `holdGap`.

To follow only the speech `matchVoiceLevels()` found (leading and trailing
silence in a file left out), build the volume in `prepare()` instead:

```tsx
const matched = await matchVoiceLevels(lines, LEVELS);
plan = await planDialogue(matched.lines, { fps: 30 });
bgmVolume = duckingVolume(plan, { volume: 0.8, to: 0.3 }, { levels: matched.report });
// <Audio src="./bgm.wav" volume={bgmVolume} />
```

- `speechIntervals(plan, { levels? })`: seconds on the plan's clock where
  a voice plays. Lines without `audio` and muted lines are skipped. With
  `levels`, each line is trimmed to its analysed speech, and silent lines
  are left out; a line the report did not analyse counts whole. A report
  for other lines throws.
- `duckingVolume(plan, duck, { levels?, origin? })`: the volume over those
  intervals, with the same curve `duck` plays (`duckEnvelope` from
  `@celesta.studio/media-utils`). `duck` takes `volume` (the level with nobody
  speaking, default 1; `to` is relative to it) plus `to`, `attack`,
  `release`, `holdGap`. `origin` is the frame on the plan's clock that the
  `<Audio>`'s sequence starts on, as for `frameKeyframes`.

Runnable: `packages/cli/examples/with-voice-level-matching.tsx`. Both forms
are precomputed curves, not a realtime sidechain compressor, so preview and
export play the same volumes.

Not covered: integrated LUFS / EBU R128 normalization, true peak, master
limiting, realtime sidechain compression, denoising, remote or compressed
audio, and rewriting WAV files.

## Synthesizing a script with VOICEVOX

`prepareNarration(lines, options)` from `@celesta.studio/voicevox` makes the voices
for a script with a local VOICEVOX Engine in `prepare()`, so no WAV paths or
query files are written by hand. It returns the lines with `audio`,
`lipSync`, and `narration` set, in order, ready for `planDialogue()`. The
engine is never installed or started for you; start it first, e.g.
`docker run --rm -p 127.0.0.1:50021:50021 voicevox/voicevox_engine:cpu-ubuntu24.04-latest`.

```tsx
import { planDialogue } from '@celesta.studio/character';
import type { DialoguePlan } from '@celesta.studio/character';
import { prepareNarration } from '@celesta.studio/voicevox';
import type { NarratedLine, NarrationLine } from '@celesta.studio/voicevox';

const SCRIPT: NarrationLine[] = [
  { id: 'intro', speaker: 'zunda', text: 'HPが1です。', spokenText: 'エイチピーがいちです。' },
  { id: 'reply', speaker: 'metan', text: '危ないわね。', expression: 'surprised' },
];

let plan: DialoguePlan<NarratedLine>;
export async function prepare() {
  const lines = await prepareNarration(SCRIPT, {
    speakers: {
      zunda: { baseUrl: 'http://127.0.0.1:50021', styleId: 3 },
      metan: { baseUrl: 'http://127.0.0.1:50021', styleId: 2, query: { speedScale: 1.1 } },
    },
    cacheDir: './.celesta/voices', // the default; relative to the entry file
  });
  plan = await planDialogue(lines, { fps: 30 });
}
// <DialogueSeries plan={plan} views={{ zunda: zundaView, metan: metanView }} />
```

Each line is a `DialogueLine` plus:

| Field | Notes |
| --- | --- |
| `id` | Required and unique; errors name it. |
| `speaker` | Key of `options.speakers` (and of `<DialogueSeries views>`). |
| `spokenText` | What the engine reads, e.g. a reading for `HP` or text without `<Span>`s; the `DialogueLine` field (see [Spoken text and subtitle text](#spoken-text-and-subtitle-text)). Defaults to `text`, which must then be a string. |
| `voice` | Per-line override of the speaker's settings (`styleId`, `query`, `interrogativeUpspeak`, `katakanaEnglish`, `revision`); `query` merges field by field. |

A line that already has `audio` is returned as it is and never synthesized,
so recorded and synthesized lines mix in one script.

A speaker (`VoicevoxVoice`): `baseUrl` (required), `styleId` (from the
engine's `GET /speakers`, `styles[].id`), `query` (`speedScale`,
`pitchScale`, `intonationScale`, `volumeScale`, `prePhonemeLength`,
`postPhonemeLength`, `pauseLength`, `pauseLengthScale`,
`outputSamplingRate`, `outputStereo`, set over the engine's query before
`/synthesis`), `interrogativeUpspeak` (default true; also used for the lip
sync), `katakanaEnglish`, `revision`.

Options: `speakers` (required), `cacheDir` (default `./.celesta/voices`),
`concurrency` (lines synthesized at once, default 2), `timeout` (ms per
engine request, default 60000), `signal` (an `AbortSignal`), `revision`
(part of every line's key), `refresh` (`true` or line ids: synthesize again
even when cached), `frameRate` (lip-sync phoneme grid, as in
`lipSyncFromVoicevox`).

`line.narration`: `audio` and `metadata` (absolute paths of the WAV and its
JSON), `duration` (seconds measured from the WAV), `query` (the full query
that was synthesized, unknown fields included; pass it to
`timingFromVoicevox(query, { interrogativeUpspeak })` for mora times),
`spokenText`, `interrogativeUpspeak`, `key`, `cached`.

**Cache.** A line's key is its spoken text, `styleId`, `query` settings,
`interrogativeUpspeak`, `katakanaEnglish`, the speaker's, line's, and
script's `revision`, and a cache format version. Display `text` (when
`spokenText` is set), expression, order, gaps, scenes, and `baseUrl` are not
in it, so changing them synthesizes nothing; changing one spoken line
synthesizes only that line, and identical lines share one synthesis. A hit
reads files only: **the engine need not be running** to reopen or export an
unchanged script. The cache cannot see the engine's user dictionary, an
engine update, or new voice models; bump `revision` (on the speaker for
one voice, in the options for all) or pass `refresh` after changing them.
Keep `cacheDir` out of version control.

Entries are `<key>.wav` and `<key>.json`, each written to a temporary file
and renamed; the JSON records the WAV's size and hash and is written last.
A missing, mismatched, or undecodable entry is synthesized again, and a
failed or cancelled line leaves no entry.

**Errors.** A failure rejects with a `NarrationError` that names the line,
its speaker, and the `stage` (`config`, `audio_query`, `synthesis`,
`write`), e.g. `prepareNarration(): line "reply" (speaker "metan") failed
at synthesis: http://127.0.0.1:50021/synthesis answered 422: …`. An unknown
speaker or a line without spoken text fails before any request. Lines that
succeeded stay cached, so fixing the cause and reloading redoes only the
failed ones. Nothing is written to stdout.

Limits: VOICEVOX Engine only (for AivisSpeech, synthesize yourself and see
[Lip sync from AivisSpeech](#lip-sync-from-aivisspeech)); the user
dictionary is not synchronized; frames never call the engine, so all
synthesis happens in `prepare()`. `packages/cli/examples/with-narration.tsx`
is a runnable two-speaker example.

## Changing the face within a line

`expression` on a line (or on `<Dialogue>`) can be a list of cues instead of
one name, to change the face partway through without splitting the voice or
the subtitle. `at` is seconds from the start of the line, the clock
`lipSync` uses (time 0 is the first sample of `audio`):

```ts
{ id: 'psd-2', speaker: 'kiritan', audio: './voices/psd-2.wav', text: '…',
  expression: [{ at: 0, expression: 'smile' }, { at: 1.237, expression: 'smug' }] }
```

- Each cue holds until the next; before the first cue (when its `at` is
  above 0) the view shows its own expression.
- The last cue holds to the end of the line, and under
  `<DialogueSeries holdThroughGap>` through the gap after it. With
  `holdSubtitle` alone the face returns to the view's own in the gap, as
  for a single name.
- Cues must be in order of `at` (equal times are allowed; the later one
  wins), with `at` a finite number >= 0; anything else throws, naming the
  cue. An unknown expression name throws as usual.
- A plain string keeps working as before.

Seconds written by hand drift when the script is edited and the voice
regenerated. Time the cues from the voice instead: a clause boundary is
where a pause starts in [`timingFromVoicevox`](#mora-and-phoneme-timing-from-voicevox)
(or [`timingFromAivisSpeech`](#lip-sync-from-aivisspeech)):

```ts
const timing = timingFromVoicevox(query);
const line = {
  id: 'psd-2', speaker: 'kiritan', audio: './voices/psd-2.wav', text: '…',
  lipSync: lipSyncFromVoicevox(timing),
  // 'smile' for the first clause, 'smug' from the pause at 「、」 on.
  expression: [{ at: 0, expression: 'smile' }, { at: timing.pauses[0].start, expression: 'smug' }],
};
```

Other parts that follow the speaker's face (a panel naming the expression,
an effect on a face change) can read it with
`expressionAt(expression, seconds)`, the same lookup `<Dialogue>` does: it
returns the name in effect `seconds` into the line, or `undefined` when the
view's own shows. With a plan, `seconds` is `(frame - planned.from) / fps`.

## Drawing the subtitle yourself

A plain `subtitle` draws only the text. For a band behind it, a name plate,
or anything else, give the subtitle a `render` function. It is called as a
component (hooks work) with the line, and what it returns is placed like the
children of a `<Group>` with the subtitle's other layer props, so draw
around the origin and set `x`/`y` on the subtitle:

```tsx
import { Group, Rect, Text, interpolate, useTextMetrics } from '@celesta.studio/react';
import type { TextStyle } from '@celesta.studio/react';
import type { CharacterSubtitle, SubtitleRenderProps } from '@celesta.studio/character';

const PLATE: TextStyle = { fontSize: 24, fontWeight: 800, fill: { type: 'solid', color: '#ffffff' } };

function Band({ content, character, metrics, style, maxWidth, held, frame, durationInFrames }: SubtitleRenderProps) {
  const plate = useTextMetrics(character.displayName, PLATE);
  const fade = Math.min(8, durationInFrames / 3);
  // Sampled mid-frame, so every frame of the run, even a one-frame run, shows.
  const opacity = interpolate(frame + 0.5, [0, fade, durationInFrames - fade, durationInFrames], [0, 1, 1, 0],
    { extrapolateLeft: 'clamp', extrapolateRight: 'clamp' });
  const width = Math.max(1120, metrics.width + 80);  // the band follows the text
  const height = metrics.height + 48;
  return (
    <Group opacity={opacity}>
      <Rect anchorX={0.5} anchorY={0.5} width={width} height={height} cornerRadius={30}
        fill="#0B0F0CC8" stroke="#7CC242" strokeWidth={4} />
      <Rect x={-width / 2 + 32} y={-height / 2} anchorY={0.5} width={plate.width + 48} height={46}
        cornerRadius={23} fill="#7CC242" />
      <Text x={-width / 2 + 56} y={-height / 2} anchorY={0.5} style={PLATE}>{character.displayName}</Text>
      {held ? null : <Text anchorX={0.5} anchorY={0.5} maxWidth={maxWidth} style={style}>{content}</Text>}
    </Group>
  );
}

const subtitle: CharacterSubtitle = {
  x: 960, y: 962, maxWidth: 1060,
  style: { fontSize: 44, align: 'center', fill: { type: 'solid', color: '#ffffff' } },
  render: (props) => <Band {...props} />,
};
// <Character name="zundamon" displayName="ずんだもん" subtitle={subtitle} … />
```

`render` gets:

| Field | Notes |
| --- | --- |
| `text` | The text shown, as a plain string: the line's, or with [subtitle pages](#subtitle-pages-and-emphasis) the current page's. |
| `content` | `text` as it is drawn, [`<Span>`](text.md#rich-text-with-span)s and `emphasis` included: `<Text style={style}>{content}</Text>` keeps its emphasis. |
| `lineText` | The whole line's text, whichever page shows. |
| `character` | `{ id, name, displayName }` of the speaker. `displayName` is `<Character displayName>`, or `name` without one. |
| `metrics` | `content` measured with the subtitle's `style` and `maxWidth`, spans and emphasis included, as `<Text>` lays it out (`width`, `height`, `lines`, …): the page shown, for a paged line. Measured only when read, so a `render` that never reads it (nor spreads or destructures it) also works in `inspect.mjs --no-native`; one that does needs the exporter ([verify.md](verify.md#measured-text---native)). |
| `style`, `maxWidth` | The subtitle's own, for drawing the text. |
| `held` | True while the line has ended but the subtitle is kept up (below), and on frames of a paged line no page covers: draw the band, not the text. `text` and `metrics` stay those of the line (the page shown last). |
| `page` | For a paged line, `{ index, count, from, durationInFrames, frame, text, timing }`: the page shown, its frames on the line's clock, and `frame`, frames since it started, for animating a page turn. Null for a line without pages. |
| `emphasis` | The emphasis on `text`, resolved (`{ start, end, text, name?, style }`, in `text`'s grapheme clusters). |
| `frame`, `durationInFrames` | Frames since the subtitle appeared and how long it stays: the enclosing `<Sequence>`, or the run of lines under `holdSubtitle`. Pages never change them. Use them to fade the band in and out. |

To keep the band up between lines, use `<DialogueSeries holdSubtitle>`: in
the gap after each line, `render` is called for that line with
`held: true`, so the band (and name plate) stays while the text clears and
the expression returns to the view's own. `holdThroughGap` keeps the text
up too. A line with a lead-in (`leadIn`, or `sceneLeadIn` at a new scene)
starts a new run: the band leaves after the gap before it and comes back
with the line. `frame` and `durationInFrames` count over the run, so the
fade above plays once per run, not per line. A plain subtitle draws nothing
in a held gap. Without `<DialogueSeries>`, place
`<Dialogue character={view} held>{previousText}</Dialogue>` in the gap
yourself.

## Subtitle pages and emphasis

A long line can show its subtitle a page at a time while its one voice
plays on, and emphasize words in a color or weight. Pages change only what
the subtitle shows: the voice is still one clip, the lip sync one track on
the line's clock, and a `subtitle.render` band keeps its `frame`.

```tsx
const line: DialogueLine = {
  id: 'danger', speaker: 'zunda', audio: './danger.wav',
  text: 'HPが1です。回復しましょう。',
  spokenText: 'えいちぴーがいちです。かいふくしましょう。',
  subtitlePages: [
    { from: 0, durationInFrames: 45, text: 'HPが1です。' },
    { from: 45, text: '回復しましょう。' },
  ],
  emphasis: [{ start: 3, end: 4, style: 'danger' }, { start: 7, end: 9, style: { fontWeight: 900 } }],
};
// <Character … subtitle={{ style, maxWidth, emphasisStyles: { danger: { color: '#ff6060', fontWeight: 800 } } }} />
```

**Pages** (`subtitlePages`, also a `<Dialogue>` prop):

- `from` counts frames from the line's start (its voice's first frame). A
  page shows on `[from, from + durationInFrames)`; without
  `durationInFrames` it lasts until the next page, and the last page until
  the line ends: the voice's end, or the end of the gap under
  `holdThroughGap`. Frames only; for seconds, use `Math.round(s * fps)`.
- The pages, in order, are the line's `text`: each continues where the one
  before ends, and only whitespace between two (a space, a `\n`) may be
  left out. So the text is never written twice in different words, and the
  subtitle file and `lineText` keep the whole line.
- A frame no page covers (before the first, or a gap between two) shows no
  text: a plain subtitle draws nothing, and `subtitle.render` gets
  `held: true` with the page shown last, so a band stays up at its size. A
  page with `text: ''` is a deliberate blank.
- Under `holdSubtitle`, the held band after the line is sized from its last
  page.
- `planDialogue()` refuses, naming the line and the page, a page outside
  the voice (`from` past the end, or a `durationInFrames` that runs past
  it), pages out of order or overlapping, a page whose text does not
  continue the line's, pages that leave text out, an unknown `timing`, and
  pages on a line whose `text` holds `<Span>`s. The planned line's
  `subtitlePages` gives each page's frames and its grapheme range of the
  text.
- When the voice is re-recorded at another length, manual `from`s stay
  where they are: a page past the new end is an error, and an open-ended
  last page follows the end. Pages from `paginateSubtitles()` are
  recomputed on every `prepare()`, so they follow the voice.
- The `--subtitles` file gets one cue per page, for the page's frames.
- `timing` labels how `from` was found: `'manual'` (default), `'aligned'`,
  or `'approximate'`; `subtitle.render` gets it in `page.timing`.

**Emphasis** (`emphasis`, also a `<Dialogue>` prop): ranges of the whole
line's text in grapheme clusters, as [`emphasisContent()`](text.md#emphasis-ranges)
counts them, so an emoji or a letter with combining marks is never split.
A `style` is `{ color?, fontWeight? }` or a name in the speaker's
`subtitle.emphasisStyles`; colors may be `<Theme>` names. Each page shows
the part of the emphasis that falls on it, drawn with `<Span>`s inside the
one `<Text>`, so the page still wraps, measures, and keeps its baseline as
one paragraph. Ranges that are empty, outside the text, or overlapping are
refused by `planDialogue()`; an unknown style name or a bad color when the
line is drawn, naming the line. A line with `emphasis` or pages needs its
`text` as a string; a line with `<Span>`s keeps working as before.

### Splitting long lines with `paginateSubtitles()`

```tsx
import { loadDialogueLipSync, paginateSubtitles, planDialogue } from '@celesta.studio/character';

export async function prepare() {
  const voiced = await loadDialogueLipSync(script);
  const paged = await paginateSubtitles(voiced, {
    fps: 30, style: SUBTITLE.style, maxWidth: SUBTITLE.maxWidth, maxLines: 2, emphasisStyles: EMPHASIS,
  });
  plan = await planDialogue(paged, { fps: 30 });
}
```

- Each line is measured with `measureText()`, with the style, width,
  emphasis weights, fonts (`fonts`), and theme (`theme`) given: pass the
  speaker's `subtitle` values. A line taking more than `maxLines` lines (2)
  or more than `maxHeight` pixels gets a page per that many measured lines;
  a line that fits, a line with `subtitlePages` of its own, and a line
  whose `text` holds `<Span>`s are returned as they are.
- Pages break where the measured lines do, so `lineBreak: 'phrase'` keeps
  Japanese phrases whole; never inside a grapheme cluster. Whitespace at a
  break is dropped; `\n` stays a line break.
- A measured line wider than `maxWidth` (a word that cannot wrap) fails,
  naming the line and the word; `overlong: 'keep'` leaves it on its page.
- **Timing.** With `speechTiming: (line) => timing` (a `SpeechTiming`,
  e.g. `timingFromVoicevox(line.narration.query)`) and the line's
  `subtitleSegments`, `[{ text, moras }]`, pages break only between
  segments (a break inside one moves back to the segment's start) and
  start on their first mora's frame: `timing: 'aligned'`. The segments must
  spell out the text exactly and their moras add up to the timing's;
  otherwise the call fails, naming the line and segment. Without both, the
  voice's frames are shared out in proportion to each page's non-space
  clusters: `timing: 'approximate'`, an estimate, not an alignment. Mora
  counts are never guessed from string offsets.
- Every generated page starts where the one before ends, and the last runs
  to the line's end.

See `packages/cli/examples/with-subtitle-pages.tsx`.

## React props

### `<Character>`

| Prop | Notes |
| --- | --- |
| `name` | Name; also the id unless `id` is given. |
| `displayName` | Name shown to viewers, e.g. on a name plate (`character.displayName` in `subtitle.render`). Defaults to `name`. |
| `portrait` | Image portrait or PSD portrait (below). |
| `subtitle` | Subtitle placement and style: common layer props (`x`, `y`, `anchorX`, …) plus `style` (a `TextStyle`) and `maxWidth`. Positions are canvas coordinates unless the `Dialogue` itself is moved. For Japanese subtitles, set `style.lineBreak: 'phrase'` so lines wrap between phrases ([text.md](text.md#line-breaking)). `render` draws it yourself ([above](#drawing-the-subtitle-yourself)); `emphasisStyles` names the styles its lines' `emphasis` uses ([above](#subtitle-pages-and-emphasis)). |

Image portrait:

```ts
{
  defaultExpression: 'calm',                    // must be a key of expressions
  expressions: { calm: './calm.png', smile: './smile.png' },
  lipSync?: { a, i, u, e, o, closed? },         // mouth images, see below
  blink?: { closed: { calm: './calm-shut.png' }, half?, overlay? }, // see Blinking
}
```

### `<CharacterView>`

`character` (the character ref), common layer props (`x`, `y`, `scale`,
anchors, `opacity`), and optionally `expression`, `mouth`
(`'closed' | 'a' | 'i' | 'u' | 'e' | 'o'`), `lipSync` (a track from
`loadLipSync`), and `blink` (`false` holds the eyes open; an object
overrides the portrait's blink timing, see [Blinking](#blinking)). The
portrait is drawn at its natural size; use `scale` for large artwork.

### `<Dialogue>`

| Prop | Notes |
| --- | --- |
| `character` | The **view** ref. Required. |
| children | Subtitle text; may hold [`<Span>`](text.md#rich-text-with-span) to emphasize part of it: `この機能、<Span style={{ fill: '#ffd447', fontWeight: 700 }}>ぜひ一度</Span>試してね`. |
| `expression` | Applied to the view while the line is active: a name, or `{ at, expression }` cues in seconds from the line's start ([above](#changing-the-face-within-a-line)). |
| `mouth`, `lipSync` | Applied to the view while the line is active. |
| `audio` | Voice file path. |
| `volume`, `playbackRate` | Number or keyframes, as on `<Audio>`. |
| `startFrom`, `muted` | As on `<Audio>`. |
| `x`, `y`, `opacity`, … | Move or fade the **subtitle**, useful inside a `<Transition>`. |
| `held` | Keep the subtitle up after its line: `subtitle.render` gets `held: true`; a plain subtitle draws nothing. Give it no `audio` or `expression`. |
| `subtitlePages`, `emphasis` | The subtitle a page at a time, on the enclosing `<Sequence>`'s clock, and emphasis by grapheme range ([below](#subtitle-pages-and-emphasis)). Need the children as one string. |

An export with `--subtitles film.srt` writes every line that is not `held`
into an SRT or WebVTT file, for as long as its sequence shows it, and a
paged line as one cue per page, for the page's frames
([subtitles.md](subtitles.md#write-the-subtitle-file-on-export)).

## Automatic lip sync

`loadLipSync({ src, text, hopHz? })` reads a voice recording and spreads the
vowels of `text` across its voiced parts, producing a mouth shape for every
moment. Call it in `prepare()`. For VOICEVOX voices, prefer
[`lipSyncFromVoicevox`](#lip-sync-from-voicevox-timing), which uses the
engine's own phoneme timing; for AivisSpeech voices, prefer
[`lipSyncFromAivisSpeech`](#lip-sync-from-aivisspeech).

```tsx
let voice: LipSyncTrack | null = null;
export async function prepare() {
  voice = await loadLipSync({ src: './voices/hello.wav', text: 'こんにちは、はじめまして！' });
}
// …
<Sequence from={30} durationInFrames={Math.ceil((voice?.durationInSeconds ?? 2) * 30)}>
  <Dialogue character={view} audio="./voices/hello.wav" lipSync={voice ?? undefined}>
    こんにちは、はじめまして！
  </Dialogue>
</Sequence>
```

- **Voices must be uncompressed WAV** (PCM 8/16/24/32-bit or float). MP3,
  AAC, and Opus are not supported by `loadLipSync`; convert them first
  (`ffmpeg -i in.mp3 out.wav`).
- **Write the transcript in kana or romaji.** Only vowels in kana
  (hiragana/katakana) and the Latin letters a/i/u/e/o are read; kanji and
  other letters are skipped. `ー` repeats the previous vowel, and small kana
  (`ゃ`, `ぁ`) replace it. Pass a kana reading as `text` even when the
  subtitle shows kanji; for planned lines, put it in `spokenText` and use
  [`loadDialogueLipSync()`](#spoken-text-and-subtitle-text).
- The track is sampled on the **local clock**: its time 0 is the start of
  the enclosing `<Sequence>`, so start the sequence when the voice starts.
- `lipSync` on a `<CharacterView>` drives the mouth for as long as the view
  is rendered; on a `<Dialogue>` only while the line is active.
- `useLipSync(track)` returns the current shape if other components need it.
- To drive the mouth by hand, pass `mouth="a"` etc. instead.

### Lip sync from VOICEVOX timing

VOICEVOX-specific helpers and types live in `@celesta.studio/voicevox`. The desktop
runtime includes the package; **File > Set Up TypeScript** installs its
declarations. Shared WAV lip sync and dialogue components stay in `@celesta.studio/character`.

When the voice comes from VOICEVOX Engine, build the track from the
`audio_query` JSON instead (engines that take the same query but choose their
own phoneme lengths are not timed exactly; for AivisSpeech, see
[Lip sync from AivisSpeech](#lip-sync-from-aivisspeech)):
`lipSyncFromVoicevox(query)` reads every consonant and vowel length, so the
mouth stays in step with the voice. It is synchronous; save the query next to
the WAV and import it or read it in `prepare()`.

```sh
# Save the query, then synthesize the WAV from that same query.
curl -s -X POST -G "http://127.0.0.1:50021/audio_query" --data-urlencode "speaker=3" \
  --data-urlencode "text=こんにちは、ずんだもんなのだ。" -o voices/hello.json
curl -s -X POST "http://127.0.0.1:50021/synthesis?speaker=3" \
  -H "Content-Type: application/json" -d @voices/hello.json -o voices/hello.wav
```

```tsx
import { lipSyncFromVoicevox } from '@celesta.studio/voicevox';
import helloQuery from './voices/hello.json';

const hello = lipSyncFromVoicevox(helloQuery);
// …
<Sequence from={30} durationInFrames={Math.ceil(hello.durationInSeconds * 30)}>
  <Dialogue character={view} audio="./voices/hello.wav" lipSync={hello}>
    こんにちは、ずんだもんなのだ。
  </Dialogue>
</Sequence>
```

- **Use the query that produced the WAV.** If you edit `speedScale`,
  `pauseLength`, `pauseLengthScale` or phoneme lengths before `/synthesis`,
  pass the edited query; those edits are reflected in the track.
- Time 0 is the start of the WAV, including `prePhonemeLength`, and
  `durationInSeconds` is the WAV length. Like VOICEVOX, each phoneme is
  rounded to 1/93.75 s; for an engine that does not use that grid, pass
  `{ frameRate: null }` (or its own rate).
- Vowels `a i u e o` (and devoiced `A I U E O`) map to their shapes; `N` (ん),
  `cl` (っ), pauses and silence are `closed`. A consonant shows its mora's
  vowel, except `m`/`b`/`p`, which close the lips.
- If you synthesize with `enable_interrogative_upspeak=false`, pass
  `lipSyncFromVoicevox(query, { interrogativeUpspeak: false })`.
- Choosing between the two: use `lipSyncFromVoicevox` whenever you have the
  query; use `loadLipSync` for recorded or third-party voices where only the
  WAV and its transcript exist.
- For other engines that report phoneme timing, build the track yourself
  with `lipSyncFromKeyframes([{ seconds, mouth }, ...], durationInSeconds)`;
  each keyframe holds until the next, and the mouth is `closed` before the
  first and after `durationInSeconds`.

### Mora and phoneme timing from VOICEVOX

`timingFromVoicevox(query, options?)` returns the timing the VOICEVOX lip-sync
track is built from: seconds from the start of the WAV (`start` / `end`, not
frames) for every phoneme, mora, pause, and accent phrase. Use it to light
moras as they are spoken, time an effect to a mora, or change a face at a
pause, and build the mouth from the same timing so they agree:

```ts
import { lipSyncFromVoicevox, timingFromVoicevox } from '@celesta.studio/voicevox';

const timing = timingFromVoicevox(query);
const line = {
  id: 'voice-1', audio: './voices/voice-1.wav', text: 'じゃあ、わたしたちの声は？',
  lipSync: lipSyncFromVoicevox(timing),
};

// Per frame, inside the line: seconds since the voice started.
const seconds = (frame - planned.from) / fps;
const current = timing.moras.find((m) => m.start <= seconds && seconds < m.end);
const secondClause = timing.pauses[0]?.start ?? Infinity; // face change point, see expression cues
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
in-line face changes ([expression cues](#changing-the-face-within-a-line)).

### Lip sync from AivisSpeech

AivisSpeech Engine serves VOICEVOX's API (default port 10101), but its model
chooses every phoneme length itself: the `audio_query` lengths are dummy
zeros, so `@celesta.studio/voicevox` cannot time it. Use `@celesta.studio/aivis-speech`,
which reads the query **and the WAV synthesized from it**: the query gives
the moras in order (punctuation included), the WAV where they fall. The
voice's start and end are exact; times inside it are estimated from the
WAV's loudness (pauses land on the model's silences; mora starts are
typically within a few tens of milliseconds, worse in the middle of a long
clause voiced without a break). The desktop runtime includes
the package; **File > Set Up TypeScript** installs its declarations.

```tsx
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { LipSyncTrack } from '@celesta.studio/character';
import { lipSyncFromAivisSpeech, timingFromAivisSpeech } from '@celesta.studio/aivis-speech';
import helloQuery from './voices/hello.json';

let hello: LipSyncTrack;
export async function prepare() {
  const wav = await readFile(join(import.meta.dirname, 'voices/hello.wav'));
  const timing = timingFromAivisSpeech(helloQuery, wav); // same shape as timingFromVoicevox
  hello = lipSyncFromAivisSpeech(timing);                // or lipSyncFromAivisSpeech(helloQuery, wav)
}
```

- **Use the query that produced the WAV**: its `speedScale`,
  `prePhonemeLength` and `postPhonemeLength` locate the voice in the WAV.
  Pass the WAV's bytes (or `decodeWav`'s result); it is synchronous and
  takes under 10 ms per second of voice, so call it in `prepare()`.
- Both engines' timings are a `SpeechTiming` from `@celesta.studio/character`
  (`moras`, `pauses`, `phonemes`, `accentPhrases`, `speech`,
  `durationInSeconds`): `exact: true` from `timingFromVoicevox`,
  `exact: false` from `timingFromAivisSpeech`. Type shared code (lit moras,
  faces at pauses) as `SpeechTiming` and it takes either;
  `lipSyncFromSpeechTiming(timing)` builds the mouth from any of them.
  AivisSpeech moras have no `upspeak` (it ignores interrogative upspeak).
- AivisSpeech keeps punctuation as moras with vowel `pau` and the normalized
  mark as `text` (`,` `.` `!` `?` `…`). They stay in `moras` (skip them for
  display with `m.vowel.phoneme !== 'pau'`) and are listed in `pauses`
  together with any `pause_mora`, **sentence-final marks included**; a mark
  the model did not pause at is short or has `start === end`.
- Mouth shapes follow the VOICEVOX rules above.
  `lipSyncFromAivisSpeech(query)` without the WAV, or
  `lipSyncFromAivisSpeech(timing, wav)`, throws.
- Don't time VOICEVOX voices with it: their queries leave out the upspeak
  mora the WAV contains, so the estimate drifts at questions. Use
  `timingFromVoicevox`, which is exact.

### Image mouths

An image portrait lip-syncs with one transparent mouth image per shape, each
the **same pixel size as the portrait** so it lines up when drawn over it:

```ts
portrait: {
  defaultExpression: 'calm',
  expressions: { calm: './mira/calm.png' },
  lipSync: { a: './mira/mouth-a.png', i: './mira/mouth-i.png', u: './mira/mouth-u.png',
             e: './mira/mouth-e.png', o: './mira/mouth-o.png', closed: './mira/mouth-closed.png' },
}
```

## PSD portraits

```tsx
let pose: string[] = [];
export async function prepare() {
  pose = await loadPsdPreset({ src: './hana/hana.pfv', favorite: 'smile' });
}
// …
<Character ref={hana} name="Hana" portrait={{
  type: 'psd',
  src: './hana/hana.psd',
  layers: pose,                         // which layers are visible
  lipSync: {                            // mouth layers, by full path
    a: 'face/mouth/a', i: 'face/mouth/i', u: 'face/mouth/u',
    e: 'face/mouth/e', o: 'face/mouth/o', closed: 'face/mouth/closed',
  },
}} />
```

- **Layer paths** are folder names and the layer name joined with `/`,
  exactly as they appear in the PSD, including PSDTool prefixes such as `*`
  and `!` (for example `琴葉姉妹/!表情/口/あいうえお/*あ`). Run
  `node scripts/inspect.mjs --psd-layers hana.psd` to list every path.
- **`layers`** chooses the visible layers. "Tachie" PSDs usually save every
  folder hidden, so without `layers` only the mouth may appear. Accepted
  forms:
  - an array of layer/folder paths;
  - a PSDTool layer-state string (paste the output of "copy layer state");
  - the result of `loadPsdPreset({ src: 'x.pfv', favorite? })`, which reads a
    PSDTool favorites file. `favorite` is a favorite's name or tree path;
    when omitted the first favorite is used. An unknown name throws and lists
    the available ones.
- Omit `layers` when the PSD's saved visibility is already the pose you
  want, as in most public character PSDs (body and hair shown, one of each
  face part shown). Expressions are then shown on top of the saved
  visibility; see [PSDs saved with a pose](#psds-saved-with-a-pose).
- **`hide`** lists layers or folders to hide under every expression, such
  as a prop the saved visibility shows. A folder hides every layer in it
  except those the expression, lip sync, or blinking shows. An unknown path
  is an error.
- The mouth layer for the current shape is forced visible and the other
  mouth layers hidden, so list all of them in `lipSync`.
- Large PSDs are big: set `scale` on the `<CharacterView>` (0.2–0.5 is
  common for full-body tachie in 1080p).
- Each layer's blend mode is applied: multiply, screen, overlay, darken,
  lighten, the dodges and burns, soft/hard/vivid/linear/pin light, hard mix,
  difference, exclusion, subtract, divide, hue, saturation, color, and
  luminosity. Folders pass through (their own blend mode and opacity are
  ignored), and dissolve and darker/lighter color layers draw as normal.

### PSD expressions

`expressions` names sets of layers shown on top of `layers`, which every
expression shares. Pick one with `expression` on the `<CharacterView>` or a
`<Dialogue>` line, as with image portraits; `defaultExpression` is shown
otherwise. An expression is a list of layer paths, a PSDTool layer-state
string, or `{ layers, lipSync }` when it has mouth layers of its own (common
in PSDs that keep a mouth folder inside each face folder):

```tsx
<Character ref={hana} name="Hana" portrait={{
  type: 'psd',
  src: './hana/hana.psd',
  layers: ['body', 'hair'],
  defaultExpression: 'calm',
  expressions: {
    calm: ['face/calm'],
    smile: smilePose,                       // e.g. a loadPsdPreset() result stored in prepare()
    angry: { layers: ['face/angry'], lipSync: {
      a: 'face/angry/mouth/a', i: 'face/angry/mouth/i', u: 'face/angry/mouth/u',
      e: 'face/angry/mouth/e', o: 'face/angry/mouth/o', closed: 'face/angry/mouth/n',
    } },
  },
  lipSync: { a: 'mouth/a', i: 'mouth/i', u: 'mouth/u', e: 'mouth/e', o: 'mouth/o', closed: 'mouth/n' },
}} />
// …
<Dialogue character={hanaView} expression="smile">Nice to meet you.</Dialogue>
```

An expression that is not a key of `expressions` throws, as for image
portraits.

### PSDs saved with a pose

Public character PSDs are usually saved with the body shown and, in each
face folder (eyes, brows, mouth, cheeks), one variant shown. Leave out
`layers` to start from that, and give each expression `{ layers, hide }`:
`hide` names the face folders it changes, which hides the saved variant,
and `layers` picks the one to show from each. Nothing else needs listing:

```tsx
<Character ref={hana} name="Hana" portrait={{
  type: 'psd',
  src: './hana/hana.psd',
  hide: 'hana/props/umbrella',              // a prop the PSD was saved with
  defaultExpression: 'normal',
  expressions: {
    normal: [],                             // the face the PSD was saved with
    smile: {
      layers: ['hana/face/brows/smile', 'hana/face/cheeks/red'],
      hide: ['hana/face/brows', 'hana/face/cheeks'],
    },
  },
  lipSync: { /* … */ },
}} />
```

With `layers` set, `hide` works the same way on top of it. Folders and
layers in `hide` must exist (`--psd-layers` lists them).

Many such PSDs have just a closed and an open mouth per face. Give the open
one to the open vowels and the closed one to the rest:

```ts
lipSync: {
  a: 'face/mouth/open', e: 'face/mouth/open', o: 'face/mouth/open',
  i: 'face/mouth/closed', u: 'face/mouth/closed', closed: 'face/mouth/closed',
},
```

## Blinking

A portrait with `blink` blinks on its own, every 4 seconds or so with the
eyes shut for 0.1 s, at irregular moments. It needs nothing per line and
keeps going through lip sync and expression changes.

PSD portraits name the eye layers by full path (a path or a list of paths
each). The open layers are forced visible and the shut ones hidden, and the
other way round during a blink. `half` (half-shut eyes) is optional and
shows on the frame either side of each blink (two at 60 fps):

```tsx
<Character ref={hana} name="Hana" portrait={{
  type: 'psd',
  src: './hana/hana.psd',
  layers: ['body', 'hair'],
  blink: { open: 'eyes/open', closed: 'eyes/closed', half: 'eyes/half' },
  defaultExpression: 'calm',
  expressions: {
    calm: ['face/calm'],
    // A face folder with eyes of its own (common in public character PSDs)
    // blinks with those instead.
    smile: { layers: ['face/smile'], blink: {
      open: ['face/smile/eyes/l', 'face/smile/eyes/r'], closed: 'face/smile/eyes/shut',
    } },
    // Eyes already shut (^^): never blink.
    happy: { layers: ['face/happy'], blink: false },
  },
}} />
```

An expression's `blink` replaces the portrait's eye layers as a set (its
`half` is not borrowed from the portrait), while timing it leaves out still
comes from the portrait's `blink`.

Image portraits give an eyes-shut image per expression; an expression
without one does not blink. By default the image replaces the expression's
image during a blink; with `overlay: true` it is a transparent eyes image the
same size as the portrait, drawn over it (under the mouth) like lip-sync
mouths:

```ts
portrait: {
  defaultExpression: 'calm',
  expressions: { calm: './mira/calm.png', smile: './mira/smile.png' },
  blink: {
    closed: { calm: './mira/calm-shut.png', smile: './mira/smile-shut.png' },
    half: { calm: './mira/calm-half.png' },   // optional
  },
}
```

Timing, on the portrait's `blink`, an expression's `blink`, or the view's
`blink={{ … }}` (the later wins):

| Field | Default | Notes |
| --- | --- | --- |
| `interval` | `4` | Average seconds between blinks; each gap varies between half and one and a half times this. |
| `duration` | `0.1` | Seconds the eyes stay shut, at least one frame. |
| `seed` | the character's id | Same seed, same blinks. Two characters with different ids already blink independently; give a character a different seed if two of its views should not blink together. |

- Blinks are a pure function of the **composition** frame and the seed, so
  preview and export match, and a cut to a new `<Sequence>` does not restart
  them. `Math.random()` is never used.
- `<CharacterView blink={false}>` holds the eyes open, for a close-up or a
  dramatic stare. Switch it per frame like any other prop.
- `blinkPhase(frame, fps, { interval?, duration?, seed? })` returns
  `'open' | 'half' | 'closed'` for the same schedule, if something else
  (a custom portrait, an eyelid effect) needs to blink in step.
- JSON projects do not blink yet.

## JSON projects

Characters live in `characters`; lines are `dialogue` items on a `dialogue`
track. Unlike React, **the portrait is drawn only while a dialogue item is
active**, and portrait and subtitle positions come from the character.

```json
"characters": {
  "mira": {
    "name": "Mira",
    "portrait": {
      "defaultExpression": "calm",
      "expressions": { "calm": "mira-calm", "smile": "mira-smile" },
      "transform": { "position": { "x": 1460, "y": 420 } },
      "lipSync": { "a": "mira-a", "i": "mira-i", "u": "mira-u", "e": "mira-e", "o": "mira-o", "closed": "mira-closed" }
    },
    "subtitle": {
      "style": {
        "fontSize": 56, "align": "center",
        "fill": { "type": "solid", "color": "#FFFFFFFF" },
        "stroke": { "paint": { "type": "solid", "color": "#3A2D52FF" }, "width": 6 }
      },
      "transform": { "position": { "x": 960, "y": 940 } },
      "maxWidth": 1600
    }
  }
}
```

```json
{
  "id": "line-02",
  "range": { "start": { "value": 3, "timescale": 1 }, "duration": { "value": 3, "timescale": 1 } },
  "content": {
    "type": "dialogue",
    "character": "mira",
    "text": "Every scene starts with a single line.",
    "audio": "line-02",
    "expression": "smile",
    "volume": 1,
    "lipSync": [
      { "time": { "value": 0,  "timescale": 30 }, "shape": "closed" },
      { "time": { "value": 3,  "timescale": 30 }, "shape": "e" },
      { "time": { "value": 9,  "timescale": 30 }, "shape": "i" },
      { "time": { "value": 15, "timescale": 30 }, "shape": "closed" }
    ]
  }
}
```

- Every id in `expressions` and `lipSync` is an **image asset** id declared
  in `assets`; `audio` is an audio asset id.
- Portrait and subtitle `transform.position` are the **centers** of the
  portrait image and the subtitle text, in canvas pixels. Leave the dialogue
  item's own `transform` unset unless you want to move both together.
- `portrait.lipSync.transform` positions the mouth; when omitted it reuses
  the portrait transform, which is right for full-size mouth overlays.
- Lip-sync cues are relative to the item start, strictly ascending, within
  the item's duration, and require both an `audio` asset and
  `portrait.lipSync`. There is no automatic lip sync in JSON; for timing
  from the recording, use React's `loadLipSync()` (optionally with
  `<ProjectTimeline />` for the rest of the timeline).
- JSON portraits are images only; PSD portraits need React.
- Without `subtitle`, the line is drawn unstyled at the top-left corner:
  always define it.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `character has no expression "x"` | `expression` (and `defaultExpression`) must be a key in `portrait.expressions`, for image and PSD portraits alike. |
| `<Dialogue> requires a declared character` | `character` must be a view ref attached to a rendered `<CharacterView>`, not the character ref. |
| `<CharacterView> requires a character with a portrait` | The `<Character>` needs a `portrait`. |
| The mouth never moves | WAV is uncompressed; transcript has kana/romaji vowels; `loadLipSync` runs in `prepare()`; the track is passed as `lipSync`; the sequence starts when the voice starts; PSD mouth paths match exactly (list them with `--psd-layers`). |
| PSD portrait is empty or shows only a mouth | Set `layers` from a PSDTool favorite or layer list. |
| A PSD expression's face part overlaps the saved one (two brows) | Add the face folder to the expression's `hide`. |
| `PSD asset … has no layer …` | A path in `lipSync`, `blink`, or `hide` (or, without `layers`, an expression's layers) is not in the PSD; check it against `--psd-layers`. |
| `prepareNarration(): … failed at audio_query: could not reach the VOICEVOX Engine` | Start the engine (the message shows the address) or fix the speaker's `baseUrl`. Cached lines do not need it. |
| A synthesized voice ignores a dictionary or engine change | The cache cannot see it; bump `revision` or pass `refresh`. |
| `planDialogue(): voice file for line … not found` | The path is relative to the entry file; the message shows where it looked. |
| `<DialogueSeries> line …: speaker "x" is not in views` | Add the speaker to `views`, or set `character` on the line. |
| The portrait never blinks | `blink` is on the `portrait` (not the view); for an image portrait, `blink.closed` has a key for the current expression; the expression's `blink` is not `false`; the view is not `blink={false}`; a blink is 3 frames every ~4 s, so step frames rather than glancing at one. |
| A PSD blink shows both eyes, or none | `open`, `closed`, and `half` must list every eye layer, by full path (`--psd-layers`). A layer in neither stays as `layers`/the expression set it, so open eyes left out of `open` show through shut ones. |
| Two characters blink at the same time | They share an id or a `seed`; give each its own `seed`. |
| Subtitle in the wrong place | React subtitle `x`/`y` are canvas coordinates with anchors like `Text`; JSON subtitle `position` is the text's center. |
