# Character dialogue, portraits, and lip sync

Dialogue scenes combine three pieces:

- a **character**: a portrait (one image per expression, or a layered PSD),
  optional lip-sync mouths, and the character's subtitle style;
- a **view** that places the portrait on screen (React only; JSON places the
  portrait from the character definition);
- **lines**: a subtitle, an optional voice recording, and optional
  expression or mouth changes while the line is active.

## Contents

- [React: a two-line conversation](#react-a-two-line-conversation)
- [Timing a script from its voices](#timing-a-script-from-its-voices)
- [React props](#react-props)
- [Automatic lip sync](#automatic-lip-sync)
- [PSD portraits](#psd-portraits)
- [Blinking](#blinking)
- [JSON projects](#json-projects)
- [Troubleshooting](#troubleshooting)

## React: a two-line conversation

```tsx
import * as React from 'react';
import { Assets, Character, CharacterView, Composition, Dialogue, Rect, Sequence } from '@celesta/react';
import type { AssetReference, CharacterViewReference } from '@celesta/react';

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
import { CharacterView, Composition, DialogueSeries, Rect, Sequence, planDialogue } from '@celesta/react';
import type { AssetReference, CharacterViewReference, DialoguePlan } from '@celesta/react';

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
| `text` | Subtitle. |
| `audio` | Voice file, relative to the entry file. Its length (rounded up to whole frames) is the line's length. |
| `durationInFrames` | Explicit length instead of measuring; required for a line without `audio`. |
| `id` | Name for looking the line up; defaults to its index (`"0"`, `"1"`, …). Unique. |
| `gap` | Seconds of silence after the line; defaults to the plan's `gap` (0.25 s). |
| `leadIn` | Seconds of silence before the line. Defaults to `sceneLeadIn` on the first line of a new `scene` (not the script's first line), else 0. |
| `scene` | Groups consecutive lines into a scene. |
| `speaker` / `character` | Who speaks: a key of `<DialogueSeries views>`, or the `<CharacterView>` ref itself. |
| `expression`, `mouth`, `lipSync`, `volume`, `muted` | Passed to the line's `<Dialogue>`. |

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
| `dialogueProps` | `(planned) => props` merged into each `<Dialogue>`, e.g. to move one subtitle. |

For lip sync, load each track in `prepare()` and put it on the line:
`lines = await Promise.all(lines.map(async (l) => ({ ...l, lipSync: await loadLipSync({ src: l.audio, text: l.reading }) })))`.

A missing voice file fails `prepare()` with
`planDialogue(): voice file for line "how" not found: ./voices/03.wav (looked at …)`.

## React props

### `<Character>`

| Prop | Notes |
| --- | --- |
| `name` | Display name; also the id unless `id` is given. |
| `portrait` | Image portrait or PSD portrait (below). |
| `subtitle` | Subtitle placement and style: common layer props (`x`, `y`, `anchorX`, …) plus `style` (a `TextStyle`) and `maxWidth`. Positions are canvas coordinates unless the `Dialogue` itself is moved. |

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
| children | Subtitle text. |
| `expression`, `mouth`, `lipSync` | Applied to the view while the line is active. |
| `audio` | Voice file path. |
| `volume`, `playbackRate` | Number or keyframes, as on `<Audio>`. |
| `startFrom`, `muted` | As on `<Audio>`. |
| `x`, `y`, `opacity`, … | Move or fade the **subtitle**, useful inside a `<Transition>`. |

## Automatic lip sync

`loadLipSync({ src, text, hopHz? })` reads a voice recording and spreads the
vowels of `text` across its voiced parts, producing a mouth shape for every
moment. Call it in `prepare()`. For VOICEVOX voices, prefer
[`lipSyncFromVoicevox`](#lip-sync-from-voicevox-timing), which uses the
engine's own phoneme timing.

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
  subtitle shows kanji.
- The track is sampled on the **local clock**: its time 0 is the start of
  the enclosing `<Sequence>`, so start the sequence when the voice starts.
- `lipSync` on a `<CharacterView>` drives the mouth for as long as the view
  is rendered; on a `<Dialogue>` only while the line is active.
- `useLipSync(track)` returns the current shape if other components need it.
- To drive the mouth by hand, pass `mouth="a"` etc. instead.

### Lip sync from VOICEVOX timing

When the voice comes from VOICEVOX Engine (or a compatible engine such as
AivisSpeech), build the track from the `audio_query` JSON instead:
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
import { lipSyncFromVoicevox } from '@celesta/react';
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
- Omit `layers` only if the PSD's saved visibility is already the pose you
  want.
- The mouth layer for the current shape is forced visible and the other
  mouth layers hidden, so list all of them in `lipSync`.
- Large PSDs are big: set `scale` on the `<CharacterView>` (0.2–0.5 is
  common for full-body tachie in 1080p).
- Each layer's blend mode is applied: multiply, screen, overlay, darken,
  lighten, the dodges and burns, soft/hard/vivid/linear/pin light, hard mix,
  difference, exclusion, subtract, and divide. Folders pass through (their own
  blend mode and opacity are ignored), and dissolve, darker/lighter color,
  hue, saturation, color, and luminosity layers draw as normal.

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
| `planDialogue(): voice file for line … not found` | The path is relative to the entry file; the message shows where it looked. |
| `<DialogueSeries> line …: speaker "x" is not in views` | Add the speaker to `views`, or set `character` on the line. |
| The portrait never blinks | `blink` is on the `portrait` (not the view); for an image portrait, `blink.closed` has a key for the current expression; the expression's `blink` is not `false`; the view is not `blink={false}`; a blink is 3 frames every ~4 s, so step frames rather than glancing at one. |
| A PSD blink shows both eyes, or none | `open`, `closed`, and `half` must list every eye layer, by full path (`--psd-layers`). A layer in neither stays as `layers`/the expression set it, so open eyes left out of `open` show through shut ones. |
| Two characters blink at the same time | They share an id or a `seed`; give each its own `seed`. |
| Subtitle in the wrong place | React subtitle `x`/`y` are canvas coordinates with anchors like `Text`; JSON subtitle `position` is the text's center. |
