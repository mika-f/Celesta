# Celesta-chan — a concert promo

Open [`film.tsx`](./film.tsx) in Celesta (File → Open…) to preview a 52.8-second,
1920×1080, 30 fps promo staged as a concert. Celesta-chan, an AI orchestra
conductor named after the instrument, leads one show from the overture to
the curtain call:

1. **Overture** — the curtain is down and the program sits on the music
   stand; the baton taps three times, the curtain rises: "READY?"
2. **Tonight's conductor** — Celesta-chan's page in the concert program.
3. **Mvt. I WRITE** — a component is typed, saved, and the preview reloads.
4. **Mvt. II LAYER** — five tracks join the score one part at a time.
5. **Render** — `Celesta-export` renders the film from one command.
6. **MC time** — four lines between songs, over the audience's penlights.
7. **Built in** — what `@celesta/react` brings, one card per beat.
8. **On the beat** — the score's own waveform runs under a playhead.
9. **Tonight's stats** — a frame counter, then the audience breathes in: "せーの…"
10. **BRAVO!!** — the drop and a standing ovation.
11. **Curtain call** — thank-yous, how to run Celesta, "Fin.", and the curtain falls.

The cut follows a 150 BPM score, so one beat is exactly 12 frames and one bar
is 48. A note pops on every beat listed in `hits.json`, which the score
also uses for a glockenspiel ting.

## Assets

- `chara/*.png`: Celesta-chan's portraits. They were generated with ChatGPT's
  image generation (through Codex) from one character sheet, on a chroma-key
  background that was then removed. The character settings, every prompt, and
  the steps are in [`chara/README.md`](./chara/README.md).
- `music.wav` and `hits.json`: an original kawaii future bass cue (chiptune
  lead, glockenspiel, side-chained supersaw stabs, formant vocal chops, and a
  celesta-like bell), synthesised with Python's standard library:

  ```sh
  python3 examples/celesta-chan/make-music.py
  ```

  `music.wav` is not checked in; run the script once before opening the film.
- Fonts (Dela Gothic One, M PLUS Rounded 1c, JetBrains Mono, DotGothic16)
  load from Google Fonts on first run and are cached.

## Export

Install Celesta from the [releases page](https://github.com/mika-f/Celesta/releases)
(macOS: the `.dmg`; Windows: the setup `.exe` or the portable `.zip`). Then
open `film.tsx` and choose **Export…**, or use the bundled command-line
exporter from the repository root:

```sh
# macOS
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" \
  --react examples/celesta-chan/film.tsx celesta-chan.mp4
```

```powershell
# Windows (installer)
& "$env:LOCALAPPDATA\Programs\Celesta\Celesta-export.exe" `
  --react examples/celesta-chan/film.tsx celesta-chan.mp4
```
