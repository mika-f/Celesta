// <MotionBlur> and <Trail> with the packages that keep state per copy:
// portraits are scoped to each sample, lip sync and blinking show the
// frame's moment, and project layers refuse to be drawn at another moment.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import * as React from 'react';
import type { ComponentType, ReactNode } from 'react';

import { Assets, Composition, Group, MotionBlur, Sequence, Trail } from '@celesta.studio/react';
import type { AssetReference, Layer } from '@celesta.studio/react';
import { Character, CharacterView, Dialogue, blinkPhase } from '@celesta.studio/character';
import type { CharacterViewReference, ImageCharacterPortrait, LipSyncTrack } from '@celesta.studio/character';
import { ProjectTimeline, ProjectTrack } from '@celesta.studio/project';
import { mount } from '@celesta.studio/react/internal';
import type { MountedComposition } from '@celesta.studio/react/internal';
import { contentOf } from './layer-content.mjs';

const h = React.createElement;
const FPS = 30;
const at = (composition: MountedComposition, frame: number) => composition.renderAt({ value: frame, timescale: FPS }, null);
const root = (...children: ReactNode[]) => h(Composition, { width: 10, height: 10, fps: FPS, durationInFrames: 120 }, ...children);

function images(layers: Layer[]): string[] {
  return layers.flatMap((layer) =>
    layer.content.type === 'image' ? [layer.content.asset.id] : layer.content.type === 'group' ? images(layer.content.layers) : [],
  );
}

function portrait(name: string, extra: Partial<ImageCharacterPortrait> = {}): ImageCharacterPortrait {
  return {
    defaultExpression: 'calm',
    expressions: { calm: `${name}-calm.png`, smile: `${name}-smile.png` },
    ...extra,
  };
}

test('project layers, evaluated at the current frame only, throw inside a motion blur or a trail copy', () => {
  for (const Wrapper of [MotionBlur, Trail]) {
    for (const [element, name] of [[h(ProjectTimeline), '<ProjectTimeline />'], [h(ProjectTrack, { id: 'main' }), '<ProjectTrack />']]) {
      assert.throws(
        () => at(mount(() => root(h(Wrapper, null, element))), 10),
        new RegExp(`${name} cannot be drawn inside <FreezeFrame>, <MotionBlur>, or <Trail>`),
        `${Wrapper.name} ${name}`,
      );
    }
  }
});

test('every sample of a view and its line shows what the frame shows, and the line plays once', () => {
  const timing = { interval: 1, duration: 0.2 };
  let blinkFrame = 30;
  while (blinkPhase(blinkFrame, FPS, { seed: 'mira', ...timing }) !== 'closed') blinkFrame += 1;
  const track: LipSyncTrack = {
    durationInSeconds: 10,
    mouthAtSeconds: (seconds) => (['a', 'i', 'u', 'e', 'o'] as const)[Math.round(seconds * FPS) % 5],
    mouthAtFrame: (frame) => (['a', 'i', 'u', 'e', 'o'] as const)[frame % 5],
  };
  const lipSync = { a: 'a.png', i: 'i.png', u: 'u.png', e: 'e.png', o: 'o.png', closed: 'closed.png' };
  const blink = { ...timing, closed: { calm: 'calm-closed.png' } };
  const compose = <P extends object>(Wrapper: ComponentType<P>, props: P | null) => {
    const mira = React.createRef<AssetReference>();
    const view = React.createRef<CharacterViewReference>();
    const World = () => h(Sequence, { from: 30 },
      h(CharacterView, { ref: view, character: mira }),
      h(Dialogue, { character: view, lipSync: track, audio: 'line.wav', children: 'line' }),
    );
    return mount(() => root(
      h(Assets, null, h(Character, { ref: mira, name: 'mira', portrait: portrait('mira', { lipSync, blink }) })),
      h(Wrapper, props, h(World)),
    ));
  };
  const live = compose(Group, null);
  const blurred = compose(MotionBlur, { samples: 4, shutterAngle: 360 });
  for (const frame of [blinkFrame, blinkFrame + 1, blinkFrame + 2]) {
    const original = images(at(live, frame).scene.layers);
    const { scene, audio } = at(blurred, frame);
    const samples = contentOf(scene.layers.at(-1), 'group').layers;
    assert.equal(samples.length, 4);
    for (const sample of samples) {
      assert.deepEqual(images([sample]), original, `frame ${frame}`);
    }
    assert.deepEqual(audio.map((clip) => clip.src), at(live, frame).audio.map((clip) => clip.src));
  }
});
