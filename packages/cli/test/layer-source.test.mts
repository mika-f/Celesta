// `layerSource` requests: the editor asks which JSX produced a layer. Run
// after `pnpm run build:runtime`: the tests exercise the built CLI (bin/
// loads dist/cli.js).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { LayerSourceReport, OccurrenceIndex } from '@celesta/react/internal';
import type { FrameMessage, Request } from '../src/cli.js';
import { contentOf } from './layer-content.mjs';

/** The answer to a `LayerSourceRequest`. */
type LayerSourceMessage = { layerSource: LayerSourceReport | { chain: null } };
/** The answer to a `CollectOccurrencesRequest`. */
type CollectedOccurrencesMessage = { collectedOccurrences: OccurrenceIndex };

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

const time = { value: 0, timescale: 30 };

function run(requests: Request[], flags = ['--source-locations']) {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), 'celesta-layer-source-')));
  const entry = join(dir, 'entry.tsx');
  const title = join(dir, 'title.tsx');
  // The Rect is on line 3, column 10 of title.tsx.
  writeFileSync(title, [
    "import { Rect } from '@celesta/react';",
    'export function Title() {',
    '  return <Rect id="title-rect" width={4} height={4} fill="#ffffff" />;',
    '}',
    '',
  ].join('\n'));
  writeFileSync(entry, `
    import { Composition, Rect, Static, Text } from '@celesta/react';
    import { Title } from './title';
    export default function Film() {
      return <Composition width={64} height={64} fps={30} durationInFrames={3}>
        <Title />
        <Static><Rect id="static-rect" width={4} height={4} fill="#00ff00" /></Static>
        <Text id="label">hello</Text>
      </Composition>;
    }
  `);
  try {
    const result = spawnSync(process.execPath, [cli, entry, ...flags], {
      input: requests.map((request) => `${JSON.stringify(request)}\n`).join(''),
      encoding: 'utf8',
    });
    const responses: (FrameMessage | LayerSourceMessage | CollectedOccurrencesMessage)[] = result.stdout.trim().split('\n').slice(1).map((line) => JSON.parse(line));
    return { dir, entry, title, result, responses };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('finds the JSX of a layer in another file', () => {
  const { responses, title, entry, result } = run([{ layerSource: 'title-rect', time, preview: true }]);
  assert.equal(result.status, 0, result.stderr);
  const { chain } = (responses[0] as LayerSourceMessage).layerSource;
  assert.ok(chain, JSON.stringify(responses));
  assert.equal(chain[0].name, 'Rect');
  assert.equal(chain[0].file, title);
  assert.equal(chain[0].line, 3);
  assert.equal(chain[0].column, 10);
  const outer = chain.find((entryItem) => entryItem.name === 'Title');
  assert.ok(outer, JSON.stringify(chain));
  assert.equal(outer.file, entry);
});

test('finds layers inside <Static>', () => {
  const { responses, result } = run([
    { time, compactTransforms: true, statics: { decoded: 0 } },
    { layerSource: 'static-rect', time, preview: true },
    { time, compactTransforms: true, statics: { decoded: 1 } },
  ]);
  assert.equal(result.status, 0, result.stderr);
  const [first, lookup, second] = responses as [FrameMessage, LayerSourceMessage, FrameMessage];
  assert.ok(lookup.layerSource.chain && lookup.layerSource.chain.length > 0);
  assert.equal(lookup.layerSource.chain[0].name, 'Rect');
  const reference = (frame: FrameMessage) =>
    contentOf(frame.scene.layers.find((layer) => layer.content.type === 'staticRef'), 'staticRef');
  assert.equal(second.seq, first.seq! + 1);
  assert.equal(reference(second).key, reference(first).key);
  assert.equal('statics' in second, false);
});

test('answers null for unknown ids', () => {
  const { responses } = run([{ layerSource: 'nope', time, preview: true }]);
  assert.deepEqual(responses[0], { layerSource: { chain: null } });
});

test('answers [] without --source-locations', () => {
  const { responses, result } = run([{ layerSource: 'title-rect', time, preview: true }], []);
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(responses[0], { layerSource: { chain: [] } });
});

test('does not change scenes', () => {
  const requests = [0, 1, 2].map((value) => ({ time: { value, timescale: 30 }, preview: true }));
  const withFlag = run(requests).responses.map((frame) => (frame as FrameMessage).scene);
  const without = run(requests, []).responses.map((frame) => (frame as FrameMessage).scene);
  assert.equal(withFlag.length, 3);
  assert.deepEqual(withFlag, without);
});

test('collects where each element renders', () => {
  const { responses, title, entry, result } = run([{ collectOccurrences: { from: 0, frames: 10, preview: true } }]);
  assert.equal(result.status, 0, result.stderr);
  const { elements } = (responses[0] as CollectedOccurrencesMessage).collectedOccurrences;
  const rect = elements.find(({ element }) => element.file === title);
  assert.ok(rect, JSON.stringify(responses));
  assert.deepEqual(rect.element, { file: title, line: 3, column: 10 });
  assert.equal(rect.instances.length, 1);
  assert.equal(rect.instances[0].chain[1].file, entry);
  // Clipped to the composition's 3 frames.
  assert.deepEqual(rect.instances[0].ranges, [[0, 3]]);
});

test('rejects unknown options', () => {
  const { result } = run([], ['--source-location']);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /^usage: .*\[--source-locations\]/);
});

// Host elements such as <CharacterView> and <Dialogue> build their child
// layers themselves; those ids map to the host element's JSX.
function runCharacter(requests: Request[]) {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), 'celesta-layer-source-character-')));
  const entry = join(dir, 'entry.tsx');
  // The <CharacterView> is on line 9, column 7; the <Dialogue> on line 10, column 7.
  writeFileSync(entry, [
    "import React from 'react';",
    "import { Assets, Composition } from '@celesta/react';",
    "import { Character, CharacterView, Dialogue } from '@celesta/character';",
    'const character = React.createRef();',
    'const view = React.createRef();',
    'export default function Film() {',
    '  return <Composition width={64} height={64} fps={30} durationInFrames={3}>',
    "    <Assets><Character ref={character} name=\"Hana\" portrait={{ expressions: { normal: './portrait.png' }, defaultExpression: 'normal' }} /></Assets>",
    '      <CharacterView id="hero-view" ref={view} character={character} />',
    '      <Dialogue id="hero-line" character={view}>hello</Dialogue>',
    '  </Composition>;',
    '}',
    '',
  ].join('\n'));
  try {
    const result = spawnSync(process.execPath, [cli, entry, '--source-locations'], {
      input: requests.map((request) => `${JSON.stringify(request)}\n`).join(''),
      encoding: 'utf8',
    });
    const responses: LayerSourceMessage[] = result.stdout.trim().split('\n').slice(1).map((line) => JSON.parse(line));
    return { entry, result, responses };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('finds the JSX of a layer a host element builds', () => {
  const { responses, entry, result } = runCharacter([
    { layerSource: 'hero-view.portrait', time, preview: true },
    { layerSource: 'hero-line.subtitle', time, preview: true },
    { layerSource: 'hero-view.nope', time, preview: true },
  ]);
  assert.equal(result.status, 0, result.stderr);
  const [portrait, subtitle, unknown] = responses.map((response) => response.layerSource.chain);
  assert.ok(portrait, JSON.stringify(responses));
  assert.equal(portrait[0].name, 'CharacterView');
  assert.equal(portrait[0].file, entry);
  assert.equal(portrait[0].line, 9);
  assert.equal(portrait[0].column, 7);
  assert.ok(subtitle, JSON.stringify(responses));
  assert.equal(subtitle[0].name, 'Dialogue');
  assert.equal(subtitle[0].line, 10);
  assert.equal(unknown, null);
});
