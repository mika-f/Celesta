# Subtitle files (SRT / WebVTT)

Reading subtitles from an SRT or WebVTT file, drawing them, and writing a
video's subtitles out as files beside it, one per language and format if
needed, or without the video. `parseSubtitles`,
`formatSubtitles`, and `<Subtitles>` come from `@celesta.studio/react`;
`<CaptionTrack>` from `@celesta.studio/components`.
`<Dialogue>` lines ([dialogue.md](dialogue.md)) go into the same file.

## Contents

- [Read a file](#read-a-file)
- [Draw the cues](#draw-the-cues): CaptionTrack, and [word by word](#light-each-word-as-it-is-spoken)
- [Write the subtitle file on export](#write-the-subtitle-file-on-export)
- [Several languages](#several-languages)
- [Subtitles without the video](#subtitles-without-the-video)
- [Write a file from data](#write-a-file-from-data)

## Read a file

Import the file and parse it at the composition's frame rate. The CLI and
the web editor bundle `.srt` and `.vtt` files as their text, and the
editor reloads when one changes.

```tsx
import { parseSubtitles } from '@celesta.studio/react';
import captions from './captions.srt';

const cues = parseSubtitles(captions, { fps: 30 });
// [{ id: '1', from: 15, durationInFrames: 75, text: '字幕ファイルを、そのまま映像に。' }, …]
```

### `parseSubtitles(source, { fps, format? })`

Returns `SubtitleCue[]` in the file's order: `{ id?, from, durationInFrames,
text }`, shown on frames `[from, from + durationInFrames)`. `from` and
`durationInFrames` are what `<Sequence>` takes.

- Times are rounded to the nearest frame at `fps`; a cue keeps at least one
  frame. Pass the composition's `fps`.
- `format` is `'srt'` or `'vtt'`; by default a file starting with `WEBVTT`
  is WebVTT, anything else SRT.
- Accepted: a byte order mark; CRLF, LF, or CR line ends; cues of several
  lines (joined with `\n`); `,` or `.` before the milliseconds; one to three
  or more fraction digits (`01,5` is 1.5 s); a missing hour (`01:02.500`);
  WebVTT cue settings after the end time; `NOTE`, `STYLE`, and `REGION`
  blocks; missing SRT numbers.
- Markup is removed, since `<Text>` draws plain text: SRT `<i>`, `<b>`,
  `<u>`, `<s>`, `<font …>` and `{\an8}`-style overrides; every WebVTT tag
  (`<v Name>`, `<c.class>`, timestamps), with `<rt>` ruby text dropped and
  character references (`&amp;`, `&#x2665;`) decoded.
- Cues whose text is empty after that are left out.
- A malformed block throws with its line number: a block without a timing
  line, times it cannot read, minutes or seconds of 60 or more, or a cue
  that ends before it starts.

The cues come out in file order; sort them by `from` before using
`cueAt()`/`useCue()` on a file that is out of order.

## Draw the cues

`<CaptionTrack cues>` from `@celesta.studio/components` draws the cue on each
frame on a band and declares the cues for the export's subtitle file. It
needs no `<Character>`, voice, or audio:

```tsx
import { CaptionTrack } from '@celesta.studio/components';

<CaptionTrack cues={cues} />
<CaptionTrack cues={[
  { id: 'intro', from: 0, durationInFrames: 60, text: 'こんにちは' },
  { id: 'next', from: 60, durationInFrames: 90, text: '動画を作ります',
    emphasis: [{ start: 0, end: 2, style: 'key' }] },
]} emphasisStyles={{ key: { color: '#ffd166', fontWeight: 700 } }} />
```

- **Timing.** A cue (`SubtitleCue` plus optional `emphasis`) is shown on
  frames `[from, from + durationInFrames)`, counted from the enclosing
  `<Sequence>`'s start; the [timed range](text.md#timed-ranges) rules apply.
  Frames no cue covers show nothing; a cue outside the enclosing sequence
  never shows. Overlapping cues throw, naming both, unless
  `overlap="latest"` lets the later one cut the earlier short. `from` and
  `durationInFrames` must be whole numbers (`parseSubtitles()` gives them).
  A cue with empty `text` shows no band, though a `render` callback is
  still called for it (to draw, say, a speaker tag). Seeking to any frame, in any
  order, draws the same picture.
- **Layout.** The band sits at the bottom of the layout area (`position`
  `'top'` / `'middle'` / `'bottom'`, `align` `'left'` / `'center'` /
  `'right'`), inside `safeArea` (as `<SafeArea padding>`, e.g.
  `'vertical-social'`) and `margin` (6% of the area's shorter side). Text
  wraps at `maxWidth` (80% of the room) with `lineBreak: 'phrase'`, keeps
  `\n`, and takes at most `maxLines` (2). With `minFontSize` below
  `fontSize` it shrinks to fit (through `useFitText`); a caption that still
  does not fit shows its first `maxLines` lines, or fails the render with
  `overflow="error"`.
- **Look.** `fontSize` (4.4% of the shorter side), `lineHeight` (1.4, a
  multiple), `fontFamily`, `color` (hex or `<Theme>` name), `style` (more
  `TextStyle`), `background` (band fill, `#000000b3`; `null` for none),
  `padding` (0.3 em / 0.6 em), `cornerRadius` (0.2 em), and `panel` for
  other `<Panel>` props such as `stroke` or `shadow`.
- **Emphasis.** `emphasis` ranges count grapheme clusters, with styles
  inline or named in `emphasisStyles`; see [Emphasis
  ranges](text.md#emphasis-ranges). Bad ranges or styles throw with the cue
  named.
- **Entrance and exit.** `enter` / `exit` take a `<Motion>` effect, `null`
  to cut; both default to a fade over a tenth of a second. A cue that
  starts where the one before it ends cuts in (and that one does not fade
  out), so back-to-back captions swap without a blink.
- **Custom drawing.** `render={(props) => …}` replaces the band and text,
  called as a component inside the cue's own `<Sequence>` and its Motion.
  It gets `cue`, `index`, `text`, `content` (spans included), `emphasis`
  (resolved), `frame` and `durationInFrames` (the cue's clock, the same as
  `useCurrentFrame()` inside it), the fitted `style`, `metrics`, `maxWidth`,
  `fits`, and `box`, where the default band would be. Replacing the band
  never changes which cue shows on a frame or its clock.
- **Subtitle file.** The track declares its cues with `<Subtitles>`
  (`lang` sets their language); `subtitles={false}` leaves them out, for a
  second track drawing the same cues.
- SRT/ASS styling is not read; cues from `parseSubtitles()` have plain
  text, so add `emphasis` in code.

### Light each word as it is spoken

`highlight` lights the word being spoken, the caption style of vertical
short videos; `paging` shows a long cue a page at a time. Either one splits
each cue into words:

```tsx
<CaptionTrack
  cues={[{
    from: 0, durationInFrames: 90, text: 'Every frame is a function',
    words: [
      { text: 'Every', from: 0 }, { text: 'frame', from: 12 }, { text: 'is', from: 30 },
      { text: 'a', from: 38 }, { text: 'function', from: 44 },
    ],
  }]}
  highlight={{ color: '#111111', background: '#ffd166', scale: 1.12, upcomingColor: '#ffffff99' }}
  paging={{ maxWords: 3 }}
/>
<CaptionTrack cues={cues} wordUnit="phrase" highlight={{ color: '#ffd166' }} />
```

- **Words.** A cue's `words` (`{ text, from, durationInFrames? }`, frames
  counted like the cue's `from`, in order, inside the cue) are found in
  `text` in order: text between two words, such as punctuation, goes with
  the word before, and space around a word is ignored. A word is lit from
  its `from` until the next word starts, or for `durationInFrames`.
  Without `words`, the cue's frames are shared out among its words by
  length (grapheme clusters).
- **Units.** `wordUnit="word"` (the default) splits text at spaces.
  `wordUnit="phrase"` also splits Japanese into phrases (文節) with the
  same BudouX model `lineBreak: 'phrase'` wraps by, and joins given `words`
  that do not start a phrase to the word before, so per-morpheme timings
  (`今日` `は`) light a phrase (`今日は`) at a time.
- **Highlight.** `color` (`#ffd166`) for the word being spoken, over its
  emphasis; `spokenColor` and `upcomingColor` for the rest of the page
  (default: the track's `color`; emphasis wins over them). `background`
  draws a band behind the word, `scale` grows it about its centre, with
  `padding` (0.08 em / 0.12 em) and `cornerRadius` (0.15 em). Over
  `transitionInFrames` (a tenth of a second; 0 cuts) the band slides from
  the word before on the same line and the word grows. The band and scale
  spill a little into the next words' room (Japanese has no spaces to
  absorb them), so give the band a color the other words read on. They
  are placed from per-glyph metrics; the browser renderer has none, so
  there only colors change. The weight cannot change per word, as that
  would reflow the page.
- **Pages.** A page takes words while it still fits `maxLines` at
  `fontSize`, and ends before `paging.maxWords` is passed, after a word
  that ends a sentence (`。！？.!?…`, unless `paging.sentences` is false),
  and where no word is spoken for `paging.pause` frames. Each page shows
  from its first word's `from` (the first from the cue's start) until the
  next page; pages cut, while the cue still fades in and out.
  `highlight` alone pages by these defaults.
- **Custom drawing.** In this mode `render` gets the page: `text`,
  `content` (with the highlight colors), and `emphasis` are the page's,
  `words` lists its words with their clusters in `text`, frames on the
  cue's clock, and `state` (`'spoken'`, `'current'`, `'upcoming'`), and
  `page` / `pages` count the pages.
- The subtitle file keeps whole cues.

`examples/word-captions/` lights English and Japanese captions word by
word on a vertical short.

See [`packages/cli/examples/with-caption-track.tsx`](../../../packages/cli/examples/with-caption-track.tsx)
and `examples/captions/`.

To draw cues yourself, give each one its own `<Sequence>`, and add
`<Subtitles cues>` so an export can write them out:

```tsx
<Subtitles cues={cues} />
{cues.map((cue, index) => (
  <Sequence key={index} from={cue.from} durationInFrames={cue.durationInFrames}>
    <CaptionBand>{cue.text}</CaptionBand>
  </Sequence>
))}
```

`<Subtitles>` draws nothing. Its cue frames count from the enclosing
`<Sequence>`'s start, like the sequence's children; a cue outside that
sequence is cut to it, the way `<Audio>` plays only inside it. It cannot be
inside `<Static>`, and inside `<FreezeFrame>` it contributes nothing.

`examples/captions/` draws the cues with `<CaptionTrack>` and shows each
cue's own SRT block next to a timeline.

## Write the subtitle file on export

`--subtitles <file>` writes the video's subtitles beside it, as SRT for
`.srt` and WebVTT for `.vtt`. Repeat it to write the same subtitles in
several formats from one export:

```sh
Celesta-export --json --react film.tsx film.mp4 --subtitles film.srt --subtitles film.vtt
```

- The file holds every `<Subtitles>` cue and every `<Dialogue>` line (not a
  `held` one) for as long as it is shown, ordered by start. A JSON project
  writes its dialogue items on enabled tracks.
- With `--from`/`--to`, times start at the span's start and cues are cut to
  the span; cues outside it are left out.
- A line reported more than once with the same text and times is one cue.
  Blank lines inside a cue are dropped; WebVTT escapes `&`, `<`, and `>`.
- With a still-image selection (`--frame`, `--every`) `--subtitles` is a usage error. Without
  `--overwrite` an existing subtitle file is `output_exists`, checked before
  anything renders (a `{lang}` path's files only when they are written);
  another extension is `unsupported_subtitles`.
- The JSON result lists each file in `outputs` as `{"type": "subtitles",
  "path", "format": "srt" | "vtt", "cues", "lang"?}`; `lang` is there when
  the subtitles have a language.
- The MCP `export_video` tool takes the same as `subtitles`: one path or a
  list of them.

## Several languages

Give each language its own `<Subtitles lang>` and put `{lang}` in the path.
Each language goes to its own file, with the language in place of `{lang}`:

```tsx
<Subtitles lang="ja" cues={japanese} />
<Subtitles lang="en" cues={english} />
```

```sh
Celesta-export --json --react film.tsx film.mp4 \
  --subtitles 'film.{lang}.srt' --subtitles 'film.{lang}.vtt'
# film.ja.srt, film.en.srt, film.ja.vtt, film.en.vtt
```

- Without `lang`, a `<Subtitles>` takes the `lang` of the enclosing
  `<Sequence>`, `<Group>`, or `<Composition>`, as text does
  ([text.md](text.md)); so do `<Dialogue>` lines. Set `lang` on every
  `<Subtitles>` whose language differs from the composition's: in a
  `<Composition lang="ja-JP">`, an English `<Subtitles>` without `lang` is
  `ja-JP` too.
- Languages are compared as written: `ja` and `ja-JP` are two files.
  Subtitles with no language at all go to `und` (`film.und.srt`). Characters
  other than letters, digits, `-`, and `_` become `_` in the file name.
- A `{lang}` path with no subtitles writes nothing and warns.
- A path without `{lang}` takes subtitles of one language only; subtitles in
  several languages there fail with `mixed_subtitle_languages`, after the
  video is written, and no subtitle file is written.
- Quote the path in the shell so `{lang}` reaches the exporter as written.

## Subtitles without the video

Leave out the output video to write only the subtitle files. Every frame
of the composition (or the `--from`/`--to` span) is evaluated as an export
would, so the files are the same as a video export writes, but nothing is
rendered or encoded: a 90-second film takes seconds, not minutes.

```sh
Celesta-export --json --react film.tsx --subtitles 'film.{lang}.vtt'
```

- `--from`/`--to`, `--props`, `--props-file`, `--project`, `--composition`,
  and `--overwrite` work as with a video. Options about pictures or encoding
  (`--frame`, `--crf`, `--preset`, `--render-quality`, …) are usage errors.
- Progress counts the evaluated frames; the JSON result has `composition`
  and the subtitle files in `outputs`, and no video.
- A JSON project writes its dialogue items without evaluating anything.
- MCP: call `export_video` with `subtitles` and no `output`.

## Write a file from data

### `formatSubtitles(cues, { fps, format })`

Writes `SubtitleCue[]` as an SRT or WebVTT string, sorted by `from`. SRT
cues are numbered from 1; WebVTT keeps an `id` that has no line break or
`-->`. Times are rounded to the millisecond. Blank lines inside a cue are
dropped, WebVTT escapes `&`, `<`, and `>`, and cues with no text are left
out. A cue without a finite `from`, a positive `durationInFrames`, or a
string `text` throws. `parseSubtitles()` reads the result back to the same
frames.
