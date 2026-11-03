// PSD portraits through the CLI. These are integration tests of the build:
// the CLI loads @celesta.studio/character from dist/, so run them after
// `pnpm run build:runtime` (repository root).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import type { Layer, LayerContent } from '@celesta.studio/react';
import { errorOf, frameOf, parseMessage } from './render-cli.mjs';

const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));
const psd = fileURLToPath(new URL('../../../examples/assets/lipsync-fixture.psd', import.meta.url));

/**
 * Renders frame 0 of a composition holding one PSD character and a view of
 * it. `portrait` is the portrait's source (minus `type` and `src`), `view`
 * the view's extra props, and `dialogue`, if given, a line on the view.
 * Returns the view's `psd` layer content, or the render error.
 */
function renderPortrait({ portrait, view = '', dialogue }: { portrait: string; view?: string; dialogue?: string }): Rendered {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-character-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import * as React from 'react';\n` +
      `import { Assets, Composition } from '@celesta.studio/react'; import { Character, CharacterView, Dialogue } from '@celesta.studio/character';\n` +
      `const hana = React.createRef();\n` +
      `const view = React.createRef();\n` +
      `export default function Root() {\n` +
      `  return <Composition width={300} height={200} fps={30} durationInFrames={30}>\n` +
      `    <Assets><Character ref={hana} name="Hana" portrait={{ type: 'psd', src: ${JSON.stringify(psd)}, ${portrait} }} /></Assets>\n` +
      `    <CharacterView ref={view} character={hana} ${view} />\n` +
      (dialogue ? `    <Dialogue character={view} ${dialogue}>Hi</Dialogue>\n` : '') +
      `  </Composition>;\n` +
      `}\n`,
  );
  try {
    const input = JSON.stringify({ time: { value: 0, timescale: 30 } });
    const result = spawnSync(process.execPath, [cli, entry], { input: `${input}\n`, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const [, response] = result.stdout.split(/\r?\n/).filter(Boolean).map(parseMessage);
    const error = errorOf(response);
    if (error) return { error };
    const find = (layers: Layer[]): PsdContent | undefined => {
      for (const layer of layers) {
        if (layer.content.type === 'psd') return layer.content;
        if (layer.content.type === 'group') {
          const found = find(layer.content.layers);
          if (found) return found;
        }
      }
      return undefined;
    };
    return find(frameOf(response).scene.layers);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

type PsdContent = Extract<LayerContent, { type: 'psd' }>;
/** The view's `psd` layer content, or the error rendering it. */
type Rendered = PsdContent | { error: string } | undefined;

/** `rendered`, checked to be the PSD content and not an error. */
function psdOf(rendered: Rendered): PsdContent {
  assert.ok(rendered && !('error' in rendered), rendered && 'error' in rendered ? rendered.error : 'a psd layer');
  return rendered;
}

/** The error `rendered` reports. */
function renderErrorOf(rendered: Rendered): string {
  assert.ok(rendered && 'error' in rendered, 'a render error');
  return rendered.error;
}

const EXPRESSIONS = `layers: ['body'], defaultExpression: 'calm',
  expressions: { calm: ['face/calm'], smile: 'face/smile', angry: { layers: ['face/angry'], lipSync: {
    a: 'face/angry/a', i: 'face/angry/i', u: 'face/angry/u', e: 'face/angry/e', o: 'face/angry/o', closed: 'face/angry/n' } } },
  lipSync: { a: 'mouth/a', i: 'mouth/i', u: 'mouth/u', e: 'mouth/e', o: 'mouth/o', closed: 'mouth/n' }`;

test('a PSD portrait shows its default expression on top of its shared layers', () => {
  const content = psdOf(renderPortrait({ portrait: EXPRESSIONS }));
  assert.deepEqual(content.visibleLayers, ['body', 'face/calm']);
});

test('a PSD portrait shows the expression its view picks', () => {
  const content = psdOf(renderPortrait({ portrait: EXPRESSIONS, view: 'expression="smile" mouth="a"' }));
  assert.deepEqual(content.visibleLayers, ['body', 'face/smile']);
  assert.deepEqual(content.enabledLayers, ['mouth/a']);
});

test('a PSD expression with its own mouths lip-syncs with them', () => {
  const content = psdOf(renderPortrait({ portrait: EXPRESSIONS, view: 'expression="angry" mouth="o"' }));
  assert.deepEqual(content.visibleLayers, ['body', 'face/angry']);
  assert.deepEqual(content.enabledLayers, ['face/angry/o']);
  assert.ok(content.disabledLayers!.includes('face/angry/a'));
  assert.ok(!content.disabledLayers!.includes('mouth/a'));
});

test('a dialogue line switches a PSD portrait to its expression', () => {
  const content = psdOf(renderPortrait({ portrait: EXPRESSIONS, dialogue: 'expression="smile"' }));
  assert.deepEqual(content.visibleLayers, ['body', 'face/smile']);
});

test('an unknown PSD expression is an error, not silently ignored', () => {
  assert.match(renderErrorOf(renderPortrait({ portrait: EXPRESSIONS, view: 'expression="sad"' })), /no expression "sad"/);
  assert.match(renderErrorOf(renderPortrait({ portrait: `layers: ['body']`, view: 'expression="smile"' })), /no expression "smile"/);
});

test('a PSD portrait without expressions keeps its layers', () => {
  const content = psdOf(renderPortrait({ portrait: `layers: ['body', 'face/calm']` }));
  assert.deepEqual(content.visibleLayers, ['body', 'face/calm']);
});

// Public character PSDs are saved showing the body and one of each face part.
const SAVED = `defaultExpression: 'calm',
  hide: 'props/sword',
  expressions: {
    calm: [],
    smile: { layers: ['face/brows/smile', 'face/cheeks/red'], hide: ['face/brows', 'face/cheeks'] },
  },
  lipSync: { a: 'face/mouth/a', i: 'face/mouth/i', u: 'face/mouth/u', e: 'face/mouth/e', o: 'face/mouth/o', closed: 'face/mouth/n' }`;

test('a PSD portrait without layers shows its expression on top of its saved visibility', () => {
  const content = psdOf(renderPortrait({ portrait: SAVED, view: 'expression="smile" mouth="a"' }));
  assert.equal(content.visibleLayers, undefined);
  assert.deepEqual(content.enabledLayers, ['face/brows/smile', 'face/cheeks/red', 'face/mouth/a']);
  assert.ok(content.disabledLayers!.includes('face/mouth/n'));
  assert.ok(['props/sword', 'face/brows', 'face/cheeks'].every((path) => content.disabledLayers!.includes(path)));
});

test('a PSD portrait hides its hide layers under every expression', () => {
  const content = psdOf(renderPortrait({ portrait: SAVED }));
  assert.equal(content.visibleLayers, undefined);
  assert.equal(content.enabledLayers, undefined);
  assert.deepEqual(content.disabledLayers, ['props/sword']);
});

test('hidden layers never hide the expression, mouth, or eyes that show', () => {
  const content = psdOf(renderPortrait({
    portrait: `layers: ['body', 'face/mouth/n'], hide: ['face/mouth/n', 'eyes/open'], blink: { open: 'eyes/open', closed: 'eyes/shut' },
      defaultExpression: 'smile',
      expressions: { smile: { layers: ['face/brows/smile'], hide: ['face/brows', 'face/brows/smile'] } },
      lipSync: { a: 'face/mouth/a', i: 'face/mouth/i', u: 'face/mouth/u', e: 'face/mouth/e', o: 'face/mouth/o', closed: 'face/mouth/n' }`,
    view: 'mouth="closed" blink={false}',
  }));
  assert.deepEqual(content.visibleLayers, ['body', 'face/mouth/n', 'face/brows/smile']);
  // A preset layer inside a hidden folder is forced on so the folder cannot hide it.
  assert.deepEqual(content.enabledLayers, ['face/mouth/n', 'face/brows/smile', 'eyes/open']);
  assert.ok(content.disabledLayers!.includes('face/brows'));
  assert.ok(['face/mouth/n', 'eyes/open', 'face/brows/smile'].every((path) => !content.disabledLayers!.includes(path)));
});
