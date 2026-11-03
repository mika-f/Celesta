// What `<Vrm>` sends: the layer's data as a pure function of the frame
// (`vrmData`), the host element's props (`vrmElementProps`), and the layer a
// mounted composition renders. Run after `pnpm run build:runtime`
// (repository root), which builds the `@celesta/react` and
// `@celesta/character` the module imports.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import { blinkPhase } from '@celesta/character';
import type { LipSyncTrack, MouthShape } from '@celesta/character';
import { Composition } from '@celesta/react';
import { mount } from '@celesta/react/internal';
import { Vrm, vrmData, vrmElementProps } from '../src/index.ts';
import type { VrmProps } from '../src/index.ts';

const src = './avatar.vrm';

/** The first frame in the first 10 s at 30 fps where the default blink is `phase`. */
function firstBlink(phase: 'half' | 'closed'): number {
  for (let frame = 0; frame < 300; frame += 1) {
    if (blinkPhase(frame, 30, {}) === phase) return frame;
  }
  throw new Error(`no ${phase} frame in the first 10 s`);
}

/** `'a'` before 1 s, `'closed'` from then on. */
const track: LipSyncTrack = {
  durationInSeconds: 2,
  mouthAtSeconds: (seconds) => (seconds < 1 ? 'a' : 'closed'),
  mouthAtFrame: (frame, fps) => (frame / fps < 1 ? 'a' : 'closed'),
};

test('blinks_from_blinkPhase', () => {
  const closed = firstBlink('closed');
  assert.equal(vrmData({ src }, closed, 30).expressions?.blink, 1);
  assert.equal(vrmData({ src }, firstBlink('half'), 30).expressions?.blink, 0.5);
  assert.equal(vrmData({ src }, 0, 30).expressions?.blink, 0);
  assert.ok(!('blink' in (vrmData({ src, blink: false }, closed, 30).expressions ?? {})));
  // Explicit expressions win over the blink.
  assert.equal(vrmData({ src, expressions: { blink: 0.3 } }, closed, 30).expressions?.blink, 0.3);
});

test('mouth_shapes_set_vowels', () => {
  const vowels: [MouthShape, Record<string, number> | undefined][] = [
    ['closed', undefined],
    ['a', { aa: 1 }],
    ['i', { ih: 1 }],
    ['u', { ou: 1 }],
    ['e', { ee: 1 }],
    ['o', { oh: 1 }],
  ];
  for (const [mouth, expressions] of vowels) {
    assert.deepEqual(vrmData({ src, blink: false, mouth }, 0, 30).expressions, expressions, mouth);
  }
  assert.deepEqual(vrmData({ src, blink: false, mouth: { ih: 0.4, ou: 0.2 } }, 0, 30).expressions, {
    ih: 0.4,
    ou: 0.2,
  });
  assert.equal(vrmData({ src, mouth: 'a', expressions: { aa: 0.2 } }, 0, 30).expressions?.aa, 0.2);
});

test('lip_sync_smooths_like_live2d', () => {
  const expressions = vrmData({ src, blink: false, lipSync: track, mouthSmoothing: 2 }, 30, 30).expressions;
  assert.deepEqual(expressions, { aa: 0.5 });
  // The default smoothing is 2 frames.
  assert.deepEqual(vrmData({ src, blink: false, lipSync: track }, 30, 30).expressions, { aa: 0.5 });
  // Without smoothing it snaps; past the track's vowels nothing is sent.
  assert.deepEqual(vrmData({ src, blink: false, lipSync: track, mouthSmoothing: 0 }, 29, 30).expressions, { aa: 1 });
  assert.equal(vrmData({ src, blink: false, lipSync: track }, 40, 30).expressions, undefined);
  // `mouth` wins over the track, a closed mouth included.
  assert.deepEqual(vrmData({ src, blink: false, mouth: 'a', lipSync: track }, 40, 30).expressions, { aa: 1 });
  assert.equal(vrmData({ src, blink: false, mouth: 'closed', lipSync: track }, 15, 30).expressions, undefined);
});

test('mouth_smoothing_is_a_number_of_frames', () => {
  for (const mouthSmoothing of [Number.POSITIVE_INFINITY, Number.NaN, -1]) {
    assert.throws(
      () => vrmData({ src, lipSync: track, mouthSmoothing }, 30, 30),
      /Vrm mouthSmoothing is a number of frames, 0 or more/,
      `${mouthSmoothing}`,
    );
  }
});

test('lights_take_srgb_hex', () => {
  const { lights } = vrmData(
    { src, lights: { directional: [{ direction: [0, -1, 0], color: '#808080' }], ambient: '#fff' } },
    0,
    30,
  );
  assert.ok(lights);
  const [light] = lights.directional;
  assert.deepEqual(light.direction, [0, -1, 0]);
  for (const channel of light.color) assert.ok(Math.abs(channel - 0.2158605) < 5e-7, `${channel}`);
  assert.equal(light.intensity, 1);
  assert.deepEqual(lights.ambient, [1, 1, 1]);
  assert.throws(
    () => vrmData({ src, lights: { directional: [{ direction: [0, -1, 0], color: 'red' }] } }, 0, 30),
    /Vrm lights colors are #rgb or #rrggbb/,
  );
  assert.equal(vrmData({ src }, 0, 30).lights, undefined);

  // The defaults: white at intensity 1, and no ambient key.
  assert.deepEqual(vrmData({ src, lights: { directional: [{ direction: [1, 0, 0] }] } }, 0, 30).lights, {
    directional: [{ direction: [1, 0, 0], color: [1, 1, 1], intensity: 1 }],
  });
  // The linear segment of the sRGB curve, and the intensity as given.
  const dark = vrmData({ src, lights: { directional: [{ direction: [1, 0, 0], color: '#0a0a0a', intensity: 2 }] } }, 0, 30)
    .lights!.directional[0];
  for (const channel of dark.color) assert.ok(Math.abs(channel - 10 / 255 / 12.92) < 1e-9, `${channel}`);
  assert.equal(dark.intensity, 2);
});

test('canvas_follows_width_height_and_aspect', () => {
  const canvas = (props: { width?: number; height?: number; aspect?: number }) =>
    vrmData({ src, ...props }, 0, 30).canvas;
  assert.deepEqual(canvas({}), [1920, 1080]);
  assert.deepEqual(canvas({ width: 1080, height: 1920 }), [1080, 1920]);
  assert.deepEqual(canvas({ width: 1080, height: 1920, aspect: 1 }), [1080, 1920]);
  assert.deepEqual(canvas({ height: 1920, aspect: 9 / 16 }), [1080, 1920]);
  assert.deepEqual(canvas({ width: 960 }), [960, 540]);
  assert.deepEqual(canvas({ width: 1000, aspect: 2 }), [1000, 500]);
  assert.deepEqual(canvas({ aspect: 1 }), [1920, 1920]);
  for (const bad of [{ width: 0 }, { width: Number.POSITIVE_INFINITY }, { height: -1 }, { aspect: Number.NaN }]) {
    assert.throws(() => canvas(bad), /Vrm width, height, and aspect are positive numbers/, JSON.stringify(bad));
  }
});

test('canvas_sides_are_from_1_to_16384', () => {
  const canvas = (props: { width?: number; height?: number; aspect?: number }) =>
    vrmData({ src, ...props }, 0, 30).canvas;
  assert.deepEqual(canvas({ width: 1, height: 1 }), [1, 1]);
  assert.deepEqual(canvas({ width: 16384, height: 16384 }), [16384, 16384]);
  assert.deepEqual(canvas({ width: 16384, aspect: 16384 }), [16384, 1]);
  // The computed side counts, not only the ones given.
  const bad: [{ width?: number; height?: number; aspect?: number }, string][] = [
    [{ width: 0.5, height: 1 }, '[0.5, 1]'],
    [{ width: 16384.5, height: 1 }, '[16384.5, 1]'],
    [{ width: 1, aspect: 2 }, '[1, 0.5]'],
    [{ width: 2048, aspect: 1 / 16 }, '[2048, 32768]'],
    [{ aspect: 1 / 16 }, '[1920, 30720]'],
  ];
  for (const [props, got] of bad) {
    assert.throws(
      () => canvas(props),
      { message: `Vrm width and height are from 1 to 16384 pixels (got ${got})` },
      JSON.stringify(props),
    );
  }
});

test('lights_are_at_most_four', () => {
  const directional = (count: number) =>
    Array.from({ length: count }, () => ({ direction: [0, -1, 0] as [number, number, number] }));
  assert.equal(vrmData({ src, lights: { directional: directional(4) } }, 0, 30).lights?.directional.length, 4);
  assert.throws(() => vrmData({ src, lights: { directional: directional(5) } }, 0, 30), {
    message: 'Vrm lights take at most 4 directional lights (got 5)',
  });
});

test('element_props_keep_only_the_common_ones', () => {
  const props: VrmProps = {
    src,
    id: 'avatar',
    x: 10,
    scale: 0.5,
    width: 1080,
    height: 1920,
    aspect: 9 / 16,
    expressions: { happy: 1 },
    mouth: 'a',
    lipSync: track,
    mouthSmoothing: 1,
    blink: { seed: 1 },
    lookAt: 'camera',
    camera: 'face',
    lights: { ambient: '#fff' },
  };
  const element = vrmElementProps(props, 45, 30);
  assert.deepEqual(Object.keys(element).sort(), ['data', 'id', 'scale', 'src', 'time', 'x']);
  assert.equal(element.scale, 0.5);
  assert.equal(element.x, 10);
  assert.equal(element.src, src);
  assert.equal(element.time, 1.5);
  assert.deepEqual(element.data, vrmData(props, 45, 30));
});

test('look_at_and_camera_pass_through', () => {
  const data = vrmData({ src, blink: false, lookAt: { yaw: 10, pitch: -5 }, camera: 'bust' }, 0, 30);
  assert.deepEqual(data, { canvas: [1920, 1080], lookAt: { yaw: 10, pitch: -5 }, camera: 'bust' });
  const explicit = {
    position: [0, 1.5, 2] as [number, number, number],
    target: [0, 1.5, 0] as [number, number, number],
    fov: 40,
  };
  assert.deepEqual(vrmData({ src, blink: false, lookAt: 'camera', camera: explicit }, 0, 30), {
    canvas: [1920, 1080],
    lookAt: 'camera',
    camera: explicit,
  });
});

test('the_layer_is_plugin_vrm_with_the_model_asset', () => {
  const h = React.createElement;
  const mounted = mount(() =>
    h(
      Composition,
      { width: 1920, height: 1080, fps: 30, durationInFrames: 90 },
      h(Vrm, { src: './a.vrm', x: 10, width: 960, blink: false, camera: 'bust' }),
    ),
  );
  const { scene } = mounted.renderAt({ value: 15, timescale: 30 }, null);
  assert.equal(scene.layers.length, 1);
  assert.deepEqual(scene.layers[0].content, {
    type: 'plugin',
    plugin: 'vrm',
    assets: { model: { id: './a.vrm', location: { type: 'file', path: './a.vrm' } } },
    time: 0.5,
    data: { canvas: [960, 540], camera: 'bust' },
  });
});
