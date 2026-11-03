import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';
import type { ReactNode } from 'react';

import { Assets, Composition, Group, Span, Text, useTextMetrics } from '@celesta.studio/react';
import type { AssetReference, FontFamily, Layer, LayerContent, TextMetrics } from '@celesta.studio/react';
import { TextBox, TextReveal, useFitText, useTypewriter } from '@celesta.studio/text';
import type { FitTextOptions, Typewriter } from '@celesta.studio/text';
import { Character, CharacterView, Dialogue, DialogueSeries, planDialogue } from '@celesta.studio/character';
import type { CharacterViewReference, SubtitleRenderProps } from '@celesta.studio/character';
import { flattenTextContent, mount, setTextMeasurer, sliceTextRuns, withTextRuns } from '@celesta.studio/react/internal';
import type { EntryComponent, MeasureTextRequest } from '@celesta.studio/react/internal';
import { contentOf } from './layer-content.mjs';

type TextContent = Extract<LayerContent, { type: 'text' }>;

const h = React.createElement;
const scene = (Root: EntryComponent) => mount(Root).renderAt({ value: 0, timescale: 30 }, null).scene;
const frame = (...children: ReactNode[]) => () => h(Composition, { width: 400, height: 200, fps: 30, durationInFrames: 1 }, ...children);

test('flattening counts code points and resolves nested spans', () => {
  const flat = flattenTextContent([
    '😀a',
    h(Span, { style: { fill: '#ff0000', fontWeight: 700 } },
      'bc',
      h(Span, { style: { fontFamily: 'Bebas Neue', fill: { type: 'solid', color: '#00ff00' } } }, 'd')),
    h(React.Fragment, null, 'e', null, false, 3),
  ], '<Text> children');
  assert.equal(flat.text, '😀abcde3');
  assert.deepEqual(flat.colorRuns, [
    { start: 2, end: 4, color: '#ff0000' },
    { start: 4, end: 5, color: '#00ff00' },
  ]);
  assert.deepEqual(flat.fontRuns, [
    { start: 2, end: 4, fontWeight: 700 },
    { start: 4, end: 5, fontWeight: 700, fontFamily: 'Bebas Neue' },
  ]);
});

test('adjacent runs with the same attributes merge', () => {
  const bold = { fontWeight: 700 };
  const flat = flattenTextContent([h(Span, { style: bold }, 'ab'), h(Span, { style: bold }, 'cd')], '<Text> children');
  assert.deepEqual(flat.fontRuns, [{ start: 0, end: 4, fontWeight: 700 }]);
});

test('a span takes a font stack, and equal stacks merge', () => {
  const stack = ['JetBrains Mono', 'Noto Sans JP'];
  const flat = flattenTextContent([
    h(Span, { style: { fontFamily: stack } }, 'ab'),
    h(Span, { style: { fontFamily: [...stack] } }, 'cd'),
    h(Span, { style: { fontFamily: 'JetBrains Mono' } }, 'e'),
  ], '<Text> children');
  assert.deepEqual(flat.fontRuns, [
    { start: 0, end: 4, fontFamily: stack },
    { start: 4, end: 5, fontFamily: 'JetBrains Mono' },
  ]);
  // The run keeps its own copy of the list.
  stack.push('Mutated');
  assert.deepEqual(flat.fontRuns[0].fontFamily, ['JetBrains Mono', 'Noto Sans JP']);
  for (const fontFamily of ['', ' ', [], ['Inter', ''], ['Inter', 3], 3]) {
    // Deliberately not font families.
    assert.throws(() => flattenTextContent(h(Span, { style: { fontFamily: fontFamily as FontFamily } }, 'a'), '<Text> children'),
      /<Span> fontFamily must be a non-empty string or a non-empty array of them/, JSON.stringify(fontFamily));
  }
});

test('empty spans emit no runs', () => {
  const flat = flattenTextContent(['a', h(Span, { style: { fontWeight: 700 } }), h(Span, { style: { fill: '#ff0000' } }, null, false), 'b'], '<Text> children');
  assert.deepEqual(flat, { text: 'ab', colorRuns: [], fontRuns: [] });
});

test('flattening rejects what it cannot style', () => {
  assert.throws(() => flattenTextContent(h(Span, { style: { fill: { type: 'linear', start: { x: 0, y: 0 }, end: { x: 1, y: 0 }, stops: [] } } }, 'a'), '<Text> children'),
    /<Span> fill must be a color or a solid paint/);
  assert.throws(() => flattenTextContent(h(Span, { style: { fontWeight: 650.5 } }, 'a'), '<Text> children'),
    /<Span> fontWeight must be a whole number from 1 to 1000/);
  const Wrapped = () => h(Span, null, 'a');
  assert.throws(() => flattenTextContent(h(Wrapped), '<Text> children'),
    /<Text> children must be a string, a number, or an array of those; style part of the text with <Span>/);
  // The renderer reads only #RRGGBB and #RRGGBBAA, so anything else fails here, not at export.
  for (const fill of ['#f00', 'red', '#12345', { type: 'solid', color: 'rgb(0, 0, 0)' }] as const) {
    assert.throws(() => flattenTextContent(h(Span, { style: { fill } }, 'a'), '<Text> children'),
      /<Span> fill must be a #RRGGBB or #RRGGBBAA color/, JSON.stringify(fill));
  }
  assert.deepEqual(flattenTextContent(h(Span, { style: { fill: '#ffd44780' } }, 'a'), 'x').colorRuns,
    [{ start: 0, end: 1, color: '#ffd44780' }]);
});

test('withTextRuns keeps plain styles as they are and refuses mixed run sources', () => {
  const style = { fontSize: 20 };
  assert.equal(withTextRuns(style, flattenTextContent('plain', 'x'), '<Text>'), style);
  const flat = flattenTextContent(h(Span, { style: { fill: '#ff0000', fontWeight: 700 } }, 'a'), 'x');
  assert.deepEqual(withTextRuns(style, flat, '<Text>'), {
    fontSize: 20, colorRuns: [{ start: 0, end: 1, color: '#ff0000' }], fontRuns: [{ start: 0, end: 1, fontWeight: 700 }],
  });
  assert.throws(() => withTextRuns({ colorRuns: [{ start: 0, end: 1, color: '#fff' }] }, flat, '<Text>'),
    /<Text> cannot combine style.colorRuns with <Span> fill/);
  assert.throws(() => withTextRuns({ fontRuns: [{ start: 0, end: 1, fontWeight: 400 }] }, flat, '<Text>'),
    /<Text> cannot combine style.fontRuns with <Span> fontWeight, fontFamily, or fontFeatures/);
  // Each list is refused only against spans that write it.
  const colorOnly = flattenTextContent(h(Span, { style: { fill: '#ff0000' } }, 'a'), 'x');
  assert.deepEqual(withTextRuns({ fontRuns: [{ start: 0, end: 1, fontWeight: 700 }] }, colorOnly, '<Text>').colorRuns,
    [{ start: 0, end: 1, color: '#ff0000' }]);
});

test('sliceTextRuns clips and rebases runs', () => {
  const flat = flattenTextContent(['ab', h(Span, { style: { fontWeight: 700, fill: '#ff0000' } }, 'c\nd'), 'e'], 'x');
  assert.deepEqual(sliceTextRuns(flat, 4, 6), {
    text: 'de', colorRuns: [{ start: 0, end: 1, color: '#ff0000' }], fontRuns: [{ start: 0, end: 1, fontWeight: 700 }],
  });
});

test('a span rendered outside text content throws', () => {
  const Root = () => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 1 }, h(Group, null, h(Span, null, 'x')));
  assert.throws(() => mount(Root).renderAt({ value: 0, timescale: 30 }, null), /<Span> is only valid inside the content of/);
});

test('Text writes spans as runs and leaves plain text as it was', () => {
  const [plain, nothing, spanned] = scene(frame(
    h(Text, { id: 'plain', style: { fontSize: 20 }, children: ['Hello ', 3] }),
    h(Text, { id: 'nothing', children: ['a', null, false] }),
    h(Text, { id: 'spanned', style: { fontSize: 20 }, children: ['速い、', h(Span, { style: { fill: '#28A34A', fontWeight: 700 } }, 'カンタン'), '。'] }),
  )).layers;
  assert.deepEqual(contentOf(plain, 'text').style, { fontSize: 20 });
  assert.equal(contentOf(plain, 'text').text, 'Hello 3');
  assert.equal(contentOf(nothing, 'text').text, 'a');
  assert.equal(contentOf(spanned, 'text').text, '速い、カンタン。');
  assert.deepEqual(contentOf(spanned, 'text').style, {
    fontSize: 20,
    colorRuns: [{ start: 3, end: 7, color: '#28A34A' }],
    fontRuns: [{ start: 3, end: 7, fontWeight: 700 }],
  });
});

test('TextBox measures and draws its spans', () => {
  const requests: MeasureTextRequest[] = [];
  setTextMeasurer(async () => { throw new Error('unused'); }, (request): TextMetrics => {
    requests.push(request);
    const fontSize = request.style.fontSize ?? 0;
    return { width: Array.from(request.text).length * fontSize, height: fontSize, ascent: 8, descent: 2, lineHeight: 10,
      capHeight: 0, xHeight: 0, lines: 1, glyphs: [] };
  });
  const [box] = scene(frame(h(TextBox, { width: 400, height: 100, minFontSize: 10, maxFontSize: 40,
    children: ['a', h(Span, { style: { fontWeight: 700 } }, 'b')] }))).layers;
  assert.ok(requests.length > 0);
  assert.ok(requests.every((request) => request.style.fontRuns?.[0]?.fontWeight === 700));
  const text = contentOf(contentOf(box, 'group').layers[0], 'text');
  assert.equal(text.text, 'ab');
  assert.deepEqual(text.style.fontRuns, [{ start: 1, end: 2, fontWeight: 700 }]);
});

test('useFitText refuses style.colorRuns with a Span fill, as fitText does', () => {
  const metrics = (): TextMetrics => ({ width: 1, height: 1, ascent: 1, descent: 0, lineHeight: 1, capHeight: 0, xHeight: 0, lines: 1, glyphs: [] });
  setTextMeasurer(async () => metrics(), metrics);
  const options: FitTextOptions = { width: 400, height: 100, minFontSize: 10, maxFontSize: 40, style: { colorRuns: [{ start: 0, end: 1, color: '#ffffff' }] } };
  const content = ['a', h(Span, { style: { fill: '#ff0000' } }, 'b')];
  function Probe() {
    useFitText(content, options);
    return null;
  }
  assert.throws(() => scene(frame(h(Probe))), /useFitText\(\) cannot combine style.colorRuns with <Span> fill/);
  function WeightProbe() {
    useFitText(['a', h(Span, { style: { fontWeight: 700 } }, 'b')], { ...options, style: { fontRuns: [{ start: 0, end: 1, fontWeight: 400 }] } });
    return null;
  }
  assert.throws(() => scene(frame(h(WeightProbe))), /useFitText\(\) cannot combine style.fontRuns with <Span> fontWeight, fontFamily, or fontFeatures/);
});

test('subtitles carry spans to the text layer and to render', () => {
  const requests: MeasureTextRequest[] = [];
  setTextMeasurer(async () => { throw new Error('unused'); }, (request): TextMetrics => {
    requests.push(request);
    return { width: 10, height: 10, ascent: 8, descent: 2, lineHeight: 10, capHeight: 0, xHeight: 0, lines: 1, glyphs: [] };
  });
  const portrait = { defaultExpression: 'normal', expressions: { normal: 'mira-normal.png' } };
  let seen: SubtitleRenderProps | undefined;
  const make = (render: ((props: SubtitleRenderProps) => ReactNode) | null) => {
    const character = React.createRef<AssetReference>();
    const view = React.createRef<CharacterViewReference>();
    return () => h(Composition, { width: 400, height: 200, fps: 30, durationInFrames: 1 },
      h(Assets, null, h(Character, { ref: character, name: 'mira', portrait, subtitle: { style: { fontSize: 20 }, ...(render ? { render } : {}) } })),
      h(CharacterView, { ref: view, character }),
      h(Dialogue, { character: view, children: ['ぜひ', h(Span, { style: { fill: '#ff0000' } }, '一度'), 'お試しを'] }));
  };
  const plain = JSON.stringify(scene(make(null)));
  assert.match(plain, /"colorRuns":\[\{"start":2,"end":4,"color":"#ff0000"\}\]/);
  scene(make((props) => { seen = props; return h(Text, { style: props.style, children: props.content }); }));
  assert.ok(seen);
  assert.equal(seen.text, 'ぜひ一度お試しを');
  assert.equal(seen.style.colorRuns, undefined);
  assert.ok(Array.isArray(seen.content));
  assert.ok(React.isValidElement(seen.content[1]));
  // `metrics` is measured on first read only.
  assert.ok(!requests.some((request) => request.text === 'ぜひ一度お試しを'));
  assert.equal(seen.metrics.width, 10);
  assert.ok(requests.some((request) => request.text === 'ぜひ一度お試しを'));
});

test('DialogueSeries lines may hold spans', async () => {
  const metrics = (): TextMetrics => ({ width: 1, height: 1, ascent: 1, descent: 0, lineHeight: 1, capHeight: 0, xHeight: 0, lines: 1, glyphs: [] });
  setTextMeasurer(async () => metrics(), metrics);
  const plan = await planDialogue(
    [{ id: 'one', text: ['ぜひ', h(Span, { style: { fill: '#ff0000' } }, '一度')], durationInFrames: 10, speaker: 'mira' }],
    { fps: 30 },
  );
  const character = React.createRef<AssetReference>();
  const view = React.createRef<CharacterViewReference>();
  const Root = () => h(Composition, { width: 400, height: 200, fps: 30, durationInFrames: plan.durationInFrames },
    h(Assets, null, h(Character, {
      ref: character, name: 'mira',
      portrait: { defaultExpression: 'calm', expressions: { calm: 'mira-calm.png' } },
      subtitle: { style: { fontSize: 20 } },
    })),
    h(CharacterView, { ref: view, character }),
    h(DialogueSeries, { plan, views: { mira: view } }));
  const json = JSON.stringify(scene(Root));
  assert.match(json, /"text":"ぜひ一度"/);
  assert.match(json, /"colorRuns":\[\{"start":2,"end":4,"color":"#ff0000"\}\]/);
});

test('useTextMetrics measures spans with their font runs; strings measure as before', () => {
  const requests: MeasureTextRequest[] = [];
  setTextMeasurer(async () => { throw new Error('unused'); }, (request): TextMetrics => {
    requests.push(request);
    return { width: 1, height: 1, ascent: 1, descent: 0, lineHeight: 1, capHeight: 0, xHeight: 0, lines: 1, glyphs: [] };
  });
  function Probe() {
    useTextMetrics('plain', { fontSize: 10, fill: { type: 'solid', color: '#fff' } });
    useTextMetrics(['a', h(Span, { style: { fontWeight: 700, fill: '#ff0000' } }, 'b')], { fontSize: 10 });
    return null;
  }
  scene(frame(h(Probe)));
  assert.deepEqual(requests[0].style, { fontSize: 10 });
  assert.equal(requests[1].text, 'ab');
  assert.deepEqual(requests[1].style, { fontSize: 10, fontRuns: [{ start: 1, end: 2, fontWeight: 700 }] });
});

test('useTypewriter counts the code points of rich content', () => {
  let typed: Typewriter | undefined;
  function Probe() {
    typed = useTypewriter(['速い、', h(Span, { style: { fontWeight: 700 } }, 'カンタン')], { from: 0, framesPerChar: 1 });
    return null;
  }
  const Root = () => h(Composition, { width: 400, height: 200, fps: 30, durationInFrames: 30 }, h(Probe));
  mount(Root).renderAt({ value: 4, timescale: 30 }, null);
  assert.ok(typed);
  assert.equal(typed.length, 5);
  assert.equal(typed.text, '速い、カン');
  assert.equal(typed.done, false);
});

/** Text layers, depth first. */
function textContents(layers: Layer[]): TextContent[] {
  return layers.flatMap((layer) =>
    layer.content.type === 'text' ? [layer.content] : layer.content.type === 'group' ? textContents(layer.content.layers) : []);
}

test('TextReveal splits a run that crosses a line', () => {
  const texts = textContents(scene(frame(h(TextReveal, { style: { fontSize: 20 },
    children: ['ab', h(Span, { style: { fontWeight: 700, fill: '#ff0000' } }, 'c\nd'), 'e'] }))).layers);
  assert.deepEqual(texts.map((text) => text.text), ['abc', 'de']);
  assert.deepEqual(texts[0].style.fontRuns, [{ start: 2, end: 3, fontWeight: 700 }]);
  assert.deepEqual(texts[1].style.fontRuns, [{ start: 0, end: 1, fontWeight: 700 }]);
  assert.deepEqual(texts[1].style.colorRuns, [{ start: 0, end: 1, color: '#ff0000' }]);
});

test('TextReveal keeps string children as they were', () => {
  const texts = textContents(scene(frame(h(TextReveal, { style: { fontSize: 20 }, children: 'one\ntwo' }))).layers);
  assert.deepEqual(texts.map((text) => [text.text, text.style]), [['one', { fontSize: 20 }], ['two', { fontSize: 20 }]]);
});
