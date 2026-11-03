// `spokenText`: what a line says, apart from what its subtitle shows. Every
// WAV here is generated, so no speech engine or voice file is needed.

import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'vitest';
import React from 'react';

import { Assets, Composition, Span } from '@celesta.studio/react';
import type { AssetReference } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';
import {
  Character, CharacterView, DialogueSeries, loadDialogueLipSync, loadLipSync, planDialogue, spokenTextOf,
} from '../src/index.js';
import type { CharacterViewReference, DialogueLine, LipSyncTrack } from '../src/index.js';

const h = React.createElement;

/** A mono 16-bit PCM WAV: a 220 Hz tone, on for the given seconds, then off, in turn. */
function wav(pattern: number[], sampleRate = 8000): Buffer {
  const samples: number[] = [];
  pattern.forEach((seconds, index) => {
    for (let i = 0; i < Math.round(seconds * sampleRate); i += 1) {
      samples.push(index % 2 === 0 ? Math.round(Math.sin((i / sampleRate) * 2 * Math.PI * 220) * 12000) : 0);
    }
  });
  const bytes = Buffer.alloc(44 + samples.length * 2);
  bytes.write('RIFF', 0, 'ascii');
  bytes.writeUInt32LE(36 + samples.length * 2, 4);
  bytes.write('WAVEfmt ', 8, 'ascii');
  bytes.writeUInt32LE(16, 16);
  bytes.writeUInt16LE(1, 20);
  bytes.writeUInt16LE(1, 22);
  bytes.writeUInt32LE(sampleRate, 24);
  bytes.writeUInt32LE(sampleRate * 2, 28);
  bytes.writeUInt16LE(2, 32);
  bytes.writeUInt16LE(16, 34);
  bytes.write('data', 36, 'ascii');
  bytes.writeUInt32LE(samples.length * 2, 40);
  samples.forEach((sample, i) => bytes.writeInt16LE(sample, 44 + i * 2));
  return bytes;
}

async function withEntryDir<T>(files: Record<string, Buffer>, fn: (dir: string) => Promise<T>): Promise<T> {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-spoken-'));
  for (const [name, contents] of Object.entries(files)) writeFileSync(join(dir, name), contents);
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

const mouths = (track: LipSyncTrack | undefined, seconds: number[]) => seconds.map((at) => track?.mouthAtSeconds(at));
const SAMPLES = [0.05, 0.25, 0.45, 0.65, 0.85, 1.05, 1.25, 1.45];
const collect = (warnings: string[]) => ({ onWarning: (message: string) => warnings.push(message) });

test('spokenTextOf is spokenText, else a string text', () => {
  assert.equal(spokenTextOf({ id: 'a', text: 'HPが1', spokenText: 'えいちぴーがいち' }), 'えいちぴーがいち');
  assert.equal(spokenTextOf({ id: 'a', text: 'そのまま' }), 'そのまま');
  assert.throws(
    () => spokenTextOf({ id: 'styled', text: h(Span, { color: '#ff0000' }, 'HP') }),
    /line "styled": the text holds <Span>s, so give the line a spokenText/,
  );
  // Wrong on purpose: lines read from JSON come in untyped.
  assert.throws(
    () => spokenTextOf({ id: 'n', text: 'x', spokenText: 3 } as unknown as DialogueLine),
    /line "n": spokenText must be a string/,
  );
});

test('loadDialogueLipSync follows the spoken text, not the subtitle', async () => {
  // Four bursts of voice: one vowel each.
  await withEntryDir({ 'hp.wav': wav([0.3, 0.1, 0.3, 0.1, 0.3, 0.1, 0.3, 0.1]) }, async () => {
    const warnings: string[] = [];
    const [line] = await loadDialogueLipSync(
      [{ id: 'danger', text: 'HPが1', spokenText: 'えいちぴー', audio: 'hp.wav' }],
      collect(warnings),
    );
    assert.deepEqual(warnings, []);
    const reference = await loadLipSync({ src: 'hp.wav', text: 'えいちぴー' });
    assert.deepEqual(mouths(line.lipSync, SAMPLES), mouths(reference, SAMPLES));
    // え い ち ぴ ー: e, i, i, i, i — not the open-and-close the subtitle "HPが1" would give.
    assert.ok(mouths(line.lipSync, SAMPLES).includes('e'));
    assert.equal(line.text, 'HPが1');
    assert.equal(line.lipSync?.durationInSeconds, reference.durationInSeconds);
  });
});

test('loadDialogueLipSync without spokenText reads the text, and warns when it cannot', async () => {
  await withEntryDir({ 'v.wav': wav([0.4, 0.2]) }, async () => {
    const warnings: string[] = [];
    const lines = await loadDialogueLipSync([
      { id: 'kana', text: 'あい', audio: 'v.wav' },
      { id: 'kanji', text: '今日は', audio: 'v.wav' },
      { id: 'digits', text: '123', audio: 'v.wav' },
    ], collect(warnings));
    assert.equal(lines[0].lipSync?.mouthAtSeconds(0.05), 'a');
    assert.equal(warnings.length, 2);
    assert.match(warnings[0], /line "kanji" has kanji and no spokenText/);
    assert.match(warnings[1], /line "digits" has no kana or romaji vowels/);
    // Open and closed only.
    assert.equal(lines[1].lipSync?.mouthAtSeconds(0.05), 'a');
    assert.equal(lines[1].lipSync?.mouthAtSeconds(0.5), 'closed');
  });
});

test('loadDialogueLipSync leaves lines it has nothing to do for, and names the line it cannot read', async () => {
  await withEntryDir({ 'bad.wav': Buffer.from('not a wav') }, async () => {
    const own: LipSyncTrack = { mouthAtSeconds: () => 'o', mouthAtFrame: () => 'o', durationInSeconds: 1 };
    const silent: DialogueLine = { id: 'silent', text: 'x', durationInFrames: 10 };
    const tracked: DialogueLine = { id: 'tracked', text: 'x', audio: 'bad.wav', lipSync: own };
    const [first, second] = await loadDialogueLipSync([silent, tracked]);
    assert.equal(first, silent);
    assert.equal(second, tracked);
    await assert.rejects(loadDialogueLipSync([{ id: 'broken', text: 'あ', audio: 'bad.wav' }]), /line "broken": could not read bad.wav as a WAV/);
    await assert.rejects(loadDialogueLipSync([{ id: 'far', text: 'あ', audio: 'https://example.com/a.wav' }]), /line "far": .* is not a local file/);
    await assert.rejects(loadDialogueLipSync([{ id: 'styled', text: h(Span, null, 'あ'), audio: 'bad.wav' }]), /give the line a spokenText/);
  });
});

test('planDialogue refuses a spokenText that is not a string', async () => {
  // Wrong on purpose: lines read from JSON come in untyped.
  const line = { id: 'n', text: 'x', spokenText: 1, durationInFrames: 5 } as unknown as DialogueLine;
  await assert.rejects(planDialogue([line], { fps: 30 }), /line "n": spokenText must be a string/);
});

test('the subtitle and the subtitle file show text, never spokenText', async () => {
  const plan = await planDialogue([
    { id: 'danger', text: 'HPが1です。', spokenText: 'えいちぴーがいちです。', durationInFrames: 30, speaker: 'a' },
  ], { fps: 30, gap: 0 });
  const actor = React.createRef<AssetReference>();
  const view = React.createRef<CharacterViewReference>();
  const Root = () => h(Composition, { width: 320, height: 180, fps: 30, durationInFrames: 30 },
    h(Assets, null, h(Character, { ref: actor, name: 'a', portrait: { defaultExpression: 'n', expressions: { n: 'n.png' } } })),
    h(CharacterView, { ref: view, character: actor }),
    h(DialogueSeries, { plan, views: { a: view } }));
  const mounted = mount(Root);
  const json = JSON.stringify(mounted.renderAt({ value: 10, timescale: 30 }, null).scene);
  assert.ok(json.includes('HPが1です。'));
  assert.ok(!json.includes('えいちぴー'));
  assert.deepEqual(mounted.collectSubtitles(0, 30).map((cue) => cue.text), ['HPが1です。']);
});
