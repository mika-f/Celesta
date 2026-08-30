// The bridge CLI (src/cli.ts) speaks a JSON-lines protocol over stdout that
// the Rust side (crates/react-bridge) parses one line at a time. A stray
// `console.log` in a composition must not land on that stream — it is routed
// to stderr instead, where the bridge inherits it to the user's terminal.
// Run after `pnpm run build`:  node --test test/

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const cli = fileURLToPath(new URL('../bin/mikan-react-render.js', import.meta.url));

test('console.log in a composition goes to stderr, leaving stdout pure protocol', () => {
  const dir = mkdtempSync(join(tmpdir(), 'mikan-console-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition, Text } from '@mikan/react';\n` +
      `console.log('module-scope diagnostic', { a: 1 });\n` +
      `export default function Root() {\n` +
      `  console.log('per-render diagnostic');\n` +
      `  return (\n` +
      `    <Composition width={320} height={240} fps={30} durationInFrames={10}>\n` +
      `      <Text x={0} y={0}>Hi</Text>\n` +
      `    </Composition>\n` +
      `  );\n` +
      `}\n`,
  );

  try {
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":1}}\n',
      encoding: 'utf8',
    });

    assert.equal(result.status, 0, result.stderr);

    // Every stdout line is a JSON protocol message, nothing else.
    const lines = result.stdout.split(/\r?\n/).filter(Boolean);
    assert.ok(lines.length >= 2);
    for (const line of lines) {
      assert.doesNotThrow(() => JSON.parse(line), `stdout line is not JSON: ${line}`);
    }
    assert.doesNotMatch(result.stdout, /diagnostic/);

    // The composition's logs surfaced on stderr instead.
    assert.match(result.stderr, /module-scope diagnostic/);
    assert.match(result.stderr, /per-render diagnostic/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('composition is loaded as ESM with its original import.meta paths', () => {
  const dir = mkdtempSync(join(tmpdir(), 'mikan-esm-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(
    entry,
    `import { Composition } from '@mikan/react';\n` +
      `await Promise.resolve();\n` +
      `if (import.meta.dirname !== ${JSON.stringify(dir)}) throw new Error('unexpected import.meta.dirname');\n` +
      `if (import.meta.filename !== ${JSON.stringify(entry)}) throw new Error('unexpected import.meta.filename');\n` +
      `export default function Root() {\n` +
      `  return <Composition width={320} height={240} fps={30} durationInFrames={10} />;\n` +
      `}\n`,
  );

  try {
    const result = spawnSync(process.execPath, [cli, entry], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    assert.ok(JSON.parse(result.stdout).config);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
