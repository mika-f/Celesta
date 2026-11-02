# turntable

A sample content plugin with a program that draws in 3D: `<Turntable>`
shows a checkered cube turning on a turntable, a half-transparent blue pane
in front of it, and a leaf cut out of a square, its stem stuck in the cube.

```tsx
import { Turntable } from '@celesta-plugin/turntable';

<Turntable x={48} y={190} speed={0.25} />
```

| Prop | Meaning |
| --- | --- |
| `speed` | Turns of the cube each second; negative turns it the other way. Default 0.5. |

The layer is 640x360. The common props (`x`, `y`, `scale`, `opacity`, …)
work as on any layer.

## How it works

- [`js/index.ts`](js/index.ts) is the module entries import. `<Turntable>`
  renders a host element, and the function registered for it turns its
  props into plugin content: plugin `turntable`, no assets, the sequence's
  time, and the speed as `data`.
- [`src/lib.rs`](src/lib.rs) is the plugin's `ContentProvider`. `open`
  answers with a scene model it makes itself: a cube of 24 vertices, four a
  face so each face has its own texture coordinates, and a square; a 2x2
  checker and a leaf as inline textures; and three unlit materials, one
  opaque, one blended, and one cut out where the leaf's alpha is below 0.5.
  `frame` answers with the camera, standing at (0, 1.2, 3) and looking at
  the cube, and where each draw is: the cube turned about +Y by the speed
  times the time, the pane and the leaf where they always are. Celesta
  draws the cube and the leaf first, writing depth, so the cube hides the
  leaf's stem, and then the pane over them.
- [`src/main.rs`](src/main.rs) serves that provider over standard input and
  output with `celesta_plugin::serve`.
- [`tests/process.rs`](tests/process.rs) runs it end to end: Celesta's plugin
  host starts the built program from a plugin directory, its answers match
  the provider's, and the GPU renderer draws them.

Run `node ../build.mjs turntable` to make its plugin directory, then
`demo.tsx` exports as the parent README shows.
