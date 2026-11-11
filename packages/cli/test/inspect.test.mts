// skills/celesta/scripts/inspect.mjs --no-native cannot shape text: an entry
// that measures and does not catch the refusal is reported as an inspection the
// inspector does not support (exit code 2), not as a broken scene. The native
// mode is covered by crates/exporter/tests/runtime/inspect.rs. Run after
// `pnpm run build:runtime` (repository root): the tests exercise the built CLI
// (inspect.mjs runs dist/cli.js).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import type { SpawnSyncReturns } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { test } from 'vitest';

import type { FrameMessage, ReadyMessage } from '../src/cli.js';
import { contentOf } from './layer-content.mjs';

const inspector = fileURLToPath(new URL('../../../skills/celesta/scripts/inspect.mjs', import.meta.url));
const runtime = fileURLToPath(new URL('../dist/cli.js', import.meta.url));

/** What `inspect.mjs --json` prints: every frame asked for, or the error that stopped it. */
interface InspectReport {
  ready: ReadyMessage;
  frames: (FrameMessage & { frame: number; error?: string; status?: 'unsupported' })[];
  error?: string;
  status?: 'unsupported';
}

/** The report inspect.mjs --json prints. */
const reportOf = (stdout: string): InspectReport => JSON.parse(stdout);

interface EntryContext {
  dir: string;
  entry: string;
  write: (source: string) => void;
  inspect: (...args: string[]) => SpawnSyncReturns<string>;
}

function withEntry(prefix: string, body: (context: EntryContext) => void) {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  const entry = join(dir, 'entry.tsx');
  const inspect = (...args: string[]) =>
    spawnSync(process.execPath, [inspector, entry, '--no-native', '--runtime', runtime, ...args], { encoding: 'utf8', timeout: 15000 });
  try {
    body({ dir, entry, write: (source) => writeFileSync(entry, source), inspect });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const measuring = `
  import { Composition, Text, useTextMetrics, useCurrentFrame } from '@celesta.studio/react';
  function Title() {
    const frame = useCurrentFrame();
    if (frame === 1) throw new Error('broken scene');
    if (frame === 0) return <Text>Celesta</Text>;
    const m = useTextMetrics('Celesta', { fontSize: 96 });
    return <Text x={(1920 - m.width) / 2} y={400} style={{ fontSize: 96 }}>Celesta</Text>;
  }
  export default function Root() {
    return <Composition width={1920} height={1080} fps={30} durationInFrames={30}><Title /></Composition>;
  }
`;

test('measuring frames are unsupported, not errors, and scene errors still win', () => {
  withEntry('celesta-inspect-', ({ write, inspect }) => {
    write(measuring);
    const text = inspect('--frames', '0,2');
    assert.equal(text.status, 2, text.stderr);
    assert.match(text.stdout, /UNSUPPORTED inspection: inspect\.mjs cannot shape text.*--native/);
    assert.match(text.stdout, /1 frame\(s\) measure text and could not be inspected/);
    assert.doesNotMatch(text.stdout, /ERROR:/);

    const json = inspect('--frames', '2,-1', '--json');
    assert.equal(json.status, 2, json.stderr);
    assert.deepEqual(
      reportOf(json.stdout).frames.map(({ frame, status }) => [frame, status]),
      [[2, 'unsupported'], [29, 'unsupported']],
    );

    const mixed = inspect('--frames', '2,1', '--json');
    assert.equal(mixed.status, 1, mixed.stderr);
    const broken = reportOf(mixed.stdout).frames[1];
    assert.match(broken.error ?? '', /broken scene/);
    assert.equal(broken.status, undefined);
  });
});

test('measuring before the handshake stops inspection as unsupported', () => {
  withEntry('celesta-inspect-startup-', ({ write, inspect }) => {
    // Frame 0 renders while the entry mounts, before the handshake.
    write(measuring.replace('if (frame === 0) return <Text>Celesta</Text>;', ''));
    const text = inspect();
    assert.equal(text.status, 2, text.stderr);
    assert.match(text.stderr, /^UNSUPPORTED inspection: /m);

    write(`
      import { Composition, Text, measureText } from '@celesta.studio/react';
      export async function prepare() { await measureText('Celesta'); }
      export default function Root() {
        return <Composition width={400} height={200} fps={30} durationInFrames={1}><Text>Celesta</Text></Composition>;
      }
    `);
    const json = inspect('--json');
    assert.equal(json.status, 2, json.stderr);
    assert.equal(reportOf(json.stdout).status, 'unsupported');

    // An entry that catches the refusal falls back and inspects normally.
    write(`
      import { Composition, Text, measureText } from '@celesta.studio/react';
      export async function prepare() { try { await measureText('Celesta'); } catch {} }
      export default function Root() {
        return <Composition width={400} height={200} fps={30} durationInFrames={1}><Text>Celesta</Text></Composition>;
      }
    `);
    const fallback = inspect('--json');
    assert.equal(fallback.status, 0, fallback.stderr);
    const report: InspectReport = JSON.parse(fallback.stdout);
    assert.equal(contentOf(report.frames[0].scene.layers[0], 'text').text, 'Celesta');
  });
});

test('other startup failures are errors', () => {
  withEntry('celesta-inspect-error-', ({ write, inspect }) => {
    write(`
      import { Composition } from '@celesta.studio/react';
      export async function prepare() { throw new Error('prepare exploded'); }
      export default function Root() { return <Composition width={400} height={200} fps={30} durationInFrames={1} />; }
    `);
    const text = inspect();
    assert.equal(text.status, 1, text.stderr);
    assert.match(text.stderr, /ERROR loading entry: .*prepare exploded/);
    const json = inspect('--json');
    assert.equal(json.status, 1, json.stderr);
    assert.match(reportOf(json.stdout).error ?? '', /prepare exploded/);
    assert.equal(reportOf(json.stdout).status, undefined);
  });
});

test('an exporter without --inspect points to updating or PNG checks', () => {
  withEntry('celesta-inspect-legacy-', ({ dir, write }) => {
    write('');
    // Node rejects the exporter arguments and exits without a handshake,
    // like an exporter from before --inspect.
    const result = spawnSync(process.execPath, [inspector, join(dir, 'entry.tsx'), '--native', process.execPath], {
      cwd: dir, encoding: 'utf8', timeout: 15000,
    });
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, /exited before the inspection handshake/);
    assert.match(result.stderr, /contact-sheet/);
  });
});

test('a timeout exits cleanly when the native process group is already gone', () => {
  withEntry('celesta-inspect-timeout-', ({ dir, entry, write }) => {
    write('');
    const preload = join(dir, 'preload.mjs');
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
    const result = spawnSync(
      process.execPath,
      ['--import', pathToFileURL(preload).href, inspector, entry, '--native', 'gone-exporter', '--timeout', '0.01'],
      { encoding: 'utf8', timeout: 15000 },
    );
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, /timed out after 0.01s/);
    assert.doesNotMatch(result.stderr, /ESRCH|at process/);
  });
});
