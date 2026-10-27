import assert from 'node:assert/strict';
import { expect, test } from 'vitest';
import React from 'react';
import type { ComponentType, ReactNode } from 'react';

import { Assets, Audio, Composition, FreezeFrame, Sequence, Video, frameKeyframes, useCurrentFrame } from '../src/index.js';
import type { AnimatedNumber, AudioProps, VideoProps } from '../src/index.js';
import { mount } from '../src/render.js';
import type { MountedComposition } from '../src/render.js';

import { childrenOf, contentOf } from './helpers.mjs';

const h = React.createElement;
const at = (composition: MountedComposition, frame: number) => composition.renderAt({ value: frame, timescale: 30 }, null);
const root = (durationInFrames: number, ...children: ReactNode[]) => () =>
  h(Composition, { width: 100, height: 100, fps: 30, durationInFrames }, ...children);

/** The keyframes of an animated number, checked to be keyframes. */
function keyframesOf(value: AnimatedNumber | undefined) {
  assert.ok(typeof value === 'object', 'keyframes');
  return value.keyframes;
}

test('a bare <Video> plays its soundtrack across the composition by default', () => {
  const composition = mount(root(60, h(Video, { src: './clip.mp4' })));
  expect(at(composition, 0).audio).toEqual([
    { src: './clip.mp4', sourceStart: 0, playbackRate: 1, volume: 1, muted: false, start: 0, duration: 2, video: true },
  ]);
});

test('only a <Video> marks its sound as coming from a video', () => {
  const composition = mount(root(30, h(Video, { src: './clip.mp4' }), h(Audio, { src: './music.wav' })));
  expect(at(composition, 0).audio.map((clip) => clip.video)).toEqual([true, undefined]);
});

test('<Video> volume, muted, startFrom and playbackRate reach its audio clip', () => {
  const volume = frameKeyframes([{ frame: 0, value: 0 }, { frame: 15, value: 1 }], { fps: 30 });
  const composition = mount(root(30,
    h(Video, { src: './loud.mp4', startFrom: 1, playbackRate: 2, volume }),
    h(Video, { src: './quiet.mp4', muted: true })));
  expect(at(composition, 0).audio).toEqual([
    { src: './loud.mp4', sourceStart: 1, playbackRate: 2, volume, muted: false, start: 0, duration: 1, video: true },
    { src: './quiet.mp4', sourceStart: 0, playbackRate: 1, volume: 1, muted: true, start: 0, duration: 1, video: true },
  ]);
});

test('<Video> sound follows the same sequence rules as <Audio> and stays in sync with its picture', () => {
  const tree = (Media: ComponentType<VideoProps & AudioProps>) => root(120,
    h(Sequence, { from: 30, durationInFrames: 60 },
      h(Sequence, { from: -15 }, h(Media, { src: './clip.mp4', startFrom: 1, playbackRate: 2 }))));
  const video = mount(tree(Video));
  const audio = mount(tree(Audio));
  for (const frame of [0, 30, 45, 89, 90]) {
    const sound = at(audio, frame).audio.map((clip) => ({ ...clip, video: true }));
    expect(at(video, frame).audio, `frame ${frame}`).toEqual(sound);
  }
  // The head is clipped by half a second, so sound and picture both start
  // one second (0.5 s at 2x) into the source after `startFrom`.
  const [clip] = at(video, 30).audio;
  expect(clip).toMatchObject({ sourceStart: 2, start: 1, duration: 2 });
  const layer = childrenOf(childrenOf(at(video, 30).scene.layers[0])[0])[0];
  expect(contentOf(layer, 'video').timing.sourceTimeSeconds).toBe(clip.sourceStart);
});

test('<Video> collected by the audio-only sweep matches per-frame rendering', () => {
  function Late() {
    return useCurrentFrame() >= 10 ? h(Video, { src: './late.mp4', volume: 0.5 }) : null;
  }
  const Root = root(20, h(Late), h(Sequence, { from: 5, durationInFrames: 5 }, h(Video, { src: './short.mp4' })));
  const reference = mount(Root);
  const expected = Array.from({ length: 20 }, (_, frame) => at(reference, frame).audio).flat();
  expect(expected.length).toBeGreaterThan(0);
  expect(mount(Root).collectAudio()).toEqual(expected);
});

test('<Video> inside <FreezeFrame> or <Assets> makes no sound', () => {
  const Root = root(10,
    h(FreezeFrame, { frame: 3 }, h(Video, { src: './frozen.mp4' })),
    h(Assets, null, h(Video, { src: './reference.mp4' })));
  expect(at(mount(Root), 0).audio).toEqual([]);
  expect(mount(Root).collectAudio()).toEqual([]);
});

test('loop reaches the picture and the sound of <Video>, and the sound of <Audio>', () => {
  const composition = mount(root(30,
    h(Video, { src: './clip.mp4', loop: true }),
    h(Audio, { src: './music.wav', loop: true }),
    h(Audio, { src: './voice.wav' })));
  const frame = at(composition, 15);
  expect(contentOf(frame.scene.layers[0], 'video').timing.loop).toBe(true);
  expect(frame.audio.map((clip) => clip.loop)).toEqual([true, true, undefined]);
  expect(contentOf(mount(root(30, h(Video, { src: './clip.mp4' }))).renderAt({ value: 0, timescale: 30 }, null)
    .scene.layers[0], 'video').timing).not.toHaveProperty('loop');
});

test('passes a video\'s width and height to its layer', () => {
  const composition = mount(root(30,
    h(Video, { src: './clip.mp4', width: 160 }),
    h(Video, { src: './clip.mp4', width: 160, height: 160 }),
    h(Video, { src: './clip.mp4', height: 90 }),
    h(Video, { src: './clip.mp4' })));
  const contents = at(composition, 0).scene.layers.map((layer) => contentOf(layer, 'video'));
  expect(contents.map(({ width, height }) => [width, height])).toEqual([
    [160, undefined], [160, 160], [undefined, 90], [undefined, undefined],
  ]);
  // An unset side stays out of the layer, as on an <Image>.
  expect(contents[0]).not.toHaveProperty('height');
  expect(contents[2]).not.toHaveProperty('width');
});

test('rejects a video width that is not finite and positive', () => {
  for (const width of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
    const composition = mount(root(30, h(Video, { src: './clip.mp4', width })));
    expect(() => at(composition, 0), `width ${width}`).toThrow('Video width must be finite and positive');
  }
  const composition = mount(root(30, h(Video, { src: './clip.mp4', height: 0 })));
  expect(() => at(composition, 0)).toThrow('Video height must be finite and positive');
});

test('<Audio> pitch and preservePitch reach its clip, shifted like volume', () => {
  const curve = frameKeyframes([{ frame: 0, value: 0 }, { frame: 30, value: 12 }], { fps: 30 });
  const composition = mount(root(60,
    h(Sequence, { from: -15 },
      h(Audio, { src: './a.wav', pitch: curve, preservePitch: true, playbackRate: 2 }),
      h(Audio, { src: './a.wav', volume: curve })),
    h(Audio, { src: './b.wav', pitch: -3 }),
    h(Audio, { src: './c.wav', pitch: 0 })));
  const [a, sameAsVolume, b, c] = at(composition, 0).audio;
  expect(a.preservePitch).toBe(true);
  // The head is clipped by half a second, which moves every keyframe the
  // same way it moves a volume curve's.
  expect(a.pitch).toEqual(sameAsVolume.volume);
  expect(keyframesOf(a.pitch).map((keyframe) => keyframe.time)).not.toEqual(curve.keyframes.map((keyframe) => keyframe.time));
  expect(b).toMatchObject({ pitch: -3 });
  expect(b.preservePitch).toBeUndefined();
  expect('pitch' in c).toBe(false);
  expect('preservePitch' in c).toBe(false);
});

test('<Audio> fades reach its clip, timed from the window heard', () => {
  const composition = mount(root(120,
    h(Audio, { src: './bgm.wav', volume: 0.6, fadeIn: 1.5, fadeOut: 3 }),
    // The outer window cuts the clip's head and tail; the fades stay at the
    // edges of what is heard rather than moving with the clip's own clock.
    h(Sequence, { from: 30, durationInFrames: 30 },
      h(Sequence, { from: -15 }, h(Audio, { src: './cut.wav', fadeIn: 0.25, fadeOut: 0.5 }))),
    h(Audio, { src: './plain.wav', fadeIn: 0, fadeOut: 0 })));
  const [bgm, cut, plain] = at(composition, 45).audio;
  expect(bgm).toMatchObject({ volume: 0.6, start: 0, duration: 4, fadeIn: 1.5, fadeOut: 3 });
  expect(cut).toMatchObject({ start: 1, duration: 1, fadeIn: 0.25, fadeOut: 0.5 });
  expect('fadeIn' in plain || 'fadeOut' in plain).toBe(false);
});

test('<Audio> fadeOut follows the end of its sequence', () => {
  const report = (durationInFrames: number) => at(mount(root(120,
    h(Sequence, { durationInFrames }, h(Audio, { src: './bgm.wav', fadeOut: 1 })))), 0).audio[0];
  expect(report(60)).toMatchObject({ duration: 2, fadeOut: 1 });
  expect(report(90)).toMatchObject({ duration: 3, fadeOut: 1 });
});

test('<Audio> rejects a negative or non-finite fade', () => {
  // `'1'` is invalid on purpose: a fade is a number.
  for (const props of [{ fadeIn: -1 }, { fadeOut: Number.NaN }, { fadeIn: '1' as unknown as number }]) {
    const composition = mount(root(30, h(Audio, { src: './a.wav', ...props })));
    expect(() => at(composition, 0)).toThrow(/<Audio> fade(In|Out) must be a finite number of seconds of at least 0/);
  }
});

test('<Video> never reports fades', () => {
  // <Video> takes no fades; they are passed to check that it ignores them.
  const composition = mount(root(30, h(Video, { src: './clip.mp4', fadeIn: 1, fadeOut: 1 } as VideoProps)));
  const [clip] = at(composition, 0).audio;
  expect('fadeIn' in clip || 'fadeOut' in clip).toBe(false);
});

test('<Video> never reports pitch', () => {
  // <Video> takes no pitch; it is passed to check that it ignores it.
  const composition = mount(root(30, h(Video, { src: './clip.mp4', playbackRate: 2, pitch: 4, preservePitch: true } as VideoProps)));
  const [clip] = at(composition, 0).audio;
  expect('pitch' in clip || 'preservePitch' in clip).toBe(false);
});
