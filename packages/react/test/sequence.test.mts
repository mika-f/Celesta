import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';

import { Audio, Composition, Sequence, Series, Text, useCurrentFrame, useVideoConfig } from '../src/index.js';
import type { Layer } from '../src/index.js';
import { mount } from '../src/render.js';
import type { MountedComposition } from '../src/render.js';

const h = React.createElement;
const at = (composition: MountedComposition, frame: number) => composition.renderAt({ value: frame, timescale: 30 }, null);
const texts = (layers: Layer[]): string[] => layers.flatMap((layer) =>
  layer.content.type === 'text' ? [layer.content.text] : texts(layer.content.type === 'group' ? layer.content.layers : []),
);

test('Sequence never evaluates children outside its window, including after seeking', () => {
  const seen: number[] = [];
  function Later() {
    const frame = useCurrentFrame();
    const { durationInFrames } = useVideoConfig();
    assert.ok(frame >= 0 && frame < durationInFrames, `inactive local frame ${frame}`);
    seen.push(frame);
    return h(Text, null, '.'.repeat(frame % 3));
  }
  const composition = mount(() => h(Composition, { width: 1920, height: 1080, fps: 30, durationInFrames: 120 },
    h(Sequence, { from: 60, durationInFrames: 60 }, h(Later)),
  ));
  assert.deepEqual(seen, [], 'inactive children also stay unmounted during config discovery');
  for (const frame of [0, 10, 59, 60, 70, 119, 120, 10, 70]) {
    const { scene, audio } = at(composition, frame);
    assert.deepEqual(texts(scene.layers), frame >= 60 && frame < 120 ? ['.'.repeat((frame - 60) % 3)] : []);
    assert.deepEqual(audio, []);
  }
  assert.deepEqual(seen, [0, 10, 59, 10]);
});

test('nested sequences retain default durations, clipped audio, and conditional hooks', () => {
  const seen: string[] = [];
  function Sound({ name }: { name: string }) {
    const frame = useCurrentFrame();
    const { durationInFrames } = useVideoConfig();
    const [src] = React.useState(`${name}.wav`);
    assert.ok(frame >= 0 && frame < durationInFrames);
    seen.push(`${name}:${frame}:${durationInFrames}`);
    return frame % 2 === 0 ? h(Audio, { src, startFrom: 1 }) : null;
  }
  const composition = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 120 },
    h(Sequence, { from: 30, durationInFrames: 60 },
      h(Sequence, { from: -15 }, h(Sound, { name: 'early' })),
      h(Sequence, { from: 15 }, h(Sound, { name: 'late' })),
    ),
  ));
  for (const frame of [0, 29, 30, 31, 44, 45, 46, 89, 90]) {
    const { audio } = at(composition, frame);
    const expected = [];
    if (frame >= 30 && frame < 90 && (frame - 15) % 2 === 0) {
      expected.push({ src: 'early.wav', sourceStart: 1.5, playbackRate: 1, volume: 1, muted: false, start: 1, duration: 2 });
    }
    if (frame >= 45 && frame < 90 && (frame - 45) % 2 === 0) {
      expected.push({ src: 'late.wav', sourceStart: 1, playbackRate: 1, volume: 1, muted: false, start: 1.5, duration: 1.5 });
    }
    assert.deepEqual(audio, expected, `audio at frame ${frame}`);
  }
  assert.deepEqual(seen, [
    'early:15:75', 'early:16:75', 'early:29:75', 'early:30:75', 'late:0:45',
    'early:31:75', 'late:1:45', 'early:74:75', 'late:44:45',
  ]);
});

test('Series evaluates only active scenes and keeps audio at overlapping boundaries', () => {
  const seen: string[] = [];
  function Scene({ name }: { name: string }) {
    const frame = useCurrentFrame();
    const { durationInFrames } = useVideoConfig();
    assert.ok(frame >= 0 && frame < durationInFrames);
    seen.push(`${name}:${frame}`);
    return h(React.Fragment, null, h(Text, null, `${name}:${frame}`), h(Audio, { src: `${name}.wav` }));
  }
  const composition = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 120 },
    h(Series, null,
      h(Series.Sequence, { durationInFrames: 60 }, h(Scene, { name: 'a' })),
      h(Series.Sequence, { durationInFrames: 60, offset: -30 }, h(Scene, { name: 'b' })),
    ),
  ));
  seen.length = 0;
  for (const frame of [0, 29, 30, 59, 60, 89, 90, 0]) {
    const { scene, audio } = at(composition, frame);
    const expected = [];
    if (frame < 60) expected.push(`a:${frame}`);
    if (frame >= 30 && frame < 90) expected.push(`b:${frame - 30}`);
    assert.deepEqual(texts(scene.layers), expected);
    assert.deepEqual(audio, expected.map((label) => ({
      src: `${label[0]}.wav`, sourceStart: 0, playbackRate: 1, volume: 1, muted: false,
      start: label[0] === 'a' ? 0 : 1, duration: 2,
    })));
    assert.deepEqual(seen.splice(0), expected, `evaluated scenes at frame ${frame}`);
  }
});

test('openEnd keeps windows that reach the composition end open past the last frame', () => {
  function Clock({ name }: { name: string }) {
    const frame = useCurrentFrame();
    const { durationInFrames } = useVideoConfig();
    return h(Text, null, `${name}:${frame}/${durationInFrames}`);
  }
  const composition = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 120 },
    h(Clock, { name: 'root' }),
    h(Sequence, { from: 30 },
      h(Clock, { name: 'open' }),
      h(Sequence, { from: 10, durationInFrames: 80 }, h(Clock, { name: 'nested' })),
      h(Sequence, { from: 10, durationInFrames: 79 }, h(Clock, { name: 'nested-early' })),
    ),
    h(Sequence, { from: 0, durationInFrames: 119 }, h(Clock, { name: 'early' })),
    h(Series, null,
      h(Series.Sequence, { durationInFrames: 60 }, h(Clock, { name: 'a' })),
      h(Series.Sequence, { durationInFrames: 60 }, h(Clock, { name: 'b' })),
    ),
    // Ends after the composition does: it keeps its own end.
    h(Sequence, { from: 110, durationInFrames: 11 },
      h(Clock, { name: 'overrun' }),
      h(Sequence, { from: 5 }, h(Clock, { name: 'overrun-child' })),
    ),
  ));
  const past = (frame: number) => texts(composition.renderAt({ value: frame, timescale: 30 }, null, { openEnd: true }).scene.layers);
  assert.deepEqual(texts(at(composition, 120).scene.layers), ['root:120/120', 'overrun:10/11', 'overrun-child:5/6']);
  // useVideoConfig() still reports the real durations, so loop math is unchanged.
  assert.deepEqual(past(120), ['root:120/120', 'open:90/90', 'nested:80/80', 'b:60/60', 'overrun:10/11', 'overrun-child:5/6']);
  assert.deepEqual(past(121), ['root:121/120', 'open:91/90', 'nested:81/80', 'b:61/60']);
  // Inside the composition, openEnd changes nothing.
  assert.deepEqual(past(118), texts(at(composition, 118).scene.layers));
  assert.deepEqual(texts(at(composition, 120).scene.layers), ['root:120/120', 'overrun:10/11', 'overrun-child:5/6']);
});

test('a nested sequence opens on its first frame although the shifted time is rounded', () => {
  // Inside the outer sequence, frame 10 at 30 fps is 0.333333 s, just
  // under 10 / 30.
  const composition = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 60 },
    h(Sequence, { from: 0 }, h(Sequence, { from: 10, durationInFrames: 10 }, h(() => h(Text, null, String(useCurrentFrame()))))),
  ));
  for (const frame of [9, 10, 11, 19, 20]) {
    assert.deepEqual(texts(at(composition, frame).scene.layers), frame >= 10 && frame < 20 ? [String(frame - 10)] : [], `frame ${frame}`);
  }
});

test('deeply nested sequences open on the same frame as one flattened sequence', () => {
  // Each level rounds its shifted time to a microsecond, so four levels
  // land below 2 / 30 at frame 45.
  const P = () => h(Text, null, String(useCurrentFrame()));
  const nested = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 80 },
    h(Sequence, { from: 20 }, h(Sequence, { from: 20, durationInFrames: 30 }, h(Sequence, { from: 3 }, h(Sequence, { from: 2 }, h(P))))),
  ));
  const flattened = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 80 },
    h(Sequence, { from: 40, durationInFrames: 30 }, h(Sequence, { from: 3 }, h(Sequence, { from: 2 }, h(P)))),
  ));
  for (const frame of [44, 45, 46, 69, 70]) {
    const expected = frame >= 45 && frame < 70 ? [String(frame - 45)] : [];
    assert.deepEqual(texts(at(nested, frame).scene.layers), expected, `nested at frame ${frame}`);
    assert.deepEqual(texts(at(flattened, frame).scene.layers), expected, `flattened at frame ${frame}`);
  }
});

test('Sequence rejects a from or durationInFrames it cannot place', () => {
  // The non-number values are invalid on purpose.
  const invalid: [Record<string, unknown>, RegExp][] = [
    [{ from: Number.NaN }, /<Sequence> `from` must be a finite number of frames, not NaN/],
    [{ from: Infinity }, /<Sequence> `from` must be a finite number of frames, not Infinity/],
    [{ from: '10' }, /<Sequence> `from` must be a finite number of frames, not "10"/],
    [{ durationInFrames: Number.NaN }, /<Sequence> `durationInFrames` must be a number of frames of at least 0, or Infinity, not NaN/],
    [{ durationInFrames: -1 }, /<Sequence> `durationInFrames` must be a number of frames of at least 0, or Infinity, not -1/],
    [{ durationInFrames: '10' }, /<Sequence> `durationInFrames` must be a number of frames of at least 0, or Infinity, not "10"/],
  ];
  for (const [props, message] of invalid) {
    assert.throws(() => at(mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 60 },
      h(Sequence, props, h(Text, null, 'shown')))), 0), message, JSON.stringify(props));
  }
});

test('Sequence durationInFrames undefined or Infinity runs to the end of its parent, and 0 is empty', () => {
  function Clock() {
    const frame = useCurrentFrame();
    const { durationInFrames } = useVideoConfig();
    return h(Text, null, `${frame}/${durationInFrames}`);
  }
  for (const durationInFrames of [undefined, Infinity]) {
    const composition = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 120 },
      h(Sequence, { from: 20, durationInFrames: 60 }, h(Sequence, { from: 10, durationInFrames }, h(Clock))),
    ));
    for (const frame of [29, 30, 79, 80]) {
      const shown = frame >= 30 && frame < 80 ? [`${frame - 30}/50`] : [];
      assert.deepEqual(texts(at(composition, frame).scene.layers), shown, `${durationInFrames} at frame ${frame}`);
    }
  }
  const empty = mount(() => h(Composition, { width: 10, height: 10, fps: 30, durationInFrames: 60 },
    h(Sequence, { from: 10, durationInFrames: 0 }, h(Clock)),
  ));
  for (const frame of [0, 9, 10, 11, 59]) {
    assert.deepEqual(texts(at(empty, frame).scene.layers), [], `frame ${frame}`);
  }
});
