import * as React from 'react';
import {
  Assets, Character, CharacterView, Composition, Dialogue, Rect, Sequence,
} from '@celesta/react';
import type { AssetReference, CharacterViewReference } from '@celesta/react';

const mira = React.createRef<AssetReference>();
const miraView = React.createRef<CharacterViewReference>();

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={180}>
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

      <Sequence durationInFrames={90}>
        <Dialogue character={miraView} audio="./voices/line-01.wav">
          Welcome back. Shall we begin?
        </Dialogue>
      </Sequence>
      <Sequence from={90} durationInFrames={90}>
        <Dialogue character={miraView} expression="smile"
          audio="./voices/line-02.wav">
          Every scene starts with a single line.
        </Dialogue>
      </Sequence>
    </Composition>
  );
}
