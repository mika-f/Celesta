import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import React from 'react';
import { test } from 'vitest';

import { Assets, Composition } from '@celesta.studio/react';
import type { AssetReference, KeyframeAnimation } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import type { AudioClipDescriptor } from '@celesta.studio/react/internal';

import {
  Character,
  CharacterView,
  DialogueSeries,
  analyzeVoiceLevel,
  decodeWav,
  matchVoiceLevels,
  planDialogue,
  scaleVolume,
} from '../src/index.js';
import type { CharacterViewReference, DialogueLine, LipSyncTrack } from '../src/index.js';

const h = React.createElement;
const RATE = 48_000;
// 20 ms analysis blocks are 960 frames at 48 kHz; a 200 Hz sine fits exactly
// four periods in one, so every voiced block's power is exactly A² / 2.
const BLOCK = 960;
const db = (value: number) => 20 * Math.log10(value);
const sineRmsDbfs = (amplitude: number) => db(amplitude) - 10 * Math.log10(2);
// `actual` is whatever the report holds there, which may be null or a
// keyframed volume; anything but a close number fails.
const close = (actual: unknown, expected: number, tolerance = 0.01) =>
  assert.ok(typeof actual === 'number' && Math.abs(actual - expected) <= tolerance, `expected ${actual} to be within ${tolerance} of ${expected}`);

/** The keyframes of a keyframed volume. */
function keyframesOf(volume: AudioClipDescriptor['volume'] | undefined) {
  assert.ok(volume !== undefined && typeof volume !== 'number', 'a keyframed volume');
  return volume.keyframes;
}

type WavFormat = 'pcm16' | 'pcm24' | 'float32' | 'mulaw';

/** Encodes channels of float samples as a WAV file. */
function wav(channels: Float64Array[], { format = 'float32', sampleRate = RATE }: { format?: WavFormat; sampleRate?: number } = {}): Buffer {
  const bits = { pcm16: 16, pcm24: 24, float32: 32, mulaw: 8 }[format];
  const audioFormat = format === 'float32' ? 3 : format === 'mulaw' ? 7 : 1;
  const frames = channels[0].length;
  const dataSize = frames * channels.length * (bits / 8);
  const buffer = Buffer.alloc(44 + dataSize);
  buffer.write('RIFF', 0, 'ascii');
  buffer.writeUInt32LE(36 + dataSize, 4);
  buffer.write('WAVE', 8, 'ascii');
  buffer.write('fmt ', 12, 'ascii');
  buffer.writeUInt32LE(16, 16);
  buffer.writeUInt16LE(audioFormat, 20);
  buffer.writeUInt16LE(channels.length, 22);
  buffer.writeUInt32LE(sampleRate, 24);
  buffer.writeUInt32LE(sampleRate * channels.length * (bits / 8), 28);
  buffer.writeUInt16LE(channels.length * (bits / 8), 32);
  buffer.writeUInt16LE(bits, 34);
  buffer.write('data', 36, 'ascii');
  buffer.writeUInt32LE(dataSize, 40);
  let offset = 44;
  for (let i = 0; i < frames; i += 1) {
    for (const channel of channels) {
      const value = Math.max(-1, Math.min(1, channel[i]));
      if (format === 'float32') buffer.writeFloatLE(value, offset);
      else if (format === 'pcm16') buffer.writeInt16LE(Math.round(value * 0x7fff), offset);
      else if (format === 'pcm24') buffer.writeIntLE(Math.round(value * 0x7fffff), offset, 3);
      else buffer.writeUInt8(0xff, offset);
      offset += bits / 8;
    }
  }
  return buffer;
}

/**
 * Speech-like mono: `syllables` bursts of a 200 Hz sine, each `blocksOn`
 * analysis blocks long, separated by `blocksOff` blocks of silence.
 */
function speech(amplitude: number, { syllables = 10, blocksOn = 8, blocksOff = 4, floor = 0 } = {}): Float64Array {
  const frames = syllables * (blocksOn + blocksOff) * BLOCK;
  const samples = new Float64Array(frames);
  for (let i = 0; i < frames; i += 1) {
    const voiced = Math.floor(i / BLOCK) % (blocksOn + blocksOff) < blocksOn;
    samples[i] = voiced ? amplitude * Math.sin((2 * Math.PI * 200 * i) / RATE) : floor * Math.sin((2 * Math.PI * 200 * i) / RATE);
  }
  return samples;
}

/** Writes files into a temp entry directory and runs `fn` with it as the entry dir. */
async function withEntry<T>(files: Record<string, Buffer>, fn: (dir: string) => Promise<T>): Promise<T> {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-voice-level-'));
  for (const [name, bytes] of Object.entries(files)) writeFileSync(join(dir, name), bytes);
  const previous = process.env.CELESTA_REACT_ENTRY_DIR;
  process.env.CELESTA_REACT_ENTRY_DIR = dir;
  try {
    return await fn(dir);
  } finally {
    if (previous === undefined) delete process.env.CELESTA_REACT_ENTRY_DIR;
    else process.env.CELESTA_REACT_ENTRY_DIR = previous;
    rmSync(dir, { recursive: true, force: true });
  }
}

test('gated speech RMS ignores silence and quiet tails; the peak covers every channel', async () => {
  const tail = new Float64Array(BLOCK * 50).map((_, i) => 0.003 * Math.sin((2 * Math.PI * 200 * i) / RATE));
  const withTail = Float64Array.from([...speech(0.25), ...tail]);
  await withEntry({ 'voice.wav': wav([speech(0.25)]), 'tail.wav': wav([withTail]) }, async () => {
    const analysis = await analyzeVoiceLevel('./voice.wav');
    assert.equal(analysis.status, 'ok');
    assert.equal(analysis.channelCount, 1);
    close(analysis.rmsDbfs, sineRmsDbfs(0.25));
    close(analysis.peakDbfs, db(0.25));
    close(analysis.speechSeconds, 10 * 8 * 0.02, 1e-9);
    close(analysis.durationSeconds, 10 * 12 * 0.02, 1e-9);
    assert.equal(analysis.speechStartSeconds, 0);
    close(analysis.speechEndSeconds, (9 * 12 + 8) * 0.02, 1e-9);
    // -50 dBFS of room tone is above the absolute gate but more than 20 dB
    // under the speech, so the relative gate drops it.
    close((await analyzeVoiceLevel('./tail.wav')).rmsDbfs, sineRmsDbfs(0.25));
  });
});

test('stereo channels with opposite polarity are measured, not cancelled', async () => {
  const left = speech(0.2);
  const right = left.map((value) => -value);
  await withEntry({ 'stereo.wav': wav([left, right]) }, async (dir) => {
    const analysis = await analyzeVoiceLevel('stereo.wav');
    assert.equal(analysis.channelCount, 2);
    close(analysis.rmsDbfs, sineRmsDbfs(0.2));
    close(analysis.peakDbfs, db(0.2));
    // The lip-sync down-mix is unchanged: it still cancels to silence.
    assert.ok(decodeWav(readFileSync(join(dir, 'stereo.wav'))).samples.every((sample) => sample === 0));
  });
});

test('PCM 16/24-bit and float WAVs agree; unsupported formats and URLs fail clearly', async () => {
  const voice = speech(0.3);
  await withEntry(
    { 'a.wav': wav([voice], { format: 'pcm16' }), 'b.wav': wav([voice], { format: 'pcm24' }), 'c.wav': wav([voice]), 'd.wav': wav([voice], { format: 'mulaw' }), 'e.wav': Buffer.from('nope') },
    async () => {
      for (const name of ['a.wav', 'b.wav', 'c.wav']) close((await analyzeVoiceLevel(name)).rmsDbfs, sineRmsDbfs(0.3), 0.02);
      await assert.rejects(analyzeVoiceLevel('d.wav'), /unsupported WAV format 7/);
      await assert.rejects(analyzeVoiceLevel('e.wav'), /not a RIFF\/WAVE file/);
      await assert.rejects(analyzeVoiceLevel('missing.wav'), /could not read missing\.wav/);
      await assert.rejects(analyzeVoiceLevel('https://example.com/a.wav'), /needs a local WAV file/);
      await assert.rejects(
        matchVoiceLevels([{ id: 'x', text: '', audio: 'd.wav' }]),
        /line "x": could not analyze d\.wav: unsupported WAV format 7.*voiceLevel: false/,
      );
    },
  );
});

test('a quiet and a loud voice converge on the target when the bounds allow', async () => {
  await withEntry({ 'quiet.wav': wav([speech(0.03)]), 'loud.wav': wav([speech(0.5)]) }, async () => {
    const { lines, report } = await matchVoiceLevels(
      [{ id: 'q', text: 'q', audio: 'quiet.wav' }, { id: 'l', text: 'l', audio: 'loud.wav', volume: 0.5 }],
      { targetRmsDbfs: -20, maxBoostDb: 20 },
    );
    assert.deepEqual(report.lines.map((line) => line.status), ['matched', 'matched']);
    for (const line of report.lines) {
      close(line.rmsAfterDbfs, -20);
      assert.ok(line.peakAfterDbfs! <= -1);
      close(line.gain, 10 ** (line.gainDb / 20), 1e-12);
    }
    close(report.lines[0].gainDb, -20 - sineRmsDbfs(0.03));
    close(report.lines[1].gainDb, -20 - sineRmsDbfs(0.5));
    // The gain multiplies the authored volume (default 1).
    close(lines[0].volume, report.lines[0].gain, 1e-12);
    close(lines[1].volume, 0.5 * report.lines[1].gain, 1e-12);
    assert.deepEqual(report.options, { targetRmsDbfs: -20, peakCeilingDbfs: -1, maxBoostDb: 20, maxCutDb: 12, speakers: {} });
  });
});

test('boost, cut, and the peak ceiling bound the gain and say so', async () => {
  // A quiet voice with one full-scale click: low RMS, high peak.
  const clicky = speech(0.02);
  clicky[100] = 0.9;
  await withEntry(
    { 'quiet.wav': wav([speech(0.01)]), 'loud.wav': wav([speech(1)]), 'clicky.wav': wav([clicky]) },
    async () => {
      const { report } = await matchVoiceLevels(
        [
          { id: 'quiet', text: '', audio: 'quiet.wav' },
          { id: 'loud', text: '', audio: 'loud.wav' },
          { id: 'clicky', text: '', audio: 'clicky.wav' },
        ],
        { targetRmsDbfs: -20, maxBoostDb: 6, maxCutDb: 3, peakCeilingDbfs: -1 },
      );
      const [quiet, loud, clickyLine] = report.lines;
      assert.equal(quiet.status, 'limited');
      assert.deepEqual(quiet.limitedBy, ['max-boost']);
      assert.equal(quiet.gainDb, 6);
      assert.ok(quiet.desiredGainDb! > 6);
      // Full scale wants about -17 dB; the cut stops at -3 dB, and the ceiling
      // then takes it to -1 dBFS peak.
      assert.deepEqual(loud.limitedBy, ['max-cut']);
      assert.equal(loud.gainDb, -3);
      // It wants more than 6 dB, and even 6 dB would push the click past -1 dBFS.
      assert.deepEqual(clickyLine.limitedBy, ['max-boost', 'peak-ceiling']);
      close(clickyLine.peakAfterDbfs, -1, 1e-6);
      assert.ok(clickyLine.rmsAfterDbfs! < -20);
    },
  );
  // The ceiling is the hardest bound: it cuts past maxCutDb.
  await withEntry({ 'hot.wav': wav([speech(1)]) }, async () => {
    const { report } = await matchVoiceLevels([{ text: '', audio: 'hot.wav' }], { targetRmsDbfs: 0, maxCutDb: 0, peakCeilingDbfs: -6 });
    assert.deepEqual(report.lines[0].limitedBy, ['peak-ceiling']);
    close(report.lines[0].gainDb, -6, 1e-6);
  });
});

test('silence, near-silence, and too-short recordings get finite unity gain', async () => {
  const near = new Float64Array(RATE).map((_, i) => 0.0003 * Math.sin((2 * Math.PI * 200 * i) / RATE));
  const short = speech(0.3, { syllables: 1, blocksOn: 2, blocksOff: 0 });
  await withEntry(
    { 'zero.wav': wav([new Float64Array(RATE)]), 'near.wav': wav([near]), 'short.wav': wav([short]), 'empty.wav': wav([new Float64Array(0)]) },
    async () => {
      const { lines, report } = await matchVoiceLevels(
        ['zero', 'near', 'short', 'empty'].map((id) => ({ id, text: '', audio: `${id}.wav`, volume: 0.7 })),
        { maxBoostDb: 60 },
      );
      assert.deepEqual(report.lines.map((line) => line.status), ['silent', 'silent', 'insufficient-speech', 'silent']);
      for (const [index, line] of report.lines.entries()) {
        assert.equal(line.gainDb, 0);
        assert.equal(line.gain, 1);
        assert.equal(line.rmsAfterDbfs, null);
        assert.equal(lines[index].volume, 0.7);
      }
      assert.equal(report.lines[0].analysis?.peakDbfs, null);
      assert.ok(Number.isFinite(report.lines[1].analysis?.peakDbfs));
      close(report.lines[2].analysis?.speechSeconds, 0.04, 1e-9);
    },
  );
});

test('speaker targets and offsets, and per-line opt-out and overrides', async () => {
  await withEntry({ 'v.wav': wav([speech(0.1)]) }, async () => {
    const rms = sineRmsDbfs(0.1);
    const { lines, report } = await matchVoiceLevels(
      [
        { id: 'a', text: '', audio: 'v.wav', speaker: 'akane' },
        { id: 'b', text: '', audio: 'v.wav', speaker: 'aoi' },
        { id: 'c', text: '', audio: 'v.wav', speaker: 'kotone' },
        { id: 'd', text: '', audio: 'v.wav', speaker: 'aoi', voiceLevel: { offsetDb: 2 } },
        { id: 'e', text: '', audio: 'v.wav', speaker: 'aoi', voiceLevel: { targetRmsDbfs: -30 } },
        { id: 'f', text: '', audio: 'v.wav', voiceLevel: false, volume: 0.4 },
        { id: 'g', text: '', audio: 'v.wav', voiceLevel: { gainDb: -6 }, volume: 0.5 },
        { id: 'h', text: '', audio: 'v.wav', muted: true },
        { id: 'i', text: '', durationInFrames: 10 },
      ],
      { targetRmsDbfs: -20, speakers: { aoi: { offsetDb: -1 }, kotone: { targetRmsDbfs: -24, offsetDb: 1 } } },
    );
    const targets = report.lines.map((line) => line.targetRmsDbfs);
    assert.deepEqual(targets.slice(0, 5), [-20, -21, -23, -19, -30]);
    report.lines.slice(0, 5).forEach((line) => close(line.gainDb, line.targetRmsDbfs! - rms));
    assert.deepEqual(report.lines.slice(5).map((line) => line.status), ['opted-out', 'fixed-gain', 'muted', 'no-audio']);
    assert.equal(report.lines[6].gainDb, -6);
    close(lines[6].volume, 0.5 * 10 ** (-6 / 20), 1e-12);
    // Lines left alone are passed through as the same objects.
    for (const index of [5, 7, 8]) {
      assert.equal(report.lines[index].gain, 1);
      assert.equal(report.lines[index].analysis, undefined);
    }
    assert.equal(lines[5].volume, 0.4);
  });
});

test('scaleVolume keeps keyframe times, easings, and same-time cuts', () => {
  assert.equal(scaleVolume(undefined, 2), 2);
  assert.equal(scaleVolume(0.5, 2), 1);
  const keyed: KeyframeAnimation<number> = {
    type: 'keyframes',
    keyframes: [
      { time: { value: 0, timescale: 1_000_000 }, value: 0 },
      { time: { value: 500_000, timescale: 1_000_000 }, value: 1, easing: 'ease-out' },
      { time: { value: 500_000, timescale: 1_000_000 }, value: 0.25 },
      { time: { value: 2_000_000, timescale: 1_000_000 }, value: 0.5, easing: null },
    ],
  };
  assert.deepEqual(scaleVolume(keyed, 2), {
    type: 'keyframes',
    keyframes: [
      { time: { value: 0, timescale: 1_000_000 }, value: 0 },
      { time: { value: 500_000, timescale: 1_000_000 }, value: 2, easing: 'ease-out' },
      { time: { value: 500_000, timescale: 1_000_000 }, value: 0.5 },
      { time: { value: 2_000_000, timescale: 1_000_000 }, value: 1, easing: null },
    ],
  });
  assert.throws(() => scaleVolume(1, Number.NaN), /finite gain/);
});

test('analysis is cached by file content: re-saving a path re-measures, editing text does not', async () => {
  // Amplitudes no other test uses, so earlier tests have not cached these bytes.
  await withEntry({ 'line.wav': wav([speech(0.123)]) }, async (dir) => {
    const script = (text: string): DialogueLine[] => [
      { id: 'one', text, audio: 'line.wav', expression: 'smile' },
      { id: 'two', text: 'again', audio: './line.wav' },
    ];
    const first = await matchVoiceLevels(script('hello'));
    // The second line shares the first one's analysis.
    assert.deepEqual(first.report.lines.map((line) => line.cached).sort(), [false, true]);
    const edited = await matchVoiceLevels(script('hello, edited').map((line) => ({ ...line, expression: 'calm' })));
    assert.deepEqual(edited.report.lines.map((line) => line.cached), [true, true]);
    assert.equal(edited.report.lines[0].gainDb, first.report.lines[0].gainDb);

    writeFileSync(join(dir, 'line.wav'), wav([speech(0.456)]));
    const resaved = await matchVoiceLevels(script('hello'));
    assert.equal(resaved.report.lines.some((line) => line.cached === false), true);
    close(resaved.report.lines[0].analysis?.rmsDbfs, sineRmsDbfs(0.456));
    close(resaved.report.lines[0].gainDb - first.report.lines[0].gainDb, db(0.123 / 0.456));
  });
});

test('matching changes only gain: timing, lip sync, and the source file stay the same', async () => {
  const bytes = wav([speech(0.05)]);
  await withEntry({ 'v.wav': bytes }, async (dir) => {
    const lipSync: LipSyncTrack = { durationInSeconds: 2.4, mouthAtSeconds: () => 'a', mouthAtFrame: () => 'a' };
    const text = h('span', null, 'hi');
    const original: DialogueLine = { id: 'v', text, audio: 'v.wav', lipSync, durationInFrames: 72, gap: 0.5, mouth: 'a' };
    const { lines, report } = await matchVoiceLevels([original]);
    const [matched] = lines;
    assert.notEqual(matched.volume, undefined);
    assert.deepEqual({ ...matched, volume: undefined }, { ...original, volume: undefined });
    assert.equal(matched.lipSync, lipSync);
    assert.equal(matched.text, text);
    assert.ok(readFileSync(join(dir, 'v.wav')).equals(bytes));
    close(report.lines[0].analysis?.durationSeconds, 2.4, 1e-9);

    const plan = await planDialogue(lines, { fps: 30 });
    const before = await planDialogue([original], { fps: 30 });
    assert.deepEqual(
      plan.lines.map(({ from, durationInFrames, spanInFrames }) => ({ from, durationInFrames, spanInFrames })),
      before.lines.map(({ from, durationInFrames, spanInFrames }) => ({ from, durationInFrames, spanInFrames })),
    );
  });
});

test('the rendered audio graph carries the matched gain on numeric and keyframed volumes', async () => {
  await withEntry({ 'v.wav': wav([speech(0.05)]) }, async () => {
    const fade: KeyframeAnimation<number> = {
      type: 'keyframes',
      keyframes: [
        { time: { value: 0, timescale: 1_000_000 }, value: 0 },
        { time: { value: 200_000, timescale: 1_000_000 }, value: 1, easing: 'ease-out' },
        { time: { value: 200_000, timescale: 1_000_000 }, value: 0.5 },
      ],
    };
    const { lines, report } = await matchVoiceLevels([
      { id: 'one', text: 'One', audio: 'v.wav', durationInFrames: 10, speaker: 'mira', volume: 0.8 },
      { id: 'two', text: 'Two', audio: 'v.wav', durationInFrames: 10, speaker: 'mira', volume: fade },
    ]);
    const gain = report.lines[0].gain;
    assert.ok(gain > 1);
    const plan = await planDialogue(lines, { fps: 30, gap: 0 });
    const mira = React.createRef<AssetReference>();
    const view = React.createRef<CharacterViewReference>();
    const Root = () => h(Composition, { width: 32, height: 32, fps: 30, durationInFrames: plan.durationInFrames },
      h(Assets, null, h(Character, { ref: mira, name: 'Mira', portrait: { defaultExpression: 'calm', expressions: { calm: 'calm.png' } } })),
      h(CharacterView, { ref: view, character: mira }),
      h(DialogueSeries, { plan, views: { mira: view } }));
    const clips = mount(Root).collectAudio();
    const first = clips.find((clip) => Math.round(clip.start * 30) === 0);
    const second = clips.find((clip) => Math.round(clip.start * 30) === 10);
    close(first?.volume, 0.8 * gain, 1e-12);
    // Sequence-local key times survive; only values are scaled.
    const keyframes = keyframesOf(second?.volume);
    assert.deepEqual(keyframes.map((key) => key.time), fade.keyframes.map((key) => key.time));
    assert.deepEqual(keyframes.map((key) => key.easing), [undefined, 'ease-out', undefined]);
    keyframes.forEach((key, index) => close(key.value, fade.keyframes[index].value * gain, 1e-12));
  });
});
