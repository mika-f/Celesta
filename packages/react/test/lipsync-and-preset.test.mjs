// Unit tests for the pure helpers behind auto lip sync and PSD portrait
// presets. Run after `pnpm run build`:  node --test test/
//
// The end-to-end paths (render.ts wiring, the Rust rasterizer) are covered
// by `crates/react-bridge/tests/node_integration.rs` and
// `crates/renderer`'s `rasterize_psd` tests.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import {
  buildEnvelope,
  decodeWav,
  lipSyncTimeline,
  parsePfv,
  resolveVisibleLayers,
  vowelShapes,
} from '../dist/index.js';

const wavPath = fileURLToPath(new URL('../../../examples/assets/voices/001.wav', import.meta.url));
const pfvPath = fileURLToPath(new URL('../../../examples/assets/lipsync-fixture.pfv', import.meta.url));
const kotonohaPfvPath = fileURLToPath(
  new URL('../../../examples/assets/illust/琴葉茜.pfv', import.meta.url),
);

test('vowelShapes matches the editor port for small kana and long vowels', () => {
  assert.deepEqual(vowelShapes('キャットーA'), ['a', 'o', 'o', 'a']);
  assert.deepEqual(vowelShapes('あいうえお'), ['a', 'i', 'u', 'e', 'o']);
  // 「こんにちは」: こ=o, ん/、 skipped, に=i, ち=i, は=a
  assert.deepEqual(vowelShapes('こんにちは、'), ['o', 'i', 'i', 'a']);
});

test('resolveVisibleLayers parses all-layer state and percent escapes', () => {
  const state = ['/body', '/body/base', '/face/mouth/a%2Fb', '/dup\\1'].join('\n');
  assert.deepEqual(resolveVisibleLayers(state), ['body', 'body/base', 'face/mouth/a/b', 'dup']);
  // compact form (no leading slash) is accepted too
  assert.deepEqual(resolveVisibleLayers('body\nbody/base'), ['body', 'body/base']);
});

test('parsePfv reads the fixture favorite and its multi-line state', () => {
  const { rootName, favorites } = parsePfv(readFileSync(pfvPath, 'utf8'));
  assert.equal(rootName, 'lipsync-fixture');
  assert.equal(favorites.length, 1);
  assert.equal(favorites[0].name, 'neutral');
  const layers = resolveVisibleLayers(favorites[0].state);
  assert.ok(layers.includes('body/base'));
  assert.ok(layers.includes('face/eyes/open'));
  assert.ok(!layers.some((l) => l.startsWith('face/mouth')));
});

test('parsePfv handles a real PSDTool favorite with * / ! names', () => {
  const { rootName, favorites } = parsePfv(readFileSync(kotonohaPfvPath, 'utf8'));
  assert.equal(rootName, '琴葉姉妹_SD立ち絵');
  assert.equal(favorites.length, 1);
  assert.equal(favorites[0].name, '茜 メイド 通常');
  const layers = resolveVisibleLayers(favorites[0].state);
  assert.ok(layers.length > 20);
  assert.ok(layers.includes('琴葉姉妹/!表情/目/*やさしい'));
  assert.ok(layers.includes('琴葉姉妹/胴体/*メイド服/!胴体/胴体'));
  assert.ok(!layers.some((l) => l.includes('あいうえお'))); // mouth is lip-synced, not in the pose
});

test('decodeWav reads 001.wav to normalized samples', () => {
  const audio = decodeWav(readFileSync(wavPath));
  assert.ok(audio.sampleRate === 44100 || audio.sampleRate === 48000);
  assert.ok(audio.samples.length > audio.sampleRate); // longer than 1 second
  assert.ok(audio.samples.every((s) => s >= -1.001 && s <= 1.001));
  assert.ok(audio.samples.some((s) => Math.abs(s) > 0.05)); // not silence
});

test('lipSyncTimeline gates silence closed and opens loud audio to vowels', () => {
  assert.deepEqual(lipSyncTimeline(new Float32Array(50), 'あいうえお'), Array(50).fill('closed'));

  const envelope = new Float32Array(50);
  for (let i = 10; i < 40; i += 1) envelope[i] = 0.8;
  const timeline = lipSyncTimeline(envelope, 'あいうえお');
  assert.equal(timeline[0], 'closed');
  assert.equal(timeline[49], 'closed');
  const vowels = new Set(timeline.filter((shape) => shape !== 'closed'));
  assert.ok(vowels.size > 1, `expected several vowels, got ${[...vowels]}`);
});

test('buildEnvelope produces one bucket per hop', () => {
  const audio = { sampleRate: 1000, samples: new Float32Array(1000).fill(0.5) };
  const envelope = buildEnvelope(audio, 100);
  assert.equal(envelope.length, 100);
  assert.ok(envelope.every((v) => Math.abs(v - 0.5) < 1e-6));
});
