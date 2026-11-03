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
    tracks: { head: { yaw: [{ frame: 0, value: 0 }] } },
  };
  const element = vrmElementProps(props, 45, 30);
  assert.deepEqual(Object.keys(element).sort(), ['data', 'id', 'scale', 'src', 'time', 'x']);
  // Motions add their clips, by asset name, for the layer to resolve.
  const moving = vrmElementProps({ ...props, motions: [{ src: './idle.vrma' }] }, 45, 30);
  assert.deepEqual(Object.keys(moving).sort(), ['clips', 'data', 'id', 'scale', 'src', 'time', 'x']);
  assert.deepEqual(moving.clips, { motion0: './idle.vrma' });
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

test('motions_name_their_clips', () => {
  const data = vrmData(
    {
      src,
      blink: false,
      motions: [
        { src: './a.vrma', loop: true },
        { src: './a.vrma', from: 30, fadeIn: 15 },
        { src: './b.vrma', from: 60 },
      ],
    },
    0,
    30,
  );
  assert.deepEqual(data.motions, [
    { clip: 'motion0', from: 0, loop: true },
    { clip: 'motion0', from: 1, fadeIn: 0.5 },
    { clip: 'motion2', from: 2 },
  ]);
  // No motions, no key.
  assert.ok(!('motions' in vrmData({ src, motions: [] }, 0, 30)));
});

test('motion_times_are_numbers_of_frames', () => {
  const motion = (extra: { from?: number; fadeIn?: number }) => () =>
    vrmData({ src, motions: [{ src: './a.vrma', ...extra }] }, 0, 30);
  // A motion may start before the sequence does.
  assert.equal(motion({ from: -30 })().motions?.[0].from, -1);
  for (const from of [Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.throws(motion({ from }), { message: 'Vrm motion 0 from is a finite number of frames' }, `${from}`);
  }
  for (const fadeIn of [Number.NaN, Number.POSITIVE_INFINITY, -1]) {
    assert.throws(motion({ fadeIn }), { message: 'Vrm motion 0 fadeIn is a number of frames, 0 or more' }, `${fadeIn}`);
  }
});

test('tracks_are_keyframes_on_the_layer_clock', () => {
  const tracks = {
    head: {
      yaw: [
        { frame: 30, value: 20, easing: 'ease-in-out' as const },
        { frame: 0, value: 0 },
      ],
    },
  };
  assert.deepEqual(vrmData({ src, blink: false, tracks }, 0, 30).tracks, {
    head: {
      yaw: {
        type: 'keyframes',
        keyframes: [
          { time: { value: 0, timescale: 30 }, value: 0 },
          { time: { value: 30, timescale: 30 }, value: 20, easing: 'ease-in-out' },
        ],
      },
    },
  });
  // Rates that are not whole time keyframes to the microsecond.
  const yaw = vrmData({ src, tracks }, 0, 29.97).tracks?.head?.yaw;
  assert.deepEqual(
    yaw?.keyframes.map((keyframe) => keyframe.time),
    [
      { value: 0, timescale: 1_000_000 },
      { value: 1_001_001, timescale: 1_000_000 },
    ],
  );
  // A fractional frame is the nearest frame at any rate, as a whole rate
  // times it: at 30 fps and at 29.97 fps alike.
  const fractional = { head: { yaw: [{ frame: 10.4, value: 0 }, { frame: 20.6, value: 1 }] } };
  const times = (fps: number) =>
    vrmData({ src, tracks: fractional }, 0, fps).tracks?.head?.yaw?.keyframes.map((keyframe) => keyframe.time);
  assert.deepEqual(times(30), [
    { value: 10, timescale: 30 },
    { value: 21, timescale: 30 },
  ]);
  assert.deepEqual(times(29.97), [
    { value: Math.round((10 * 1_000_000) / 29.97), timescale: 1_000_000 },
    { value: Math.round((21 * 1_000_000) / 29.97), timescale: 1_000_000 },
  ]);
  // The hips move too; no tracks, no key.
  const hips = vrmData({ src, tracks: { hips: { y: [{ frame: 0, value: 0.1 }] } } }, 0, 30).tracks;
  assert.deepEqual(hips, {
    hips: { y: { type: 'keyframes', keyframes: [{ time: { value: 0, timescale: 30 }, value: 0.1 }] } },
  });
  assert.ok(!('tracks' in vrmData({ src, tracks: {} }, 0, 30)));
});

test('bad_tracks_throw', () => {
  const data = (tracks: unknown) => () => vrmData({ src, tracks: tracks as VrmProps['tracks'] }, 0, 30);
  assert.throws(data({ tail: { yaw: [{ frame: 0, value: 0 }] } }), {
    message: 'Vrm tracks name "tail", which is not a VRM humanoid bone',
  });
  assert.throws(data({ head: { x: [{ frame: 0, value: 0 }] } }), {
    message: 'Vrm tracks: only hips takes x, y, and z (head has x)',
  });
  assert.throws(data({ head: { rotation: [{ frame: 0, value: 0 }] } }), {
    message: 'Vrm tracks: head has "rotation"; a track is pitch, yaw, roll, x, y, or z',
  });
  assert.throws(data({ head: { yaw: [] } }), { message: 'Vrm tracks: head yaw has no keyframes' });
  assert.throws(data({ head: { yaw: [{ frame: Number.NaN, value: 0 }] } }), {
    message: 'Vrm tracks: head yaw has a keyframe whose frame or value is not a finite number',
  });
  assert.throws(data({ head: { yaw: [{ frame: 0, value: Number.POSITIVE_INFINITY }] } }), {
    message: 'Vrm tracks: head yaw has a keyframe whose frame or value is not a finite number',
  });
});

test('the_layer_opens_each_clip_once', () => {
  const h = React.createElement;
  const mounted = mount(() =>
    h(
      Composition,
      { width: 1920, height: 1080, fps: 30, durationInFrames: 90 },
      h(Vrm, {
        src: './a.vrm',
        blink: false,
        motions: [{ src: './idle.vrma', loop: true }, { src: './idle.vrma', from: 30 }, { src: './wave.vrma', from: 60 }],
      }),
    ),
  );
  const { scene } = mounted.renderAt({ value: 0, timescale: 30 }, null);
  const content = scene.layers[0].content as unknown as { assets: unknown; data: { motions: unknown } };
  assert.deepEqual(content.assets, {
    model: { id: './a.vrm', location: { type: 'file', path: './a.vrm' } },
    motion0: { id: './idle.vrma', location: { type: 'file', path: './idle.vrma' } },
    motion2: { id: './wave.vrma', location: { type: 'file', path: './wave.vrma' } },
  });
  assert.deepEqual(content.data.motions, [
    { clip: 'motion0', from: 0, loop: true },
    { clip: 'motion0', from: 1 },
    { clip: 'motion2', from: 2 },
  ]);
});
