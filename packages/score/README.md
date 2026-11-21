# @celesta.studio/score

The source of the first-party plugin `score` ([`plugins/score`](../../plugins/score/)),
imported by compositions as `@celesta-plugin/score`. `defineScore()` records
sound effects, drums, and notes against a composition's frames, seconds,
and beats; `score.prepare()` renders them in the entry's `prepare()` to a
48 kHz, 16-bit stereo WAV named by its content's hash; `<ScoreAudio>` plays
it through `<Audio>`.

The synthesizer (`src/dmath.ts`, `src/dsp.ts`, `src/voices.ts`,
`src/render.ts`) uses only arithmetic IEEE 754 rounds exactly, never
`Math.sin`, `Math.exp`, `Math.pow`, or `**`, so the bytes are the same on
every engine and system. `test/score.test.ts` pins them by SHA-256: a change
to any voice, the mixer, or the file must bump `SYNTH_VERSION` in
`src/score.ts`, which renames every cached file.

See the user guide's `@celesta-plugin/score` page and
[`docs/handoff/2026-11-22-score-synth.md`](../../docs/handoff/2026-11-22-score-synth.md).
