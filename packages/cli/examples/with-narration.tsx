// Two characters talk through a script whose voices VOICEVOX Engine
// synthesizes during prepare(): prepareNarration() asks the engine for each
// line, keeps the WAV and its final query under ./.celesta/voices, and gives
// planDialogue() lines with `audio` and `lipSync` already set. Reopening or
// exporting the unchanged script reads the cache, so the engine is needed
// only when a line's spoken text or voice changes.
//
// Setup: start VOICEVOX Engine on port 50021 before the first preview, e.g.
//   docker run --rm -p 127.0.0.1:50021:50021 voicevox/voicevox_engine:cpu-ubuntu24.04-latest
// then open this file in Celesta, or export it:
//   celesta-exporter --react packages/cli/examples/with-narration.tsx -o narration.mp4
//
// Edit SCRIPT and VOICES below. Changing a line's `text` while it has a
// `spokenText`, its expression, the order, or a gap reuses its voice;
// changing what is spoken, a style, or a setting synthesizes that line again.
//
// Portraits: examples/assets/dialogue-demo (original characters, see its
// README.md). Voices: VOICEVOX:四国めたん, VOICEVOX:ずんだもん.
import * as React from 'react';

import { Assets, Composition, Group, Rect, Text } from '@celesta.studio/react';
import { Character, CharacterView, DialogueSeries, planDialogue } from '@celesta.studio/character';
import { prepareNarration } from '@celesta.studio/voicevox';
import type { AssetReference } from '@celesta.studio/react';
import type { CharacterSubtitle, CharacterViewReference, DialoguePlan } from '@celesta.studio/character';
import type { NarratedLine, NarrationLine, VoicevoxVoice } from '@celesta.studio/voicevox';

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;
const DEMO = '../../../examples/assets/dialogue-demo';
const ENGINE = 'http://127.0.0.1:50021';

type Speaker = 'shizuku' | 'komugi';

// Who speaks with which VOICEVOX style (`GET /speakers` lists them).
const VOICES: Record<Speaker, VoicevoxVoice> = {
  shizuku: { baseUrl: ENGINE, styleId: 2 }, // 四国めたん（ノーマル）
  komugi: { baseUrl: ENGINE, styleId: 3, query: { speedScale: 1.1 } }, // ずんだもん（ノーマル）
};

const SCRIPT: (NarrationLine & { speaker: Speaker })[] = [
  { id: 'hello', speaker: 'shizuku', text: '台本を書き換えるだけで、声が変わります。' },
  {
    id: 'hp',
    speaker: 'komugi',
    text: 'HPが1なのだ！',
    spokenText: 'エイチピーがいちなのだ！',
    expression: 'smile',
  },
  { id: 'cache', speaker: 'shizuku', text: '変えた台詞だけを、もう一度読み上げるのね？', gap: 0.5 },
  { id: 'done', speaker: 'komugi', text: 'そのとおりなのだ。', expression: 'smile' },
];

const characters = { shizuku: React.createRef<AssetReference>(), komugi: React.createRef<AssetReference>() };
const views = { shizuku: React.createRef<CharacterViewReference>(), komugi: React.createRef<CharacterViewReference>() };

let plan: DialoguePlan<NarratedLine<(typeof SCRIPT)[number]>>;

export async function prepare(): Promise<void> {
  const lines = await prepareNarration(SCRIPT, { speakers: VOICES, cacheDir: './.celesta/voices' });
  plan = await planDialogue(lines, { fps: FPS, gap: 0.3 });
}

const SUBTITLE: CharacterSubtitle = {
  x: WIDTH / 2,
  y: 600,
  anchorX: 0.5,
  maxWidth: 1080,
  style: {
    fontSize: 40,
    fontWeight: 700,
    align: 'center',
    fill: { type: 'solid', color: '#ffffff' },
    stroke: { paint: { type: 'solid', color: '#2b2340' }, width: 8 },
  },
};

function mouths() {
  const mouth = (shape: string) => `${DEMO}/portraits/mouth-${shape}.png`;
  return { a: mouth('a'), i: mouth('i'), u: mouth('u'), e: mouth('e'), o: mouth('o'), closed: mouth('closed') };
}

function portrait(name: Speaker) {
  return {
    defaultExpression: 'normal',
    expressions: { normal: `${DEMO}/portraits/${name}-normal.png`, smile: `${DEMO}/portraits/${name}-smile.png` },
    lipSync: mouths(),
  };
}

export default function Root() {
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={plan.durationInFrames}>
      <Assets>
        <Character ref={characters.shizuku} name="しずく" portrait={portrait('shizuku')} subtitle={SUBTITLE} />
        <Character ref={characters.komugi} name="こむぎ" portrait={portrait('komugi')} subtitle={SUBTITLE} />
      </Assets>
      <Rect width={WIDTH} height={HEIGHT} fill="#eaf4ff" />
      <Group x={190} y={90}>
        <CharacterView ref={views.shizuku} character={characters.shizuku} mouth="closed" />
      </Group>
      <Group x={730} y={90}>
        <CharacterView ref={views.komugi} character={characters.komugi} mouth="closed" />
      </Group>
      <DialogueSeries plan={plan} views={views} />
      <Text x={WIDTH - 24} y={HEIGHT - 20} anchorX={1} anchorY={1}
        style={{ fontSize: 16, fill: { type: 'solid', color: '#4a4560' } }}>
        VOICEVOX:四国めたん　VOICEVOX:ずんだもん
      </Text>
    </Composition>
  );
}
