import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';
import type { ReactElement } from 'react';

import { Composition, Text, Theme } from '@celesta.studio/react';
import type { Layer, LayerContent, ResolvedTheme, Scene } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import { emphasisContent, graphemes, resolveEmphasis, sliceEmphasis } from '../src/index.js';
import type { EmphasisOptions, TextEmphasis } from '../src/index.js';

const FAMILY = '👨‍👩‍👧';
const FLAG = '🇯🇵';
const ACCENTED = 'é';
const YOSHI = '𠮷';

type TextContent = Extract<LayerContent, { type: 'text' }>;

function render(element: ReactElement): Scene {
  const Root = () => React.createElement(Composition, { width: 640, height: 360, fps: 30, durationInFrames: 1 }, element);
  return mount(Root).renderAt({ value: 0, timescale: 30 }, null).scene;
}

function textOf(layer: Layer | undefined): TextContent {
  assert.equal(layer?.content.type, 'text');
  return layer!.content as TextContent;
}

test('graphemes keeps emoji sequences, flags, combining marks, and surrogate pairs whole', () => {
  assert.deepEqual(graphemes(`A${FAMILY}${FLAG}${ACCENTED}${YOSHI}が`), ['A', FAMILY, FLAG, ACCENTED, YOSHI, 'が']);
  assert.deepEqual(graphemes(''), []);
});

test('resolveEmphasis counts clusters, sorts, and looks up named styles', () => {
  const text = `HPが1${FAMILY}です`;
  const resolved = resolveEmphasis(text, [
    { start: 4, end: 5, style: { fontWeight: 800 } },
    { start: 3, end: 4, style: 'danger' },
  ], { styles: { danger: { color: '#ff6060' } } });
  assert.deepEqual(resolved, [
    { start: 3, end: 4, text: '1', name: 'danger', style: { color: '#ff6060' } },
    { start: 4, end: 5, text: FAMILY, style: { fontWeight: 800 } },
  ]);
});

test('resolveEmphasis resolves <Theme> color names it is given', () => {
  const theme: ResolvedTheme = { fonts: {}, colors: { accent: '#28a34a' } };
  const [entry] = resolveEmphasis('abc', [{ start: 0, end: 1, style: { color: 'accent' } }], { theme });
  assert.equal(entry.style.color, '#28a34a');
  assert.throws(
    () => resolveEmphasis('abc', [{ start: 0, end: 1, style: { color: 'accent' } }]),
    /emphasis 0 style color "accent" is neither/,
  );
});

test('resolveEmphasis refuses ranges, styles, and overlaps it cannot draw', () => {
  const what = 'line "danger"';
  // Wrong on purpose: the checks are for data that comes in untyped.
  const bad = (emphasis: unknown, pattern: RegExp, options: EmphasisOptions = {}) =>
    assert.throws(() => resolveEmphasis(`HP${FAMILY}`, emphasis as TextEmphasis[], { what, ...options }), pattern);
  bad([{ start: 0, end: 4, style: {} }], /line "danger": emphasis 0 \(0–4\) must cover at least one of the text's 3 grapheme clusters/);
  bad([{ start: 2, end: 2, style: {} }], /emphasis 0 \(2–2\)/);
  bad([{ start: -1, end: 1, style: {} }], /emphasis 0 \(-1–1\)/);
  bad([{ start: 0.5, end: 1, style: {} }], /whole-number start and end/);
  bad([{ start: 0, end: 1, style: 'loud' }], /emphasis 0 names the unknown style "loud" \(known: "soft"\)/, { styles: { soft: {} } });
  bad([{ start: 0, end: 1, style: { color: '#ff6060', fontSize: 40 } }], /unknown property "fontSize"/);
  bad([{ start: 0, end: 1, style: { color: 'red' } }], /color "red" is neither/);
  bad([{ start: 0, end: 1, style: { fontWeight: 0 } }], /fontWeight must be a whole number from 1 to 1000/);
  bad([{ start: 0, end: 1, style: 'x' }], /style "x" has unknown property "underline"/, { styles: { x: { underline: true } as never } });
  bad([{ start: 0, end: 2, style: {} }, { start: 1, end: 3, style: {} }], /emphasis 1 \(1–3\) overlaps emphasis 0 \(0–2\)/);
  bad({ start: 0 }, /line "danger": emphasis must be a list/);
});

test('emphasisContent draws as <Span> runs over the right code points', () => {
  // The family emoji is 5 code points; the run after it starts at code point 6.
  const text = `${FAMILY}x${YOSHI}y`;
  const content = emphasisContent(text, [
    { start: 1, end: 2, style: { color: '#ff6060' } },
    { start: 2, end: 3, style: { fontWeight: 700 } },
  ]);
  const layer = textOf(render(React.createElement(Text, { style: { fontSize: 20 }, children: content })).layers[0]);
  assert.equal(layer.text, text);
  assert.deepEqual(layer.style.colorRuns, [{ start: 5, end: 6, color: '#ff6060' }]);
  assert.deepEqual(layer.style.fontRuns, [{ start: 6, end: 7, fontWeight: 700 }]);
});

test('emphasisContent with no emphasis is the text itself', () => {
  assert.equal(emphasisContent('plain', undefined), 'plain');
  assert.equal(emphasisContent('plain', []), 'plain');
});

test('emphasisContent inside <Theme> takes resolved colors', () => {
  const theme: ResolvedTheme = { fonts: {}, colors: { hot: '#ff0000' } };
  const content = emphasisContent('ab', [{ start: 1, end: 2, style: 'warn' }], { theme, styles: { warn: { color: 'hot' } } });
  const scene = render(React.createElement(Theme, { colors: { hot: '#00ff00' } }, React.createElement(Text, { children: content })));
  // The style was resolved against the theme given, not the one it is drawn in.
  assert.deepEqual(textOf(scene.layers[0]).style.colorRuns, [{ start: 1, end: 2, color: '#ff0000' }]);
});

test('sliceEmphasis cuts emphasis to part of the text and rebases it', () => {
  const resolved = resolveEmphasis('abcdef', [
    { start: 0, end: 2, style: 'a' },
    { start: 3, end: 6, style: {} },
  ], { styles: { a: { fontWeight: 700 } } });
  assert.deepEqual(sliceEmphasis(resolved, 1, 4), [
    { start: 0, end: 1, text: 'b', name: 'a', style: { fontWeight: 700 } },
    { start: 2, end: 3, text: 'd', style: {} },
  ]);
  assert.deepEqual(sliceEmphasis(resolved, 2, 3), []);
});
