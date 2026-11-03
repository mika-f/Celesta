import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';
import type { ReactElement } from 'react';

import { Composition, Rect, Sequence, Text, Theme } from '@celesta.studio/react';
import type { Layer, LayerContent, Paint, ThemeValues } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import { Center, Fit, useLayoutBounds } from '@celesta.studio/layout';
import { Panel } from '../src/index.js';
import type { PanelProps } from '../src/index.js';

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

/** The layers of a group layer. */
const childrenOf = (layer: Layer | undefined): Layer[] => layerOf(layer, 'group').content.layers;

/** The color of a solid paint. */
function colorOf(paint: Paint | null | undefined): string {
  assert.ok(paint && paint.type === 'solid', 'a solid paint');
  return paint.color;
}

/** A rect the size of the layout bounds it sees, to read them back. */
function Probe() {
  const { width, height } = useLayoutBounds();
  return h(Rect, { width, height, fill: '#ff0000' });
}

const position = (layer: Layer) => layer.transform.position;

test('a panel draws its background, its content inside the padding, then its border', () => {
  const [panel] = render(h(Panel, {
    x: 100, y: 200, width: 720, height: 320, padding: 32, fill: '#182030', stroke: '#FFFFFF33', strokeWidth: 3, cornerRadius: 20,
  }, h(Probe)))();
  assert.deepEqual(position(panel), { x: 100, y: 200 });
  const [background, content, border] = childrenOf(panel);
  const fill = layerOf(background, 'rect').content;
  assert.deepEqual(
    [fill.width, fill.height, colorOf(fill.fill), fill.cornerRadius],
    [720, 320, '#182030', 20]);
  assert.equal(fill.stroke, undefined);
  assert.deepEqual(position(content), { x: 32, y: 32 });
  assert.equal(layerOf(content, 'group').content.clip, undefined);
  const probe = layerOf(childrenOf(content)[0], 'rect').content;
  assert.deepEqual([probe.width, probe.height], [656, 256]);
  const edge = layerOf(border, 'rect').content;
  assert.equal(edge.fill, undefined);
  assert.deepEqual([edge.width, edge.height, edge.cornerRadius], [720, 320, 20]);
  assert.deepEqual([colorOf(edge.stroke?.paint), edge.stroke?.width], ['#FFFFFF33', 3]);
});

test('per-edge padding moves the origin and shrinks the area Center and Fit see', () => {
  const [panel] = render(h(Panel, { width: 400, height: 300, padding: { top: 10, right: 20, bottom: 30, left: 40 } },
    h(Center, null, h(Text, null, 'c')),
    h(Fit, { sourceWidth: 100, sourceHeight: 100 }, h(Probe))))();
  const [content] = childrenOf(panel);
  assert.deepEqual(position(content), { x: 40, y: 10 });
  const [center, fit] = childrenOf(content);
  // 340×260 inside the padding.
  assert.deepEqual(position(center), { x: 170, y: 130 });
  assert.equal(fit.transform.scale.x, 2.6);
  assert.deepEqual(position(fit), { x: 40, y: 0 });
});

test('missing padding edges are 0, and no fill or stroke draws only the content', () => {
  const [panel] = render(h(Panel, { width: 200, height: 100, padding: { left: 12 } }, h(Probe)))();
  assert.equal(childrenOf(panel).length, 1);
  const [content] = childrenOf(panel);
  assert.deepEqual(position(content), { x: 12, y: 0 });
  const probe = layerOf(childrenOf(content)[0], 'rect').content;
  assert.deepEqual([probe.width, probe.height], [188, 100]);
});

test('clip cuts the content to the rounded panel, padding included, and leaves the shadow alone', () => {
  const shadow = { color: '#00000080', blur: 24, offsetX: 0, offsetY: 12 };
  const [panel] = render(h(Panel, { width: 600, height: 240, padding: { top: 20, left: 30 }, fill: '#202020', cornerRadius: 24, shadow, clip: true },
    h(Probe)))();
  assert.equal(layerOf(panel, 'group').content.clip, undefined);
  const [background, content] = childrenOf(panel);
  assert.deepEqual(background.effects?.shadow, shadow);
  assert.equal(content.effects, undefined);
  assert.deepEqual(layerOf(content, 'group').content.clip, { x: -30, y: -20, width: 600, height: 240, cornerRadius: 24 });
});

test('panels nest, each inner one laying out in its parent\'s content area', () => {
  const [outer] = render(h(Panel, { width: 800, height: 600, padding: 50 },
    h(Center, null, h(Panel, { width: 300, height: 200, padding: 25, anchorX: 0.5, anchorY: 0.5 }, h(Probe)))))();
  const [content] = childrenOf(outer);
  const [center] = childrenOf(content);
  assert.deepEqual(position(center), { x: 350, y: 250 });
  const [inner] = childrenOf(center);
  const [innerContent] = childrenOf(inner);
  // The inner panel is centred on its parent's centre: its top-left at −150, −100.
  assert.deepEqual(position(innerContent), { x: -125, y: -75 });
  const probe = layerOf(childrenOf(innerContent)[0], 'rect').content;
  assert.deepEqual([probe.width, probe.height], [250, 150]);
});

test('the anchor is the point the panel scales and rotates about', () => {
  const [panel] = render(h(Panel, { x: 960, y: 540, width: 400, height: 200, anchorX: 0.5, anchorY: 1, scale: 2, rotation: 10, fill: '#ffffff' }))();
  assert.deepEqual(position(panel), { x: 960, y: 540 });
  assert.deepEqual(panel.transform.scale, { x: 2, y: 2 });
  assert.equal(panel.transform.rotation, 10);
  // The group keeps the top-left anchor; the panel offsets its own layers instead.
  assert.deepEqual(panel.transform.anchor, { x: 0, y: 0 });
  const [background, content] = childrenOf(panel);
  assert.deepEqual(position(background), { x: -200, y: -200 });
  assert.deepEqual(position(content), { x: -200, y: -200 });
});

test('padding wider than the panel leaves a content area of 0 at the left and top padding', () => {
  const [panel] = render(h(Panel, { width: 100, height: 50, padding: { left: 80, right: 40, top: 60 } }, h(Probe)))();
  const [content] = childrenOf(panel);
  assert.deepEqual(position(content), { x: 80, y: 60 });
  const probe = layerOf(childrenOf(content)[0], 'rect').content;
  assert.deepEqual([probe.width, probe.height], [0, 0]);
});

test('a panel of size 0 is allowed and its clip hides the content', () => {
  const [panel] = render(h(Panel, { width: 0, height: 0, clip: true }, h(Probe)))();
  const [content] = childrenOf(panel);
  assert.deepEqual(layerOf(content, 'group').content.clip, { x: 0, y: 0, width: 0, height: 0, cornerRadius: 0 });
});

test('negative or non-finite sizes, padding, radius, and border widths throw', () => {
  const cases: [PanelProps, RegExp][] = [
    [{ width: -1, height: 10 }, /width must be a finite number of 0 or more, not -1/],
    [{ width: 10, height: Infinity }, /height must be a finite number of 0 or more, not Infinity/],
    [{ width: Number.NaN, height: 10 }, /width must be a finite number of 0 or more, not NaN/],
    [{ width: 10, height: 10, padding: -4 }, /padding top must be/],
    [{ width: 10, height: 10, padding: { bottom: Number.NaN } }, /padding bottom must be/],
    [{ width: 10, height: 10, cornerRadius: -2 }, /cornerRadius must be/],
    [{ width: 10, height: 10, stroke: '#ffffff', strokeWidth: -1 }, /strokeWidth must be/],
    [{ width: 10, height: 10, anchorX: Infinity }, /anchorX must be a finite number/],
  ];
  for (const [props, message] of cases) {
    assert.throws(() => render(h(Panel, props))(), message, JSON.stringify(props));
  }
});

test('a shadow without a fill throws instead of drawing nothing', () => {
  assert.throws(() => render(h(Panel, { width: 10, height: 10, stroke: '#ffffff', shadow: { color: '#000000', blur: 8, offsetX: 0, offsetY: 4 } }))(),
    /<Panel> shadow is cast by the background, so it needs a fill/);
});

test('colors take <Theme> names, and the border defaults to 2 px wide', () => {
  const [panel] = render(h(Panel, {
    width: 100, height: 100, fill: 'surface', stroke: 'line', shadow: { color: 'shade', blur: 4, offsetX: 0, offsetY: 2 },
  }), { theme: { colors: { surface: '#101820', line: '#334455', shade: '#00000066' } } })();
  const [background, , border] = childrenOf(panel);
  assert.equal(colorOf(layerOf(background, 'rect').content.fill), '#101820');
  assert.equal(background.effects?.shadow?.color, '#00000066');
  const { stroke } = layerOf(border, 'rect').content;
  assert.deepEqual([colorOf(stroke?.paint), stroke?.width], ['#334455', 2]);
  assert.throws(() => render(h(Panel, { width: 10, height: 10, fill: 'nope' }))(), /<Panel> fill "nope"/);
});

test('a gradient fill passes through, and strokeWidth 0 draws no border', () => {
  const fill: Paint = { type: 'linear', start: { x: 0, y: 0 }, end: { x: 0, y: 100 }, stops: [{ offset: 0, color: '#000000' }, { offset: 1, color: '#ffffff' }] };
  const [panel] = render(h(Panel, { width: 100, height: 100, fill, stroke: '#ffffff', strokeWidth: 0 }))();
  assert.equal(childrenOf(panel).length, 2);
  assert.equal(layerOf(childrenOf(panel)[0], 'rect').content.fill?.type, 'linear');
});

test('a panel renders the same on a seek as when played up to it, inside a Sequence', () => {
  const scene = h(Sequence, { from: 30 }, h(Panel, { width: 300, height: 200, padding: 20, fill: '#123456', clip: true }, h(Center, null, h(Probe))));
  const played = render(scene);
  for (let frame = 0; frame <= 60; frame += 1) played(frame);
  assert.deepEqual(played(60), render(scene)(60));
});
