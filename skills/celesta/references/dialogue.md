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
- [React props](#react-props)
- [Automatic lip sync](#automatic-lip-sync)
- [PSD portraits](#psd-portraits)
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

To fit lines to their recordings, measure them in `prepare()` with
`preloadMedia(voice)` and `mediaDurationInFrames(info, fps)`, then compute
`at`/`len` from those lengths (plus a small gap).

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
}
```

### `<CharacterView>`

`character` (the character ref), common layer props (`x`, `y`, `scale`,
anchors, `opacity`), and optionally `expression`, `mouth`
(`'closed' | 'a' | 'i' | 'u' | 'e' | 'o'`), and `lipSync` (a track from
`loadLipSync`). The portrait is drawn at its natural size; use `scale` for
large artwork.

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
moment. Call it in `prepare()`.

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
| `character has no expression "x"` | `expression` must be a key in `portrait.expressions`. |
| `<Dialogue> requires a declared character` | `character` must be a view ref attached to a rendered `<CharacterView>`, not the character ref. |
| `<CharacterView> requires a character with a portrait` | The `<Character>` needs a `portrait`. |
| The mouth never moves | WAV is uncompressed; transcript has kana/romaji vowels; `loadLipSync` runs in `prepare()`; the track is passed as `lipSync`; the sequence starts when the voice starts; PSD mouth paths match exactly (list them with `--psd-layers`). |
| PSD portrait is empty or shows only a mouth | Set `layers` from a PSDTool favorite or layer list. |
| Subtitle in the wrong place | React subtitle `x`/`y` are canvas coordinates with anchors like `Text`; JSON subtitle `position` is the text's center. |
