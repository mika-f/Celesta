import * as React from 'react';

import { Assets, Character, CharacterView, Composition } from '@celesta/react';
import type { AssetReference } from '@celesta/react';

const teto = React.createRef<AssetReference>();

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={30} durationInFrames={30}>
      <Assets>
        <Character
          ref={teto}
          name="重音テト"
          portrait={{
            type: 'psd',
            src: './teto.psd',
            lipSync: {
              a: '本体/顔パーツ/口/あいうえお/あ',
              i: '本体/顔パーツ/口/あいうえお/い',
              u: '本体/顔パーツ/口/あいうえお/う',
              e: '本体/顔パーツ/口/あいうえお/え',
              o: '本体/顔パーツ/口/あいうえお/お',
              closed: '本体/顔パーツ/口/ん',
            },
          }}
        />
      </Assets>
      <CharacterView character={teto} mouth="a" />
    </Composition>
  );
}
