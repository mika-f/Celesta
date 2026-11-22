# Sound effects and simple music as data (@celesta-plugin/score)

`@celesta-plugin/score` synthesizes a video's ticks, pops, whooshes, drums,
and simple melodic parts from data written in the entry, so a film needs no
audio files for them. It is a first-party plugin from Celesta's registry:
**never `npm install` it**. A project declares it with
`celesta plugin add score` (`score@<Celesta version>` on a pre-release), when
the user wants it added. Until a release carries it, build it from a Celesta
checkout (`node scripts/plugins/build.mts plugins/score`) and pass
`--plugin-dir <checkout>/plugins/dist` to `Celesta-export` and `inspect.mjs`
(through `CELESTA_PLUGIN_PATH`).

Reach for it when the user wants sound effects on cues (a tick per typed
character, a whoosh into each cut, a pop per word) or a short procedural
bed. Use files (`<Audio src>`) for recorded sound, voices, and real music.

## Contents

- [Write and play a score](#write-and-play-a-score)
- [Time and length](#time-and-length)
- [Calls](#calls)
- [Names](#names)
- [Errors and warnings](#errors-and-warnings)
- [The file, determinism, and checking](#the-file-determinism-and-checking)

## Write and play a score

```tsx
import { Composition, Scenes, defineScenes } from '@celesta.studio/react';
import { typewriterFrames } from '@celesta.studio/text';
import { ScoreAudio, defineScore } from '@celesta-plugin/score';

const SCENES = defineScenes([/* … */]);
const GRID = { bpm: 120 };                         // the same object goes to useBeat(GRID)
const TYPING = { from: 12, framesPerChar: 3 };     // the same object goes to <TypewriterText>

export const score = defineScore({ fps: 30, durationInFrames: SCENES.durationInFrames }, s => {
  s.tempo(GRID);
  s.master({ gain: 0.8, fadeOut: 1.2 });
  s.ticks(typewriterFrames('npm run build', TYPING), { vel: 0.55, vary: 0.6 });
  for (const scene of SCENES.scenes.slice(1)) s.fx('whoosh', { frame: scene.from, reverse: true });
  s.program('lead', 'marimba', { bright: 0.7 });
  s.pattern('lead', 'x.x.x.xX', { beat: 8, repeat: 4, pitch: ['D5', 'F5', 'A5', 'C6'] });
  s.drums({ kick: 'x...x...', hat: '.x.x.x.X' }, { beat: 8, repeat: 4 });
});

export async function prepare() {
  await score.prepare();                           // renders .celesta/score/<sha256>.wav
}

export default function Film() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={SCENES.durationInFrames}>
      <Scenes scenes={SCENES} />
      <ScoreAudio score={score} />                {/* top level: score frame 0 = composition frame 0 */}
    </Composition>
  );
}
```

- Derive every time from the data the picture uses (scene starts,
  `typewriterFrames()`, cue `at` frames, the beat grid), never from numbers
  copied by hand, so moving a scene moves its sound.
- Put `<ScoreAudio>` at the top level, not inside a `<Sequence>`, or the
  score's frames shift. It takes `<Audio>`'s other props (`volume`,
  `duckGroup`, `duck`).
- `score.src` throws until `prepare()` resolved. An entry with its own
  `prepare()` awaits `score.prepare()` in it too.
- `examples/score` in the Celesta repository is a complete film.

## Time and length

Every time is exactly one of: a number or `{ frame }` (composition frame,
fractions allowed), `{ t }` (seconds), `{ beat }` (beats on the grid from
`s.tempo()`, from 0). A length is at most one of `frames`, `dur` (seconds),
`beats`; a note without one lasts a beat after a `beat` start, else 0.5 s.

- `s.tempo({ bpm, beatsPerBar, offset })` takes `useBeat()`'s options;
  `offset` is the frame of beat 0. A later `s.tempo()` changes the grid for
  later calls only. `s.beatFrame(n)` gives the frame of beat `n`.
- `s.run()`'s `step` is in the unit of its start; `s.pattern()` and
  `s.drums()` steps are always beats (`step`, default 0.5).

## Calls

| Call | Notes |
| --- | --- |
| `s.program(track, patch, { gain, pan, reverb, bright })` | Required before a track's notes. Notes copy the settings when recorded. |
| `s.note(track, pitch, { …time, …length, vel })` | Pitch: MIDI number (60 = C4) or `'C4'`, `'F#3'`, `'Bb2'`. |
| `s.chord(track, pitches, { …time, …length, vel })` | |
| `s.run(track, pitches, { …time, step, cresc, vel })` | `cresc` is added to `vel` by the last note. |
| `s.pattern(track, 'x.X.o.xx', { …time, step, repeat, pitch, beats, vel })` | `x` note, `X` accent, `o` ghost, `.`/`-` rest; spaces and `\|` ignored. `pitch` may be a list taken in turn. |
| `s.drums({ kick: 'x...', hat: '.x.x' }, { …time, step, repeat, vel })` | Each line repeats on its own. |
| `s.drum(name, time \| { …time, vel, pitch, pan, gain, reverb })` | |
| `s.fx(name, time \| { …time, dur, vel, pitch, pan, gain, reverb, reverse })` | `dur` (the whole length, which the sound ends within) only for `sparkle`, `whoosh`, `revswell`, `roll`; `reverse` only for `whoosh`. |
| `s.ticks(times, { sound, vary, vel, pitch, pan, gain, reverb })` | One effect per time; `vary` is a seeded pitch wander in semitones. |
| `s.master({ gain, fadeIn, fadeOut, reverb })` | Fades in seconds. Default gain 0.8. |

`vel` 0–1 (default 0.8), `pan` -1–1, `reverb` 0–1 (the send to one shared
room), `pitch` in semitones.

## Names

- Patches: `marimba` `vibes` `glock` `ep` `keys` `pluck` `bass` `pad`
- Drums: `kick` `snare` `clap` `sidestick` `hat` `openhat` `shaker` `tamb` `woodblock` `cowbell` `tom` `crash` `ride`
- Effects: `tick` `click` `doubleclick` `pop` `ding` `chime` `sparkle` `whoosh` `revswell` `roll` `boing` `thud`

A `whoosh` with `reverse: true` and `revswell` end on their time (they
swell into a cut or hit); everything else starts on it. The part of a
swell before 0 is left out.

`PATCH_NAMES`, `DRUM_NAMES`, and `EFFECT_NAMES` export the lists.

## Errors and warnings

- Thrown when the module loads, naming the call: an unknown name (with the
  nearest one: `s.fx('wosh'): unknown effect "wosh"; did you mean
  'whoosh'?`), a character a pattern does not know, `beat` before
  `s.tempo()`, a note on a track without `s.program()`, two time keys, a
  value out of range, and any event before 0.
- A sound starting at or after the composition's end is dropped with one
  warning per call (a reversed whoosh or `revswell` starts `dur` before its
  time, so one timed on the end still plays) (`score.warnings`, stderr:
  `celesta score: s.drums(kick): 6 events start at or after the end (2 s), first at 2 s; dropped`).
  Fix the count (`repeat`) rather than ignoring it. `strict: true` on
  `defineScore()` makes it an error. Sound still ringing at the end is cut.

## The file, determinism, and checking

- The WAV is 48 kHz, 16-bit stereo, exactly the composition's length, named
  by a SHA-256 of the resolved score and the synthesizer's version, in
  `.celesta/score/` beside the entry (`prepare({ cacheDir })`). Unchanged
  scores are not written again. Keep `.celesta/` out of Git.
- The bytes are identical on every run and system (only exactly rounded
  arithmetic and seeded noise). `defineScore(…, { seed })` changes the noise.
- Only the desktop app and `Celesta-export` play it; `@celesta.studio/web`
  has no file system for `prepare()`.
- `inspect.mjs` lists the score as an audio clip of the WAV. `score.events`
  lists every sound in seconds and frames, for checking timings against the
  picture without listening. Say you could not listen to it, and export an
  MP4 (or a short `--from`/`--to` span) for the user to check by ear.
