# Circles drawn as rects (2026-10-06)

`<Circle radius>` (`packages/react/src/shapes.ts`) now returns a `<Rect>`
`2 * radius` square with `cornerRadius = radius`, instead of an `<Ellipse>`
path. Its props and placement are unchanged: `x`/`y` place the box's
`anchorX`/`anchorY` point, gradients are in box-local pixels, the stroke lies
inside the box (`strokeWidth` defaults to 2 and is capped at the radius), a 0
radius returns `null`. `<Ellipse>` stays a path, even when square: a rect's
corners are circular, not elliptical.

Measured on an Apple M4 (GPU renderer, scratch example, not committed):

- Speed: 600 animated circles (fills, translucent fills, and strokes), 150
  1080p frames through `submit`/`drain`: paths 420 to 440 ms, rects 63 to
  83 ms, about 6x faster.
- Placement: a circle centred at (100, 100) whose radius grows from 20 to
  21 in 0.1 steps keeps its centroid within 1e-4 px as a rect (it used to
  drift up to 0.5 px, see `2026-10-05-circles-ellipses-arrows`). At
  sub-pixel centres the centroid is within 0.005 px. The CPU renderer agrees
  with the GPU within one or two code values.
- Coverage: a filled rect circle's ink is within 0.05% of the true area,
  also under non-uniform `scaleX`/`scaleY` (the corner distance is a
  first-order approximation there); the path's is 0.1 to 0.3% short. Edge
  pixels differ from the path's by up to about 40 code values, from the
  two anti-aliasing methods.

Fixed on the way: a rect's stroke and fill were mixed as straight colors,
so where the inner edge of a stroke met no fill (a ring) or a translucent
one, the stroke's color was pulled toward the fill's transparent black. A
stroke-only ring drew about 4.5% less ink than its area, with a dark fringe
on its inner edge. `rect.rs` (`mix_premultiplied`) and `layer.wgsl`'s
`rect_color` now mix premultiplied; the ring's ink matches the path's.
`a_stroke_without_fill_keeps_its_color_along_the_inner_edge` covers it.
