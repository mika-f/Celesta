# `<TransitionSeries>` (2026-10-08)

`<TransitionSeries>` (`packages/react/src/transition-series.ts`) lays scenes
out from their lengths with cuts, cross-fades, slides, and rectangular wipes
between them (issue #141). It is built from `<Sequence>` and `<Group>`
(`opacity`, `x`/`y`, `clip`) only, so preview, export, and the browser
runtime evaluate it like any other React tree; no renderer primitive was
added. `Series` and `Transition` are unchanged.

- Timing: `computeTransitionSeries(items)` takes the children's props in
  order (scenes `{ durationInFrames }`, transitions `{ type,
  durationInFrames? }`, told apart by `type`). The transition element also
  takes `from`, the edge a slide or wipe enters from (`'left'` by default;
  only `left`, `right`, `top`, and `bottom`, otherwise `edgeAt` throws when
  the series renders), and `easing` (linear by default); only the
  presentation reads them. A transition of n frames starts the next scene n
  frames before the previous one ends; the total is the last scene's end.
  `transitions[i].from` is the overlap's start.
- Validation (throws): no scene, a transition first or last or after
  another, unknown `type`, a `'cut'` with frames, non-integer or
  non-positive lengths, and a scene shorter than its transitions in and out
  together. Equal is allowed: the previous scene ends on the frame the next
  starts, so at most two scenes are mounted on any frame. Sequences are
  half-open, so consecutive scenes never leave an empty frame.
- Clocks: each scene is a `<Sequence from durationInFrames>`; its frame 0
  is the first frame of its transition in, `useVideoConfig().durationInFrames`
  includes both overlaps, and media start there.
- Presentation: `SceneFrame` wraps every scene in a `<Group>`, idle or not,
  so layer ids inside stay the same when a transition starts (export keys
  video decode sessions by layer id). Progress on the k-th overlapped frame
  is `easing((k + 1) / (n + 1))`, so no overlapped frame shows one scene
  alone. The entering scene is later in the tree and draws above.
  `crossfade` fades only the entering scene (the leaving one stays opaque,
  so opaque scenes mix without a dip); `slide` pushes both by the
  composition's width/height; `wipe` clips the entering scene to a strip
  from `from`, computing the far edge as `size - strip` so it lands exactly.
- Audio: overlapping scenes play together at their own volume; nothing is
  faded automatically, because multiplying a scene gain into nested
  keyframed volumes is not representable in `Animatable`.
  `useTransitionVolume(volume)` returns `frameKeyframes` ramps over the
  scene's transitions (the plain `volume` when neither side has frames) for
  an `<Audio>` directly in the scene (keys count from the scene's start).
  `useTransitionSeriesScene()` exposes `{ index, enter, exit }`.
- Tests: `packages/react/test/transition-series.test.mjs` (layout,
  validation, per-frame visibility, each presentation, audio).
  `crates/react-bridge/tests/node_integration.rs` checks
  `examples/with-transition-series.tsx` through the bridge (frame counts,
  collected audio graph cross-fade) and adds it to the batched-vs-per-frame
  audio graph parity test.
