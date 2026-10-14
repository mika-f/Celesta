# Group masks in React and the browser (2026-10-14)

Issue #146, the second half. PR #205 added `GroupMask` to the scene model
and both native renderers. This change makes masks usable from React and
draws them in `@celesta/web`. The design is in
`docs/superpowers/specs/2026-10-09-group-mask-design.md`; this is Tasks 6–11
of `docs/superpowers/plans/2026-10-09-group-masks.md`, plus the browser
renderer, which that plan left out.

- React: `<Group mask={…}>` takes any drawing. `<Mask mode="luminance"
  invert>` sets how it is read, as the prop's value or as a direct child of
  the group. `Group` renders the mask as its last host child, after a
  fragment holding the children, so turning the mask on or off keeps the
  children's tree-path ids and state. The fragment also keeps React from
  asking a static list of children for keys. The walker (`buildLayer` in
  `render.ts`) takes the `mask` host child out of the group's children.
  Anywhere else, `<Mask>` throws `` `<Mask>` must be a direct child of
  `<Group>` ``. The audio-only walk still descends into it, so audio
  inside a mask plays. `inheritTextLanguage` and `recordBuiltLayers`
  reach mask layers. Compact transforms already did, since mask layers go
  through `buildLayer`. The plan's `cli.ts` `compactLayers` no longer
  exists.
- `browser.ts` has its own export list. `Mask` has to be on it, or
  `@celesta/web`'s build fails where `@celesta/transitions` imports it.
- `@celesta/web`: a masked group is always isolated, as in the native
  renderers. Its children, with the group's clip, draw onto one canvas.
  Its mask layers, at full opacity and without clips, draw onto another.
  `applyMask` then keeps the children with `destination-in` (or
  `destination-out` when inverted). For `luminance`, the mask canvas is
  first turned into an alpha mask of `(0.2126 R + 0.7152 G + 0.0722 B) × A`
  of its straight color, read with `getImageData`. The result composites
  onto the parent once, at the group's opacity and blend mode, through the
  parent's clips. `shapeBox`, which places a vignette, uses the GPU plan's
  box: the children's box, intersected with the mask's unless the mask is
  inverted. `types.ts` is hand-written and needed the `mask` field.
- Checks: `packages/react/test/mask.test.mjs`, a masked group in
  `packages/cli/test/compact-transforms.test.mjs`, an exported PNG in
  `crates/exporter/tests/png_integration.rs`
  (`react_png_draws_a_masked_group`), and mock-canvas tests in
  `packages/web/test/scene-canvas.test.mjs`. A browser run of the same
  scenes as the native export differed only along anti-aliased edges.
- Example: `packages/cli/examples/with-mask.tsx` (video inside letters,
  then a blurred circular wipe). Bench: a `masks` workload; see
  `docs/performance/group-masks.md`.
- `HANDOFF.md` no longer says general masks are missing.
