import assert from 'node:assert/strict';
import { beforeEach, test } from 'vitest';
import * as React from 'react';
import type { ReactElement } from 'react';

import { Composition, Rect, Sequence, Text, Theme, useCurrentFrame } from '@celesta.studio/react';
import type { Clip, Layer, LayerContent, Scene, TextMetrics } from '@celesta.studio/react';
import { mount, setTextMeasurer } from '@celesta.studio/react/internal';
import type { MeasureTextRequest, MountedComposition } from '@celesta.studio/react/internal';
import { CaptionTrack } from '../src/index.js';
import type { CaptionCue, CaptionRenderProps, CaptionTrackProps } from '../src/index.js';

const h = React.createElement;

// Every character is `fontSize` wide; lines break at `\n` and wrap at maxWidth.
function measure({ text, style, maxWidth }: MeasureTextRequest): TextMetrics {
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
    descent: lineHeight - size,
    lineHeight,
    capHeight: size * 0.7,
    xHeight: size * 0.5,
    lines: lines.length,
    glyphs: [],
  };
}

beforeEach(() => setTextMeasurer(async (request) => measure(request), measure));

const root = (child: ReactElement, durationInFrames = 300) => () =>
  h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames }, child);
const at = (composition: MountedComposition, frame: number): Scene =>
  composition.renderAt({ value: frame, timescale: 30 }, null).scene;

/** Where a layer ends up: its position and opacity summed through its groups. */
interface Placed {
  x: number;
  y: number;
  opacity: number;
}
type Leaf = Placed & ({ kind: 'layer'; content: Exclude<LayerContent, { type: 'group' }> } | { kind: 'clip' });
type TextLeaf = Placed & Extract<LayerContent, { type: 'text' }>;
type RectLeaf = Placed & Extract<LayerContent, { type: 'rect' }>;

/** Every non-group layer, and every group's clip, in drawing order. */
function leaves(layers: Layer[], x = 0, y = 0, opacity = 1, out: Leaf[] = []): Leaf[] {
  for (const layer of layers) {
    const placed = { x: x + layer.transform.position.x, y: y + layer.transform.position.y, opacity: opacity * layer.opacity };
    if (layer.content.type === 'group') {
      if (layer.content.clip) out.push({ ...placed, kind: 'clip' });
      leaves(layer.content.layers, placed.x, placed.y, placed.opacity, out);
    } else out.push({ ...placed, kind: 'layer', content: layer.content });
  }
  return out;
}

const drawn = (composition: MountedComposition, frame: number) => leaves(at(composition, frame).layers);
const textsAt = (composition: MountedComposition, frame: number): TextLeaf[] =>
  drawn(composition, frame).flatMap((l) => l.kind === 'layer' && l.content.type === 'text'
    ? [{ ...l.content, x: l.x, y: l.y, opacity: l.opacity }] : []);
const rectsAt = (composition: MountedComposition, frame: number): RectLeaf[] =>
  drawn(composition, frame).flatMap((l) => l.kind === 'layer' && l.content.type === 'rect'
    ? [{ ...l.content, x: l.x, y: l.y, opacity: l.opacity }] : []);
const clipsAt = (composition: MountedComposition, frame: number) => drawn(composition, frame).filter((l) => l.kind === 'clip');
const shown = (composition: MountedComposition, frame: number) => textsAt(composition, frame).map((l) => l.text);
const track = (props: CaptionTrackProps) => h(CaptionTrack, props);

const cues: CaptionCue[] = [
  { id: 'intro', from: 0, durationInFrames: 60, text: 'こんにちは' },
  { id: 'next', from: 60, durationInFrames: 90, text: '動画を作ります' },
  { id: 'late', from: 180, durationInFrames: 30, text: 'Bye' },
];

const still = { enter: null, exit: null } as const;

test('cues switch on their boundary frames from data alone, with gaps showing nothing', () => {
  const captions = mount(root(track({ cues, ...still })));
  assert.deepEqual(shown(captions, 0), ['こんにちは']);
  assert.deepEqual(shown(captions, 59), ['こんにちは']);
  assert.deepEqual(shown(captions, 60), ['動画を作ります']);
  assert.deepEqual(shown(captions, 149), ['動画を作ります']);
  assert.deepEqual(shown(captions, 150), []);
  assert.deepEqual(shown(captions, 179), []);
  assert.deepEqual(shown(captions, 180), ['Bye']);
  assert.deepEqual(shown(captions, 210), []);
  // A band (the Panel's background rect) behind every caption, and none in the gap.
  assert.equal(rectsAt(captions, 30).length, 1);
  assert.equal(rectsAt(captions, 160).length, 0);
});

test('the default band sits at the bottom centre, sized from the measured text', () => {
  const captions = mount(root(track({ cues, ...still, fontSize: 40 })));
  const [band] = rectsAt(captions, 10);
  const [text] = textsAt(captions, 10);
  // 5 characters × 40 px, padding 0.6 em at the sides and 0.3 em above and below; line height 1.4.
  assert.equal(band.width, 200 + 48);
  assert.equal(band.height, 56 + 24);
  assert.equal(band.x, (1920 - 248) / 2);
  // The margin is 6% of 1080.
  assert.equal(band.y + band.height, 1080 - 65);
  assert.equal(text.maxWidth, Math.round(0.8 * (1920 - 130)));
  assert.equal(text.style.align, 'center');
  assert.equal(text.style.lineHeight, 56);
  assert.equal(text.style.lineBreak, 'phrase');
});

test('nested in a Sequence, cues count from its start', () => {
  const captions = mount(root(h(Sequence, { from: 100 }, h(Sequence, { from: 20 }, track({ cues, ...still })))));
  assert.deepEqual(shown(captions, 119), []);
  assert.deepEqual(shown(captions, 120), ['こんにちは']);
  assert.deepEqual(shown(captions, 180), ['動画を作ります']);
});

test('seeking in any order draws the same frames', () => {
  const frames = [200, 0, 75, 59, 60, 150, 3];
  const forward = mount(root(track({ cues })));
  const expected = [...frames].sort((a, b) => a - b).map((frame) => [frame, JSON.stringify(drawn(forward, frame))] as const);
  const shuffled = mount(root(track({ cues })));
  const got = new Map(frames.map((frame) => [frame, JSON.stringify(drawn(shuffled, frame))]));
  for (const [frame, json] of expected) assert.equal(got.get(frame), json, `frame ${frame}`);
});

test('Japanese captions wrap to the line limit; past it they clip, or fail with overflow error', () => {
  const long: CaptionCue = { from: 0, durationInFrames: 30, text: 'あいうえおかきくけこさしすせそ' };
  const captions = mount(root(track({ cues: [long], ...still, fontSize: 40, maxWidth: 240, maxLines: 2 })));
  // Six characters a line, three lines measured, two shown.
  assert.equal(rectsAt(captions, 0)[0].height, 2 * 56 + 24);
  assert.equal(clipsAt(captions, 0).length, 1);
  assert.throws(
    () => at(mount(root(track({ cues: [{ ...long, id: 'wall' }], ...still, fontSize: 40, maxWidth: 240, overflow: 'error' }))), 0),
    /<CaptionTrack> cue 0 \("wall"\) "あいうえお.*" does not fit 2 lines of 240 px even at 40 px/,
  );
  // Two lines fit and nothing is clipped; `\n` breaks a line.
  const two = mount(root(track({ cues: [{ ...long, text: 'あいう\nえお' }], ...still, fontSize: 40, maxWidth: 240 })));
  assert.equal(rectsAt(two, 0)[0].height, 2 * 56 + 24);
  assert.equal(clipsAt(two, 0).length, 0);
});

test('minFontSize lets a long caption shrink to fit maxLines', () => {
  const captions = mount(root(track({
    cues: [{ from: 0, durationInFrames: 30, text: 'あいうえおかきくけこさしすせそ' }], ...still,
    fontSize: 40, minFontSize: 20, maxWidth: 240, maxLines: 2,
  })));
  const size = textsAt(captions, 0)[0].style.fontSize ?? 0;
  assert.ok(size < 40 && size >= 20);
});

test('emphasis draws as runs over grapheme clusters, with named styles and theme colors', () => {
  const captions = mount(root(h(Theme, { colors: { warn: '#ffd166' } }, track({
    ...still,
    emphasisStyles: { hot: { color: 'warn', fontWeight: 800 } },
    cues: [{ from: 0, durationInFrames: 30, text: 'HPが👨‍👩‍👧1', emphasis: [{ start: 3, end: 5, style: 'hot' }] }],
  }))));
  const [text] = textsAt(captions, 0);
  assert.deepEqual(text.style.colorRuns, [{ start: 3, end: 9, color: '#ffd166' }]);
  assert.deepEqual(text.style.fontRuns, [{ start: 3, end: 9, fontWeight: 800 }]);
  assert.throws(
    () => at(mount(root(track({ ...still, cues: [{ id: 'x', from: 0, durationInFrames: 30, text: 'ab', emphasis: [{ start: 1, end: 3, style: {} }] }] }))), 0),
    /<CaptionTrack> cue 0 \("x"\): emphasis 0 \(1–3\) must cover/,
  );
});

test('a render callback replaces the band and text without changing the cue clock', () => {
  const seen: [string | undefined, number, number, number][] = [];
  function Custom(info: CaptionRenderProps) {
    const local = useCurrentFrame();
    seen.push([info.cue.id, info.frame, local, info.durationInFrames]);
    return h(Rect, { id: 'custom', x: info.box.x, y: info.box.y, width: info.box.width, height: info.box.height, fill: '#ff0000' });
  }
  const captions = mount(root(h(Sequence, { from: 10 }, track({ cues, render: Custom }))));
  at(captions, 10);
  at(captions, 99);
  at(captions, 190);
  assert.deepEqual(seen, [
    ['intro', 0, 0, 60],
    ['next', 29, 29, 90],
    ['late', 0, 0, 30],
  ]);
  assert.deepEqual(shown(captions, 99), []);
  const plain = mount(root(h(Sequence, { from: 10 }, track({ cues, background: null }))));
  // No band, and the same cue on the same frames.
  assert.deepEqual(shown(plain, 99), ['動画を作ります']);
  assert.equal(rectsAt(plain, 99).length, 0);
  assert.deepEqual(shown(plain, 69), ['こんにちは']);
});

test('captions fade in and out, but back-to-back cues swap without a blink', () => {
  const captions = mount(root(track({ cues })));
  const opacity = (frame: number) => textsAt(captions, frame)[0].opacity;
  // 3-frame fades at 30 fps: hidden on the first frame, at rest on the third.
  assert.equal(opacity(0), 0);
  assert.ok(opacity(1) > 0 && opacity(1) < 1);
  assert.equal(opacity(2), 1);
  // "intro" ends where "next" starts: no exit, no entrance.
  assert.equal(opacity(59), 1);
  assert.equal(opacity(60), 1);
  // "next" ends before a gap, so it fades out.
  assert.ok(opacity(148) < 1);
  assert.equal(opacity(180), 0);
});

test('overlapping cues are refused unless the later one may cut the earlier', () => {
  const overlapping: CaptionCue[] = [
    { id: 'a', from: 0, durationInFrames: 60, text: 'A' },
    { id: 'b', from: 30, durationInFrames: 60, text: 'B' },
  ];
  assert.throws(
    () => at(mount(root(track({ cues: overlapping, ...still }))), 0),
    /<CaptionTrack>: cue 1 \("b"\) \(frames 30–90\) overlaps cue 0 \("a"\) \(frames 0–60\)/,
  );
  const captions = mount(root(track({ cues: overlapping, ...still, overlap: 'latest' })));
  assert.deepEqual(shown(captions, 29), ['A']);
  assert.deepEqual(shown(captions, 30), ['B']);
});

test('bad cues and props fail with the cue named', () => {
  // Wrong on purpose: the checks are for data that comes in untyped.
  const bad = (props: Record<string, unknown>, pattern: RegExp) =>
    assert.throws(() => at(mount(root(track({ ...still, ...props } as unknown as CaptionTrackProps))), 0), pattern);
  bad({ cues: [{ id: 'q', from: 0, durationInFrames: 0, text: 'x' }] }, /cue 0 \("q"\) needs a whole-number `durationInFrames` of at least 1/);
  bad({ cues: [{ from: 0.5, durationInFrames: 10, text: 'x' }] }, /cue 0 needs a whole-number `from`/);
  bad({ cues: [{ from: 0, durationInFrames: 10, text: 3 }] }, /cue 0 needs a string `text`/);
  bad({ cues, maxLines: 0 }, /maxLines must be a whole number of at least 1/);
  bad({ cues, color: 'nope' }, /<CaptionTrack> color "nope" is neither/);
});

test('an empty caption shows no band, and the cues go into the subtitle file', () => {
  const withEmpty: CaptionCue[] = [...cues, { from: 220, durationInFrames: 10, text: '' }];
  const captions = mount(root(track({ cues: withEmpty, ...still })));
  assert.deepEqual(drawn(captions, 225), []);
  const files = captions.collectSubtitles(0, 300).map(({ text, start, end }) => [text, start, end]);
  assert.deepEqual(files, [['こんにちは', 0, 2], ['動画を作ります', 2, 5], ['Bye', 6, 7]]);
  const silent = mount(root(track({ cues, ...still, subtitles: false })));
  assert.deepEqual(silent.collectSubtitles(0, 300), []);
});

test('a text layer of its own is untouched by the track', () => {
  const captions = mount(root(h(React.Fragment, null, h(Text, { id: 'title', children: 'Title' }), track({ cues, ...still }))));
  assert.deepEqual(shown(captions, 0), ['Title', 'こんにちは']);
});

// Word highlight and paging.

const runsAt = (composition: MountedComposition, frame: number) => textsAt(composition, frame)[0].style.colorRuns ?? [];
const words = (text: string, froms: number[], durationInFrames?: number) =>
  text.split(' ').map((word, index) => ({ text: word, from: froms[index], ...(durationInFrames ? { durationInFrames } : {}) }));
const lit = { color: '#ff0000', spokenColor: '#888888', upcomingColor: '#444444', transitionInFrames: 0 } as const;

test('the word being spoken is lit by its frames, the others in their spoken and upcoming colors', () => {
  const captions = mount(root(track({
    ...still, highlight: lit,
    cues: [{ from: 0, durationInFrames: 40, text: 'one two three', words: words('one two three', [0, 10, 20]) }],
  })));
  assert.deepEqual(runsAt(captions, 0), [
    { start: 0, end: 3, color: '#ff0000' }, { start: 4, end: 7, color: '#444444' }, { start: 8, end: 13, color: '#444444' },
  ]);
  assert.deepEqual(runsAt(captions, 15), [
    { start: 0, end: 3, color: '#888888' }, { start: 4, end: 7, color: '#ff0000' }, { start: 8, end: 13, color: '#444444' },
  ]);
  assert.deepEqual(runsAt(captions, 39).at(-1), { start: 8, end: 13, color: '#ff0000' });
  // Without the other colors, only the word being spoken changes.
  const plain = mount(root(track({
    ...still, highlight: {},
    cues: [{ from: 0, durationInFrames: 40, text: 'one two three', words: words('one two three', [0, 10, 20]) }],
  })));
  assert.deepEqual(runsAt(plain, 15), [{ start: 4, end: 7, color: '#ffd166' }]);
});

test('a word with a duration goes dark after it, and punctuation goes with the word before it', () => {
  const captions = mount(root(track({
    ...still, highlight: lit,
    cues: [{ from: 0, durationInFrames: 40, text: 'Wait, what?', words: [{ text: 'Wait', from: 0, durationInFrames: 5 }, { text: ' what', from: 20 }] }],
  })));
  assert.deepEqual(runsAt(captions, 4)[0], { start: 0, end: 5, color: '#ff0000' });
  assert.deepEqual(runsAt(captions, 10), [{ start: 0, end: 5, color: '#888888' }, { start: 6, end: 11, color: '#444444' }]);
  assert.deepEqual(runsAt(captions, 20)[1], { start: 6, end: 11, color: '#ff0000' });
});

test('without words, the cue\'s frames are shared out by length, and phrase splits Japanese', () => {
  const english = mount(root(track({ ...still, highlight: lit, cues: [{ from: 0, durationInFrames: 30, text: 'aa bbbb' }] })));
  assert.deepEqual(runsAt(english, 9)[0], { start: 0, end: 2, color: '#ff0000' });
  assert.deepEqual(runsAt(english, 10)[1], { start: 3, end: 7, color: '#ff0000' });
  const japanese = mount(root(track({
    ...still, highlight: lit, wordUnit: 'phrase', cues: [{ from: 0, durationInFrames: 50, text: '今日は晴れ' }],
  })));
  assert.deepEqual(runsAt(japanese, 29), [{ start: 0, end: 3, color: '#ff0000' }, { start: 3, end: 5, color: '#444444' }]);
  assert.deepEqual(runsAt(japanese, 30), [{ start: 0, end: 3, color: '#888888' }, { start: 3, end: 5, color: '#ff0000' }]);
  // Given words join into the phrase they are part of.
  const joined = mount(root(track({
    ...still, highlight: lit, wordUnit: 'phrase',
    cues: [{ from: 0, durationInFrames: 50, text: '今日は晴れ', words: [{ text: '今日', from: 0 }, { text: 'は', from: 10 }, { text: '晴れ', from: 20 }] }],
  })));
  assert.deepEqual(runsAt(joined, 15)[0], { start: 0, end: 3, color: '#ff0000' });
  // As single words, "は" is lit on its own.
  const single = mount(root(track({
    ...still, highlight: lit,
    cues: [{ from: 0, durationInFrames: 50, text: '今日は晴れ', words: [{ text: '今日', from: 0 }, { text: 'は', from: 10 }, { text: '晴れ', from: 20 }] }],
  })));
  assert.deepEqual(runsAt(single, 15)[1], { start: 2, end: 3, color: '#ff0000' });
});

test('pages hold at most maxWords words, end at sentences and pauses, and fit maxLines', () => {
  const byWords = mount(root(track({
    ...still, paging: { maxWords: 2 },
    cues: [{ from: 0, durationInFrames: 60, text: 'a b c d e', words: words('a b c d e', [5, 10, 20, 30, 40]) }],
  })));
  assert.deepEqual([0, 19, 20, 39, 40, 59].map((frame) => shown(byWords, frame)[0]), ['a b', 'a b', 'c d', 'c d', 'e', 'e']);
  const bySentence = mount(root(track({ ...still, highlight: lit, cues: [{ from: 0, durationInFrames: 60, text: 'Hi. Go now' }] })));
  assert.deepEqual([shown(bySentence, 0)[0], shown(bySentence, 59)[0]], ['Hi.', 'Go now']);
  const unbroken = mount(root(track({ ...still, paging: { sentences: false }, cues: [{ from: 0, durationInFrames: 60, text: 'Hi. Go now' }] })));
  assert.deepEqual(shown(unbroken, 59), ['Hi. Go now']);
  const byPause = mount(root(track({
    ...still, paging: { pause: 10 },
    cues: [{ from: 0, durationInFrames: 60, text: 'a b c', words: words('a b c', [0, 5, 40], 5) }],
  })));
  assert.deepEqual([shown(byPause, 39)[0], shown(byPause, 40)[0]], ['a b', 'c']);
  // Six characters a line: "aaa bbb" takes two lines, so each page holds one word.
  const byLines = mount(root(track({
    ...still, paging: {}, fontSize: 40, maxWidth: 240, maxLines: 1,
    cues: [{ from: 0, durationInFrames: 30, text: 'aaa bbb ccc' }],
  })));
  assert.deepEqual([0, 10, 20].map((frame) => shown(byLines, frame)[0]), ['aaa', 'bbb', 'ccc']);
  // Emphasis is cut to the page.
  const emphasized = mount(root(track({
    ...still, paging: { maxWords: 1 },
    cues: [{ from: 0, durationInFrames: 20, text: 'ab cd', emphasis: [{ start: 1, end: 4, style: { color: '#00ff00' } }] }],
  })));
  assert.deepEqual(runsAt(emphasized, 0), [{ start: 1, end: 2, color: '#00ff00' }]);
  assert.deepEqual(runsAt(emphasized, 19), [{ start: 0, end: 1, color: '#00ff00' }]);
});

// Every character is `fontSize` wide, with glyphs placed as `align` puts them in `maxWidth`.
function measureGlyphs(request: MeasureTextRequest): TextMetrics {
  const metrics = measure(request);
  const size = request.style.fontSize ?? 10;
  const per = request.maxWidth === undefined ? Infinity : Math.max(1, Math.floor(request.maxWidth / size));
  const chars = Array.from(request.text);
  const lines = Math.ceil(chars.length / per);
  const glyphs = chars.map((text, index) => {
    const line = Math.floor(index / per);
    const count = Math.min(per, chars.length - line * per);
    const room = request.maxWidth === undefined ? 0 : request.maxWidth - count * size;
    const shift = request.style.align === 'center' ? room / 2 : request.style.align === 'right' ? room : 0;
    return { start: index, end: index + 1, rtl: false, text, x: shift + (index - line * per) * size, width: size, line, inkAscent: size, inkDescent: 0 };
  });
  return { ...metrics, lines, glyphs };
}

const isBand = (rect: RectLeaf) => rect.fill?.type === 'solid' && rect.fill.color === '#00ff00';

function scaleOf(layers: Layer[], text: string): number | undefined {
  for (const layer of layers) {
    if (layer.content.type !== 'group') continue;
    if (layer.content.layers.some((child) => child.content.type === 'text' && child.content.text === text)) return layer.transform.scale.x;
    const inner = scaleOf(layer.content.layers, text);
    if (inner !== undefined) return inner;
  }
  return undefined;
}

test('with glyph metrics, the word being spoken grows over a band that slides from the word before', () => {
  setTextMeasurer(async (request) => measureGlyphs(request), measureGlyphs);
  const captions = mount(root(track({
    ...still, fontSize: 40, background: null, align: 'left',
    highlight: { color: '#ff0000', background: '#00ff00', scale: 1.5, transitionInFrames: 4, padding: 0, cornerRadius: 0 },
    cues: [{ from: 0, durationInFrames: 40, text: 'ab cd', words: words('ab cd', [0, 10]) }],
  })));
  const bandAt = (frame: number) => rectsAt(captions, frame).find((rect) => isBand(rect))!;
  const [page, word, onBand, wordOnBand] = textsAt(captions, 14);
  // The page leaves the word clear; the word is drawn again, on its own, in
  // the text's color off the band and in the highlight's on it.
  assert.equal(page.text, 'ab cd');
  assert.deepEqual(page.style.colorRuns, [{ start: 3, end: 5, color: '#00000000' }]);
  assert.equal(word.text, 'cd');
  assert.deepEqual(word.style.fill, { type: 'solid', color: '#ffffff' });
  // The band clips a copy of the page in the highlight's color, so what it covers reads on it.
  assert.equal(onBand.text, 'ab cd');
  assert.deepEqual(onBand.style.fill, { type: 'solid', color: '#ff0000' });
  assert.deepEqual(wordOnBand.style.fill, { type: 'solid', color: '#ff0000' });
  const clip = at(captions, 14).layers.flatMap(function find(layer): Clip[] {
    return layer.content.type === 'group' ? [...(layer.content.clip ? [layer.content.clip] : []), ...layer.content.layers.flatMap(find)] : [];
  });
  assert.deepEqual(clip.map(({ width, height }) => [width, height]), [[80 * 1.5, 40 * 1.5]]);
  // At rest: grown 1.5 times about the word's centre, the band with it.
  const centre = page.x + 3 * 40 + 40;
  assert.equal(scaleOf(at(captions, 14).layers, 'cd'), 1.5);
  assert.deepEqual(bandAt(14), { ...bandAt(14), x: centre - 60, width: 80 * 1.5, height: 40 * 1.5 });
  // On its first frame the word has not grown, and the band is where "ab"'s
  // was; it slides over the transition.
  assert.equal(scaleOf(at(captions, 10).layers, 'cd'), 1);
  assert.equal(bandAt(9).x, page.x + 40 - 60);
  assert.equal(bandAt(10).x, bandAt(9).x);
  assert.equal(bandAt(10).width, bandAt(9).width);
  assert.ok(bandAt(12).x > bandAt(10).x && bandAt(12).x < bandAt(14).x);
  // Spoken from the cue's start, "ab" grows from 1 without a band to slide from.
  assert.equal(scaleOf(at(captions, 0).layers, 'ab'), 1);
  assert.equal(bandAt(0).width, 80);
});

test('without glyph metrics, the highlight only changes colors', () => {
  const captions = mount(root(track({
    ...still, highlight: { ...lit, background: '#00ff00', scale: 2 },
    cues: [{ from: 0, durationInFrames: 40, text: 'ab cd', words: words('ab cd', [0, 10]) }],
  })));
  assert.deepEqual(shown(captions, 15), ['ab cd']);
  assert.equal(rectsAt(captions, 15).filter((rect) => isBand(rect)).length, 0);
  assert.deepEqual(runsAt(captions, 15)[1], { start: 3, end: 5, color: '#ff0000' });
});

test('a render callback gets the page and its words', () => {
  const seen: Pick<CaptionRenderProps, 'text' | 'page' | 'pages' | 'words'>[] = [];
  const captions = mount(root(h(Sequence, { from: 100 }, track({
    paging: { maxWords: 2 },
    cues: [{ from: 0, durationInFrames: 60, text: 'a b c', words: words('a b c', [0, 10, 30]) }],
    render: ({ text, page, pages, words }) => {
      seen.push({ text, page, pages, words });
      return null;
    },
  }))));
  at(captions, 115);
  at(captions, 130);
  assert.deepEqual(seen, [
    { text: 'a b', page: 0, pages: 2, words: [
      { start: 0, end: 1, text: 'a', from: 0, until: 10, state: 'spoken' },
      { start: 2, end: 3, text: 'b', from: 10, until: 30, state: 'current' },
    ] },
    { text: 'c', page: 1, pages: 2, words: [{ start: 0, end: 1, text: 'c', from: 30, until: 60, state: 'current' }] },
  ]);
});

test('bad words and highlight props fail with the cue named', () => {
  const bad = (props: Record<string, unknown>, pattern: RegExp) =>
    assert.throws(() => at(mount(root(track({ ...still, highlight: {}, ...props } as unknown as CaptionTrackProps))), 10), pattern);
  const cue = { id: 'w', from: 10, durationInFrames: 20, text: 'one two' };
  bad({ cues: [{ ...cue, words: [{ text: 'three', from: 10 }] }] }, /cue 0 \("w"\): word 0 "three" is not in the cue's text "one two"/);
  bad({ cues: [{ ...cue, words: [{ text: 'one', from: 30 }] }] }, /word 0 \("one"\) from must be a whole frame inside the cue \(10–29\), not 30/);
  bad({ cues: [{ ...cue, words: [{ text: 'one', from: 20 }, { text: 'two', from: 15 }] }] }, /word 1 \("two"\) starts at 15, before word 0 at 20/);
  bad({ cues: [{ ...cue, words: [{ text: 'two', from: 10 }, { text: 'one', from: 15 }] }] }, /word 1 "one" is not in the cue's text/);
  bad({ cues: [{ ...cue, words: [{ text: ' ', from: 10 }] }] }, /word 0 needs a non-empty `text`/);
  bad({ cues: [cue], paging: { maxWords: 0 } }, /paging.maxWords must be a whole number of at least 1/);
  bad({ cues: [cue], wordUnit: 'char' }, /wordUnit must be 'word' or 'phrase'/);
  bad({ cues: [cue], highlight: { scale: 0 } }, /highlight.scale must be a finite number greater than 0/);
  bad({ cues: [cue], highlight: { color: 'nope' } }, /<CaptionTrack> highlight.color "nope" is neither/);
});
