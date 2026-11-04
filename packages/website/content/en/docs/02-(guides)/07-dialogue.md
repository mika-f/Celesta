Dialogue scenes are built from three pieces. Each works in React and in JSON projects:

- A **character** defines a portrait (one image per expression, or a layered PSD), optional lip-sync mouths, and how that character’s subtitles look.
- A **character view** places the portrait on screen.
- A **dialogue line** shows a subtitle, plays a voice recording, and can change the view’s expression or mouth while it is on screen.

## Your first conversation in React

Save the file below with two portrait images and two voice recordings beside it:

```tsx file=examples/dialogue.tsx title="dialogue.tsx"
```

1. **Declare the character** inside `<Assets>`. The ref lets views refer to it. `subtitle` takes the same position and style props as `Text`.
2. **Place a view** with `<CharacterView>`. It shows the `defaultExpression` unless a line says otherwise, and takes `x`, `y`, `scale`, and anchors like any layer.
3. **Time each line** by wrapping `<Dialogue>` in a `Sequence`. The subtitle, voice, and `expression` apply only while it is active, then the view returns to its default.

> [!NOTE]
> **One view, many lines**
> A `Dialogue` points to a view, not to the character, so the same portrait stays in place while lines change around it. Add a second `Character` and view for a two-person conversation.

Adjust a line with `volume`, `startFrom`, `playbackRate`, or `muted`, just as on `Audio`. Setting `x`, `y`, or `opacity` on a `Dialogue` moves or fades its subtitle, which is handy inside a `Transition`.

Emphasize part of a line with `<Span>` inside the `Dialogue`. A `subtitle.render` receives the line as written in `content`: draw it with `<Text style={style}>{content}</Text>` to keep the emphasis; `text` is the same line as a plain string.

## Automatic lip sync

`loadLipSync()` listens to a voice recording and spreads the vowels of its transcript across the spoken parts, producing a mouth shape (`a`, `i`, `u`, `e`, `o`, or `closed`) for every moment. Call it from `prepare()` and pass the result to a `Dialogue` or `CharacterView` as `lipSync`.

```tsx file=examples/lip-sync.tsx title="lip-sync.tsx"
```

- **Voices** must be uncompressed WAV files. Write the transcript in kana or romaji so its vowels can be read; kanji are skipped.
- **Spoken text** apart from the subtitle: give a planned line a kana `spokenText` (`text: 'HPが1です。'`, `spokenText: 'えいちぴーがいちです。'`) and call `loadDialogueLipSync(lines)` in `prepare()`. The subtitle and the subtitle file show `text`; the mouth and speech engines follow `spokenText`.
- **PSD portraits** name their mouth layers by path, such as `face/mouth/a`. Layered “tachie” files usually save every folder hidden, so pick visible layers with `layers`: an array of layer paths, a PSDTool layer-state string, or a `.pfv` favorite loaded with `loadPsdPreset()`.
- **Image portraits** can lip-sync too. Give `portrait.lipSync` one transparent mouth image per shape, the same size as the portrait, so each lines up when drawn over it.
- To drive a mouth yourself, pass `mouth="a"` (or another shape) to a `CharacterView` or `Dialogue`. `useLipSync(track)` returns the current shape if you want to react to it elsewhere.

## Sequence a conversation automatically

Call planDialogue(lines, { fps, gap, sceneLeadIn }) in prepare(). It measures each audio file unless durationInFrames is supplied, then places the lines back to back. gap and leadIn use seconds; durations use frames. Every line needs audio or a positive durationInFrames. Set Composition durationInFrames from the plan and pass speaker names to DialogueSeries through views. The plan exposes line(), startOf(), range(), and scene() for other direction.

A long line can show its subtitle a page at a time while its one voice and lip sync play on: give it `subtitlePages` (`{ from, durationInFrames?, text }` in frames from the line's start, the pages together spelling the line's `text`), or let `paginateSubtitles(lines, { fps, style, maxWidth, maxLines })` split it from the measured text in `prepare()`. `emphasis` ranges (grapheme clusters, with a color and weight, or a name from the speaker's `subtitle.emphasisStyles`) color words on whichever page they fall. `subtitle.render` gets the page, its emphasis, and the same line clock, so a band never restarts.

```tsx file=examples/dialogue-series.tsx title="dialogue-series.tsx"
```

holdThroughGap keeps the previous line’s text and expression through its gap. holdSubtitle lets a custom subtitle.render keep its background between lines; its held flag identifies the gap. subtitle.render receives text, character (including displayName), metrics, style, maxWidth, frame, and durationInFrames so you can draw a measured subtitle panel. See the repository’s with-dialogue-series.tsx for a complete example.

To change the face partway through a line, give `expression` a list of `{ at, expression }` cues instead of one name. `at` is seconds from the start of the line (the first sample of its voice), the clock `lipSync` uses. Before the first cue the view shows its own expression; from each cue on, that cue’s, and the last one holds to the end of the line (through the gap under holdThroughGap). Cues need `at` >= 0, in order. Rather than writing seconds by hand, take them from the voice, such as `timingFromVoicevox(query).pauses[0].start` (where the pause at a comma starts), so they follow a regenerated voice. Other parts can read the same face with `expressionAt(expression, seconds)`.

```ts
{ id: 'psd-2', speaker: 'kiritan', audio: './voices/psd-2.wav', text: '…',
  expression: [{ at: 0, expression: 'smile' }, { at: timing.pauses[0].start, expression: 'smug' }] }
```

For voices generated with VOICEVOX or AivisSpeech, use the synthesis AudioQuery with [@celesta.studio/voicevox](/en/docs/voicevox/) or [@celesta.studio/aivis-speech](/en/docs/aivis-speech/) instead of spreading the transcript’s vowels over the WAV.

To have VOICEVOX Engine read the script for you, call `prepareNarration()` from [@celesta.studio/voicevox](/en/docs/voicevox/) before planDialogue(): it synthesizes each line, caches the voice, and sets `audio` and `lipSync`, so only lines whose spoken text or voice changes are synthesized again.

## Import a voice tool's export folder

VOICEROID, A.I.VOICE, and AquesTalk front ends can save each line as `name.wav` beside its text in `name.txt`. `importVoiceFolder({ directory, speaker })` reads such a folder in prepare() and returns lines for planDialogue(): ids from the file names, subtitles from the text files, lengths from the recordings, and lip sync from the waveform. Lines play in natural order of their names (`voice-2` before `voice-10`) on every OS.

```tsx
const lines = await importVoiceFolder({ directory: './voices/akane', speaker: 'akane', manifest: './voices/akane/manifest.json' });
plan = await planDialogue(lines, { fps: 30 });
```

Transcripts are UTF-8 unless you pass `encoding: 'utf-16le'` or `'shift_jis'`; one that does not decode fails with its path instead of showing garbled text. An optional manifest (`{ "lines": [{ "file": "voice-1", "id", "speaker", "text", "spokenText", "scene", "expression", "gap" }] }`) overrides ids, order, speaker, subtitle, kana spoken text, scene, expression, and gap. Mouth vowels come from the kana `spokenText` or the transcript; a line with kanji and no `spokenText` only opens and closes its mouth, with a warning.

While the composition is open, resaving, adding, or deleting a recording or its text reloads the preview, even in a folder outside the entry's directory, and later lines move with a longer take. A pair caught halfway through being written fails the reload and keeps the last good preview until the save completes (pass `incomplete: 'skip'` to leave such lines out instead). Only uncompressed PCM or float WAV is read.

## Match voice levels

Voices from different speakers or engines often differ in level. Call `matchVoiceLevels(lines, { targetRmsDbfs, peakCeilingDbfs, maxBoostDb, maxCutDb, speakers })` in prepare() before planDialogue() and pass its `lines` on. It measures each local PCM/float WAV once and multiplies the line’s `volume` (a number or keyframes, whose times and easings are kept) by a constant gain toward the target. Nothing changes unless you call it.

The measurement is gated speech RMS in dBFS (silence under -60 dBFS and quiet parts more than 20 dB below the speech are ignored, every channel is measured separately) and the sample peak over every channel. It is a controllable first step, not LUFS and not a guarantee of equal perceived loudness. The gain is clamped to `maxBoostDb` / `maxCutDb` and kept under `peakCeilingDbfs`; silent or very short recordings get 0 dB. Set `speakers: { name: { offsetDb, targetRmsDbfs } }` per speaker, and `voiceLevel` on a line: `false` to leave it alone, `{ offsetDb }` or `{ targetRmsDbfs }` to adjust it, or `{ gainDb }` for a fixed gain. `matched.report` lists each line’s measured level, gain, and which bound limited it. Results are cached by file content, so replacing a WAV is measured again.

To turn BGM or game audio down while someone speaks, give it `duck={{ under: 'voice' }}`: dialogue voices are in the group `"voice"` unless a line's `duckGroup` says otherwise (`null` for none). To follow only the speech the report found, build its `volume` in `prepare()` with duckingVolume(plan, { volume, to }, { levels: report }) instead; speechIntervals(plan, { levels }) gives the intervals themselves. `packages/cli/examples/with-voice-level-matching.tsx` shows the whole workflow.

## See it with real artwork

`packages/cli/examples/with-dialogue-series.tsx` demonstrates an automatically timed conversation; `packages/cli/examples/with-lip-sync.tsx` shows a PSD portrait with lip sync; `packages/cli/examples/with-voice-folder.tsx` reads its script from a voice folder. On [GitHub](https://github.com/mika-f/Celesta), choose **Code → Download ZIP**, extract the complete archive including assets, and open an entry with **File → Open…**. No source build is needed.
