// `@celesta-plugin/<id>` through the built CLI (bin/ loads dist/cli.js): run
// `pnpm run build:runtime` (repository root) first. The plugins are the
// samples in examples/plugins, found through plugin directories on
// CELESTA_PLUGIN_PATH whose manifests name the samples' TypeScript sources;
// the CLI bundles them into the entry as esbuild would any module.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, test } from 'vitest';

import type { Layer, Scene } from '@celesta/react';
import { findPlugins, resolvePluginModule } from '../src/plugins.js';
import { contentOf } from './layer-content.mjs';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));
const samples = fileURLToPath(new URL('../../../examples/plugins/', import.meta.url));

const scratch: string[] = [];
afterEach(() => {
  for (const dir of scratch.splice(0)) rmSync(dir, { recursive: true, force: true });
});

/** A directory of plugin directories, one per sample in `ids`, with manifests naming their sources. */
function pluginPath(ids: string[]): string {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-plugins-'));
  scratch.push(dir);
  for (const id of ids) {
    mkdirSync(join(dir, id));
    const module = join(samples, id, 'js', 'index.ts');
    writeFileSync(join(dir, id, 'celesta-plugin.json'), JSON.stringify({ id, version: '0.0.0', protocol: 1, module }));
  }
  return dir;
}

/** The scenes of 30 fps frames `frames` of `source`, with plugins from `plugins`. */
function render(source: string, frames: number[], plugins: string): { scenes: Scene[]; dir: string } {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-plugin-entry-'));
  scratch.push(dir);
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, source);
  const result = spawnSync(process.execPath, [cli, entry], {
    input: frames.map((frame) => `{"time":{"value":${frame},"timescale":30}}\n`).join(''),
    encoding: 'utf8',
    env: { ...process.env, CELESTA_PLUGIN_PATH: plugins },
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const [, ...lines] = result.stdout.split(/\r?\n/).filter(Boolean);
  return { scenes: lines.map((line) => (JSON.parse(line) as { scene: Scene }).scene), dir };
}

/** Every layer in `layers`, groups' children included. */
function flatten(layers: Layer[]): Layer[] {
  return layers.flatMap((layer) => [layer, ...(layer.content.type === 'group' ? flatten(layer.content.layers) : [])]);
}

test('finds plugins in each path, first of an id winning', () => {
  const first = pluginPath(['flag']);
  const second = pluginPath(['flag', 'beep-voice']);
  const found = findPlugins([first, second].join(delimiter));
  assert.deepEqual(
    found.map((plugin) => [plugin.id, plugin.directory]),
    [
      ['flag', join(first, 'flag')],
      ['beep-voice', join(second, 'beep-voice')],
    ],
  );
  const previous = process.env.CELESTA_PLUGIN_PATH;
  process.env.CELESTA_PLUGIN_PATH = second;
  try {
    assert.deepEqual(resolvePluginModule('@celesta-plugin/flag'), { path: join(samples, 'flag', 'js', 'index.ts') });
    assert.match((resolvePluginModule('@celesta-plugin/missing') as { error: string }).error, /no plugin "missing"/);
  } finally {
    if (previous === undefined) delete process.env.CELESTA_PLUGIN_PATH;
    else process.env.CELESTA_PLUGIN_PATH = previous;
  }
});

test('<Flag> puts plugin content in the scene on the sequence clock', () => {
  const { scenes } = render(
    `import { Composition, Sequence } from '@celesta/react';
     import { Flag } from '@celesta-plugin/flag';
     export default function Root() {
       return <Composition width={320} height={240} fps={30} durationInFrames={90}>
         <Sequence from={30}><Flag id="flag" src="flag.png" x={10} amplitude={0.1} /></Sequence>
       </Composition>;
     }`,
    [45],
    pluginPath(['flag']),
  );
  const layer = flatten(scenes[0]!.layers).find((candidate) => candidate.id.endsWith('flag'));
  const content = contentOf(layer, 'plugin');
  assert.equal(content.plugin, 'flag');
  assert.equal(content.time, 0.5);
  assert.ok(content.assets?.image?.id.endsWith('flag.png'));
  assert.deepEqual(content.data, { amplitude: 0.1 });
});

test('an entry that imports a plugin no directory provides fails naming it', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-plugin-entry-'));
  scratch.push(dir);
  const entry = join(dir, 'entry.tsx');
  // Used, or esbuild drops the import before resolving it.
  writeFileSync(entry, `import { Flag } from '@celesta-plugin/flag'; export default function Root() { return <Flag src="a.png" />; }`);
  const result = spawnSync(process.execPath, [cli, entry], {
    input: '',
    encoding: 'utf8',
    env: { ...process.env, CELESTA_PLUGIN_PATH: pluginPath([]) },
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stdout + result.stderr, /no plugin \\?"flag\\?" is installed/);
});

// planDialogue() measures the WAVs through the host, which this CLI alone
// does not answer; the lines it would be given are checked instead.
test('beep-voice voices lines in prepare() with audio and lip sync', () => {
  const { scenes, dir } = render(
    `import { Composition, Text } from '@celesta/react';
     import { prepareBeepNarration } from '@celesta-plugin/beep-voice';
     let summary = '';
     export async function prepare() {
       const lines = await prepareBeepNarration(
         [{ id: 'a', speaker: 's', text: 'こんにちは。' }, { id: 'b', speaker: 's', text: 'こんにちは。' }],
         { speakers: { s: { pitch: 300 } }, cacheDir: './voices' },
       );
       summary = JSON.stringify({
         audio: lines.map((line) => line.audio),
         duration: lines[0].lipSync.durationInSeconds,
         mouth: [0.06, 0.25, 0.5].map((seconds) => lines[0].lipSync.mouthAtSeconds(seconds)),
       });
     }
     export default function Root() {
       return <Composition width={64} height={64} fps={30} durationInFrames={1}><Text id="summary">{summary}</Text></Composition>;
     }`,
    [0],
    pluginPath(['beep-voice']),
  );
  const text = contentOf(flatten(scenes[0]!.layers).find((layer) => layer.id.endsWith('summary')), 'text');
  const summary = JSON.parse(text.text) as { audio: string[]; duration: number; mouth: string[] };
  // The same text and voice share one WAV, kept where the entry asked.
  assert.equal(summary.audio[0], summary.audio[1]);
  assert.ok(summary.audio[0]!.startsWith(join(dir, 'voices')), summary.audio[0]);
  assert.ok(existsSync(summary.audio[0]!));
  // A lead-in, five beeps at twelve a second, a stop of two, and a tail: 24 kHz 16-bit mono.
  assert.ok(Math.abs(summary.duration - (0.05 + 7 / 12 + 0.15)) < 1e-4, String(summary.duration));
  const wav = readFileSync(summary.audio[0]!);
  assert.equal(wav.subarray(8, 12).toString('ascii'), 'WAVE');
  assert.equal(wav.length, 44 + Math.round(summary.duration * 24_000) * 2);
  // The first beep (こ) rounds the mouth, the third (に) spreads it, and the stop closes it.
  assert.deepEqual(summary.mouth, ['o', 'i', 'closed']);
});
