import * as React from 'react';
import {
  Assets, Character, CharacterView, Composition, Dialogue,
  loadLipSync, loadPsdPreset,
} from '@celesta/react';
import type {
  AssetReference, CharacterViewReference, LipSyncTrack,
} from '@celesta/react';

const FPS = 30;
const VOICE = './voices/hello.wav';
const LINE = 'こんにちは、はじめまして！';

const hana = React.createRef<AssetReference>();
const hanaView = React.createRef<CharacterViewReference>();

// Filled once by prepare(), before the first frame renders.
let pose: string[] = [];
let voice: LipSyncTrack | null = null;

export async function prepare() {
  pose = await loadPsdPreset({ src: './hana/hana.pfv', favorite: 'smile' });
  voice = await loadLipSync({ src: VOICE, text: LINE });
}

export default function Root() {
  const seconds = (voice?.durationInSeconds ?? 2) + 0.5;
  return (
    <Composition width={1920} height={1080} fps={FPS}
      durationInFrames={Math.ceil(seconds * FPS)}>
      <Assets>
        <Character
          ref={hana}
          name="Hana"
          portrait={{
            type: 'psd',
            src: './hana/hana.psd',
            layers: pose,
            lipSync: {
              a: 'face/mouth/a', i: 'face/mouth/i', u: 'face/mouth/u',
              e: 'face/mouth/e', o: 'face/mouth/o',
              closed: 'face/mouth/closed',
            },
          }}
          subtitle={{
            x: 960, y: 960, anchorX: 0.5, anchorY: 0.5,
            style: {
              fontSize: 60, align: 'center',
              fill: { type: 'solid', color: '#ffffff' },
              stroke: { paint: { type: 'solid', color: '#c2577a' }, width: 6 },
            },
          }}
        />
      </Assets>

      <CharacterView ref={hanaView} character={hana} x={1300} y={120} />
      {voice && (
        <Dialogue character={hanaView} audio={VOICE} lipSync={voice}>
          {LINE}
        </Dialogue>
      )}
    </Composition>
  );
}
