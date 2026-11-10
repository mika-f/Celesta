# Green screen

Keys green-screen footage over another scene with the `chromaKey` prop.

- `composite.tsx` — the composition: a presenter keyed over a night
  skyline, 1280 × 720, 30 fps, three seconds. `<Video>` keys the footage
  with `chromaKey={{ color: '#00b140', tolerance: 0.2, softness: 0.1, spill }}`,
  with `spill` rising from 0 to 1 over the first two seconds, so the green
  cast on the presenter fades. `<Image>` keys a still of the same footage
  with the defaults, and the raw footage plays in the corner.
- `footage.tsx` — the footage itself, drawn by Celesta: a presenter waving
  in front of an unevenly lit `#00b140` screen, with a shadow on the screen,
  a sheer scarf the screen shows through, a green cast on everything, soft
  moving edges, and sensor grain (`noise`). Its colors are flat on
  purpose, so what the key leaves is easy to judge.
- `assets/footage.mp4` and `assets/still.jpg` — `footage.tsx` exported. The
  H.264 encoding halves the color resolution, as camera footage does, so
  the key sees blocky chroma at the edges.

Everything here is made with Celesta from this repository and is under the
repository's license.

`tolerance` is how near a color's chroma (its BT.709 CbCr) must be to the
key's to become transparent, and `softness` how far beyond it colors fade
back in. Neutral colors lie as far from the key as the key's own chroma:
0.36 for `#00b140`, 0.60 for `#00ff00`. Keep `tolerance + softness` below
that, or grays and blacks turn translucent too.

From the repository root, preview the composite or export it:

```sh
cargo run -p celesta-editor -- examples/greenscreen/composite.tsx
cargo run -p celesta-exporter -- --react examples/greenscreen/composite.tsx greenscreen.mp4
```

To make the footage again after changing `footage.tsx`:

```sh
cargo run -p celesta-exporter -- --react examples/greenscreen/footage.tsx \
  --video-option crf=22 --video-option preset=slow --overwrite \
  examples/greenscreen/assets/footage.mp4
cargo run -p celesta-exporter -- --react examples/greenscreen/footage.tsx \
  --frame 45 --overwrite still.png
sips -s format jpeg -s formatOptions 85 still.png --out examples/greenscreen/assets/still.jpg
```

`sips` is macOS's; any converter to JPEG will do.
