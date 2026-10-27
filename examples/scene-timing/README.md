# Scene timing

Ten seconds at 30 fps in three scenes, Intro, Main, and Outro, for trying
timing on the editor's timeline. The scenes come from a `defineScenes` list
whose lengths are numbers written in each entry, and the composition is as
long as the list (`durationInFrames={SCENES.durationInFrames}`), so every
scene's end drags on the **Chapters** row.

- `film.tsx`: the composition. Open it with Celesta using **File → Open…** and
  turn on **Select layers**.

## What to try

- Drag the end of Intro on the **Chapters** row: its entry's
  `durationInFrames` changes, Main and Outro move, and so does the
  composition's end.
- Replace `SCENES.durationInFrames` on `<Composition>` with `300` and drag
  Intro's end to the right: the drag stops 89 frames on, before Outro would
  start at the composition's end, with **The composition is 300 frames
  long** beside the end.
- Select **A Sequence of its own** in Intro and drag its band's start edge,
  end edge, and middle: `from` and `durationInFrames` change.
- Select the circle in Main: its `<Sequence>`'s `from` is written as an
  expression, so the start edge and the middle are locked and say why; the
  end drags.
- Select Intro's dark background, move the playhead 45 frames into Intro,
  and click **Set timing** on its band: the `<Rect>` is wrapped in
  `<Sequence from={45} durationInFrames={45}>`. The import stays as it is,
  since `film.tsx` already imports `Sequence`; in a file that doesn't, the
  same click adds it.
- Select the **Scene timing** title: **Set timing** is disabled, since its
  `<Text>` is the root of `Title`. Choose **Select** on the `<Group>` row
  under **Called from** and click **Set timing** there: the `<Group>` is
  wrapped, and the title's fade, which `Title` reads from its own frame, now
  starts at the playhead.
- Select the orange tag's `<Rect>`, which Intro and Main both draw: **Set
  timing** is disabled under **This scene** and wraps the `<Rect>` inside
  `Tag` under **Shared value**, for both scenes.
- Select the light price tag's `<Rect>` in Main, which Intro draws too, and
  set its width under **Edits change**: **This scene** gives `PriceTag` a
  `rectWidth` prop that only Main's call passes, while **Shared value**
  changes both scenes.
- Select the small mark at the top right, which every scene draws, choose
  **Shared value**, and change its fill: every scene changes.

## Reproduce

From the repository root, with a source build:

```sh
node skills/celesta/scripts/inspect.mjs examples/scene-timing/film.tsx --frames 0,45,150,250
target/release/celesta-exporter --json --react examples/scene-timing/film.tsx examples/scene-timing/scene-timing.mp4
```
