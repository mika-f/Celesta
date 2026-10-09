import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import * as browser from '../dist/browser.js';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

function evaluate(body, frames = [0], { fps = 30, duration = 60 } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-authoring-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `import * as C from '@celesta/react';
    export default function Root() {
      return <C.Composition width={320} height={240} fps={${fps}} durationInFrames={${duration}}>
        ${body}
      </C.Composition>;
    }`);
  try {
    const input = frames.map((frame) => JSON.stringify({ time: { value: frame, timescale: fps } })).join('\n');
    const result = spawnSync(process.execPath, [cli, entry], { input: `${input}\n`, encoding: 'utf8' });
    const messages = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
    return { result, responses: messages.slice(1) };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function render(body, frames, config) {
  const { result, responses } = evaluate(body, frames, config);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  for (const response of responses) assert.equal(response.error, undefined, response.error);
  return responses.map((response) => response.scene.layers);
}

test('Title shortcuts evaluate identically to explicit Text props', () => {
  const [[short, explicit]] = render(`
    <C.Title center font="Inter" size={48} weight={700} color="#ff8800" opacity={0.6}>Hello</C.Title>
    <C.Text x={160} y={120} anchorX={0.5} anchorY={0.5} opacity={0.6}
      style={{ fontFamily: 'Inter', fontSize: 48, fontWeight: 700, align: 'center', fill: { type: 'solid', color: '#ff8800' } }}>Hello</C.Text>
  `);
  const { id: _shortId, ...shortLayer } = short;
  const { id: _explicitId, ...explicitLayer } = explicit;
  assert.deepEqual(shortLayer, explicitLayer);
});

test('Theme inherits, nests, and permits local shorthand and full style overrides', () => {
  const [[outer, nested, local, sibling, plain]] = render(`
    <C.Theme font="Inter" size={50} weight={600} color="#112233" style={{ letterSpacing: 2 }}>
      <C.Title>outer</C.Title>
      <C.Theme color="#445566">
        <C.Title>nested</C.Title>
        <C.Title size={40} color="#778899" style={{ fontSize: 32, fill: { type: 'solid', color: '#abcdef' } }}>local</C.Title>
      </C.Theme>
      <C.Title>sibling</C.Title>
      <C.Text>plain</C.Text>
    </C.Theme>
  `);
  assert.equal(outer.content.style.fontFamily, 'Inter');
  assert.equal(outer.content.style.fontSize, 50);
  assert.equal(nested.content.style.fontWeight, 600);
  assert.equal(nested.content.style.letterSpacing, 2);
  assert.equal(nested.content.style.fill.color, '#445566');
  assert.equal(local.content.style.fontSize, 32);
  assert.equal(local.content.style.fill.color, '#abcdef');
  assert.deepEqual(sibling.content.style, outer.content.style);
  assert.notEqual(plain.content.style.fontFamily, 'Inter', 'Theme does not change existing Text');
});

test('centering uses layout bounds and explicit coordinates and anchors override it', () => {
  const [[safeArea, explicit]] = render(`
    <C.SafeArea padding={{ left: 20, right: 40, top: 10, bottom: 30 }}>
      <C.Title center>safe</C.Title>
    </C.SafeArea>
    <C.Title center x={0} y={25} anchorX={0} anchorY="baseline" align="right">explicit</C.Title>
  `);
  assert.deepEqual(safeArea.content.layers[0].transform.position, { x: 130, y: 100 });
  assert.deepEqual(explicit.transform.position, { x: 0, y: 25 });
  assert.equal(explicit.transform.anchor.x, 0);
  assert.equal(explicit.content.baselineAnchor, true);
  assert.equal(explicit.content.style.align, 'right');
});

test('Motion restarts on sequence-local frames and seeking is deterministic', () => {
  const [before, first, middle, done, again] = render(`
    <C.Sequence from={10} durationInFrames={20}>
      <C.Motion x={30} y={40} opacity={0.8}
        enter={{ type: 'slide-up', durationInFrames: 5, distance: 20, easing: t => t }}>
        <C.Rect width={10} height={10} fill="#ffffff" />
      </C.Motion>
    </C.Sequence>
  `, [9, 10, 12, 14, 12]);
  assert.deepEqual(before, []);
  const motion = (layers) => layers[0].content.layers[0];
  assert.equal(motion(first).opacity, 0);
  assert.deepEqual(motion(first).transform.position, { x: 30, y: 60 });
  assert.equal(motion(middle).opacity, 0.4);
  assert.deepEqual(motion(middle).transform.position, { x: 30, y: 50 });
  assert.equal(motion(done).opacity, 0.8);
  assert.deepEqual(motion(done).transform.position, { x: 30, y: 40 });
  assert.deepEqual(again, middle);
});

test('Title scale entrance keeps its pivot at the chosen position', () => {
  const [[start], [done]] = render(`
    <C.Title center enter={{ type: 'scale-in', durationInFrames: 5 }}>center</C.Title>
  `, [0, 4]);
  assert.deepEqual(start.transform.position, { x: 160, y: 120 });
  assert.equal(start.transform.scale.x, 0.8);
  assert.deepEqual(start.content.layers[0].transform.position, { x: 0, y: 0 });
  assert.deepEqual(start.content.layers[0].transform.anchor, { x: 0.5, y: 0.5 });
  assert.equal(done.transform.scale.x, 1);
  assert.deepEqual(done.transform.position, start.transform.position);
});

test('default entrance lasts half a second and settles within short sequences', () => {
  const [[first], [done]] = render('<C.Title enter="fade">default</C.Title>', [0, 11], { fps: 24 });
  assert.equal(first.opacity, 0);
  assert.equal(done.opacity, 1);
  const [[one]] = render('<C.Title enter="fade">single</C.Title>', [0], { duration: 1 });
  assert.equal(one.opacity, 1);
});

test('all slide presets move toward their final position', () => {
  const [[up, down, left, right]] = render(`
    {['slide-up', 'slide-down', 'slide-left', 'slide-right'].map(type => (
      <C.Motion key={type} enter={type}><C.Text>{type}</C.Text></C.Motion>
    ))}
  `);
  assert.deepEqual(up.transform.position, { x: 0, y: 64 });
  assert.deepEqual(down.transform.position, { x: 0, y: -64 });
  assert.deepEqual(left.transform.position, { x: 64, y: 0 });
  assert.deepEqual(right.transform.position, { x: -64, y: 0 });
});

test('invalid entrances fail with actionable errors', () => {
  for (const [enter, error] of [
    ['"unknown"', /unknown <Motion> entrance/],
    ['"toString"', /unknown <Motion> entrance/],
    ['{ type: "fade", durationInFrames: 0 }', /positive integer.*durationInFrames/],
    ['{ type: "slide-up", distance: -1 }', /distance must be a finite non-negative number/],
    ['{ type: "scale-in", scaleFrom: Infinity }', /scaleFrom must be a finite non-negative number/],
  ]) {
    const { result } = evaluate(`<C.Title enter={${enter}}>invalid</C.Title>`);
    assert.match(result.stdout + result.stderr, error);
  }
});

test('the browser runtime exports the same authoring helpers', () => {
  assert.equal(typeof browser.Theme, 'function');
  assert.equal(typeof browser.Title, 'function');
  assert.equal(typeof browser.Motion, 'function');
});
