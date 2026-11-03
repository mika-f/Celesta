// What `<Vrm>` sends: the layer's data as a pure function of the frame
// (`vrmData`), and the host element's props (`vrmElementProps`). Run after
// `pnpm run build:runtime` (repository root), which builds the
// `@celesta/react` and `@celesta/character` the module imports.

import assert from 'node:assert/strict';
import { test } from 'vitest';

import { blinkPhase } from '@celesta/character';
import type { LipSyncTrack } from '@celesta/character';
import { vrmData, vrmElementProps } from '../src/index.ts';

const src = './avatar.vrm';

test('blinks_from_blinkPhase', () => {
  let closed = -1;
  for (let frame = 0; frame < 300; frame += 1) {
    if (blinkPhase(frame, 30, {}) === 'closed') {
      closed = frame;
      break;
    }
  }
  assert.ok(closed >= 0, 'no closed frame in the first 10 s');
  assert.equal(vrmData({ src }, closed, 30).expressions?.blink, 1);
  assert.equal(vrmData({ src }, 0, 30).expressions?.blink, 0);
  assert.ok(!('blink' in (vrmData({ src, blink: false }, closed, 30).expressions ?? {})));
});

test('mouth_shapes_set_vowels', () => {
  assert.deepEqual(vrmData({ src, blink: false, mouth: 'a' }, 0, 30).expressions, { aa: 1 });
  assert.deepEqual(vrmData({ src, blink: false, mouth: 'o' }, 0, 30).expressions, { oh: 1 });
  assert.equal(vrmData({ src, blink: false, mouth: 'closed' }, 0, 30).expressions, undefined);
  assert.deepEqual(vrmData({ src, blink: false, mouth: { ih: 0.4, ou: 0.2 } }, 0, 30).expressions, {
    ih: 0.4,
    ou: 0.2,
  });
  assert.equal(vrmData({ src, mouth: 'a', expressions: { aa: 0.2 } }, 0, 30).expressions?.aa, 0.2);
});

test('lip_sync_smooths_like_live2d', () => {
  const track: LipSyncTrack = {
    durationInSeconds: 2,
    mouthAtSeconds: (seconds) => (seconds < 1 ? 'a' : 'closed'),
    mouthAtFrame: (frame, fps) => (frame / fps < 1 ? 'a' : 'closed'),
  };
  const expressions = vrmData({ src, blink: false, lipSync: track, mouthSmoothing: 2 }, 30, 30).expressions;
  assert.deepEqual(expressions, { aa: 0.5 });
  // Without smoothing it snaps; past the track's vowels nothing is sent.
  assert.deepEqual(vrmData({ src, blink: false, lipSync: track, mouthSmoothing: 0 }, 29, 30).expressions, { aa: 1 });
  assert.equal(vrmData({ src, blink: false, lipSync: track }, 40, 30).expressions, undefined);
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
});

test('canvas_follows_width_height_and_aspect', () => {
  const canvas = (props: { width?: number; height?: number; aspect?: number }) =>
    vrmData({ src, ...props }, 0, 30).canvas;
  assert.deepEqual(canvas({}), [1920, 1080]);
  assert.deepEqual(canvas({ width: 1080, height: 1920 }), [1080, 1920]);
  assert.deepEqual(canvas({ height: 1920, aspect: 9 / 16 }), [1080, 1920]);
  assert.deepEqual(canvas({ width: 960 }), [960, 540]);
  assert.deepEqual(canvas({ aspect: 1 }), [1920, 1920]);
  assert.throws(() => canvas({ width: 0 }), /Vrm width, height, and aspect are positive numbers/);
  assert.throws(() => canvas({ aspect: Number.NaN }), /Vrm width, height, and aspect are positive numbers/);

  const props = vrmElementProps({ src, width: 1080, height: 1920, aspect: 9 / 16, scale: 0.5, x: 10 }, 45, 30);
  for (const key of ['width', 'height', 'aspect']) assert.ok(!(key in props), key);
  assert.equal(props.scale, 0.5);
  assert.equal(props.x, 10);
  assert.equal(props.src, src);
  assert.equal(props.time, 1.5);
  assert.deepEqual(props.data, vrmData({ src, width: 1080, height: 1920 }, 45, 30));
});

test('look_at_and_camera_pass_through', () => {
  const data = vrmData({ src, blink: false, lookAt: { yaw: 10, pitch: -5 }, camera: 'bust' }, 0, 30);
  assert.deepEqual(data, { canvas: [1920, 1080], lookAt: { yaw: 10, pitch: -5 }, camera: 'bust' });
  const explicit = { position: [0, 1.5, 2] as [number, number, number], target: [0, 1.5, 0] as [number, number, number], fov: 40 };
  assert.deepEqual(vrmData({ src, blink: false, lookAt: 'camera', camera: explicit }, 0, 30), {
    canvas: [1920, 1080],
    lookAt: 'camera',
    camera: explicit,
  });
});
