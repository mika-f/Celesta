import * as React from 'react';

import { Assets, Character, Composition, Dialogue } from '@mikan/react';
import type { AssetReference } from '@mikan/react';

const character = React.createRef<AssetReference>();

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={90}>
      <Assets>
        <Character
          ref={character}
          name="Mikan"
          portrait={{
            defaultExpression: 'default',
            expressions: { default: './character.png' },
            x: 840,
            y: 120,
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
      <Dialogue character={character} audio="./voice.wav">
        React から Dialogue を表示できます。
      </Dialogue>
    </Composition>
  );
}
