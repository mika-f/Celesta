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
- Select the **Scene timing** title in Intro, move the playhead into Intro,
  and click **Set timing** on its band: the title is wrapped in a
  `<Sequence>` from the playhead to the end of Intro, and its fade starts
  there.
- Select the orange tag's `<Rect>`, which Intro and Main both draw: **Set
  timing** is disabled under **This scene** and wraps the `<Rect>` inside
  `Tag` under **Shared value**.

## Reproduce

From the repository root, with a source build:

```sh
node skills/celesta/scripts/inspect.mjs examples/scene-timing/film.tsx --frames 0,45,150,250
target/release/celesta-exporter --json --react examples/scene-timing/film.tsx examples/scene-timing/scene-timing.mp4
```
