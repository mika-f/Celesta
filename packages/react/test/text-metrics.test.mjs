import assert from 'node:assert/strict';
import { test } from 'vitest';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { isAbsolute, join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import React from 'react';

import { Composition, Font, Rect, useCurrentFrame, useTextMetrics } from '../dist/index.js';
import { mount, createResolver } from '../dist/render.js';
import { registerComponent } from '../dist/registry.js';
import { setTextMeasurer } from '../dist/text-measure.js';

function metrics(width) {
  return { width, height: 24, ascent: 18, descent: 6, lineHeight: 24, lines: 1, glyphs: [] };
}

test('render metrics cache by value and refresh for text, style, wrapping and composition fonts', () => {
  const requests = [];
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
  const render = (frame) => mounted.renderAt({ value: frame, timescale: 30 }, null).scene;
  assert.equal(render(0).layers[0].content.width, 5 * 20 + 48);
  const count = requests.length;
  render(1);
  assert.equal(requests.length, count);
  assert.equal(render(2).layers[0].content.width, 9 * 20 + 48);
  render(3);
  assert.equal(requests.at(-1).style.fontSize, 40);
  render(4);
  assert.equal(requests.at(-1).maxWidth, 200);
  render(5);
  assert.match(requests.at(-1).fonts[0].location.path, /two\.ttf$/);
  assert.ok(isAbsolute(requests.at(-1).fonts[0].location.path));
  const beforeLanguageChange = requests.length;
  render(6);
  assert.equal(requests.length, beforeLanguageChange + 1);
  assert.equal(requests.at(-1).style.lang, 'zh-Hant');

  registerComponent('MetricsLabelTest', Label);
  const preview = createResolver().resolve([{ component: 'MetricsLabelTest', props: {} }], {
    width: 400, height: 200, fps: 30, durationInFrames: 6,
    time: { value: 5, timescale: 30 }, preview: true,
  }, mounted.fonts);
  assert.deepEqual(preview[0][0].content, render(5).layers[0].content);
});


test('CLI shares prepare and render RPCs, handles large UTF-8 replies, and forwards measurement errors', { timeout: 15000 }, async () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-metrics-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `
    import { Composition, Rect, measureText, preloadMedia, useCurrentFrame, useTextMetrics } from '@celesta/react';
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
  const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));
  const child = spawn(process.execPath, [cli, entry], { stdio: ['pipe', 'pipe', 'pipe'] });
  let stderr = '';
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  const exited = new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('exit', (code) => resolve(code));
  });
  const measurements = [];
  const frames = [];
  try {
    const lines = createInterface({ input: child.stdout });
    for await (const line of lines) {
      const message = JSON.parse(line);
      const send = (value) => child.stdin.write(JSON.stringify(value) + '\n');
      if (message.measureText) {
        const request = message.measureText;
        measurements.push(request);
        if (request.text.endsWith('2')) {
          send({ error: 'measurement failed for frame 2' });
        } else {
          // Far larger than a pipe/read chunk, including multibyte clusters.
          send({ metrics: { ...metrics(request.text.length), glyphs: Array.from(request.text, (text, x) => ({ text, x, width: 1, line: 0 })) } });
        }
      } else if (message.probeMedia) {
        send({ media: { durationSeconds: 1, audio: [] } });
      } else if (message.config) {
        send({ time: { value: 0, timescale: 30 } });
      } else {
        frames.push(message);
        if (frames.length < 3) send({ time: { value: frames.length, timescale: 30 } });
        else child.stdin.end();
      }
    }
    assert.equal(await exited, 0, stderr);
    assert.equal(measurements[0].text, 'prepare');
    assert.equal(frames[0].scene.layers[0].content.width, '日本語 AV '.repeat(2000).length + 1);
    assert.equal(frames[1].scene.layers[0].content.width, frames[0].scene.layers[0].content.width);
    assert.equal(frames[2].error, 'measurement failed for frame 2');
    assert.equal(measurements.filter((request) => !request.text.endsWith('2')).length, 3);
  } finally {
    child.kill();
    rmSync(dir, { recursive: true, force: true });
  }
});
