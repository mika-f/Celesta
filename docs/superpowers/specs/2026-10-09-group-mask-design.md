# Group alpha / luminance masks design (issue #146)

## Goal

A `Group` can already be limited to a rectangle with `clip`, rounded or not.
It cannot be cut out by an arbitrary shape, glyph, or image: video inside
letters, a circular wipe, or a soft-edged reveal. `<Mask>` lets another drawn
subtree decide, per pixel, how much of a group shows.

```tsx
// Video inside letters.
<Group>
  <Mask>
    <Text style={{ fontSize: 320, fontWeight: 900 }}>CELESTA</Text>
  </Mask>
  <Video src="./clip.mp4" />
</Group>

// A circular wipe from scene A to scene B.
<SceneA />
<Group>
  <Mask>
    <Circle x={960} y={540} anchorX={0.5} anchorY={0.5} radius={radius} fill="#ffffff" />
  </Mask>
  <SceneB />
</Group>
```

The existing rectangle `clip` is unchanged.

## Scene model (`celesta-composition`)

```rust
LayerContent::Group {
    layers: Vec<Layer>,
    clip: Option<Clip>,
    /// Shows the children only where this subtree is drawn; see `Mask`.
    mask: Option<Mask>,
}

pub struct Mask {
    /// Drawn in the group's own coordinate space, like `layers`; never shown.
    pub layers: Vec<Layer>,
    pub mode: MaskMode,   // default Alpha
    pub invert: bool,     // default false
}

pub enum MaskMode { Alpha, Luminance }
```

- JSON: `{"type":"group","layers":[…],"mask":{"layers":[…],"mode":"luminance","invert":true}}`.
  `mask` is omitted when `None`, `mode` when `alpha`, and `invert` when false.
  Added to `LayerContentDef` as well, so tagged deserialization keeps working.
- ts-rs exports `Mask` and `MaskMode`; `pnpm run codegen` regenerates
  `packages/react/src/generated/`.
- Project JSON (`celesta-project`) does not get masks. The evaluator builds
  groups with `mask: None`.

## Behavior

### Mask value

The mask subtree is drawn onto a transparent canvas of its own. Each pixel of
that canvas gives a value `m` in 0–1:

- `alpha`: the canvas alpha.
- `luminance`: `(0.2126 R + 0.7152 G + 0.0722 B) × A`, with the sRGB-encoded
  channel values used as they are (no linearization). This is the Rec. 709
  luma of the straight color times its alpha. On a premultiplied canvas it is
  just the dot product of the premultiplied color with the weights.
- `invert`: `m` becomes `1 - m`.

Where the subtree draws nothing, the canvas is transparent: `m = 0` (or 1
when inverted). An empty `layers` hides the group, or shows it unmasked when
inverted.

### Coordinates and drawing of the mask subtree

- The mask layers are positioned in the group's own coordinate space, the
  space the children's `x`/`y` are given in. The mask moves, scales, and
  rotates with the group.
- The mask is never shown. To show the shape as well, draw it again as a
  sibling.
- The mask layers start at opacity 1 and normal blending: the group's own
  `opacity` and `blendMode` apply to the masked result only. A mask layer's
  own opacity counts, so a layer at 50 % opacity shows the group at 50 %.
- The mask layers are drawn without any clip: neither the group's `clip` nor
  an ancestor's. The children already carry those clips, so the masked result
  is clipped exactly once, and anti-aliased edges are not attenuated twice. A
  group with its own `clip` inside the mask subtree still clips its own
  children there.
- Blend modes, effects, nested groups, and nested masks inside the mask
  subtree work as anywhere else. They composite onto the mask canvas, never
  onto the scene. A blurred circle gives a soft-edged wipe.

### Order of application

For a group layer with a mask:

1. The children composite onto a transparent canvas through their own blend
   modes and clips. A masked group is always isolated, as a group with a
   blend mode other than `normal` is. A child's blend mode mixes with its
   siblings beneath it, not with the scene behind the group.
2. Every pixel of that canvas, all four premultiplied channels, is
   multiplied by `m`.
3. The layer's effects (blur, then shadow and glow behind) are applied to the
   masked result. A shadow follows the cut-out shape, not the uncut children.
4. The result composites onto the parent through the group's `opacity`,
   `blendMode`, and the parent's clips, as any isolated group does today.

Nested masks compose: a masked group inside the children is masked first,
and the outer mask then applies to everything, including it.

## React API (`@celesta/react`)

```ts
export interface MaskProps {
  /** What decides visibility: the mask's alpha (default) or its luminance. */
  mode?: 'alpha' | 'luminance';
  /** Shows the group where the mask is not drawn instead. Defaults to false. */
  invert?: boolean;
  children?: ReactNode;
}
export function Mask(props: MaskProps): ReactElement;
```

- `<Mask>` renders a `'mask'` host element. It is a direct child of
  `<Group>`, at any position among the siblings; its children become
  `mask.layers`, and the other children stay `layers`.
- A registered component is rendered inside an internal `'group'`
  (`ResolvedProjectLayer`, `ResolverHost`), so a `<Mask>` at the top of its
  output masks that wrapper, which is the component's own output. This is
  allowed.
- `<Mask>` has no layer of its own and no transform props. To move the mask
  separately from the children, wrap its children in a `<Group>`.
- Errors thrown by the walker:
  - `<Mask>` anywhere other than directly under `<Group>`, including under
    `<Sequence>` and `<FreezeFrame>`: "`<Mask>` must be a direct child of
    `<Group>`".
  - Two `<Mask>`s under one `<Group>`: "a `<Group>` takes at most one
    `<Mask>`".
  - A `mode` other than the two: "unknown mask mode …; expected one of alpha,
    luminance", in the style of `extractBlendMode`.
- Inside the mask, time, `lang`, and fonts work as in the group's other
  children. Mask layers get tree-path ids like any other layer, so a
  `<Video>` in the mask and one in the children keep separate decode
  sessions.
- The mask subtree is silent: `<Audio>` and voiced `<Dialogue>` inside it are
  collected neither by `renderAt` nor by the audio-only walk, as inside
  `<FreezeFrame>`. A mask is a matte, not something heard.

## Implementation

### `celesta-composition`

Add `Mask`, `MaskMode`, and the `mask` field (in `model.rs` and
`LayerContentDef`). Update every `LayerContent::Group` constructor.

### Traversals that must see the mask subtree

A `LayerContent::Group { layers, .. }` match skips a new field silently. Each
traversal that must recurse names `mask` explicitly instead of `..`, so a
later field also fails to compile there:

- `crates/exporter/src/project.rs` `absolutize_layers`: assets inside masks
  are resolved too.
- `crates/editor/src/preview.rs`: `collect_component_requests`,
  `strip_missing_components`, `splice_resolved_components`.
- `crates/bench`, `crates/gpu-renderer/examples`, and test helpers build
  groups with `mask: None`.
- `packages/react/src/render.ts` `inheritTextLanguage` and
  `packages/react/src/cli.ts` `compactLayers` recurse into `mask.layers`.

### CPU reference renderer (`crates/renderer/src/layer.rs`)

In the `Group` branch, when `mask` is set:

1. Render the children onto a transparent `RgbaFrame` with the child state
   at opacity 1 and normal blending, as the isolated path does now.
2. Render `mask.layers` onto another transparent frame with the child
   state's position and scale, opacity 1, normal blending, and `clip: None`.
3. Per pixel, compute `m` from the mask frame. The frames hold straight
   alpha, so luminance multiplies the luma by alpha explicitly. Scale the
   children frame's alpha by `m`.
4. Composite with `blend_with_mode(…, state.opacity, state.blend_mode)`.

`render_effect_layer` already draws the layer without its effects and then
filters the result, so step 3 of the order of application needs no extra
work. The CPU renderer still rejects rotation, so CPU tests stay
axis-aligned.

### GPU renderer (`crates/gpu-renderer`)

- `layer.rs` `PreparedItem` gains `BeginMask`, `MaskContent`, and
  `EndMask(PreparedLayer, MaskSpec)`. `prepare_layer` emits `BeginMask`, the
  mask layers (state as for the CPU: child transform, opacity 1, no clip),
  `MaskContent`, the children (child state, opacity 1), and `EndMask` with a
  canvas layer at the group's opacity and blend mode and `clip: None`.
  `MaskSpec` carries the mask's mode and invert. A
  masked group always takes this path, whatever its blend mode.
- `plan.rs` `plan_groups` plans one region for both canvases. The bounds
  are the children's content bounds intersected with the mask's, or the
  children's alone when inverted. An empty region marks the group
  `drawn: false`, so a mask that lies entirely off the children draws
  nothing. As for other undrawn groups, both canvases are then the 1×1
  canvas none of the layers can touch, so the items still have a target.
- `GpuStep` gains matching steps. `compositor.rs` `draw_group` draws the
  mask steps onto one pooled canvas and the children onto another, then
  runs a mask pass into a third. It returns that canvas to be drawn onto the
  parent with the `EndMask` instance, like an isolated group's.
- The mask pass is a new pipeline in a new `mask.wgsl`. It draws a
  full-screen triangle that `textureLoad`s the children canvas (group 0) and
  the mask canvas (group 1, the same `texture_layout`) at the same texel, and
  writes `children × m`. Mode and invert are pipeline-overridable constants:
  four pipelines, no uniform. The pass is scissored to the planned region.
- Instance and filter-pass accounting (`instance_count`, `begin_frame`'s
  pass reservation) counts the new items. `composited` becomes true for a
  frame with a mask, as it does for any group canvas.
- Scenes without masks take exactly the code paths they take today.

## Testing

### Rust

- `celesta-composition`: serde round trip; `mask`, `mode`, and `invert` are
  omitted at their defaults; a `group` without `mask` still deserializes.
- `celesta-renderer`: an alpha mask of a `Path` circle cuts a `Rect`;
  luminance with a gray mask gives the expected partial alpha; invert; an
  empty mask; the mask follows the group's position and scale; mask plus
  `clip` (clipped once); a child with `multiply` blends only with its
  siblings; a shadow follows the cut-out; nested masks; group opacity
  applies once; a mask layer's own opacity counts.
- `celesta-gpu-renderer`: each case above matches the CPU renderer within
  the existing parity tolerance (`clips_groups_like_the_cpu_renderer` is the
  model). GPU-only cases: a rotated masked group against expected pixel
  samples; a mask entirely off the children draws nothing, and inverted
  draws the children; a `Text` mask and an `Image` mask.
- `celesta-exporter`: a React entry with a masked group renders through the
  export path (`png_integration.rs`) and matches the GPU renderer's frame.
- Preview: the editor's native preview draws with the same `GpuRenderer`,
  so the parity tests cover its pixels. Opening `with-mask.tsx` in the
  editor and scrubbing through both halves checks the preview wiring.

### React (`packages/react/test/mask.test.mjs`)

1. `<Group><Mask>…</Mask>…</Group>` serializes `mask.layers` apart from
   `layers`, at any sibling position, with `mode` and `invert` omitted at
   their defaults.
2. Each walker error above throws its message.
3. Mask layers get unique tree-path ids, and inherit `lang`.
4. `<Audio>` inside `<Mask>` is collected neither by `renderAt` nor by
   `collectAudio`.
5. `compactLayers` and `inheritTextLanguage` reach `mask.layers`.

### Performance

Add a `masks` workload to `crates/bench/src/workloads.rs`: text masking a
moving image, plus a blurred circular wipe. Record it in
`docs/performance/` with reproduction steps and within-run ratios against
the `text` workload. The existing workloads must not slow down.

## Examples

`packages/react/examples/with-mask.tsx`: video inside letters for the first
half (a `<Video src="./clip.mp4">`, as `with-video.tsx` does), then a
circular wipe with a blurred edge between two scenes for the second half.

## Docs

- `packages/website/src/docs-content.tsx` and its `en.json` / `ja.json`
  keys, next to "Clip a group".
- `skills/celesta/references/react-core.md`.
- A handoff note `docs/handoff/<date>-group-masks.md`, dated the day it
  lands, and its line in `HANDOFF.md`.

## Out of scope

- Masks on non-group layers (a `mask` on any `Layer`). Wrap the layer in a
  `<Group>`.
- Masks in project JSON timelines.
- Linear-light luminance, or luma weights other than Rec. 709.
- A mask that is shown as well (After Effects' visible matte layer). Draw it
  twice.
