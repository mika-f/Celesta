// Run with: node packages/web/test/text-runs.mjs [path-to-chromium]
import { createRequire } from 'node:module';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
const require = createRequire(new URL('../../react/package.json', import.meta.url));
const { build } = require('esbuild');
const directory = mkdtempSync(join(tmpdir(), 'celesta-web-text-'));
try {
  const module = fileURLToPath(new URL('../src/scene-canvas.ts', import.meta.url));
  const { outputFiles } = await build({ stdin: { contents: `
    import { SceneCanvas } from ${JSON.stringify(module)};
    const check = (value, message) => { if (!value) throw new Error(message); };
    const canvas = document.createElement('canvas'); canvas.width = 900; canvas.height = 240;
    document.body.append(canvas);
    const ctx = canvas.getContext('2d');
    const renderer = new SceneCanvas();
    const raster = async (text, style, maxWidth) => {
      ctx.reset();
      await renderer.layer(ctx, { id: 'text', opacity: 1, transform: {
        position: { x: 10, y: 70 }, scale: { x: 1, y: 1 }, rotation: 0, anchor: { x: 0, y: 0 } },
        content: { type: 'text', text, style, maxWidth, baselineAnchor: true } }, new DOMMatrix(), 1);
      return ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    };
    (async () => {
      for (const text of ['AV office é', 'אבג AV é', 'AV\\noffice é', '😀 é AV']) {
        const style = { fontSize: 40, lineHeight: 54, fontFamily: 'serif' };
        const plain = await raster(text, style);
        const colorRuns = Array.from(text).map((_, start) => ({ start, end: start + 1, color: start % 2 ? '#00ff00' : '#ff0000' }));
        const colored = await raster(text, { ...style, colorRuns });
        const broad = await raster(text, { ...style, colorRuns: [{ start: 0, end: Array.from(text).length, color: '#fff' }] });

        let differing = 0, occupied = 0;
        for (let i = 3; i < plain.length; i += 4) { differing += Math.abs(broad[i] - colored[i]) > 8 ? 1 : 0; occupied += broad[i] > 0 ? 1 : 0; }
        check(differing / occupied < 0.01, text + ': color changes altered glyph coverage ' + differing + '/' + occupied );
        const hidden = await raster(text, { ...style, colorRuns, visibleCharacters: 0 });
        check(hidden.every(n => n === 0), 'zero reveal draws pixels');
        const full = await raster(text, { ...style, colorRuns, visibleCharacters: Array.from(text).length });
        check(full.every((n, i) => n === colored[i]), 'full reveal differs');
      }
      const partial = await raster('éX', { fontSize: 40, lineHeight: 54, visibleCharacters: 1 });
      const cluster = await raster('éX', { fontSize: 40, lineHeight: 54, visibleCharacters: 2 });
      check(partial.every((n, i) => n === cluster[i]), 'combining cluster must reveal atomically');
      for (const style of [
        { fontSize: 40, lineHeight: 54, stroke: { width: 2, paint: { type: 'solid', color: '#fff' } } },
        { fontSize: 40, lineHeight: 54, fill: { type: 'linear', start: { x: 0, y: 0 }, end: { x: 200, y: 0 }, stops: [{ offset: 0, color: '#f00' }, { offset: 1, color: '#00f' }] } },
      ]) {
        const regular = await raster('AV é', { ...style, visibleCharacters: 5 });
        const colored = await raster('AV é', { ...style, colorRuns: [{ start: 0, end: 5, color: '#00ff00' }] });
        let max = 0, differences = 0;
        for (let i = 3; i < regular.length; i += 4) { const diff = Math.abs(regular[i] - colored[i]); max = Math.max(max, diff); differences += diff > 1 ? 1 : 0; }
        check(max <= 1, 'stroke or gradient changed coverage ' + JSON.stringify(style) + ' max=' + max + ' pixels=' + differences);
      }
      const wrapped = await raster('AV   AV', { fontSize: 40, lineHeight: 54, colorRuns: [{ start: 5, end: 7, color: '#ff0000' }] }, 80);
      check(wrapped.some((n, i) => i % 4 === 0 && n === 255 && wrapped[i + 1] === 0 && wrapped[i + 2] === 0), 'wrapped source color ranges were lost');
      document.body.append(Object.assign(document.createElement('pre'), { textContent: 'PASS' }));
    })().catch(error => document.body.append(Object.assign(document.createElement('pre'), { textContent: 'FAIL: ' + error.stack })));
  `, loader: 'js', resolveDir: directory }, bundle: true, write: false, format: 'iife' });
  writeFileSync(join(directory, 'test.html'), '<!doctype html><body><script>' + outputFiles[0].text + '</script>');
  const chrome = process.argv[2] ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  const result = spawnSync(chrome, ['--headless', '--no-sandbox', '--disable-gpu', '--user-data-dir=' + join(directory, 'profile'), '--dump-dom', '--virtual-time-budget=10000', new URL('file://' + join(directory, 'test.html')).href], { encoding: 'utf8', timeout: 30000, maxBuffer: 8 * 1024 * 1024 });
  const report = result.stdout?.match(/<pre>([\s\S]*?)<\/pre>/)?.[1];
  assert.equal(report, 'PASS', report ?? result.stderr);
  console.log('PASS: browser color runs, full shaping, multiline and grapheme reveals');
} finally { rmSync(directory, { recursive: true, force: true }); }
