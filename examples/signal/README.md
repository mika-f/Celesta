# CELESTA / SIGNAL

A 16-second editorial motion piece made entirely in Celesta. Frame counts
accelerate into an animated signal; source code gives way to a live preview,
a rotating radial composition, frame-accurate scrubbing, and a hard-cut
title. Original, locally synthesized 120 BPM score. 1920 × 1080 / 30 fps.

The composition is `film.tsx` (scenes in `scenes/`, shared pieces in `components/`); `poster.jpg` is a frame from the finished film.
From the repository root:

```sh
python3 examples/celesta-signal/make-score.py
node skills/celesta/scripts/inspect.mjs examples/celesta-signal/film.tsx \
  --frames 0,50,80,170,230,275,320,370,410,460
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" \
  --react examples/celesta-signal/film.tsx examples/celesta-signal/celesta-signal.mp4
```

The soundtrack and MP4 are generated and ignored by Git. Run the score
generator before opening the composition on a fresh checkout. IBM Plex Mono
is referenced from `../afterimage/assets/fonts/` (SIL Open Font License).
