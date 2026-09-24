import * as React from 'react';

import { Assets, Character, CharacterView, Composition } from '@celesta/react';
import type { AssetReference } from '@celesta/react';

const chara = React.createRef<AssetReference>();

// A raw PSDTool "all layer" state string, exactly what "copy layer state"
// produces — pasted straight into `layers`. render.ts parses it into the
// visible-layer list the PSD content carries.
const POSE = ['/body', '/body/base', '/body/outfit-navy', '/face', '/face/eyes', '/face/eyes/open'].join('\n');

export default function Root() {
  return (
    <Composition width={640} height={480} fps={30} durationInFrames={30}>
      <Assets>
        <Character
          ref={chara}
          name="preset-tester"
          portrait={{
            type: 'psd',
            src: '../../../examples/assets/lipsync-fixture.psd',
            layers: POSE,
            lipSync: {
              a: 'face/mouth/a',
              i: 'face/mouth/i',
              u: 'face/mouth/u',
              e: 'face/mouth/e',
              o: 'face/mouth/o',
              closed: 'face/mouth/closed',
            },
          }}
        />
      </Assets>
      <CharacterView character={chara} mouth="a" />
    </Composition>
  );
}
