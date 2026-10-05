import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { test } from 'vitest';

const inspector = fileURLToPath(new URL('../../../skills/celesta/scripts/inspect.mjs', import.meta.url));
const runtime = fileURLToPath(new URL('../dist/cli.js', import.meta.url));

test('Node-only inspection distinguishes unsupported metrics, fallbacks and scene errors', { timeout: 30000 }, () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-inspect-'));
  const entry = join(dir, 'entry.tsx');
  const inspect = (...args) => spawnSync(process.execPath, [inspector, entry, '--runtime', runtime, ...args], { encoding: 'utf8', timeout: 15000 });
  try {
    const source = `
      import { Composition, Text, useTextMetrics, useCurrentFrame } from '@celesta/react';
      function Title() {
        if (useCurrentFrame() === 1) throw new Error('broken scene');
        const m = useTextMetrics('Celesta', { fontSize: 96 });
        return <Text x={(1920 - m.width) / 2} y={400} style={{ fontSize: 96 }}>Celesta</Text>;
      }
      export default function Root() {
        return <Composition width={1920} height={1080} fps={30} durationInFrames={30}><Title /></Composition>;
      }
    `;
    writeFileSync(entry, source);
    const text = inspect('--frames', '0');
    assert.equal(text.status, 2, text.stderr);
    assert.match(text.stdout + text.stderr, /UNSUPPORTED inspection:/);
    assert.doesNotMatch(text.stdout, /ERROR:|OK:/);
    const startup = inspect('--json');
    assert.equal(startup.status, 2, startup.stderr);
    assert.equal(JSON.parse(startup.stdout).status, 'unsupported');
    // Mount evaluates frame 0 before the ready handshake; defer measurement
    // to exercise unsupported responses after a successful handshake as well.
    writeFileSync(entry, source.replace("const m = useTextMetrics", "if (useCurrentFrame() === 0) return <Text>Celesta</Text>; const m = useTextMetrics"));
    const json = inspect('--frames', '2,-1', '--json');
    assert.equal(json.status, 2, json.stderr);
    assert.deepEqual(JSON.parse(json.stdout).frames.map(({ frame, status }) => [frame, status]), [[2, 'unsupported'], [29, 'unsupported']]);
    const mixed = inspect('--frames', '2,1', '--json');
    assert.equal(mixed.status, 1, mixed.stderr);
    assert.match(JSON.parse(mixed.stdout).frames[1].error, /broken scene/);
    assert.equal(JSON.parse(mixed.stdout).frames[1].status, undefined);

    writeFileSync(entry, `
      import { Composition, Text, measureText } from '@celesta/react';
      export async function prepare() { await measureText('Celesta'); }
      export default function Root() {
        return <Composition width={400} height={200} fps={30} durationInFrames={1}><Text>Celesta</Text></Composition>;
      }
    `);
    const prepare = inspect('--json');
    assert.equal(prepare.status, 2, prepare.stderr);
    assert.equal(JSON.parse(prepare.stdout).status, 'unsupported');
    writeFileSync(entry, `
      import { Composition, Text, measureText } from '@celesta/react';
      export async function prepare() { try { await measureText('Celesta'); } catch {} }
      export default function Root() {
        return <Composition width={400} height={200} fps={30} durationInFrames={1}><Text>Celesta</Text></Composition>;
      }
    `);
    const fallback = inspect('--json');
    assert.equal(fallback.status, 0, fallback.stderr);
    assert.equal(JSON.parse(fallback.stdout).frames[0].scene.layers[0].content.text, 'Celesta');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('native startup without a handshake explains exporter compatibility and PNG fallback', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-legacy-inspect-'));
  try {
    const entry = join(dir, 'entry.tsx');
    writeFileSync(entry, '');
    // Node rejects the exporter arguments, like an older native exporter,
    // exiting without emitting a protocol handshake or structured error.
    const result = spawnSync(process.execPath, [inspector, entry, '--native', process.execPath], {
      cwd: dir, encoding: 'utf8', timeout: 15000,
    });
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, /supports --inspect/);
    assert.match(result.stderr, /PNG frames \/ a contact sheet/);
    assert.doesNotMatch(result.stderr, /the Celesta runtime exited unexpectedly/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('timeout still exits cleanly when the native process group is already gone', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-inspect-timeout-'));
  try {
    const entry = join(dir, 'entry.tsx');
    const preload = join(dir, 'preload.mjs');
    writeFileSync(entry, '');
    writeFileSync(preload, `
      import childProcess from 'node:child_process';
      import { EventEmitter } from 'node:events';
      import { PassThrough } from 'node:stream';
      import { syncBuiltinESMExports } from 'node:module';
      Object.defineProperty(process, 'platform', { value: 'linux' });
      process.kill = () => { throw Object.assign(new Error('process group already gone'), { code: 'ESRCH' }); };
      childProcess.spawn = () => Object.assign(new EventEmitter(), { pid: 12345, stdin: new PassThrough(), stdout: new PassThrough() });
      syncBuiltinESMExports();
    `);
    const result = spawnSync(process.execPath, ['--import', pathToFileURL(preload).href, inspector, entry, '--native', 'gone-exporter', '--timeout', '0.01'], {
      encoding: 'utf8', timeout: 15000,
    });
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, /timed out after 0.01s/);
    assert.doesNotMatch(result.stderr, /Error:|ESRCH|at process/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('startup failures in prepare() or the entry source report an error and exit 1', { timeout: 30000 }, () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-inspect-startup-'));
  const entry = join(dir, 'entry.tsx');
  const inspect = (...args) => spawnSync(process.execPath, [inspector, entry, '--runtime', runtime, ...args], { encoding: 'utf8', timeout: 15000 });
  const body = `export default function Root() { return <Composition width={400} height={200} fps={30} durationInFrames={1} />; }`;
  try {
    for (const source of [
      `import { Composition } from '@celesta/react';\nexport async function prepare() { throw new Error('prepare exploded'); }\n${body}`,
      `import { Composition } from '@celesta/react';\nexport default function Root( {\n`,
    ]) {
      writeFileSync(entry, source);
      const text = inspect();
      assert.equal(text.status, 1, text.stderr);
      assert.match(text.stderr, /ERROR loading entry/);
      assert.doesNotMatch(text.stdout + text.stderr, /UNSUPPORTED/);
      const json = inspect('--json');
      assert.equal(json.status, 1, json.stderr);
      assert.ok(JSON.parse(json.stdout).error);
      assert.equal(JSON.parse(json.stdout).status, undefined);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
