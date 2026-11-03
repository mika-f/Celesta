import assert from 'node:assert/strict';
import { test } from 'vitest';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { isAbsolute, join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import React from 'react';

import { Composition, Font, Rect, useCurrentFrame, useTextMetrics } from '../src/index.js';
import { mount, createResolver } from '../src/render.js';
import { registerComponent } from '../src/registry.js';
import { setTextMeasurer } from '../src/text-measure.js';
import type { MeasureTextRequest } from '../src/text-measure.js';
import type { CompositionConfig, Layer, TextMetrics } from '../src/index.js';

import { contentOf } from './helpers.mjs';
import type { FrameResponse } from './helpers.mjs';

function metrics(width: number): TextMetrics {
  return { width, height: 24, ascent: 18, descent: 6, lineHeight: 24, capHeight: 0, xHeight: 0, lines: 1, glyphs: [] };
}

/** The width of a rect layer. */
const widthOf = (layer: Layer | undefined) => contentOf(layer, 'rect').width;

/** The path of the file the last request's first font is loaded from. */
function lastFontPath(requests: MeasureTextRequest[]): string {
  const location = requests.at(-1)?.fonts[0].location;
  assert.equal(location?.type, 'file');
  return location.path;
}

test('render metrics cache by value and refresh for text, style, wrapping and composition fonts', () => {
  const requests: MeasureTextRequest[] = [];
  setTextMeasurer(async () => metrics(0), (request) => {
    requests.push(request);
    return metrics(request.text.length * (request.fonts.length ? 20 : 10));
  });
  function Label() {
    const frame = useCurrentFrame();
    const text = frame < 2 ? '日本 AV' : '日本 AV 123';
    const m = useTextMetrics(text, { fontSize: frame < 3 ? 20 : 40, lang: frame < 6 ? 'ja' : 'zh-Hant' }, { maxWidth: frame < 4 ? 100 : 200 });
    return React.createElement(Rect, { width: m.width + 48, height: m.height + 24 });
  }
  const Root = () => {
    const frame = useCurrentFrame();
    return React.createElement(Composition, { width: 400, height: 200, fps: 30, durationInFrames: 7 },
      React.createElement(Label),
      React.createElement(Font, { src: frame < 5 ? './one.ttf' : './two.ttf' }));
  };
  const mounted = mount(Root);
  const render = (frame: number) => mounted.renderAt({ value: frame, timescale: 30 }, null).scene;
  assert.equal(widthOf(render(0).layers[0]), 5 * 20 + 48);
  const count = requests.length;
  render(1);
  assert.equal(requests.length, count);
  assert.equal(widthOf(render(2).layers[0]), 9 * 20 + 48);
  render(3);
  assert.equal(requests.at(-1)?.style.fontSize, 40);
  render(4);
  assert.equal(requests.at(-1)?.maxWidth, 200);
  render(5);
  assert.match(lastFontPath(requests), /two\.ttf$/);
  assert.ok(isAbsolute(lastFontPath(requests)));
  const beforeLanguageChange = requests.length;
  render(6);
  assert.equal(requests.length, beforeLanguageChange + 1);
  assert.equal(requests.at(-1)?.style.lang, 'zh-Hant');

  registerComponent('MetricsLabelTest', Label);
  const preview = createResolver().resolve([{ component: 'MetricsLabelTest', props: {} }], {
    width: 400, height: 200, fps: 30, durationInFrames: 6,
    time: { value: 5, timescale: 30 }, preview: true,
  }, mounted.fonts).components;
  assert.deepEqual(preview[0]?.[0].content, render(5).layers[0].content);
});


/** A line the CLI writes: a request to the bridge, its handshake, or a frame. */
type Message =
  | { measureText: MeasureTextRequest }
  | { probeMedia: { path: string } }
  | { config: CompositionConfig }
  | FrameResponse;

// Integration test of the build: runs packages/cli/bin, which loads the
// built dist/ of this package and the others, so it stays on built output.
test('CLI shares prepare and render RPCs, handles large UTF-8 replies, and forwards measurement errors', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-metrics-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `
    import { Composition, Rect, measureText, useCurrentFrame, useTextMetrics } from '@celesta.studio/react';
    import { preloadMedia } from '@celesta.studio/media-utils';
    export async function prepare() {
      const [m, media] = await Promise.all([measureText('prepare'), preloadMedia('voice.wav')]);
      if (m.width !== 7 || media.durationSeconds !== 1) throw new Error('incorrect prepare response');
    }
    function Label() {
      const frame = useCurrentFrame();
      const m = useTextMetrics('日本語 AV '.repeat(2000) + frame);
      return <Rect width={m.width} height={m.height} />;
    }
    export default function Root() {
      return <Composition width={400} height={200} fps={30} durationInFrames={3}><Label /></Composition>;
    }
  `);
  const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));
  const child = spawn(process.execPath, [cli, entry], { stdio: ['pipe', 'pipe', 'pipe'] });
  let stderr = '';
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  const exited = new Promise<number | null>((resolve, reject) => {
    child.on('error', reject);
    child.on('exit', (code) => resolve(code));
  });
  const measurements: MeasureTextRequest[] = [];
  const frames: FrameResponse[] = [];
  try {
    const lines = createInterface({ input: child.stdout });
    for await (const line of lines) {
      const message: Message = JSON.parse(line);
      const send = (value: unknown) => child.stdin.write(JSON.stringify(value) + '\n');
      if ('measureText' in message) {
        // The Rust bridge tells measurements from frames by this prefix
        // alone, so a frame it receives early is parsed only once.
        assert.ok(line.startsWith('{"measureText":'), line.slice(0, 40));
        const request = message.measureText;
        measurements.push(request);
        if (request.text.endsWith('2')) {
          send({ error: 'measurement failed for frame 2' });
        } else {
          // Far larger than a pipe/read chunk, including multibyte clusters.
          send({ metrics: { ...metrics(request.text.length), glyphs: Array.from(request.text, (text, x) => ({ text, x, width: 1, line: 0 })) } });
        }
      } else if ('probeMedia' in message) {
        send({ media: { durationSeconds: 1, audio: [] } });
      } else if ('config' in message) {
        send({ time: { value: 0, timescale: 30 } });
      } else {
        frames.push(message);
        if (frames.length < 3) send({ time: { value: frames.length, timescale: 30 } });
        else child.stdin.end();
      }
    }
    assert.equal(await exited, 0, stderr);
    assert.equal(measurements[0].text, 'prepare');
    assert.equal(widthOf(frames[0].scene.layers[0]), '日本語 AV '.repeat(2000).length + 1);
    assert.equal(widthOf(frames[1].scene.layers[0]), widthOf(frames[0].scene.layers[0]));
    assert.equal(frames[2].error, 'measurement failed for frame 2');
    assert.equal(measurements.filter((request) => !request.text.endsWith('2')).length, 3);
  } finally {
    child.kill();
    rmSync(dir, { recursive: true, force: true });
  }
});

test('text carets use shaped cluster advances and bidi direction without prefix measurement', async () => {
  const { textCaret } = await import('../src/index.js');
  const m = { ...metrics(80), glyphs: [
    { text: 'ffi', start: 0, end: 3, rtl: false, x: 10, width: 30, line: 0, inkAscent: 0, inkDescent: 0 },
    { text: 'א', start: 4, end: 5, rtl: true, x: 20, width: 12, line: 1, inkAscent: 0, inkDescent: 0 },
  ] };
  assert.deepEqual(textCaret(m, 1), { x: 20, y: 0, line: 0 });
  assert.deepEqual(textCaret(m, 3), { x: 40, y: 0, line: 0 });
  assert.deepEqual(textCaret(m, 4), { x: 32, y: 24, line: 1 });
  assert.deepEqual(textCaret(m, 5), { x: 20, y: 24, line: 1 });
});

test('text carets retain consecutive and trailing empty lines', async () => {
  const { textCaret } = await import('../src/index.js');
  const m = { ...metrics(12), lines: 5, height: 120, lineStarts: [0, 3, 5, 7, 8], glyphs: [
    { text: 'A', start: 0, end: 1, rtl: false, x: 0, width: 12, line: 0, inkAscent: 0, inkDescent: 0 },
    { text: 'B', start: 5, end: 6, rtl: false, x: 0, width: 12, line: 2, inkAscent: 0, inkDescent: 0 },
  ] };
  assert.deepEqual(textCaret(m, 1), { x: 12, y: 0, line: 0 });
  assert.deepEqual(textCaret(m, 3), { x: 0, y: 24, line: 1 });
  assert.deepEqual(textCaret(m, 7), { x: 0, y: 72, line: 3 });
  assert.deepEqual(textCaret(m, 8), { x: 0, y: 96, line: 4 });
});


test('carets stop searching after finding the requested cluster without copying glyphs', async () => {
  const { textCaret } = await import('../src/index.js');
  const glyphs = [{ text: 'A', start: 0, end: 1, rtl: false, x: 0, width: 12, line: 0, inkAscent: 0, inkDescent: 0 }];
  Object.defineProperty(glyphs, 1, { get() { throw new Error('searched past the matching cluster'); } });
  assert.deepEqual(textCaret({ ...metrics(12), lineStarts: [0], glyphs }, 0), { x: 0, y: 0, line: 0 });
});
