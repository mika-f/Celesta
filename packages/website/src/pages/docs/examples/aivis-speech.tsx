import * as React from 'react';
import { Assets, Composition, Rect } from '@celesta/react';
import { Character, CharacterView, Dialogue } from '@celesta/character';
import type { AssetReference } from '@celesta/react';
import type { CharacterViewReference, LipSyncTrack } from '@celesta/character';

import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { lipSyncFromAivisSpeech } from '@celesta/aivis-speech';
import query from './aivis-speech-query.json';

// The WAV is needed too: AivisSpeech queries hold no phoneme lengths.
let track: LipSyncTrack;
export async function prepare() {
  track = lipSyncFromAivisSpeech(query, await readFile(join(import.meta.dirname, 'voices/hello.wav')));
}
const mira = React.createRef<AssetReference>();
const miraView = React.createRef<CharacterViewReference>();

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={30} durationInFrames={Math.ceil(track.durationInSeconds * 30)}>
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
            lipSync: {
              a: './mira/mouth-a.png', i: './mira/mouth-i.png',
              u: './mira/mouth-u.png', e: './mira/mouth-e.png',
              o: './mira/mouth-o.png', closed: './mira/mouth-closed.png',
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

      <Dialogue character={miraView} audio="./voices/hello.wav" lipSync={track}>
        こんにちは、はじめまして。
      </Dialogue>
    </Composition>
  );
}
