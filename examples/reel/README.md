# Celesta — "Code is the cut." reel

Open [`film.tsx`](./film.tsx) in Celesta to preview a
32-second, 1920×1080, 30 fps kinetic-type reel cut to a 120 BPM score (one
beat = 15 frames). Chapters: a one-bar cold open, a one-word-per-beat
manifesto, a formula-driven cell field, a code editor whose preview reloads on
every save, a scrolling timeline, an export counter, and a logo resolve.

Fonts (Space Grotesk, JetBrains Mono) load from Google Fonts on first run and
are cached. The score is original and deterministic,
generated with Python's standard library:

```sh
python3 examples/reel/make-music.py
```

Export from the repository root:

```sh
PKG_CONFIG_PATH=/opt/homebrew/opt/ffmpeg@7/lib/pkgconfig \
  cargo run -p celesta-exporter --release -- \
  --react examples/reel/film.tsx examples/celesta-reel.mp4
```
