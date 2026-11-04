import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import React from 'react';
import type { ReactNode } from 'react';
import { test } from 'vitest';
import { Audio, Composition, Text, useCurrentFrame, useVideoConfig } from '@celesta.studio/react';
import type { CompositionProps } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import type { EntryComponent } from '@celesta.studio/react/internal';
import type { FrameMessage, ReadyMessage } from '../src/cli.js';
import { contentOf } from './layer-content.mjs';

const config = { width: 400, height: 200, fps: 30, durationInFrames: 3 };
const root = (props: Partial<CompositionProps>, children?: ReactNode) =>
  React.createElement(Composition, { ...config, ...props }, children);
const fragment = (...children: ReactNode[]) => React.createElement(React.Fragment, null, ...children);

function Caption() {
  const { width, height, fps, durationInFrames } = useVideoConfig();
  return React.createElement(Text, { children: `${width}x${height}/${fps}/${durationInFrames}/${useCurrentFrame()}` });
}

test('selects a root by ID with its own clock, dimensions, audio and persistent state', () => {
  let inactiveRenders = 0;
  let selectedMounts = 0;
  function StatefulCaption() { React.useState(() => ++selectedMounts); return React.createElement(Caption); }
  function Inactive(): ReactNode { inactiveRenders++; throw new Error('inactive root mounted'); }
  const entry = () => fragment(
    root({ id: 'landscape' }, React.createElement(Inactive)),
    root({ id: 'portrait', width: 200, height: 400, fps: 24, durationInFrames: 2 }, fragment(
      React.createElement(StatefulCaption), React.createElement(Audio, { src: 'portrait.wav' }))));
  const selected = mount(entry, 'portrait');
  try {
    assert.equal(selected.compositionId, 'portrait');
    assert.deepEqual(selected.compositions.map(({ id, width, height, frameRate, durationInFrames }) =>
      [id, width, height, frameRate.numerator, durationInFrames]),
      [['landscape', 400, 200, 30, 3], ['portrait', 200, 400, 24, 2]]);
    const frame = selected.renderAt({ value: 1, timescale: 24 }, null);
    assert.deepEqual([frame.scene.width, frame.scene.height, frame.scene.frameRate.numerator], [200, 400, 24]);
    assert.equal(contentOf(frame.scene.layers[0], 'text').text, '200x400/24/2/1');
    assert.equal(selected.collectAudio().length, 2);
    assert.ok(selected.collectAudio().every(({ src }) => src === 'portrait.wav'));
    assert.equal(inactiveRenders, 0);
    assert.equal(selectedMounts, 1);
  } finally { selected.dispose(); }
});

test('defaults to the first root and keeps unnamed single entries compatible', () => {
  for (const [entry, id] of [
    [() => root({}, React.createElement(Caption)), 'default'],
    [() => fragment(root({ id: 'first' }), root({ id: 'second' })), 'first'],
  ] satisfies [EntryComponent, string][]) {
    const selected = mount(entry);
    try {
      assert.equal(selected.compositionId, id);
      assert.equal(selected.renderAt({ value: 0, timescale: 30 }, null).scene.width, 400);
    } finally { selected.dispose(); }
  }
});

test('rejects missing, empty, duplicate and unknown IDs with useful diagnostics', () => {
  assert.throws(() => mount(() => fragment(root({}), root({ id: 'b' }))), /non-empty string `id`/);
  assert.throws(() => mount(() => root({ id: ' ' })), /non-empty string `id`/);
  assert.throws(() => mount(() => fragment(root({ id: 'a' }), root({ id: 'a' }))), /duplicate.*a/);
  assert.throws(() => mount(() => root({ id: 'a' }), 'missing'), /unknown.*missing.*available: a/);
});

// Exercises the built CLI (inspect.mjs runs dist/cli.js): run `pnpm run
// build:runtime` (repository root) first.
test('inspect.mjs exposes the list and selects frames with shared entry properties', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-compositions-'));
  const entry = join(dir, 'entry.tsx');
  const inspector = fileURLToPath(new URL('../../../skills/celesta/scripts/inspect.mjs', import.meta.url));
  const runtime = fileURLToPath(new URL('../dist/cli.js', import.meta.url));
  writeFileSync(entry, `
    import { Composition, Text, defineProjectProperties, getProjectProperty } from '@celesta.studio/react';
    defineProjectProperties({ title: { type: 'string', defaultValue: 'shared' } });
    export default () => <>
      <Composition id="wide" width={400} height={200} fps={30} durationInFrames={3}><Text>{getProjectProperty('title')}</Text></Composition>
      <Composition id="tall" width={200} height={400} fps={24} durationInFrames={2}><Text>{getProjectProperty('title')}</Text></Composition>
    </>;
  `);
  try {
    for (const [id, width] of [['wide', 400], ['tall', 200]] satisfies [string, number][]) {
      const result = spawnSync(process.execPath, [inspector, entry, '--no-native', '--runtime', runtime,
        '--json', '--frames', '0,-1', '--composition', id, '--props', '{"title":"changed"}'], { encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
      const { ready, frames }: { ready: ReadyMessage; frames: FrameMessage[] } = JSON.parse(result.stdout);
      assert.equal(ready.compositionId, id);
      assert.deepEqual(ready.compositions.map(({ id }) => id), ['wide', 'tall']);
      assert.ok(frames.every(({ scene }) => scene.width === width
        && scene.layers[0].content.type === 'text' && scene.layers[0].content.text === 'changed'));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
