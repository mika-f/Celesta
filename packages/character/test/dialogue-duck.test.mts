// Dialogue voices mark where speech is for ducking: their clips belong to
// "voice" unless a line says otherwise, and they never duck themselves.

import assert from 'node:assert/strict';
import { test } from 'vitest';
import React from 'react';

import { Assets, Composition } from '@celesta.studio/react';
import type { AssetReference, TextMetrics } from '@celesta.studio/react';
import { mount, setTextMeasurer } from '@celesta.studio/react/internal';
import type { AudioClipDescriptor, MeasureTextRequest } from '@celesta.studio/react/internal';
import { Character, CharacterView, Dialogue, DialogueSeries, planDialogue } from '../src/index.js';
import type { CharacterViewReference, DialogueLine, DialogueProps } from '../src/index.js';

const h = React.createElement;

const measure = ({ text }: MeasureTextRequest): TextMetrics => ({
  width: text.length * 10, height: 12.5, ascent: 10, descent: 2.5, lineHeight: 12.5, capHeight: 7, xHeight: 5,
  lines: 1, glyphs: [], lineStarts: [0],
});
setTextMeasurer(async (request) => measure(request), measure);

const portrait = {
  defaultExpression: 'n', expressions: { n: 'n.png' },
  lipSync: { a: 'a.png', i: 'i.png', u: 'u.png', e: 'e.png', o: 'o.png', closed: 'c.png' },
};

/** The voice clips of frame 0, with the characters and views a line needs. */
function voices(line: (view: React.RefObject<CharacterViewReference | null>) => React.ReactNode): AudioClipDescriptor[] {
  const actor = React.createRef<AssetReference>();
  const view = React.createRef<CharacterViewReference>();
  const Root = () => h(Composition, { width: 64, height: 36, fps: 30, durationInFrames: 30 },
    h(Assets, null, h(Character, { ref: actor, name: 'zunda', portrait })),
    h(CharacterView, { ref: view, character: actor }),
    line(view));
  return mount(Root).renderAt({ value: 0, timescale: 30 }, null).audio;
}

function dialogue(props: Partial<DialogueProps>) {
  return voices((view) => h(Dialogue, { character: view, audio: './v1.wav', ...props, children: 'hi' }))[0];
}

test('a voice belongs to "voice" unless the line says otherwise', () => {
  assert.equal(dialogue({}).duckGroup, 'voice');
  assert.equal(dialogue({ duckGroup: 'narrator' }).duckGroup, 'narrator');
  assert.ok(!('duckGroup' in dialogue({ duckGroup: null })));
});

test('a voice never ducks', () => {
  // Deliberately passed although `DialogueProps` has no `duck`.
  const props = { duck: { under: 'voice' } } as Partial<DialogueProps>;
  assert.ok(!('duck' in dialogue(props)));
});

test('a DialogueSeries line passes its duckGroup on', async () => {
  const lines: DialogueLine[] = [
    { speaker: 'zunda', text: 'a', durationInFrames: 10, audio: './a.wav', duckGroup: 'narrator' },
    { speaker: 'zunda', text: 'b', durationInFrames: 10, audio: './b.wav' },
  ];
  const plan = await planDialogue(lines, { fps: 30, gap: 0 });
  const actor = React.createRef<AssetReference>();
  const view = React.createRef<CharacterViewReference>();
  const Root = () => h(Composition, { width: 64, height: 36, fps: 30, durationInFrames: 20 },
    h(Assets, null, h(Character, { ref: actor, name: 'zunda', portrait })),
    h(CharacterView, { ref: view, character: actor }),
    h(DialogueSeries, { plan, views: { zunda: view } }));
  const mounted = mount(Root);
  const at = (frame: number) => mounted.renderAt({ value: frame, timescale: 30 }, null).audio[0].duckGroup;
  assert.equal(at(0), 'narrator');
  assert.equal(at(15), 'voice');
});
