import * as React from 'react';
import {
  Assets, Character, CharacterView, Composition, DialogueSeries, Rect, planDialogue,
} from '@celesta/react';
import type { AssetReference, CharacterViewReference, DialoguePlan } from '@celesta/react';

const mira = React.createRef<AssetReference>();
const miraView = React.createRef<CharacterViewReference>();

let plan: DialoguePlan;

export async function prepare() {
  plan = await planDialogue([
    { id: 'hello', speaker: 'mira', text: 'Welcome back. Shall we begin?',
      audio: './voices/line-01.wav' },
    { id: 'start', speaker: 'mira', text: 'Every scene starts with a single line.',
      audio: './voices/line-02.wav', expression: 'smile' },
  ], { fps: 30, gap: 0.25 });
}

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={plan.durationInFrames}>
      <Rect width={1920} height={1080} fill="#20243a" />
      <Assets>
        <Character
          ref={mira}
          name="Mira"
          portrait={{
            defaultExpression: 'calm',
            expressions: {
              calm: './mira/calm.png',
              smile: './mira/smile.png',
            },
          }}
          subtitle={{
            x: 960, y: 940, anchorX: 0.5, anchorY: 0.5, maxWidth: 1600,
            style: {
              fontSize: 56,
              fill: { type: 'solid', color: '#ffffff' },
              stroke: { paint: { type: 'solid', color: '#3a2d52' }, width: 6 },
              align: 'center',
            },
          }}
        />
      </Assets>

      <CharacterView ref={miraView} character={mira} x={1400} y={200} />

      <DialogueSeries plan={plan} views={{ mira: miraView }} holdThroughGap />
    </Composition>
  );
}
