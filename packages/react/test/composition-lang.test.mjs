import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import React from 'react';
import { test } from 'vitest';

import { Composition, Group, ProjectTimeline, Sequence, Text, TextBox, useFitText, useTextMetrics } from '../dist/index.js';
import { mount, createResolver } from '../dist/render.js';
import { registerComponent } from '../dist/registry.js';
import { setTextMeasurer } from '../dist/text-measure.js';

function texts(layers) {
  return layers.flatMap(({ content }) => content.type === 'text' ? [content]
    : content.type === 'group' ? texts(content.layers) : []);
}

test('composition language reaches nested text, project layers, metrics and component previews', () => {
  const requests = [];
  setTextMeasurer(async () => { throw new Error('unexpected async measurement'); }, (request) => {
    requests.push(request);
    return { width: request.style.lang === 'zh-Hant' ? 80 : 40, height: 24,
      ascent: 18, descent: 6, lineHeight: 24, lines: 1, glyphs: [] };
  });
  const sharedStyle = Object.freeze({ fontSize: 40 });
  function Caption() {
    const inherited = useTextMetrics('inherited', sharedStyle);
    const override = useTextMetrics('override', { lang: 'zh-Hant' });
    return React.createElement(Group, null,
      React.createElement(Text, { style: sharedStyle, maxWidth: inherited.width }, 'inherited'),
      React.createElement(Text, { style: { lang: 'zh-Hant' }, maxWidth: override.width }, 'override'));
  }
  const config = { width: 400, height: 200, fps: 30, durationInFrames: 2 };
  const projectLayers = mount(() => React.createElement(Composition, config,
    React.createElement(Group, null,
      React.createElement(Text, null, 'project'),
      React.createElement(Text, { style: { lang: 'ko' } }, 'project override'))))
    .renderAt({ value: 0, timescale: 30 }, null).scene.layers;
  const originalProjectLayers = structuredClone(projectLayers);
  const root = mount(() => {
    // Hooks in the entry itself also see the discovered composition language.
    const rootMetrics = useTextMetrics('root');
    return React.createElement(Composition, { ...config, lang: 'ja-JP' },
      React.createElement(Text, { maxWidth: rootMetrics.width }, 'root'),
      React.createElement(Sequence, { from: 0 }, React.createElement(Caption)),
      React.createElement(Text, { style: { lang: null } }, 'nullable'),
      React.createElement(ProjectTimeline));
  });
  assert.equal(root.config.lang, 'ja-JP');
  const scene = root.renderAt({ value: 0, timescale: 30 }, { layers: projectLayers, tracks: {} }).scene;
  assert.deepEqual(texts(scene.layers).map(({ text, style, maxWidth }) => [text, style.lang, maxWidth]), [
    ['root', 'ja-JP', 40], ['inherited', 'ja-JP', 40], ['override', 'zh-Hant', 80],
    ['nullable', 'ja-JP', undefined], ['project', 'ja-JP', undefined], ['project override', 'ko', undefined],
  ]);
  assert.ok(requests.filter(({ text }) => text === 'inherited').every(({ style }) => style.lang === 'ja-JP'));
  assert.deepEqual(projectLayers, originalProjectLayers);
  assert.deepEqual(sharedStyle, { fontSize: 40 });

  registerComponent('CompositionLanguageCaption', Caption);
  const preview = createResolver(root.config.lang).resolve(
    [{ component: 'CompositionLanguageCaption', props: {} }],
    { ...config, time: { value: 0, timescale: 30 }, preview: true },
  );
  assert.deepEqual(texts(preview[0]), texts(scene.layers).filter(({ text }) => text === 'inherited' || text === 'override'));
  const plain = mount(() => React.createElement(Composition, config, React.createElement(Caption)));
  assert.equal(plain.config.lang, undefined);
  assert.equal(texts(plain.renderAt({ value: 0, timescale: 30 }, null).scene.layers)[0].style.lang, undefined);
  assert.throws(() => mount(() => React.createElement(Composition, { ...config, lang: 42 })), /string `lang`/);
});

test('fitted text measures with the inherited language and preserves explicit overrides', () => {
  setTextMeasurer(async () => { throw new Error('unexpected async measurement'); }, ({ style }) => {
    const width = style.fontSize * (style.lang === 'zh-Hant' ? 4 : style.lang === 'ja-JP' ? 2 : 1);
    return { width, height: style.fontSize, ascent: style.fontSize, descent: 0,
      lineHeight: style.fontSize, lines: 1, glyphs: [] };
  });
  const options = Object.freeze({ width: 40, height: 40, minFontSize: 5, maxFontSize: 30,
    style: Object.freeze({ fontFamily: 'Test Font' }) });
  function Caption() {
    const inherited = useFitText('fit', options);
    const override = useFitText('override', { ...options, style: { lang: 'zh-Hant' } });
    return React.createElement(Group, null,
      React.createElement(Text, { style: inherited.style }, 'fit'),
      React.createElement(Text, { style: override.style }, 'override'),
      React.createElement(TextBox, options, 'box'));
  }
  const config = { width: 400, height: 200, fps: 30, durationInFrames: 1, lang: 'ja-JP' };
  const root = mount(() => React.createElement(Composition, config, React.createElement(Caption)));
  const content = texts(root.renderAt({ value: 0, timescale: 30 }, null).scene.layers);
  assert.deepEqual(content.map(({ text, style }) => [text, style.lang, style.fontSize]), [
    ['fit', 'ja-JP', 20], ['override', 'zh-Hant', 10], ['box', 'ja-JP', 20],
  ]);
  registerComponent('CompositionLanguageFittedCaption', Caption);
  const preview = createResolver(root.config.lang).resolve(
    [{ component: 'CompositionLanguageFittedCaption', props: {} }],
    { ...config, time: { value: 0, timescale: 30 }, preview: true },
  );
  assert.deepEqual(texts(preview[0]), content);
  assert.deepEqual(options.style, { fontFamily: 'Test Font' });
});

test('CLI inherits composition language for subtitles and preview requests from older bridges', () => {
  const dir = mkdtempSync(join(tmpdir(), 'celesta-composition-lang-'));
  const entry = join(dir, 'entry.tsx');
  writeFileSync(entry, `
    import React from 'react';
    import { Assets, Character, CharacterView, Composition, Dialogue, Text, registerComponent } from '@celesta/react';
    const character = React.createRef();
    const view = React.createRef();
    function Caption() { return <Text>preview</Text>; }
    registerComponent('LanguageCaption', Caption);
    export default function Root() {
      return <Composition width={400} height={200} fps={30} durationInFrames={1} lang="ja-JP">
        <Assets><Character ref={character} name="Hana"
          portrait={{ expressions: { normal: './portrait.png' }, defaultExpression: 'normal' }}
          subtitle={{ style: { fontSize: 40 } }} /></Assets>
        <CharacterView ref={view} character={character} />
        <Dialogue character={view}>subtitle</Dialogue>
        <Text style={{ lang: 'zh-Hant' }}>override</Text>
      </Composition>;
    }
  `);
  try {
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('../bin/celesta-react-render.js', import.meta.url)), entry], {
      input: JSON.stringify({ time: { value: 0, timescale: 30 } }) + '\n'
        + JSON.stringify({ components: [{ component: 'LanguageCaption', props: {} }],
          runtime: { width: 400, height: 200, fps: 30, durationInFrames: 1, time: { value: 0, timescale: 30 }, preview: true } }) + '\n',
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);
    const [metadata, frame, preview] = result.stdout.split(/\r?\n/).filter(Boolean).map(JSON.parse);
    assert.equal(metadata.config.lang, 'ja-JP');
    assert.deepEqual(texts(frame.scene.layers).map(({ style }) => style.lang), ['ja-JP', 'zh-Hant']);
    assert.equal(texts(preview.components[0])[0].style.lang, 'ja-JP');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
