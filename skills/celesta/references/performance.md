# Rendering cost

Preview and export draw every frame on the GPU. A frame's cost depends on
how many layers there are, and far more on what they ask the renderer to
do. Thousands of flat `Rect`s are cheap; a few dozen effects are not.

## Contents

- [What costs what](#what-costs-what)
- [Same picture, less work](#same-picture-less-work)
- [Find slow parts](#find-slow-parts)

## What costs what

| Cheap | Costs more |
| --- | --- |
| `Rect`, flat or gradient, including a gradient whose colors change every frame, and `Circle` (a rounded `Rect`) | `blur`, `glow`, `shadow`: each layer with an effect is drawn onto a canvas of its own and filtered in several extra GPU passes over the area it covers |
| `x`/`y`, `scale`, `rotation`, `opacity` | A `blendMode` other than `'normal'`: every blended layer reads what is beneath it, which takes a copy and a GPU pass of its own |
| Text, images and SVGs whose content and drawn size stay the same: rasterized once and reused | `Path`, `Line`, `Polyline`, `Ellipse`, `Arrow`: each path's outline is rebuilt on the CPU every frame and shaded on the GPU, so cost grows with segment count and covered area. Still far cheaper than the same line art as hundreds of `Rect`s |
| | Text whose string or style (including a gradient `fill`'s colors) changes every frame: rasterized again each frame. Text drawn at a changing scale is rasterized again in steps of about 9% |
| | `@celesta.studio/code` while typing: each new character rasterizes its whole line again (one `Text` per line, shaping cached) |

## Same picture, less work

- **Wrap content that does not change in `<Static>`.** Backgrounds, grids,
  labels, and other content that stays the same from frame to frame is
  rebuilt every frame otherwise. Inside `<Static>`, 100,000 still layers
  cost about 7 ms a frame instead of 85 ms or more; see
  [react-core.md](react-core.md#static-still-subtrees).
- **Draw thousands of similar elements with `<Instances>`.** Every layer
  costs time in React, in sending the frame to the renderer, and in the
  renderer itself: 100,000 `<Rect>`s take about 180 ms a frame, the same
  100,000 particles in one `<Instances>` about 10 ms. Use it for particles,
  dots, snow, and sprites; see
  [react-core.md](react-core.md#instances-particles).
- **Put one effect on a `Group`, not one on each layer.** A `Group`'s effect
  filters all its children together. Letters that fade one by one can
  share one `glow` on their `Group`; it follows each letter's opacity.
- **Draw many small lights as gradients.** For dozens of glowing points
  (stars, windows, sparks), draw a radial-gradient `Rect` that fades to
  transparent instead of using `glow`. Animate it with `opacity` and
  `scale`:

  ```tsx
  <Rect x={x} y={y} width={40} height={40} anchorX={0.5} anchorY={0.5} opacity={brightness}
    fill={{ type: 'radial', center: { x: 20, y: 20 }, radius: 20,
      stops: [{ offset: 0, color: '#FFE8C0' }, { offset: 1, color: '#FFE8C000' }] }} />
  ```

- **Blend a few large layers rather than many small ones.** Layers that use
  the same mode and do not overlap one another can go in one
  `<Group blendMode="add">`. The group then blends once.
- **Draw line art as paths, not rects.** Group strands that share a width
  and opacity into one `Path` each: tens of paths, not thousands of thin
  `Rect`s.
- **Animate text with transforms.** Moving, fading, and rotating text
  reuses its raster; changing its string or style every frame does not.

## Find slow parts

The exporter reports its speed. With `--json` it is `stats.renderFps`, the
average for the whole export. In text mode (outside a terminal, or with
`--no-ui`) it prints lines such as
`rendering frame 300/1530  58.5 fps  elapsed 00:00:05  eta 00:00:21` to
stderr; the `fps` is the average so far.

Both speeds count video encoding too. The encoder works on earlier frames
while Celesta renders the next ones, so the export runs at the speed of the
slower of the two. With the default `--preset medium`, encoding a large video
(1080 × 1920, for example) can be the slower one; the speed then stays about
the same when you add or remove effects. `--preset ultrafast` encodes fast
enough to keep encoding out of the measurement, so use it whenever you
compare rendering speeds.

To find which part of a video is slow, export spans of a few seconds and
compare their speeds. Choose start times that fit inside the video;
this example is for one at least 44 s long:

```sh
for t in 0 10 20 30 40; do
  Celesta-export --json --overwrite --preset ultrafast --from $t --to $((t + 4)) \
    --react scene.tsx /tmp/celesta-speed.mp4 | grep -o '"renderFps":[0-9.]*'
done
```

Startup is counted too, so a very short span reads slower than it renders.
Then look at what the slow span draws (a contact sheet of it helps, see
[verify.md](verify.md#contact-sheets)). The usual causes are dozens of
effects, many blended layers, and large paths.

A machine without a GPU (CI, containers) renders with a software Vulkan
driver and is much slower: roughly 10–60 fps at 1080p on 4 cores.
