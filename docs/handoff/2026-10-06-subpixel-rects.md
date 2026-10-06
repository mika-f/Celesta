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
- Edge distance (`RectDistance` / `rect_distance`) is measured in units of
  a pixel's width across the edge, `|n.x| + |n.y|` for the edge's normal `n`
  (1 along a row or column, up to √2 at 45 degrees): the local distance over
  the L1 length of its gradient in output pixels. `0.5 - distance` then
  ramps across an edge as a pixel-sized box filter does. Measured in
  Euclidean pixels instead, a thin strip near 45 degrees drew 0.71 to 1.47
  of its area depending on where its edges fell between pixel centres, so a
  moving strip flickered. Exact along straight edges; round a corner (an
  ellipse under a non-uniform scale) a first-order approximation, signed by
  quadrant so a shear leans opposite corners apart.
- Coverage is capped per rect (`coverage_cap` / `rect_coverage_cap`, found
  once per rect, in the vertex shader on the GPU). One sample's
  `0.5 - distance` alone drew a 0.1 px rect at 55% and an empty one at 50%.
  A box filter gives a pixel inside the ramps of a pair of parallel edges
  their distance apart in those units, at most 1; the cap multiplies the two
  pairs' fractions, which is exact for an unsheared box and an approximation
  under shear, where the two pairs' coverages are not independent. The box inside a stroke is capped the same way, and
  the stroke and fill mix by the fill's share of what the rect covers.
- `thin_strips_cover_their_area_wherever_they_fall` holds strips from 0.1 to
  1 px wide, rotated or sheared, within 0.97 to 1.12 of their area at ten
  sampled sub-pixel offsets. A box smaller than a pixel in both directions cannot be
  placed exactly by one sample: leaning, it draws about 0.5 to 1.7 of its
  area depending on position, at the same sampled offsets (`boxes_within_a_pixel_stay_near_their_area_when_leaning`).
- `CpuRenderer` is slower on scenes of many thin rects (`ribbons` about 9%):
  it shades every pixel each rect covers plus a one-pixel border, instead of
  copying a small texture. Preview and export use the GPU renderer, which got
  faster on every workload.
- This lifted the reason `<Circle>` was drawn as a path rather than a
  rounded `<Rect>`; it is now a rect (`2026-10-06-circles-as-rects`), which
  also made the stroke and fill mix premultiplied.
