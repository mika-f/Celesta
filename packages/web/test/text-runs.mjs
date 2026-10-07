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
    const raster = async (text, style, maxWidth, baselineAnchor = true) => {
      ctx.reset();
      await renderer.layer(ctx, { id: 'text', opacity: 1, transform: {
        position: { x: 10, y: 70 }, scale: { x: 1, y: 1 }, rotation: 0, anchor: { x: 0, y: 0 } },
        content: { type: 'text', text, style, maxWidth, baselineAnchor } }, new DOMMatrix(), 1);
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
      const compare = (a, b, label) => {
        let differing = 0, occupied = 0;
        for (let i = 0; i < a.length; i += 4) {
          if (a[i + 3] < 250 || b[i + 3] < 250) continue;
          occupied++;
          if ([0, 1, 2].some(c => Math.abs(a[i + c] - b[i + c]) > 3)) differing++;
        }
        check(occupied > 0 && differing / occupied < 0.01, label + ': changed opaque colors ' + differing + '/' + occupied);
      };
      const bounds = pixels => {
        let left = 900, top = 240, right = 0, bottom = 0;
        for (let i = 3; i < pixels.length; i += 4) {
          if (pixels[i] <= 8) continue;
          const x = ((i - 3) / 4) % 900, y = Math.floor((i - 3) / 4 / 900);
          left = Math.min(left, x); top = Math.min(top, y); right = Math.max(right, x); bottom = Math.max(bottom, y);
        }
        return [left, top, right, bottom];
      };
      const compact = { fontSize: 40, lineHeight: 8, fontFamily: 'serif' };
      const directBounds = bounds(await raster('jÁ\\njÁ', compact));
      const styledBounds = bounds(await raster('jÁ\\njÁ', { ...compact, visibleCharacters: 5 }));
      check(directBounds.every((n, i) => Math.abs(n - styledBounds[i]) <= 1), 'compact multiline ink was clipped ' + directBounds + '/' + styledBounds);
      const crBounds = bounds(await raster('AV\\rAV', compact, undefined, false));
      const lfBounds = bounds(await raster('AV\\nAV', compact, undefined, false));
      check(crBounds.every((n, i) => n === lfBounds[i]), 'CR geometry differs from LF ' + crBounds + '/' + lfBounds);
      const gradient = { type: 'linear', start: { x: 0, y: 0 }, end: { x: 100, y: 70 }, stops: [{ offset: 0, color: '#f00' }, { offset: 1, color: '#00f' }] };
      const stroked = { fontSize: 40, lineHeight: 54, fill: gradient, stroke: { width: 5, paint: gradient } };
      compare(await raster('AV é', stroked), await raster('AV é', { ...stroked, visibleCharacters: 5 }), 'padded gradient origin');
      const mono = { fontSize: 40, lineHeight: 54, fill: { type: 'solid', color: '#ff0000' } };
      compare(await raster('A\\uFE0F', mono), await raster('A\\uFE0F', { ...mono, colorRuns: [{ start: 0, end: 2, color: '#ff0000' }] }), 'monochrome variation glyph');
      // Inspect locale propagation at the platform boundary.
      const extent = SVGTextContentElement.prototype.getExtentOfChar;
      let extents = 0, lang;
      SVGTextContentElement.prototype.getExtentOfChar = function(index) {
        extents++; lang = this.getAttributeNS('http://www.w3.org/XML/1998/namespace', 'lang'); return extent.call(this, index);
      };
      await raster('Locale test', { fontSize: 39, visibleCharacters: 2 });
      check(lang === navigator.language, 'SVG locale differs from resolved Canvas locale');
      const transform = { position: { x: 0, y: 50 }, scale: { x: 1, y: 1 }, rotation: 0, anchor: { x: 0, y: 0 } };
      const scene = color => ({ width: 900, height: 240, layers: Array.from({ length: 80 }, (_, i) => ({
        id: String(i), opacity: 1, transform,
        content: { type: 'text', text: 'line ' + i, style: { fontSize: 24, colorRuns: [{ start: 0, end: 4, color }] } },
      })) });
      await renderer.draw(canvas, scene('#ff0000'));
      const shaped = extents;
      await renderer.draw(canvas, scene('#00ff00'));
      await renderer.draw(canvas, scene('#00ff00'));
      check(extents === shaped, 'large scenes re-shape cached lines');
      check(renderer.textLayouts.size === 80 && renderer.styledText.size === 80, 'active scene cache retention');
      await renderer.draw(canvas, { width: 900, height: 240, layers: [] });
      check(renderer.textLayouts.size <= 32 && renderer.styledText.size <= 64, 'idle caches were not bounded');
      SVGTextContentElement.prototype.getExtentOfChar = extent;
      renderer.dispose();
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
