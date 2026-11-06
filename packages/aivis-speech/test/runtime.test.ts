// Integration test of the build: spawns packages/cli/dist/cli.js (and the
// staged runtime), which load @celesta.studio/aivis-speech's dist/, so it needs
// `pnpm run build:runtime` first.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'vitest';

// What the CLI prints: the composition's metadata, then one scene per time sent.
interface Metadata {
  config: { durationInFrames: number };
}
interface Frame {
  scene: { layers: { id: string; content: { width: number } }[] };
}

test.for([
  { name: 'source', packaged: false },
  { name: 'packaged', packaged: true },
])('$name CLI resolves AivisSpeech outside the workspace and shares the lip-sync clock', ({ packaged }) => {
  const root = mkdtempSync(join(tmpdir(), 'celesta-aivis-speech-'));
  // Milliseconds per phase, printed so a slow CI run shows where its time went.
  const timings: Record<string, number> = {};
  let mark = performance.now();
  const lap = (phase: string) => {
    const now = performance.now();
    timings[phase] = Math.round(now - mark);
    mark = now;
  };
  try {
    let cli = fileURLToPath(new URL('../../cli/dist/cli.js', import.meta.url));
    if (packaged) {
      const runtime = join(root, 'runtime');
      const stage = fileURLToPath(new URL('../../../scripts/stage-react-runtime.mjs', import.meta.url));
      const result = spawnSync(process.execPath, [stage, runtime], { encoding: 'utf8', timeout: 30000 });
      assert.equal(result.status, 0, result.stdout + result.stderr);
      lap('stage');
      cli = join(runtime, 'dist/cli.js');
      const types = join(runtime, 'dist/project-types');
      assert.match(readFileSync(join(types, 'node_modules/@celesta.studio/aivis-speech/dist/index.d.ts'), 'utf8'), /AivisSpeechAudioQuery/);
      assert.equal(JSON.parse(readFileSync(join(runtime, 'runtime-packages.json'), 'utf8'))['@celesta.studio/aivis-speech'],
        JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version);
      const adapter = join(runtime, 'node_modules/@celesta.studio/aivis-speech');
      for (const name of ['src', 'test', 'tsconfig.json']) {
        assert.ok(!existsSync(join(adapter, name)), `Runtime must not ship ${name}`);
      }
      assert.ok(existsSync(join(adapter, 'dist/index.js')));
      assert.ok(existsSync(join(adapter, 'README.md')));
      writeFileSync(join(root, 'tsconfig.json'), JSON.stringify({
        extends: './runtime/dist/project-types/tsconfig.json',
        include: ['film.tsx'],
      }));
    }
    const entry = join(root, 'film.tsx');
    writeFileSync(entry, `
      import { Composition, Rect } from '@celesta.studio/react';
      import { lipSyncFromSpeechTiming, useLipSync, type LipSyncTrack, type SpeechTiming } from '@celesta.studio/character';
      import { lipSyncFromAivisSpeech, timingFromAivisSpeech, type AivisSpeechAudioQuery, type AivisSpeechTiming } from '@celesta.studio/aivis-speech';
      const query: AivisSpeechAudioQuery = {
        accent_phrases: [{ moras: [
          { text: 'ア', vowel: 'a' },
          { text: 'イ', vowel: 'i' },
        ] }],
        prePhonemeLength: 0,
        postPhonemeLength: 0,
      };
      // 0.4 s of an even tone: the moras split by their typical lengths.
      const samples = Float32Array.from({ length: 3200 }, (_, i) => 0.3 * Math.sin(i / 4));
      const timing: AivisSpeechTiming = timingFromAivisSpeech(query, { sampleRate: 8000, samples });
      const track: LipSyncTrack = lipSyncFromAivisSpeech(timing);
      // Every public call shape type-checks against the shipped declarations.
      const fromQuery: LipSyncTrack = lipSyncFromAivisSpeech(query, { sampleRate: 8000, samples });
      const fromEither = (input: AivisSpeechAudioQuery | AivisSpeechTiming): LipSyncTrack =>
        lipSyncFromAivisSpeech(input, 'accent_phrases' in input ? { sampleRate: 8000, samples } : undefined);
      // An AivisSpeech timing is the engine-neutral SpeechTiming.
      const shared: SpeechTiming = timing;
      const fromShared: LipSyncTrack = lipSyncFromSpeechTiming(shared);
      void fromQuery;
      void fromEither;
      void fromShared;
      function Mouth() {
        const mouth = useLipSync(track);
        return <Rect id={mouth} width={mouth === 'a' ? 20 : 10} height={10} />;
      }
      export default function Film() {
        return <Composition width={64} height={64} fps={10} durationInFrames={4}><Mouth /></Composition>;
      }
    `);
    if (packaged) {
      const tsc = fileURLToPath(new URL('../../cli/node_modules/typescript/bin/tsc', import.meta.url));
      const checked = spawnSync(process.execPath, [tsc, '-p', join(root, 'tsconfig.json')], {
        encoding: 'utf8', timeout: 30000,
      });
      assert.equal(checked.status, 0, checked.stdout + checked.stderr);
      lap('tsc');
    }
    const result = spawnSync(process.execPath, [cli, entry], {
      input: '{"time":{"value":0,"timescale":10}}\n{"time":{"value":3,"timescale":10}}\n',
      encoding: 'utf8', timeout: 30000,
    });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    lap('cli');
    if (packaged) console.log(`packaged AivisSpeech phases (ms): ${JSON.stringify(timings)}`);
    const [metadata, first, second] = result.stdout.trim().split(/\r?\n/).map((line) => JSON.parse(line)) as [Metadata, Frame, Frame];
    assert.equal(metadata.config.durationInFrames, 4);
    assert.equal(first.scene.layers[0].id, 'a');
    assert.equal(first.scene.layers[0].content.width, 20);
    assert.equal(second.scene.layers[0].id, 'i');
    assert.equal(second.scene.layers[0].content.width, 10);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
