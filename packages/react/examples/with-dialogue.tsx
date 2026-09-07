import * as React from 'react';

import { Assets, Character, CharacterView, Composition, Dialogue } from '@mikan/react';
import type { AssetReference, CharacterViewReference } from '@mikan/react';

const character = React.createRef<AssetReference>();
const view = React.createRef<CharacterViewReference>();

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={90}>
      <Assets>
        <Character
          ref={character}
          name="Frameweave"
          portrait={{
            defaultExpression: 'default',
            expressions: { default: './character.png' },
          }}
          subtitle={{
            x: 640,
            y: 640,
            anchorX: 0.5,
            style: { fontSize: 48, fill: { type: 'solid', color: '#FFFFFF' } },
            maxWidth: 1120,
          }}
        />
      </Assets>
      <CharacterView ref={view} character={character} x={840} y={120} />
      <Dialogue character={view} audio="./voice.wav">
        React から Dialogue を表示できます。
      </Dialogue>
    </Composition>
  );
}
