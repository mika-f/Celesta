// `importVoiceFolder()`: pairing, ordering, encodings, manifests, partial
// writes, and the preparation dependencies the editor watches. Every fixture
// is generated: PCM WAVs of a tone, transcripts in temporary folders. The
// last test is an integration test of the build: it runs the CLI, which
// loads @celesta/character from dist/.

import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, test } from 'vitest';

import { decodeWav, importVoiceFolder, naturalCompare, planDialogue } from '../src/index.js';
import type { VoiceTextEncoding } from '../src/index.js';
import { prepareDependencies, setMediaProbe } from '@celesta/react/internal';
import { errorOf, parseMessage } from './render-cli.mjs';
import type { CliMessage } from './render-cli.mjs';

const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));

/** A mono 16-bit PCM WAV: `seconds` of a tone, loud enough to open the mouth. */
function wav(seconds: number, sampleRate = 8000): Buffer {
  const frames = Math.round(seconds * sampleRate);
  const bytes = Buffer.alloc(44 + frames * 2);
  bytes.write('RIFF', 0, 'ascii');
  bytes.writeUInt32LE(36 + frames * 2, 4);
  bytes.write('WAVE', 8, 'ascii');
  bytes.write('fmt ', 12, 'ascii');
  bytes.writeUInt32LE(16, 16);
  bytes.writeUInt16LE(1, 20);
  bytes.writeUInt16LE(1, 22);
  bytes.writeUInt32LE(sampleRate, 24);
  bytes.writeUInt32LE(sampleRate * 2, 28);
  bytes.writeUInt16LE(2, 32);
  bytes.writeUInt16LE(16, 34);
  bytes.write('data', 36, 'ascii');
  bytes.writeUInt32LE(frames * 2, 40);
  for (let i = 0; i < frames; i += 1) {
    bytes.writeInt16LE(Math.round(Math.sin((i / sampleRate) * 2 * Math.PI * 220) * 12000), 44 + i * 2);
  }
  return bytes;
}

let cache: string;
let previousHome: string | undefined;
let previousXdg: string | undefined;
beforeAll(() => {
  // Snapshots go to a throwaway cache, not the developer's.
  cache = mkdtempSync(join(tmpdir(), 'celesta-voice-cache-'));
  previousHome = process.env.HOME;
  previousXdg = process.env.XDG_CACHE_HOME;
  process.env.HOME = cache;
  process.env.XDG_CACHE_HOME = cache;
  process.env.LOCALAPPDATA = cache;
});
afterAll(() => {
  process.env.HOME = previousHome;
  if (previousXdg === undefined) delete process.env.XDG_CACHE_HOME;
  else process.env.XDG_CACHE_HOME = previousXdg;
  rmSync(cache, { recursive: true, force: true });
});

/** Runs `fn(dir)` with a temporary entry directory holding `files`. */
async function withFolder<T>(files: Record<string, string | Buffer>, fn: (dir: string) => Promise<T>): Promise<T> {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-voices-'));
  for (const [name, contents] of Object.entries(files)) {
    mkdirSync(join(dir, name, '..'), { recursive: true });
    writeFileSync(join(dir, name), contents);
  }
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

const quiet = { onWarning: () => {} };

test('naturalCompare orders digit runs by value, the same everywhere', () => {
  const names = ['voice-10', 'Voice-2', 'voice-1', 'voice-02', 'voice-2', 'a', 'B', 'voice-1b'];
  assert.deepEqual([...names].sort(naturalCompare), ['a', 'B', 'voice-1', 'voice-1b', 'Voice-2', 'voice-02', 'voice-2', 'voice-10']);
});

test('a folder of pairs becomes lines with stable ids, subtitles, durations, and a speaker', async () => {
  await withFolder(
    {
      'voices/voice-10.wav': wav(0.3),
      'voices/voice-10.txt': 'じゅう',
      'voices/voice-2.wav': wav(1),
      'voices/voice-2.txt': '﻿  こんにちは\r\nせかい  \r\n',
      'voices/voice-1.WAV': wav(0.5),
      'voices/voice-1.txt': 'あいうえお',
      'voices/notes.md': 'ignored',
      'voices/.voice-3.wav': wav(0.1),
      'voices/sub/voice-4.wav': wav(0.1),
      'voices/sub/voice-4.txt': 'nested',
    },
    async (dir) => {
      const lines = await importVoiceFolder({ directory: './voices', speaker: 'akane', ...quiet });
      assert.deepEqual(lines.map((line) => line.id), ['voice-1', 'voice-2', 'voice-10']);
      assert.equal(lines[1].text, 'こんにちは\nせかい');
      assert.equal(lines[1].durationInSeconds, 1);
      assert.equal(lines[0].speaker, 'akane');
      assert.equal(lines[0].source.wav, join(dir, 'voices', 'voice-1.WAV'));
      // The audio is a snapshot holding the same bytes.
      assert.notEqual(lines[0].audio, lines[0].source.wav);
      assert.equal(basename(lines[0].audio), 'voice-1.WAV');
      assert.deepEqual(readFileSync(lines[0].audio), readFileSync(lines[0].source.wav));
      // Waveform lip sync takes the transcript's vowels while the tone sounds.
      assert.equal(lines[0].lipSync!.mouthAtSeconds(0.05), 'a');
      assert.equal(lines[0].lipSync!.mouthAtSeconds(0.45), 'o');
      assert.equal(lines[0].lipSync!.durationInSeconds, 0.5);

      const recursive = await importVoiceFolder({ directory: 'voices', recursive: true, ...quiet });
      assert.deepEqual(recursive.map((line) => line.id), ['sub/voice-4', 'voice-1', 'voice-2', 'voice-10']);
    },
  );
});

test('planDialogue lays imported lines out from their real lengths', async () => {
  await withFolder({ 'v/a.wav': wav(1), 'v/a.txt': 'あ', 'v/b.wav': wav(0.5), 'v/b.txt': 'い' }, async () => {
    setMediaProbe(async (file) => {
      const audio = decodeWav(new Uint8Array(readFileSync(file)));
      const seconds = audio.samples.length / audio.sampleRate;
      return { durationSeconds: seconds, audio: [{ durationSeconds: seconds }] };
    });
    const plan = await planDialogue(await importVoiceFolder({ directory: 'v', ...quiet }), { fps: 30, gap: 0 });
    assert.deepEqual(plan.lines.map((line) => [line.id, line.from, line.durationInFrames]), [['a', 0, 30], ['b', 30, 15]]);
  });
});

test('a manifest overrides ids, order, speaker, text, spokenText, scene, expression, and gap', async () => {
  const manifest = {
    lines: [
      { file: 'voice-2', id: 'greeting', speaker: 'aoi', text: '今日は', spokenText: 'きょうは', scene: 'intro', expression: 'smile', gap: 0.5 },
      { file: 'Voice-1', expression: [{ at: 0.2, expression: 'angry' }] },
    ],
  };
  await withFolder(
    {
      'v/voice-1.wav': wav(0.2),
      'v/voice-1.txt': 'いち',
      'v/voice-2.wav': wav(0.2),
      'v/voice-2.txt': 'こんにちは',
      'v/voice-3.wav': wav(0.2),
      'v/voice-3.txt': 'さん',
      'v/manifest.json': JSON.stringify(manifest),
    },
    async () => {
      const lines = await importVoiceFolder({ directory: 'v', speaker: 'akane', manifest: 'v/manifest.json', ...quiet });
      assert.deepEqual(lines.map((line) => line.id), ['greeting', 'voice-1', 'voice-3']);
      assert.equal(lines[0].speaker, 'aoi');
      assert.equal(lines[0].text, '今日は');
      assert.equal(lines[0].spokenText, 'きょうは');
      assert.equal(lines[0].scene, 'intro');
      assert.equal(lines[0].expression, 'smile');
      assert.equal(lines[0].gap, 0.5);
      // The spoken text, not the kanji subtitle, gives the vowels: き(i) ょ(o) う(u) は(a).
      assert.equal(lines[0].lipSync!.mouthAtSeconds(0.01), 'o');
      assert.deepEqual(lines[1].expression, [{ at: 0.2, expression: 'angry' }]);
      assert.equal(lines[2].speaker, 'akane');
    },
  );
});

test('manifest mistakes are reported with the file and the line', async () => {
  const cases: [unknown, RegExp][] = [
    [{ lines: [{ file: 'voice-1', colour: 'red' }] }, /lines\[0\] has unknown key "colour"/],
    [{ lines: [{ file: 'voice-1', reading: 'あ' }] }, /lines\[0\]\.reading is now spokenText/],
    [{ lines: [{ file: 'voice-1', spokenText: 1 }] }, /lines\[0\]\.spokenText must be a string/],
    [{ lines: [{ file: 'voice-1.wav' }] }, /without its extension: "voice-1"/],
    [{ lines: [{ file: 'voice-1', gap: -1 }] }, /lines\[0\]\.gap must be a number of seconds >= 0/],
    [{ lines: [{ file: 'voice-9' }] }, /lists "voice-9", but the folder has no voice-9\.wav/],
    [{ lines: [{ file: 'voice-1' }, { file: 'VOICE-1' }] }, /lists "VOICE-1" more than once/],
    [{ lines: [{ file: 'voice-1', id: 'voice-2' }] }, /line id "voice-2" is used by both/],
    [{ items: [] }, /unknown key "items"/],
  ];
  for (const [manifest, expected] of cases) {
    await withFolder(
      { 'voice-1.wav': wav(0.1), 'voice-1.txt': 'あ', 'voice-2.wav': wav(0.1), 'voice-2.txt': 'い', 'm.json': JSON.stringify(manifest) },
      async (dir) => {
        await assert.rejects(importVoiceFolder({ directory: '.', manifest: 'm.json', ...quiet }), (error) => {
          assert.ok(error instanceof Error);
          assert.match(error.message, expected);
          return true;
        });
        void dir;
      },
    );
  }
  await withFolder({ 'voice-1.wav': wav(0.1), 'voice-1.txt': 'あ', 'm.json': '{ nope' }, async () => {
    await assert.rejects(importVoiceFolder({ directory: '.', manifest: 'm.json' }), /m\.json is not valid JSON/);
  });
});

test('transcripts decode as UTF-8 by default and as UTF-16LE or Shift_JIS when asked', async () => {
  const utf16 = Buffer.concat([Buffer.from([0xff, 0xfe]), Buffer.from('こんにちは', 'utf16le')]);
  const shiftJis = Buffer.from([0x82, 0xa0, 0x82, 0xa2]); // あい
  await withFolder({ 'u/a.wav': wav(0.1), 'u/a.txt': utf16, 's/a.wav': wav(0.1), 's/a.txt': shiftJis }, async () => {
    await assert.rejects(importVoiceFolder({ directory: 'u' }), /UTF-16 byte order mark; pass encoding: 'utf-16le'/);
    assert.equal((await importVoiceFolder({ directory: 'u', encoding: 'utf-16le' }))[0].text, 'こんにちは');

    await assert.rejects(importVoiceFolder({ directory: 's' }), /is not valid utf-8; set `encoding`/);
    assert.equal((await importVoiceFolder({ directory: 's', encoding: 'shift_jis' }))[0].text, 'あい');

    const custom = await importVoiceFolder({ directory: 's', encoding: (bytes) => `${bytes.length} bytes` });
    assert.equal(custom[0].text, '4 bytes');
    await assert.rejects(importVoiceFolder({ directory: 's', encoding: () => 'bad �' }), /replacement characters/);
    // Not a supported encoding, cast to check the error.
    await assert.rejects(importVoiceFolder({ directory: 's', encoding: 'latin1' as VoiceTextEncoding }), /unsupported encoding "latin1"/);
  });
  // UTF-16LE without a BOM decodes as UTF-8 full of NULs.
  await withFolder({ 'a.wav': wav(0.1), 'a.txt': Buffer.from('hi', 'utf16le') }, async () => {
    await assert.rejects(importVoiceFolder({ directory: '.' }), /NUL characters; it is probably UTF-16/);
  });
});

test('unpaired, empty, and truncated files fail with every file named, or are skipped when asked', async () => {
  const truncated = wav(0.5).subarray(0, 200);
  await withFolder(
    {
      'good.wav': wav(0.1),
      'good.txt': 'よい',
      'lonely.wav': wav(0.1),
      'orphan.txt': 'みなしご',
      'empty.wav': wav(0.1),
      'empty.txt': '  \n',
      'cut.wav': truncated,
      'cut.txt': 'きれた',
    },
    async (dir) => {
      await assert.rejects(importVoiceFolder({ directory: '.' }), (error) => {
        assert.ok(error instanceof Error);
        assert.match(error.message, new RegExp(`${escape(join(dir, 'lonely.wav'))} has no matching \\.txt file`));
        assert.match(error.message, new RegExp(`${escape(join(dir, 'orphan.txt'))} has no matching \\.wav file`));
        assert.match(error.message, /empty\.txt is incomplete \(the transcript is empty\)/);
        assert.match(error.message, /cut\.wav is incomplete \(WAV data chunk is truncated/);
        assert.match(error.message, /incomplete: 'skip'/);
        return true;
      });
      const warnings: string[] = [];
      const lines = await importVoiceFolder({ directory: '.', incomplete: 'skip', onWarning: (m) => warnings.push(m) });
      assert.deepEqual(lines.map((line) => line.id), ['good']);
      assert.equal(warnings.length, 4);
    },
  );
});

test('names that differ only in case pair, but two of one kind are ambiguous', async () => {
  await withFolder({ 'Hello.wav': wav(0.1), 'hello.txt': 'はろー' }, async () => {
    const lines = await importVoiceFolder({ directory: '.', ...quiet });
    assert.deepEqual(lines.map((line) => line.id), ['Hello']);
  });
  if (process.platform === 'linux') {
    await withFolder({ 'a.wav': wav(0.1), 'A.wav': wav(0.1), 'a.txt': 'あ' }, async () => {
      await assert.rejects(importVoiceFolder({ directory: '.' }), /differ only in case/);
    });
  }
});

test('text without kana vowels falls back to an open/closed mouth with a warning', async () => {
  await withFolder({ 'k.wav': wav(0.2), 'k.txt': '今日は晴天', 'n.wav': wav(0.2), 'n.txt': '123' }, async () => {
    const warnings: string[] = [];
    const lines = await importVoiceFolder({ directory: '.', onWarning: (m) => warnings.push(m) });
    assert.equal(warnings.length, 2);
    assert.match(warnings[0], /line "k" has kanji and no spokenText/);
    assert.match(warnings[1], /line "n" has no kana or romaji vowels/);
    assert.equal(lines[0].lipSync!.mouthAtSeconds(0.1), 'a');
    const off = await importVoiceFolder({ directory: '.', lipSync: false, ...quiet });
    assert.equal(off[0].lipSync, undefined);
  });
});

test('a resaved recording under the same name gets a new snapshot and length', async () => {
  await withFolder({ 'a.wav': wav(0.5), 'a.txt': 'あ' }, async (dir) => {
    const [before] = await importVoiceFolder({ directory: '.', ...quiet });
    // Saved atomically, as many tools do: write beside, then rename over.
    writeFileSync(join(dir, 'a.wav.tmp'), wav(1.5));
    renameSync(join(dir, 'a.wav.tmp'), join(dir, 'a.wav'));
    writeFileSync(join(dir, 'a.txt'), 'ああ');
    const [after] = await importVoiceFolder({ directory: '.', ...quiet });
    assert.equal(before.durationInSeconds, 0.5);
    assert.equal(after.durationInSeconds, 1.5);
    assert.notEqual(after.audio, before.audio);
    assert.equal(after.text, 'ああ');
  });
});

test('the folder, its files, and the manifest are recorded as preparation dependencies', async () => {
  await withFolder({ 'v/a.wav': wav(0.1), 'v/a.txt': 'あ' }, async (dir) => {
    await importVoiceFolder({ directory: 'v', manifest: 'v/missing.json' }).catch(() => {});
    await importVoiceFolder({ directory: 'v', ...quiet });
    const recorded = new Map(prepareDependencies().map((dependency) => [dependency.path, dependency]));
    for (const file of ['v', 'v/a.wav', 'v/a.txt']) {
      const dependency = recorded.get(join(dir, file));
      assert.ok(dependency, `${file} is recorded`);
      assert.match(String(dependency.mtimeNs), /^\d+$/);
    }
    // A manifest that does not exist yet is watched for its creation.
    assert.deepEqual(recorded.get(join(dir, 'v/missing.json')), { path: join(dir, 'v/missing.json'), mtimeNs: null, size: null });
  });
});

test('the CLI reports the files prepare() read in its ready message', async (t) => {
  await withFolder({ 'voices/a.wav': wav(0.1), 'voices/a.txt': 'あ' }, async (dir) => {
    const entry = join(dir, 'entry.tsx');
    writeFileSync(
      entry,
      `import { Composition } from '@celesta/react';
import { importVoiceFolder } from '@celesta/character';
let count = 0;
export async function prepare() { count = (await importVoiceFolder({ directory: './voices' })).length; }
export default function Entry() { return <Composition width={16} height={16} fps={30} durationInFrames={count} />; }
`,
    );
    const child = spawn(process.execPath, [cli, entry], { stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env } });
    t.signal.addEventListener('abort', () => child.kill(), { once: true });
    try {
      let ready: CliMessage | undefined;
      for await (const line of createInterface({ input: child.stdout })) {
        ready = parseMessage(line);
        child.stdin.end();
        break;
      }
      assert.ok(ready, 'a ready message');
      assert.equal(errorOf(ready), undefined, 'a ready message without an error');
      assert.ok('config' in ready, 'a ready message');
      assert.equal(ready.config.durationInFrames, 1);
      const paths = ready.dependencies.map((dependency) => dependency.path);
      assert.ok(paths.includes(join(dir, 'voices')));
      assert.ok(paths.includes(join(dir, 'voices', 'a.wav')));
      assert.ok(paths.includes(join(dir, 'voices', 'a.txt')));
    } finally {
      child.kill();
    }
  });
});

function escape(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
