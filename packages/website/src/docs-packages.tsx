import type { ReactNode } from 'react';
import { DocCode } from './DocCode';
import { Api, Note } from './docs-shared';

// Chapters for the packages that ship beside @celesta/react.

/** Joins example lines, so code reads naturally without escaping newlines. */
const lines = (...rows: string[]) => rows.join('\n');

export const packageContents: Record<string, ReactNode> = {
  'math': <>
    <p><code>@celesta/math</code> is a small, dependency-free toolkit for the numbers behind motion: seeded randomness, smooth noise, number shaping, waves, angles, and 2D points. It is bundled with the runtime, so a composition imports it directly:</p>
    <DocCode label="Import" language="tsx" code={"import { random, randomRange, noise } from '@celesta/math';"} />
    <p>Choose <strong>File → Set Up TypeScript</strong> for editor types (see <a href="/docs/react-compositions/">Your first React composition</a>), or use it in the <a href="/#playground">web editor</a>, which accepts <code>@celesta/react</code>, <code>@celesta/math</code>, and <code>react</code>. The package is not published to npm yet.</p>
    <p>This chapter covers randomness. <a href="/docs/math-noise/">Noise & fbm</a> covers smooth drift, and <a href="/docs/math-shaping/">shaping, waves & geometry</a> covers the everyday helpers.</p>

    <h3>Why not Math.random()?</h3>
    <p>A composition is a function of the frame. The preview scrubs backward, the exporter renders frames in order, and both must draw the same picture for the same frame. <code>Math.random()</code> gives a different answer every call, so stars would jump around each time you scrub. Every function in <code>@celesta/math</code> is a pure function of its inputs: the same seed returns the same value in preview, in export, and on every machine.</p>

    <h3>Seeds</h3>
    <p>A <code>Seed</code> is a number or a string. <code>random(seed)</code> returns a number in <code>[0, 1)</code>, and the same seed always returns the same number. Give each thing you randomize its own seed, and vary it by index and by property:</p>
    <DocCode label="One seed per property" language="tsx" code={lines(
      'const x = randomRange(`star-${i}-x`, 0, 1920);',
      'const y = randomRange(`star-${i}-y`, 0, 1080);',
      'const size = randomRange(`star-${i}-size`, 2, 6);',
    )} />
    <ul>
      <li>Reusing a seed reuses the value. <code>random('a')</code> called twice is the same number, so <code>x</code> and <code>y</code> need different seeds or the stars line up on a diagonal.</li>
      <li>A fractional number is its own seed: <code>0.5</code> does not collide with <code>0</code> or <code>1</code>. A number and a string that spells it, such as <code>0.5</code> and <code>'0.5'</code>, are the same seed.</li>
      <li>A seed must be a string or a finite number. <code>NaN</code> and <code>Infinity</code> throw an error that names the function.</li>
      <li>To make a value change over time, put time in the seed: <code>random(Math.floor(frame / 4))</code> jumps to a new value every four frames, which suits flicker and glitch effects. For smooth change, use <a href="/docs/math-noise/"><code>noise</code></a>.</li>
    </ul>

    <h3>The random functions</h3>
    <Api caption="Randomness" rows={[
      [<code>random(seed)</code>, <>A number in <code>[0, 1)</code>.</>],
      [<code>randomRange(seed, min, max)</code>, <>A number in <code>[min, max)</code>.</>],
      [<code>randomInt(seed, min, max)</code>, <>A whole number from <code>min</code> to <code>max</code>, <strong>both included</strong>. Throws if no whole number lies between them.</>],
      [<code>randomBool(seed, probability?)</code>, <><code>true</code> with the given probability (<code>0.5</code> by default).</>],
      [<code>randomSign(seed)</code>, <><code>-1</code> or <code>1</code>, each half the time. Handy for mirroring a direction or spin.</>],
      [<code>randomPick(seed, items)</code>, <>One element of <code>items</code>. Throws on an empty list.</>],
      [<code>shuffle(seed, items)</code>, <>A shuffled <strong>copy</strong> of <code>items</code>; the original is untouched.</>],
      [<code>randomGaussian(seed, mean?, stdDev?)</code>, <>A bell-curve value: most land within <code>stdDev</code> of <code>mean</code> (defaults <code>0</code> and <code>1</code>), a few land much further out.</>],
      [<code>randomInCircle(seed, radius?, center?)</code>, <>A point spread evenly over a disc, as <code>{'{ x, y }'}</code>. Defaults to radius <code>1</code> about the origin.</>],
    ]} />

    <h3>A seeded starfield</h3>
    <p>Because the seeds are fixed, the field below is identical on every frame; only the drift, from <a href="/docs/math-noise/"><code>noise</code></a>, moves:</p>
    <DocCode label="A starfield that drifts" language="tsx" code={lines(
      "import { noise, randomRange } from '@celesta/math';",
      '',
      'function Stars() {',
      '  const frame = useCurrentFrame();',
      '  return <>',
      '    {Array.from({ length: 80 }, (_, i) => (',
      '      <Rect key={i} width={4} height={4} cornerRadius={2} fill="#ffffff"',
      '        x={randomRange(`star-${i}-x`, 0, 1920) + noise(i, frame / 40) * 30}',
      '        y={randomRange(`star-${i}-y`, 0, 1080)}',
      '        opacity={randomRange(`star-${i}-o`, 0.2, 0.8)} />',
      '    ))}',
      '  </>;',
      '}',
    )} />

    <h3>Choosing and ordering things</h3>
    <p><code>randomPick</code> and <code>shuffle</code> pick from lists you already have, such as a palette or a set of captions. Seed them with something stable, like the item’s index:</p>
    <DocCode label="A palette, and a shuffled order" language="tsx" code={lines(
      "const palette = ['#a68bbf', '#7cf29c', '#f2c97c', '#7cc4f2'];",
      '',
      'const color = randomPick(`bar-${i}-color`, palette);',
      "const order = shuffle('slides', ['intro', 'demo', 'pricing', 'outro']);",
      'const mirrored = randomBool(`bar-${i}-flip`, 0.3); // flipped about 30% of the time',
      'const spin = 45 * randomSign(`bar-${i}-spin`);     // clockwise or counter-clockwise',
    )} />

    <h3>Natural-looking scatter</h3>
    <p>Uniform randomness looks artificial: every value is as likely as any other. <code>randomGaussian</code> clusters values around a center, and <code>randomInCircle</code> spreads points evenly by area instead of crowding the middle of a disc, so a burst of particles looks round rather than starry-centered:</p>
    <DocCode label="A burst of particles" language="tsx" code={lines(
      "import { randomGaussian, randomInCircle } from '@celesta/math';",
      '',
      'const particles = Array.from({ length: 120 }, (_, i) => {',
      '  const p = randomInCircle(`spark-${i}`, 260, { x: 960, y: 540 });',
      '  const size = Math.max(2, randomGaussian(`spark-${i}-size`, 8, 3));',
      '  return <Rect key={i} x={p.x} y={p.y} anchorX={0.5} anchorY={0.5}',
      '    width={size} height={size} cornerRadius={size / 2} fill="#f2c97c" />;',
      '});',
    )} />
    <Note title="Keep the seed, change the look">When a random layout is almost right, change the seed’s prefix (<code>star-</code> to <code>sky-</code>) and the whole field rearranges. Because every value is repeatable, the version you liked in the preview is the version you export.</Note>
  </>,

  'math-noise': <>
    <p>Noise is smooth randomness: nearby inputs give nearby outputs, so values wander instead of jumping. Use it for drift, wobble, handheld shake, floating particles, and organic textures. Like everything in <a href="/docs/math/"><code>@celesta/math</code></a>, it is deterministic: the same seed and coordinates return the same value on every render.</p>

    <h3>1D noise over time</h3>
    <p><code>noise(seed, t)</code> returns a value in <code>[-1, 1]</code>. It picks a random value at each whole number of <code>t</code> and blends smoothly between them, so the scale of <code>t</code> sets the speed: <code>frame / 20</code> changes direction about every 20 frames.</p>
    <DocCode label="A gentle floating motion" language="tsx" code={lines(
      "import { noise } from '@celesta/math';",
      '',
      'function Floating({ children }: { children: React.ReactNode }) {',
      '  const frame = useCurrentFrame();',
      '  return (',
      '    <Group x={noise(\'float-x\', frame / 45) * 24} y={noise(\'float-y\', frame / 60) * 16}',
      '      rotation={noise(\'float-r\', frame / 90) * 3}>',
      '      {children}',
      '    </Group>',
      '  );',
      '}',
    )} />
    <ul>
      <li>Use a <strong>different seed for each property</strong>. With one seed, <code>x</code> and <code>y</code> move in lockstep along a diagonal.</li>
      <li>Larger divisors are slower and calmer: <code>frame / 90</code> drifts, <code>frame / 8</code> trembles.</li>
      <li>Scale by the amount you want: <code>noise(…) * 24</code> moves up to 24 pixels either way.</li>
      <li>Add a per-item offset for a crowd: <code>noise(`dot-${'{i}'}`, frame / 40)</code> keeps each dot on its own path.</li>
    </ul>
    <Note title="Camera shake">The <code>shake</code> prop of <a href="/docs/text-camera-lines/"><code>Camera</code></a> is built for handheld drift. Reach for <code>noise</code> directly when you want to shake something that is not the camera, or to shape the shake yourself.</Note>

    <h3>2D and 3D noise</h3>
    <p><code>noise2D(seed, x, y)</code> samples a smooth field over a plane. Scale the position down, as in <code>x / 200</code>, to stretch the features; the smaller the divisor, the busier the field. <code>noise3D(seed, x, y, z)</code> adds a third coordinate, which you can treat as time to animate a field smoothly.</p>
    <DocCode label="A field of dots that breathe" language="tsx" code={lines(
      "import { noise3D } from '@celesta/math';",
      '',
      'function Field() {',
      '  const frame = useCurrentFrame();',
      '  const dots = [];',
      '  for (let row = 0; row < 12; row++) {',
      '    for (let col = 0; col < 20; col++) {',
      '      // Two coordinates for position, the third for time.',
      "      const n = noise3D('field', col / 6, row / 6, frame / 60);",
      '      const size = 28 + n * 20;',
      '      dots.push(<Rect key={`${row}-${col}`} x={140 + col * 82} y={100 + row * 80}',
      '        anchorX={0.5} anchorY={0.5} width={size} height={size} cornerRadius={size / 2}',
      '        opacity={0.5 + n * 0.5} fill="#a68bbf" />);',
      '    }',
      '  }',
      '  return <>{dots}</>;',
      '}',
    )} />
    <p>Every result lies in <code>[-1, 1]</code>. To turn it into a size, an opacity, or any other range, use <a href="/docs/math-shaping/"><code>remap</code></a> or the arithmetic above: <code>0.5 + n * 0.5</code> maps it to <code>[0, 1]</code>.</p>

    <h3>Layered noise (fbm)</h3>
    <p>One layer of noise moves on a single scale, which looks smooth and a little artificial. Fractal Brownian motion, <code>fbm</code>, adds several layers, each finer and weaker than the last. The result wanders on a large scale and also has small detail, like clouds, smoke, terrain, or flame. <code>fbm</code>, <code>fbm2D</code>, and <code>fbm3D</code> take the same arguments as the plain versions plus an options object, and still return values in <code>[-1, 1]</code>.</p>
    <Api caption="fbm options" rows={[
      [<code>octaves</code>, <>How many layers to add. A whole number of at least 1; the default is <code>4</code>. More layers add finer detail and cost a little more per call.</>],
      [<code>lacunarity</code>, <>How much finer each layer is than the one before. The default is <code>2</code>, doubling the frequency.</>],
      [<code>gain</code>, <>How much weaker each layer is than the one before. A finite number of at least 0; the default is <code>0.5</code>. Lower values keep the result smoother, higher values make it rougher.</>],
    ]} />
    <DocCode label="A rough horizon line" language="tsx" code={lines(
      "import { fbm } from '@celesta/math';",
      '',
      'const points: [number, number][] = Array.from({ length: 97 }, (_, i) => [',
      '  i * 20,',
      "  700 + fbm('ridge', i / 14, { octaves: 5, gain: 0.55 }) * 120,",
      ']);',
      '',
      '<Polyline points={points} stroke="#a68bbf" strokeWidth={3} />',
    )} />
    <p>Invalid options throw an error that names the function, such as <code>fbm2D() requires a whole number of octaves of at least 1</code>.</p>

    <h3>Which one do I use?</h3>
    <div className="doc-table-wrap" tabIndex={0} aria-label="Choosing a noise function"><table><caption>Choosing a noise function</caption><thead><tr><th>You want</th><th>Use</th></tr></thead><tbody>
      <tr><td>One value that drifts over time</td><td><code>noise(seed, frame / n)</code></td></tr>
      <tr><td>A value that wanders on several scales</td><td><code>fbm(seed, frame / n)</code></td></tr>
      <tr><td>A pattern across the screen</td><td><code>noise2D</code> or <code>fbm2D</code> sampled at scaled positions</td></tr>
      <tr><td>A pattern across the screen that moves</td><td><code>noise3D</code> or <code>fbm3D</code> with time as the third coordinate</td></tr>
      <tr><td>A new, unrelated value each frame or beat</td><td><a href="/docs/math/"><code>random</code></a> with the frame in the seed</td></tr>
    </tbody></table></div>
  </>,

  'math-shaping': <>
    <p>The remaining helpers in <a href="/docs/math/"><code>@celesta/math</code></a> reshape numbers, build repeating motion, and do the angle and point arithmetic that layout and orbits need. They are plain functions, so you can use them anywhere: in a component, in <code>prepare()</code>, or in a data file.</p>

    <h3>Shaping numbers</h3>
    <Api caption="Scalars" rows={[
      [<code>clamp(value, min, max)</code>, <>Limits <code>value</code> to <code>[min, max]</code>. <code>clamp01(value)</code> limits it to <code>[0, 1]</code>.</>],
      [<code>lerp(a, b, t)</code>, <>The value <code>t</code> of the way from <code>a</code> to <code>b</code>. <code>t</code> is not clamped, so values outside 0–1 extrapolate.</>],
      [<code>inverseLerp(a, b, value)</code>, <>The opposite: how far <code>value</code> is from <code>a</code> to <code>b</code>, as a <code>t</code>. Returns <code>0</code> when <code>a === b</code>.</>],
      [<code>remap(value, inMin, inMax, outMin, outMax)</code>, <>Moves a value from one range to another, without clamping. <code>remapClamped</code> keeps the result inside the output range.</>],
      [<code>step(edge, x)</code>, <><code>0</code> below <code>edge</code>, <code>1</code> from <code>edge</code> on.</>],
      [<code>smoothstep(edge0, edge1, x)</code>, <>A smooth 0–1 ramp as <code>x</code> goes from <code>edge0</code> to <code>edge1</code>. <code>smootherstep</code> has an even gentler start and finish.</>],
      [<code>fract(x)</code>, <>The fractional part, always in <code>[0, 1)</code>, also for negative numbers.</>],
      [<code>mod(value, divisor)</code>, <>Modulo with the sign of the divisor: <code>mod(-1, 4)</code> is <code>3</code>, not <code>-1</code>.</>],
      [<code>wrap(value, min, max)</code>, <>Wraps into <code>[min, max)</code>, for looping positions and hues.</>],
      [<code>pingPong(value, length)</code>, <>Bounces a steadily increasing value back and forth between <code>0</code> and <code>length</code>.</>],
      [<code>snap(value, increment)</code>, <>Rounds to the nearest multiple of <code>increment</code>, for stepped or grid-locked motion.</>],
      [<code>roundTo(value, decimals?)</code>, <>Rounds to a number of decimals, for tidy on-screen readouts.</>],
      [<code>approxEqual(a, b, epsilon?)</code>, <>Whether two numbers differ by at most <code>epsilon</code> (<code>1e-6</code> by default).</>],
    ]} />
    <Note title="remap or interpolate?"><code>remap</code> is a plain linear conversion with no easing and, unless you use <code>remapClamped</code>, no clamping. For motion over frames, with easing and <code>extrapolateLeft</code>/<code>extrapolateRight</code>, use <a href="/docs/animation/"><code>interpolate</code></a> from <code>@celesta/react</code>. <code>remap</code> shines when converting other kinds of values, such as turning a noise result into an opacity.</Note>
    <DocCode label="Loops and bounces" language="tsx" code={lines(
      "import { pingPong, snap, wrap } from '@celesta/math';",
      '',
      'const frame = useCurrentFrame();',
      'const ticker = wrap(frame * 4, -200, 1920);   // scrolls right, re-enters at the left',
      'const sweep = pingPong(frame * 8, 1600);      // 0 → 1600 → 0, repeating',
      'const stepped = snap(sweep, 100);             // the same sweep, in 100 px jumps',
    )} />

    <h3>Waves</h3>
    <p>The four wave functions repeat with a <strong>period of 1</strong> and return values in <code>[-1, 1]</code>, so you pass <code>frame / framesPerCycle</code>. They are in phase: each is positive for the first half of a cycle and negative for the second, so you can swap one for another.</p>
    <Api caption="Waves" rows={[
      [<code>sineWave(t)</code>, <>A smooth sine. Gentle pulses, bobbing, breathing.</>],
      [<code>triangleWave(t)</code>, <>Straight ramps up and down. Constant-speed back-and-forth.</>],
      [<code>squareWave(t)</code>, <><code>1</code> for the first half of each cycle, <code>-1</code> for the second. On/off blinking.</>],
      [<code>sawtoothWave(t)</code>, <>Ramps from 0 up to 1, drops to -1 halfway, and ramps back to 0. Repeating sweeps.</>],
    ]} />
    <DocCode label="A breathing circle on a 2-second cycle" language="tsx" code={lines(
      "import { sineWave } from '@celesta/math';",
      '',
      'const { fps } = useVideoConfig();',
      'const frame = useCurrentFrame();',
      'const breath = sineWave(frame / (fps * 2)); // one full cycle every two seconds',
      '',
      '<Rect x={960} y={540} anchorX={0.5} anchorY={0.5} width={200} height={200}',
      '  cornerRadius={100} scale={1 + 0.08 * breath} fill="#a68bbf" />',
    )} />

    <h3>Angles</h3>
    <p>Functions that take or return angles use <strong>radians</strong>. A layer’s <code>rotation</code> prop is in <strong>degrees</strong>, so convert with <code>radToDeg</code> when you hand it a computed angle.</p>
    <Api caption="Angles" rows={[
      [<code>TAU</code>, <>A full turn in radians (2π).</>],
      [<><code>degToRad(degrees)</code>, <code>radToDeg(radians)</code></>, <>Convert between degrees and radians.</>],
      [<code>normalizeAngle(angle)</code>, <>Wraps an angle into <code>[-π, π)</code>.</>],
      [<code>angleDifference(from, to)</code>, <>The shortest signed turn from one angle to another.</>],
      [<code>lerpAngle(a, b, t)</code>, <>Interpolates between angles the short way round, so 350° to 10° passes through 0° instead of sweeping back through 180°.</>],
    ]} />

    <h3>Points and geometry</h3>
    <p>Points are plain <code>{'{ x, y }'}</code> objects, typed as <code>Vec2</code>. Celesta’s y axis points down, so a positive angle turns <strong>clockwise</strong> on screen, the same as <code>rotation</code>.</p>
    <Api caption="2D points" rows={[
      [<code>distance(a, b)</code>, <>The straight-line distance between two points.</>],
      [<code>angleBetween(from, to)</code>, <>The direction from one point to another, in radians; <code>0</code> points along +x.</>],
      [<><code>lerpPoint(a, b, t)</code>, <code>midpoint(a, b)</code></>, <>A point along a segment, and the point halfway.</>],
      [<code>rotatePoint(point, angle, origin?)</code>, <>Turns a point about an origin (the coordinate origin by default).</>],
      [<><code>polarToCartesian(angle, radius, center?)</code>, <code>cartesianToPolar(point, center?)</code></>, <>Convert between a direction-and-distance and an <code>{'{ x, y }'}</code>. The second returns <code>{'{ angle, radius }'}</code>.</>],
      [<><code>quadraticBezierPoint(p0, p1, p2, t)</code>, <code>cubicBezierPoint(p0, p1, p2, p3, t)</code></>, <>The point at <code>t</code> (0–1) along a Bézier curve, as drawn by a path’s <code>quadTo</code> and <code>cubicTo</code>. Use them to move an object along a curve you also draw.</>],
    ]} />
    <DocCode label="Dots on a rotating ring" language="tsx" code={lines(
      "import { TAU, polarToCartesian } from '@celesta/math';",
      '',
      'const frame = useCurrentFrame();',
      'const dots = Array.from({ length: 12 }, (_, i) => {',
      '  const angle = (i / 12) * TAU + frame / 60; // evenly spaced, slowly turning',
      '  const p = polarToCartesian(angle, 300, { x: 960, y: 540 });',
      '  return <Rect key={i} x={p.x} y={p.y} anchorX={0.5} anchorY={0.5}',
      '    width={24} height={24} cornerRadius={12} fill="#a68bbf" />;',
      '});',
    )} />
    <DocCode label="A marker riding a curve" language="tsx" code={lines(
      "import { cubicBezierPoint } from '@celesta/math';",
      '',
      'const p0 = { x: 200, y: 800 }, p1 = { x: 500, y: 200 };',
      'const p2 = { x: 1400, y: 200 }, p3 = { x: 1700, y: 800 };',
      'const t = progress(useCurrentFrame(), 0, 90, Easings.easeInOutCubic);',
      'const marker = cubicBezierPoint(p0, p1, p2, p3, t);',
      '',
      '<Path commands={[',
      "  { type: 'moveTo', ...p0 },",
      "  { type: 'cubicTo', x1: p1.x, y1: p1.y, x2: p2.x, y2: p2.y, x: p3.x, y: p3.y },",
      ']} stroke="#57456c" strokeWidth={4} />',
      '<Rect x={marker.x} y={marker.y} anchorX={0.5} anchorY={0.5} width={32} height={32}',
      '  cornerRadius={16} fill="#a68bbf" />',
    )} />
    <p>Both <code>quadraticBezierPoint</code> and <code>cubicBezierPoint</code> use the same control points as a <code>Path</code>’s <code>quadTo</code> and <code>cubicTo</code> commands, so the marker stays on the drawn curve. See <a href="/docs/text-camera-lines/">Text effects, camera & lines</a> for <code>Path</code>.</p>
  </>,

  'code': <>
    <p><code>@celesta/code</code> draws syntax-highlighted source code in a composition. It tokenizes with <a href="https://twinkleplop.pngwn.at">twinkleplop</a> and draws each colored run with Celesta’s own <code>Text</code> and <code>Rect</code>, so code scales, fades, blurs, and exports like any other layer. It is optional: <code>@celesta/react</code> does not depend on it.</p>
    <Note title="Availability">The desktop app and CLI include <code>@celesta/code</code>, and <strong>File → Set Up TypeScript</strong> adds its declarations alongside <code>@celesta/react</code> and <code>@celesta/math</code>. You do not install anything from npm; the package is not published there. The <a href="/#playground">web editor</a> does not support it yet, so use the desktop app or CLI for compositions that import it.</Note>

    <h3>Your first code block</h3>
    <DocCode label="code-block.tsx" language="tsx" code={lines(
      "import { Assets, Composition, Font, Rect } from '@celesta/react';",
      "import { Code, codeThemes } from '@celesta/code';",
      '',
      "const source = `const message: string = 'Hello, Celesta.';",
      'console.log(message);`;',
      '',
      "const style = { fontFamily: 'IBM Plex Mono', fontSize: 40, lineHeight: 60 };",
      '',
      'export default function Root() {',
      '  return (',
      '    <Composition width={1280} height={480} fps={30} durationInFrames={90}>',
      '      <Assets>',
      '        <Font src="./fonts/IBMPlexMono-Regular.ttf" />',
      '      </Assets>',
      '      <Rect width={1280} height={480} fill="#161418" />',
      '      <Code x={64} y={64} language="ts" style={style} theme={codeThemes.dark}>',
      '        {source}',
      '      </Code>',
      '    </Composition>',
      '  );',
      '}',
    )} />
    <p><code>Code</code> takes the source as its only child, as a string. Keep indentation and trailing newlines inside the string: they are part of the picture.</p>

    <h3>Fonts</h3>
    <p>Use a <strong>monospaced</strong> font. <code>Code</code> defaults to JetBrains Mono at 24 px with a line height of 1.5 × the font size. Load the font file with <code>{'<Font src="…" />'}</code>, as above, or install it on the machine that renders. If the font is missing, Celesta falls back as it does for any text.</p>

    <h3>Props</h3>
    <Api caption="Code props" rows={[
      [<code>children</code>, <>The source, as a string.</>],
      [<code>language</code>, <><code>tsx</code>, <code>ts</code>, <code>json</code>, <code>bash</code>, or <code>text</code> (the default, which applies no coloring).</>],
      [<code>style</code>, <>A Celesta <code>TextStyle</code>: <code>fontFamily</code>, <code>fontSize</code>, <code>lineHeight</code>, and so on. Text is left aligned and never wraps, and the theme supplies each run’s fill color. Other <code>align</code> values throw.</>],
      [<code>theme</code>, <>Token colors, a foreground fallback, and the line highlight color. <code>codeThemes.dark</code> by default.</>],
      [<code>tabSize</code>, <>Width of a tab stop, counted in characters. Defaults to <code>2</code>.</>],
      [<code>highlightLines</code>, <>One-based line numbers to mark with a highlight band.</>],
      [<code>highlightWidth</code>, <>Band width in pixels. Defaults to the width of the source; set it to reach the edge of a surrounding panel.</>],
      [<code>visibleCharacters</code>, <>How much of the source to show, for typing effects. See <a href="/docs/code-typing/">Code: typing, carets & tokens</a>.</>],
      [<><code>x</code>, <code>y</code>, <code>scale</code>, <code>rotation</code>, <code>anchorX</code>/<code>anchorY</code>, <code>opacity</code>, <code>blendMode</code>, <code>blur</code>, <code>shadow</code>, <code>glow</code></>, <>The usual layer props apply to the whole block.</>],
    ]} />

    <h3>Line highlights</h3>
    <p><code>highlightLines</code> draws a band behind the lines you name, full width and full line height, even while the code is still being typed:</p>
    <DocCode label="Mark one line" language="tsx" code={lines(
      '<Code x={64} y={64} language="ts" style={style} highlightLines={[2]} highlightWidth={1152}>',
      '  {source}',
      '</Code>',
    )} />
    <p>Animate the highlight by changing the list as the frame advances; each band is cheap, so highlighting a line per beat or per cue is fine.</p>

    <h3>Themes</h3>
    <p><code>codeThemes.dark</code> and <code>codeThemes.light</code> are ready to use. A theme does not draw a panel background: place a <code>Rect</code> behind the code, sized to your design. To customize, spread a built-in theme and override what you need:</p>
    <DocCode label="A custom theme" language="tsx" code={lines(
      'const theme = {',
      '  ...codeThemes.dark,',
      "  highlightLine: '#a68bbf30',",
      "  tokens: { ...codeThemes.dark.tokens, keyword: '#e0b7ff' },",
      '};',
    )} />
    <p>Token names are twinkleplop’s, such as <code>keyword</code>, <code>string</code>, <code>comment</code>, <code>number</code>, <code>function</code>, <code>type</code>, <code>tag_name</code>, <code>attr_name</code>, <code>punctuation</code>, and <code>operator</code>. In JSON, object keys are <code>property</code>; string values keep <code>string</code> and <code>string_escape</code>. A name your theme does not list uses <code>foreground</code>.</p>
    <p>Continue with <a href="/docs/code-typing/">typing effects, carets, and tokens</a>.</p>
  </>,

  'code-typing': <>
    <p>Code looks best when it arrives a character at a time. This chapter reveals <a href="/docs/code/"><code>Code</code></a> as if it were typed, puts a caret where the typing is, and shows the lower-level tools behind both.</p>

    <h3>Typing code in</h3>
    <p>Pass <code>useTypewriter()</code>’s <code>length</code> to <code>visibleCharacters</code>:</p>
    <DocCode label="Type the source out" language="tsx" code={lines(
      "import { useTypewriter } from '@celesta/react';",
      "import { Code } from '@celesta/code';",
      '',
      'function Typed() {',
      '  const { length } = useTypewriter(source, { from: 15, framesPerChar: 0.5 });',
      '  return <Code x={64} y={64} language="ts" style={style} visibleCharacters={length}>{source}</Code>;',
      '}',
    )} />
    <p>The whole source is tokenized once, when the source or language changes, and typing only reveals the result. Colors therefore reflect the <strong>finished</strong> code: a half-typed string is already colored as a string, and nothing flickers as a quote is closed.</p>
    <h3>What visibleCharacters counts</h3>
    <ul>
      <li>Unicode <strong>code points of the original source</strong>, matching <code>useTypewriter().length</code>. A single-code-point emoji counts once; a joined, flag, or skin-tone sequence counts once per code point.</li>
      <li>A tab costs one character and an LF costs one. A <strong>CRLF costs two</strong>.</li>
      <li>Fractions round down, a negative count shows nothing, and the default <code>Infinity</code> shows everything.</li>
    </ul>

    <h3>A caret that follows the typing</h3>
    <p><code>useCodePoint(source, {'{ line, column }'}, style?, tabSize?)</code> measures a position in the source with the same font and tab expansion as <code>Code</code>, so you can place a caret, underline, or callout there. Lines and columns are <strong>one-based</strong>, and columns count original characters (a tab or a single-code-point emoji is one column). The column may be one past the last character, to sit at a line’s end. An invalid position throws.</p>
    <DocCode label="A blinking caret at the end of the typed text" language="tsx" code={lines(
      "import { Group, Rect, useTypewriter } from '@celesta/react';",
      "import { Code, useCodePoint } from '@celesta/code';",
      '',
      'function TypedWithCaret() {',
      '  const { length, caretVisible } = useTypewriter(source, { from: 15, framesPerChar: 0.5 });',
      '  const typed = Array.from(source).slice(0, length).join(\'\').split(/\\r\\n|\\r|\\n/);',
      '  const caret = useCodePoint(source, {',
      '    line: typed.length,',
      '    column: Array.from(typed[typed.length - 1]).length + 1,',
      '  }, style);',
      '',
      '  return (',
      '    <Group x={64} y={64}>',
      '      <Code language="ts" style={style} visibleCharacters={length}>{source}</Code>',
      '      {caretVisible && <Rect x={caret.x} y={caret.y} width={2} height={caret.lineHeight} fill="#a68bbf" />}',
      '    </Group>',
      '  );',
      '}',
    )} />
    <p>Wrapping both in one <code>Group</code> keeps the caret aligned with the code however the group is moved or scaled. The hook returns <code>x</code>, the line’s top <code>y</code>, the <code>baseline</code>, and <code>lineHeight</code>, all relative to <code>Code</code>’s top-left corner, before any group transform.</p>

    <h3>Revealing whole lines</h3>
    <p><code>codeCharacterCount(source, {'{ line, column }'})</code> turns a source position into the number of characters before it, which is exactly what <code>visibleCharacters</code> wants. It is a plain function, so it also works outside React. To show the first three lines of a four-line snippet:</p>
    <DocCode label="Show the first three lines" language="tsx" code={lines(
      "import { Code, codeCharacterCount } from '@celesta/code';",
      '',
      'function FirstThreeLines({ source }: { source: string }) {',
      '  const count = codeCharacterCount(source, { line: 4, column: 1 });',
      '  return <Code language="ts" visibleCharacters={count}>{source}</Code>;',
      '}',
    )} />
    <p>A position at the end of a line leaves out that line’s newline; use column 1 of the next line to include it. Pair this with <a href="/docs/motion-toolkit/"><code>useCue</code></a> to build up a snippet one step at a time, one cue per line.</p>

    <h3>Tokens</h3>
    <p><code>tokenizeCode(source, language?)</code> returns the tokens behind <code>Code</code>: each one has the original <code>text</code>, a <code>type</code> (twinkleplop’s name, or <code>plain</code>), and its <code>start</code> and <code>end</code> offsets in UTF-16 units, end exclusive. Tokens preserve all of the source, whitespace included, and need neither HTML nor a DOM. Use them for your own rendering or to analyze the code:</p>
    <DocCode label="Count the comments" language="tsx" code={lines(
      "import { tokenizeCode } from '@celesta/code';",
      '',
      "const comments = tokenizeCode(source, 'ts').filter((token) => token.type === 'comment');",
    )} />

    <h3>Limits</h3>
    <ul>
      <li>Each color run is its own <code>Text</code> layer, and the package measures prefixes of the source. Long lines with many colors cost more to measure, add layers, and take longer to rasterize. Keep snippets to what fits on screen.</li>
      <li>Shaping does not carry across color boundaries, so ligatures, kerning, combining characters, and right-to-left text are not preserved as one run. Use a monospaced font and left-to-right code.</li>
      <li>Proportional fonts are not a supported layout guarantee.</li>
      <li>Blank runs get no <code>Text</code> layer but keep their measured spacing.</li>
    </ul>
    <p>Sharing styled-text shaping in the core renderer is the planned fix; it is tracked in <a href="https://github.com/mika-f/Celesta/issues/99">issue 99</a>. Browser support is tracked in <a href="https://github.com/mika-f/Celesta/issues/100">issue 100</a>.</p>
  </>,
};
