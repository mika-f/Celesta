// <PluginLayer>: one layer of plugin content, from the public API, so plugin
// modules need nothing from `@celesta.studio/react/internal`.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';
import type { ReactElement } from 'react';

import { Composition, PluginLayer, Sequence } from '../src/index.js';
import type { Time } from '../src/index.js';
import { mount } from '../src/render.js';

import { childrenOf, contentOf } from './helpers.mjs';

const h = React.createElement;
const at = (frame: number): Time => ({ value: frame, timescale: 30 });
const film = (body: ReactElement) => () => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 90 }, body);

test('draws plugin content with its assets, the sequence time, and data', () => {
  const mounted = mount(
    film(h(Sequence, { from: 30 }, h(PluginLayer, { plugin: 'flag', assets: { image: './flag.png' }, data: { waves: 2 }, x: 10, opacity: 0.5 }))),
  );
  const { scene } = mounted.renderAt(at(45), null);
  // The sequence is a group around it.
  const [layer] = childrenOf(scene.layers[0]);
  assert.deepEqual(contentOf(layer, 'plugin'), {
    type: 'plugin',
    plugin: 'flag',
    assets: { image: { id: './flag.png', location: { type: 'file', path: './flag.png' } } },
    time: 0.5,
    data: { waves: 2 },
  });
  assert.equal(layer.opacity, 0.5);
});

test('leaves out assets and data it was not given, and takes a time of its own', () => {
  const mounted = mount(film(h(PluginLayer, { plugin: 'turntable', time: 3 })));
  const { scene } = mounted.renderAt(at(0), null);
  assert.deepEqual(contentOf(scene.layers[0], 'plugin'), { type: 'plugin', plugin: 'turntable', time: 3 });
});

test('refuses a layer without a plugin id or with a time that is not finite', () => {
  assert.throws(
    () => mount(film(h(PluginLayer, { plugin: '' }))).renderAt(at(0), null),
    /needs the id of the plugin/,
  );
  assert.throws(
    () => mount(film(h(PluginLayer, { plugin: 'flag', time: Number.NaN }))).renderAt(at(0), null),
    /time is a finite number/,
  );
});
