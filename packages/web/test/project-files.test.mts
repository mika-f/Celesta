import assert from 'node:assert/strict';
import { test } from 'vitest';
import { build } from 'esbuild-wasm';
import type { BuildOptions } from 'esbuild-wasm';
import { currentModuleName, projectFiles } from '../src/project-files';

/** What a bundle's `module.exports` holds, read back as the test expects it. */
type Exports<T> = { exports: { default?: T } };
type Element = { type: unknown; props: Record<string, unknown> };
const React = { createElement: (type: unknown, props: Record<string, unknown>): Element => ({ type, props }) };

test('bundle a nested TSX project, resolve JSON and keep runtime imports external', async () => {
  const files = {
    'film.tsx': "import { Scene } from './scenes'; export default Scene;",
    'scenes/index.tsx': "import { Composition } from '@celesta.studio/react'; import data from '../data.json'; export function Scene() { return <Composition width={data.width} />; }",
    'data.json': '{"width":320}',
  };
  const options = { entryPoints: ['film.tsx'], bundle: true, write: false, format: 'cjs', jsxFactory: 'React.createElement' } satisfies BuildOptions;
  const result = await build({ ...options, plugins: [projectFiles(files, 'film.tsx')] });
  const module: Exports<() => Element> = { exports: {} };
  const Component = Symbol('Composition');
  new Function('module', 'exports', 'require', 'React', result.outputFiles[0].text)(
    module, module.exports,
    (name: string) => { assert.equal(name, '@celesta.studio/react'); return { Composition: Component }; },
    React,
  );
  assert.deepEqual(module.exports.default!(), { type: Component, props: { width: 320 } });
  await assert.rejects(build({ ...options, logLevel: 'silent', plugins: [projectFiles({ 'film.tsx': "import fs from 'node:fs'; export default fs;" }, 'film.tsx')] }), /unavailable in the web editor/);
  await assert.rejects(build({ ...options, logLevel: 'silent', plugins: [projectFiles({ 'film.tsx': "import Scene from './missing'; export default Scene;" }, 'film.tsx')] }), /Missing project source/);
});

test('the scope before @celesta.studio stays a runtime import', async () => {
  assert.equal(currentModuleName('@celesta/react'), '@celesta.studio/react');
  assert.equal(currentModuleName('@celesta.studio/react'), '@celesta.studio/react');
  assert.equal(currentModuleName('react'), 'react');
  const files = { 'film.tsx': "import { Composition } from '@celesta/react'; export default Composition;" };
  const result = await build({ entryPoints: ['film.tsx'], bundle: true, write: false, format: 'cjs', plugins: [projectFiles(files, 'film.tsx')] });
  const module: Exports<unknown> = { exports: {} };
  const Component = Symbol('Composition');
  new Function('module', 'exports', 'require', result.outputFiles[0].text)(
    module, module.exports,
    (name: string) => { assert.equal(currentModuleName(name), '@celesta.studio/react'); return { Composition: Component }; },
  );
  assert.equal(module.exports.default, Component);
});

test('directory imports resolve JSX, JavaScript and JSON index files', async () => {
  const files = {
    'film.tsx': "import Scene from './scene'; import value from './value'; import data from './data'; export default () => Scene(value + data.width);",
    'scene/index.jsx': 'export default width => <rect width={width} />;',
    'value/index.js': 'export default 20;',
    'data/index.json': '{"width":300}',
  };
  const result = await build({ entryPoints: ['film.tsx'], bundle: true, write: false, format: 'cjs', jsxFactory: 'React.createElement', plugins: [projectFiles(files, 'film.tsx')] });
  const module: Exports<() => Element> = { exports: {} };
  new Function('module', 'exports', 'React', result.outputFiles[0].text)(module, module.exports, React);
  assert.deepEqual(module.exports.default!(), { type: 'rect', props: { width: 320 } });
});

test('subtitle files import as their text', async () => {
  const files = {
    'film.tsx': "import srt from './captions.srt'; import vtt from './captions.vtt'; export default [srt, vtt];",
    'captions.srt': '1\n00:00:00,000 --> 00:00:01,000\nHi\n',
    'captions.vtt': 'WEBVTT\n',
  };
  const result = await build({ entryPoints: ['film.tsx'], bundle: true, write: false, format: 'cjs', plugins: [projectFiles(files, 'film.tsx')] });
  const module: Exports<string[]> = { exports: {} };
  new Function('module', 'exports', result.outputFiles[0].text)(module, module.exports);
  assert.deepEqual(module.exports.default, [files['captions.srt'], files['captions.vtt']]);
});
