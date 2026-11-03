import assert from 'node:assert/strict';
import * as React from 'react';
import { test } from 'vitest';
import { Composition, Rect, registerComponent } from '@celesta.studio/react';
import type { Layer, LayerEffects } from '@celesta.studio/react';
import { mount } from '@celesta.studio/react/internal';

import { ProjectTimeline, ProjectTrack, useProjectTrack } from '../src/index.js';

test('a track id named after an Object.prototype member is an unknown track', () => {
  let counted: number | undefined;
  function Count() {
    counted = useProjectTrack('constructor').length;
    return null;
  }
  const mounted = mount(() => React.createElement(
    Composition,
    { width: 10, height: 10, fps: 1, durationInFrames: 1 },
    React.createElement(Count),
    React.createElement(ProjectTrack, { id: 'toString' }),
  ));
  try {
    const { scene } = mounted.renderAt({ value: 0, timescale: 1 }, { layers: [], tracks: {} });
    assert.equal(counted, 0);
    assert.deepEqual(scene.layers, []);
  } finally {
    mounted.dispose();
  }
});

test('a resolved component item keeps the effects the evaluator computed', () => {
  registerComponent('TintedForEffects', () => React.createElement(Rect, { width: 4, height: 4, fill: '#ffffff' }));
  const effects: LayerEffects = { blur: 2, filters: [{ type: 'grayscale', amount: 1 }, { type: 'vignette', amount: 0.5, radius: 0.5, softness: 0.5 }] };
  const layer: Layer = {
    id: 'item', transform: { position: { x: 3, y: 4 }, scale: { x: 1, y: 1 }, rotation: 0, anchor: { x: 0, y: 0 } },
    opacity: 1, effects, content: { type: 'missingComponent', component: 'TintedForEffects', props: {} },
  };
  const mounted = mount(() => React.createElement(
    Composition,
    { width: 10, height: 10, fps: 1, durationInFrames: 1 },
    React.createElement(ProjectTimeline),
  ));
  try {
    const { scene } = mounted.renderAt({ value: 0, timescale: 1 }, { layers: [layer], tracks: {} });
    const [group] = scene.layers;
    assert.equal(group.content.type, 'group');
    assert.deepEqual(group.effects, effects);
  } finally {
    mounted.dispose();
  }
});
