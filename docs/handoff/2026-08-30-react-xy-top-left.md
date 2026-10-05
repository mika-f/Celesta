# `@celesta/react` `x`/`y` place the top-left corner (2026-08-30)

`extractTransform` in `packages/react/src/render.ts` now defaults `anchor` to
`{ x: 0, y: 0 }` instead of `{ x: 0.5, y: 0.5 }`, so a component's `x`/`y`
address its top-left corner (CSS/canvas/Remotion convention) rather than its
centre. Nothing changed in the Rust renderers or the `Scene` /
`EvaluatedTransform` contract — the renderers already position a layer as
`position - size * anchor` and pivot `scale`/`rotation` about the same anchor;
only the React default the bridge emits moved.

- The `anchor` a layer carries is still both the positioning reference **and**
  the `scale`/`rotation` pivot, so `<Text anchorX={0.5} anchorY={0.5}>` brings
  back the old behaviour (centre-anchored, spins/scales about the centre) and
  `x`/`y` then address that centre. `<Group>` has no size and ignores
  `anchor`: its children are placed relative to its `x`/`y` and it scales
  about that point regardless.
- `CommonProps` in `src/components.ts` documents `x`/`y`/`anchorX`/`anchorY`
  accordingly.
- Examples updated to keep their rendered output: `title.tsx`,
  `with-sequence.tsx`, `with-properties.tsx`, `with-project.tsx` add
  `anchorX={0.5} anchorY={0.5}` to their centred captions; `with-rect.tsx`
  and `homepage-demo.tsx`'s full-frame background `<Rect>` switch to `x={0}
  y={0}`; `homepage-demo.tsx`'s card captions (centred on each card's local
  origin) and its animated card `<Rect>` add the 0.5 anchors;
  `character-lipsync-demo.tsx` likewise.
- The `celesta-react-bridge` integration tests assert layer structure/content,
  not transform values, so they are unaffected.
