# Path motion

Moving along a path and morphing between paths with `@celesta.studio/shapes`.
Six seconds at 30 fps, 1920 × 1080, in two halves:

1. **Along a path.** A route written as SVG path data, with cubic, smooth
   cubic (`S`), arc (`A`), and quadratic segments, goes through
   `parseSvgPath` and draws itself on with `<Path progress>`. An arrowhead
   rides the tip at `pointOnPath`, turned by `tangentOnPath(...).angle`, and
   small arrowheads mark the tangent every 80 px from `getPointAtLength` and
   `getTangentAtLength`. The caption shows the drawn length against
   `getLength` and the heading in degrees.
2. **Between paths.** `interpolatePath` morphs a circle (eight cubics from
   two SVG arcs), a five-pointed star (ten straight sides), and a heart (six
   cubics) into one another. Each holds for a second, then morphs for one.

The composition is `film.tsx`. Inter and JetBrains Mono come from Google
Fonts, so the first export needs a network connection. From the repository
root:

```sh
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" \
  --react examples/path-motion/film.tsx --frame 45,100,165 examples/path-motion/frame.png
"/Applications/Celesta.app/Contents/MacOS/Celesta-export" \
  --react examples/path-motion/film.tsx examples/path-motion/path-motion.mp4
```
