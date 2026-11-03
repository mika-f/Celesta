import assert from 'node:assert/strict';
import { beforeEach, test } from 'vitest';
import * as React from 'react';
import type { ReactElement, ReactNode } from 'react';

import { Composition, Rect, Sequence, Span, Theme, useCurrentFrame } from '@celesta.studio/react';
import type { Layer, LayerContent, Paint, TextMetrics, TextStyle, ThemeValues } from '@celesta.studio/react';
import { mount, setTextMeasurer } from '@celesta.studio/react/internal';
import type { MeasureTextRequest } from '@celesta.studio/react/internal';
import { Center, useLayoutBounds } from '@celesta.studio/layout';
import { Badge } from '../src/index.js';
import type { BadgeProps } from '../src/index.js';

const h = React.createElement;

// A stand-in for the shaper: Japanese characters are a full em wide, others
// half an em (0.6 em in "mono"); lines break at `maxWidth` between characters.
const requests: MeasureTextRequest[] = [];
function advance(character: string, style: TextStyle): number {
  const size = style.fontSize ?? 20;
  if (/[　-鿿]/u.test(character)) return size;
  return size * (style.fontFamily === 'mono' ? 0.6 : 0.5);
}
function metrics(request: MeasureTextRequest): TextMetrics {
  const { text, style, maxWidth } = request;
  const size = style.fontSize ?? 20;
  const lineHeight = style.lineHeight ?? size * 1.25;
  const lines: string[][] = [[]];
  for (const character of Array.from(text)) {
    const line = lines[lines.length - 1];
    const width = line.reduce((sum, c) => sum + advance(c, style), 0) + advance(character, style);
    if (maxWidth !== undefined && line.length > 0 && width > maxWidth) lines.push([character]);
    else line.push(character);
  }
  const widths = lines.map((line) => line.reduce((sum, c) => sum + advance(c, style), 0));
  return {
    width: Math.max(...widths), height: text === '' ? 0 : lines.length * lineHeight,
    ascent: size, descent: lineHeight - size, lineHeight, capHeight: size * 0.7, xHeight: size * 0.5,
    lines: lines.length, glyphs: [],
  };
}
beforeEach(() => {
  requests.length = 0;
  setTextMeasurer(async (request) => metrics(request), (request) => {
    requests.push(request);
    return metrics(request);
  });
});

type Content<T extends LayerContent['type']> = Extract<LayerContent, { type: T }>;

/** `layer`'s content, checked to be of `type`. */
function contentOf<T extends LayerContent['type']>(layer: Layer | undefined, type: T): Content<T> {
  assert.ok(layer, `a ${type} layer`);
  assert.equal(layer.content.type, type);
  return layer.content as Content<T>;
}

/** The color of a solid paint. */
function colorOf(paint: Paint | null | undefined): string {
  assert.ok(paint && paint.type === 'solid', 'a solid paint');
  return paint.color;
}

function render(element: ReactElement, { theme }: { theme?: ThemeValues } = {}) {
  const composition = mount(() => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 300 },
    theme ? h(Theme, theme, element) : element));
  return (frame = 0): Layer[] => composition.renderAt({ value: frame, timescale: 30 }, null).scene.layers;
}

// The label goes in as createElement's children, as JSX would pass it; they
// replace the `children: undefined` that BadgeProps requires.
const badge = (props: Omit<BadgeProps, 'children'>, ...children: ReactNode[]) =>
  h(Badge, { ...props, children: undefined }, ...children);

/** The background, content group, and text of a rendered badge. */
function parts(layer: Layer | undefined) {
  const layers = contentOf(layer, 'group').layers;
  const background = layers.find((child) => child.content.type === 'rect' && child.content.fill);
  const content = layers.find((child) => child.content.type === 'group');
  const text = contentOf(content, 'group').layers.find((child) => child.content.type === 'text');
  assert.ok(content && text);
  // A badge without a fill has no background, so it is looked up on use.
  return {
    background, content, text, group: contentOf(content, 'group'), label: contentOf(text, 'text'),
    get rect() { return contentOf(background, 'rect'); },
  };
}

function Probe() {
  const { width, height } = useLayoutBounds();
  return h(Rect, { width, height, fill: '#ff0000' });
}

const style: TextStyle = { fontFamily: 'sans', fontSize: 20 };

test('the background is the measured text plus the padding, and the text sits on its baseline inside it', () => {
  const [layer] = render(badge({ x: 100, y: 50, fill: '#204060', style, padding: { top: 10, right: 20, bottom: 10, left: 20 } }, 'Chapter'))();
  const { rect, content, text, label } = parts(layer);
  assert.deepEqual(layer.transform.position, { x: 100, y: 50 });
  // 7 characters × 10 px, 25 px line box.
  assert.deepEqual([rect.width, rect.height], [110, 45]);
  assert.equal(rect.cornerRadius, 8);
  assert.deepEqual(content.transform.position, { x: 20, y: 10 });
  assert.deepEqual(text.transform.position, { x: 0, y: 20 });
  assert.equal(label.baselineAnchor, true);
  assert.equal(label.text, 'Chapter');
});

test('Japanese and Latin text are measured, not estimated from the number of characters', () => {
  const [ja] = render(badge({ fill: '#000000', style, padding: 0 }, '第1章'))();
  const [en] = render(badge({ fill: '#000000', style, padding: 0 }, 'abc'))();
  // 第 and 章 are 20 px, 1 is 10 px; abc is 3 × 10 px.
  assert.equal(parts(ja).rect.width, 50);
  assert.equal(parts(en).rect.width, 30);
});

test('the measurement and the drawn text get the same text and style, shorthand and theme fonts included', () => {
  const [layer] = render(badge({ fill: 'surface', fontFamily: 'display', fontSize: 30, fontWeight: 700, color: 'ink', padding: 0 },
    'Hi ', h(Span, { fontWeight: 400 }, 'there')), { theme: { fonts: { display: 'mono' }, colors: { surface: '#101010', ink: '#fafafa' } } })();
  const { rect, label } = parts(layer);
  const [request] = requests;
  assert.equal(request.text, 'Hi there');
  assert.equal(request.style.fontFamily, 'mono');
  assert.equal(request.style.fontSize, 30);
  assert.equal(label.style.fontFamily, 'mono');
  assert.equal(label.style.fontSize, 30);
  assert.equal(label.style.fontWeight, 700);
  assert.deepEqual(label.style.fontRuns, request.style.fontRuns);
  assert.equal(colorOf(label.style.fill), '#fafafa');
  assert.equal(colorOf(rect.fill), '#101010');
  // 8 characters × 18 px in mono at 30 px.
  assert.equal(rect.width, 144);
});

test('changing the text or the font resizes the background on the next frame', () => {
  function Changing({ frame }: { frame: number }) {
    return badge({ fill: '#000000', padding: 0, style: { fontFamily: frame < 2 ? 'sans' : 'mono', fontSize: 20 } }, frame < 1 ? 'ab' : 'abcd');
  }
  const composition = mount(() => h(Composition, { width: 400, height: 200, fps: 30, durationInFrames: 10 },
    h(FrameDriven, { render: (frame) => h(Changing, { frame }) })));
  const widthAt = (frame: number) => parts(composition.renderAt({ value: frame, timescale: 30 }, null).scene.layers[0]).rect.width;
  assert.deepEqual([widthAt(0), widthAt(1), widthAt(2)], [20, 40, 48]);
});

test('empty text keeps one line box, so the badge does not collapse', () => {
  const [layer] = render(badge({ fill: '#000000', style }, ''))();
  const { rect } = parts(layer);
  assert.deepEqual([rect.width, rect.height], [32, 41]);
});

test('a leading decoration lays out in its own box one line tall, then the gap, then the text', () => {
  const [layer] = render(badge({
    fill: '#000000', style, padding: { left: 24, right: 24, top: 16, bottom: 16 }, leadingWidth: 8, gap: 12,
    leading: h(Center, null, h(Probe)),
  }, 'API'))();
  const { rect, group, text } = parts(layer);
  assert.equal(rect.width, 24 + 8 + 12 + 30 + 24);
  const [center] = group.layers;
  // The leading box is 8 × 25; Center puts its children at the middle.
  assert.deepEqual(center.transform.position, { x: 4, y: 12.5 });
  const probe = contentOf(contentOf(center, 'group').layers[0], 'rect');
  assert.deepEqual([probe.width, probe.height], [8, 25]);
  assert.equal(text.transform.position.x, 20);
});

test('a leading decoration without a width throws', () => {
  assert.throws(() => render(badge({ leading: h(Probe) }, 'x'))(), /<Badge> leading needs a leadingWidth/);
});

test('maxWidth wraps long text inside the padding and the badge grows taller, never wider', () => {
  const [layer] = render(badge({ fill: '#000000', style, padding: 10, maxWidth: 100 }, 'abcdefghijklmnop'))();
  const { rect, label } = parts(layer);
  // 80 px of room: 8 characters a line, two lines.
  assert.equal(label.maxWidth, 80);
  assert.deepEqual([rect.width, rect.height], [100, 70]);
  const [short] = render(badge({ fill: '#000000', style, padding: 10, maxWidth: 100 }, 'abc'))();
  assert.equal(parts(short).rect.width, 50);
});

test('maxWidth leaves room for the leading decoration', () => {
  const [layer] = render(badge({ style, padding: 0, maxWidth: 60, leading: h(Probe), leadingWidth: 10, gap: 10 }, 'abcdefgh'))();
  assert.equal(parts(layer).label.maxWidth, 40);
});

test('cornerRadius "pill" is half the measured height', () => {
  const [layer] = render(badge({ fill: '#000000', style, padding: 10, cornerRadius: 'pill' }, 'pill'))();
  assert.equal(parts(layer).rect.cornerRadius, 22.5);
});

test('the anchor and transforms move the background and the text together', () => {
  const [layer] = render(badge({
    x: 960, y: 540, anchorX: 0.5, anchorY: 0.5, scale: 2, rotation: 15, fill: '#000000', style, padding: 10,
  }, 'abcd'))();
  const { background, content } = parts(layer);
  assert.ok(background);
  assert.deepEqual(layer.transform.position, { x: 960, y: 540 });
  assert.deepEqual(layer.transform.scale, { x: 2, y: 2 });
  assert.equal(layer.transform.rotation, 15);
  // 60 × 45: both start at the top-left of the box centred on the anchor.
  assert.deepEqual(background.transform.position, { x: -30, y: -22.5 });
  assert.deepEqual(content.transform.position, { x: -20, y: -12.5 });
});

test('the group language goes into the measured and drawn style', () => {
  render(badge({ lang: 'ja', style }, '漢字'))();
  assert.equal(requests[0].style.lang, 'ja');
});

test('negative or non-finite sizes throw', () => {
  const cases: [Omit<BadgeProps, 'children'>, RegExp][] = [
    [{ padding: -1 }, /<Badge> padding top must be/],
    [{ cornerRadius: -2 }, /<Badge> cornerRadius must be/],
    [{ gap: Number.NaN, leading: h(Probe), leadingWidth: 4 }, /<Badge> gap must be/],
    [{ leading: h(Probe), leadingWidth: -4 }, /<Badge> leadingWidth must be/],
    [{ maxWidth: Infinity }, /<Badge> maxWidth must be/],
  ];
  for (const [props, message] of cases) {
    assert.throws(() => render(badge({ style, ...props }, 'x'))(), message, JSON.stringify(props));
  }
});

test('a badge renders the same on a seek as when played up to it, inside a Sequence', () => {
  const scene = h(Sequence, { from: 10 }, badge({ fill: '#123456', style, cornerRadius: 'pill' }, 'seek'));
  const played = render(scene);
  for (let frame = 0; frame <= 30; frame += 1) played(frame);
  assert.deepEqual(played(30), render(scene)(30));
});

/** Calls `render` with the current frame, to change props from frame to frame. */
function FrameDriven({ render: draw }: { render: (frame: number) => ReactElement }) {
  return draw(useCurrentFrame());
}
