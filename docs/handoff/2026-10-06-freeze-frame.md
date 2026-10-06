# `<FreezeFrame frame>` (2026-10-06)

`<FreezeFrame frame>` (`packages/react/src/components.ts`) draws its children
as the composition looked at an absolute frame (issue #88). The design is in
[`docs/superpowers/specs/2026-10-06-freeze-frame-design.md`](../superpowers/specs/2026-10-06-freeze-frame-design.md).

- `frame` counts on the root clock even inside a `<Sequence>`. `<Sequence>`
  overwrites `CompositionRuntimeContext`, so `render.ts` also provides the
  root runtime as `RootRuntimeContext` (`hooks.ts`), in `mount` and in
  `createResolver`. Inside, `useCurrentFrame()` is `frame` and
  `durationInFrames` is the composition's; `lang` is inherited.
- The walker's `childFreezeContext` resets `originSec`, `rangeStartSec`, and
  `rangeEndSec` to the root's (`WalkContext.compositionEndSec`). Inheriting
  the enclosing range would make `childSequenceContext` drop the frozen
  world's sequences when the freeze sits in a short `<Sequence>`.
  `collectCharacterViewOverrides` uses the same context, so both passes gate
  inner sequences on the frozen time.
- Always silent: the group is walked with a throwaway audio array, and the
  audio-only sweep skips the subtree.
- Portraits: react-reconciler 0.29.2 attaches `getPublicInstance`'s result
  unconditionally and writes `null` on detach, so a ref cannot be skipped in
  the host config. Instead `FreezeFrame` provides a `ViewScopeContext`. A
  `CharacterView` in a scope passes a callback ref that registers its host
  node under the author's ref; a `Dialogue` in a scope passes a stable
  `{ get current() }` object reading that map. `characterViewOverrides` is
  keyed by host node, so copies get their own overrides with no other change.
  A frozen `Dialogue` whose view is not in its innermost freeze throws.
- Authored ids inside are prefixed with the freeze layer's id and `/`
  (`WalkContext.idPrefix`): export keeps one sequential video decode session
  per layer id (`crates/media/src/ffmpeg.rs`).
- Project layers are evaluated by Rust for the current frame only, so
  `<ProjectTimeline />`, `<ProjectTrack />`, and `useProjectTrack()` throw
  inside a freeze (`FreezeFrameContext`, `project-runtime.ts`) instead of
  showing the current frame's content as frame N.
- Tests: `packages/react/test/freeze-frame.test.mjs`.
