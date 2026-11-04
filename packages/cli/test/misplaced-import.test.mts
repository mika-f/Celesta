// Exercises the built CLI (bin/ loads dist/cli.js): run `pnpm run
// build:runtime` (repository root) first.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

function load(source: string): { error?: string; config?: { width: number }; legacyImports?: unknown } {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-imports-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, source);
  try {
    const result = spawnSync(process.execPath, [cli, entry], { input: '', encoding: 'utf8' });
    return JSON.parse(result.stdout.split(/\r?\n/)[0]);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test('a name imported from the wrong Celesta package names the one that exports it', () => {
  const response = load(
    `import { Composition, Circle } from '@celesta.studio/react';\n` +
      `export default () => <Composition width={10} height={10} fps={1} durationInFrames={1}><Circle radius={2} /></Composition>;\n`,
  );
  assert.equal(response.error, '@celesta.studio/react does not export Circle; import it from @celesta.studio/shapes');
});

test('an unknown Celesta package fails the build', () => {
  const response = load(
    `import { Composition } from '@celesta.studio/react';\nimport { Nope } from '@celesta.studio/nope';\n` +
      `export default () => <Composition width={10} height={10} fps={1} durationInFrames={1}><Nope /></Composition>;\n`,
  );
  assert.match(response.error ?? '', /@celesta\.studio\/nope is not a Celesta package/);
});

test('an entry importing the scope before @celesta.studio still uses the runtime', () => {
  const response = load(
    `import { Composition } from '@celesta/react';\nimport { Circle } from '@celesta/shapes';\n` +
      `export default () => <Composition width={10} height={10} fps={1} durationInFrames={1}><Circle radius={2} /></Composition>;\n`,
  );
  assert.equal(response.error, undefined);
  assert.equal(response.config?.width, 10);
  assert.deepEqual(response.legacyImports, [
    { file: 'entry.tsx', specifier: '@celesta/react', current: '@celesta.studio/react' },
    { file: 'entry.tsx', specifier: '@celesta/shapes', current: '@celesta.studio/shapes' },
  ]);
});

test('an entry importing only the current names reports no legacy imports', () => {
  const response = load(
    `import { Composition } from '@celesta.studio/react';\n` +
      `export default () => <Composition width={10} height={10} fps={1} durationInFrames={1} />;\n`,
  );
  assert.equal(response.error, undefined);
  assert.equal(response.legacyImports, undefined);
});
