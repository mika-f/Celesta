`@celesta-plugin/score` writes a video's sound effects and simple music as data: ticks, pops, and whooshes, a few melodic patches, drums, and a beat grid. The score is recorded against the same frames, seconds, and beats the picture uses, rendered in `prepare()` to a WAV file, and played by an `<Audio>`, so the preview and the export play the same file, and the file is the same, byte for byte, on every run and every system. No audio files are needed.

> [!NOTE]
> **Add the plugin**
> `@celesta-plugin/score` is a first-party plugin from Celesta's [plugin registry](/en/docs/plugins/#registry-plugins). Add it to the project with the `celesta` command, which declares it in `celesta.config.json`, locks it in `celesta.lock`, and downloads it:
>
> ```sh
> celesta plugin add score
> ```
>
> The plugin's versions follow Celesta's; on a pre-release of Celesta, name that version, as in `celesta plugin add score@0.10.0-alpha.4`. Until a release carries it, build it from a checkout with `node scripts/plugins/build.mts plugins/score` and pass `--plugin-dir plugins/dist` to `Celesta-export`, or put that folder in `CELESTA_PLUGIN_PATH`.

The score is rendered with Node's file system, so it plays in the desktop app and `Celesta-export`, not in the browser playground (`@celesta.studio/web`).

## Write a score

`defineScore()` runs its builder once, when the module loads, and checks every name and value as they are recorded. Render it in `prepare()`, and place `<ScoreAudio>` at the composition's top level.

```tsx
import { Composition, defineScenes } from '@celesta.studio/react';
import { typewriterFrames } from '@celesta.studio/text';
import { ScoreAudio, defineScore } from '@celesta-plugin/score';

const scenes = defineScenes([
  { id: 'intro', durationInFrames: 120 },
  { id: 'main', durationInFrames: 240 },
]);
const grid = { bpm: 120 }; // also for useBeat(grid)
const typing = { from: 12, framesPerChar: 3 };

export const score = defineScore({ fps: 30, durationInFrames: scenes.durationInFrames }, s => {
  s.tempo(grid);
  s.master({ gain: 0.8, fadeOut: 1.5 });
  s.ticks(typewriterFrames('npm run build', typing)); // one tick per character
  s.fx('whoosh', { frame: scenes.scenes[1].from, reverse: true }); // swells into the cut
  s.program('lead', 'marimba', { pan: -0.2, bright: 0.7 });
  s.run('lead', ['C5', 'D5', 'E5', 'G5'], { beat: 8, step: 0.5, cresc: 0.2 });
  s.drums({ kick: 'x...x...', hat: '.x.x.x.X' }, { beat: 8, repeat: 4 });
});

export async function prepare() {
  await score.prepare();
}

export default function Film() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={scenes.durationInFrames}>
      {/* … */}
      <ScoreAudio score={score} />
    </Composition>
  );
}
```

`examples/score` in the repository is a twelve-second film made this way.

## Timing

Every call takes a time as exactly one of:

| Key | Meaning |
| --- | --- |
| a number, or `frame` | A frame of the composition; fractions are fine (beats at 128 BPM fall between frames). |
| `t` | Seconds from the start. |
| `beat` | A beat on the grid set by `s.tempo()`, counted from 0. |

Lengths take at most one of `frames`, `dur` (seconds), or `beats`. A note without one lasts a beat when it starts on `beat`, and half a second otherwise.

`s.tempo({ bpm, beatsPerBar, offset })` takes the same options as `useBeat()` and `beatAt()`, so passing one object to both keeps the picture and the sound on one grid; `offset` is the frame beat 0 lands on. Calling it again changes the grid for the calls after it. `s.beatFrame(n)` returns the frame a beat lands on. Scene starts from `defineScenes()` or `computeSeries()`, cue frames from `useCue()` lists, and `typewriterFrames()` are all frames, so they go in as they are.

## Calls

| Call | What it records |
| --- | --- |
| `s.program(track, patch, { gain, pan, reverb, bright })` | Gives a track one of the patches. Notes take the track's settings when they are recorded. |
| `s.note(track, pitch, { …time, …length, vel })` | One note. A pitch is a MIDI number (60 is C4) or a name: `'C4'`, `'F#3'`, `'Bb2'`. |
| `s.chord(track, pitches, { …time, …length, vel })` | Notes together. |
| `s.run(track, pitches, { …time, step, cresc, vel })` | Notes at even steps. `step` is in the unit of the start: frames after `frame`, seconds after `t`, beats after `beat`. `cresc` is added to `vel` by the last note. |
| `s.pattern(track, 'x.X.o.xx', { …time, step, repeat, pitch, beats, vel })` | One character per `step` beats (half a beat by default): `x` a note, `X` an accent, `o` a ghost, `.` or `-` a rest; spaces and `\|` are ignored. `pitch` may be a list, taken in turn. |
| `s.drums({ kick: 'x...', hat: '.x.x' }, { …time, step, repeat, vel })` | Drum lines in the same characters; each line repeats on its own. |
| `s.drum(name, time or { …time, vel, pitch, pan, gain, reverb })` | One drum hit. |
| `s.fx(name, time or { …time, dur, vel, pitch, pan, gain, reverb, reverse })` | One effect. `pitch` moves it in semitones. `dur` sets the length of `sparkle`, `whoosh`, `revswell`, and `roll`. |
| `s.ticks(times, { sound, vary, vel, pitch, pan, gain, reverb })` | One effect (`tick` by default) per time. `vary` lets each tick's pitch wander by up to that many semitones, the same way every run. |
| `s.master({ gain, fadeIn, fadeOut, reverb })` | The whole mix's level, fades in seconds, and the room's level. |

`vel` is 0 to 1 and defaults to 0.8. `pan` is -1 (left) to 1 (right), and `reverb` is the share sent to the room, 0 to 1.

| Kind | Names |
| --- | --- |
| Patches | `marimba` `vibes` `glock` `ep` `keys` `pluck` `bass` `pad` |
| Drums | `kick` `snare` `clap` `sidestick` `hat` `openhat` `shaker` `tamb` `woodblock` `cowbell` `tom` `crash` `ride` |
| Effects | `tick` `click` `doubleclick` `pop` `ding` `chime` `sparkle` `whoosh` `revswell` `roll` `boing` `thud` |

A sound starts on its time, except a `whoosh` with `reverse: true` and `revswell`, which swell into it and stop there, for a cut or a hit. Their start may fall before 0; the part before 0 is left out.

## Mistakes

- An unknown patch, drum, or effect, a character a pattern does not know, a `beat` before `s.tempo()`, a note on a track without `s.program()`, and a value out of range throw, naming the call and, for a misspelled name, the nearest one: `s.fx('wosh'): unknown effect "wosh"; did you mean 'whoosh'?`.
- An event before 0 throws.
- An event that starts at or after the composition's end is dropped, with one warning per call in `score.warnings` and on stderr, for example when a `repeat` runs past the end. Pass `strict: true` to `defineScore()` to throw instead. Sound still ringing at the end is cut there.

## The file

- `prepare()` writes a 48 kHz, 16-bit stereo WAV exactly as long as the composition to `.celesta/score/<sha256>.wav` beside the entry (`prepare({ cacheDir })` to change it). The name is a hash of the resolved events, the master settings, the seed, and the synthesizer's version, so an unchanged score is not written again and a changed one never plays a stale file.
- The synthesizer uses only arithmetic that IEEE 754 rounds exactly, and seeded noise, so the bytes do not depend on the system or the Node.js version. `defineScore(…, { seed })` changes the noise.
- `score.events` lists every sound, resolved to seconds and frames, for drawing something in step with it. `score.toWav()` returns the bytes without writing them.
- `<ScoreAudio>` takes every other `<Audio>` prop, such as `volume`, `duckGroup`, and `duck`.
