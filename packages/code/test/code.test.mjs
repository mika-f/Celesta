import assert from 'node:assert/strict';
import test from 'node:test';
import { spawn, spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import React from 'react';
import { Composition, Font, Rect, useCurrentFrame } from '@celesta/react';
import { mount, setTextMeasurer } from '@celesta/react/internal';
import { Code, codeCharacterCount, codeThemes, tokenizeCode, useCodePoint } from '../dist/index.js';

function metrics(request) {
  const lines = request.text.split('\n');
  const lineHeight = request.style.lineHeight ?? 36;
  return {
    width: Math.max(...lines.map(line => Array.from(line).length)) * 10,
    height: lines.length * lineHeight, ascent: lineHeight * 0.75, descent: lineHeight * 0.25,
    lineHeight, lines: lines.length, glyphs: lines.flatMap((line, row) => {
      const start = lines.slice(0, row).reduce((n, text) => n + Array.from(text).length + 1, 0);
      return Array.from(line).map((text, column) => ({ text, start: start + column, end: start + column + 1,
        rtl: false, x: column * 10, width: 10, line: row }));
    }),
  };
}

function layers(scene) {
  const result = [];
  function walk(layer, x = 0, y = 0) {
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
function renderedText(scene, left = 0) {
  const rows = new Map();
  for (const layer of layers(scene).filter(layer => layer.content.type === 'text')) {
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

const frameAt = (mounted, frame) => mounted.renderAt({ value: frame, timescale: 30 }, null).scene;

test('all supported grammars preserve whitespace and UTF-16 source offsets', () => {
  const samples = {
    tsx: '<Text label="😀あ">{value}</Text>\r\n',
    ts: 'const value: string = "😀あ";\n/* first\n second */\n',
    json: '{\n\t"value": "😀あ", "count": 42\n}\n',
    bash: '# comment\n\techo "😀あ"\n',
    text: '  😀あ\t é\r\n\n',
  };
  for (const [language, source] of Object.entries(samples)) {
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
  assert.throws(() => tokenizeCode('hello', 'ruby'), /Unsupported code language/);
});

test('typing, seeking and line highlights reuse full-source measurements and keep emoji intact', () => {
  const source = 'const value = "😀あ";\r\n\t// comment\n';
  const style = { fontFamily: 'IBM Plex Mono', fontSize: 24, lineHeight: 36 };
  const requests = [];
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
  assert.equal(first[0].content.width, Array.from(source.split('\r\n')[0]).length * 10);
  const measurements = requests.length;
  const typedScene = frameAt(mounted, 1);
  const typed = layers(typedScene);
  assert.equal(renderedText(typedScene, 40), 'const value = "😀');
  assert.equal(typed[0].content.width, first[0].content.width);
  const full = layers(frameAt(mounted, 2));
  assert.equal(full.find(layer => layer.content.type === 'rect').y, 20 + 36);
  assert.ok(full.some(layer => layer.content.text === '  // comment' && layer.x === 40));
  assert.ok(full.some(layer => layer.content.text?.includes('const') && layer.content.style.colorRuns.some(run => run.color === codeThemes.dark.tokens.keyword)));
  assert.equal(full.find(layer => layer.content.type === 'text').content.baselineAnchor, true);
  assert.ok(requests.at(-1).fonts.some(font => font.location.path.endsWith('mono.ttf')));
  frameAt(mounted, 0);
  frameAt(mounted, 3);
  assert.equal(requests.length, measurements, 'unchanged geometry must not make per-frame measurement RPCs');
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
  assert.deepEqual([first.x, first.y, first.content.height], [40, 30, 40]);
  assert.deepEqual([second.x, second.y, second.content.height], [50, 70, 40]);
});

test('highlightWidth spans a panel independently of text width and visibility', () => {
  const requests = [];
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
  assert.deepEqual(first.filter(layer => layer.content.type === 'rect').map(layer => layer.content.width), [30, 30]);
  const measurements = requests.length;
  for (const [frame, width] of [[1, 200], [2, 0], [3, 500]]) {
    const scene = frameAt(mounted, frame);
    const rects = layers(scene).filter(layer => layer.content.type === 'rect');
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
    assert.equal(tokens.find(token => token.text === value).type, 'string', value);
  }
  assert.ok(tokens.some(token => token.type === 'string_escape' && token.text === '\\n'));
  assert.ok(tokenizeCode('"incomplete', 'json').every(token => token.type !== 'property'));
  setTextMeasurer(async request => metrics(request), metrics);
  const Root = () => React.createElement(Composition, { width: 800, height: 300, fps: 30, durationInFrames: 1 },
    React.createElement(Code, { language: 'json', children: source }));
  const textLayers = layers(frameAt(mount(Root), 0)).filter(layer => layer.content.type === 'text');
  assert.ok(textLayers.some(layer => layer.content.style.colorRuns.some(run => Array.from(layer.content.text).slice(run.start, run.end).join('') === JSON.stringify(keys[1]) && run.color === codeThemes.dark.tokens.property)));
  assert.ok(textLayers.some(layer => layer.content.style.colorRuns.some(run => Array.from(layer.content.text).slice(run.start, run.end).join('') === '"broll: value"' && run.color === codeThemes.dark.tokens.string)));
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
  assert.ok(first.some(layer => layer.content.style?.colorRuns.some(run => run.color === '#ff0000')));
  assert.ok(second.some(layer => layer.content.text?.includes('let')));
  assert.ok(second[0].content.width > first[0].content.width);
  assert.equal(plain.filter(layer => layer.content.type === 'text').length, 1);
  assert.equal(plain[1].content.style.fill.color, '#abcdef');
});

test('invalid layout inputs fail before rendering', () => {
  assert.throws(() => tokenizeCode(null), /source must be a string/);
  for (const tabSize of [0, -1, 1.5, Infinity, NaN]) {
    assert.throws(() => Code({ children: '', tabSize }), /tabSize/);
  }
  for (const style of [{ fontSize: 0 }, { lineHeight: -1 }, { fontSize: Infinity }, { lineHeight: NaN }]) {
    assert.throws(() => Code({ children: '', style }), /positive fontSize and lineHeight/);
  }
  assert.throws(() => Code({ children: '', style: { align: 'center' } }), /left alignment/);
  assert.throws(() => Code({ children: '', visibleCharacters: NaN }), /visibleCharacters/);
  for (const highlightWidth of [-1, Infinity, NaN, '100']) {
    assert.throws(() => Code({ children: '', highlightWidth }), /highlightWidth/);
  }
  for (const convert of [codeCharacterCount, useCodePoint]) {
    assert.throws(() => convert(null, { line: 1, column: 1 }), /source must be a string/);
    for (const line of [0, -1, 2, 1.5, Infinity, NaN]) {
      assert.throws(() => convert('abc', { line, column: 1 }), /line is out of range/);
    }
    for (const column of [0, -1, 5, 1.5, Infinity, NaN]) {
      assert.throws(() => convert('abc', { line: 1, column }), /column is out of range/);
    }
  }
});

for (const packaged of [false, true]) test(`${packaged ? 'packaged' : 'source'} CLI resolves Code without project dependencies and shares useTypewriter's runtime`, { timeout: 30000 }, async t => {
  const directory = mkdtempSync(join(tmpdir(), 'celesta-code-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const entry = join(directory, 'entry.tsx');
  writeFileSync(entry, `
    import { Composition } from '@celesta/react';
    import { useTypewriter } from '@celesta/text';
    import { Code, codeCharacterCount } from '@celesta/code';
    const source = 'const value = "😀";';
    const maximum: number = codeCharacterCount(source, { line: 1, column: Array.from(source).length + 1 });
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
  t.signal.addEventListener('abort', () => child.kill(), { once: true });
  let stderr = '';
  child.stderr.on('data', chunk => { stderr += chunk; });
  const exited = new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('exit', resolve);
  });
  const frames = [];
  let measured = 0;
  let firstMeasurements;
  try {
    for await (const line of createInterface({ input: child.stdout })) {
      const message = JSON.parse(line);
      const send = value => child.stdin.write(JSON.stringify(value) + '\n');
      assert.equal(message.error, undefined, message.error);
      if (message.measureText) {
        measured++;
        send({ metrics: metrics(message.measureText) });
      } else if (message.config) {
        firstMeasurements = measured;
        send({ time: { value: 0, timescale: 30 } });
      } else {
        frames.push(message.scene);
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
  const browserMetrics = request => ({ ...metrics(request), get glyphs() {
    throw new Error('Shaped glyph metrics are unavailable in the browser runtime');
  } });
  setTextMeasurer(async request => browserMetrics(request), browserMetrics);
  const Label = () => { useCodePoint('abc', { line: 1, column: 2 }); return null; };
  const Root = () => React.createElement(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 1 }, React.createElement(Label));
  assert.throws(() => frameAt(mount(Root), 0), /useCodePoint\(\).*unsupported in browser renders/);
});
