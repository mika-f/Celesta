// Project property values from outside the source (--props, --props-file and
// a companion project) reach the CLI on the first stdin line when it runs
// with --properties-stdin. They are checked against defineProjectProperties()
// before prepare() and are readable from prepare() and from rendering.
// Run after `pnpm run build`.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, test } from 'vitest';

const cli = fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url));
const FRAME = '{"time":{"value":0,"timescale":1}}';

let dir;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'celesta-props-'));
});
afterEach(() => {
  rmSync(dir, { recursive: true, force: true });
});

const SCHEMA_ENTRY =
  `import * as fs from 'node:fs';\n` +
  `import { Composition, ProjectProvider, Text, defineProjectProperties, getProjectProperty, loadProjectFromString, useProjectProperty } from '@celesta/react';\n` +
  `defineProjectProperties({\n` +
  `  title: { type: 'string', defaultValue: 'Default' },\n` +
  `  accent: { type: 'color', defaultValue: '#ff8800' },\n` +
  `  count: { type: 'number', defaultValue: 2 },\n` +
  `  mode: { type: 'select', defaultValue: 'a', options: ['a', 'b'] },\n` +
  `  data: { type: 'path', defaultValue: './default.txt' },\n` +
  `});\n` +
  `let lines: string[] = [];\n` +
  `export async function prepare() {\n` +
  `  lines = fs.readFileSync(getProjectProperty<string>('data'), 'utf8').trim().split('\\n');\n` +
  `}\n` +
  `function Body() {\n` +
  `  const title = useProjectProperty<string>('title');\n` +
  `  const accent = useProjectProperty<string>('accent');\n` +
  `  const mode = useProjectProperty<string>('mode');\n` +
  `  return <Text x={0} y={0} style={{ fill: { type: 'solid', color: accent } }}>{[title, mode, ...lines].join('|')}</Text>;\n` +
  `}\n` +
  `const project = loadProjectFromString(JSON.stringify({ version: 0, settings: { width: 64, height: 64, frameRate: { numerator: 30, denominator: 1 }, sampleRate: 48000 }, assets: {}, characters: {}, tracks: [], properties: { mode: 'b', title: 'Provider' } }));\n` +
  `export default function Root() {\n` +
  `  return (\n` +
  `    <Composition width={64} height={64} fps={30} durationInFrames={getProjectProperty<number>('count') * 10}>\n` +
  `      <ProjectProvider project={project}><Body /></ProjectProvider>\n` +
  `    </Composition>\n` +
  `  );\n` +
  `}\n`;

function run(entrySource, layers) {
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, entrySource);
  const args = layers ? [cli, entry, '--properties-stdin'] : [cli, entry];
  const input = layers ? `${JSON.stringify({ propertyInputs: layers })}\n${FRAME}\n` : `${FRAME}\n`;
  const result = spawnSync(process.execPath, args, { input, encoding: 'utf8' });
  return result.stdout.split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line));
}

function textOf(messages) {
  const scene = messages.find((message) => message.scene)?.scene;
  assert.ok(scene, JSON.stringify(messages));
  return scene.layers[0].content;
}

test('without inputs, the Provider beats the schema defaults and path defaults resolve against the entry', () => {
  writeFileSync(join(dir, 'default.txt'), 'from-default\n');
  const messages = run(SCHEMA_ENTRY);
  assert.equal(messages[0].config.durationInFrames, 20);
  const content = textOf(messages);
  assert.equal(content.text, 'Provider|b|from-default');
  assert.equal(content.style.fill.color, '#ff8800');
});

test('later layers win over earlier ones and over the Provider, in prepare() and rendering', () => {
  mkdirSync(join(dir, 'variant'));
  writeFileSync(join(dir, 'variant', 'rows.txt'), 'row-1\nrow-2\n');
  const messages = run(SCHEMA_ENTRY, [
    { source: 'project', file: 'p.celesta.json', baseDir: dir, values: { title: 'Project', unrelated: 1 } },
    {
      source: 'propsFile',
      file: 'variant/a.json',
      baseDir: join(dir, 'variant'),
      values: { title: 'File', count: 3, data: 'rows.txt', accent: '#112233' },
    },
    { source: 'props', baseDir: dir, values: { title: 'Inline' } },
  ]);
  assert.equal(messages[0].config.durationInFrames, 30);
  const content = textOf(messages);
  assert.equal(content.text, 'Inline|b|row-1|row-2');
  assert.equal(content.style.fill.color, '#112233');
});

test('every invalid value is reported before prepare() runs', () => {
  const messages = run(SCHEMA_ENTRY, [
    { source: 'project', file: 'p.celesta.json', baseDir: dir, values: { count: 'many' } },
    {
      source: 'propsFile',
      file: 'a.json',
      baseDir: dir,
      values: { titel: 'typo', accent: 'red', mode: 'c', data: 'missing.txt' },
    },
    { source: 'props', baseDir: dir, values: { count: true } },
  ]);
  assert.equal(messages.length, 1, JSON.stringify(messages));
  const issues = messages[0].invalidProperties;
  assert.deepEqual(
    issues.map(({ key, source }) => `${source} ${key}`),
    [
      '--project p.celesta.json count',
      '--props-file a.json titel',
      '--props-file a.json accent',
      '--props-file a.json mode',
      '--props-file a.json data',
      '--props count',
    ],
  );
  assert.match(issues[1].message, /declared: title, accent, count, mode, data/);
  assert.match(issues[3].message, /one of "a", "b"/);
  assert.match(issues[4].message, /missing\.txt does not exist/);
});

test('outside values need a schema, but a companion project without one stays compatible', () => {
  const entry =
    `import { Composition, Text } from '@celesta/react';\n` +
    `export default function Root() {\n` +
    `  return <Composition width={64} height={64} fps={30} durationInFrames={1}><Text x={0} y={0}>ok</Text></Composition>;\n` +
    `}\n`;
  const compatible = run(entry, [{ source: 'project', file: 'p.celesta.json', baseDir: dir, values: { title: 'x' } }]);
  assert.equal(textOf(compatible).text, 'ok');

  const [rejected] = run(entry, [{ source: 'props', baseDir: dir, values: { title: 'x' } }]);
  assert.equal(rejected.invalidProperties[0].key, '');
  assert.match(rejected.invalidProperties[0].message, /defineProjectProperties/);
});

test('reading a property at module scope is an error', () => {
  const entry =
    `import { Composition, defineProjectProperties, getProjectProperty } from '@celesta/react';\n` +
    `defineProjectProperties({ title: { type: 'string', defaultValue: 'x' } });\n` +
    `const title = getProjectProperty('title');\n` +
    `export default function Root() {\n` +
    `  return <Composition width={64} height={64} fps={30} durationInFrames={1}>{null}</Composition>;\n` +
    `}\n`;
  const [message] = run(entry, []);
  assert.match(message.error, /read at module scope/);
});
