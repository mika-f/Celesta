import * as React from 'react';

import { Assets, Audio, Character, CharacterView, Composition, loadLipSync } from '@mikan/react';
import type { AssetReference, LipSyncTrack } from '@mikan/react';

// 001.wav is a ~1s committed spoken fixture; enough to show the mouth
// leaving 'closed' once the voice starts.
const VOICE = '../../../examples/assets/voices/001.wav';
const NARRATION = 'あいうえお';

const chara = React.createRef<AssetReference>();
let lipSync: LipSyncTrack | null = null;

export async function prepare(): Promise<void> {
  lipSync = await loadLipSync({ src: VOICE, text: NARRATION });
}

export default function Root() {
  const track = lipSync;
  return (
    <Composition width={640} height={480} fps={30} durationInFrames={45}>
      <Assets>
        <Character
          ref={chara}
          name="lipsync-tester"
          portrait={{
            type: 'psd',
            src: '../../../examples/assets/lipsync-fixture.psd',
            layers: ['body', 'body/base'],
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
      {track ? <CharacterView character={chara} lipSync={track} /> : null}
      <Audio src={VOICE} />
    </Composition>
  );
}
