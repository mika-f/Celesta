`@celesta.studio/lottie` draws a Lottie JSON file, exported from After Effects with Bodymovin, in step with the composition's frames. Each frame is drawn as paths, groups, and masks that depend only on its time, so preview, export, and `@celesta.studio/web` draw the same picture, and scrubbing to any frame gives the same result as playing up to it.

The desktop app, the CLI, and `@celesta.studio/web` include it. It is private and is not published to npm.

## Place an animation

Import the JSON file and pass its contents as `data`.

```tsx
import { Composition, Sequence } from '@celesta.studio/react';
import { Lottie } from '@celesta.studio/lottie';

import confetti from './confetti.json';

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={60} durationInFrames={300}>
      <Lottie data={confetti} x={960} y={540} anchorX={0.5} anchorY={0.5} width={800} />
      <Sequence from={120}>
        <Lottie data={confetti} endFrame={30} playbackRate={2} loop={false} />
      </Sequence>
    </Composition>
  );
}
```

| Props | Use it for |
| --- | --- |
| `data` | The Lottie JSON's contents. |
| `width` / `height` | The size to draw at. Defaults to the file's `w` × `h`; one alone keeps the aspect ratio. Nothing outside this box is drawn. |
| `playbackRate` | Speed. Defaults to 1; negative plays backwards. |
| `loop` | Defaults to true. False holds the frame before `endFrame`. |
| `startFrame` / `endFrame` | The range to play, in the file's own frames. Defaults to its `ip` / `op`. |
| `assetsPath` | The folder image layers' files are in, relative to the entry like an `<Image>` `src`. Defaults to each image asset's `u`. |
| `onWarning` | Receives a message for each feature that is not drawn. Defaults to `console.warn`, once per file. |

The common props (`x`, `y`, `anchorX`, `anchorY`, `rotation`, `opacity`, …) work too. Playback starts at the enclosing Sequence's frame 0. Values interpolate between the file's frames, so a 30 fps file stays smooth in a 60 fps video.

## What is drawn

- Layers: shape, precomposition (with time remap and stretch), solid, image, and null; parenting; blend modes.
- Masks: add, subtract, intersect, inverted, opacity. Track mattes: alpha and luma, and their inverted forms.
- Shapes: groups, paths, rectangles (rounded too), ellipses, stars, polygons, fills, strokes (dashes, caps, joins), linear and radial gradients, and trim paths.

These are not drawn, with a warning: text layers, expressions, effects and layer styles, 3D layers, auto-orient, merge paths, repeaters, round corners, the even-odd fill rule, radial gradient highlights, mask expansion, and embedded (base64) images. Export images as separate files. `inspectLottie(data)` returns the same messages without drawing.
