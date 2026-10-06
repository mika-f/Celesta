import { docPath } from './docs-nav';
import { t, text, homePath } from './i18n';
import type { ReactNode } from 'react';
import { DocCode } from './DocCode';
import { Api, Note } from './docs-shared';
import voicevoxScene from './examples/voicevox.tsx?raw';

// Chapters for the packages that ship beside @celesta/react.

/** Joins example lines, so code reads naturally without escaping newlines. */
const lines = (...rows: string[]) => rows.join('\n');

export const packageContents: Record<string, ReactNode> = {
  'voicevox': <>
    <p>{text('package.voicevox-intro', [<code>@celesta/voicevox</code>, <code>@celesta/react</code>])}</p>
    <p>{text('package.voicevox-runtime')}</p>
    <h3>{text('package.voicevox-example')}</h3>
    <p>{text('package.voicevox-query')}</p>
    <DocCode label="voicevox.tsx" language="tsx" code={voicevoxScene.trim()} />
    <Api caption="@celesta/voicevox" rows={[
      [<code>lipSyncFromVoicevox(query, options?)</code>, text('api.voicevox-convert')],
      [<code>voicevoxVowelShape(vowel)</code>, text('api.voicevox-vowel')],
    ]} />
    <Api caption={t('package.voicevox-options')} rows={[
      [<code>frameRate</code>, text('api.voicevox-frame-rate')],
      [<code>interrogativeUpspeak</code>, text('api.voicevox-upspeak')],
    ]} />
    <p>{text('package.voicevox-types')}</p>
  </>,
  'math': <>
    <p>{text('package.is-a-small-dependency-free-toolkit-for', [<code>@celesta/math</code>])}</p>
    <DocCode label={t('package.import')} language="tsx" code={"import { random, randomRange, noise } from '@celesta/math';"} />
    <p>{text('package.choose-file-set-up-typescript-for-editor', [<strong />, <a href={docPath('react-compositions')} />, <a href={`${homePath}#playground`} />, <code>@celesta/react</code>, <code>@celesta/math</code>, <code>react</code>])}</p>
    <p>{text('package.this-chapter-covers-randomness-noise-fbm-covers', [<a href={docPath('math-noise')} />, <a href={docPath('math-shaping')} />])}</p>

    <h3>{text('package.why-not-math-random')}</h3>
    <p>{text('package.a-composition-is-a-function-of-the', [<code>Math.random()</code>, <code>@celesta/math</code>])}</p>

    <h3>{text('package.seeds')}</h3>
    <p>{text('package.a-is-a-number-or-a-string', [<code>Seed</code>, <code>random(seed)</code>, <code>[0, 1)</code>])}</p>
    <DocCode label={t('package.one-seed-per-property')} language="tsx" code={lines(
      'const x = randomRange(`star-${i}-x`, 0, 1920);',
      'const y = randomRange(`star-${i}-y`, 0, 1080);',
      'const size = randomRange(`star-${i}-size`, 2, 6);',
    )} />
    <ul>
      <li>{text('package.reusing-a-seed-reuses-the-value-called', [<code>random('a')</code>, <code>x</code>, <code>y</code>])}</li>
      <li>{text('package.a-fractional-number-is-its-own-seed', [<code>0.5</code>, <code>0</code>, <code>1</code>, <code>0.5</code>, <code>'0.5'</code>])}</li>
      <li>{text('package.a-seed-must-be-a-string-or', [<code>NaN</code>, <code>Infinity</code>])}</li>
      <li>{text('package.to-make-a-value-change-over-time', [<code>random(Math.floor(frame / 4))</code>, <a href={docPath('math-noise')} />, <code>noise</code>])}</li>
    </ul>

    <h3>{text('package.the-random-functions')}</h3>
    <Api caption={t('package.randomness')} rows={[
      [<code>random(seed)</code>, <>{text('api.a-number-in', [<code>[0, 1)</code>])}</>],
      [<code>randomRange(seed, min, max)</code>, <>{text('api.a-number-in-a045', [<code>[min, max)</code>])}</>],
      [<code>randomInt(seed, min, max)</code>, <>{text('api.a-whole-number-from-to-throws-if', [<code>min</code>, <code>max</code>, <strong />, text('package.both-included')])}</>],
      [<code>randomBool(seed, probability?)</code>, <>{text('api.with-the-given-probability-by-default', [<code>true</code>, <code>0.5</code>])}</>],
      [<code>randomSign(seed)</code>, <>{text('api.or-each-half-the-time-handy-for', [<code>-1</code>, <code>1</code>])}</>],
      [<code>randomPick(seed, items)</code>, <>{text('api.one-element-of-throws-on-an-empty', [<code>items</code>])}</>],
      [<code>shuffle(seed, items)</code>, <>{text('api.a-shuffled-of-the-original-is-untouched', [<strong />, text('package.copy'), <code>items</code>])}</>],
      [<code>randomGaussian(seed, mean?, stdDev?)</code>, <>{text('api.a-bell-curve-value-most-land-within', [<code>stdDev</code>, <code>mean</code>, <code>0</code>, <code>1</code>])}</>],
      [<code>randomInCircle(seed, radius?, center?)</code>, <>{text('api.a-point-spread-evenly-over-a-disc', [<code>{'{ x, y }'}</code>, <code>1</code>])}</>],
    ]} />

    <h3>{text('package.a-seeded-starfield')}</h3>
    <p>{text('package.because-the-seeds-are-fixed-the-field', [<a href={docPath('math-noise')} />, <code>noise</code>])}</p>
    <DocCode label={t('package.a-starfield-that-drifts')} language="tsx" code={lines(
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

    <h3>{text('package.choosing-and-ordering-things')}</h3>
    <p>{text('package.and-pick-from-lists-you-already-have', [<code>randomPick</code>, <code>shuffle</code>])}</p>
    <DocCode label={t('package.a-palette-and-a-shuffled-order')} language="tsx" code={lines(
      "const palette = ['#a68bbf', '#7cf29c', '#f2c97c', '#7cc4f2'];",
      '',
      'const color = randomPick(`bar-${i}-color`, palette);',
      "const order = shuffle('slides', ['intro', 'demo', 'pricing', 'outro']);",
      'const mirrored = randomBool(`bar-${i}-flip`, 0.3); // flipped about 30% of the time',
      'const spin = 45 * randomSign(`bar-${i}-spin`);     // clockwise or counter-clockwise',
    )} />

    <h3>{text('package.natural-looking-scatter')}</h3>
    <p>{text('package.uniform-randomness-looks-artificial-every-value-is', [<code>randomGaussian</code>, <code>randomInCircle</code>])}</p>
    <DocCode label={t('package.a-burst-of-particles')} language="tsx" code={lines(
      "import { randomGaussian, randomInCircle } from '@celesta/math';",
      '',
      'const particles = Array.from({ length: 120 }, (_, i) => {',
      '  const p = randomInCircle(`spark-${i}`, 260, { x: 960, y: 540 });',
      '  const size = Math.max(2, randomGaussian(`spark-${i}-size`, 8, 3));',
      '  return <Rect key={i} x={p.x} y={p.y} anchorX={0.5} anchorY={0.5}',
      '    width={size} height={size} cornerRadius={size / 2} fill="#f2c97c" />;',
      '});',
    )} />
    <Note title={t('package.keep-the-seed-change-the-look')}>{text('package.when-a-random-layout-is-almost-right', [<code>star-</code>, <code>sky-</code>])}</Note>
  </>,

  'math-noise': <>
    <p>{text('package.noise-is-smooth-randomness-nearby-inputs-give', [<a href={docPath('math')} />, <code>@celesta/math</code>])}</p>

    <h3>{text('package.1d-noise-over-time')}</h3>
    <p>{text('package.returns-a-value-in-it-picks-a', [<code>noise(seed, t)</code>, <code>[-1, 1]</code>, <code>t</code>, <code>t</code>, <code>frame / 20</code>])}</p>
    <DocCode label={t('package.a-gentle-floating-motion')} language="tsx" code={lines(
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
      <li>{text('package.use-a-different-seed-for-each-property', [<strong />, <code>x</code>, <code>y</code>])}</li>
      <li>{text('package.larger-divisors-are-slower-and-calmer-drifts', [<code>frame / 90</code>, <code>frame / 8</code>])}</li>
      <li>{text('package.scale-by-the-amount-you-want-moves', [<code>noise(…) * 24</code>])}</li>
      <li>{text('package.add-a-per-item-offset-for-a', [<code>noise(`dot-${'{i}'}`, frame / 40)</code>])}</li>
    </ul>
    <Note title={t('package.camera-shake')}>{text('package.the-prop-of-is-built-for-handheld', [<code>shake</code>, <a href={docPath('text-camera-lines')} />, <code>Camera</code>, <code>noise</code>])}</Note>

    <h3>{text('package.2d-and-3d-noise')}</h3>
    <p>{text('package.samples-a-smooth-field-over-a-plane', [<code>noise2D(seed, x, y)</code>, <code>x / 200</code>, <code>noise3D(seed, x, y, z)</code>])}</p>
    <DocCode label={t('package.a-field-of-dots-that-breathe')} language="tsx" code={lines(
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
    <p>{text('package.every-result-lies-in-to-turn-it', [<code>[-1, 1]</code>, <a href={docPath('math-shaping')} />, <code>remap</code>, <code>0.5 + n * 0.5</code>, <code>[0, 1]</code>])}</p>

    <h3>{text('package.layered-noise-fbm')}</h3>
    <p>{text('package.one-layer-of-noise-moves-on-a', [<code>fbm</code>, <code>fbm</code>, <code>fbm2D</code>, <code>fbm3D</code>, <code>[-1, 1]</code>])}</p>
    <Api caption={t('package.fbm-options')} rows={[
      [<code>octaves</code>, <>{text('api.how-many-layers-to-add-a-whole', [<code>4</code>])}</>],
      [<code>lacunarity</code>, <>{text('api.how-much-finer-each-layer-is-than', [<code>2</code>])}</>],
      [<code>gain</code>, <>{text('api.how-much-weaker-each-layer-is-than', [<code>0.5</code>])}</>],
    ]} />
    <DocCode label={t('package.a-rough-horizon-line')} language="tsx" code={lines(
      "import { fbm } from '@celesta/math';",
      '',
      'const points: [number, number][] = Array.from({ length: 97 }, (_, i) => [',
      '  i * 20,',
      "  700 + fbm('ridge', i / 14, { octaves: 5, gain: 0.55 }) * 120,",
      ']);',
      '',
      '<Polyline points={points} stroke="#a68bbf" strokeWidth={3} />',
    )} />
    <p>{text('package.invalid-options-throw-an-error-that-names', [<code>fbm2D() requires a whole number of octaves of at least 1</code>])}</p>

    <h3>{text('package.which-one-do-i-use')}</h3>
    <div className="doc-table-wrap" tabIndex={0} aria-label={t('package.choosing-a-noise-function')}><table><caption>{text('package.choosing-a-noise-function-m508')}</caption><thead><tr><th>{text('package.you-want')}</th><th>{text('package.use')}</th></tr></thead><tbody>
      <tr><td>{text('package.one-value-that-drifts-over-time')}</td><td><code>noise(seed, frame / n)</code></td></tr>
      <tr><td>{text('package.a-value-that-wanders-on-several-scales')}</td><td><code>fbm(seed, frame / n)</code></td></tr>
      <tr><td>{text('package.a-pattern-across-the-screen')}</td><td>{text('package.or-sampled-at-scaled-positions', [<code>noise2D</code>, <code>fbm2D</code>])}</td></tr>
      <tr><td>{text('package.a-pattern-across-the-screen-that-moves')}</td><td>{text('package.or-with-time-as-the-third-coordinate', [<code>noise3D</code>, <code>fbm3D</code>])}</td></tr>
      <tr><td>{text('package.a-new-unrelated-value-each-frame-or')}</td><td>{text('package.with-the-frame-in-the-seed', [<a href={docPath('math')} />, <code>random</code>])}</td></tr>
    </tbody></table></div>
  </>,

  'math-shaping': <>
    <p>{text('package.the-remaining-helpers-in-reshape-numbers-build', [<a href={docPath('math')} />, <code>@celesta/math</code>, <code>prepare()</code>])}</p>

    <h3>{text('package.shaping-numbers')}</h3>
    <Api caption={t('package.scalars')} rows={[
      [<code>clamp(value, min, max)</code>, <>{text('api.limits-to-limits-it-to', [<code>value</code>, <code>[min, max]</code>, <code>clamp01(value)</code>, <code>[0, 1]</code>])}</>],
      [<code>lerp(a, b, t)</code>, <>{text('api.the-value-of-the-way-from-to', [<code>t</code>, <code>a</code>, <code>b</code>, <code>t</code>])}</>],
      [<code>inverseLerp(a, b, value)</code>, <>{text('api.the-opposite-how-far-is-from-to', [<code>value</code>, <code>a</code>, <code>b</code>, <code>t</code>, <code>0</code>, <code>a === b</code>])}</>],
      [<code>remap(value, inMin, inMax, outMin, outMax)</code>, <>{text('api.moves-a-value-from-one-range-to', [<code>remapClamped</code>])}</>],
      [<code>step(edge, x)</code>, <>{text('api.below-from-on', [<code>0</code>, <code>edge</code>, <code>1</code>, <code>edge</code>])}</>],
      [<code>smoothstep(edge0, edge1, x)</code>, <>{text('api.a-smooth-0-1-ramp-as-goes', [<code>x</code>, <code>edge0</code>, <code>edge1</code>, <code>smootherstep</code>])}</>],
      [<code>fract(x)</code>, <>{text('api.the-fractional-part-always-in-also-for', [<code>[0, 1)</code>])}</>],
      [<code>mod(value, divisor)</code>, <>{text('api.modulo-with-the-sign-of-the-divisor', [<code>mod(-1, 4)</code>, <code>3</code>, <code>-1</code>])}</>],
      [<code>wrap(value, min, max)</code>, <>{text('api.wraps-into-for-looping-positions-and-hues', [<code>[min, max)</code>])}</>],
      [<code>pingPong(value, length)</code>, <>{text('api.bounces-a-steadily-increasing-value-back-and', [<code>0</code>, <code>length</code>])}</>],
      [<code>snap(value, increment)</code>, <>{text('api.rounds-to-the-nearest-multiple-of-for', [<code>increment</code>])}</>],
      [<code>roundTo(value, decimals?)</code>, <>{text('api.rounds-to-a-number-of-decimals-for')}</>],
      [<code>approxEqual(a, b, epsilon?)</code>, <>{text('api.whether-two-numbers-differ-by-at-most', [<code>epsilon</code>, <code>1e-6</code>])}</>],
    ]} />
    <Note title={t('package.remap-or-interpolate')}>{text('package.is-a-plain-linear-conversion-with-no', [<code>remap</code>, <code>remapClamped</code>, <code>extrapolateLeft</code>, <code>extrapolateRight</code>, <a href={docPath('animation')} />, <code>interpolate</code>, <code>@celesta/react</code>, <code>remap</code>])}</Note>
    <DocCode label={t('package.loops-and-bounces')} language="tsx" code={lines(
      "import { pingPong, snap, wrap } from '@celesta/math';",
      '',
      'const frame = useCurrentFrame();',
      'const ticker = wrap(frame * 4, -200, 1920);   // scrolls right, re-enters at the left',
      'const sweep = pingPong(frame * 8, 1600);      // 0 → 1600 → 0, repeating',
      'const stepped = snap(sweep, 100);             // the same sweep, in 100 px jumps',
    )} />

    <h3>{text('package.waves')}</h3>
    <p>{text('package.the-four-wave-functions-repeat-with-a', [<strong />, <code>[-1, 1]</code>, <code>frame / framesPerCycle</code>])}</p>
    <Api caption={t('package.waves-m527')} rows={[
      [<code>sineWave(t)</code>, <>{text('api.a-smooth-sine-gentle-pulses-bobbing-breathing')}</>],
      [<code>triangleWave(t)</code>, <>{text('api.straight-ramps-up-and-down-constant-speed')}</>],
      [<code>squareWave(t)</code>, <>{text('api.for-the-first-half-of-each-cycle', [<code>1</code>, <code>-1</code>])}</>],
      [<code>sawtoothWave(t)</code>, <>{text('api.ramps-from-0-up-to-1-drops')}</>],
    ]} />
    <DocCode label={t('package.a-breathing-circle-on-a-2-second')} language="tsx" code={lines(
      "import { sineWave } from '@celesta/math';",
      '',
      'const { fps } = useVideoConfig();',
      'const frame = useCurrentFrame();',
      'const breath = sineWave(frame / (fps * 2)); // one full cycle every two seconds',
      '',
      '<Rect x={960} y={540} anchorX={0.5} anchorY={0.5} width={200} height={200}',
      '  cornerRadius={100} scale={1 + 0.08 * breath} fill="#a68bbf" />',
    )} />

    <h3>{text('package.angles')}</h3>
    <p>{text('package.functions-that-take-or-return-angles-use', [<strong />, <code>rotation</code>, <strong />, <code>radToDeg</code>])}</p>
    <Api caption={t('package.angles-m531')} rows={[
      [<code>TAU</code>, <>{text('api.a-full-turn-in-radians-2')}</>],
      [<><code>degToRad(degrees)</code>, <code>radToDeg(radians)</code></>, <>{text('api.convert-between-degrees-and-radians')}</>],
      [<code>normalizeAngle(angle)</code>, <>{text('api.wraps-an-angle-into', [<code>[-π, π)</code>])}</>],
      [<code>angleDifference(from, to)</code>, <>{text('api.the-shortest-signed-turn-from-one-angle')}</>],
      [<code>lerpAngle(a, b, t)</code>, <>{text('api.interpolates-between-angles-the-short-way-round')}</>],
    ]} />

    <h3>{text('package.points-and-geometry')}</h3>
    <p>{text('package.points-are-plain-objects-typed-as-celesta', [<code>{'{ x, y }'}</code>, <code>Vec2</code>, <strong />, <code>rotation</code>])}</p>
    <Api caption={t('package.2d-points')} rows={[
      [<code>distance(a, b)</code>, <>{text('api.the-straight-line-distance-between-two-points')}</>],
      [<code>angleBetween(from, to)</code>, <>{text('api.the-direction-from-one-point-to-another', [<code>0</code>])}</>],
      [<><code>lerpPoint(a, b, t)</code>, <code>midpoint(a, b)</code></>, <>{text('api.a-point-along-a-segment-and-the')}</>],
      [<code>rotatePoint(point, angle, origin?)</code>, <>{text('api.turns-a-point-about-an-origin-the')}</>],
      [<><code>polarToCartesian(angle, radius, center?)</code>, <code>cartesianToPolar(point, center?)</code></>, <>{text('api.convert-between-a-direction-and-distance-and', [<code>{'{ x, y }'}</code>, <code>{'{ angle, radius }'}</code>])}</>],
      [<><code>quadraticBezierPoint(p0, p1, p2, t)</code>, <code>cubicBezierPoint(p0, p1, p2, p3, t)</code></>, <>{text('api.the-point-at-0-1-along-a', [<code>t</code>, <code>quadTo</code>, <code>cubicTo</code>])}</>],
    ]} />
    <DocCode label={t('package.dots-on-a-rotating-ring')} language="tsx" code={lines(
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
    <DocCode label={t('package.a-marker-riding-a-curve')} language="tsx" code={lines(
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
    <p>{text('package.both-and-use-the-same-control-points', [<code>quadraticBezierPoint</code>, <code>cubicBezierPoint</code>, <code>Path</code>, <code>quadTo</code>, <code>cubicTo</code>, <a href={docPath('text-camera-lines')} />, <code>Path</code>])}</p>
  </>,

  'code': <>
    <p>{text('package.draws-syntax-highlighted-source-code-in-a', [<code>@celesta/code</code>, <a href="https://twinkleplop.pngwn.at" />, <code>Text</code>, <code>Rect</code>, <code>@celesta/react</code>])}</p>
    <Note title={t('package.availability')}>{text('package.the-desktop-app-and-cli-include-and', [<code>@celesta/code</code>, <strong />, <code>@celesta/react</code>, <code>@celesta/math</code>, <a href={`${homePath}#playground`} />])}</Note>

    <h3>{text('package.your-first-code-block')}</h3>
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
    <p>{text('package.takes-the-source-as-its-only-child', [<code>Code</code>])}</p>

    <h3>{text('package.fonts')}</h3>
    <p>{text('package.use-a-monospaced-font-defaults-to-jetbrains', [<strong />, <code>Code</code>, <code>{'<Font src="…" />'}</code>])}</p>

    <h3>{text('package.props')}</h3>
    <Api caption={t('package.code-props')} rows={[
      [<code>children</code>, <>{text('api.the-source-as-a-string')}</>],
      [<code>language</code>, <>{text('api.or-the-default-which-applies-no-coloring', [<code>tsx</code>, <code>ts</code>, <code>json</code>, <code>bash</code>, <code>text</code>])}</>],
      [<code>style</code>, <>{text('api.a-celesta-and-so-on-text-is', [<code>TextStyle</code>, <code>fontFamily</code>, <code>fontSize</code>, <code>lineHeight</code>, <code>align</code>])}</>],
      [<code>theme</code>, <>{text('api.token-colors-a-foreground-fallback-and-the', [<code>codeThemes.dark</code>])}</>],
      [<code>tabSize</code>, <>{text('api.width-of-a-tab-stop-counted-in', [<code>2</code>])}</>],
      [<code>highlightLines</code>, <>{text('api.one-based-line-numbers-to-mark-with')}</>],
      [<code>highlightWidth</code>, <>{text('api.band-width-in-pixels-defaults-to-the')}</>],
      [<code>visibleCharacters</code>, <>{text('api.how-much-of-the-source-to-show', [<a href={docPath('code-typing')} />, text('package.code-typing-carets-tokens')])}</>],
      [<><code>x</code>, <code>y</code>, <code>scale</code>, <code>rotation</code>, <code>anchorX</code>/<code>anchorY</code>, <code>opacity</code>, <code>blendMode</code>, <code>blur</code>, <code>shadow</code>, <code>glow</code></>, <>{text('api.the-usual-layer-props-apply-to-the')}</>],
    ]} />

    <h3>{text('package.line-highlights')}</h3>
    <p>{text('package.draws-a-band-behind-the-lines-you', [<code>highlightLines</code>])}</p>
    <DocCode label={t('package.mark-one-line')} language="tsx" code={lines(
      '<Code x={64} y={64} language="ts" style={style} highlightLines={[2]} highlightWidth={1152}>',
      '  {source}',
      '</Code>',
    )} />
    <p>{text('package.animate-the-highlight-by-changing-the-list')}</p>

    <h3>{text('package.themes')}</h3>
    <p>{text('package.and-are-ready-to-use-a-theme', [<code>codeThemes.dark</code>, <code>codeThemes.light</code>, <code>Rect</code>])}</p>
    <DocCode label={t('package.a-custom-theme')} language="tsx" code={lines(
      'const theme = {',
      '  ...codeThemes.dark,',
      "  highlightLine: '#a68bbf30',",
      "  tokens: { ...codeThemes.dark.tokens, keyword: '#e0b7ff' },",
      '};',
    )} />
    <p>{text('package.token-names-are-twinkleplop-s-such-as', [<code>keyword</code>, <code>string</code>, <code>comment</code>, <code>number</code>, <code>function</code>, <code>type</code>, <code>tag_name</code>, <code>attr_name</code>, <code>punctuation</code>, <code>operator</code>, <code>property</code>, <code>string</code>, <code>string_escape</code>, <code>foreground</code>])}</p>
    <p>{text('package.continue-with-typing-effects-carets-and-tokens', [<a href={docPath('code-typing')} />])}</p>
  </>,

  'code-typing': <>
    <p>{text('package.code-looks-best-when-it-arrives-a', [<a href={docPath('code')} />, <code>Code</code>])}</p>

    <h3>{text('package.typing-code-in')}</h3>
    <p>{text('package.pass-s-to', [<code>useTypewriter()</code>, <code>length</code>, <code>visibleCharacters</code>])}</p>
    <DocCode label={t('package.type-the-source-out')} language="tsx" code={lines(
      "import { useTypewriter } from '@celesta/react';",
      "import { Code } from '@celesta/code';",
      '',
      'function Typed() {',
      '  const { length } = useTypewriter(source, { from: 15, framesPerChar: 0.5 });',
      '  return <Code x={64} y={64} language="ts" style={style} visibleCharacters={length}>{source}</Code>;',
      '}',
    )} />
    <p>{text('package.the-whole-source-is-tokenized-once-when', [<strong />])}</p>
    <h3>{text('package.what-visiblecharacters-counts')}</h3>
    <ul>
      <li>{text('package.unicode-code-points-of-the-original-source', [<strong />, <code>useTypewriter().length</code>])}</li>
      <li>{text('package.a-tab-costs-one-character-and-an', [<strong />])}</li>
      <li>{text('package.fractions-round-down-a-negative-count-shows', [<code>Infinity</code>])}</li>
    </ul>

    <h3>{text('package.a-caret-that-follows-the-typing')}</h3>
    <p>{text('package.measures-a-position-in-the-source-with', [<code>useCodePoint(source, {'{ line, column }'}, style?, tabSize?)</code>, <code>Code</code>, <strong />])}</p>
    <DocCode label={t('package.a-blinking-caret-at-the-end-of')} language="tsx" code={lines(
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
    <p>{text('package.wrapping-both-in-one-keeps-the-caret', [<code>Group</code>, <code>x</code>, <code>y</code>, <code>baseline</code>, <code>lineHeight</code>, <code>Code</code>])}</p>

    <h3>{text('package.revealing-whole-lines')}</h3>
    <p>{text('package.turns-a-source-position-into-the-number', [<code>codeCharacterCount(source, {'{ line, column }'})</code>, <code>visibleCharacters</code>])}</p>
    <DocCode label={t('package.show-the-first-three-lines')} language="tsx" code={lines(
      "import { Code, codeCharacterCount } from '@celesta/code';",
      '',
      'function FirstThreeLines({ source }: { source: string }) {',
      '  const count = codeCharacterCount(source, { line: 4, column: 1 });',
      '  return <Code language="ts" visibleCharacters={count}>{source}</Code>;',
      '}',
    )} />
    <p>{text('package.a-position-at-the-end-of-a', [<a href={docPath('motion-toolkit')} />, <code>useCue</code>])}</p>

    <h3>{text('package.tokens')}</h3>
    <p>{text('package.returns-the-tokens-behind-each-one-has', [<code>tokenizeCode(source, language?)</code>, <code>Code</code>, <code>text</code>, <code>type</code>, <code>plain</code>, <code>start</code>, <code>end</code>])}</p>
    <DocCode label={t('package.count-the-comments')} language="tsx" code={lines(
      "import { tokenizeCode } from '@celesta/code';",
      '',
      "const comments = tokenizeCode(source, 'ts').filter((token) => token.type === 'comment');",
    )} />

    <h3>{text('package.limits')}</h3>
    <ul>
      <li>{text('package.each-color-run-is-its-own-layer', [<code>Text</code>])}</li>
      <li>{text('package.shaping-does-not-carry-across-color-boundaries')}</li>
      <li>{text('package.proportional-fonts-are-not-a-supported-layout')}</li>
      <li>{text('package.blank-runs-get-no-layer-but-keep', [<code>Text</code>])}</li>
    </ul>
    <p>{text('package.sharing-styled-text-shaping-in-the-core', [<a href="https://github.com/mika-f/Celesta/issues/99" />, <a href="https://github.com/mika-f/Celesta/issues/100" />])}</p>
  </>,
};
