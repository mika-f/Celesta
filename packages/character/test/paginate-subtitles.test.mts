// `paginateSubtitles()` against a fixed measurer: every code point is
// `fontSize` wide (twice that at weight 700 or more), ASCII words never
// break, other text breaks anywhere, and `\n` always does. No fonts or speech
// engine are needed.

import assert from 'node:assert/strict';
import { beforeEach, test } from 'vitest';

import type { GlyphMetrics, TextMetrics } from '@celesta.studio/react';
import { setTextMeasurer } from '@celesta.studio/react/internal';
import type { MeasureTextRequest } from '@celesta.studio/react/internal';
import { paginateSubtitles, planDialogue } from '../src/index.js';
import type { DialogueLine, PaginateSubtitlesOptions, SpeechTiming } from '../src/index.js';

function measure({ text, style, maxWidth = Infinity }: MeasureTextRequest): TextMetrics {
  const size = style.fontSize ?? 10;
  const weight = (index: number) =>
    (style.fontRuns ?? []).some((run) => run.start <= index && index < run.end && (run.fontWeight ?? 0) >= 700) ? 2 : 1;
  const rows: { start: number; glyphs: GlyphMetrics[] }[] = [{ start: 0, glyphs: [] }];
  let x = 0;
  let index = 0;
  const tokens = text.split('\n').flatMap((paragraph, p) => [...(p > 0 ? ['\n'] : []), ...(paragraph.match(/[A-Za-z]+|[\s\S]/gu) ?? [])]);
  for (const token of tokens) {
    const length = Array.from(token).length;
    if (token === '\n') {
      index += 1;
      rows.push({ start: index, glyphs: [] });
      x = 0;
      continue;
    }
    const width = Array.from(token).reduce((sum, _, i) => sum + size * weight(index + i), 0);
    if (x > 0 && x + width > maxWidth + 1e-3 && token !== ' ') {
      rows.push({ start: index, glyphs: [] });
      x = 0;
    }
    rows[rows.length - 1].glyphs.push({
      start: index, end: index + length, x, width, line: rows.length - 1, text: token, rtl: false, inkAscent: 0, inkDescent: 0,
    });
    x += width;
    index += length;
  }
  const glyphs = rows.flatMap((row) => row.glyphs);
  return {
    width: Math.max(0, ...glyphs.map((glyph) => glyph.x + glyph.width)), height: rows.length * size * 1.25,
    ascent: size, descent: size * 0.25, lineHeight: size * 1.25, capHeight: 7, xHeight: 5,
    lines: rows.length, glyphs, lineStarts: rows.map((row) => row.start),
  };
}

let requests: MeasureTextRequest[];
beforeEach(() => {
  requests = [];
  setTextMeasurer(async (request) => {
    requests.push(request);
    return measure(request);
  }, measure);
});

const base: PaginateSubtitlesOptions = { fps: 30, style: { fontSize: 10 }, maxWidth: 60, maxLines: 1 };

/** A speech timing with one mora every `step` seconds. */
function timing(moras: number, step = 0.1): SpeechTiming {
  const list = Array.from({ length: moras }, (_, i) => ({
    text: 'ア', start: i * step, end: (i + 1) * step, consonant: null,
    vowel: { phoneme: 'a', start: i * step, end: (i + 1) * step }, accentPhrase: 0,
  }));
  return {
    exact: true, durationInSeconds: moras * step, speech: { start: 0, end: moras * step },
    accentPhrases: [], moras: list, pauses: [], phonemes: [],
  };
}

const texts = (line: DialogueLine) => line.subtitlePages?.map((page) => page.text);

test('a line that fits and a line with pages are left as they are', async () => {
  const fits: DialogueLine = { id: 'short', text: 'みじかい', durationInFrames: 30 };
  const manual: DialogueLine = { id: 'manual', text: 'あいうえおかきくけこ', durationInFrames: 30, subtitlePages: [{ from: 0, text: 'あいうえおかきくけこ' }] };
  const [a, b] = await paginateSubtitles([fits, manual], base);
  assert.equal(a, fits);
  assert.equal(b, manual);
});

test('long text splits where the measured lines do, with frames shared out and marked approximate', async () => {
  // Six characters a line: three lines, one a page.
  const [line] = await paginateSubtitles([{ id: 'long', text: 'あいうえおかきくけこさしすせそたち', durationInFrames: 90 }], base);
  assert.deepEqual(line.subtitlePages, [
    { from: 0, text: 'あいうえおか', timing: 'approximate' },
    { from: 32, text: 'きくけこさし', timing: 'approximate' },
    { from: 64, text: 'すせそたち', timing: 'approximate' },
  ]);
  // The measurement used the real style and width.
  assert.deepEqual([requests[0].maxWidth, requests[0].style.fontSize], [60, 10]);
  // The pages are valid for planDialogue.
  const plan = await planDialogue([line], { fps: 30 });
  assert.deepEqual(plan.lines[0].subtitlePages?.map((page) => [page.from, page.end]), [[0, 32], [32, 64], [64, 90]]);
});

test('maxLines and maxHeight both limit the lines on a page', async () => {
  const text = 'あいうえおかきくけこさしすせそたちつてと';
  const [two] = await paginateSubtitles([{ id: 'x', text, durationInFrames: 60 }], { ...base, maxLines: 2 });
  assert.deepEqual(texts(two), ['あいうえおかきくけこさし', 'すせそたちつてと']);
  const [tall] = await paginateSubtitles([{ id: 'x', text, durationInFrames: 60 }], { ...base, maxLines: 9, maxHeight: 25 });
  assert.deepEqual(texts(tall), ['あいうえおかきくけこさし', 'すせそたちつてと']);
  await assert.rejects(paginateSubtitles([{ id: 'x', text, durationInFrames: 60 }], { ...base, maxHeight: 5 }), /line "x": maxHeight 5 px is less than one line/);
});

test('breaks never cut a grapheme cluster, and whitespace at a break is dropped', async () => {
  // The family emoji is 5 code points; the measurer would break inside it.
  const [line] = await paginateSubtitles([{ id: 'emoji', text: 'あいう👨‍👩‍👧えおか', durationInFrames: 60 }], base);
  assert.deepEqual(texts(line), ['あいう👨‍👩‍👧', 'えおか']);
  const [spaced] = await paginateSubtitles([{ id: 'spaced', text: 'Hi you all', durationInFrames: 60 }], { ...base, maxWidth: 30 });
  assert.deepEqual(texts(spaced), ['Hi', 'you', 'all']);
  await planDialogue([line, spaced], { fps: 30 });
});

test('emphasis is measured at its weight', async () => {
  // Bold characters measure twice as wide here, so the bold line wraps sooner.
  const [line] = await paginateSubtitles([{
    id: 'bold', text: 'あいうえおかき', durationInFrames: 60, emphasis: [{ start: 0, end: 2, style: 'loud' }],
  }], { ...base, emphasisStyles: { loud: { fontWeight: 800 } } });
  assert.deepEqual(texts(line), ['あいうえ', 'おかき']);
  await assert.rejects(
    paginateSubtitles([{ id: 'bad', text: 'あい', durationInFrames: 60, emphasis: [{ start: 0, end: 1, style: 'nope' }] }], base),
    /paginateSubtitles\(\): line "bad": emphasis 0 names the unknown style "nope"/,
  );
});

test('with speech timing and segments, pages break between segments and start on their first mora', async () => {
  const line: DialogueLine = {
    id: 'danger', text: 'HPが1です。回復しましょう。', spokenText: 'えいちぴーがいちです。かいふくしましょう。', durationInFrames: 90,
    subtitleSegments: [
      { text: 'HPが', moras: 5 }, { text: '1です。', moras: 4 }, { text: '回復', moras: 4 }, { text: 'しましょう。', moras: 4 },
    ],
  };
  // Seven columns measure "HPが1です。" / "回復しましょう" / "。"; the second
  // break, inside "しましょう。", moves back to the boundary before it.
  const [paged] = await paginateSubtitles([line], { ...base, maxWidth: 70, speechTiming: () => timing(17) });
  assert.deepEqual(paged.subtitlePages, [
    { from: 0, text: 'HPが1です。', timing: 'aligned' },
    // The 10th mora starts at 0.9 s, the 14th at 1.3 s.
    { from: 27, text: '回復', timing: 'aligned' },
    { from: 39, text: 'しましょう。', timing: 'aligned' },
  ]);
  // Eight columns would break after "回"; the page ends before "回復" instead.
  const [snapped] = await paginateSubtitles([line], { ...base, maxWidth: 80, speechTiming: () => timing(17) });
  assert.deepEqual(snapped.subtitlePages?.map((page) => [page.from, page.text]), [[0, 'HPが1です。'], [27, '回復しましょう。']]);
  // Without segments the same line is approximate.
  const [rough] = await paginateSubtitles([{ ...line, subtitleSegments: undefined }], { ...base, maxWidth: 80, speechTiming: () => timing(17) });
  assert.ok(rough.subtitlePages?.every((page) => page.timing === 'approximate'));
});

test('an inconsistent segment mapping is refused with the line and segment named', async () => {
  const line: DialogueLine = {
    id: 'danger', text: 'HPが1です。回復', durationInFrames: 90,
    subtitleSegments: [{ text: 'HPが', moras: 5 }, { text: '2です。', moras: 4 }, { text: '回復', moras: 4 }],
  };
  const options = { ...base, maxWidth: 70, speechTiming: () => timing(13) };
  await assert.rejects(paginateSubtitles([line], options), /line "danger": segment 1 \("2です。"\) does not continue the line's text at cluster 3/);
  await assert.rejects(
    paginateSubtitles([{ ...line, subtitleSegments: [{ text: 'HPが1です。回復', moras: 12 }] }], options),
    /line "danger": the segments give 12 moras, but the speech timing has 13/,
  );
  await assert.rejects(
    paginateSubtitles([{ ...line, subtitleSegments: [{ text: 'HPが1です。', moras: 9 }] }], options),
    /line "danger": the segments cover 7 of the text's 9 grapheme clusters/,
  );
  await assert.rejects(
    paginateSubtitles([{ ...line, subtitleSegments: [{ text: 'HPが1です。回復', moras: 1.5 }] }], options),
    /segment 0 \("HPが1です。回復"\) needs a whole number of moras/,
  );
});

test('a word wider than maxWidth fails, or stays on its page with overlong keep', async () => {
  const line: DialogueLine = { id: 'word', text: 'Supercalifragilistic ok', durationInFrames: 60 };
  await assert.rejects(paginateSubtitles([line], base), /line "word": "Supercalifragilistic" is 200 px wide, wider than maxWidth 60/);
  const [kept] = await paginateSubtitles([line], { ...base, overlong: 'keep' });
  assert.deepEqual(texts(kept), ['Supercalifragilistic', 'ok']);
});

test('pages that cannot all start inside the line are refused', async () => {
  await assert.rejects(
    paginateSubtitles([{ id: 'tiny', text: 'あいうえおかきくけこさしすせそ', durationInFrames: 2 }], base),
    /line "tiny": its 3 pages do not fit its 2 frames/,
  );
});
