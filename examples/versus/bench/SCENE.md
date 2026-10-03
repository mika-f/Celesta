# NEBULA: the benchmark scene

One scene, implemented three times with the same math and the same fonts:

- [`celesta/nebula.tsx`](celesta/nebula.tsx): Celesta React composition
- [`remotion/src/Nebula.tsx`](remotion/src/Nebula.tsx): Remotion (DOM, CSS and one inline SVG)
- [`fframes/src/lib.rs`](fframes/src/lib.rs): fframes (`svgr!` SVG tree, Skia on Vulkan)

1920×1080, 60 fps, 600 frames (10 s), no audio. `t = frame / 60`.
`rand(i, k) = fract(sin(i * 12.9898 + k * 78.233) * 43758.5453)`.
Blur and glow sizes are Gaussian standard deviations (σ), as in CSS `blur()`
and SVG `stdDeviation`; CSS `drop-shadow()` takes twice σ as its radius.

Drawn back to front:

| Layer | Count | Definition |
| --- | ---: | --- |
| Background | 1 | Vertical linear gradient, top `mix(#0B1026, #1A0B2E, k)`, bottom `mix(#04050C, #0B1A2E, k)`, `k = 0.5 + 0.5 sin(0.6t)` (RGB mix, rounded). |
| Blobs | 6 | Circles of radius `260 + 60 sin(0.8t + j)` at `(960 + 520 cos(0.35t + 1.047j), 540 + 260 sin(0.5t + 1.3j))`, colors `#FF3D7F #3DA5FF #7B5CFF #00E0B8 #FFB13D #FF6B3D`, opacity 0.55, blur σ 48, each screen-blended. |
| Rings | 24 | Ellipse strokes centered at (960, 540), `rx = 180 + 30k`, `ry = 0.38 rx`, rotated `7.5k ± 12t` degrees (odd `k` turn the other way), stroke `#8FB8FF` 1.5 px, opacity `0.18 + 0.22 (0.5 + 0.5 sin(2t + 0.4k))`. |
| Particles | 1500 | Circles of diameter `2 + 6 rand(i,3)` at angle `a = 2π rand(i,2) + t (0.1 + 0.4 rand(i,3)) dir` (`dir = -1` when `rand(i,4) < 0.5`), radius `r = 60 + 1000 rand(i,1)`, position `(960 + r cos a, 540 + 0.45 r sin a)`, colors `#FFFFFF #9EC9FF #FFC2E0 #B8FFF0` by `i % 4`, opacity `0.3 + 0.7 (0.5 + 0.5 sin(t (2 + 4 rand(i,5)) + i))`. |
| Spectrum | 80 | Bars 14 px wide every 20 px from x = 163, bottom at y = 1040, height `20 + 180 |sin(2.1t + 0.27k) cos(1.3t + 0.11k)|`, corner radius 4, gradient `#3DA5FF` (bottom) to `#FF3D7F` (top). |
| Title | 2 texts | Group at (960, 470) scaled `1 + 0.03 sin(2t)`, glow `#7B5CFF` σ 24. "NEBULA" in Bebas Neue 220 px, white, letter spacing `20 + 10 sin t`, centered; a subtitle in IBM Plex Mono 24 px `#C8D6FF` 150 px below. |
| HUD | 49 texts | IBM Plex Mono 18 px `#9EC9FF` at 0.8 opacity: `CHjj ±v.vvv` with `v = 100 sin(t (1 + 0.13j) + j)`, 24 per column at x = 40 and x = 1720 from y = 60, every 30 px; `FRAME nnnn / 600` at the bottom right. |

About 1,660 layers per frame. Every value changes every frame except the
subtitle, so nothing can be cached between frames as a whole.
