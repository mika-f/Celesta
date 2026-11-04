import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import { heartPath, piePath, polygonPath, starPath, trianglePath } from '../src/index.js';
import type { PathCommand } from '../src/index.js';
import { commandOf, contentOf, parseLine } from './helpers.mjs';
import type { Command, FrameResponse, OnCurveCommand } from './helpers.mjs';

type Point = [number, number];

// The tests that render() are integration tests of the build: they run
// packages/cli/bin, which loads the built dist/ of this package and the others.
const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

/**
 * Renders frames 0 to `frames - 1` of a composition whose body is `body`, as
 * the CLI's output lines: the config, then one line per frame.
 */
function render(body: string, { frames = 1 } = {}): FrameResponse[] {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-shapes-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Sequence, useCurrentFrame } from '@celesta.studio/react';\n` +
      `import { Heart, Path, Pie, Polygon, Star, Triangle, piePath, starPath } from '@celesta.studio/shapes';\n` +
      `export default function Root() {\n` +
      `  const frame = useCurrentFrame();\n` +
      `  return (\n` +
      `    <Composition width={300} height={200} fps={30} durationInFrames={${frames}}>\n` +
      `      ${body}\n` +
      `    </Composition>\n` +
      `  );\n` +
      `}\n`,
  );
  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: Array.from({ length: frames }, (_, frame) => `{"time":{"value":${frame},"timescale":30}}\n`).join(''),
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    return result.stdout.split(/\r?\n/).filter(Boolean).map(parseLine);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** The on-curve points of a path: each command's end point. */
function onCurve(commands: PathCommand[]): Point[] {
  return commands.filter((c): c is OnCurveCommand => c.type !== 'close').map((c): Point => [c.x, c.y]);
}

/** The point halfway along a cubic that starts at `from`. */
function cubicMidpoint(from: Point, c: Command<'cubicTo'>): Point {
  return [(from[0] + 3 * c.x1 + 3 * c.x2 + c.x) / 8, (from[1] + 3 * c.y1 + 3 * c.y2 + c.y) / 8];
}

function assertNear(actual: readonly number[], expected: readonly number[], message: string, tolerance = 1e-9) {
  assert.equal(actual.length, expected.length, message);
  actual.forEach((value, i) => {
    assert.ok(Math.abs(value - expected[i]) < tolerance, `${message}: [${actual}] != [${expected}]`);
  });
}

test('a polygon has its corners on the circle of its radius, the first at 12 o\'clock, clockwise', () => {
  const commands = polygonPath({ radius: 50, sides: 6 });
  assert.deepEqual(commands.map((c) => c.type), ['moveTo', ...Array(5).fill('lineTo'), 'close']);
  const points = onCurve(commands);
  assertNear(points[0], [50, 0], 'first corner');
  assertNear(points[1], [50 + 50 * Math.sin(Math.PI / 3), 25], 'second corner');
  for (const [x, y] of points) {
    assertNear([Math.hypot(x - 50, y - 50)], [50], 'corner on the circle');
  }
});

test('a star alternates tips on its radius with inner corners at innerRatio of it', () => {
  const points = onCurve(starPath({ radius: 100, points: 5, innerRatio: 0.4 }));
  assert.equal(points.length, 10);
  assertNear(points[0], [100, 0], 'first tip');
  points.forEach(([x, y], i) => {
    assertNear([Math.hypot(x - 100, y - 100)], [i % 2 === 0 ? 100 : 40], `corner ${i}`);
    const degrees = (Math.atan2(x - 100, 100 - y) * 180) / Math.PI;
    assertNear([(degrees + 360) % 360], [36 * i], `angle of corner ${i}`, 1e-6);
  });
  // innerRatio defaults to half the radius.
  assertNear([Math.hypot(...onCurve(starPath({ radius: 100, points: 5 }))[1].map((v) => v - 100))], [50], 'default');
});

test('a triangle fills its box with its top corner at apex along the top edge', () => {
  assert.deepEqual(trianglePath({ width: 120, height: 90 }), [
    { type: 'moveTo', x: 60, y: 0 },
    { type: 'lineTo', x: 120, y: 90 },
    { type: 'lineTo', x: 0, y: 90 },
    { type: 'close' },
  ]);
  assert.deepEqual(onCurve(trianglePath({ width: 120, height: 90, apex: 0 }))[0], [0, 0]);
});

test('a rounded corner is an arc of cornerRadius tangent to both of its sides', () => {
  // A diamond: every corner is a right angle, so each arc is a quarter turn
  // touching its sides 10 px from the corner.
  const commands = polygonPath({ radius: 50, sides: 4, cornerRadius: 10 });
  const start = commandOf(commands[0], 'moveTo');
  const arc = commandOf(commands[1], 'cubicTo');
  const side = Math.SQRT1_2 * 10;
  assertNear([start.x, start.y], [50 - side, side], 'arc starts on the incoming side');
  assertNear([arc.x, arc.y], [50 + side, side], 'arc ends on the outgoing side');
  // The arc's centre is on the bisector, 10 px from both sides.
  const [mx, my] = cubicMidpoint([start.x, start.y], arc);
  assertNear([Math.hypot(mx - 50, my - 10 * Math.SQRT2)], [10], 'arc midpoint on the circle', 1e-4);
  // The tangent at each end runs along the side.
  assertNear([arc.x1 - start.x + (arc.y1 - start.y)], [0], 'start tangent along the side');
  assert.deepEqual(commands.map((c) => c.type), [
    'moveTo', 'cubicTo', 'lineTo', 'cubicTo', 'lineTo', 'cubicTo', 'lineTo', 'cubicTo', 'close',
  ]);
});

test('a corner radius too large for the sides shrinks to reach halfway along the shorter one', () => {
  const start = commandOf(trianglePath({ width: 100, height: 100, cornerRadius: 1e6 })[0], 'moveTo');
  // The top corner's arc starts halfway along the left side.
  assertNear([start.x, start.y], [25, 50], 'start of the top arc');
  // A star's inner corners round as well, and nothing is NaN.
  const star = starPath({ radius: 50, points: 5, innerRatio: 0, cornerRadius: 1e6 });
  assert.ok(star.every((c) => Object.values(c).every((v) => typeof v === 'string' || Number.isFinite(v))));
});

test('a pie runs clockwise from the centre out to startAngle and round to endAngle', () => {
  const commands = piePath({ radius: 50, endAngle: 90 });
  assert.deepEqual(commands.map((c) => c.type), ['moveTo', 'lineTo', 'cubicTo', 'close']);
  const points = onCurve(commands);
  assertNear(points[0], [50, 50], 'centre');
  assertNear(points[1], [50, 0], '12 o\'clock');
  assertNear(points[2], [100, 50], '3 o\'clock');
  const [mx, my] = cubicMidpoint(points[1], commandOf(commands[2], 'cubicTo'));
  assertNear([Math.hypot(mx - 50, my - 50)], [50], 'arc midpoint on the circle', 1e-4);
  // Below startAngle it runs counter-clockwise.
  assertNear(onCurve(piePath({ radius: 50, startAngle: 90, endAngle: 0 }))[2], [50, 0], 'back to 12 o\'clock');
  const toNine = onCurve(piePath({ radius: 50, endAngle: -90 }));
  assertNear(toNine[toNine.length - 1], [0, 50], 'to 9 o\'clock');
});

test('a pie draws nothing at a zero sweep and a whole circle at 360 degrees or more', () => {
  assert.deepEqual(piePath({ radius: 50, startAngle: 30, endAngle: 30 }), []);
  for (const endAngle of [390, 430, -330]) {
    const circle = piePath({ radius: 50, startAngle: 30, endAngle, cornerRadius: 10 });
    assert.equal(circle[0].type, 'moveTo');
    assert.ok(circle.every((c) => c.type !== 'lineTo'), `${endAngle}: no sides`);
    for (const [x, y] of onCurve(circle)) {
      assertNear([Math.hypot(x - 50, y - 50)], [50], `${endAngle}: on the rim`);
    }
  }
});

test('a rounded pie has arcs tangent to its sides and rim', () => {
  const commands = piePath({ radius: 100, endAngle: 90, cornerRadius: 10 });
  const start = commandOf(commands[0], 'moveTo');
  const out = commandOf(commands[1], 'lineTo');
  // The centre corner is a right angle, so its arc touches each side 10 px out.
  assertNear([start.x, start.y], [100, 90], 'after the centre corner');
  // The rim corner touches the side sqrt(R^2 - 2Rr) from the centre.
  assertNear([out.x, out.y], [100, 100 - Math.sqrt(100 * 100 - 2 * 100 * 10)], 'before the rim corner');
  const rim = onCurve(commands).slice(2, -2);
  for (const [x, y] of rim) {
    assert.ok(Math.hypot(x - 100, y - 100) <= 100 + 1e-9, `${x}, ${y} inside the rim`);
  }
});

test('a rounded pie stays finite and inside its circle at every angle of a sweep', () => {
  for (let endAngle = -720; endAngle <= 720; endAngle += 3.75) {
    const commands = piePath({ radius: 40, startAngle: 15, endAngle, cornerRadius: 12 });
    for (const command of commands) {
      for (const [key, value] of Object.entries(command)) {
        if (key === 'type') continue;
        assert.ok(Number.isFinite(value), `${endAngle}: ${key} is ${value}`);
      }
    }
    for (const [x, y] of onCurve(commands)) {
      assert.ok(Math.hypot(x - 40, y - 40) <= 40 + 1e-9, `${endAngle}: ${x}, ${y}`);
    }
  }
});

test('a heart fills its box, symmetric about its middle, from the bottom tip', () => {
  const commands = heartPath({ width: 200, height: 180 });
  assert.deepEqual(commands[0], { type: 'moveTo', x: 100, y: 180 });
  const xs: number[] = [];
  const ys: number[] = [];
  let from: Point = [100, 180];
  for (const command of commands.slice(1, -1)) {
    const c = commandOf(command, 'cubicTo');
    for (let t = 0; t <= 1; t += 1 / 64) {
      const u = 1 - t;
      xs.push(u ** 3 * from[0] + 3 * u * u * t * c.x1 + 3 * u * t * t * c.x2 + t ** 3 * c.x);
      ys.push(u ** 3 * from[1] + 3 * u * u * t * c.y1 + 3 * u * t * t * c.y2 + t ** 3 * c.y);
    }
    from = [c.x, c.y];
  }
  assertNear([Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)], [0, 200, 0, 180], 'bounds', 0.05);
  const points = onCurve(commands);
  for (let i = 0; i < points.length; i += 1) {
    const mirror = points[points.length - 1 - i];
    assertNear([points[i][0] + mirror[0], points[i][1] - mirror[1]], [200, 0], `point ${i} mirrored`);
  }
});

test('generators validate their options and return nothing for an empty size', () => {
  assert.deepEqual(polygonPath({ radius: 0, sides: 5 }), []);
  assert.deepEqual(starPath({ radius: 0, points: 5 }), []);
  assert.deepEqual(trianglePath({ width: 0, height: 10 }), []);
  assert.deepEqual(heartPath({ width: 10, height: 0 }), []);
  assert.deepEqual(piePath({ radius: 0, endAngle: 90 }), []);
  assert.throws(() => polygonPath({ radius: 10, sides: 2 }), /polygonPath\(\) requires a whole `sides` of at least 3/);
  assert.throws(() => starPath({ radius: 10, points: 4.5 }), /starPath\(\) requires a whole `points`/);
  assert.throws(() => starPath({ radius: 10, points: 5, innerRatio: 1.5 }), /`innerRatio` to be from 0 to 1/);
  assert.throws(() => trianglePath({ width: 10, height: 10, apex: -0.1 }), /`apex` to be from 0 to 1/);
  assert.throws(() => trianglePath({ width: -1, height: 10 }), /finite `width` of at least 0/);
  assert.throws(() => heartPath({ width: 10, height: NaN }), /finite `height` of at least 0/);
  assert.throws(() => piePath({ radius: 10, endAngle: Infinity }), /finite `endAngle`/);
  assert.throws(() => piePath({ radius: 10, endAngle: 90, cornerRadius: -1 }), /finite `cornerRadius` of at least 0/);
});

test('each shape is one path layer with its generator\'s commands, fill, and centred stroke', () => {
  const [, frame] = render(
    `<Star x={10} y={20} radius={60} points={5} innerRatio={0.4} cornerRadius={4} opacity={0.5} fill="#FFD84D" stroke="#FFFFFF" join="round" />` +
      `<Triangle width={80} height={60} apex={0.25} cornerRadius={6} stroke="#FF0000" strokeWidth={5} cap="round" />` +
      `<Polygon radius={30} sides={7} fill="#3366CC" />` +
      `<Pie radius={40} startAngle={-45} endAngle={200} cornerRadius={5} fill="#4DD8C0" />` +
      `<Heart width={70} height={60} fill="#EF402B" />`,
  );
  const [star, triangle, polygon, pie, heart] = frame.scene.layers;
  assert.deepEqual(star.transform.position, { x: 10, y: 20 });
  assert.equal(star.opacity, 0.5);
  assert.deepEqual(contentOf(star, 'path'), {
    type: 'path',
    commands: starPath({ radius: 60, points: 5, innerRatio: 0.4, cornerRadius: 4 }),
    fill: { type: 'solid', color: '#FFD84D' },
    stroke: { paint: { type: 'solid', color: '#FFFFFF' }, width: 2 },
    lineJoin: 'round',
  });
  assert.deepEqual(contentOf(triangle, 'path'), {
    type: 'path',
    commands: trianglePath({ width: 80, height: 60, apex: 0.25, cornerRadius: 6 }),
    stroke: { paint: { type: 'solid', color: '#FF0000' }, width: 5 },
    lineCap: 'round',
  });
  assert.deepEqual(contentOf(polygon, 'path').commands, polygonPath({ radius: 30, sides: 7 }));
  assert.deepEqual(contentOf(pie, 'path').commands, piePath({ radius: 40, startAngle: -45, endAngle: 200, cornerRadius: 5 }));
  assert.deepEqual(contentOf(heart, 'path').commands, heartPath({ width: 70, height: 60 }));
});

test('a generator\'s commands passed to a path draw the same shape as the component', () => {
  const [, frame] = render(
    `<Path x={5} y={6} commands={starPath({ radius: 50, points: 6 })} fill="#FFFFFF" />` +
      `<Star x={5} y={6} radius={50} points={6} fill="#FFFFFF" />`,
  );
  const [path, star] = frame.scene.layers;
  assert.deepEqual({ ...path, id: star.id }, star);
});

test('a shape is placed and turned about its anchor, and its gradients move with it', () => {
  const gradient = `{ type: 'linear', start: { x: 0, y: 0 }, end: { x: 100, y: 0 }, stops: [{ offset: 0, color: '#000000' }, { offset: 1, color: '#FFFFFF' }] }`;
  const [, frame] = render(
    `<Polygon x={150} y={100} anchorX={0.5} anchorY={0.25} rotation={30} radius={50} sides={4} fill={${gradient}} />`,
  );
  const [polygon] = frame.scene.layers;
  assert.deepEqual(polygon.transform.position, { x: 150, y: 100 });
  assert.equal(polygon.transform.rotation, 30);
  // A path has no anchor, so the commands move so that the anchor is at the origin.
  const expected = [[0, -25], [50, 25], [0, 75], [-50, 25]];
  const { commands, fill } = contentOf(polygon, 'path');
  onCurve(commands).forEach((point, i) => assertNear(point, expected[i], `corner ${i}`));
  assert.ok(fill?.type === 'linear');
  assert.deepEqual(fill.start, { x: -50, y: -25 });
  assert.deepEqual(fill.end, { x: 50, y: -25 });
});

test('a pie\'s angles animate from nothing, through a slice, to a whole circle', () => {
  const frames = render(`<Pie radius={50} endAngle={frame * 180} fill="#FFFFFF" />`, { frames: 3 }).slice(1);
  assert.deepEqual(frames[0].scene.layers, []);
  assert.deepEqual(contentOf(frames[1].scene.layers[0], 'path').commands, piePath({ radius: 50, endAngle: 180 }));
  assert.deepEqual(contentOf(frames[2].scene.layers[0], 'path').commands, piePath({ radius: 50, endAngle: 360 }));
});

test('shape props are validated', () => {
  const cases: [string, RegExp][] = [
    [`<Star radius={10} points={2} fill="#FFFFFF" />`, /<Star> requires a whole `points` of at least 3/],
    [`<Polygon radius={-1} sides={5} fill="#FFFFFF" />`, /<Polygon> requires a finite `radius` of at least 0/],
    [`<Triangle width={10} height={10} apex={2} fill="#FFFFFF" />`, /<Triangle> requires `apex` to be from 0 to 1/],
    [`<Pie radius={10} endAngle={NaN} fill="#FFFFFF" />`, /<Pie> requires a finite `endAngle`/],
    [`<Heart width={10} height={10} stroke="#FFFFFF" strokeWidth={0} />`, /<Heart> requires a positive `strokeWidth`/],
  ];
  // One CLI process renders every case, each alone in its own frame; frame 0
  // stays empty, since an error while mounting it ends the process.
  const body = cases.map(([shape], i) => `<Sequence from={${i + 1}} durationInFrames={1}>${shape}</Sequence>`).join('');
  const [, empty, ...frames] = render(body, { frames: cases.length + 1 });
  assert.deepEqual(empty.scene.layers, []);
  cases.forEach(([shape, error], i) => assert.match(frames[i].error ?? '', error, shape));
});
