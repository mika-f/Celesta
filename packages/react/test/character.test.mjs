import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));
const psd = fileURLToPath(new URL('../../../examples/assets/lipsync-fixture.psd', import.meta.url));

/**
 * Renders frame 0 of a composition holding one PSD character and a view of
 * it. `portrait` is the portrait's source (minus `type` and `src`), `view`
 * the view's extra props, and `dialogue`, if given, a line on the view.
 * Returns the view's `psd` layer content, or the render error.
 */
function renderPortrait({ portrait, view = '', dialogue }) {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-character-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import * as React from 'react';\n` +
      `import { Assets, Character, CharacterView, Composition, Dialogue } from '@celesta/react';\n` +
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
    const [, response] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
    if (response.error) return { error: response.error };
    const find = (layers) => {
      for (const layer of layers) {
        if (layer.content.type === 'psd') return layer.content;
        if (layer.content.type === 'group') {
          const found = find(layer.content.layers);
          if (found) return found;
        }
      }
      return undefined;
    };
    return find(response.scene.layers);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const EXPRESSIONS = `layers: ['body'], defaultExpression: 'calm',
  expressions: { calm: ['face/calm'], smile: 'face/smile', angry: { layers: ['face/angry'], lipSync: {
    a: 'face/angry/a', i: 'face/angry/i', u: 'face/angry/u', e: 'face/angry/e', o: 'face/angry/o', closed: 'face/angry/n' } } },
  lipSync: { a: 'mouth/a', i: 'mouth/i', u: 'mouth/u', e: 'mouth/e', o: 'mouth/o', closed: 'mouth/n' }`;

test('a PSD portrait shows its default expression on top of its shared layers', () => {
  const content = renderPortrait({ portrait: EXPRESSIONS });
  assert.deepEqual(content.visibleLayers, ['body', 'face/calm']);
});

test('a PSD portrait shows the expression its view picks', () => {
  const content = renderPortrait({ portrait: EXPRESSIONS, view: 'expression="smile" mouth="a"' });
  assert.deepEqual(content.visibleLayers, ['body', 'face/smile']);
  assert.deepEqual(content.enabledLayers, ['mouth/a']);
});

test('a PSD expression with its own mouths lip-syncs with them', () => {
  const content = renderPortrait({ portrait: EXPRESSIONS, view: 'expression="angry" mouth="o"' });
  assert.deepEqual(content.visibleLayers, ['body', 'face/angry']);
  assert.deepEqual(content.enabledLayers, ['face/angry/o']);
  assert.ok(content.disabledLayers.includes('face/angry/a'));
  assert.ok(!content.disabledLayers.includes('mouth/a'));
});

test('a dialogue line switches a PSD portrait to its expression', () => {
  const content = renderPortrait({ portrait: EXPRESSIONS, dialogue: 'expression="smile"' });
  assert.deepEqual(content.visibleLayers, ['body', 'face/smile']);
});

test('an unknown PSD expression is an error, not silently ignored', () => {
  assert.match(renderPortrait({ portrait: EXPRESSIONS, view: 'expression="sad"' }).error, /no expression "sad"/);
  assert.match(renderPortrait({ portrait: `layers: ['body']`, view: 'expression="smile"' }).error, /no expression "smile"/);
});

test('a PSD portrait without expressions keeps its layers', () => {
  const content = renderPortrait({ portrait: `layers: ['body', 'face/calm']` });
  assert.deepEqual(content.visibleLayers, ['body', 'face/calm']);
});
