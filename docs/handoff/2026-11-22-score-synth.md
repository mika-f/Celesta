# Score: sound written as data (2026-11-22)

A first-party plugin, `score`, imported as `@celesta-plugin/score`, that
synthesizes a film's sound effects, drums, and simple music from data in the
entry, so a film needs no audio files for ticks, pops, and whooshes. Modelled
on the "Sound API" (`kit.score`) of Claude's Motion film spec, adapted to
Celesta's frames, beat grid, and `prepare()`.

## Scope (agreed with the maintainer)

- Built-in effects (`tick`, `click`, `doubleclick`, `pop`, `ding`, `chime`,
  `sparkle`, `whoosh` and its reversed form, `revswell`, `roll`, `boing`,
  `thud`), eight melodic patches with notes, chords, runs, and patterns,
  thirteen drums with pattern strings, a tempo and beat grid, master gain and
  fades.
- Times in frames, seconds, or beats, so scores compose with
  `typewriterFrames`, `useBeat`/`beatAt`, `defineScenes`/`computeSeries`
  starts, and `useCue` lists.
- Unknown names are errors (Motion is silent), events past the end are
  dropped with a warning, and the output is bit-identical between runs.
- Shape: a component-only first-party plugin rendering a WAV in `prepare()`,
  cached by content hash, played through `<Audio>`. No Rust or composition
  model change.

## Design

- `defineScore({ fps, durationInFrames, seed?, strict? }, s => …)` runs the
  builder once at module scope (`src/score.ts`, `buildScore`). Every call
  resolves its times to seconds at once, against the grid set by the latest
  `s.tempo()`, and checks its names and values; errors name the call
  (`s.fx('wosh'): unknown effect "wosh"; did you mean 'whoosh'?`). The
  result's `events` are sorted by time, stably.
- Times: a number or `{ frame }`, `{ t }`, `{ beat }`, exactly one. Lengths:
  `frames`, `dur`, `beats`, at most one. `s.tempo()` takes `BeatOptions`
  (`bpm`, `beatsPerBar`, `offset` in frames), so one object drives
  `useBeat()` and the score. `run`'s `step` follows its start's unit;
  pattern and drum steps are beats (default 0.5).
- Events before 0 throw. Events whose start rounds to the last sample or
  later are dropped and counted per call; one warning per call goes to
  `score.warnings` and stderr (`console.warn`, never stdout, which carries
  the bridge protocol), or throws with `strict`. The reversed whoosh and
  `revswell` are anchored at their end; their start may be before 0 and is
  trimmed.
- `score.prepare()` (`src/define.ts`, `src/cache.ts`) writes
  `<cacheDir>/<sha256>.wav`, `cacheDir` defaulting to `./.celesta/score`
  resolved from `CELESTA_REACT_ENTRY_DIR` like `prepareNarration()`'s. The
  hash covers `SYNTH_VERSION`, the sample rate, the duration, the seed, the
  master, and every resolved event. A file of the expected size is a hit and
  renders nothing; a miss renders, writes a temporary file, and renames it.
  Node's modules are imported dynamically, so a composition importing the
  plugin still loads without a file system (`@celesta.studio/web`, where
  `prepare()` then fails and `score.src` throws its "call prepare()" error).
- `<ScoreAudio score {...audioProps}>` is `<Audio src={score.src}>`.

### Determinism

- The synthesizer (`src/dmath.ts`, `src/dsp.ts`, `src/voices.ts`,
  `src/render.ts`) uses only `+ - * /`, `Math.floor`, `Math.round`,
  `Math.abs`, `Math.min`/`max`, and `Math.imul`, which every engine computes
  exactly. `sin` (range reduction and a degree-15 Taylor polynomial, about
  1e-12) and `exp` (reduction by ln 2, a 14-term series, exact powers of 2)
  stand in for `Math.sin`/`Math.exp`; there is no `Math.pow` or `**`.
- Noise is Mulberry32, seeded per event from a hash of the score's seed and
  the event's kind, name, start sample, and repeat count at that sample, so
  adding an event elsewhere does not change another's noise.
- Events mix in time order into `Float64Array` buses, then a Freeverb-style
  room (8 combs and 4 allpasses per channel) on the send, the master gain,
  linear fades, a soft limiter above 0.85, and rounding to 16 bits. No
  dither.
- `test/score.test.ts` pins the SHA-256 of a score that sounds every voice.
  The Plugins CI job runs the package's tests on Linux, macOS, and Windows
  for pull requests that touch it, so the pin checks the bytes across
  systems. Changing any voice or the mixer changes it: bump `SYNTH_VERSION`
  and the pin together.

### Voices

Each voice renders one event at unit velocity into a mono buffer; the mixer
applies velocity, gain, equal-power pan, and the send. Levels were set by
measuring each voice's peak at velocity 1 (about 0.5 for drums and effects,
0.8 for the kick, 0.25 to 0.5 for patches); they were not tuned by ear.

- Patches: `marimba`, `vibes` (with tremolo), `glock` are decaying sine
  partials with a noise knock; `ep` is two-operator phase modulation with a
  14:1 tine; `keys` is stretched harmonics with a hammer; `pluck` is
  Karplus–Strong; `bass` is a sine and a filtered saw; `pad` is three
  detuned PolyBLEP saws through two one-pole lowpasses with a slow attack.
- Drums are pitch-swept sines and filtered noise (state-variable filters).
- Effects as listed in the user guide.

## Files

- `packages/score` (`@celesta.studio/score`, private): the module and its
  Vitest tests. `plugins/score/plugin.json` points at its `src/index.ts`. It
  is not a dependency of `@celesta.studio/cli`: there is no old import to
  keep, and the core names no first-party plugin.
- Registered with the other component plugins: `FOLLOWS_CELESTA`
  (`scripts/plugins/version.mts`), `COMPONENTS`
  (`scripts/package-media-plugins.py`), `plugin-release.yml`'s `--plugins`,
  and their tests.
- `scripts/typecheck-examples.mjs` also builds and stages a first-party
  plugin an example imports without declaring it.
- `.gitignore`: `**/.celesta/score/`.
- `examples/score`: twelve seconds, three scenes. Ticks follow the same
  `typewriterFrames()` options as `<TypewriterText>`, a reversed whoosh and
  a kick land on each later scene's start, Beat's squares and its groove
  share one `GRID`, and Outro pops a word a beat. It declares no
  `plugins.dependencies` yet: a declared plugin without a `celesta.lock`
  stops `Celesta-export` and MCP, and there is nothing to lock until the
  plugin is published. Run it with `--plugin-dir plugins/dist`.
- User guide: a `@celesta-plugin/score` page (en, ja) under packages, a
  pointer in the media guide, and the plugins page's list. Skill:
  `skills/celesta/references/score.md`, linked from `SKILL.md`.

## Checks

- `pnpm --dir packages/score test`: type check and 21 tests (math accuracy,
  repeat determinism, the pinned hash, the WAV header and length, sample
  placement from frames, seconds, and beats, agreement with `beatAt()`,
  scene starts, patterns, runs, the reversed whoosh's end, trimming, the
  fade, every error, the warnings and `strict`, the cache, `<ScoreAudio>`).
- `pnpm --dir scripts/plugins test` (bump-version and inputs list `score`),
  `python3 -m unittest scripts/test_package_media_plugins.py`,
  `node scripts/typecheck-examples.mjs score`,
  `pnpm --dir packages/website test`.
- EXPORT-CHECK

## Not done

- Nobody has listened to the result: the voices' levels and timbres were set
  from measured peaks only.
- `@celesta.studio/web` does not play scores.
- `examples/shorts-type`, `shorts-type-en`, `shorts-tips`, and `delta`
  still generate their scores with Python (`make-score*.py`). Moving
  `shorts-type` to the plugin fits it well, but needs the plugin published
  and the example locked first.
- No tempo ramps, MIDI import, or sampled sounds.

## Left for the maintainer

1. After the next release publishes `score`, add
   `"plugins": { "dependencies": { "score": "<version>" } }` to
   `examples/score/celesta.config.json`, run `celesta plugin install` there,
   and commit the `celesta.lock`.
2. If the registry keeps an allow-list of first-party ids, add `score`.
