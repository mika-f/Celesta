import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import React from 'react';
import { onTestFinished, test } from 'vitest';
import { Composition, Font, Group, Rect, Span, Text, useCurrentFrame, useTextMetrics } from '@celesta/react';
import type { GlyphMetrics, Layer, LayerContent, Paint, Scene, TextMetrics } from '@celesta/react';
import { mount, setTextMeasurer } from '@celesta/react/internal';
import type { MeasureTextRequest, MountedComposition } from '@celesta/react/internal';
import { Code, codeCharacterCount, codeLines, codeThemes, highlightCode, tokenizeCode, useCodePoint } from '../src/index.js';
import type { CodeLanguage, HighlightedCodeLine } from '../src/index.js';

type ContentOf<T extends LayerContent['type']> = Extract<LayerContent, { type: T }>;
/** A leaf layer with its position summed through its groups. */
type Placed<C extends LayerContent = LayerContent> = Omit<Layer, 'content'> & { content: C; x: number; y: number };

const isText = (layer: Placed): layer is Placed<ContentOf<'text'>> => layer.content.type === 'text';
const isRect = (layer: Placed): layer is Placed<ContentOf<'rect'>> => layer.content.type === 'rect';

function contentOf<T extends LayerContent['type']>(layer: { content: LayerContent } | null | undefined, type: T): ContentOf<T> {
  assert.equal(layer?.content.type, type);
  return layer!.content as ContentOf<T>;
}
const textOf = (layer: { content: LayerContent } | null | undefined) => contentOf(layer, 'text');
const rectOf = (layer: { content: LayerContent } | null | undefined) => contentOf(layer, 'rect');
const groupOf = (layer: { content: LayerContent } | null | undefined) => contentOf(layer, 'group');

/** The color of a solid paint. */
function colorOf(paint: Paint | null | undefined): string {
  assert.ok(paint && paint.type === 'solid', 'a solid paint');
  return paint.color;
}

// capHeight, xHeight and the glyphs' ink are 0, as for a font that does not give them.
function metrics(request: MeasureTextRequest): TextMetrics {
  const lines = request.text.split('\n');
  const lineHeight = request.style.lineHeight ?? 36;
  return {
    width: Math.max(...lines.map(line => Array.from(line).length)) * 10,
    height: lines.length * lineHeight, ascent: lineHeight * 0.75, descent: lineHeight * 0.25,
    lineHeight, capHeight: 0, xHeight: 0, lines: lines.length, glyphs: lines.flatMap((line, row) => {
      const start = lines.slice(0, row).reduce((n, text) => n + Array.from(text).length + 1, 0);
      return Array.from(line).map((text, column) => ({ text, start: start + column, end: start + column + 1,
        rtl: false, x: column * 10, width: 10, line: row, inkAscent: 0, inkDescent: 0 }));
    }),
  };
}

function layers(scene: Scene): Placed[] {
  const result: Placed[] = [];
  function walk(layer: Layer, x = 0, y = 0) {
    x += layer.transform.position.x;
    y += layer.transform.position.y;
    if (layer.content.type === 'group') {
      layer.content.layers.forEach(child => walk(child, x, y));
    } else result.push({ ...layer, x, y });
  }
  scene.layers.forEach(layer => walk(layer));
  return result;
}

// Blank runs deliberately have no Text layer: reconstruct their spacing from
// the measured positions, so a missing space in the layout fails this check.
function renderedText(scene: Scene, left = 0): string {
  const rows = new Map<number, Placed<ContentOf<'text'>>[]>();
  for (const layer of layers(scene).filter(isText)) {
    const row = rows.get(layer.y) ?? [];
    row.push(layer);
    rows.set(layer.y, row);
  }
  return [...rows].sort(([a], [b]) => a - b).map(([, row]) => {
    let text = '';
    for (const layer of row.sort((a, b) => a.x - b.x)) {
      text += ' '.repeat((layer.x - left) / 10 - Array.from(text).length) + Array.from(layer.content.text).slice(0, layer.content.style.visibleCharacters ?? Infinity).join('');
    }
    return text;
  }).join('\n');
}

const frameAt = (mounted: MountedComposition, frame: number): Scene => mounted.renderAt({ value: frame, timescale: 30 }, null).scene;

test('all supported grammars preserve whitespace and UTF-16 source offsets', () => {
  const samples = {
    tsx: '<Text label="😀あ">{value}</Text>\r\n',
    ts: 'const value: string = "😀あ";\n/* first\n second */\n',
    json: '{\n\t"value": "😀あ", "count": 42\n}\n',
    bash: '# comment\n\techo "😀あ" a-b.c --x-y=1 file#1 -\n',
    text: '  😀あ\t é\r\n\n',
  };
  for (const [language, source] of Object.entries(samples) as [CodeLanguage, string][]) {
    const tokens = tokenizeCode(source, language);
    assert.equal(tokens.map(token => token.text).join(''), source, language);
    let offset = 0;
    for (const token of tokens) {
      assert.equal(token.start, offset);
      assert.equal(token.text, source.slice(token.start, token.end));
      assert.ok(token.end > token.start);
      offset = token.end;
    }
    assert.equal(offset, source.length);
    if (language !== 'text') assert.ok(tokens.some(token => token.type === 'string'), language);
  }
  assert.deepEqual(tokenizeCode('', 'tsx'), []);
  // Wrong on purpose: the check is for an untyped language name.
  assert.throws(() => tokenizeCode('hello', 'ruby' as CodeLanguage), /Unsupported code language/);
});

test('bash keeps hyphenated words whole and recognizes builtins only as whole words', () => {
  const tokens = (source: string) => tokenizeCode(source, 'bash').filter(token => token.type !== 'plain').map(token => [token.text, token.type]);
  assert.deepEqual(tokens('Celesta-export --react film.tsx out.mp4 \\\n  --bit-depth 10'), [
    ['Celesta-export', 'identifier'], ['--react', 'parameter'], ['film.tsx', 'identifier'], ['out.mp4', 'identifier'],
    ['\\\n', 'operator'], ['--bit-depth', 'parameter'], ['10', 'identifier'],
  ]);
  assert.deepEqual(tokens('celesta-exporter --props=props.json'), [
    ['celesta-exporter', 'identifier'], ['--props', 'parameter'], ['=', 'operator'], ['props.json', 'identifier'],
  ]);
  assert.deepEqual(tokens('node scripts/migrate-packages.mjs'), [['node', 'identifier'], ['scripts/migrate-packages.mjs', 'identifier']]);
  // Keywords and builtins inside a word stay part of it.
  assert.deepEqual(tokens('ls ./test.sh in.mp4 /usr/local-bin -c:v'), [
    ['ls', 'identifier'], ['.', 'punctuation'], ['/', 'operator'], ['test.sh', 'identifier'], ['in.mp4', 'identifier'],
    ['/', 'operator'], ['usr/local-bin', 'identifier'], ['-c:v', 'parameter'],
  ]);
  assert.deepEqual(tokens('echo hi; export FOO=1; cd -'), [
    ['echo', 'builtin'], ['hi', 'identifier'], [';', 'punctuation'], ['export', 'builtin'], ['FOO', 'identifier'],
    ['=', 'operator'], ['1', 'identifier'], [';', 'punctuation'], ['cd', 'builtin'], ['-', 'parameter'],
  ]);
  assert.deepEqual(tokens('for f in a-b; do :; done'), [
    ['for', 'keyword'], ['f', 'identifier'], ['in', 'keyword'], ['a-b', 'identifier'], [';', 'punctuation'],
    ['do', 'keyword'], [':', 'builtin'], [';', 'punctuation'], ['done', 'keyword'],
  ]);
  assert.deepEqual(tokens('foo-bar() { true; }').slice(0, 2), [['foo-bar', 'function'], ['()', 'punctuation']]);
  // A `#` inside a word does not start a comment.
  assert.deepEqual(tokens('echo out#1 # note'), [['echo', 'builtin'], ['out#1', 'identifier'], ['# note', 'comment']]);
  assert.deepEqual(tokens('x=$(cmd -v a-b)'), [
    ['x', 'identifier'], ['=', 'operator'], ['$(', 'punctuation'], ['cmd', 'identifier'], ['-v', 'parameter'],
    ['a-b', 'identifier'], [')', 'punctuation'],
  ]);
  assert.deepEqual(tokens('[[ -f foo-bar.txt ]]'), [['[[', 'keyword'], ['-f', 'operator'], ['foo-bar.txt', 'identifier'], [']]', 'keyword']]);
  // In arithmetic and parameter expansion, `-` is still an operator.
  assert.deepEqual(tokens('$((a-b)) ${A:-x-y}'), [
    ['$((', 'punctuation'], ['a', 'identifier'], ['-', 'operator'], ['b', 'identifier'], ['))', 'punctuation'],
    ['${', 'punctuation'], ['A', 'identifier'], [':-', 'operator'], ['x', 'identifier'], ['-', 'operator'], ['y', 'identifier'], ['}', 'punctuation'],
  ]);
});

test('typing, seeking and line highlights reuse full-source measurements and keep emoji intact', () => {
  const source = 'const value = "😀あ";\r\n\t// comment\n';
  const style = { fontFamily: 'IBM Plex Mono', fontSize: 24, lineHeight: 36 };
  const requests: MeasureTextRequest[] = [];
  setTextMeasurer(async request => metrics(request), request => {
    requests.push(request);
    return metrics(request);
  });
  const count = Array.from(source.slice(0, source.indexOf('😀'))).length + 1;
  function Root() {
    const frame = useCurrentFrame();
    return React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: 4 },
      React.createElement(Font, { src: './mono.ttf' }),
      React.createElement(Code, {
        x: 40, y: 20, language: 'ts', style, children: source,
        visibleCharacters: frame === 0 ? 0 : frame === 1 ? count : Infinity,
        highlightLines: frame < 2 ? [1] : [2],
      }));
  }
  const mounted = mount(Root);
  const first = layers(frameAt(mounted, 0));
  assert.equal(first.filter(layer => layer.content.type === 'text').length, 0);
  assert.equal(rectOf(first[0]).width, Array.from(source.split('\r\n')[0]).length * 10);
  const measurements = requests.length;
  const typedScene = frameAt(mounted, 1);
  const typed = layers(typedScene);
  assert.equal(renderedText(typedScene, 40), 'const value = "😀');
  assert.equal(rectOf(typed[0]).width, rectOf(first[0]).width);
  const full = layers(frameAt(mounted, 2));
  assert.equal(full.find(isRect)!.y, 20 + 36);
  assert.ok(full.filter(isText).some(layer => layer.content.text === '  // comment' && layer.x === 40));
  assert.ok(full.filter(isText).some(layer => layer.content.text.includes('const') && layer.content.style.colorRuns!.some(run => run.color === codeThemes.dark.tokens.keyword)));
  assert.equal(full.find(isText)!.content.baselineAnchor, true);
  assert.ok(requests.at(-1)!.fonts.some(font => font.location.type === 'file' && font.location.path.endsWith('mono.ttf')));
  frameAt(mounted, 0);
  frameAt(mounted, 3);
  assert.equal(requests.length, measurements, 'unchanged geometry must not make per-frame measurement RPCs');
});

test('a font stack reaches every line and the measurement the highlight band uses', () => {
  const fontFamily = ['JetBrains Mono', 'Noto Sans JP'];
  const requests: MeasureTextRequest[] = [];
  setTextMeasurer(async request => metrics(request), request => {
    requests.push(request);
    // The stack's second family draws the Japanese twice as wide.
    const measured = metrics(request);
    const widths = request.text.split('\n').map(line => Array.from(line).reduce((width, character) => width + (/[぀-鿿]/u.test(character) ? 20 : 10), 0));
    return { ...measured, width: Math.max(...widths) };
  });
  const source = 'const a = 1;\n// 動画をコードで書ける\n';
  const Root = () => React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: 1 },
    React.createElement(Code, { language: 'ts', style: { fontFamily, fontSize: 24, lineHeight: 36 }, highlightLines: [2], children: source }));
  const scene = layers(frameAt(mount(Root), 0));
  const texts = scene.filter(isText);
  assert.equal(texts.length, 2);
  for (const text of texts) assert.deepEqual(text.content.style.fontFamily, fontFamily);
  assert.ok(requests.length > 0 && requests.every(request => JSON.stringify(request.style.fontFamily) === JSON.stringify(fontFamily)));
  assert.equal(scene.find(isRect)!.content.width, 3 * 10 + 10 * 20);
});

test('code-point positions use original columns, tab stops and CRLF lines', () => {
  setTextMeasurer(async request => metrics(request), metrics);
  const source = '😀\tX\r\n\tあ\n';
  function Caret() {
    const frame = useCurrentFrame();
    const position = frame === 0 ? { line: 1, column: 3 } : { line: 2, column: 3 };
    const point = useCodePoint(source, position, { fontSize: 24, lineHeight: 40 }, 4);
    return React.createElement(Rect, { x: point.x, y: point.baseline, width: 2, height: point.lineHeight });
  }
  const Root = () => React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: 2 }, React.createElement(Caret));
  const mounted = mount(Root);
  const [first] = layers(frameAt(mounted, 0));
  const [second] = layers(frameAt(mounted, 1));
  assert.deepEqual([first.x, first.y, rectOf(first).height], [40, 30, 40]);
  assert.deepEqual([second.x, second.y, rectOf(second).height], [50, 70, 40]);
});

test('highlightWidth spans a panel independently of text width and visibility', () => {
  const requests: MeasureTextRequest[] = [];
  setTextMeasurer(async request => metrics(request), request => {
    requests.push(request);
    return metrics(request);
  });
  function Root() {
    const frame = useCurrentFrame();
    return React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: 4 },
      React.createElement(Code, {
        x: 20, y: 30, children: 'abc\nx', highlightLines: [1, 2],
        highlightWidth: [undefined, 200, 0, 500][frame],
        visibleCharacters: frame === 1 ? 1 : Infinity,
      }));
  }
  const mounted = mount(Root);
  const first = layers(frameAt(mounted, 0));
  assert.deepEqual(first.filter(isRect).map(layer => layer.content.width), [30, 30]);
  const measurements = requests.length;
  for (const [frame, width] of [[1, 200], [2, 0], [3, 500]]) {
    const scene = frameAt(mounted, frame);
    const rects = layers(scene).filter(isRect);
    assert.deepEqual(rects.map(layer => [layer.x, layer.y, layer.content.width]), [[20, 30, width], [20, 66, width]]);
    assert.equal(renderedText(scene, 20), frame === 1 ? 'a' : 'abc\nx');
  }
  assert.equal(requests.length, measurements, 'band width changes must reuse text measurements');
});

test('JSON property keys include escaped fragments and differ from string values', () => {
  const keys = ['id', 'a"b\\c', 'nested', '', 'array'];
  const source = `{ "id": "broll: value", ${JSON.stringify(keys[1])} \r\n : ${JSON.stringify('value\ntext')}, "nested": { "": "😀" }, "array": ["item"] }`;
  const tokens = tokenizeCode(source, 'json');
  assert.equal(tokens.map(token => token.text).join(''), source);
  for (const key of keys) {
    const text = JSON.stringify(key);
    const start = source.indexOf(text);
    const fragments = tokens.filter(token => token.start >= start && token.end <= start + text.length);
    assert.equal(fragments.map(token => token.text).join(''), text);
    assert.ok(fragments.every(token => token.type === 'property'), text);
  }
  for (const value of ['"broll: value"', '"😀"', '"item"']) {
    assert.equal(tokens.find(token => token.text === value)!.type, 'string', value);
  }
  assert.ok(tokens.some(token => token.type === 'string_escape' && token.text === '\\n'));
  assert.ok(tokenizeCode('"incomplete', 'json').every(token => token.type !== 'property'));
  setTextMeasurer(async request => metrics(request), metrics);
  const Root = () => React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: 1 },
    React.createElement(Code, { language: 'json', children: source }));
  const textLayers = layers(frameAt(mount(Root), 0)).filter(isText);
  assert.ok(textLayers.some(layer => layer.content.style.colorRuns!.some(run => Array.from(layer.content.text).slice(run.start, run.end).join('') === JSON.stringify(keys[1]) && run.color === codeThemes.dark.tokens.property)));
  assert.ok(textLayers.some(layer => layer.content.style.colorRuns!.some(run => Array.from(layer.content.text).slice(run.start, run.end).join('') === '"broll: value"' && run.color === codeThemes.dark.tokens.string)));
});

test('codeCharacterCount converts source positions into visible code points', () => {
  const source = '😀\tX\r\n\tあ\nZ\r';
  const positions = [
    [1, 1, 0], [1, 2, 1], [1, 3, 2], [1, 4, 3],
    [2, 1, 5], [2, 3, 7], [3, 1, 8], [3, 2, 9], [4, 1, 10],
  ];
  for (const [line, column, expected] of positions) {
    assert.equal(codeCharacterCount(source, { line, column }), expected);
  }
  assert.equal(codeCharacterCount('', { line: 1, column: 1 }), 0);
  assert.equal(codeCharacterCount('é', { line: 1, column: 3 }), 2);
  setTextMeasurer(async request => metrics(request), metrics);
  const Root = () => React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: 1 },
    React.createElement(Code, {
      children: source, tabSize: 4,
      visibleCharacters: codeCharacterCount(source, { line: 2, column: 3 }),
    }));
  assert.equal(renderedText(frameAt(mount(Root), 0)), '😀   X\n    あ');
});

test('source and grammar changes refresh colors and geometry; unknown theme tokens use foreground', () => {
  setTextMeasurer(async request => metrics(request), metrics);
  const theme = { foreground: '#abcdef', highlightLine: '#112233', tokens: { keyword: '#ff0000' } };
  function Root() {
    const frame = useCurrentFrame();
    return React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: 3 },
      React.createElement(Code, {
        language: frame === 2 ? 'text' : 'ts', theme, highlightLines: [1],
        children: frame === 0 ? 'const x = 1;' : 'let longer = 2;',
      }));
  }
  const mounted = mount(Root);
  const first = layers(frameAt(mounted, 0));
  const second = layers(frameAt(mounted, 1));
  const plain = layers(frameAt(mounted, 2));
  assert.ok(first.filter(isText).some(layer => layer.content.style.colorRuns!.some(run => run.color === '#ff0000')));
  assert.ok(second.filter(isText).some(layer => layer.content.text.includes('let')));
  assert.ok(rectOf(second[0]).width > rectOf(first[0]).width);
  assert.equal(plain.filter(isText).length, 1);
  assert.equal(colorOf(textOf(plain[1]).style.fill), '#abcdef');
});

test('invalid layout inputs fail before rendering', () => {
  // Wrong on purpose: the checks are for data that comes in untyped.
  const nil = null as unknown as string;
  assert.throws(() => tokenizeCode(nil), /source must be a string/);
  for (const tabSize of [0, -1, 1.5, Infinity, NaN]) {
    assert.throws(() => Code({ children: '', tabSize }), /tabSize/);
  }
  for (const style of [{ fontSize: 0 }, { lineHeight: -1 }, { fontSize: Infinity }, { lineHeight: NaN }]) {
    assert.throws(() => Code({ children: '', style }), /positive fontSize and lineHeight/);
  }
  assert.throws(() => Code({ children: '', style: { align: 'center' } }), /left alignment/);
  assert.throws(() => Code({ children: '', visibleCharacters: NaN }), /visibleCharacters/);
  for (const highlightWidth of [-1, Infinity, NaN, '100']) {
    assert.throws(() => Code({ children: '', highlightWidth: highlightWidth as number }), /highlightWidth/);
  }
  for (const convert of [codeCharacterCount, useCodePoint]) {
    assert.throws(() => convert(nil, { line: 1, column: 1 }), /source must be a string/);
    for (const line of [0, -1, 2, 1.5, Infinity, NaN]) {
      assert.throws(() => convert('abc', { line, column: 1 }), /line is out of range/);
    }
    for (const column of [0, -1, 5, 1.5, Infinity, NaN]) {
      assert.throws(() => convert('abc', { line: 1, column }), /column is out of range/);
    }
  }
});

// What the bridge prints on stdout, one JSON message per line.
interface BridgeMessage {
  error?: string;
  measureText?: MeasureTextRequest;
  config?: unknown;
  scene?: Scene;
}

// Integration test of the build: spawns packages/cli/bin/celesta-react-render.js
// (and the staged runtime), which load @celesta/code's dist/, so it needs
// `pnpm run build:runtime` first.
for (const packaged of [false, true]) test(`${packaged ? 'packaged' : 'source'} CLI resolves Code without project dependencies and shares useTypewriter's runtime`, { timeout: 30000 }, async ({ signal }) => {
  const directory = mkdtempSync(join(tmpdir(), 'celesta-code-'));
  onTestFinished(() => rmSync(directory, { recursive: true, force: true }));
  const entry = join(directory, 'entry.tsx');
  writeFileSync(entry, `
    import { Composition, type TextColorRun } from '@celesta/react';
    import { useTypewriter } from '@celesta/text';
    import { Code, codeCharacterCount, codeLines, highlightCode, tokenizeCode } from '@celesta/code';
    import type { CodeLine, CodeRun, HighlightCodeOptions, HighlightedCodeLine, HighlightedCodeRun } from '@celesta/code';
    const source = 'const value = "😀";';
    const maximum: number = codeCharacterCount(source, { line: 1, column: Array.from(source).length + 1 });
    const options: HighlightCodeOptions = { language: 'ts', tabSize: 4 };
    const lines: HighlightedCodeLine[] = highlightCode(source, options);
    const runs: HighlightedCodeRun[] = lines[0].runs;
    export const colorRuns: TextColorRun[] = runs;
    export const tokenRuns: CodeRun[] = codeLines(tokenizeCode(source, 'ts')).flatMap((line: CodeLine) => line.runs);
    function Demo() {
      const { length } = useTypewriter(source, { framesPerChar: 0.5 });
      return <Code language="ts" visibleCharacters={Math.min(length, maximum)} highlightLines={[1]} highlightWidth={800}>{source}</Code>;
    }
    export default function Root() {
      return <Composition width={800} height={300} fps={30} durationInFrames={20}><Demo /></Composition>;
    }
  `);
  let cli = fileURLToPath(new URL('../../cli/bin/celesta-react-render.js', import.meta.url));
  if (packaged) {
    const runtime = join(directory, 'runtime/react');
    const stage = fileURLToPath(new URL('../../../scripts/stage-react-runtime.mjs', import.meta.url));
    const staged = spawnSync(process.execPath, [stage, runtime], { encoding: 'utf8' });
    assert.equal(staged.status, 0, staged.stderr);
    const closure = JSON.parse(readFileSync(join(runtime, 'runtime-packages.json'), 'utf8'));
    assert.ok(closure['@celesta/code']);
    assert.ok(closure['@twinkleplop/core']);
    // The editor copies this same support template into a project at setup.
    cpSync(join(runtime, 'dist/project-types'), join(directory, '.celesta'), { recursive: true });
    writeFileSync(join(directory, 'tsconfig.json'), '{"extends":"./.celesta/tsconfig.json"}');
    const compiler = fileURLToPath(new URL('../node_modules/typescript/lib/tsc.js', import.meta.url));
    const checked = spawnSync(process.execPath, [compiler, '-p', directory], { encoding: 'utf8' });
    assert.equal(checked.status, 0, checked.stdout + checked.stderr);
    cli = join(runtime, 'dist/cli.js');
  }
  const child = spawn(process.execPath, [cli, entry], { stdio: ['pipe', 'pipe', 'pipe'] });
  signal.addEventListener('abort', () => child.kill(), { once: true });
  let stderr = '';
  child.stderr.on('data', chunk => { stderr += chunk; });
  const exited = new Promise<number | null>((resolve, reject) => {
    child.on('error', reject);
    child.on('exit', resolve);
  });
  const frames: Scene[] = [];
  let measured = 0;
  let firstMeasurements: number | undefined;
  try {
    for await (const line of createInterface({ input: child.stdout })) {
      const message: BridgeMessage = JSON.parse(line);
      const send = (value: unknown) => child.stdin.write(JSON.stringify(value) + '\n');
      assert.equal(message.error, undefined, 'a message without an error');
      if (message.measureText) {
        measured++;
        send({ metrics: metrics(message.measureText) });
      } else if (message.config) {
        firstMeasurements = measured;
        send({ time: { value: 0, timescale: 30 } });
      } else {
        frames.push(message.scene!);
        if (frames.length === 1) send({ time: { value: 19, timescale: 30 } });
        else child.stdin.end();
      }
    }
    assert.equal(await exited, 0, stderr);
    assert.equal(frames.length, 2);
    assert.equal(renderedText(frames[0]), 'c');
    assert.equal(renderedText(frames[1]), 'const value = "😀";');
    assert.equal(measured, firstMeasurements);
  } finally {
    child.kill();
  }
});


test('long lines and changing caret positions have constant measurements and one Text per line', () => {
  for (const source of ['const n = 1; '.repeat(100), Array(20).fill('const n = 1; '.repeat(10)).join('\n')]) {
    let calls = 0;
    setTextMeasurer(async request => metrics(request), request => { calls++; return metrics(request); });
    function Root() {
      const frame = useCurrentFrame();
      const point = useCodePoint(source, { line: 1, column: frame + 1 });
      return React.createElement(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 3 },
        React.createElement(Code, { children: source, language: 'ts', visibleCharacters: frame ? Infinity : 0 }),
        React.createElement(Rect, { x: point.x, width: 2, height: 36 }));
    }
    const mounted = mount(Root);
    frameAt(mounted, 0);
    assert.equal(calls, 4, 'two geometry measurements each for Code and caret, independent of runs');
    const full = layers(frameAt(mounted, 1));
    assert.equal(full.filter(layer => layer.content.type === 'text').length, source.split('\n').length);
    frameAt(mounted, 2);
    frameAt(mounted, 0);
    assert.equal(calls, 4, 'reveal, seek and caret moves reuse full geometry');
  }
});

test('useCodePoint explicitly rejects browser glyph metrics', () => {
  const browserMetrics = (request: MeasureTextRequest): TextMetrics => ({ ...metrics(request), get glyphs(): GlyphMetrics[] {
    throw new Error('Shaped glyph metrics are unavailable in the browser runtime');
  } });
  setTextMeasurer(async request => browserMetrics(request), browserMetrics);
  const Label = () => { useCodePoint('abc', { line: 1, column: 2 }); return null; };
  const Root = () => React.createElement(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 1 }, React.createElement(Label));
  assert.throws(() => frameAt(mount(Root), 0), /useCodePoint\(\).*unsupported in browser renders/);
});

// Each Code line is a Group holding an optional band and an optional Text.
function codeLineTexts(scene: Scene): (Layer | null)[] {
  return groupOf(scene.layers[0]).layers.map(line => groupOf(line).layers.find(layer => layer.content.type === 'text') ?? null);
}

// What a caller drawing one Text per line passes, given `visible` original code points.
function lineText(line: HighlightedCodeLine, visible: number) {
  if (!line.text || visible <= line.start) return null;
  const shown = visible - line.start;
  return {
    text: line.text,
    colorRuns: line.runs.map(({ start, end, color }) => ({ start, end, color })),
    visibleCharacters: shown < line.columns.length - 1 ? line.columns[shown] : undefined,
  };
}

const highlightCases: [CodeLanguage, string, number][] = [
  ['tsx', '<Text label="😀あ">{value}</Text>\r\n', 2],
  ['ts', 'const value: string = "日本語";\n/* first\n second */\n', 2],
  ['json', '{\n\t"value": "😀あ",\t"count": 42\n}\n', 4],
  ['bash', '# comment\n\techo "👩‍💻 🇯🇵 👍🏽"\n', 2],
  ['text', '  😀あ\t é\r\n\n', 2],
  ['ts', 'a\tb\t\tc\r\n\r\n\tx = 1;\rlet y\t= "\t";\n\n', 4],
  ['tsx', 'const 名前 = "👨‍👩‍👧‍👦";\r// 絵文字 🇯🇵\n', 3],
  ['bash', '\r\n\r\recho ok', 2],
  ['bash', 'Celesta-export --react film.tsx out.mp4 \\\n\t--bit-depth 10\n', 2],
  ['text', '', 2],
  ['text', '\n', 2],
];

test('highlightCode matches the text, colorRuns and visibleCharacters Code gives each line', () => {
  setTextMeasurer(async request => metrics(request), metrics);
  for (const [language, source, tabSize] of highlightCases) {
    for (const theme of [codeThemes.dark, codeThemes.light]) {
      const lines = highlightCode(source, { language, theme, tabSize });
      const total = Array.from(source).length;
      const counts = [...Array(total + 2).keys(), Infinity];
      function Root() {
        const frame = useCurrentFrame();
        return React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: counts.length },
          React.createElement(Code, { language, theme, tabSize, visibleCharacters: counts[frame], children: source }));
      }
      const mounted = mount(Root);
      counts.forEach((visible, frame) => {
        const drawn = codeLineTexts(frameAt(mounted, frame));
        assert.equal(drawn.length, lines.length, `${language} ${JSON.stringify(source)}`);
        drawn.forEach((layer, index) => {
          const label = `${language} ${JSON.stringify(source)} line ${index + 1} visible ${visible}`;
          const expected = lineText(lines[index], visible);
          if (!expected) return assert.equal(layer, null, label);
          const content = textOf(layer);
          assert.deepEqual({
            text: content.text, colorRuns: content.style.colorRuns,
            visibleCharacters: content.style.visibleCharacters,
          }, expected, label);
        });
      });
    }
  }
});

test('highlightCode lines record expanded text, source starts, columns and merged runs', () => {
  const theme = { foreground: '#111111', highlightLine: '#000000', tokens: { keyword: '#ff0000', builtin: '#ff0000', string: '#00ff00' } };
  assert.deepEqual(highlightCode('a\tbc\r\n\t😀あ\rx\n\n', { tabSize: 4, theme }), [
    { text: 'a   bc', start: 0, columns: [0, 1, 4, 5, 6], runs: [{ start: 0, end: 6, color: '#111111', type: 'plain' }] },
    { text: '    😀あ', start: 6, columns: [0, 4, 5, 6], runs: [{ start: 0, end: 6, color: '#111111', type: 'plain' }] },
    { text: 'x', start: 10, columns: [0, 1], runs: [{ start: 0, end: 1, color: '#111111', type: 'plain' }] },
    { text: '', start: 12, columns: [0], runs: [] },
    { text: '', start: 13, columns: [0], runs: [] },
  ]);
  const [line] = highlightCode('const s = "👩‍💻";', { language: 'ts', theme });
  assert.equal(line.text, 'const s = "👩‍💻";');
  assert.deepEqual(line.runs.map(run => [Array.from(line.text).slice(run.start, run.end).join(''), run.color, run.type]), [
    ['const', '#ff0000', 'keyword'], [' s = ', '#111111', 'plain'], ['"👩‍💻"', '#00ff00', 'string'], [';', '#111111', 'punctuation'],
  ]);
  // Same-colored neighbours merge and keep the first token's type; codeLines keeps every token.
  const merged = highlightCode('const x = 1;', { language: 'ts', theme: { ...theme, foreground: '#ff0000' } })[0].runs;
  assert.deepEqual(merged, [{ start: 0, end: 12, color: '#ff0000', type: 'keyword' }]);
  const tokens = codeLines(tokenizeCode('const x = 1;', 'ts'))[0].runs;
  assert.deepEqual(tokens.map(run => run.type), ['keyword', 'plain', 'constant', 'plain', 'operator', 'plain', 'number', 'punctuation']);
  assert.deepEqual(codeLines(tokenizeCode('\tx'), 4)[0], { text: '    x', start: 0, columns: [0, 4, 5], runs: [{ type: 'plain', text: '    x', column: 0 }] });
});

test('highlightCode uses Code defaults and rejects the inputs Code rejects', () => {
  const source = 'const tab =\t"x";\n';
  assert.deepEqual(highlightCode(source), highlightCode(source, { language: 'text', theme: codeThemes.dark, tabSize: 2 }));
  assert.deepEqual(highlightCode(source, { language: 'ts' }), highlightCode(source, { language: 'ts', theme: codeThemes.dark, tabSize: 2 }));
  assert.deepEqual(codeLines(tokenizeCode(source)), codeLines(tokenizeCode(source), 2));
  // Wrong on purpose: the checks are for data that comes in untyped.
  assert.throws(() => highlightCode(null as unknown as string), /source must be a string/);
  assert.throws(() => highlightCode('x', { language: 'ruby' as CodeLanguage }), /Unsupported code language/);
  for (const tabSize of [0, -1, 1.5, Infinity, NaN]) {
    assert.throws(() => highlightCode('x', { tabSize }), /tabSize/);
    assert.throws(() => codeLines([], tabSize), /tabSize/);
  }
});

test('one Text per line drawn from highlightCode matches Code, with colorRuns or <Span> fills', () => {
  setTextMeasurer(async request => metrics(request), metrics);
  const source = 'import { Text } from "@celesta/react";\r\n\n\tconst 名前 = "👩‍💻";\r\texport default 名前;\n';
  const style = { fontFamily: 'IBM Plex Mono', fontSize: 24, lineHeight: 36 };
  const theme = codeThemes.light;
  const counts = [0, 10, 41, 50, Infinity];
  function SelfDrawn({ visible, spans }: { visible: number; spans: boolean }) {
    const lines = highlightCode(source, { language: 'tsx', theme, tabSize: 4 });
    const { ascent } = useTextMetrics('M', style);
    return React.createElement(Group, { x: 40, y: 20 }, lines.map((line, index) => {
      const props = lineText(line, visible);
      if (!props) return null;
      const characters = Array.from(line.text);
      return React.createElement(Text, {
        key: index, y: index * style.lineHeight + ascent, anchorY: 'baseline',
        style: {
          ...style, align: 'left', fill: { type: 'solid', color: theme.foreground },
          visibleCharacters: props.visibleCharacters, ...(spans ? {} : { colorRuns: line.runs }),
        },
        children: spans
          ? line.runs.map((run, key) => React.createElement(Span, { key, style: { fill: run.color } }, characters.slice(run.start, run.end).join('')))
          : line.text,
      });
    }));
  }
  const summary = (scene: Scene) => layers(scene).filter(isText).map(({ x, y, content }) => ({
    x, y, text: content.text, baselineAnchor: content.baselineAnchor, fill: content.style.fill,
    fontFamily: content.style.fontFamily, fontSize: content.style.fontSize, lineHeight: content.style.lineHeight,
    colorRuns: content.style.colorRuns!.map(({ start, end, color }) => ({ start, end, color })),
    visibleCharacters: content.style.visibleCharacters,
  }));
  for (const spans of [false, true]) {
    function Root() {
      const visible = counts[useCurrentFrame()];
      return React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: counts.length },
        React.createElement(Code, { x: 40, y: 20, language: 'tsx', style, theme, tabSize: 4, visibleCharacters: visible, children: source }),
        React.createElement(Group, { x: 1000 }, React.createElement(SelfDrawn, { visible, spans })));
    }
    const mounted = mount(Root);
    counts.forEach((visible, frame) => {
      const drawn = summary(frameAt(mounted, frame));
      const code = drawn.filter(layer => layer.x < 1000);
      const own = drawn.filter(layer => layer.x >= 1000).map(layer => ({ ...layer, x: layer.x - 1000 }));
      assert.ok(code.length > 0 || visible === 0);
      assert.deepEqual(own, code, `${spans ? 'Span' : 'colorRuns'} visible ${visible}`);
    });
  }
});
