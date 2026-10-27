// Integration test of the build: runs packages/cli/bin, which loads the
// built dist/ of this package and the others, so it stays on built output.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import { contentOf, parseLine } from './helpers.mjs';
const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

/** Runs an entry whose module scope is `setup` and whose root holds `body`; one frame per time. */
function frames(setup: string, body: string, times = [0]) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-instances-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `import { Composition, Instances, useCurrentFrame } from '@celesta/react';
    ${setup}
    export default () => <Composition width={100} height={100} fps={30} durationInFrames={10}>${body}</Composition>;`);
  const input = times.map((value) => JSON.stringify({ time: { value, timescale: 30 } })).join('\n') + '\n';
  try {
    const result = spawnSync(process.execPath, [cli, entry], { input, encoding: 'utf8', maxBuffer: 64 << 20 });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim().split(/\r?\n/).map(parseLine).slice(1);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
const layer = (props: string, setup = '') => frames(setup, `<Instances ${props} />`)[0];
const f32 = (values: ArrayLike<number>) => Buffer.from(new Float32Array(values).buffer).toString('base64');
const u8 = (values: ArrayLike<number>) => Buffer.from(new Uint8Array(values)).toString('base64');

test('circles reach the scene as base64 columns', () => {
  const [frame] = [layer(
    'count={2} x={new Float32Array([1.5, -2])} y={new Float32Array([3, 4])} width={4} height={4} color={new Uint8Array([255, 0, 0, 255, 0, 0, 255, 128])} shape="circle"',
  )];
  const [instances] = frame.scene.layers;
  assert.deepEqual(instances.content, {
    type: 'instances',
    count: 2,
    shape: { type: 'circle' },
    width: 4,
    height: 4,
    x: f32([1.5, -2]),
    y: f32([3, 4]),
    colors: u8([255, 0, 0, 255, 0, 0, 255, 128]),
  });
  assert.deepEqual(instances.transform.position, { x: 0, y: 0 });
  assert.equal(instances.transform.rotation ?? 0, 0);
});

test('a subarray sends only its own elements', () => {
  const frame = layer('count={2} x={new Float32Array([9, 1, 2]).subarray(1)} y={[0, 0]} width={1} height={1}');
  assert.equal(contentOf(frame.scene.layers[0], 'instances').x, f32([1, 2]));
});

test('plain arrays are columns, and array sizes become widths and heights', () => {
  const frame = layer('count={2} x={[1, 2]} y={[3, 4]} width={[5, 6]} height={7} rotation={[0, 90]} fill="#ff0000"');
  const content = contentOf(frame.scene.layers[0], 'instances');
  assert.equal(content.x, f32([1, 2]));
  assert.equal(content.widths, f32([5, 6]));
  assert.equal(content.height, 7);
  assert.equal(content.rotation, f32([0, 90]));
  assert.equal(content.color, '#ff0000');
  assert.equal(content.width, undefined);
  assert.equal(content.heights, undefined);
  assert.deepEqual(content.shape, { type: 'rect', cornerRadius: 0 });
});

test('an image source draws a sprite of its natural size', () => {
  const frame = layer('count={1} x={[1]} y={[2]} src="./spark.png"');
  const content = contentOf(frame.scene.layers[0], 'instances');
  assert.deepEqual(content.shape, { type: 'image', asset: { id: './spark.png', location: { type: 'file', path: './spark.png' } } });
  assert.equal(content.width, undefined);
  assert.equal(content.height, undefined);
});

test('scale and opacity apply to the layer; particle columns do not move it', () => {
  const frame = layer('count={1} x={[40]} y={[50]} width={2} height={2} scale={2} opacity={0.5} blendMode="screen"');
  const [instances] = frame.scene.layers;
  assert.deepEqual(instances.transform.scale, { x: 2, y: 2 });
  assert.deepEqual(instances.transform.position, { x: 0, y: 0 });
  assert.equal(instances.opacity, 0.5);
  assert.equal(instances.blendMode, 'screen');
});

test('invalid props fail the frame and name the prop', () => {
  for (const [props, name] of [
    ['count={-1} x={[]} y={[]} width={1} height={1}', 'count'],
    ['count={1.5} x={[1]} y={[1]} width={1} height={1}', 'count'],
    ['count={2} x={[1]} y={[1, 2]} width={1} height={1}', 'x'],
    ['count={1} x={[1]} y={[1]} width={1} height={1} color={[1, 2, 3]}', 'color'],
    ['count={1} x={[1]} y={[1]} width={1} height={1} shape="star"', 'shape'],
    ['count={1} x={[1]} y={[1]} height={1}', 'width'],
    ['count={1} x={1} y={[1]} width={1} height={1}', 'x'],
    ['count={1} x={[1]} y={[1]} width={1} height={1} fill="red"', 'fill'],
    ['count={1} x={[1]} y={[1]} width={1} height={1} fill="#aéaaa"', 'fill'],
  ]) {
    const frame = layer(props);
    assert.match(frame.error ?? '', new RegExp(`<Instances> ${name}`), props);
  }
});

test('no particles send empty columns', () => {
  const frame = layer('count={0} x={[]} y={[]} width={1} height={1}');
  assert.equal(contentOf(frame.scene.layers[0], 'instances').x, '');
});

test('a reused array sends its contents of each frame', () => {
  const [first, second] = frames(
    'const xs = new Float32Array(1); function Moving() { xs[0] = useCurrentFrame(); return <Instances count={1} x={xs} y={[0]} width={1} height={1} />; }',
    '<Moving />',
    [0, 1],
  );
  assert.equal(contentOf(first.scene.layers[0], 'instances').x, f32([0]));
  assert.equal(contentOf(second.scene.layers[0], 'instances').x, f32([1]));
});

test('a column longer than one base64 piece encodes whole', () => {
  // 200,001 floats: 800,004 bytes, three pieces with a short last one.
  const frame = layer(
    'count={200_001} x={xs} y={xs} width={1} height={1}',
    'const xs = Float32Array.from({ length: 200_001 }, (_, i) => i * 0.5);',
  );
  const expected = f32(Float32Array.from({ length: 200_001 }, (_, i) => i * 0.5));
  assert.equal(contentOf(frame.scene.layers[0], 'instances').x, expected);
});
