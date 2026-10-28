import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';
import type { ReactElement } from 'react';

import { Composition, Sequence, Theme } from '@celesta/react';
import type { Layer, LayerContent, Paint, ThemeValues } from '@celesta/react';
import { mount } from '@celesta/react/internal';
import { PatternBackground } from '../src/index.js';
import type { PatternBackgroundProps, PatternName, PatternTile } from '../src/index.js';

const h = React.createElement;

function render(element: ReactElement, { width = 1920, height = 1080, theme }: { width?: number; height?: number; theme?: ThemeValues } = {}) {
  const composition = mount(() => h(Composition, { width, height, fps: 30, durationInFrames: 300 },
    theme ? h(Theme, theme, element) : element));
  return (frame = 0): Layer[] => composition.renderAt({ value: frame, timescale: 30 }, null).scene.layers;
}

/** A layer whose content is of type `T`. */
type LayerOf<T extends LayerContent['type']> = Layer & { content: Extract<LayerContent, { type: T }> };

/** `layer`, checked to have content of `type`. */
function layerOf<T extends LayerContent['type']>(layer: Layer | undefined, type: T): LayerOf<T> {
  assert.ok(layer, `a ${type} layer`);
  assert.equal(layer.content.type, type);
  return layer as LayerOf<T>;
}

/** The color of a solid paint. */
function colorOf(paint: Paint | null | undefined): string {
  assert.ok(paint && paint.type === 'solid', 'a solid paint');
  return paint.color;
}

/** One decoded mark: its centre, size, rotation, and RGBA bytes. */
interface Mark {
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  color: number[];
}

const floats = (b64: string) => {
  const bytes = Buffer.from(b64, 'base64');
  return Array.from(new Float32Array(bytes.buffer, bytes.byteOffset, bytes.byteLength / 4));
};
const bytes = (b64: string) => Array.from(Buffer.from(b64, 'base64'));

// Every layer, depth first, with its depth.
function walk(layers: Layer[], out: Layer[] = []): Layer[] {
  for (const layer of layers) {
    out.push(layer);
    if (layer.content.type === 'group') walk(layer.content.layers, out);
  }
  return out;
}

/** The marks of each <Instances> layer, decoded. */
function marks(layers: Layer[]): { shape: string; marks: Mark[] }[] {
  return walk(layers).filter((layer) => layer.content.type === 'instances').map((layer) => {
    const c = layerOf(layer, 'instances').content;
    const xs = floats(c.x);
    const ys = floats(c.y);
    // Without per-mark sizes, every mark has the layer's width and height.
    const ws = c.widths ? floats(c.widths) : xs.map(() => c.width!);
    const hs = c.heights ? floats(c.heights) : xs.map(() => c.height!);
    const rs = c.rotation ? floats(c.rotation) : xs.map(() => 0);
    const cs = c.colors ? bytes(c.colors) : [];
    return {
      shape: c.shape.type,
      marks: xs.map((x, i) => ({ x, y: ys[i], width: ws[i], height: hs[i], rotation: rs[i], color: cs.slice(i * 4, i * 4 + 4) })),
    };
  });
}

const visible = (list: Mark[], width: number, height: number) => list.filter((m) =>
  m.x + m.width / 2 > 0 && m.x - m.width / 2 < width && m.y + m.height / 2 > 0 && m.y - m.height / 2 < height);

test('dots draw as one layer, clipped to the canvas, over the background', () => {
  const layers = render(h(PatternBackground, { pattern: 'dots', spacing: 80, size: 3, background: '#101820', color: '#FFFFFF', opacity: 0.12 }))();
  // Representative layer count: clip group, background, marks group, one <Instances>.
  assert.equal(walk(layers).length, 4);
  const root = layerOf(layers[0], 'group');
  assert.deepEqual(root.content.clip, { x: 0, y: 0, width: 1920, height: 1080, cornerRadius: 0 });
  const [background, group] = root.content.layers;
  assert.equal(colorOf(layerOf(background, 'rect').content.fill), '#101820');
  assert.equal(group.opacity, 0.12);
  const [{ shape, marks: dots }] = marks(layers);
  assert.equal(shape, 'circle');
  const inside = visible(dots, 1920, 1080);
  assert.equal(inside.length, 24 * 14);
  assert.deepEqual([inside[0].x, inside[0].y, inside[0].width], [40, 40, 3]);
  assert.ok(dots.every((d) => d.color.join() === '255,255,255,255'));
});

test('patterns fill portrait, square, and arbitrary areas with no gap at the edges', () => {
  for (const [width, height] of [[1920, 1080], [1080, 1920], [1080, 1080], [333, 77]]) {
    for (const offset of [0, -17, 23.5]) {
      const layers = render(h(PatternBackground, { pattern: 'grid', spacing: 50, x: 10, y: 20, width, height, offsetX: offset, offsetY: offset }),
        { width: 2000, height: 2000 })();
      const [{ marks: lines }] = marks(layers);
      const xs = lines.filter((l) => l.height > l.width).map((l) => l.x).sort((a, b) => a - b);
      const ys = lines.filter((l) => l.width > l.height).map((l) => l.y).sort((a, b) => a - b);
      // A line at or before each edge, and none more than a cell apart.
      assert.ok(xs[0] <= 0 && xs.at(-1)! >= width, `${width}x${height} ${offset}`);
      assert.ok(ys[0] <= 0 && ys.at(-1)! >= height, `${width}x${height} ${offset}`);
      assert.ok(xs.every((x, i) => i === 0 || Math.abs(x - xs[i - 1] - 50) < 1e-3));
      assert.deepEqual(layers[0].transform.position, { x: 10, y: 20 });
    }
  }
});

test('stripes cover the area at any angle', () => {
  for (const angle of [0, 30, 45, 90, -60, 135]) {
    const [{ marks: stripes }] = marks(render(h(PatternBackground, { pattern: 'stripes', spacing: 40, size: 10, angle }), { width: 400, height: 300 })());
    const theta = (angle * Math.PI) / 180;
    const n = [-Math.sin(theta), Math.cos(theta)];
    const ts = stripes.map((s) => s.x * n[0] + s.y * n[1]).sort((a, b) => a - b);
    const corners = [[0, 0], [400, 0], [0, 300], [400, 300]].map(([x, y]) => x * n[0] + y * n[1]);
    assert.ok(ts[0] <= Math.min(...corners) && ts.at(-1)! >= Math.max(...corners), `angle ${angle}`);
    assert.ok(ts.every((t, i) => i === 0 || Math.abs(t - ts[i - 1] - 40) < 1e-3));
    // Each stripe, centred on the area's middle along its length, reaches past both sides.
    const across = Math.abs(400 * Math.cos(theta)) + Math.abs(300 * Math.sin(theta));
    const d = [Math.cos(theta), Math.sin(theta)];
    assert.ok(stripes.every((s) => s.width >= across && Math.abs((s.x - 200) * d[0] + (s.y - 150) * d[1]) < 1e-3));
    assert.ok(stripes.every((s) => s.rotation === angle && s.height === 10));
  }
});

test('speed moves the pattern with the frame, the same on a seek and in a nested Sequence', () => {
  const props: PatternBackgroundProps = { pattern: 'dots', spacing: 40, speedX: 30, speedY: -15, variation: { opacity: 0.5, scale: 0.5, seed: 'x' } };
  const direct = render(h(PatternBackground, props));
  const nested = render(h(Sequence, { from: 20 }, h(Sequence, { from: 10 }, h(PatternBackground, props))));
  const at = (layers: Layer[]) => marks(layers)[0].marks;
  const frame45 = at(direct(45));
  assert.deepEqual(at(direct(45)), frame45);
  assert.deepEqual(at(nested(75)), frame45);
  // Dot (column, row) keeps its variation as the pattern moves 1.5 s × 30 px.
  const find = (list: Mark[], x: number, y: number) => list.find((d) => Math.abs(d.x - x) < 1e-3 && Math.abs(d.y - y) < 1e-3);
  const before = find(at(direct(0)), 20, 20);
  const after = find(frame45, 20 + 45, 20 - 22.5);
  assert.ok(before && after);
  assert.deepEqual([after.width, after.color], [before.width, before.color]);
  const varied = new Set(frame45.map((d) => d.width));
  assert.ok(varied.size > 10);
  assert.ok(frame45.every((d) => d.width >= 2 && d.width <= 4 && d.color[3] >= 127));
});

test('custom marks draw in every cell, a function varying them by cell', () => {
  const tile: PatternTile = {
    marks: ({ column, row }) => (column + row) % 2 === 0
      ? [{ shape: 'circle', width: 10, color: 'accent', opacity: 0.5 }, { x: 0, y: 0, width: 4, height: 2, rotation: 45 }]
      : [],
  };
  const layers = render(h(PatternBackground, { pattern: tile, spacing: [100, 50], color: '#00ff00', opacity: 0.8 }),
    { width: 400, height: 200, theme: { colors: { accent: '#ff000080' } } })();
  const byShape: Record<string, Mark[]> = Object.fromEntries(marks(layers).map((m) => [m.shape, m.marks]));
  const circles = visible(byShape.circle, 400, 200);
  assert.equal(circles.length, 8);
  assert.deepEqual(circles[0].color, [255, 0, 0, 64]);
  assert.deepEqual([circles[0].x, circles[0].y], [50, 25]);
  assert.ok(byShape.rect.every((r) => r.rotation === 45 && r.color.join() === '0,255,0,255'));
  assert.equal(walk(layers).find((l) => l.content.type === 'group' && l.opacity !== 1)?.opacity, 0.8);
});

test('grid lines cross without darkening: opacity and color alpha fade the group', () => {
  const layers = render(h(PatternBackground, { pattern: 'grid', color: '#ffffff80', opacity: 0.5 }))();
  const group = layerOf(layers[0], 'group').content.layers[0];
  assert.ok(Math.abs(group.opacity - 0.5 * 128 / 255) < 1e-6);
  assert.ok(marks(layers)[0].marks.every((l) => l.color[3] === 255));
});

test('a pattern needing too many marks, or bad sizes, throws', () => {
  assert.throws(() => render(h(PatternBackground, { pattern: 'dots', spacing: 1 }))(), /more than 100000 marks/);
  assert.doesNotThrow(() => render(h(PatternBackground, { pattern: 'dots', spacing: 1, maxMarks: 3_000_000 }), { width: 100, height: 100 })());
  assert.throws(() => render(h(PatternBackground, { spacing: 0 }))(), /spacing must be a finite number greater than 0/);
  assert.throws(() => render(h(PatternBackground, { pattern: 'grid', size: -1 }))(), /size must be/);
  // Not a pattern, cast to check the error.
  assert.throws(() => render(h(PatternBackground, { pattern: 'hex' as PatternName }))(), /pattern must be/);
  assert.throws(() => render(h(PatternBackground, { color: 'nope' }))(), /color "nope"/);
});
