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
| `Rect`, flat or gradient, including a gradient whose colors change every frame | `blur`, `glow`, `shadow`: each layer with an effect is drawn onto a canvas of its own and filtered in several extra GPU passes over the area it covers |
| `x`/`y`, `scale`, `rotation`, `opacity` | A `blendMode` other than `'normal'`: every blended layer reads what is beneath it, which takes a copy and a GPU pass of its own |
| Text, images and SVGs whose content and drawn size stay the same: rasterized once and reused | `Path`, `Line`, `Polyline`, `Circle`, `Ellipse`, `Arrow`: each path's outline is rebuilt on the CPU every frame and shaded on the GPU, so cost grows with segment count and covered area. Still far cheaper than the same line art as hundreds of `Rect`s |
| | Text whose string or style (including a gradient `fill`'s colors) changes every frame: rasterized again each frame. Text drawn at a changing scale is rasterized again in steps of about 9% |
| | `@celesta/code` with long, many-colored lines: one `Text` layer per color run |

## Same picture, less work

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

The exporter reports its speed as it renders. In text mode (outside a
terminal, or with `--no-ui`) it prints lines such as
`rendering frame 300/1530  58.5 fps  elapsed 00:00:05  eta 00:00:21` to
stderr; the `fps` is the average so far.

To find which part of a video is slow, export spans of a few seconds and
compare the last `fps` each one prints. `--preset ultrafast` keeps encoding
time out of the measurement. Choose start times that fit inside the video;
this example is for one at least 44 s long:

```sh
for t in 0 10 20 30 40; do
  Celesta-export --no-ui --overwrite --preset ultrafast --from $t --to $((t + 4)) \
    --react scene.tsx /tmp/celesta-speed.mp4 2>&1 | grep 'rendering frame' | tail -1
done
```

Startup is counted too, so a very short span reads slower than it renders.
Then look at what the slow span draws (a contact sheet of it helps, see
[verify.md](verify.md#contact-sheets)). The usual causes are dozens of
effects, many blended layers, and large paths.

A machine without a GPU (CI, containers) renders with a software Vulkan
driver and is much slower: roughly 10–60 fps at 1080p on 4 cores.
