import { expect, test } from 'vitest';
import React from 'react';

import {
  Composition, FreezeFrame, Sequence, Static, Subtitles, formatSubtitles, parseSubtitles,
} from '../dist/index.js';
import { mount } from '../dist/render.js';

const h = React.createElement;

test('parses SRT with a BOM, CRLF, several lines, and loose timecodes', () => {
  const source = '﻿1\r\n00:00:01,000 --> 00:00:02,500\r\nHello\r\n<i>world</i>\r\n\r\n'
    + '2\r\n0:00:03.2 --> 00:00:04,25  X1:10 X2:20\r\n{\\an8}Second\r\n\r\n\r\n'
    + '00:01:00,000 --> 01:00:00,000\r\nNo index\r\n';
  expect(parseSubtitles(source, { fps: 30 })).toEqual([
    { id: '1', from: 30, durationInFrames: 45, text: 'Hello\nworld' },
    { id: '2', from: 96, durationInFrames: 32, text: 'Second' },
    { from: 1800, durationInFrames: 106200, text: 'No index' },
  ]);
});

test('parses CR-only line ends', () => {
  expect(parseSubtitles('1\r00:00:00,000 --> 00:00:01,000\rOne\r', { fps: 10 }))
    .toEqual([{ id: '1', from: 0, durationInFrames: 10, text: 'One' }]);
});

test('parses WebVTT headers, blocks, settings, and markup', () => {
  const source = [
    'WEBVTT - captions',
    'Kind: captions',
    '',
    'NOTE a comment',
    'spanning lines',
    '',
    'STYLE',
    '::cue { color: yellow }',
    '',
    'intro',
    '00:01.000 --> 00:02.000 align:start position:10%',
    '<v Alice>Hi &amp; <b>welcome</b></v>',
    '<ruby>漢字<rt>かんじ</rt></ruby> &lt;3 &#x2665;',
    '',
    '00:00:02.000 --> 00:00:03.000',
    '<c.loud>Two</c> <00:00:02.500>parts',
    '',
  ].join('\n');
  expect(parseSubtitles(source, { fps: 30 })).toEqual([
    { id: 'intro', from: 30, durationInFrames: 30, text: 'Hi & welcome\n漢字 <3 ♥' },
    { from: 60, durationInFrames: 30, text: 'Two parts' },
  ]);
});

test('rounds times to frames and keeps at least one frame', () => {
  expect(parseSubtitles('00:00:00,010 --> 00:00:00,020\nshort\n', { fps: 24 }))
    .toEqual([{ from: 0, durationInFrames: 1, text: 'short' }]);
  // 23.976 fps: 1.001 s is frame 24.
  expect(parseSubtitles('00:00:01,001 --> 00:00:02,002\nx\n', { fps: 24000 / 1001 })[0])
    .toEqual({ from: 24, durationInFrames: 24, text: 'x' });
});

test('skips cues with no text', () => {
  expect(parseSubtitles('1\n00:00:00,000 --> 00:00:01,000\n<i></i>\n\n2\n00:00:01,000 --> 00:00:02,000\nB\n', { fps: 10 }))
    .toEqual([{ id: '2', from: 10, durationInFrames: 10, text: 'B' }]);
});

test('reports malformed files with their line', () => {
  expect(() => parseSubtitles('1\n00:00:01 -> 00:00:02\nx\n', { fps: 30 })).toThrow(/line 1: expected a timing line/);
  expect(() => parseSubtitles('\n\n1\n00:00:aa,000 --> 00:00:02,000\nx\n', { fps: 30 })).toThrow(/line 4: cannot read/);
  expect(() => parseSubtitles('00:00:02,000 --> 00:00:01,000\nx\n', { fps: 30 })).toThrow(/line 1: the cue ends before/);
  expect(() => parseSubtitles('00:00:61,000 --> 00:01:02,000\nx\n', { fps: 30 })).toThrow(/below 60/);
  expect(() => parseSubtitles('', { fps: 0 })).toThrow(/positive fps/);
});

test('formats SRT and WebVTT, sorted, with text made safe', () => {
  const cues = [
    { id: 'b', from: 90, durationInFrames: 30, text: 'Later' },
    { id: 'a --> no', from: 0, durationInFrames: 45, text: 'First\n\nline & <tag>' },
    { from: 60, durationInFrames: 10, text: '   ' },
    { from: 108000, durationInFrames: 1, text: 'An hour in' },
  ];
  expect(formatSubtitles(cues, { fps: 30, format: 'srt' })).toBe([
    '1', '00:00:00,000 --> 00:00:01,500', 'First', 'line & <tag>', '',
    '2', '00:00:03,000 --> 00:00:04,000', 'Later', '',
    '3', '01:00:00,000 --> 01:00:00,033', 'An hour in', '',
  ].join('\n'));
  expect(formatSubtitles(cues, { fps: 30, format: 'vtt' })).toBe([
    'WEBVTT', '',
    '00:00:00.000 --> 00:00:01.500', 'First', 'line &amp; &lt;tag&gt;', '',
    'b', '00:00:03.000 --> 00:00:04.000', 'Later', '',
    '01:00:00.000 --> 01:00:00.033', 'An hour in', '',
  ].join('\n'));
  expect(formatSubtitles([], { fps: 30, format: 'vtt' })).toBe('WEBVTT\n\n');
  expect(() => formatSubtitles([{ from: 0, durationInFrames: 0, text: 'x' }], { fps: 30, format: 'srt' }))
    .toThrow(/cue 0 requires a positive `durationInFrames`/);
});

test('a formatted file parses back to the same cues', () => {
  const cues = [
    { id: 'one', from: 3, durationInFrames: 57, text: 'こんにちは\n二行目' },
    { id: 'two', from: 75, durationInFrames: 1, text: 'a < b & c' },
  ];
  for (const format of ['srt', 'vtt']) {
    const parsed = parseSubtitles(formatSubtitles(cues, { fps: 30, format }), { fps: 30 });
    expect(parsed.map(({ id: _id, ...cue }) => cue)).toEqual(cues.map(({ id: _id, ...cue }) => cue));
  }
});

function frames(Root, count) {
  const mounted = mount(Root);
  return Array.from({ length: count }, (_, frame) =>
    mounted.renderAt({ value: frame, timescale: 10 }, null).subtitles);
}

test('<Subtitles> reports the cues shown on each frame, cut to its sequence', () => {
  const cues = [
    { from: 0, durationInFrames: 3, text: 'early' },
    { from: 3, durationInFrames: 10, text: 'cut' },
    { from: 20, durationInFrames: 5, text: 'outside' },
  ];
  const Root = () => h(Composition, { width: 10, height: 10, fps: 10, durationInFrames: 12 },
    h(Sequence, { from: 2, durationInFrames: 6 }, h(Subtitles, { cues })));
  const reports = frames(Root, 12);
  expect(reports[0]).toEqual([]);
  expect(reports[2]).toEqual([{ text: 'early', start: 0.2, end: 0.5 }]);
  expect(reports[5]).toEqual([{ text: 'cut', start: 0.5, end: 0.8 }]);
  expect(reports[8]).toEqual([]);
  expect(reports.flat().map(({ text }) => text)).not.toContain('outside');
});

test('<Subtitles> reports nothing inside a <FreezeFrame> and is rejected in <Static>', () => {
  const cues = [{ from: 0, durationInFrames: 10, text: 'frozen' }];
  const Frozen = () => h(Composition, { width: 10, height: 10, fps: 10, durationInFrames: 2 },
    h(FreezeFrame, { frame: 0 }, h(Subtitles, { cues })));
  expect(frames(Frozen, 2)).toEqual([[], []]);
  const Cached = () => h(Composition, { width: 10, height: 10, fps: 10, durationInFrames: 2 },
    h(Static, null, h(Subtitles, { cues })));
  expect(() => frames(Cached, 1)).toThrow(/<Static> cannot contain <subtitles>/);
});

test('<Subtitles> checks its cues', () => {
  const Root = () => h(Composition, { width: 10, height: 10, fps: 10, durationInFrames: 1 },
    h(Subtitles, { cues: [{ from: 0, durationInFrames: 1 }] }));
  expect(() => frames(Root, 1)).toThrow(/<Subtitles>: cue 0 requires a string `text`/);
});

test('<Subtitles lang> tags its cues; a composition or sequence lang does not', () => {
  const cue = (text) => [{ from: 0, durationInFrames: 10, text }];
  const Root = () => h(Composition, { width: 10, height: 10, fps: 10, durationInFrames: 1, lang: 'ja-JP' },
    h(Subtitles, { lang: 'en', cues: cue('Hello') }),
    h(Subtitles, { lang: 'pt-BR', cues: cue('Olá') }),
    h(Subtitles, { lang: '', cues: cue('untagged') }),
    h(Sequence, { lang: 'fr' }, h(Subtitles, { cues: cue('inherited nothing') })));
  // The one-frame composition cuts every cue to 0.1 s.
  expect(frames(Root, 1)[0]).toEqual([
    { text: 'Hello', start: 0, end: 0.1, lang: 'en' },
    { text: 'Olá', start: 0, end: 0.1, lang: 'pt-BR' },
    { text: 'untagged', start: 0, end: 0.1 },
    { text: 'inherited nothing', start: 0, end: 0.1 },
  ]);
  for (const lang of ['en_US', '../en', 'en-', 42]) {
    const Bad = () => h(Composition, { width: 10, height: 10, fps: 10, durationInFrames: 1 },
      h(Subtitles, { lang, cues: cue('x') }));
    expect(() => frames(Bad, 1)).toThrow(/<Subtitles> requires `lang` to be a language tag/);
  }
});

test('collectSubtitles() sweeps a span of frames and reports each subtitle once', () => {
  const cues = [
    { from: 0, durationInFrames: 3, text: 'first' },
    { from: 3, durationInFrames: 4, text: 'second' },
    { from: 9, durationInFrames: 3, text: 'third' },
  ];
  const Root = () => h(Composition, { width: 10, height: 10, fps: 10, durationInFrames: 12 },
    h(Subtitles, { lang: 'en', cues }),
    h(Sequence, { from: 2, durationInFrames: 4 }, h(Subtitles, { cues: [{ from: 0, durationInFrames: 9, text: 'cut' }] })),
    h(FreezeFrame, { frame: 0 }, h(Subtitles, { cues: [{ from: 0, durationInFrames: 12, text: 'frozen' }] })));
  const mounted = mount(Root);
  expect(mounted.collectSubtitles(2, 6)).toEqual([
    { text: 'first', start: 0, end: 0.3, lang: 'en' },
    { text: 'cut', start: 0.2, end: 0.6 },
    { text: 'second', start: 0.3, end: 0.7, lang: 'en' },
  ]);
  // The sweep reports what rendering the same frames reports.
  const rendered = frames(Root, 12).flat();
  expect(mounted.collectSubtitles(0, 12)).toEqual(rendered.filter((subtitle, index) =>
    rendered.findIndex((other) => JSON.stringify(other) === JSON.stringify(subtitle)) === index));
});
