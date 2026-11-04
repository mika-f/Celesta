import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import { Composition, Theme, defineScenes } from '@celesta.studio/react';
import type { Layer, LayerContent, Paint, Point, SceneEntry, ThemeValues } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import { FrameChrome } from '../src/index.js';
import type { FrameChromeProps } from '../src/index.js';

const h = React.createElement;

/** A leaf layer's content, with where it ends up and how it is drawn (over any `x` of its own). */
type Placed<C> = C extends unknown ? Omit<C, 'x' | 'y' | 'opacity' | 'anchor'> & { x: number; y: number; opacity: number; anchor: Point } : never;
type Leaf = Placed<Exclude<LayerContent, { type: 'group' }>>;
type LeafOf<T extends Leaf['type']> = Extract<Leaf, { type: T }>;

const is = <T extends Leaf['type']>(type: T) => (leaf: Leaf): leaf is LeafOf<T> => leaf.type === type;

/** The color of a solid paint. */
function colorOf(paint: Paint | null | undefined): string {
  assert.ok(paint && paint.type === 'solid', 'a solid paint');
  return paint.color;
}

// Every leaf layer with its position summed through its groups.
function leaves(layers: Layer[], x = 0, y = 0, out: Leaf[] = []): Leaf[] {
  for (const layer of layers) {
    const lx = x + layer.transform.position.x;
    const ly = y + layer.transform.position.y;
    if (layer.content.type === 'group') leaves(layer.content.layers, lx, ly, out);
    else out.push({ ...layer.content, x: lx, y: ly, opacity: layer.opacity, anchor: layer.transform.anchor });
  }
  return out;
}

interface Options {
  width?: number;
  height?: number;
  durationInFrames?: number;
  frame?: number;
  theme?: ThemeValues;
}

function render<T extends SceneEntry>(props: FrameChromeProps<T>, { width = 1920, height = 1080, durationInFrames = 90, frame = 0, theme }: Options = {}) {
  const chrome = h(FrameChrome<T>, props);
  const composition = mount(() => h(Composition, { width, height, fps: 30, durationInFrames },
    theme ? h(Theme, theme, chrome) : chrome));
  return (f = frame): Layer[] => composition.renderAt({ value: f, timescale: 30 }, null).scene.layers;
}

/** The text leaf that reads `text`. */
function textAt(layers: Layer[], text: string): LeafOf<'text'> {
  const found = leaves(layers).filter(is('text')).find((layer) => layer.text === text);
  assert.ok(found, `a text "${text}"`);
  return found;
}

test('FrameChrome keeps its corners and labels inside the margin at any canvas shape', () => {
  for (const [width, height] of [[1920, 1080], [1080, 1920], [1080, 1080]]) {
    const layers = render({ corners: true, timecode: true, title: 'T', caption: 'C', margin: 40, cornerLength: 20, gap: 10 },
      { width, height })(0);
    const rects = leaves(layers).filter(is('rect'));
    assert.equal(rects.length, 8);
    const xs = rects.flatMap((r) => [r.x, r.x + r.width!]);
    const ys = rects.flatMap((r) => [r.y, r.y + r.height!]);
    assert.deepEqual([Math.min(...xs), Math.max(...xs)], [40, width - 40], `${width}x${height}`);
    assert.deepEqual([Math.min(...ys), Math.max(...ys)], [40, height - 40], `${width}x${height}`);
    const title = textAt(layers, 'T');
    assert.deepEqual([title.x, title.y, title.anchor.x], [70, 50, 0]);
    const timecode = textAt(layers, '00:00:00:00');
    assert.deepEqual([timecode.x, timecode.y, timecode.anchor.x], [width - 70, 50, 1]);
    const caption = textAt(layers, 'C');
    assert.deepEqual([caption.x, caption.y, caption.anchor.x], [width - 70, height - 50, 1]);
  }
});

test('FrameChrome draws only what is turned on, the bar blended apart from the marks', () => {
  assert.deepEqual(render({ blendMode: 'difference', opacity: 0.8 })(0), []);
  const layers = render({ corners: true, progress: true, blendMode: 'difference', progressBlendMode: 'normal', opacity: 0.8 })(10);
  assert.deepEqual(layers.map((layer) => [layer.blendMode, layer.opacity]), [['difference', 0.8], [undefined, 0.8]]);
  const same = render({ corners: true, progress: true, blendMode: 'difference' })(10);
  assert.deepEqual(same.map((layer) => layer.blendMode), ['difference', 'difference']);
});

test('the progress bar is empty on the first frame and full on the last', () => {
  const frame = render({ progress: true, margin: 60, trackOpacity: 0 }, { width: 1000, durationInFrames: 31 });
  const bar = (f: number) => leaves(frame(f)).filter(is('rect'));
  assert.deepEqual(bar(0), []);
  assert.equal(bar(15)[0].width, 440);
  const full = bar(30)[0];
  assert.deepEqual([full.x, full.width, full.y + full.height! / 2], [60, 880, 1080 - 30]);
});

test('scene segments fill as one bar, without a break at the boundaries', () => {
  const scenes = defineScenes([
    { id: 'a', durationInFrames: 20 },
    { id: 'empty', durationInFrames: 0 },
    { id: 'b', durationInFrames: 40 },
  ]);
  const frame = render({ progress: true, scenes, margin: 0, segmentGap: 10, trackOpacity: 0.5, progressColor: '#ff0000' },
    { width: 610, durationInFrames: 60 });
  const filled = (f: number) => leaves(frame(f)).filter(is('rect')).filter((layer) => layer.fill?.type === 'solid' && layer.fill.color === '#ff0000');
  const tracks = leaves(frame(0)).filter(is('rect'));
  assert.deepEqual(tracks.map((r) => [r.x, r.width]), [[0, 200], [210, 400]]);
  assert.ok(tracks.every((r) => r.opacity === 0.5));
  assert.deepEqual(filled(0), []);
  // Filled share of the bar, segment by segment, against the film's progress.
  let last = 0;
  for (let f = 0; f < 60; f++) {
    const share = filled(f).reduce((sum, r) => sum + r.width!, 0) / 600;
    assert.ok(Math.abs(share - f / 59) < 1e-9, `frame ${f}: ${share}`);
    assert.ok(share >= last);
    last = share;
  }
  assert.deepEqual(filled(59).map((r) => r.width), [200, 400]);
});

test('labels show the timecode, the current chapter, and a two-weight title', () => {
  const scenes = defineScenes([
    { id: 'intro', name: 'COLD OPEN', durationInFrames: 30 },
    { id: 'body', durationInFrames: 30 },
  ]);
  const frame = render({ timecode: true, timecodeHours: false, chapters: true, scenes, title: 'CELESTA', subtitle: '/ REEL',
    color: 'paper', fontFamily: 'mono', formatChapter: (s) => `0${s.index + 1}  ${s.name}` },
  { theme: { colors: { paper: '#f4f1ea' }, fonts: { mono: 'JetBrains Mono' } } });
  const layers = frame(45);
  assert.ok(textAt(layers, '00:01:15'));
  const chapter = textAt(layers, '02  body');
  assert.equal(chapter.style.fontFamily, 'JetBrains Mono');
  assert.equal(colorOf(chapter.style.fill), '#f4f1ea');
  const title = textAt(layers, 'CELESTA / REEL');
  assert.equal(title.style.fontWeight, 400);
  assert.deepEqual(title.style.fontRuns?.map((run) => run.fontWeight), [700]);
  assert.deepEqual(title.style.colorRuns?.map((run) => run.color), ['#f4f1ea99']);
  assert.ok(textAt(frame(10), '01  COLD OPEN'));
  assert.deepEqual(leaves(frame(89)).map((l) => (l.type === 'text' ? l.text : undefined)), ['CELESTA / REEL', '00:02:29'], 'no chapter after the last scene');
});

test('positions move labels, and two labels in one corner throw', () => {
  const layers = render({ timecode: true, title: 'T', positions: { timecode: 'bottom-right' } })(0);
  const timecode = textAt(layers, '00:00:00:00');
  assert.deepEqual([timecode.x, timecode.y], [1920 - 94, 1080 - 68]);
  assert.throws(() => render({ timecode: true, title: 'T', positions: { title: 'top-right' } })(0), /both placed top-right/);
  assert.throws(() => render({ progress: true, color: 'nope' })(0), /neither a #RRGGBB/);
});
