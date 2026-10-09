import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));

function load(source) {
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
    `import { Composition, Circle } from '@celesta/react';\n` +
      `export default () => <Composition width={10} height={10} fps={1} durationInFrames={1}><Circle radius={2} /></Composition>;\n`,
  );
  assert.equal(response.error, '@celesta/react does not export Circle; import it from @celesta/shapes');
});

test('an unknown Celesta package fails the build', () => {
  const response = load(
    `import { Composition } from '@celesta/react';\nimport { Nope } from '@celesta/nope';\n` +
      `export default () => <Composition width={10} height={10} fps={1} durationInFrames={1}><Nope /></Composition>;\n`,
  );
  assert.match(response.error, /@celesta\/nope is not a Celesta package/);
});
