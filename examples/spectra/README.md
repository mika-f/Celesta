# SPECTRA — a performance reel

An 11.5-second, 1920×1080, 60 fps motion graphic that puts most of Celesta's
renderer to work, made as a benchmark for performance tuning. It is the
`spectra` workload of `celesta-bench` (see
[`docs/performance/benchmarking.md`](../../docs/performance/benchmarking.md)).

Five 150-frame chapters play in a `<Series>`, each overlapping the one before
by 15 frames, where two chapters draw at once through `<Transition>`
crossfades. A HUD with the chapter, timecode and progress, and a multiply
vignette, sit over all of them.

| Chapter | What it exercises |
| --- | --- |
| IGNITION | 700 circles, six radial-gradient lights with `blur` and `screen` blending, 16 stroked ellipses in a squashed group, a `TextReveal` title with `glow` and `shadow` and animated letter spacing |
| GEOMETRY | A `<Grid>` of six tiles, one per blend mode, each an isolated, rotated group with a rounded `clip` and ten rotating gradient rects; a 52-line grid; star and polygon `Path`s with gradient fills; a `Polyline` that draws itself on with `glow`; `Arrow`s |
| TYPOGRAPHY | Twelve words scaling and spacing out every frame; a Japanese paragraph typed on with `useTypewriter` in a `TextBox` with phrase line breaking; a `useCountUp` counter with a stroke and a shadow; a ticker clipped to a band |
| SOURCE | A `<Camera>` pushing in with shake over an editor card (`shadow` and `glow`) typing highlighted TSX with `@celesta/code`, beside portraits drawn with `fit="cover"` and `"contain"` in rounded clips |
| SIGNAL | 900 rotating particles, 48 gradient bars, three line charts drawing on, a donut of path arcs with a shadow, count-up metric cards, and a logo that focuses out of a `blur` |

Every value derives from the frame, so any frame renders on its own. Fonts
are the NEBULA benchmark's (`../versus/bench/assets/fonts`), the portraits
come from `../assets/dialogue-demo/portraits`, and the Japanese text uses the
system's fallback font, as the `text` workload does.

Preview and export from the repository root, after building the React
runtime as described in the root README:

```sh
cargo run -p celesta-editor --release -- examples/spectra/film.tsx
cargo run -p celesta-exporter --release -- --react examples/spectra/film.tsx spectra.mp4
```
