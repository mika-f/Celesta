import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

for (const packaged of [false, true]) {
  test(`${packaged ? 'packaged' : 'source'} CLI resolves VOICEVOX outside the workspace and shares the lip-sync clock`, () => {
    const root = mkdtempSync(join(tmpdir(), 'celesta-voicevox-'));
    try {
      let cli = fileURLToPath(new URL('../../react/dist/cli.js', import.meta.url));
      if (packaged) {
        const runtime = join(root, 'runtime');
        const stage = fileURLToPath(new URL('../../../scripts/stage-react-runtime.mjs', import.meta.url));
        const result = spawnSync(process.execPath, [stage, runtime], { encoding: 'utf8', timeout: 30000 });
        assert.equal(result.status, 0, result.stdout + result.stderr);
        cli = join(runtime, 'dist/cli.js');
        const types = join(runtime, 'dist/project-types');
        assert.match(readFileSync(join(types, 'node_modules/@celesta/voicevox/dist/index.d.ts'), 'utf8'), /VoicevoxAudioQuery/);
        assert.equal(JSON.parse(readFileSync(join(runtime, 'runtime-packages.json'), 'utf8'))['@celesta/voicevox'],
          JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version);
        const adapter = join(runtime, 'node_modules/@celesta/voicevox');
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
        import { Composition, Rect, useLipSync, type LipSyncTrack } from '@celesta/react';
        import { lipSyncFromVoicevox, type VoicevoxAudioQuery } from '@celesta/voicevox';
        const query: VoicevoxAudioQuery = {
          accent_phrases: [{ moras: [
            { vowel: 'a', vowel_length: 0.2 },
            { vowel: 'i', vowel_length: 0.2 },
          ] }],
        };
        const track: LipSyncTrack = lipSyncFromVoicevox(query, { frameRate: null });
        function Mouth() {
          const mouth = useLipSync(track);
          return <Rect id={mouth} width={mouth === 'a' ? 20 : 10} height={10} />;
        }
        export default function Film() {
          return <Composition width={64} height={64} fps={10} durationInFrames={4}><Mouth /></Composition>;
        }
      `);
      if (packaged) {
        const tsc = fileURLToPath(new URL('../../react/node_modules/typescript/bin/tsc', import.meta.url));
        const checked = spawnSync(process.execPath, [tsc, '-p', join(root, 'tsconfig.json')], {
          encoding: 'utf8', timeout: 30000,
        });
        assert.equal(checked.status, 0, checked.stdout + checked.stderr);
      }
      const result = spawnSync(process.execPath, [cli, entry], {
        input: '{"time":{"value":0,"timescale":10}}\n{"time":{"value":3,"timescale":10}}\n',
        encoding: 'utf8', timeout: 30000,
      });
      assert.equal(result.status, 0, result.stdout + result.stderr);
      const [metadata, first, second] = result.stdout.trim().split(/\r?\n/).map(JSON.parse);
      assert.equal(metadata.config.durationInFrames, 4);
      assert.equal(first.scene.layers[0].id, 'a');
      assert.equal(first.scene.layers[0].content.width, 20);
      assert.equal(second.scene.layers[0].id, 'i');
      assert.equal(second.scene.layers[0].content.width, 10);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}
