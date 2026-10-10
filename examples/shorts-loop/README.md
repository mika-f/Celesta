# CELESTA / SHORTS LOOP

A 12.8-second vertical loop for YouTube Shorts and TikTok, made to be watched
on repeat: the last frame runs into the first, picture and sound alike, so
the player's restart is invisible. Three custom WGSL shaders from
`@celesta/shader` do the work. `silk` computes the whole background per
pixel, `kaleido` folds a group of plain Celesta shapes into a kaleidoscope,
and `lens` sends a refracting ring across the finished frame on every bar.
1080 × 1920 / 30 fps / 12.8 s.

The composition is `film.tsx` (the one scene in `scenes/`, its pieces in
`components/`, the shaders in `shaders/`); `poster.jpg` is a frame from the
finished film. From the repository root:

```sh
python3 examples/shorts-loop/make-score.py
node skills/celesta/scripts/inspect.mjs examples/shorts-loop/film.tsx --every 30
python3 examples/shorts-loop/render.py
```

`render.py` exports with `target/release/celesta-exporter` (`--exporter`
picks another build) and needs `ffmpeg`. It writes `shorts-loop.mp4`. The
soundtrack and MP4 are generated and ignored by Git. Run the score generator
before opening the composition on a fresh checkout. Bebas Neue and IBM Plex
Mono are referenced from `../afterimage/assets/fonts/` (SIL Open Font
License).

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
  smears the first kick of the exporter's soundtrack for about 10 ms, and a
  looping player repeats that smear on every pass. `render.py` keeps the
  exporter's picture and re-encodes the score with the loop's last four AAC
  frames ahead of its start, which the MP4's edit list then hides.

## Layout

The picture fills the whole frame. Only the words keep clear of the
platforms' UI: nothing to read sits in the bottom fifth (from y = 1536), near
the right edge, or under the top tabs. The copy, "NO START. NO END.", reads
the same from wherever a viewer joins the loop. The frame is already at full
strength on frame 0, and the first ring crosses the copy within the first
second.
