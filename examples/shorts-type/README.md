# SHORTS TYPE

English | [日本語](README.ja.md)

A 28-second vertical (1080 × 1920, 30 fps) piece of Japanese kinetic
typography. A short manifesto that opens with 「動画を、コードで書く。」
("Write video in code.") flows into a narrow column on the beat of a 120 BPM
score.

The narrower the column, the more phrase line breaking, measured and fitted
text, and `<Span>` emphasis show their worth; each scene is a working example
of one of them. The same structure with English copy is
[`shorts-type-en`](../shorts-type-en/).

- `film.tsx` — the React entry to open in Celesta (File → Open…). Scenes are in
  `scenes/`, shared parts in `components/`, the scene order in `timeline.ts`,
  and everything measured in `prepare()` in `measure.ts`.
- `make-score.py` — generates the score with Python's standard library.
- `poster.jpg` — a still from the exported video.

![poster](poster.jpg)

## Scenes

120 BPM: one beat is 15 frames, one bar 60. Every scene is two bars and starts
on a downbeat.

| Time | What happens | APIs |
| --- | --- | --- |
| 0–4 s | 「動画を、コードで書く。」 The first frame already shows the opening line; the headline is up by frame 12 | `TextReveal`, `<Span>`, `measureText` (the caret's position), `Camera` |
| 4–8 s | 「すべてのフレームは、フレーム番号の関数。」 ("Every frame is a function of its number."), one line per beat, then the frame number itself | `TextReveal`, `useCurrentFrame` |
| 8–12 s | The same sentence with `lineBreak: 'normal'` above and `'phrase'` below, while the column narrows from 760 to 400 px every other beat | `lineBreak`, `maxWidth`, `useCue` |
| 12–16 s | A box that changes shape every other beat; the same copy refills it as large as it fits | `fitText`, `useCue` |
| 16–20 s | The lit phrase moves every beat; the highlight bars are placed from the measured glyphs | `<Span>`, `measureText`, `useCue`, `interpolateColor` |
| 20–24 s | One word per beat: 「拍に／合わせて／言葉を／落とす。／書いて、／保存して、／すぐ／確かめる。」 ("On the beat, drop the words. Write, save, check right away.") | `useBeat` |
| 24–28 s | The opening line and the wordmark, on the opening background, so a looping player cuts back into the hook without a jump | `TextReveal` |

The HUD at the top (scene number and a four-beat metronome) is drawn with
`cueAt()` and `useBeat()`, and with `blendMode="difference"` so it reads on
every background colour.

## Effects

Almost everything besides the copy is computed from the beat and the scene
cuts.

- **Halftone** (`halftone.ts`): a `@celesta/shader` filter on the background
  rect. Dots grow towards the bottom, drift up, and swell where a ring rises
  from below on every beat.
- **Finishing pass** (`fx.ts`, `components/Fx.tsx`): a shader on a `Group`
  around the whole frame. Every beat kicks an RGB split that grows with the
  square of the distance from the centre, so copy in the middle stays crisp;
  every cut gets 7 frames of horizontal glitch slices and a 4-frame flash. That
  is one flash every four seconds, well under three per second.
- **Tapes** (`components/Tape.tsx`): two slanted bands across the bottom scroll
  the scene's line and API. They are decoration, so they may run through the
  area the platform UI covers.
- **Punch-in**: each scene's copy springs in from 0.9× and starts blurred. It
  grows from smaller, so it never leaves the safe area.
- **Speed lines** (`components/Burst.tsx`): in the BEAT scene, one `Path` of
  wedges flies out per word, and the word drops in with motion blur.

`make-score.py` hits the same frames: an impact and a glitch stutter on every
cut, and a kick on every beat.

## Safe area

Shorts and TikTok draw their UI over the bottom ~20% of the frame (caption,
progress bar) and along the right edge (like, comment and share buttons).
Copy meant to be read (headlines, body, code) stays inside `SAFE` in
`constants.ts` (top 192, right 200, bottom 384, left 72 px). Backgrounds, the
halftone, the tapes and the speed lines fill the whole frame.

Each scene draws its full-frame layers in `components/Stage.tsx`, and puts only
the copy inside a `<SafeArea>` and a `<Camera>`. The camera zooms about the
centre of the safe area, so a push-in never pushes copy towards the UI. The
text column (`COL`, 760 px wide) is a little narrower than the safe area,
leaving room for a zoom of a few percent.

To see the covered areas, turn on the `guides` project property:

```sh
Celesta-export --react examples/shorts-type/film.tsx --props '{"guides":true}' --every 30 --contact-sheet sheet.png
```

## Measuring and inspect.mjs

`measureText()` and `fitText()` run once in `prepare()` and keep their results
in module variables (`measure.ts`). `<Font>` is not loaded yet at that point,
so the same Google Fonts URL is passed in `fonts`.

`inspect.mjs` cannot shape text, so measuring fails there. Every value starts
as an estimate (Japanese glyphs one em wide), and when measuring fails those
estimates keep every frame evaluable. Check how text looks with PNG frames.

## Regenerate

The WAV and MP4 are ignored by Git. After cloning, generate the score first:

```sh
python3 examples/shorts-type/make-score.py
node skills/celesta/scripts/inspect.mjs examples/shorts-type/film.tsx --every 15
Celesta-export --react examples/shorts-type/film.tsx shorts-type.mp4
```

From a source checkout, replace `Celesta-export` with
`cargo run -p celesta-exporter --release --`. The fonts (Noto Sans JP,
JetBrains Mono; both SIL Open Font License) load from Google Fonts on first
run and are cached. The video and score are original procedural work made for
this repository.
