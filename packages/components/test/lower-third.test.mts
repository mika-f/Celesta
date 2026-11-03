import assert from 'node:assert/strict';
import { beforeEach, test } from 'vitest';
import * as React from 'react';
import type { ReactElement } from 'react';

import { Composition, Sequence, Theme } from '@celesta.studio/react';
import type { Layer, LayerContent, Paint, TextMetrics } from '@celesta.studio/react';
import { SafeArea } from '@celesta.studio/layout';
import { mount, setTextMeasurer } from '@celesta.studio/react/internal';
import type { MeasureTextRequest, MountedComposition } from '@celesta.studio/react/internal';
import { LowerThird } from '../src/index.js';
import type { LowerThirdPlacement, LowerThirdProps } from '../src/index.js';

const h = React.createElement;

// Every character is `fontSize` wide; lines break at `\n` and wrap at maxWidth.
function measure({ text, style, maxWidth }: Pick<MeasureTextRequest, 'text' | 'style' | 'maxWidth'>): TextMetrics {
  const size = style.fontSize ?? 10;
  const per = maxWidth === undefined ? Infinity : Math.max(1, Math.floor(maxWidth / size));
  const lines = text === '' ? [] : text.split('\n').flatMap((line) => {
    const chars = Array.from(line);
    const out: string[] = [];
    for (let i = 0; i < Math.max(1, chars.length); i += per) out.push(chars.slice(i, i + per).join(''));
    return out;
  });
  const lineHeight = style.lineHeight ?? size * 1.25;
  return {
    width: Math.max(0, ...lines.map((line) => Array.from(line).length * size)),
    height: lines.length * lineHeight,
    ascent: size,
    descent: size * 0.25,
    lineHeight,
    capHeight: size * 0.7,
    xHeight: size * 0.5,
    lines: lines.length,
    glyphs: [],
    lineStarts: [0],
  };
}

beforeEach(() => setTextMeasurer(async (request) => measure(request), measure));

const at = (composition: MountedComposition, frame: number) => composition.renderAt({ value: frame, timescale: 30 }, null);
const root = (child: ReactElement, { width = 1920, height = 1080, durationInFrames = 90 } = {}) => () =>
  h(Composition, { width, height, fps: 30, durationInFrames }, child);

/** A leaf layer's content, with where it ends up and its opacity (over any `x` of its own). */
type Placed<C> = C extends unknown ? Omit<C, 'x' | 'y' | 'opacity'> & { x: number; y: number; opacity: number } : never;
type Leaf = Placed<Exclude<LayerContent, { type: 'group' }>>;
type LeafOf<T extends Leaf['type']> = Extract<Leaf, { type: T }>;

const is = <T extends Leaf['type']>(type: T) => (leaf: Leaf): leaf is LeafOf<T> => leaf.type === type;

/** The color of a solid paint. */
function colorOf(paint: Paint | null | undefined): string {
  assert.ok(paint && paint.type === 'solid', 'a solid paint');
  return paint.color;
}

// Every text and rect layer with its position and opacity summed through its groups.
function leaves(layers: Layer[], x = 0, y = 0, opacity = 1, out: Leaf[] = []): Leaf[] {
  for (const layer of layers) {
    const lx = x + layer.transform.position.x;
    const ly = y + layer.transform.position.y;
    const lo = opacity * layer.opacity;
    if (layer.content.type === 'group') leaves(layer.content.layers, lx, ly, lo, out);
    else out.push({ ...layer.content, x: lx, y: ly, opacity: lo });
  }
  return out;
}

function parts(composition: MountedComposition, frame: number) {
  const all = leaves(at(composition, frame).scene.layers);
  const rects = all.filter(is('rect'));
  const texts: Record<string, LeafOf<'text'>> = Object.fromEntries(all.filter(is('text')).map((l) => [l.text, l]));
  return { band: rects[0], accent: rects[1], texts };
}

// Top and bottom of a text layer, from its baseline and the fake metrics.
function box(layer: LeafOf<'text'>) {
  const size = layer.style.fontSize!;
  const lineHeight = layer.style.lineHeight!;
  const { lines } = measure({ text: layer.text, style: layer.style, maxWidth: layer.maxWidth ?? undefined });
  return { top: layer.y - size, bottom: layer.y - size + lines * lineHeight, lines };
}

const still: Partial<LowerThirdProps> = { enter: null, exit: null, textEnter: null };
const sized: Partial<LowerThirdProps> = {
  ...still, margin: 60, padding: { top: 10, right: 30, bottom: 10, left: 20 }, gap: 6, accentWidth: 8,
  titleSize: 40, titleLineHeight: 1.25, subtitleSize: 20, subtitleLineHeight: 1.5,
};

test('the band hugs the longer line, with the accent on its outer edge', () => {
  const { band, accent, texts } = parts(mount(root(h(LowerThird, { ...sized, title: 'Name', subtitle: 'A longer role' }))), 0);
  const content = Math.max(4 * 40, 13 * 20);
  const height = 10 + 40 * 1.25 + 6 + 20 * 1.5 + 10;
  assert.deepEqual([band.x, band.y, band.width, band.height], [60, 1080 - 60 - height, 8 + 20 + content + 30, height]);
  assert.deepEqual([accent.x, accent.y, accent.width, accent.height], [60, band.y, 8, height]);
  for (const text of ['Name', 'A longer role']) {
    assert.equal(texts[text].x, 60 + 8 + 20, text);
    assert.equal(texts[text].maxWidth, content, text);
    assert.equal(texts[text].style.align, 'left', text);
  }
  assert.equal(box(texts.Name).top, band.y + 10);
  assert.equal(box(texts['A longer role']).top, box(texts.Name).bottom + 6);
  assert.equal(box(texts['A longer role']).bottom, band.y + height - 10);
});

test('without a subtitle the band holds the title alone', () => {
  const { band, texts } = parts(mount(root(h(LowerThird, { ...sized, title: 'Name' }))), 0);
  assert.deepEqual(Object.keys(texts), ['Name']);
  assert.deepEqual([band.width, band.height], [8 + 20 + 160 + 30, 10 + 50 + 10]);
});

test('each placement sits in its corner inside the margin, its lines aligned toward it', () => {
  const placed = (placement: LowerThirdPlacement) => parts(mount(root(h(LowerThird, { ...sized, placement, title: 'Name', subtitle: 'Role' }))), 0);
  const width = 8 + 20 + 160 + 30;
  const height = 10 + 50 + 6 + 30 + 10;
  const right = placed('bottom-right');
  assert.deepEqual([right.band.x, right.band.y], [1920 - 60 - width, 1080 - 60 - height]);
  assert.equal(right.accent.x, 1920 - 60 - 8);
  assert.equal(right.texts.Name.x, 1920 - 60 - width + 20);
  assert.equal(right.texts.Name.style.align, 'right');
  const center = placed('bottom-center');
  assert.equal(center.band.x, (1920 - width) / 2);
  assert.equal(center.texts.Role.style.align, 'center');
  assert.deepEqual([placed('top-left').band.x, placed('top-left').band.y], [60, 60]);
  assert.deepEqual([placed('top-right').band.x, placed('top-right').band.y], [1920 - 60 - width, 60]);
});

test('a long Japanese name wraps and shrinks to its line limit, drawn with the breaks it was fitted with', () => {
  const name = 'とても長い名前の登壇者がいても二行に収まるまで小さくなります';
  const { band, texts } = parts(mount(root(h(LowerThird, {
    ...sized, title: name, maxWidth: 800, titleSize: 60, minTitleSize: 20, maxTitleLines: 2,
  }))), 0);
  const title = texts[name];
  const budget = 800 - 8 - 20 - 30;
  assert.ok(title.style.fontSize! < 60 && title.style.fontSize! >= 20, `size ${title.style.fontSize}`);
  // Drawn at the band's column, it keeps the two lines it was fitted with at the budget.
  assert.equal(box(title).lines, 2);
  assert.equal(measure({ text: name, style: title.style, maxWidth: budget }).lines, 2);
  assert.ok(band.width <= 800);
});

test('in landscape and portrait the band stays inside the safe area and margin', () => {
  for (const [width, height] of [[1920, 1080], [1080, 1920]]) {
    const { band, texts } = parts(mount(root(h(LowerThird, {
      ...still, safeArea: 'vertical-social', title: 'A speaker with a fairly long name', subtitle: 'And a role that is long as well',
    }), { width, height })), 0);
    const margin = Math.round(Math.min(width, height) * 0.06);
    const inset = { top: Math.round(0.1 * height), right: Math.round(0.185 * width), bottom: Math.round(0.2 * height), left: Math.round(width / 15) };
    const room = width - inset.left - inset.right - 2 * margin;
    assert.equal(band.x, inset.left + margin, `${width}x${height}`);
    assert.equal(band.y + band.height, height - inset.bottom - margin, `${width}x${height}`);
    assert.ok(band.x + band.width <= width - inset.right - margin + 1e-9, `${width}x${height}`);
    // Landscape caps the band at 60% of the room; portrait lets it use all of it.
    assert.ok(band.width <= (width > height ? room * 0.6 : room) + 1e-9, `${width}x${height}`);
    for (const text of Object.values(texts)) {
      assert.ok(text.x + text.maxWidth! <= band.x + band.width + 1e-9, `${width}x${height} ${text.text}`);
    }
  }
});

test('LowerThird lays out in the area of an enclosing <SafeArea>', () => {
  const { band } = parts(mount(root(h(SafeArea, { padding: 100 }, h(LowerThird, { ...sized, title: 'Name', placement: 'bottom-right' })))), 0);
  assert.equal(band.x + band.width, 1920 - 100 - 60);
  assert.equal(band.y + band.height, 1080 - 100 - 60);
});

test('in a <Sequence>, the band enters at its start, the lines follow, and all leave at its end', () => {
  const composition = mount(root(h(Sequence, { from: 10, durationInFrames: 60 },
    h(LowerThird, { title: 'Name', subtitle: 'Role' })), { durationInFrames: 80 }));
  const shown = (frame: number) => parts(composition, frame);
  assert.equal(shown(9).band, undefined);
  assert.equal(shown(10).band.opacity, 0);
  assert.deepEqual(Object.keys(shown(10).texts), []);
  // A fifth of a second apart: the title at 6 frames, the subtitle at 12.
  assert.deepEqual(Object.keys(shown(16).texts), ['Name']);
  assert.deepEqual(Object.keys(shown(22).texts), ['Name', 'Role']);
  const settled = shown(45);
  assert.deepEqual([settled.band.opacity, settled.texts.Name.opacity, settled.texts.Role.opacity], [1, 1, 1]);
  assert.equal(settled.band.x, Math.round(1080 * 0.06));
  // fade-right: the band starts left of its place.
  assert.ok(shown(12).band.x < settled.band.x);
  // The lines leave with the band, at its pace.
  const leaving = shown(62);
  assert.ok(leaving.band.opacity > 0 && leaving.band.opacity < 1);
  assert.equal(leaving.texts.Name.opacity, leaving.band.opacity);
  assert.equal(shown(69).band.opacity, 0);
  assert.equal(shown(70).band, undefined);
});

test('null cuts the band and the lines in and out', () => {
  const composition = mount(root(h(LowerThird, { ...still, title: 'Name', subtitle: 'Role' })));
  for (const frame of [0, 45, 89]) {
    const { band, texts } = parts(composition, frame);
    assert.deepEqual([band.opacity, texts.Name.opacity, texts.Role.opacity], [1, 1, 1], `frame ${frame}`);
  }
});

test('colors and fonts take <Theme> names', () => {
  const card = h(LowerThird, {
    ...still, title: 'T', subtitle: 'S', background: 'bg', color: 'ink', subtitleColor: 'soft', accent: 'accent', fontFamily: 'ui',
  });
  const { band, accent, texts } = parts(mount(root(h(Theme, {
    fonts: { ui: 'Inter' }, colors: { bg: '#000000', ink: '#ffffff', soft: '#888888', accent: '#ff3300' },
  }, card))), 0);
  assert.equal(colorOf(band.fill), '#000000');
  assert.equal(colorOf(accent.fill), '#ff3300');
  assert.deepEqual([colorOf(texts.T.style.fill), colorOf(texts.S.style.fill)], ['#ffffff', '#888888']);
  assert.deepEqual([texts.T.style.fontFamily, texts.S.style.fontFamily], ['Inter', 'Inter']);
});

test('accentWidth 0 draws no accent', () => {
  const { accent, texts } = parts(mount(root(h(LowerThird, { ...sized, accentWidth: 0, title: 'Name' }))), 0);
  assert.equal(accent, undefined);
  assert.equal(texts.Name.x, 60 + 20);
});

test('LowerThird is the same at a frame whether played to or seeked, alone or in nested sequences', () => {
  const card = h(LowerThird, { title: 'Name', subtitle: 'Role', placement: 'bottom-right' });
  const flat = mount(root(card, { durationInFrames: 40 }));
  const nested = mount(root(h(Sequence, { from: 10 }, h(Sequence, { from: 5, durationInFrames: 40 }, card)), { durationInFrames: 70 }));
  const expected = Array.from({ length: 40 }, (_, frame) => parts(flat, frame));
  for (const frame of [27, 3, 39, 0, 14, 8, 27]) {
    assert.deepEqual(parts(nested, frame + 15), expected[frame], `frame ${frame}`);
  }
});

test('LowerThird rejects unknown placements, bad sizes, and bad colors', () => {
  const render = (props: Partial<LowerThirdProps>) => () => at(mount(root(h(LowerThird, { title: 'T', ...props }))), 0);
  // Not a placement, cast to check the error.
  assert.throws(render({ placement: 'middle' as LowerThirdPlacement }), /unknown placement "middle"/);
  assert.throws(render({ titleSize: 0 }), /titleSize/);
  assert.throws(render({ accentWidth: -1 }), /accentWidth/);
  assert.throws(render({ maxWidth: 0 }), /maxWidth/);
  assert.throws(render({ accent: 'nope' }), /accent "nope"/);
});
