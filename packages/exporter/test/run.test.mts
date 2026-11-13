import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

import { CelestaExportError, describeComponent, exportStills, exportVideo, getComposition, listComponents } from '../src/index.js';

const exporter = fileURLToPath(new URL('./fixtures/fake-exporter.mjs', import.meta.url));
const run = (mode: string) => ({ exporter, env: { ...process.env, FAKE_MODE: mode } });
// The fake exporter is a script with a shebang, which Windows cannot run directly.
const skip = process.platform === 'win32';

test('progress events reach the callback and the report resolves', { skip }, async () => {
  const events: string[] = [];
  const result = await exportStills({
    source: 'film.tsx',
    output: 'f.png',
    frames: [0],
    ...run('progress'),
    onProgress: (event) => events.push(event.event),
  });
  assert.deepEqual(events, ['composition', 'rendering', 'wrote']);
  assert.equal(result.composition.frames, 30);
  assert.deepEqual(result.outputs, [{ type: 'frame', path: '/work/f.png', frame: 0, time: 0 }]);
  // The report's status is not part of the result, nor its type.
  assert.equal((result as { status?: unknown }).status, undefined);
  // The fake echoes its arguments as warnings.
  assert.ok(result.warnings.includes('--json-progress'));
});

test('a video output carries its tracks', { skip }, async () => {
  const events: unknown[] = [];
  const result = await exportVideo({
    source: 'film.tsx',
    output: 'film.mp4',
    ...run('video'),
    onProgress: (event) => events.push(event),
  });
  const output = result.outputs[0]!;
  assert.equal(output.type, 'video');
  if (output.type !== 'video') return;
  assert.equal(output.muxer, 'ffmpeg/mp4');
  assert.equal(output.video.codec, 'avc1.64001f');
  assert.equal(output.audio, null);
  assert.deepEqual(output.files, ['/work/film.mp4']);
  assert.deepEqual((events[0] as { event: string; muxerName: string }).muxerName, 'MP4');
});

test('component listings resolve without a status', { skip }, async () => {
  const listed = await listComponents({ check: true, ...run('components') });
  assert.equal(listed[0]!.id, 'ffmpeg/mp4');
  assert.deepEqual(listed[0]!.args, ['--json', '--list-components', '--check']);
  const builtIn = listed.find((component) => component.origin === 'built-in');
  assert.equal(builtIn?.id, 'builtin/png');
  assert.equal(builtIn?.plugin, 'builtin');
  const described = await describeComponent('ffmpeg-x264/libx264', run('components'));
  assert.equal(described.kind, 'videoEncoder');
  assert.deepEqual(described.settings, []);
});

test('a failed export rejects with the report error', { skip }, async () => {
  await assert.rejects(exportVideo({ source: 'film.tsx', output: 'film.mp4', ...run('error') }), (error) => {
    assert.ok(error instanceof CelestaExportError);
    assert.equal(error.code, 'output_exists');
    assert.equal(error.hint, 'pass --overwrite to replace it, or write to another path');
    assert.equal(error.report!.status, 'error');
    return true;
  });
});

test('an exporter too old for the flags says so', { skip }, async () => {
  await assert.rejects(getComposition({ source: 'film.tsx', ...run('old') }), (error) => {
    assert.ok(error instanceof CelestaExportError);
    assert.equal(error.code, 'usage');
    assert.match(error.hint!, /older than @celesta\.studio\/exporter/);
    return true;
  });
});

test('an exporter that ends without a report keeps its stderr', { skip }, async () => {
  await assert.rejects(getComposition({ source: 'film.tsx', ...run('crash') }), (error) => {
    assert.ok(error instanceof CelestaExportError);
    assert.equal(error.code, 'exporter_failed');
    assert.match(error.stderr!, /boom/);
    return true;
  });
});

test('aborting asks the exporter to cancel on stdin', { skip }, async () => {
  const controller = new AbortController();
  const exporting = exportVideo({
    source: 'film.tsx',
    output: 'film.mp4',
    ...run('wait-cancel'),
    signal: controller.signal,
    onProgress: () => controller.abort(),
  });
  await assert.rejects(exporting, { name: 'CelestaExportError', code: 'cancelled' });
});

test('an already aborted signal does not start the exporter', async () => {
  await assert.rejects(
    exportVideo({ source: 'film.tsx', output: 'film.mp4', exporter: '/nonexistent', signal: AbortSignal.abort() }),
    { code: 'cancelled' },
  );
});

test('a throwing callback cancels the export and rejects with its error', { skip }, async () => {
  const failure = new Error('stop');
  await assert.rejects(
    exportVideo({ source: 'film.tsx', output: 'film.mp4', ...run('wait-cancel'), onProgress: () => { throw failure; } }),
    (error) => error === failure,
  );
});

test('a missing exporter is exporter_not_found', async () => {
  await assert.rejects(getComposition({ source: 'film.tsx', exporter: '/nonexistent/celesta-exporter' }), {
    code: 'exporter_not_found',
  });
});

test('aborting a listing kills the exporter at once', { skip }, async () => {
  const controller = new AbortController();
  const started = Date.now();
  const listing = listComponents({ ...run('hang'), signal: controller.signal });
  setTimeout(() => controller.abort(), 100);
  await assert.rejects(listing, { code: 'cancelled' });
  assert.ok(Date.now() - started < 5000);
  const describing = describeComponent('ffmpeg-x264/libx264', { ...run('hang'), signal: AbortSignal.timeout(100) });
  await assert.rejects(describing, { code: 'cancelled' });
});

test('a component error report rejects', { skip }, async () => {
  await assert.rejects(listComponents(run('component-error')), { code: 'component_unavailable' });
  await assert.rejects(describeComponent('ffmpeg/nope', run('component-error')), { code: 'component_unavailable' });
});
