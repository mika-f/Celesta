import { expect, test } from 'vitest';
import React from 'react';

import { Audio, Composition, FreezeFrame, Group, Mask, Rect, Sequence, Text, Video, useCurrentFrame } from '../dist/index.js';
import { mount } from '../dist/render.js';

const h = React.createElement;
const at = (frame) => ({ value: frame, timescale: 30 });

function scene(children, frame = 0) {
  const Root = () => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 30 }, children);
  return mount(Root).renderAt(at(frame), null);
}

const square = (fill) => h(Rect, { width: 10, height: 10, fill });

test('a <Mask> child becomes the group mask at any sibling position', () => {
  for (const children of [
    [h(Mask, { key: 'm' }, square('#ffffff')), h(Rect, { key: 'r', width: 50, height: 50, fill: '#ff0000' })],
    [h(Rect, { key: 'r', width: 50, height: 50, fill: '#ff0000' }), h(Mask, { key: 'm' }, square('#ffffff'))],
  ]) {
    const [group] = scene(h(Group, null, ...children)).scene.layers;
    expect(group.content.layers).toHaveLength(1);
    expect(group.content.layers[0].content.fill).toEqual({ type: 'solid', color: '#ff0000' });
    expect(group.content.mask).toEqual({ layers: [expect.objectContaining({ content: expect.objectContaining({ type: 'rect' }) })] });
  }
});

test('the mask prop, a <Mask> in the prop, and a <Mask> child serialize alike', () => {
  const matte = () => square('#ffffff');
  const child = () => h(Rect, { width: 50, height: 50, fill: '#ff0000' });
  const groups = scene([
    h(Group, { key: 1, mask: matte() }, child()),
    h(Group, { key: 2, mask: h(Mask, null, matte()) }, child()),
    h(Group, { key: 3 }, child(), h(Mask, null, matte())),
  ]).scene.layers;
  // Layer ids are tree paths, so only the rest of each mask is compared.
  const [bare, wrapped, nested] = groups.map(({ content }) => ({
    ...content.mask,
    layers: content.mask.layers.map(({ content: layer }) => layer),
  }));
  expect(wrapped).toEqual(bare);
  expect(nested).toEqual(bare);
  expect('mode' in bare).toBe(false);
  expect('invert' in bare).toBe(false);

  const [luminance] = scene(h(Group, { mask: h(Mask, { mode: 'luminance', invert: true }, matte()) }, child())).scene.layers;
  expect(luminance.content.mask.mode).toBe('luminance');
  expect(luminance.content.mask.invert).toBe(true);
});

test('null and false give no mask; an empty <Mask> gives an empty one', () => {
  for (const mask of [null, undefined, false]) {
    const [group] = scene(h(Group, { mask }, square('#ff0000'))).scene.layers;
    expect('mask' in group.content).toBe(false);
  }
  const [group] = scene(h(Group, { mask: h(Mask) }, square('#ff0000'))).scene.layers;
  expect(group.content.mask).toEqual({ layers: [] });
});

test('toggling the mask prop keeps the children ids and state', () => {
  const seen = [];
  function Counter() {
    // The initializer runs once per mount.
    React.useState(() => seen.push('mount'));
    return h(Rect, { width: 10, height: 10, fill: '#ff0000' });
  }
  function Root() {
    const frame = useCurrentFrame();
    return h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 30 },
      h(Group, { mask: frame % 2 === 1 && square('#ffffff') },
        h(Video, { src: './a.mp4' }), h(Counter)));
  }
  const mounted = mount(Root);
  const ids = [0, 1, 2].map((frame) => mounted.renderAt(at(frame), null).scene.layers[0].content.layers.map((l) => l.id));
  expect(ids[0]).toEqual(ids[1]);
  expect(ids[1]).toEqual(ids[2]);
  expect(seen).toEqual(['mount']);
});

test('a group without a mask keeps the layer ids it had before masks', () => {
  const [group] = scene(h(Group, null, square('#ff0000'), square('#00ff00'))).scene.layers;
  expect(group.content.layers.map((layer) => layer.id)).toEqual(['root.0.0', 'root.0.1']);
});

test('mask layers get their own tree-path ids and inherit lang', () => {
  const [group] = scene(h(Group, { lang: 'ja', mask: h(Video, { src: './a.mp4' }) }, h(Video, { src: './a.mp4' }))).scene.layers;
  const childId = group.content.layers[0].id;
  const maskId = group.content.mask.layers[0].id;
  expect(childId).not.toBe(maskId);
  const [texted] = scene(h(Group, { lang: 'ja', mask: h(Text, null, 'あ') }, square('#ff0000'))).scene.layers;
  expect(texted.content.mask.layers[0].content.style.lang).toBe('ja');
});

test('raw project layers inside a mask inherit the group lang', () => {
  const text = { id: 't', transform: { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, rotation: 0, anchor: { x: 0.5, y: 0.5 } },
    opacity: 1, content: { type: 'text', text: 'あ', style: {} } };
  const masked = { ...text, id: 'g', content: { type: 'group', layers: [], mask: { layers: [text] } } };
  const [group] = scene(h(Group, { lang: 'ja' }, h('rawLayers', { layers: [masked] }))).scene.layers;
  expect(group.content.layers[0].content.mask.layers[0].content.style.lang).toBe('ja');
});

test('a misplaced or second mask throws', () => {
  const misplaced = [
    h(Mask, null, square('#ffffff')),
    h(Sequence, null, h(Mask, null, square('#ffffff'))),
    h(FreezeFrame, { frame: 0 }, h(Mask, null, square('#ffffff'))),
    h(Group, { mask: h(Mask, null, h(Mask, null, square('#ffffff'))) }),
  ];
  for (const children of misplaced) {
    expect(() => scene(children)).toThrow('`<Mask>` must be a direct child of `<Group>`');
  }
  expect(() => scene(h(Group, null, h(Mask), h(Mask)))).toThrow('a <Group> takes at most one mask');
  expect(() => scene(h(Group, { mask: square('#ffffff') }, h(Mask)))).toThrow('a <Group> takes at most one mask');
  expect(() => scene(h(Group, { mask: h(Mask, { mode: 'red' }) }))).toThrow(
    'unknown mask mode "red"; expected one of alpha, luminance',
  );
});

test('audio inside a mask plays once, in renderAt and in collectAudio', () => {
  const Root = () => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 2 },
    h(Group, { mask: h(Audio, { src: 'matte.wav' }) }, h(Audio, { src: 'child.wav' })));
  const mounted = mount(Root);
  const frame = mounted.renderAt(at(0), null).audio.map(({ src }) => src);
  expect(frame.sort()).toEqual(['child.wav', 'matte.wav']);
  const all = mount(Root).collectAudio().map(({ src }) => src);
  expect(all.filter((src) => src === 'matte.wav')).toHaveLength(2);
  expect(all.filter((src) => src === 'child.wav')).toHaveLength(2);
});
