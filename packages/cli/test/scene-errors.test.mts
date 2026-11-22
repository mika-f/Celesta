// A scene that throws: preview frames carry `sceneErrors` and draw the
// scene's error card, every other frame request fails naming the scene. Run
// after `pnpm run build:runtime` (repository root): the test exercises the
// built CLI (bin/ loads dist/cli.js).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { Layer } from '@celesta.studio/react';
import type { FrameMessage, Request } from '../src/cli.js';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));
const entry = fileURLToPath(new URL('../examples/with-scene-error.tsx', import.meta.url));

type Answer = FrameMessage | { error: string } | { collectedAudio: unknown[] };

function answers(requests: Request[]): { answers: Answer[]; stderr: string } {
  const result = spawnSync(process.execPath, [cli, entry], {
    input: requests.map((request) => `${JSON.stringify(request)}\n`).join(''),
    encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  return { answers: result.stdout.trim().split('\n').slice(1).map((line) => JSON.parse(line)), stderr: result.stderr };
}

const texts = (layers: Layer[]): string[] => layers.flatMap((layer) =>
  layer.content.type === 'text' ? [layer.content.text] : texts(layer.content.type === 'group' ? layer.content.layers : []),
);
const time = (frame: number) => ({ value: frame, timescale: 30 });
const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

test('preview frames draw the card and report the scene; other frames fail naming it', () => {
  const { answers: [card, good, failed, outro, audio], stderr } = answers([
    { time: time(120), preview: true },
    { time: time(70), preview: true },
    { time: time(120) },
    { time: time(170) },
    { collectAudio: true, preview: true },
  ]);
  assert.ok('scene' in card);
  assert.equal(texts(card.scene.layers)[0], 'Scene failed to render');
  const [report] = card.sceneErrors ?? [];
  assert.equal(report?.id, 'stats');
  assert.equal(report?.name, 'Stats');
  assert.equal(report?.message, "Cannot read properties of undefined (reading 'label')");
  // The stack names the entry's own file and line, not the temporary bundle.
  assert.match(report?.stack ?? '', new RegExp(`at Broken \\(${escape(entry)}:40:`));
  assert.match(report?.componentStack ?? '', new RegExp(`^\\s+at Broken \\(${escape(entry)}:`));

  assert.ok('scene' in good);
  assert.deepEqual(texts(good.scene.layers), ['Views: 1200 10']);
  assert.equal(good.sceneErrors, undefined);

  assert.deepEqual(failed, {
    error: "scene \"stats\" (Stats) threw while rendering: Cannot read properties of undefined (reading 'label')",
  });
  assert.ok('scene' in outro);
  assert.deepEqual(texts(outro.scene.layers), ['Outro 20']);
  assert.deepEqual(audio, { collectedAudio: [] });
  // Caught scene errors are reported in the frames, not printed.
  assert.equal(stderr, '');
});
