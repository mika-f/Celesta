# Circles, ellipses, and arrows (2026-10-05)

`<Circle>`, `<Ellipse>`, and `<Arrow>` (`packages/react/src/shapes.ts`) are
React components that each return one `<Path>` (issue #135); there is no new
renderer primitive, so preview, export, CPU, and GPU draw them as any path.

- `Ellipse width height` / `Circle radius` follow `<Rect>`'s conventions,
  which `Path` alone does not: `x`/`y` place the box's `anchorX`/`anchorY`
  point (the path's commands are shifted by `-anchor * size`, since a path
  layer ignores the anchor), gradient `Paint`s are in box-local pixels
  (shifted by the same offset), and the stroke lies inside the box (the
  outline is inset by half the stroke width, which is capped at half the
  shorter side). The outline is eight cubic arcs (`k = 4/3 tan(π/16)`),
  within about 4e-6 of the radius. A 0 size returns `null`; negative or
  non-finite sizes throw.
- `Arrow x1 y1 x2 y2` is one closed polygon filled with `stroke` (no path
  stroke), so translucent and gradient arrows have no seam and the head's tip
  lands exactly on its end point. `headLength`/`headWidth` default to
  4 × `strokeWidth`; a head is at least as wide as the shaft. When the arrow
  is shorter than its heads they scale down uniformly to fit, which keeps an
  arrow growing from its tail continuous. Zero length returns `null`;
  non-finite points, non-positive head sizes, or an unknown `heads` throw.
- `examples/with-shapes.tsx` shows all three; the bridge test renders it with
  `CpuRenderer` at frame 0 (the CPU renderer rejects any rotation).
- Curved arrows and drawing progress along arcs are left for later `Path`
  work.
- Drawing `Circle` as `<Rect cornerRadius={radius}>` (GPU-shaded, no CPU
  flattening) was measured and rejected: 600 animated circles exported
  about twice as fast (1.2 s vs 2.5 s for 150 1080p frames), but a rect is
  shaded at its size rounded up to whole pixels, anchored on that size, and
  its top-left is snapped to a whole pixel when unscaled (to match the CPU
  renderer texel for texel). A circle whose radius grew from 20 to 21 at a
  fixed centre drifted up to 0.5 px and jumped 0.9 px between 20.5 and 20.6;
  the path stays centred. Revisit only if rects become sub-pixel accurate.
