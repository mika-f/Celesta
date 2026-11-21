# Score

Twelve seconds at 30 fps, 120 BPM, whose every sound is written as data in
`score.ts` and synthesized by `@celesta-plugin/score`: there is no audio
file in the folder.

- `film.tsx`: the composition. `prepare()` renders the score, and
  `<ScoreAudio>` plays it.
- `score.ts`: the score, written against the same timeline as the picture.
- `timeline.ts`: the three scenes (`defineScenes`).
- `constants.ts`: size, the beat grid (`GRID`, also passed to `useBeat()`),
  and the typing options (`TYPING`, also passed to `<TypewriterText>`).
- `scenes/`: Type, Beat, and Outro.

## What it shows

- **Type**: a command types itself, and `s.ticks(typewriterFrames(…))`
  ticks on every character, with the same options the scene types with; a
  `ding` when it is done.
- **Into each cut**: a whoosh with `reverse: true` swells into the first
  frame of Beat and Outro, read from the scene list, and a kick lands there.
- **Beat**: drums, a bass pattern, and a marimba pattern on `GRID`, and four
  squares bumping on the same beats through `useBeat(GRID)`.
- **Outro**: a pop and a bell under each word, a beat apart, and the mix
  fading out over the last 1.2 seconds.

Move a scene's length in `timeline.ts` or the typing speed in
`constants.ts`, and the sounds move with the picture.

## Run it

`score` comes from Celesta's plugin registry. Until a release carries it,
build it from this checkout and name the folder:

```sh
node scripts/plugins/build.mts plugins/score
Celesta-export --react examples/score/film.tsx score.mp4 --plugin-dir "$PWD/plugins/dist"
```

The WAV is written to `examples/score/.celesta/score/<hash>.wav` on the
first run and reused while the score is unchanged.
