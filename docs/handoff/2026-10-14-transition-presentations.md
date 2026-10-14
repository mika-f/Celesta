# Transitions as functions, seven new types, and a series' size (2026-10-14)

Issues #291, #290, and #221. They were done together because #291 asks for
the built-in types to be written with the same mechanism the new ones use.
They also needed #146's masks in React and the browser; see
`2026-10-14-group-masks.md`.

## The mechanism

- `defineTransition({ name, present })` in
  `packages/transitions/src/presentation.ts` returns a frozen
  `{ name, present }`. `present` is called once per overlapped frame for
  each scene. It gets `{ progress, frame, durationInFrames, from, width,
  height }` and returns `{ enter?, exit? }`, Group props without
  `children`, `id`, or `lang`. `progress` is already eased, and keeps the
  old rule: (frame + 1) / (n + 1). The name `'cut'` is reserved. A cut is
  still a string only, since it takes no frames.
- `<TransitionSeries.Transition type>` and the items of
  `computeTransitionSeries` take a built-in name or such an object (checked
  by shape: a string `name` and a `present` function). An unknown type,
  including a transition function passed without calling it, throws and
  lists the names. `useTransitionSeriesScene()` reports `enter`/`exit.type`
  as the definition's `name`. Its type widened from the name union to
  `TransitionSeriesType | (string & {})`.
- The scene's `Group` is always rendered, with `{}` outside the overlap.
  A style's `mask` goes after the children (see the masks note), so layer
  ids stay put as transitions start and end. The entering scene is still
  drawn above the leaving one. A transition cannot reorder them, since
  layer ids are tree paths.

## Built-in types

`packages/transitions/src/built-in-transitions.ts`. `crossfade`, `slide`,
and `wipe` moved here unchanged: the old tests pass as they were, and
exported frames are byte-identical. The other types:

- `iris({ center, direction, feather })`: a mask disc, a `Rect` with
  `cornerRadius`, reaching the farthest corner plus the feather. The
  feather is a radial gradient fill, not `blur`, which would fade at the
  scene's edges. `'close'` inverts the mask.
- `clockWipe({ center, startAngle, direction })`, named `'clock-wipe'`: a
  fan `Path` mask of chords of at most 30°, at a radius that keeps every
  chord past the farthest corner.
- `zoom({ direction, scale, center })`: a group has no size, so its anchor
  does nothing. Scaling about a point moves the group by `point × (1 − s)`.
- `dissolve({ cellSize, seed })`: one `Path` mask of the shown squares,
  merged into one rectangle per run in a row. The order comes from
  `@celesta/math`'s `random`, cached for the last grid size.
- `blur({ amount })`: `blur` on both scenes, with the entering one fading
  in.
- `flip({ shade })` and `cube({ shade })`: drawn without perspective. No
  renderer has a non-affine transform, and Canvas 2D cannot draw one. A
  flip scales one axis by |cos θ| and darkens with `brightness`; `from`
  picks the axis only. A cube is the orthographic projection of two faces
  meeting at an edge. It is scaled by 1 / (cos θ + sin θ), so the pair
  always spans the series.

Every type draws the same in the GPU renderer and `@celesta/web`: masks,
clips, `blur`, and `brightness` all run in both. A scene with every type
was drawn in a browser both ways: evaluated by the editor's worker, and
evaluated by the CLI runtime. It matched the native export except along
anti-aliased edges.

## Size (#221)

`<TransitionSeries width height>` defaults to `useLayoutBounds()` from
`@celesta/layout`: the area of an enclosing `<SafeArea>` or `<Fit>`, or else
the composition's size. `@celesta/transitions` now depends on
`@celesta/layout`, `@celesta/math`, and `@celesta/shapes`. The web
playground's Vite aliases resolve them from source, so the layout context
is shared.

## Follow-up

- A normal-blend `Group`'s `opacity` was not isolated, so overlapping
  children of a fading scene faded separately. mika-f/Celesta#356 isolates
  every group below opacity 1, so the scenes of `crossfade` and every type
  that fades now fade as one picture.
