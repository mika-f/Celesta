import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import * as celesta from '@celesta/react';
import { Easings, beatAt, computeSeries, cueAt, frameToTimecode, progress } from '@celesta/react';
import { pointOnPolyline } from '@celesta/shapes';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

/**
 * Renders `frames` of a 300×200, 30 fps composition. `source` is module code
 * that defines `function Body()`, rendered inside the composition.
 */
function render(source, frames = [0], durationInFrames = 60) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-motion-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import * as Core from '@celesta/react';\n` +
      `import * as Layout from '@celesta/layout';\n` +
      `import * as Shapes from '@celesta/shapes';\n` +
      `import * as TextMotion from '@celesta/text';\n` +
      `import * as Transitions from '@celesta/transitions';\n` +
      `const C = { ...Core, ...Layout, ...Shapes, ...TextMotion, ...Transitions };\n` +
      `const { Composition } = C;\n` +
      `${source}\n` +
      `export default function Root() {\n` +
      `  return <Composition width={300} height={200} fps={30} durationInFrames={${durationInFrames}}><Body /></Composition>;\n` +
      `}\n`,
  );
  try {
    const input = frames.map((frame) => JSON.stringify({ time: { value: frame, timescale: 30 } })).join('\n');
    const result = spawnSync(process.execPath, [cli, entry], { input: `${input}\n`, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const [, ...responses] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
    for (const response of responses) {
      assert.equal(response.error, undefined, response.error);
    }
    return responses.map((response) => response.scene.layers);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** Every text layer's string, depth first. */
function texts(layers) {
  return layers.flatMap((layer) =>
    layer.content.type === 'text' ? [layer.content.text] : layer.content.type === 'group' ? texts(layer.content.layers) : [],
  );
}

test('progress clamps, eases, and rejects an empty span', () => {
  assert.equal(progress(0, 10, 20), 0);
  assert.equal(progress(20, 10, 20), 0.5);
  assert.equal(progress(40, 10, 20), 1);
  assert.equal(progress(20, 10, 20, Easings.easeInQuad), 0.25);
  assert.throws(() => progress(0, 0, 0), /positive durationInFrames/);
});

test('every easing starts at exactly 0 and ends at exactly 1', () => {
  for (const [name, easing] of Object.entries(Easings)) {
    // `===` rather than Object.is: -0 at the start is fine.
    assert.ok(easing(0) === 0, `${name}(0) = ${easing(0)}`);
    assert.ok(easing(1) === 1, `${name}(1) = ${easing(1)}`);
  }
  // Before its span, an overshooting curve must not leak a tiny positive value.
  assert.equal(progress(0, 10, 20, Easings.easeOutBack), 0);
  assert.equal(progress(40, 10, 20, Easings.easeInBack), 1);
});

test('entries import random and noise from @celesta/math', () => {
  assert.equal(celesta.random, undefined);
  assert.equal(celesta.noise, undefined);
  const [layers] = render(
    `import { noise, random } from '@celesta/math';\n` +
      `function Body() {\n` +
      `  return <C.Text>{String(random(7)) + ' ' + String(noise(3, 2.5))}</C.Text>;\n` +
      `}`,
  );
  assert.deepEqual(texts(layers), ['0.3443175407592207 -0.1304047736339271']);
});

test('frameToTimecode formats hours, minutes, seconds, and frames', () => {
  assert.equal(frameToTimecode(0, 30), '00:00:00:00');
  assert.equal(frameToTimecode(3 * 3600 * 30 + 62 * 30 + 29, 30), '03:01:02:29');
  assert.equal(frameToTimecode(-5, 24), '00:00:00:00');
  assert.throws(() => frameToTimecode(0, 29.97), /positive integer fps/);
});

test('beatAt maps frames onto beats, bars, and a decaying pulse', () => {
  const onBeat = beatAt(75, 30, { bpm: 120 });
  assert.equal(onBeat.framesPerBeat, 15);
  assert.equal(onBeat.beat, 5);
  assert.equal(onBeat.bar, 1);
  assert.equal(onBeat.beatInBar, 1);
  assert.equal(onBeat.progress, 0);
  assert.equal(onBeat.barProgress, 0.25);
  assert.equal(onBeat.pulse, 1);

  const later = beatAt(80, 30, { bpm: 120, decay: 5 });
  assert.ok(Math.abs(later.pulse - Math.exp(-1)) < 1e-12);
  assert.ok(Math.abs(later.progress - 1 / 3) < 1e-12);

  assert.equal(beatAt(10, 30, { bpm: 120, offset: 25 }).beat, -1);
  assert.equal(beatAt(30, 30, { bpm: 90, beatsPerBar: 3 }).bar, 0);
  assert.throws(() => beatAt(0, 30, { bpm: 0 }), /positive bpm/);
});

test('cueAt finds the cue in effect and its neighbours', () => {
  const cues = [{ at: 10, name: 'a' }, { at: 20, name: 'b' }, { at: 40, name: 'c' }];
  assert.equal(cueAt(cues, 9), null);
  const active = cueAt(cues, 25);
  assert.equal(active.cue.name, 'b');
  assert.equal(active.index, 1);
  assert.equal(active.frame, 5);
  assert.equal(active.previous.name, 'a');
  assert.equal(active.next.name, 'c');
  assert.equal(cueAt(cues, 99).next, undefined);
  assert.throws(() => cueAt([{ at: 5 }, { at: 1 }], 0), /sorted/);
});

test('computeSeries lays items back to back, with overlaps and gaps', () => {
  assert.deepEqual(
    computeSeries([{ durationInFrames: 30 }, { durationInFrames: 20, offset: -10 }, { durationInFrames: 5, offset: 3 }]),
    {
      sequences: [
        { from: 0, durationInFrames: 30 },
        { from: 20, durationInFrames: 20 },
        { from: 43, durationInFrames: 5 },
      ],
      durationInFrames: 48,
    },
  );
  assert.throws(() => computeSeries([{ durationInFrames: 0 }]), /positive integer durationInFrames/);
  assert.throws(() => computeSeries([{ durationInFrames: 5, offset: -6 }]), /before frame 0/);
});

test('Series plays each item in turn with its own local clock', () => {
  const source = `
    function Label({ name }: { name: string }) {
      return <C.Text>{\`\${name}:\${C.useCurrentFrame()}\`}</C.Text>;
    }
    function Body() {
      return (
        <C.Series>
          <C.Series.Sequence durationInFrames={10}><Label name="a" /></C.Series.Sequence>
          {false}
          <C.Series.Sequence durationInFrames={10} offset={-2}><Label name="b" /></C.Series.Sequence>
        </C.Series>
      );
    }`;
  const [first, overlap, last, after] = render(source, [0, 8, 17, 18]);
  assert.deepEqual(texts(first), ['a:0']);
  assert.deepEqual(texts(overlap), ['a:8', 'b:0']);
  assert.deepEqual(texts(last), ['b:9']);
  assert.deepEqual(texts(after), []);
});

test('Series rejects children other than Series.Sequence', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-motion-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Series, Text } from '@celesta/react';\n` +
      `export default function Root() {\n` +
      `  return <Composition width={300} height={200} fps={30} durationInFrames={1}><Series><Text>x</Text></Series></Composition>;\n` +
      `}\n`,
  );
  try {
    const result = spawnSync(process.execPath, [cli, entry], { input: '{"time":{"value":0,"timescale":1}}\n', encoding: 'utf8' });
    assert.match(result.stdout + result.stderr, /<Series> children must be <Series.Sequence> elements/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('Stagger starts each child a fixed number of frames after the previous one', () => {
  const source = `
    function Row({ name }: { name: string }) {
      return <C.Text>{\`\${name}:\${C.useCurrentFrame()}\`}</C.Text>;
    }
    function Body() {
      return (
        <C.Stagger from={2} each={5}>
          {['a', 'b', 'c'].map((name) => <Row key={name} name={name} />)}
        </C.Stagger>
      );
    }`;
  const [before, second, third] = render(source, [1, 7, 12]);
  assert.deepEqual(texts(before), []);
  assert.deepEqual(texts(second), ['a:5', 'b:0']);
  assert.deepEqual(texts(third), ['a:10', 'b:5', 'c:0']);
});

test('Line draws a two-point path, with optional round caps', () => {
  const source = `
    function Body() {
      return (
        <>
          <C.Line x1={10} y1={20} x2={10} y2={60} strokeWidth={4} stroke="#FF0000" cap="butt" />
          <C.Line x1={0} y1={0} x2={30} y2={40} strokeWidth={10} />
          <C.Line x1={5} y1={5} x2={5} y2={5} cap="butt" />
        </>
      );
    }`;
  const [[butt, round, ...rest]] = render(source);
  assert.equal(rest.length, 0, 'a zero-length butt line draws nothing');
  assert.deepEqual(butt.content.commands, [
    { type: 'moveTo', x: 10, y: 20 },
    { type: 'lineTo', x: 10, y: 60 },
  ]);
  assert.deepEqual(butt.content.stroke, { paint: { type: 'solid', color: '#FF0000' }, width: 4 });
  assert.equal(butt.content.lineCap, undefined);
  assert.equal(round.content.lineCap, 'round');
  assert.equal(round.content.stroke.width, 10);
});

test('Polyline draws the first part of its length and pointOnPolyline finds the tip', () => {
  const points = [[0, 0], [100, 0], [100, 100]];
  assert.deepEqual(pointOnPolyline(points, 0), [0, 0]);
  assert.deepEqual(pointOnPolyline(points, 0.75), [100, 50]);
  assert.deepEqual(pointOnPolyline(points, 2), [100, 100]);

  const source = `
    function Body() {
      return <C.Polyline points={[[0, 0], [100, 0], [100, 100]]} progress={0.75} cap="butt" opacity={0.5} />;
    }`;
  const [[path]] = render(source);
  assert.equal(path.opacity, 0.5);
  assert.deepEqual(path.content.commands, [
    { type: 'moveTo', x: 0, y: 0 },
    { type: 'lineTo', x: 100, y: 0 },
    { type: 'lineTo', x: 100, y: 50 },
  ]);
});

test('Camera puts its focus point at the center and zooms about it', () => {
  const source = `
    function Body() {
      return <C.Camera x={1000} y={500} zoom={2} rotation={10}><C.Rect width={1} height={1} /></C.Camera>;
    }`;
  const [[outer]] = render(source);
  assert.deepEqual(outer.transform.position, { x: 150, y: 100 });
  assert.deepEqual(outer.transform.scale, { x: 2, y: 2 });
  assert.equal(outer.transform.rotation, 10);
  const [inner] = outer.content.layers;
  assert.deepEqual(inner.transform.position, { x: -1000, y: -500 });

  const shaky = `
    function Body() {
      return <C.Camera shake={8}><C.Rect width={1} height={1} /></C.Camera>;
    }`;
  const [[a], [b]] = render(shaky, [3, 3]);
  assert.deepEqual(a, b, 'shake is deterministic');
  const { x, y } = a.content.layers[0].transform.position;
  assert.ok(Math.abs(x + 150) <= 8 && Math.abs(y + 100) <= 8, `${x}, ${y}`);
});

test('TextReveal masks each line and slides it up into place, staggered', () => {
  const source = `
    function Body() {
      return (
        <C.TextReveal x={10} y={20} lineHeight={50} from={0} stagger={10} durationInFrames={10}
          easing={(t) => t} style={{ fontSize: 40, lineHeight: 99 }}>
          {'ONE\\nTWO'}
        </C.TextReveal>
      );
    }`;
  const [[start], [half], [done]] = render(source, [0, 5, 30]);
  assert.deepEqual(start.transform.position, { x: 10, y: 20 });
  const [one, two] = start.content.layers;
  assert.equal(one.content.clip.height, 50);
  assert.equal(two.transform.position.y, 50);
  const text = (line) => line.content.layers[0];
  assert.equal(text(one).content.text, 'ONE');
  assert.equal(text(one).transform.position.y, 90);
  assert.equal(text(one).content.style.lineHeight, undefined);
  assert.equal(text(half.content.layers[0]).transform.position.y, 65);
  assert.equal(text(half.content.layers[1]).transform.position.y, 90);
  assert.equal(text(done.content.layers[1]).transform.position.y, 40);
});

test('useTypewriter types by code point and blinks the caret while idle', () => {
  const source = `
    function Body() {
      const t = C.useTypewriter('あい🙂x', { from: 2, framesPerChar: 2, blinkFrames: 4 });
      return <C.Text>{\`\${t.text}|\${t.length}|\${t.done}|\${t.caretVisible}\`}</C.Text>;
    }`;
  const frames = render(source, [0, 1, 2, 5, 8, 10, 12]).map(texts).map(([t]) => t);
  assert.deepEqual(frames, [
    '|0|false|false',
    '|0|false|false',
    'あ|1|false|true',
    'あい|2|false|true',
    'あい🙂x|4|true|false',
    'あい🙂x|4|true|true',
    'あい🙂x|4|true|false',
  ]);
});

test('useCountUp counts from a value to the target and rounds', () => {
  const source = `
    function Body() {
      const n = C.useCountUp(10, { from: 2, delay: 5, durationInFrames: 10, easing: (t) => t, decimals: 1 });
      return <C.Text>{n}</C.Text>;
    }`;
  const frames = render(source, [0, 8, 20]).map(texts).map(([t]) => t);
  assert.deepEqual(frames, ['2', '4.4', '10']);
});

test('Transition combines several effects', () => {
  const source = `
    function Body() {
      return (
        <C.Transition type={['fade', 'slide', 'scale']} durationInFrames={5} slideFrom="bottom" distance={20} scaleFrom={0.5}>
          <C.Text>x</C.Text>
        </C.Transition>
      );
    }`;
  const [[group]] = render(source, [2], 10);
  assert.equal(group.opacity, 0.5);
  assert.equal(group.transform.position.y, 10);
  assert.equal(group.transform.scale.x, 0.75);
});
