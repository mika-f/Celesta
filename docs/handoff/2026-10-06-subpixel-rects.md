# Sub-pixel accurate rects (2026-10-06)

`Rect` layers used to be rasterized (or, on the GPU, shaded texel for texel)
as a texture of their size rounded up to whole pixels, anchored on that
size, placed on a whole pixel, and stretched. A rect at x = 10.5 snapped to
11, a centred 10.5-wide rect sat a quarter pixel off, and a 10.25-wide rect
at scale 2 drew 22 pixels wide. Both renderers now shade every output pixel
a rect covers at its centre, through the layer's transform (PR #166).

- CPU: `celesta_renderer::rasterize_rect_transformed` paints the rect
  `[0, width] x [0, height]` through a `PathTransform` into the part of the
  canvas it covers, like `rasterize_path`, and `CpuRenderer` draws it.
  `rasterize_rect` still returns the rect at its own size, one texel per
  unit rounded up to whole texels, but shades it the same way, so it shares
  the coverage cap and the stroke/fill mix below.
- GPU: `layer.wgsl`'s `rect_color` maps the interpolated scene position
  (`world`, not `@builtin(position)`, which is in the target's pixels when a
  preview is drawn through a fitted viewport) through the rect's inverse
  transform. The quad covers the rect plus one pixel for anti-aliasing and is
  never snapped or filtered. `shades_rects_like_the_cpu_rasterizer` and
  `shades_sheared_rects_like_the_cpu_rasterizer` hold the two within two
  code values.
- Edge distance in output pixels (`RectDistance` / `rect_distance`): exact
  for rotation with a uniform scale and along straight edges. A non-uniform
  scale makes the corners ellipses, approximated by the local distance over
  its gradient, signed by quadrant so a shear leans opposite corners apart.
- Coverage is capped per rect (`coverage_cap` / `rect_coverage_cap`, found
  once per rect, in the vertex shader on the GPU): at most the rect's area,
  its width across each pair of parallel edges, and 1. One sample's
  `0.5 - distance` alone drew a 0.1 px rect at 55% and an empty one at 50%.
  The box inside a stroke is capped the same way, and the stroke and fill mix
  by the fill's share of what the rect covers.
- A single sample per pixel still aliases along sloped thin strips (a 0.1 px
  strip at 45 degrees covers about 0.77 of its area), as it did before.
- `CpuRenderer` is slower on scenes of many thin rects (`ribbons` about 9%):
  it shades every pixel each rect covers plus a one-pixel border, instead of
  copying a small texture. Preview and export use the GPU renderer, which got
  faster on every workload.
- This lifts the reason `<Circle>` is drawn as a path rather than a rounded
  `<Rect>` (see `2026-10-05-circles-ellipses-arrows`); that comparison can be
  measured again.
