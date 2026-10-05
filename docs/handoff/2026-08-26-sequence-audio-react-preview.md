# `<Sequence>`, per-frame audio collection, and editor React preview (2026-08-26)

Three related gaps closed in one pass; they share one protocol change.

- **`<Sequence>` component** (`packages/react/src/components.ts`,
  `src/render.ts`). Props: `from?: number` (frame offset in the enclosing
  timeline's own frame numbering, default 0), `durationInFrames?: number`
  (default: run until the enclosing window ends), plus the common
  transform/opacity props. It is a real function component: it reads the
  enclosing runtime context and re-provides a shifted
  `CompositionRuntimeContext` around its children, so hooks called from a
  component *inside* a sequence see `frame - from` (JSX children evaluate
  where they are written, so hooks written inline in the parent still see the
  parent's clock — the same rule as every other React host, called out
  because it surprises people). The `'sequence'` host node itself always
  stays in the instance tree regardless of time: whether the window contains
  the currently rendered time is decided by render.ts's walker per requested
  frame (`childSequenceContext`), not at React render time. Inactive → no
  layers and no audio collected for that frame. Active → children render
  inside a group layer carrying the sequence's own x/y/opacity, and local
  time is shifted (`<Video>`/`<Audio>` inside play synced to the sequence's
  own clock). Nested sequences compose (origins add, audible windows
  intersect). This is what replaced the "no `<Sequence>`-style range offset"
  caveat on `<Video>`/`<Audio>`.
- **Per-frame audio collection** (`src/render.ts`, `src/cli.ts`,
  `crates/react-bridge/src/lib.rs`, `crates/exporter/src/lib.rs`). Frame
  responses are now `{scene, audio}` where `audio` lists every `<Audio>`
  element that rendered into *that* tree, each as
  `{src, sourceStart, playbackRate, volume, muted, start, duration}`:
  composition-space audible window (`start`/`duration`, intersected through
  any enclosing sequences and never before the clip's local zero),
  `sourceStart` adjusted so head-clipping keeps the source clock continuous,
  and `playbackRate`/`volume` as `number | KeyframeAnimation` — the
  TypeScript mirror of the project format's `Animatable<f64>` (this closes
  the "static-only volume" caveat too). When a window clips a clip's head,
  animation keyframes shift earlier by the clipped amount so curves stay
  aligned with what is actually heard (exact for static rates; documented
  approximation for keyframed rates). Because collection rides the same walk
  as layers, an `<Audio>` behind an ordinary React conditional or hook now
  contributes sound on exactly the frames where it renders — the old single
  `Ready`-message collection (`collectAudioClips`, `audioClips`) is gone.
  Rust mirrors this: `ReactAudioClipDescriptor` carries the new shape,
  `ReactBridge::evaluate_at` returns `FrameEvaluation { scene, audio }`
  (the `scene_at*` wrappers still return just the `Scene`),
  `render_react_video` accumulates reports across frames, and
  `merge_react_audio_clips` collapses bit-identical per-frame reports while
  preserving multiplicity (two identical clips playing at once stay two).
  **Export flow reorder**: frames stream into the staged output file first,
  the graph is built afterwards; empty graph → staged file published
  directly (silent exports keep their no-mix/no-mux path, verified against
  `title.tsx`), non-empty → mix + mux into a second temp then publish.
  **Bug fixed here**: `export_react_entry_impl` canonicalizes the entry's
  and companion project's asset roots before absolutizing relative asset
  paths — a relative CLI entry path produced a still-relative "absolute"
  path that the audio mixer joined twice.
- **Editor React preview** (`src/render.ts`'s new `createResolver()`,
  `src/cli.ts`, `crates/react-bridge/src/lib.rs`'s
  `resolve_components`, `crates/editor/src/main.rs`). A new protocol request
  `{"components": [{component, props}], "runtime": {width, height, fps,
  durationInFrames, time}}` answers
  `{"components": [layers|null, ...]}`: each registered name renders against
  a second persistent reconciler root dedicated to resolution (hook state in
  resolved components survives across calls; unresolved names yield null).
  The `runtime` object — added 2026-08-26; older bridges omit it and the
  components then see frame-0 placeholders — carries the entry's own
  `<Composition>` facts (the same ones the handshake metadata was read from)
  plus the exact preview playhead, so resolved components'
  `useCurrentFrame()`/`useVideoConfig()` match what an export renders at
  that frame (`ReactBridge::resolve_components` takes the `Time` parameter
  and derives the rest from its own metadata; the editor passes
  `scene.time`). `<ProjectTimeline />`/`useProjectTrack()` inside a
  *resolved* component still see empty content — threading the real
  per-frame project layers into resolution requests is future work. The GPUI
  preview worker
  owns an optional `ReactPreviewBridge` keyed by (node, cli script, entry) —
  respawned when `react_entry` changes, spawn/resolution failures remembered
  per context so a broken setup does not restart Node every frame. Each
  preview request collects the evaluated scene's `missingComponent` layers
  recursively, resolves them, splices resolved layers back inside their
  original layer shells (same placement semantics as the exporter's
  `rawTransform` wrapper), strips unresolved ones, and returns warnings;
  `EditorView` renders those as an amber overlay in the preview panel. With
  no `react_entry` set, component clips are hidden with an explanatory
  warning instead of failing the whole GPU render (`GpuRenderer` still
  errors on `missingComponent` content — the honest export outcome).
  **Bug fixed here**: `EditorDocument::load` now canonicalizes the project
  path before deriving the asset root, so paths serialized relative to the
  root resolve back to exactly the same absolute path (on macOS `/var` is a
  symlink to `/private/var`, which broke the `react_entry` round trip).

Verified: new integration tests (`shifts_media_inside_sequences_...`,
`collects_conditionally_rendered_audio_with_keyframed_volume_...`,
`reports_audio_clips_per_frame_...`,
`resolves_individual_components_through_the_bridge_...`,
`resolves_components_against_the_requested_time_...` in
`crates/react-bridge/tests/node_integration.rs` against new examples
`with-sequence.tsx`, `with-conditional-audio.tsx`, and
`with-frame-component.tsx` — the last one registers a component rendering
`useCurrentFrame()`/`useVideoConfig()` and asserts two resolve calls on the
same root report "frame 15 of 640 at 30fps" then "frame 7 of ...", proving
resolved hooks follow the requested time); exporter unit tests
for report merging and graph construction; end-to-end
`celesta-exporter --react packages/react/examples/with-sequence.tsx out.mp4`
produced h264+aac (ffprobe) whose extracted frame 45 shows only the
sequence-shifted testsrc video and frame 75 shows "frame 15 inside" beside
it, while `title.tsx` stayed video-only with no mix/mux progress stages.
