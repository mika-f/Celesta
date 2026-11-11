import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, lstatSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';

const packageDirectory = resolve(process.argv[2]);
const mac = process.platform === 'darwin';
const runtime = mac ? join(packageDirectory, 'Contents/Resources') : join(packageDirectory, 'runtime');
const node = mac ? join(packageDirectory, 'Contents/Helpers/node') : join(runtime, 'node.exe');
const exporter = join(packageDirectory, mac ? 'Contents/MacOS/Celesta-export' : 'Celesta-export.exe');
const mcp = join(packageDirectory, mac ? 'Contents/MacOS/Celesta-mcp' : 'Celesta-mcp.exe');
const directory = mkdtempSync(join(tmpdir(), 'Celesta package 日本語 '));
const entry = join(directory, 'test.tsx');
const env = { ...process.env, PATH: mac ? '/usr/bin:/bin' : join(process.env.SystemRoot, 'System32') };
// Plugin folders searched before the bundled one would let another media
// plugin write the MOV below: CELESTA_PLUGIN_PATH, and the user's plugin folder and
// settings (which can add folders), found from these. Windows names
// variables in any case.
const userFolders = ['APPDATA', 'HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME'];
for (const name of Object.keys(env)) {
  const upper = name.toUpperCase();
  if (name.startsWith('DYLD_') || upper === 'CELESTA_PLUGIN_PATH' || userFolders.includes(upper)) {
    delete env[name];
  }
}
for (const name of userFolders) env[name] = directory;
delete env.NODE_PATH;
delete env.NODE_OPTIONS;
delete env.ESBUILD_BINARY_PATH;

function run(executable, args, input) {
  const result = spawnSync(executable, args, {
    cwd: directory, env, input, encoding: 'utf8', timeout: 120_000, windowsHide: true,
  });
  assert.equal(result.status, 0, `${result.error ?? ''}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

function assertNoLinks(directory) {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    const stat = lstatSync(path);
    assert.ok(!stat.isSymbolicLink(), `Package depends on a link: ${path}`);
    if (stat.isDirectory()) assertNoLinks(path);
  }
}

try {
  assertNoLinks(runtime);
  writeFileSync(entry, `
import { useState } from 'react';
import { Composition, Text, useCurrentFrame } from '@celesta.studio/react';
function Content() {
  const [text] = useState('Celesta');
  const frame = useCurrentFrame();
  return <Text x={0} y={0}>{text + frame}</Text>;
}
export default function Root() {
  return <Composition width={64} height={64} fps={1} durationInFrames={1}><Content /></Composition>;
}
`);
  const protocol = run(node, [join(runtime, 'react/dist/cli.js'), entry],
    '{"time":{"value":0,"timescale":1}}\n');
  const messages = protocol.trim().split(/\r?\n/).map((line) => JSON.parse(line));
  assert.ok(messages[0].config);
  assert.ok(messages[1].scene);
  // Each build is a separate process with the license files its source declares.
  for (const id of ['ffmpeg']) {
    const codec = join(packageDirectory, mac ? `Contents/Resources/PlugIns/${id}` : `plugins/${id}`);
    const manifest = JSON.parse(readFileSync(join(codec, 'celesta-plugin.json'), 'utf8'));
    assert.deepEqual(manifest.roles, ['media']);
    const program = join(codec, Object.values(manifest.executable)[0]);
    run(program, ['--check-license']);
    run(program, ['--check-build']);
    for (const kind of ['decoders', 'videoEncoders', 'audioEncoders', 'muxers']) {
      assert.ok(manifest.media?.[kind]?.length > 0, `The standard plugin must offer ${kind}`);
    }
    const programs = Object.values(manifest.executable ?? {});
    assert.equal(programs.length, 1);
    assert.ok(existsSync(join(codec, programs[0])));
    const source = JSON.parse(readFileSync(new URL(`../plugins/${id}/plugin.json`, import.meta.url), 'utf8'));
    assert.deepEqual(readdirSync(join(codec, 'licenses')).sort(),
      source.licenses.map((license) => basename(license)).sort());
  }
  // The media plugin's crates are in the plugins workspace, whose licenses
  // the package collects with the root's: the media plugin's alone, not
  // those of the plugins it does not bundle.
  const licenses = join(packageDirectory, mac ? 'Contents/Resources/licenses' : 'licenses');
  const rustPackages = JSON.parse(readFileSync(join(licenses, 'rust-packages.json'), 'utf8').replace(/^\u{FEFF}/u, ''))
    .map((crate) => crate.name);
  for (const crate of ['ez-ffmpeg', 'celesta-codec-ffmpeg']) {
    assert.ok(rustPackages.includes(crate), `licenses/rust-packages.json must name ${crate}`);
  }
  for (const id of ['ffmpeg-x264', 'ffmpeg-opus', 'ffmpeg-vpx']) {
    assert.ok(!existsSync(join(packageDirectory, mac ? `Contents/Resources/PlugIns/${id}` : `plugins/${id}`)), `${id} must be a separate download`);
    assert.ok(!rustPackages.includes(`celesta-codec-${id}`), `${id} must not enter the app's Rust inventory`);
  }
  assert.ok(!rustPackages.includes('celesta-plugin-vrm'), 'licenses/rust-packages.json must not name the unbundled VRM plugin');
  const output = join(directory, 'test.mov');
  run(exporter, ['--react', entry, output]);
  const mov = readFileSync(output);
  assert.ok(mov.includes(Buffer.from('moov')));
  assert.ok(mov.length > 100);
  if (mac) {
    // The AVFoundation plugin links system frameworks only, so it ships no
    // Frameworks/ and carries MIT/Apache-2.0 alone.
    const avfoundation = join(packageDirectory, 'Contents/Resources/PlugIns/avfoundation');
    assert.ok(existsSync(join(avfoundation, 'celesta-plugin.json')), 'Package must include the AVFoundation plugin');
    const avManifest = JSON.parse(readFileSync(join(avfoundation, 'celesta-plugin.json'), 'utf8'));
    assert.deepEqual(avManifest.roles, ['media']);
    const ids = (kind) => (avManifest.media?.[kind] ?? []).map((component) => component.id);
    assert.deepEqual(ids('decoders'), ['avfoundation']);
    assert.deepEqual(ids('videoEncoders'), ['h264', 'hevc', 'prores']);
    assert.deepEqual(ids('audioEncoders'), ['aac']);
    assert.deepEqual(ids('muxers'), ['mp4', 'mov']);
    const avPrograms = Object.entries(avManifest.executable ?? {});
    assert.equal(avPrograms.length, 1, 'The AVFoundation plugin must name exactly one executable');
    assert.ok(avPrograms[0][0].startsWith('macos-'), `The AVFoundation plugin's executable ${avPrograms[0][0]} must be a macos-* one`);
    assert.ok(existsSync(join(avfoundation, avPrograms[0][1])), `The AVFoundation plugin's program ${avPrograms[0][1]} must exist`);
    const avSource = JSON.parse(readFileSync(new URL('../plugins/avfoundation/plugin.json', import.meta.url), 'utf8'));
    assert.deepEqual(
      readdirSync(join(avfoundation, 'licenses')).sort(),
      avSource.licenses.map((license) => basename(license)).sort(),
      "The AVFoundation plugin's licenses/ must hold exactly the files plugins/avfoundation/plugin.json lists",
    );
    assert.deepEqual(readdirSync(join(avfoundation, 'licenses')).sort(), ['LICENSE-APACHE', 'LICENSE-MIT']);
    for (const crate of ['celesta-codec-avfoundation', 'objc2-video-toolbox']) {
      assert.ok(rustPackages.includes(crate), `licenses/rust-packages.json must name ${crate}`);
    }
    const output2 = join(directory, 'test-avfoundation.mp4');
    run(exporter, ['--react', entry, '--muxer', 'avfoundation/mp4', output2]);
    const avMp4 = readFileSync(output2);
    assert.equal(avMp4.toString('ascii', 4, 8), 'ftyp');
    assert.ok(avMp4.length > 100);
  }
  if (!mac) {
    // The Media Foundation plugin links Windows system libraries only, so it
    // carries MIT/Apache-2.0 alone. It has no muxer: its encoders write into
    // the FFmpeg plugin's MP4.
    const native = join(packageDirectory, 'plugins/mediafoundation');
    assert.ok(existsSync(join(native, 'celesta-plugin.json')), 'Package must include the Media Foundation plugin');
    const nativeManifest = JSON.parse(readFileSync(join(native, 'celesta-plugin.json'), 'utf8'));
    assert.deepEqual(nativeManifest.roles, ['media']);
    const nativeIds = (kind) => (nativeManifest.media?.[kind] ?? []).map((component) => component.id);
    assert.deepEqual(nativeIds('decoders'), ['mediafoundation']);
    assert.deepEqual(nativeIds('videoEncoders'), ['h264', 'hevc']);
    assert.deepEqual(nativeIds('audioEncoders'), ['aac']);
    assert.deepEqual(nativeIds('muxers'), []);
    const nativePrograms = Object.entries(nativeManifest.executable ?? {});
    assert.equal(nativePrograms.length, 1, 'The Media Foundation plugin must name exactly one executable');
    assert.ok(nativePrograms[0][0].startsWith('windows-'), `The Media Foundation plugin's executable ${nativePrograms[0][0]} must be a windows-* one`);
    assert.ok(existsSync(join(native, nativePrograms[0][1])), `The Media Foundation plugin's program ${nativePrograms[0][1]} must exist`);
    const nativeSource = JSON.parse(readFileSync(new URL('../plugins/mediafoundation/plugin.json', import.meta.url), 'utf8'));
    assert.deepEqual(
      readdirSync(join(native, 'licenses')).sort(),
      nativeSource.licenses.map((license) => basename(license)).sort(),
      "The Media Foundation plugin's licenses/ must hold exactly the files plugins/mediafoundation/plugin.json lists",
    );
    assert.deepEqual(readdirSync(join(native, 'licenses')).sort(), ['LICENSE-APACHE', 'LICENSE-MIT']);
    for (const crate of ['celesta-codec-mediafoundation', 'windows']) {
      assert.ok(rustPackages.includes(crate), `licenses/rust-packages.json must name ${crate}`);
    }
    // The encoder is the machine's: a Windows Server without the Media
    // Foundation feature, or a runner without a usable encoder, has none.
    const listing = JSON.parse(run(exporter, ['--list-components', '--check', '--json']));
    const h264 = listing.components.find((component) => component.id === 'mediafoundation/h264');
    assert.ok(h264, 'The exporter must list mediafoundation/h264');
    if (h264.available) {
      const output3 = join(directory, 'test-mediafoundation.mp4');
      run(exporter, ['--react', entry, '--video-encoder', 'mediafoundation/h264', output3]);
      const nativeMp4 = readFileSync(output3);
      assert.equal(nativeMp4.toString('ascii', 4, 8), 'ftyp');
      assert.ok(nativeMp4.length > 100);
    } else {
      console.log(`skipping the Media Foundation export: ${h264.reason ?? 'mediafoundation/h264 is unavailable'}`);
    }
  }
  assert.ok(!existsSync(join(runtime, 'react/.tmp')), 'Runtime must not write into the install directory');
  assert.ok(
    existsSync(join(runtime, 'react/dist/project-types/version.json')),
    'Package must include the TypeScript support template',
  );
  assert.ok(
    existsSync(join(runtime, 'react/dist/project-types/node_modules/@celesta.studio/code/dist/index.d.ts')),
    'Set Up TypeScript must include Code declarations',
  );
  assert.ok(
    existsSync(join(runtime, 'react/dist/project-types/node_modules/@celesta.studio/voicevox/dist/index.d.ts')),
    'Set Up TypeScript must include VOICEVOX declarations',
  );
  assert.ok(
    existsSync(join(runtime, 'react/dist/project-types/node_modules/@celesta.studio/aivis-speech/dist/index.d.ts')),
    'Set Up TypeScript must include AivisSpeech declarations',
  );
  assert.ok(
    existsSync(join(runtime, 'react/dist/project-types/node_modules/@celesta.studio/character/dist/index.d.ts')),
    'Set Up TypeScript must include the split package declarations',
  );
  writeFileSync(entry, `
import { Composition } from '@celesta.studio/react';
import { Code, codeCharacterCount } from '@celesta.studio/code';
import { Center } from '@celesta.studio/layout';
import { Circle } from '@celesta.studio/shapes';
import { lipSyncFromVoicevox } from '@celesta.studio/voicevox';
import { lipSyncFromAivisSpeech } from '@celesta.studio/aivis-speech';
const track = lipSyncFromVoicevox({ accent_phrases: [{ moras: [{ vowel: 'a', vowel_length: 0.2 }] }] });
if (track.mouthAtSeconds(0.1) !== 'a') throw new Error('VOICEVOX lip sync failed');
const aivis = lipSyncFromAivisSpeech({ accent_phrases: [{ moras: [{ text: 'ア', vowel: 'a' }] }] },
  { sampleRate: 8000, samples: Float32Array.from({ length: 1600 }, (_, i) => 0.3 * Math.sin(i / 4)) });
if (aivis.mouthAtSeconds(0.15) !== 'a') throw new Error('AivisSpeech lip sync failed');
const source = '{ "id": "broll" }';
export default function Root() {
  return <Composition width={320} height={120} fps={1} durationInFrames={1}>
    <Code language="json" highlightLines={[1]} highlightWidth={320}
      visibleCharacters={codeCharacterCount(source, { line: 1, column: 18 })}>{source}</Code>
    <Center><Circle radius={8} fill="#ffffff" /></Center>
  </Composition>;
}
`);
  const codeOutput = join(directory, 'code.png');
  run(exporter, ['--react', entry, '--frame', '0', codeOutput]);
  assert.deepEqual([...readFileSync(codeOutput).subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  const mcpReplies = run(mcp, [], [
    { jsonrpc: '2.0', id: 1, method: 'initialize', params: {
      protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'test-package', version: '0' } } },
    { jsonrpc: '2.0', method: 'notifications/initialized' },
    { jsonrpc: '2.0', id: 2, method: 'tools/list' },
  ].map((message) => JSON.stringify(message)).join('\n') + '\n')
    .trim().split(/\r?\n/).map((line) => JSON.parse(line));
  const tools = mcpReplies.find((reply) => reply.id === 2).result.tools.map((tool) => tool.name).sort();
  assert.deepEqual(tools, [
    'contact_sheet', 'describe_component', 'export_video', 'init_project', 'inspect',
    'list_components', 'list_templates', 'read_docs', 'render_frames',
  ]);
  console.log('Package smoke test passed: bundled React hooks, Celesta packages, Code, VOICEVOX, AivisSpeech, TypeScript support, export, and the MCP server, without Node.js on PATH.');
} finally {
  assert.ok(directory.startsWith(join(tmpdir(), 'Celesta package 日本語 ')));
  rmSync(directory, { recursive: true, force: true });
}
