import assert from 'node:assert/strict';
import { test } from 'vitest';

import { duckEnvelope } from '@celesta.studio/media-utils';
import { duckingVolume, planDialogue, speechIntervals } from '../src/index.js';
import type { DialogueLine, VoiceLevelAnalysis, VoiceLevelLineReport, VoiceLevelReport } from '../src/index.js';

const line = (id: string, fields: Partial<DialogueLine> = {}): DialogueLine =>
  ({ id, text: id, durationInFrames: 30, audio: `./${id}.wav`, ...fields });

// 30 fps, no gap: a at 0–1 s, b at 1–2 s, c at 2–3 s, d at 3–4 s.
const plan = () => planDialogue([line('a'), line('b', { muted: true }), line('c', { audio: undefined }), line('d')], { fps: 30, gap: 0 });

function analysis(start: number | null, end: number | null): VoiceLevelAnalysis {
  return {
    version: 1, sampleRate: 48_000, channelCount: 1, durationSeconds: 1,
    status: start === null ? 'silent' : 'ok', speechSeconds: 0, rmsDbfs: null, peakDbfs: null,
    speechStartSeconds: start, speechEndSeconds: end,
  };
}

/** A report with only what the helper reads: one entry per line, in order, for lines a, b, c, d. */
function report(analyses: (VoiceLevelAnalysis | undefined)[], ids = ['a', 'b', 'c', 'd']): VoiceLevelReport {
  const lines = analyses.map((entry, index) => ({ id: ids[index], index, analysis: entry }) as VoiceLevelLineReport);
  // Deliberately partial: the helper reads `lines` only.
  return { lines } as VoiceLevelReport;
}

test('lines count from their planned start for their planned length', async () => {
  assert.deepEqual(speechIntervals(await plan()), [[0, 1], [3, 4]]);
});

test('a level report trims lines to their speech', async () => {
  const levels = report([analysis(0.2, 0.7), undefined, undefined, analysis(null, null)]);
  assert.deepEqual(speechIntervals(await plan(), { levels }), [[0.2, 0.7]]);
  // A line the report did not analyze counts whole; speech past the line is cut.
  const partial = report([undefined, undefined, undefined, analysis(0.5, 9)]);
  assert.deepEqual(speechIntervals(await plan(), { levels: partial }), [[0, 1], [3.5, 4]]);
});

test('a report for other lines throws', async () => {
  const planned = await plan();
  assert.throws(() => speechIntervals(planned, { levels: report([analysis(0, 1)]) }), /report has 1 line, the plan 4/);
  const others = report([undefined, undefined, undefined, undefined], ['a', 'b', 'x', 'd']);
  assert.throws(() => speechIntervals(planned, { levels: others }), /line "c" of the plan is "x" in the report/);
});

test('speech found past the end of a shorter planned line is left out', async () => {
  const levels = report([analysis(1.5, 2), undefined, undefined, undefined]);
  assert.deepEqual(speechIntervals(await plan(), { levels }), [[3, 4]]);
});

test('duckingVolume is duckEnvelope over the intervals, with origin in frames', async () => {
  const planned = await plan();
  const duck = { volume: 0.8, to: 0.3, attack: 0.2 };
  assert.deepEqual(duckingVolume(planned, duck, { origin: 15 }), duckEnvelope([[0, 1], [3, 4]], duck, { origin: 0.5 }));
});

test('an attack before frame 0 keeps its key', async () => {
  const curve = duckingVolume(await plan(), {});
  assert.ok(typeof curve !== 'number');
  assert.deepEqual(curve.keyframes[0].time, { value: -150_000, timescale: 1_000_000 });
});
