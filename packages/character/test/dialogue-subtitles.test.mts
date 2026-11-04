// What `<Dialogue>` lines report for the exporter's subtitle file.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';
import type { ReactElement, ReactNode } from 'react';

import { Assets, Composition, FreezeFrame, Sequence, Span } from '@celesta.studio/react';
import type { AssetReference } from '@celesta.studio/react';
import { Character, CharacterView, Dialogue, DialogueSeries, planDialogue } from '../src/index.js';
import type { CharacterPortrait, CharacterViewReference, DialogueProps } from '../src/index.js';
import { mount } from '@celesta.studio/react/internal';
import type { SubtitleDescriptor } from '@celesta.studio/react/internal';

const h = React.createElement;

const portrait: CharacterPortrait = { defaultExpression: 'calm', expressions: { calm: 'calm.png' } };

// The line goes in as createElement's children, as JSX would pass it; they
// replace the `children: undefined` that DialogueProps requires.
const dialogue = (props: Omit<DialogueProps, 'children'>, ...children: ReactNode[]) =>
  h(Dialogue, { ...props, children: undefined }, ...children);

/** Every distinct subtitle reported over `frames` frames at 30 fps, in first-seen order. */
function subtitles(Root: () => ReactElement, frames: number) {
  const mounted = mount(Root);
  const seen = new Map<string, SubtitleDescriptor>();
  for (let frame = 0; frame < frames; frame++) {
    for (const cue of mounted.renderAt({ value: frame, timescale: 30 }, null).subtitles) {
      seen.set(JSON.stringify(cue), cue);
    }
  }
  return [...seen.values()].map(({ text, start, end }) => [text, Math.round(start * 30), Math.round(end * 30)]);
}

test('each dialogue line is a subtitle for as long as its sequence, held lines are not', async () => {
  const plan = await planDialogue(
    [
      { id: 'one', text: 'First', durationInFrames: 10, gap: 0.2, speaker: 'mira' },
      { id: 'two', text: 'Second', durationInFrames: 10, gap: 0.2, speaker: 'mira' },
    ],
    { fps: 30 },
  );
  const mira = React.createRef<AssetReference>();
  const view = React.createRef<CharacterViewReference>();
  const Root = () =>
    h(
      Composition,
      { width: 640, height: 360, fps: 30, durationInFrames: plan.durationInFrames },
      h(Assets, null, h(Character, { ref: mira, name: 'mira', portrait })),
      h(CharacterView, { ref: view, character: mira }),
      h(DialogueSeries, { plan, views: { mira: view }, holdSubtitle: true }),
    );
  // one: 0–10, gap to 16; two: 16–26. The held gaps report nothing.
  assert.deepEqual(subtitles(Root, plan.durationInFrames), [['First', 0, 10], ['Second', 16, 26]]);
});

test('a dialogue line reports its spans as plain text, and nothing inside a freeze frame', () => {
  const mira = React.createRef<AssetReference>();
  const view = React.createRef<CharacterViewReference>();
  const Root = () =>
    h(
      Composition,
      { width: 640, height: 360, fps: 30, durationInFrames: 20 },
      h(Assets, null, h(Character, { ref: mira, name: 'mira', portrait })),
      h(CharacterView, { ref: view, character: mira }),
      h(Sequence, { from: 5, durationInFrames: 10 },
        dialogue({ character: view }, 'Hello ', h(Span, { style: { fill: '#ff0000' } }, 'world'))),
      h(FreezeFrame, { frame: 7 },
        h(CharacterView, { ref: view, character: mira }),
        dialogue({ character: view }, 'Frozen')),
    );
  assert.deepEqual(subtitles(Root, 20), [['Hello world', 5, 15]]);
});
