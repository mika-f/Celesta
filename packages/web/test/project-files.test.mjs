import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild-wasm';
import { projectFiles } from '../src/project-files.ts';

test('bundle a nested TSX project, resolve JSON and keep runtime imports external', async () => {
  const files = {
    'film.tsx': "import { Scene } from './scenes'; export default Scene;",
    'scenes/index.tsx': "import { Composition } from '@celesta/react'; import data from '../data.json'; export function Scene() { return <Composition width={data.width} />; }",
    'data.json': '{"width":320}',
  };
  const options = { entryPoints: ['film.tsx'], bundle: true, write: false, format: 'cjs', jsxFactory: 'React.createElement' };
  const result = await build({ ...options, plugins: [projectFiles(files, 'film.tsx')] });
  const module = { exports: {} };
  const Component = Symbol('Composition');
  new Function('module', 'exports', 'require', 'React', result.outputFiles[0].text)(
    module, module.exports,
    name => { assert.equal(name, '@celesta/react'); return { Composition: Component }; },
    { createElement: (type, props) => ({ type, props }) },
  );
  assert.deepEqual(module.exports.default(), { type: Component, props: { width: 320 } });
  await assert.rejects(build({ ...options, logLevel: 'silent', plugins: [projectFiles({ 'film.tsx': "import fs from 'node:fs'; export default fs;" }, 'film.tsx')] }), /unavailable in the web editor/);
  await assert.rejects(build({ ...options, logLevel: 'silent', plugins: [projectFiles({ 'film.tsx': "import Scene from './missing'; export default Scene;" }, 'film.tsx')] }), /Missing project source/);
});
