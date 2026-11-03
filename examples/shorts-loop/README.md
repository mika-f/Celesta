# CELESTA / SHORTS LOOP

A 12.8-second vertical loop for YouTube Shorts and TikTok, made to be watched
on repeat: the last frame runs into the first, picture and sound alike, so
the player's restart is invisible. Four custom WGSL shaders from
`@celesta.studio/shader` do the work. `silk` computes the whole background per
pixel, `kaleido` folds a group of plain Celesta shapes into a kaleidoscope,
`sheen` runs a band of light across the copy, and `lens` sends a refracting
ring across the finished frame on every bar.
1080 × 1920 / 30 fps / 12.8 s.

The composition is `film.tsx` (the one scene in `scenes/`, its pieces in
`components/`, the shaders in `shaders/`); `poster.jpg` is a frame from the
finished film.

The commands below need Celesta. `inspect.mjs` uses the React runtime of a
source build, or of an installed Celesta app when there is none. To build
both the runtime and the exporter from a fresh checkout:

```sh
pnpm install && pnpm --dir packages/react run codegen && pnpm run build:runtime
cargo build -p celesta-exporter --release
```

Then, from the repository root:

```sh
python3 examples/shorts-loop/make-score.py
node skills/celesta/scripts/inspect.mjs examples/shorts-loop/film.tsx --every 30
target/release/celesta-exporter --loop --react examples/shorts-loop/film.tsx examples/shorts-loop/shorts-loop.mp4
```

With a macOS app recent enough to take `--loop`, skip the build and export
with `/Applications/Celesta.app/Contents/MacOS/Celesta-export` in place of
`target/release/celesta-exporter`.

The soundtrack and `shorts-loop.mp4` are generated and ignored by Git. Run
the score generator before opening the composition on a fresh checkout. Bebas Neue and IBM Plex Mono are referenced from
`../afterimage/assets/fonts/` (SIL Open Font License).

## How it loops

- **One length for everything.** 384 frames are four bars of 4/4 at 75 BPM:
  24 frames per beat, 96 per bar. `make-score.py` reads the tempo, bar count,
  and frame rate from `constants.ts` and refuses values that do not divide
  into whole frames and samples.
- **Periodic motion only.** Every animated value is built from `loop.ts`:
  `theta(frame)` runs from 0 towards 2π over the loop, `wave()` turns a
  whole number of times per loop (and throws on a fraction), and `hit()` and
  `within()` repeat every beat or bar. The shaders get `theta`, never a
  time in seconds, and use it only in whole multiples. So frame 384, if it
  existed, would be frame 0, and the step from 383 to 0 is an ordinary step.
- **Nothing marks the start.** No fade in or out, no event that happens only
  once. The ring on bar one is the same as on bars two to four, so the loop
  point looks like any other bar line.
- **The score is a ring.** Notes, pad tails, and delay echoes that ring past
  the end of bar four wrap to the start of bar one, as they would on a second
  pass.
- **The AAC soundtrack too.** An AAC encoder starts from silence, which
  smears the first kick of the soundtrack for about 10 ms, and a looping
  player repeats that smear on every pass. `--loop` makes the exporter
  encode the end of the score ahead of its start, which the MP4's edit list
  then hides, so the restart sounds like any other bar line.

## Layout

The picture fills the whole frame. Only the words keep clear of the
platforms' UI: nothing to read sits in the bottom fifth (from y = 1536), near
the right edge, or under the top tabs. The copy, "LIGHT, FOLDED.", names what
the shaders do, so it reads the same from wherever a viewer joins the loop.
The frame is already at full strength on frame 0, and the first ring crosses
the copy within the first second.
