# `<FreezeFrame frame>` design (issue #88)

## Goal

Celesta's picture is a pure function of the frame number, so a composition
can redraw any past frame in place: rewinds, flashbacks, thumbnail strips.
`examples/zunda` (PR #75) does this with `<Sequence from={now - frame}>`, but
has to leave portraits and audio out of what it redraws:

- A `<CharacterView ref={view}>` drawn a second time overwrites the shared
  module-level ref, so the live `<Dialogue character={view}>` drives the copy.
- `<Audio>` and voiced `<Dialogue>` inside are collected again and play twice.
- The `from` offset has to be recomputed every frame to hold one frame.

`<FreezeFrame frame={N}>` draws its children as the composition looked at
frame `N`, portraits included, without sound.

```tsx
<Group scale={0.25} x={80} y={80}>
  <FreezeFrame frame={1200}>
    <World />
  </FreezeFrame>
</Group>
```

## API

```ts
export interface FreezeFrameProps extends CommonProps {
  /** Composition frame the children are drawn at. */
  frame: number;
  children?: ReactNode;
}
export function FreezeFrame(props: FreezeFrameProps): ReactElement;
```

`FreezeFrame` is a positionable group layer, like `<Sequence>` (`x`, `y`,
`scale`, `opacity`, `id`, effects, …).

## Behavior

### Time

- `frame` is an absolute composition frame, whatever `<Sequence>`s enclose the
  `<FreezeFrame>`. Inside, children see the root clock at that frame:
  `useCurrentFrame()` is `frame`, `useCurrentTime()` is `frame / fps`, and
  `useVideoConfig().durationInFrames` is the composition's.
- `<Sequence>`s inside open and close against that frame, so a `World` written
  for the root renders exactly as it did at frame `frame`.
- Lip sync, blinking, and `<Video>` read the same clock, so they show what the
  original frame showed. A constant `frame` holds still; `frame={x - 90}` plays
  the original three seconds late.
- `frame` is not clamped to `[0, durationInFrames)`. A non-finite `frame`
  throws.
- `lang` is not time: it is inherited from the enclosing tree.
- Nested `<FreezeFrame>`s: the innermost one sets the clock.

### Audio

Always silent, with no `muted` prop. `<Audio>` and voiced `<Dialogue>` inside
are never collected, either by `renderAt` or by `collectAudio`'s sweep. Delayed
playback with sound is already `<Sequence from>`.

### Portraits

- A `<CharacterView>` inside never writes the user's ref. The live ref keeps
  pointing at the live view, even when the copy mounts or unmounts.
- A `<Dialogue>` inside resolves its `character` ref only to a
  `<CharacterView>` inside the same (innermost) `<FreezeFrame>`. Its
  expression and mouth apply to that copy; a `<Dialogue>` outside still drives
  the live view.
- A `<Dialogue>` inside whose view is not rendered inside the same
  `<FreezeFrame>` throws: "`<Dialogue>` inside `<FreezeFrame>` must refer to a
  `<CharacterView>` rendered inside the same `<FreezeFrame>`". It never falls
  back to the live view, which would make the frozen picture depend on the
  current frame.

### Layer ids

Layer ids are tree paths unless the author sets `id`, so an authored `id`
inside would equal the live layer's. Inside a `<FreezeFrame>`, authored ids are
prefixed with the freeze layer's id and `/`. Export keeps a sequential video
decode session per layer id (`crates/media/src/ffmpeg.rs`), and a live video
and its frozen copy sharing one would re-seek every frame.

## Why not the reconciler

react-reconciler 0.29.2's `commitAttachRef` assigns `getPublicInstance`'s
result to the ref unconditionally, and `safelyDetachRef` writes `null` on
unmount. There is no host-config hook to skip a ref, so returning `null` from
`getPublicInstance` would still clobber the live ref. Switching
`CharacterViewReference` to ids would break every composition and still need
per-freeze scoping, since ids repeat inside and outside. The ref is instead
kept off the host element by `CharacterView` itself, through a React context
scope.

## Implementation

### React side (`hooks.ts`, `components.ts`)

- `RootRuntimeContext` (internal): the root `CompositionRuntimeContextValue`,
  provided by `render.ts`'s `mount` and `createResolver` next to
  `CompositionRuntimeContext`. `<Sequence>` overwrites the latter, so
  `FreezeFrame` needs this for the composition's duration.
- `ViewScopeContext` (internal): `null` outside any freeze. A scope holds a
  `Map<RefObject, HostNode>` from user refs to the copies' host nodes, and
  hands out stable per-ref objects whose `current` reads that map.
- `FreezeFrame` provides `{ ...root, lang: enclosing lang, time: frame / fps }`
  as `CompositionRuntimeContext`, a fresh scope as `ViewScopeContext`, and the
  root runtime again as `RootRuntimeContext`. It renders a `'freeze-frame'`
  host element carrying its props.
- `CharacterView` in a scope passes a callback ref instead of the user's: it
  registers the host node under the user's ref when attached and removes it on
  detach, only if the map still holds that node.
- `Dialogue` in a scope replaces `character` with the scope's object for that
  ref, for both the host element and `useRenderedSubtitle`. A view not yet
  attached in this commit is handled by the existing `requestRerender()` pass.
  `DialogueSeries` renders `Dialogue`, so it needs no change.

### Walker (`render.ts`)

- `'freeze-frame'` joins `HOST_TYPES` and gets its own group branch in
  `buildLayer`, which walks the children with the freeze context.
- `WalkContext` gains `compositionEndSec` (the root `rangeEndSec`), `frozen`,
  and `idPrefix`. `childFreezeContext(node, layerId, context)` returns
  `time = frame / fps`, `originSec = 0`, `rangeStartSec = 0`,
  `rangeEndSec = compositionEndSec`, the inherited `lang`, `frozen = true`,
  and `idPrefix = layerId + '/'`. It does not inherit the parent's range:
  `childSequenceContext` drops layers when the range is empty, so a freeze in
  a short `<Sequence>` would otherwise lose its world.
- `walkNode` skips the whole subtree in an audio-only walk, and otherwise
  builds the group with a throwaway audio array.
- `collectCharacterViewOverrides` descends through `'freeze-frame'` with the
  same child context, so both passes gate inner `<Sequence>`s on the same
  time. The override map stays shared: it is keyed by host node, and copies
  are distinct nodes.
- A `<Dialogue>` with an unresolved view throws the scoped message above when
  `context.frozen`.
- `buildLayer` applies `context.idPrefix` to authored ids.

### Tests (`packages/react/test/freeze-frame.test.mjs`)

1. Inside, `useCurrentFrame()` is `N` and `durationInFrames` is the
   composition's, also when the freeze is inside a `<Sequence>`.
2. Inner `<Sequence>`s open and close against `N`; a freeze inside a short
   `<Sequence>` keeps its world.
3. Inner `<Audio>` and voiced `<Dialogue>` are collected neither by `renderAt`
   nor by `collectAudio`.
4. With one ref used by a `CharacterView` and `Dialogue` both outside and
   inside, each `Dialogue`'s expression lands on its own view, and unmounting
   the copy leaves the live ref attached.
5. Inner lip sync and blinking match the original frame `N`.
6. A `<Dialogue>` inside whose view is outside throws; in nested freezes the
   innermost one scopes.
7. An inner `<Video>`'s `sourceTimeSeconds` follows `N`; authored ids inside
   are prefixed.

### Docs

`packages/website/src/docs-content.tsx`, `skills/celesta/references/react-core.md`,
`docs/handoff/2026-10-06-freeze-frame.md`, and its line in `HANDOFF.md`.

## Out of scope

- A hook returning the absolute composition frame inside a `<Sequence>` (for
  `frame={absoluteNow - 90}`). Add it when a composition needs it.
- Sound from frozen children.
