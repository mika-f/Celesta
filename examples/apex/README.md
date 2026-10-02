# CELESTA / APEX

A one-minute motion film made entirely in Celesta. Eight chapters: a cold open,
a glitch-title tunnel, kinetic typography, a 3D gyroscope, a spectrum, wireframe
cubes, a wave field, and a finale. Every 3D rotation, wave, and spectrum bar is
a pure function of the frame. Original, locally synthesized 120 BPM score whose
arrangement follows the picture; every cut lands on a beat.
1920 × 1080 / 30 fps / 60 s.

The composition is `film.tsx` (scenes in `scenes/`, shared pieces in `components/`); `poster.jpg` is a frame from the finished film.
From the repository root:

```sh
python3 examples/apex/make-score.py
node skills/celesta/scripts/inspect.mjs examples/apex/film.tsx --every 150
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" \
  --react examples/apex/film.tsx examples/apex/apex.mp4
```

The soundtrack and MP4 are generated and ignored by Git. Run the score
generator before opening the composition on a fresh checkout. Bebas Neue and
IBM Plex Mono are referenced from `../afterimage/assets/fonts/` (SIL Open Font
License).
