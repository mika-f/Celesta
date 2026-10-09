import assert from 'node:assert/strict';
import * as React from 'react';
import { test } from 'vitest';
import { Composition } from '@celesta/react';
import { mount } from '@celesta/react/internal';

import { ProjectTrack, useProjectTrack } from '../dist/index.js';

test('a track id named after an Object.prototype member is an unknown track', () => {
  let counted;
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
