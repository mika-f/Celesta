// Project property values from outside the source (--props, --props-file and a
// companion project) reach the CLI on the first stdin line when it runs with
// --properties-stdin. They are checked against defineProjectProperties() before
// prepare() and are readable from prepare() and from rendering. Integration
// tests of the build: they run the CLI, which loads @celesta.studio/project from its
// dist/. Run after `pnpm run build:runtime` (repository root).

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, test } from 'vitest';

import type { CompositionConfig, JsonValue, LayerContent, Scene } from '@celesta.studio/react';

const cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));
const FRAME = '{"time":{"value":0,"timescale":1}}';

/** One source of values on the first stdin line, as packages/cli reads it. */
interface PropertyInputLayer {
  source: 'project' | 'propsFile' | 'entryProps' | 'props';
  file?: string;
  baseDir: string;
  values: Record<string, JsonValue>;
}

/** One rejected value, as packages/cli reports it. */
interface PropertyIssue {
  key: string;
  source: string;
  message: string;
}

/** A line the CLI prints; each holds one of these. */
interface Message {
  config?: CompositionConfig;
  scene?: Scene;
  invalidProperties?: PropertyIssue[];
  error?: string;
}

type Messages = Message[] & { status: number | null; stderr: string };

type TextContent = Extract<LayerContent, { type: 'text' }>;

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'celesta-props-'));
});
afterEach(() => {
  rmSync(dir, { recursive: true, force: true });
});

const SCHEMA_ENTRY =
  `import * as fs from 'node:fs';\n` +
  `import { Composition, Text, defineProjectProperties, getProjectProperty } from '@celesta.studio/react'; import { ProjectProvider, loadProjectFromString, useProjectProperty } from '@celesta.studio/project';\n` +
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

function run(entrySource: string, layers?: PropertyInputLayer[]): Messages {
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, entrySource);
  const args = layers ? [cli, entry, '--properties-stdin'] : [cli, entry];
  const input = layers ? `${JSON.stringify({ propertyInputs: layers })}\n${FRAME}\n` : `${FRAME}\n`;
  const result = spawnSync(process.execPath, args, { input, encoding: 'utf8' });
  const messages = result.stdout.split(/\r?\n/).filter(Boolean).map((line): Message => JSON.parse(line));
  return Object.assign(messages, { status: result.status, stderr: result.stderr });
}

function textOf(messages: Messages): TextContent {
  const scene = messages.find((message) => message.scene)?.scene;
  assert.ok(scene, JSON.stringify(messages));
  const { content } = scene.layers[0];
  assert.equal(content.type, 'text');
  return content as TextContent;
}

/** The color of a text's solid fill. */
function fillOf(content: TextContent): string {
  const { fill } = content.style;
  assert.ok(fill?.type === 'solid', 'a solid fill');
  return fill.color;
}

/** The issues `message` reports, checked to be there. */
function issuesOf(message: Message | undefined): PropertyIssue[] {
  assert.ok(message?.invalidProperties, JSON.stringify(message));
  return message.invalidProperties;
}

test('without inputs, the Provider beats the schema defaults and path defaults resolve against the entry', () => {
  writeFileSync(join(dir, 'default.txt'), 'from-default\n');
  const messages = run(SCHEMA_ENTRY);
  assert.equal(messages[0].config?.durationInFrames, 20);
  const content = textOf(messages);
  assert.equal(content.text, 'Provider|b|from-default');
  assert.equal(fillOf(content), '#ff8800');
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
  assert.equal(messages[0].config?.durationInFrames, 30);
  const content = textOf(messages);
  assert.equal(content.text, 'Inline|b|row-1|row-2');
  assert.equal(fillOf(content), '#112233');
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
    { source: 'props', baseDir: dir, values: { count: true, data: '.' } },
  ]);
  assert.equal(messages.status, 1);
  assert.equal(messages.length, 1, JSON.stringify(messages));
  const issues = issuesOf(messages[0]);
  assert.deepEqual(
    issues.map(({ key, source }) => `${source} ${key}`),
    [
      '--project p.celesta.json count',
      '--props-file a.json titel',
      '--props-file a.json accent',
      '--props-file a.json mode',
      '--props-file a.json data',
      '--props count',
      '--props data',
    ],
  );
  assert.match(issues[1].message, /declared: title, accent, count, mode, data/);
  assert.match(issues[3].message, /one of "a", "b"/);
  assert.match(issues[4].message, /missing\.txt is not an existing file/);
  // A directory is not a file.
  assert.match(issues[6].message, /is not an existing file/);
});

test('outside values need a schema, but a companion project without one stays compatible', () => {
  const entry =
    `import { Composition, Text } from '@celesta.studio/react';\n` +
    `export default function Root() {\n` +
    `  return <Composition width={64} height={64} fps={30} durationInFrames={1}><Text x={0} y={0}>ok</Text></Composition>;\n` +
    `}\n`;
  const compatible = run(entry, [{ source: 'project', file: 'p.celesta.json', baseDir: dir, values: { title: 'x' } }]);
  assert.equal(textOf(compatible).text, 'ok');

  const rejectedRun = run(entry, [{ source: 'props', baseDir: dir, values: { title: 'x' } }]);
  assert.equal(rejectedRun.status, 1);
  const [rejected] = rejectedRun;
  assert.equal(issuesOf(rejected)[0].key, '');
  assert.match(issuesOf(rejected)[0].message, /defineProjectProperties/);
});

test('reading a property at module scope is an error', () => {
  const entry =
    `import { Composition, defineProjectProperties, getProjectProperty } from '@celesta.studio/react';\n` +
    `defineProjectProperties({ title: { type: 'string', defaultValue: 'x' } });\n` +
    `const title = getProjectProperty('title');\n` +
    `export default function Root() {\n` +
    `  return <Composition width={64} height={64} fps={30} durationInFrames={1}>{null}</Composition>;\n` +
    `}\n`;
  const result = run(entry, []);
  assert.equal(result.status, 1);
  assert.match(result[0].error ?? '', /read at module scope/);
});

test('an empty path turns a file off, and __proto__ is an ordinary key', () => {
  const entry =
    `import { Composition, Text, defineProjectProperties, getProjectProperty } from '@celesta.studio/react';\n` +
    `defineProjectProperties({ data: { type: 'path', defaultValue: './default.txt' }, ['__proto__']: { type: 'string', defaultValue: 'd' } });\n` +
    `export default function Root() {\n` +
    `  return <Composition width={64} height={64} fps={30} durationInFrames={1}><Text x={0} y={0}>{JSON.stringify([getProjectProperty('data'), getProjectProperty('__proto__')])}</Text></Composition>;\n` +
    `}\n`;
  const messages = run(entry, [{ source: 'props', baseDir: dir, values: JSON.parse('{"data":"","__proto__":"set"}') }]);
  assert.equal(textOf(messages).text, '["","set"]');
});

test('a malformed handshake is reported as an error line', () => {
  // Deliberately not a values object.
  const messages = run(SCHEMA_ENTRY, [{ source: 'props', baseDir: dir, values: null as unknown as Record<string, JsonValue> }]);
  assert.equal(messages.status, 1);
  assert.match(messages[0].error ?? '', /invalid project property inputs/);
});

test('entry props.json is strict when the entry declares properties', () => {
  const file = join(dir, 'props.json');
  const messages = run(SCHEMA_ENTRY, [
    { source: 'entryProps', file, baseDir: dir, values: { titel: 'x', accent: 'red' } },
  ]);
  assert.equal(messages.status, 1);
  const issues = issuesOf(messages[0]);
  assert.equal(issues.length, 2, JSON.stringify(messages));
  assert.deepEqual(
    issues.map(({ key, source }) => [key, source]),
    [
      ['titel', `props.json ${file}`],
      ['accent', `props.json ${file}`],
    ],
  );
});

test('entry props.json is ignored with a warning when the entry declares nothing', () => {
  const entry =
    `import { Composition, Text } from '@celesta.studio/react';\n` +
    `export default function Root() {\n` +
    `  return <Composition width={64} height={64} fps={30} durationInFrames={1}><Text x={0} y={0}>ok</Text></Composition>;\n` +
    `}\n`;
  const file = join(dir, 'props.json');
  const messages = run(entry, [{ source: 'entryProps', file, baseDir: dir, values: { title: 'x' } }]);
  assert.equal(messages.status, 0, messages.stderr);
  assert.ok(messages[0].config, JSON.stringify(messages));
  assert.equal(textOf(messages).text, 'ok');
  assert.match(messages.stderr, /ignoring .*props\.json.*declares no project properties/);
});

test('entry props.json values reach prepare() and rendering, and paths resolve from its folder', () => {
  writeFileSync(join(dir, 'auto.txt'), 'auto-line\n');
  const messages = run(SCHEMA_ENTRY, [
    { source: 'entryProps', file: join(dir, 'props.json'), baseDir: dir, values: { title: 'Auto', data: './auto.txt' } },
  ]);
  assert.equal(messages.status, 0, messages.stderr);
  assert.equal(textOf(messages).text, 'Auto|b|auto-line');
});

test('entry props.json naming a missing file is reported by key', () => {
  const file = join(dir, 'props.json');
  const messages = run(SCHEMA_ENTRY, [{ source: 'entryProps', file, baseDir: dir, values: { data: './gone.txt' } }]);
  assert.equal(messages.status, 1);
  const issues = issuesOf(messages[0]);
  assert.equal(issues.length, 1, JSON.stringify(messages));
  assert.equal(issues[0].key, 'data');
  assert.match(issues[0].message, /gone\.txt is not an existing file/);
});
