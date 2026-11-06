# turntable

A sample content plugin with a program that draws in 3D: `<Turntable>`
shows a checkered cube, toon-shaded and outlined, turning on a turntable
under one light, a half-transparent blue pane in front of it, and a leaf cut
out of a square, its stem stuck in the cube.

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
  checker and a leaf as inline textures; the cube's shader; and three
  materials. The cube's material is the sample's own WGSL toon shader,
  [`src/toon.wgsl`](src/toon.wgsl), sent with the model: Celesta compiles
  it against its scene material interface and binds the material's
  parameters and its one texture slot, the checker. The material is opaque,
  and its 64 bytes of parameters set what the shader draws:
  - `shade`: the shaded color, a bluish lilac times the checker, and the
    shading shift, −0.05, added to N·L before the step. The checker is the
    lit color, and the step between them is nearly hard (`shape`'s toony
    0.95), so a face turned from the light goes cool rather than only
    darker.
  - `rim`: a faint blue-grey and a fresnel power of 8, so the rim lifts
    only the faces seen at a slant, such as the top, lit by the light and
    the ambient light.
  - `outline`: a near-black color, mixed halfway towards the surface's.
  - `shape`: the toony factor, and the outline's width, 2% of the
    canvas's height where a metre fills it.

  The outline is the material's outline pass, drawn behind the cube with
  front faces culled. Its vertex hook moves each vertex in clip space,
  along the normal as seen on screen, as MToon's screen-coordinates
  outline does: up to about 5 of the layer's pixels where the face beyond
  the silhouette is seen edge-on, fewer where that face turns toward the
  camera. The other two materials are unlit, one blended and one cut out
  where the leaf's alpha is below 0.5, so the light leaves them as they
  were.
  `frame` answers with the camera, standing at (0, 1.2, 3) and looking at
  the cube; the lights, one white directional light of intensity 0.8
  travelling along (−0.65, −0.65, 0.3), so from above, behind, and to the
  right, and a flat ambient, (0.22, 0.22, 0.24) in the constant coefficient
  alone; and where each draw is: the cube turned about +Y by the speed
  times the time, the pane and the leaf where they always are. Light and
  ambient add up to about 1 (1.02, and the rim adds to that), so the
  checker's light squares stay below white, though a lit white texel would
  not.
  From the camera, the top and a face turned to the right are lit, and a
  face turned to the left or squarely toward the camera is in shade. Celesta
  draws the cube and the leaf first, writing depth, so the cube hides the
  leaf's stem, and then the pane over them.
- The outline pushes each vertex out along its normal. The cube's faces
  keep their own normals, four vertices a face, so at a corner the outline
  of each face moves its own way and leaves a notch: an outline drawn this
  way breaks on hard edges, as MToon's does in UniVRM. A model whose
  outline must close shares its normals across the edge. For the same reason the rim, which
  follows each face's normal, lifts a whole face evenly instead of glowing
  along its edge.
- [`src/main.rs`](src/main.rs) serves that provider over standard input and
  output with `celesta_plugin::serve`.
- [`tests/process.rs`](tests/process.rs) runs it end to end: Celesta's plugin
  host starts the built program from a plugin directory, its answers match
  the provider's, and the GPU renderer draws them.

Run `node scripts/plugins/build.mts examples/plugins/turntable` at the
repository root to make its plugin directory, then `demo.tsx` exports as
the parent README shows.
