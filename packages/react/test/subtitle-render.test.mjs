import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';

import {
  Assets,
  Character,
  CharacterView,
  Composition,
  Dialogue,
  DialogueSeries,
  Group,
  Rect,
  Sequence,
  Text,
  planDialogue,
  registerComponent,
} from '../dist/index.js';
import { createResolver, mount } from '../dist/render.js';
import { setTextMeasurer } from '../dist/text-measure.js';

// Every character is `fontSize` wide and one line is `fontSize * 1.25` high.
function measure({ text, style }) {
  const size = style.fontSize ?? 10;
  return {
    width: Array.from(text).length * size,
    height: size * 1.25,
    ascent: size,
    descent: size * 0.25,
    lineHeight: size * 1.25,
    lines: 1,
    glyphs: [],
  };
}

function installMeasurer() {
  const requests = [];
  const sync = (request) => {
    requests.push(request);
    return measure(request);
  };
  setTextMeasurer(async (request) => sync(request), sync);
  return requests;
}

const h = React.createElement;

/** A band sized from the metrics, and the facts the render got as text. */
function band({ text, character, metrics, held, frame, durationInFrames, maxWidth }) {
  return h(
    React.Fragment,
    null,
    h(Rect, { id: 'band', width: metrics.width + 20, height: metrics.height, fill: '#000000' }),
    h(Text, { id: 'facts' }, `${character.id}/${character.name}/${character.displayName}|${text}|${held}|${frame}/${durationInFrames}|${maxWidth}`),
    held ? null : h(Text, { id: 'said' }, text),
  );
}

function subtitle(extra = {}) {
  return { x: 100, y: 50, maxWidth: 600, style: { fontSize: 10 }, render: band, ...extra };
}

function portrait(name) {
  return { defaultExpression: 'calm', expressions: { calm: `${name}-calm.png`, smile: `${name}-smile.png` } };
}

function renderFrames(Root, frames) {
  const mounted = mount(Root);
  return frames.map((frame) => mounted.renderAt({ value: frame, timescale: 30 }, null).scene);
}

/** Layers with an id, depth first. */
function find(layers, id) {
  for (const layer of layers) {
    if (layer.id === id) return layer;
    if (layer.content.type === 'group') {
      const found = find(layer.content.layers, id);
      if (found) return found;
    }
  }
  return undefined;
}

function texts(layers) {
  return layers.flatMap((layer) =>
    layer.content.type === 'text' ? [layer.content.text] : layer.content.type === 'group' ? texts(layer.content.layers) : [],
  );
}

function images(layers) {
  return layers.flatMap((layer) =>
    layer.content.type === 'image' ? [layer.content.asset.id] : layer.content.type === 'group' ? images(layer.content.layers) : [],
  );
}

test('subtitle.render draws the line from its text, speaker, metrics, and sequence', () => {
  const requests = installMeasurer();
  const mira = React.createRef();
  const view = React.createRef();
  // The view and the line mount together at frame 10, so the view's ref is
  // only attached by that commit.
  const Root = () =>
    h(
      Composition,
      { width: 640, height: 360, fps: 30, durationInFrames: 60 },
      h(Assets, null, h(Character, { ref: mira, name: 'mira', displayName: 'ミラ', portrait: portrait('mira'), subtitle: subtitle() })),
      h(
        Sequence,
        { from: 10, durationInFrames: 30 },
        h(CharacterView, { ref: view, character: mira }),
        h(Dialogue, { character: view, id: 'line', expression: 'smile' }, 'Hello', ' there'),
      ),
    );
  const [first, later] = renderFrames(Root, [10, 25]);
  for (const [scene, frame] of [[first, 0], [later, 15]]) {
    const subtitleGroup = find(scene.layers, 'line.subtitle');
    assert.ok(subtitleGroup, 'the rendered subtitle sits in the dialogue layer');
    assert.deepEqual(subtitleGroup.transform.position, { x: 100, y: 50 });
    assert.equal(find(scene.layers, 'band').content.width, 11 * 10 + 20);
    assert.equal(find(scene.layers, 'facts').content.text, `mira/mira/ミラ|Hello there|false|${frame}/30|600`);
    assert.equal(find(scene.layers, 'said').content.text, 'Hello there');
    assert.deepEqual(images(scene.layers), ['mira-smile.png']);
  }
  assert.ok(requests.some((request) => request.text === 'Hello there' && request.maxWidth === 600));
});

test('a held Dialogue keeps a rendered band but no plain subtitle', () => {
  installMeasurer();
  const mira = React.createRef();
  const hana = React.createRef();
  const miraView = React.createRef();
  const hanaView = React.createRef();
  const Root = () =>
    h(
      Composition,
      { width: 640, height: 360, fps: 30, durationInFrames: 30 },
      h(
        Assets,
        null,
        h(Character, { ref: mira, name: 'Mira', portrait: portrait('mira'), subtitle: subtitle() }),
        h(Character, { ref: hana, name: 'Hana', portrait: portrait('hana'), subtitle: { style: { fontSize: 10 } } }),
      ),
      h(CharacterView, { ref: miraView, character: mira }),
      h(CharacterView, { ref: hanaView, character: hana }),
      h(Dialogue, { character: miraView, held: true }, 'Bye'),
      h(Dialogue, { character: hanaView, held: true }, 'Bye too'),
    );
  const [scene] = renderFrames(Root, [3]);
  // displayName falls back to name; the plain subtitle draws nothing.
  assert.deepEqual(texts(scene.layers), ['Mira/Mira/Mira|Bye|true|3/30|600']);
});

test('DialogueSeries holdSubtitle keeps the band through gaps and restarts it after a lead-in', async () => {
  installMeasurer();
  const plan = await planDialogue(
    [
      { id: 'one', text: 'First', durationInFrames: 10, gap: 0.2, speaker: 'mira', expression: 'smile' },
      { id: 'two', text: 'Second', durationInFrames: 10, gap: 0.2, speaker: 'hana' },
      { id: 'three', text: 'Third', durationInFrames: 10, gap: 0.2, leadIn: 0.5, speaker: 'mira' },
    ],
    { fps: 30 },
  );
  // one: 0–10, gap to 16; two: 16–26, gap to 32; lead-in to 47; three: 47–57, gap to 63.
  assert.deepEqual(plan.lines.map((line) => line.from), [0, 16, 47]);
  const mira = React.createRef();
  const hana = React.createRef();
  const miraView = React.createRef();
  const hanaView = React.createRef();
  const Root = () =>
    h(
      Composition,
      { width: 640, height: 360, fps: 30, durationInFrames: plan.durationInFrames },
      h(
        Assets,
        null,
        h(Character, { ref: mira, name: 'mira', displayName: 'ミラ', portrait: portrait('mira'), subtitle: subtitle() }),
        h(Character, { ref: hana, name: 'hana', displayName: 'ハナ', portrait: portrait('hana'), subtitle: subtitle() }),
      ),
      h(Group, null, h(CharacterView, { ref: miraView, character: mira }), h(CharacterView, { ref: hanaView, character: hana })),
      h(DialogueSeries, { plan, views: { mira: miraView, hana: hanaView }, holdSubtitle: true }),
    );
  const frames = [0, 9, 12, 16, 30, 35, 47, 60];
  const scenes = renderFrames(Root, frames);
  const facts = scenes.map((scene) => find(scene.layers, 'facts')?.content.text);
  assert.deepEqual(facts, [
    'mira/mira/ミラ|First|false|0/32|600',
    'mira/mira/ミラ|First|false|9/32|600',
    'mira/mira/ミラ|First|true|12/32|600',
    'hana/hana/ハナ|Second|false|16/32|600',
    'hana/hana/ハナ|Second|true|30/32|600',
    undefined,
    'mira/mira/ミラ|Third|false|0/16|600',
    'mira/mira/ミラ|Third|true|13/16|600',
  ]);
  // The held gap shows the band without the line, and the expression is the view's own again.
  assert.equal(find(scenes[2].layers, 'said'), undefined);
  assert.deepEqual(images(scenes[1].layers), ['mira-smile.png', 'hana-calm.png']);
  assert.deepEqual(images(scenes[2].layers), ['mira-calm.png', 'hana-calm.png']);

  // holdThroughGap keeps the line itself through the gap, inside the same run.
  const HeldLines = () =>
    h(
      Composition,
      { width: 640, height: 360, fps: 30, durationInFrames: plan.durationInFrames },
      h(Assets, null,
        h(Character, { ref: mira, name: 'mira', portrait: portrait('mira'), subtitle: subtitle() }),
        h(Character, { ref: hana, name: 'hana', portrait: portrait('hana'), subtitle: subtitle() })),
      h(CharacterView, { ref: miraView, character: mira }),
      h(CharacterView, { ref: hanaView, character: hana }),
      h(DialogueSeries, { plan, views: { mira: miraView, hana: hanaView }, holdSubtitle: true, holdThroughGap: true }),
    );
  const [held] = renderFrames(HeldLines, [12]);
  assert.equal(find(held.layers, 'facts').content.text, 'mira/mira/mira|First|false|12/32|600');
});

test('an editor preview draws a rendered subtitle whose view mounts with it', () => {
  installMeasurer();
  const mira = React.createRef();
  const view = React.createRef();
  registerComponent('SubtitlePreviewTest', () =>
    h(
      React.Fragment,
      null,
      h(Assets, null, h(Character, { ref: mira, name: 'mira', portrait: portrait('mira'), subtitle: subtitle() })),
      h(CharacterView, { ref: view, character: mira }),
      h(Dialogue, { character: view }, 'Preview'),
    ),
  );
  const [layers] = createResolver().resolve([{ component: 'SubtitlePreviewTest', props: {} }], {
    width: 640, height: 360, fps: 30, durationInFrames: 30, time: { value: 0, timescale: 30 }, preview: true,
  });
  assert.equal(find(layers, 'facts').content.text, 'mira/mira/mira|Preview|false|0/30|600');
});

test('a rendered subtitle rejects children that are not text, like a plain one', () => {
  installMeasurer();
  const mira = React.createRef();
  const view = React.createRef();
  const Root = () =>
    h(
      Composition,
      { width: 640, height: 360, fps: 30, durationInFrames: 30 },
      h(Assets, null, h(Character, { ref: mira, name: 'mira', portrait: portrait('mira'), subtitle: subtitle() })),
      h(CharacterView, { ref: view, character: mira }),
      h(Dialogue, { character: view }, 'Hi ', h('group', null)),
    );
  assert.throws(() => renderFrames(Root, [0]), /<Dialogue> children must be a string, a number, or an array of those/);
});
