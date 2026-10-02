# Dialogue demo assets

Assets for `packages/react/examples/with-dialogue-series.tsx`: two original
characters, しずく (Shizuku, a water drop) and こむぎ (Komugi, a bread bun),
talking through `script.json`.

| Path | What | License |
| --- | --- | --- |
| `portraits/*.png` | Portraits (normal, smile) and shared lip-sync mouths, drawn with Celesta by `source/draw-portraits.tsx` | Same as this repository (MIT OR Apache-2.0) |
| `script.json` | Lines, kana readings for lip sync, and scenes | Same as this repository |
| `voices/*.wav` | One recording per line, synthesized with Open JTalk | CC BY 3.0 (see below) |
| `source/` | Scripts that regenerate everything above | Same as this repository |

## Voice credits

The voices were synthesized with [Open JTalk](https://open-jtalk.sourceforge.net/)
(Modified BSD license) using HTS voices released by the
[MMDAgent](https://www.mmdagent.jp/) project, part of MMDAgent_Example-1.8:

- しずく: HTS Voice "Mei" (`mei_normal`, `mei_happy`),
  Copyright (c) 2009-2018 Nagoya Institute of Technology, Department of
  Computer Science.
- こむぎ: HTS Voice "Takumi" (`takumi_normal`, `takumi_happy`),
  Copyright (c) 2017-2018 Nagoya Institute of Technology, Department of
  Computer Science.

Both voices are licensed under the
[Creative Commons Attribution 3.0](https://creativecommons.org/licenses/by/3.0/)
license, and so are the recordings made with them in `voices/`. When you
reuse the recordings, keep this attribution. The characters here are
unrelated to MMDAgent's own characters; only the voices are used.

## Regenerating

```sh
MMDAGENT_VOICES=/path/to/MMDAgent_Example-1.8/Voice examples/assets/dialogue-demo/source/generate.sh
```

It needs Open JTalk with the NAIST JDIC dictionary, ffmpeg, node, python3
with Pillow, and a built `celesta-exporter`. The renderer always paints an
opaque background, so `draw-portraits.tsx` draws every image on black and on
white and `source/matte.py` recovers the transparent PNG from the pair.
`script.json`'s optional `speech` field overrides the text Open JTalk reads
(for example `間` → `ま`).
