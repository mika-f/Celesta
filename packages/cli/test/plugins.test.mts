// `@celesta-plugin/<id>` through the built CLI (bin/ loads dist/cli.js): run
// `pnpm run build:runtime` (repository root) first. The plugins are the
// samples in examples/plugins, copied into plugin directories on
// CELESTA_PLUGIN_PATH whose manifests name the samples' TypeScript sources;
// the CLI bundles them into the entry as esbuild would any module.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { afterEach, test } from 'vitest';

import type { Layer, Scene } from '@celesta.studio/react';
import {
  findPlugins,
  isContainedPath,
  isValidPluginId,
  manifestPath,
  resolvePluginImport,
  resolvePluginModule,
  standInSource,
} from '../src/plugins.js';
import { contentOf } from './layer-content.mjs';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));
const samples = fileURLToPath(new URL('../../../examples/plugins/', import.meta.url));

const scratch: string[] = [];
afterEach(() => {
  for (const dir of scratch.splice(0)) rmSync(dir, { recursive: true, force: true });
});

/** A directory of plugin directories, one per sample in `ids`, each with a copy of the sample's sources. */
function pluginPath(ids: string[]): string {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-plugins-'));
  scratch.push(dir);
  for (const id of ids) {
    cpSync(join(samples, id, 'js'), join(dir, id, 'js'), { recursive: true });
    writeManifest(join(dir, id), { id, module: 'js/index.ts' });
  }
  return dir;
}

function writeManifest(directory: string, fields: { id: string; module?: string }): void {
  mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, 'celesta-plugin.json'), JSON.stringify({ version: '0.0.0', protocol: 1, ...fields }));
}

/** The run of the built CLI on `source` for 30 fps frames `frames`, with plugins from `plugins` and waiting ones from `waiting`. */
function run(source: string, frames: number[], plugins: string, waiting = '') {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-plugin-entry-'));
  scratch.push(dir);
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, source);
  const result = spawnSync(process.execPath, [cli, entry], {
    input: frames.map((frame) => `{"time":{"value":${frame},"timescale":30}}\n`).join(''),
    encoding: 'utf8',
    env: { ...process.env, CELESTA_PLUGIN_PATH: plugins, CELESTA_WAITING_PLUGIN_PATH: waiting },
  });
  return { result, dir };
}

/** The scenes of 30 fps frames `frames` of `source`, with plugins from `plugins` and waiting ones from `waiting`. */
function render(source: string, frames: number[], plugins: string, waiting = ''): { scenes: Scene[]; dir: string } {
  const { result, dir } = run(source, frames, plugins, waiting);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const [, ...lines] = result.stdout.split(/\r?\n/).filter(Boolean);
  return { scenes: lines.map((line) => (JSON.parse(line) as { scene: Scene }).scene), dir };
}

/** Every layer in `layers`, groups' children included. */
function flatten(layers: Layer[]): Layer[] {
  return layers.flatMap((layer) => [layer, ...(layer.content.type === 'group' ? flatten(layer.content.layers) : [])]);
}

test('finds plugins in each path, first of an id winning', async () => {
  // A plugin directory itself, then a directory of them.
  const first = join(pluginPath(['flag']), 'flag');
  const second = pluginPath(['flag', 'beep-voice']);
  const plugins = await findPlugins([first, join(second, 'missing'), second].join(delimiter));
  assert.deepEqual(
    plugins.found.map((plugin) => [plugin.id, plugin.directory]),
    [
      ['flag', first],
      ['beep-voice', join(second, 'beep-voice')],
    ],
  );
  assert.deepEqual(plugins.skipped, []);
  assert.deepEqual(resolvePluginModule('@celesta-plugin/flag', plugins), { path: join(first, 'js', 'index.ts') });
  assert.match((resolvePluginModule('@celesta-plugin/missing', plugins) as { error: string }).error, /no plugin "missing"/);
});

test('plugin ids and manifest paths stay inside their directories', () => {
  for (const id of ['flag', 'beep-voice', 'a.b_c', '0']) assert.ok(isValidPluginId(id), id);
  for (const id of ['', '..', '.hidden', '-x', 'Flag', 'a/b', 'a\\b', 'c:', 'ü']) assert.ok(!isValidPluginId(id), id);
  for (const file of ['bin/p.exe', './js/index.js', 'a..b/c', 'js\\index.js']) assert.ok(isContainedPath(file), file);
  for (const file of ['', '../p', 'bin/../../p', 'bin\\..\\p', '/usr/p', '\\\\server\\p', 'C:\\p', 'C:p', 'p:stream']) {
    assert.ok(!isContainedPath(file), file);
  }
});

test('a module path written with \\ names the same file on every platform', async () => {
  const dir = pluginPath(['flag']);
  writeManifest(join(dir, 'flag'), { id: 'flag', module: 'js\\index.ts' });
  const plugins = await findPlugins(dir);
  assert.deepEqual(plugins.skipped, []);
  const [flag] = plugins.found;
  assert.equal(flag?.module, join(dir, 'flag', 'js', 'index.ts'));
  assert.ok(flag?.module !== undefined && existsSync(flag.module));
  assert.equal(manifestPath(dir, './js//index.ts'), join(dir, 'js', 'index.ts'));
});

test('skips manifests that reach outside their directory, and relative paths', async () => {
  const root = mkdtempSync(join(tmpdir(), 'celesta-plugins-'));
  scratch.push(root);
  writeFileSync(join(root, 'outside.ts'), '');
  writeManifest(join(root, 'plugins', 'a'), { id: '../escape', module: 'index.ts' });
  writeManifest(join(root, 'plugins', 'b'), { id: 'b', module: '../../outside.ts' });
  writeManifest(join(root, 'plugins', 'c'), { id: 'c', module: join(root, 'outside.ts') });
  writeManifest(join(root, 'plugins', 'd'), { id: 'd', module: 'index.ts' });
  let linked = true;
  try {
    // Windows creates symbolic links only in developer mode or as an administrator.
    symlinkSync(join(root, 'outside.ts'), join(root, 'plugins', 'd', 'index.ts'), 'file');
  } catch {
    linked = false;
  }
  // The skipped "b" does not shadow a later one, as in Celesta's host.
  writeManifest(join(root, 'later', 'b'), { id: 'b' });

  const plugins = await findPlugins(['relative', join(root, 'plugins'), join(root, 'later')].join(delimiter));
  assert.deepEqual(
    plugins.found.map((plugin) => plugin.id),
    linked ? ['b'] : ['d', 'b'],
  );
  assert.deepEqual(
    plugins.skipped.map((plugin) => [plugin.id, plugin.reason]),
    [
      [undefined, 'it is not an absolute path'],
      [undefined, 'its id "../escape" is not a valid plugin id'],
      ['b', '"../../outside.ts" is not a relative path inside the plugin directory'],
      ['c', `${JSON.stringify(join(root, 'outside.ts'))} is not a relative path inside the plugin directory`],
      ...(linked ? [['d', '"index.ts" leads outside the plugin directory']] : []),
    ],
  );
  const missing = resolvePluginModule('@celesta-plugin/c', plugins) as { error: string };
  assert.match(missing.error, /no plugin "c" is installed .*was skipped: .* is not a relative path inside/);
  assert.match((resolvePluginModule('@celesta-plugin/..', plugins) as { error: string }).error, /not name a valid plugin id/);
});

test('<Flag> puts plugin content in the scene on the sequence clock', () => {
  const { scenes } = render(
    `import { Composition, Sequence } from '@celesta.studio/react';
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

test('a waiting plugin resolves to a stand-in, after the plugins that may run', async () => {
  const allowed = pluginPath(['flag']);
  const waiting = pluginPath(['flag', 'beep-voice']);
  const plugins = await findPlugins(allowed);
  const waitingPlugins = await findPlugins(waiting);
  assert.deepEqual(resolvePluginImport('@celesta-plugin/flag', plugins, waitingPlugins), {
    path: join(allowed, 'flag', 'js', 'index.ts'),
  });
  assert.deepEqual(resolvePluginImport('@celesta-plugin/beep-voice', plugins, waitingPlugins), {
    standIn: { id: 'beep-voice', module: join(waiting, 'beep-voice', 'js', 'index.ts') },
  });
  const none = { found: [], skipped: [] };
  assert.match(
    (resolvePluginImport('@celesta-plugin/missing', plugins, waitingPlugins) as { error: string }).error,
    /no plugin "missing"/,
  );
  assert.ok('error' in resolvePluginImport('@celesta-plugin/flag', none, none));
});

test('a stand-in draws nothing for components and throws for anything else', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-stand-in-'));
  scratch.push(dir);
  const file = join(dir, 'stand-in.mjs');
  writeFileSync(file, standInSource('p', ['Flag', 'default', 'useWave', 'odd-name']));
  const standIn = (await import(pathToFileURL(file).href)) as Record<string, (props?: unknown) => unknown>;
  assert.equal(standIn.Flag!({}), null);
  assert.equal(standIn.default!({ children: 'kept' }), 'kept');
  assert.throws(() => standIn.useWave!(), /plugin "p" waits for your approval/);
  assert.throws(() => standIn['odd-name']!(), /waits for your approval/);
});

test('an entry that imports a waiting plugin opens without its layers or code', () => {
  const plugins = pluginPath([]);
  const waiting = pluginPath(['flag']);
  const { scenes } = render(
    `import { Composition, Text } from '@celesta.studio/react';
     import { Flag } from '@celesta-plugin/flag';
     export default function Root() {
       return <Composition width={64} height={64} fps={30} durationInFrames={1}>
         <Flag id="flag" src="flag.png" />
         <Text id="title">kept</Text>
       </Composition>;
     }`,
    [0],
    plugins,
    join(waiting, 'flag'),
  );
  const layers = flatten(scenes[0]!.layers);
  assert.ok(!layers.some((layer) => layer.content.type === 'plugin'));
  assert.ok(layers.some((layer) => layer.id.endsWith('title')));

  // Calling one of its functions says it waits.
  const { result } = run(
    `import { Composition } from '@celesta.studio/react';
     import { prepareBeepNarration } from '@celesta-plugin/beep-voice';
     export async function prepare() { await prepareBeepNarration([], {}); }
     export default function Root() { return <Composition width={64} height={64} fps={30} durationInFrames={1} />; }`,
    [0],
    plugins,
    pluginPath(['beep-voice']),
  );
  assert.notEqual(result.status, 0);
  assert.match(result.stdout + result.stderr, /plugin \\?"beep-voice\\?" waits for your approval/);
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
    `import { Composition, Text } from '@celesta.studio/react';
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
