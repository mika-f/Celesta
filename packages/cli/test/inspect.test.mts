// skills/celesta/scripts/inspect.mjs --no-native cannot shape text: an entry
// that measures and does not catch the refusal is reported as an inspection the
// inspector does not support (exit code 2), not as a broken scene. The native
// mode is covered by crates/exporter/tests/integration/inspect.rs. Run after
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

test('an entry that calls Math.random() or Date.now() while rendering gets a nondeterministic_render warning', () => {
  withEntry('celesta-inspect-determinism-', ({ write, inspect }) => {
    write(`
      import { Composition, Text, useCurrentFrame } from '@celesta.studio/react';
      function Value() {
        const frame = useCurrentFrame();
        return <Text>{String(frame > 10 ? Date.now() : Math.random())}</Text>;
      }
      export default () => <Composition width={64} height={64} fps={30} durationInFrames={30}><Value /></Composition>;
    `);
    const text = inspect('--frames', '0,20');
    assert.equal(text.status, 0, text.stderr);
    assert.match(
      text.stdout,
      /^warning: nondeterministic_render: the entry calls Math\.random\(\) and Date\.now\(\) while rendering;/m,
    );
    const json = inspect('--frames', '0,20', '--json');
    assert.equal(json.status, 0, json.stderr);
    const report = reportOf(json.stdout) as InspectReport & { warnings: string[] };
    assert.equal(report.warnings.length, 1);
    assert.match(report.warnings[0], /^nondeterministic_render: the entry calls Math\.random\(\) and Date\.now\(\)/);
    // Frame 20 reads the frozen clock: two thirds of a second, in whole milliseconds.
    const [, later] = report.frames;
    assert.equal(contentOf(later.scene.layers[0], 'text').text, '666');
  });
});

test('an entry that reads no clock gets no warning', () => {
  withEntry('celesta-inspect-determinism-', ({ write, inspect }) => {
    write(`
      import { Composition, Text, useCurrentFrame } from '@celesta.studio/react';
      const Value = () => <Text>{String(useCurrentFrame())}</Text>;
      export default () => <Composition width={64} height={64} fps={30} durationInFrames={30}><Value /></Composition>;
    `);
    const json = inspect('--json');
    assert.equal(json.status, 0, json.stderr);
    assert.deepEqual((reportOf(json.stdout) as InspectReport & { warnings: string[] }).warnings, []);
  });
});

test('rough scenes are listed with their desc and warned about', () => {
  withEntry('celesta-inspect-rough-', ({ write, inspect }) => {
    write(`
      import { Composition, Scenes, defineScenes } from '@celesta.studio/react';
      const SCENES = defineScenes([
        { id: 'open', name: 'Open', durationInFrames: 30, desc: 'The title fades in.', Scene: () => null },
        { id: 'card', name: 'The card', durationInFrames: 30, status: 'rough', desc: 'A card slides in.' },
      ]);
      export default function Root() {
        return <Composition width={1920} height={1080} fps={30} durationInFrames={SCENES.durationInFrames} chapters={SCENES}><Scenes scenes={SCENES} /></Composition>;
      }
    `);
    const result = inspect('--frames', '45');
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /chapters \(1 of 2 rough\):\n {2}Open \(open\) \[0, 30\): The title fades in\.\n {2}The card \(card\) \[30, 60\) rough: A card slides in\.\n/);
    assert.match(result.stdout, /text "The card"/);
    assert.match(
      result.stdout,
      /warning: 1 scene is still rough and draws a placeholder card: The card \[30, 60\); export warns about them \(rough_scenes\)/,
    );
  });
});

/** What `inspect.mjs --json` reports as quality issues. */
interface QualityIssue {
  code: string;
  message: string;
  measured: 'layers' | 'pixels';
  frames?: { from: number; to: number; start: number; end: number }[];
  layer?: string;
  value?: number;
  limit?: number;
}

const lowContrast = `
  import { Composition, Rect, Text, useCurrentFrame, interpolate } from '@celesta.studio/react';
  const chapters = [
    { id: 'intro', from: 0, durationInFrames: 10 },
    { id: 'body', from: 10, durationInFrames: 50 },
  ];
  function Fading() {
    // Fades in over frames 0–20, then holds: measured only once it holds.
    const opacity = interpolate(useCurrentFrame(), [0, 20], [0, 1], { extrapolateRight: 'clamp' });
    return <Text id="fading" x={160} y={150} opacity={opacity} style={{ fontSize: 8, fill: { type: 'solid', color: '#FFFFFF' } }}>Hi</Text>;
  }
  export default function Root() {
    return <Composition width={320} height={180} fps={30} durationInFrames={60} chapters={chapters}>
      <Rect width={320} height={180} fill="#0F2A3D" />
      <Text id="title" x={160} y={90} style={{ fontSize: 8, fill: { type: 'solid', color: '#1F4A6D' } }}>Hello</Text>
      <Text id="readable" x={160} y={40} style={{ fontSize: 8, fill: { type: 'solid', color: '#EAF4F2' } }}>Readable</Text>
      <Fading />
    </Composition>;
  }
`;

test('the layer checks report low-contrast text and short scenes with their frames', () => {
  withEntry('celesta-inspect-quality-', ({ write, inspect }) => {
    write(lowContrast);
    const json = inspect('--every', '1', '--json');
    assert.equal(json.status, 0, json.stderr);
    const { quality } = JSON.parse(json.stdout) as { quality: QualityIssue[] };
    assert.deepEqual(quality.map((issue) => issue.code), ['low_contrast_text', 'short_scene']);
    const [contrast, short] = quality as [QualityIssue, QualityIssue];
    assert.equal(contrast.layer, 'title');
    assert.equal(contrast.measured, 'layers');
    assert.equal(contrast.limit, 3);
    assert.deepEqual(contrast.frames, [{ from: 0, to: 60, start: 0, end: 2 }]);
    assert.match(contrast.message, /^text layer "title" \("Hello"\) is #1F4A6D on #0F2A3D behind it: 1\.\d\d:1, below 3:1, at frames 0–59/);
    assert.equal(short.value, 0.333);
    assert.match(short.message, /^chapter "intro" lasts 0\.33 s \(frames 0–9 \(0\.00–0\.33 s\)\), under 1 s/);

    // Frames asked for in any order make sorted spans.
    const reversed = JSON.parse(inspect('--frames', '30,0', '--json').stdout) as { quality: QualityIssue[] };
    assert.deepEqual(
      reversed.quality[0]!.frames!.map(({ from, to }) => [from, to]),
      [[0, 1], [30, 31]],
    );

    const text = inspect('--frames', '0,30');
    assert.equal(text.status, 0, text.stderr);
    assert.match(text.stdout, /warning: low_contrast_text: text layer "title"/);
    assert.match(text.stdout, /warning: short_scene: chapter "intro"/);
  });
});

test('a first frame of nothing but solid rectangles is blank', () => {
  withEntry('celesta-inspect-blank-', ({ write, inspect }) => {
    write(`
      import { Composition, Rect, Text, useCurrentFrame } from '@celesta.studio/react';
      export default function Root() {
        const frame = useCurrentFrame();
        return <Composition width={320} height={180} fps={30} durationInFrames={60}>
          <Rect width={320} height={180} fill="#101018" />
          <Text x={160} y={90} opacity={frame === 0 ? 0 : 1} style={{ fontSize: 24 }}>Title</Text>
        </Composition>;
      }
    `);
    const { quality } = JSON.parse(inspect('--json').stdout) as { quality: QualityIssue[] };
    assert.deepEqual(quality.map((issue) => [issue.code, issue.frames?.[0]?.from]), [['blank_first_frame', 0]]);
    assert.match(quality[0]!.message, /^frame 0 shows only solid rectangles/);
    // Frame 0 not inspected: nothing to say about it.
    const later = JSON.parse(inspect('--frames', '1,2', '--json').stdout) as { quality: QualityIssue[] };
    assert.deepEqual(later.quality, []);
  });
});

test('a backdrop its group clips away is not behind the text', () => {
  withEntry('celesta-inspect-clip-', ({ write, inspect }) => {
    write(`
      import { Composition, Group, Rect, Text } from '@celesta.studio/react';
      export default function Root() {
        return <Composition width={320} height={180} fps={30} durationInFrames={30}>
          <Rect width={320} height={180} fill="#000000" />
          <Group clip={{ x: 0, y: 0, width: 160, height: 180 }}>
            <Rect width={320} height={180} fill="#FFFFFF" />
          </Group>
          <Text id="right" x={240} y={90} style={{ fontSize: 24, fill: { type: 'solid', color: '#FFFFFF' } }}>Right</Text>
          <Text id="left" x={80} y={90} style={{ fontSize: 24, fill: { type: 'solid', color: '#FFFFFF' } }}>Left</Text>
        </Composition>;
      }
    `);
    const json = inspect('--json');
    assert.equal(json.status, 0, json.stderr);
    const { quality } = JSON.parse(json.stdout) as { quality: QualityIssue[] };
    assert.deepEqual(quality.map((issue) => [issue.code, issue.layer]), [['low_contrast_text', 'left']]);
  });
});
