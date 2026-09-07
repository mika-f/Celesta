import * as React from 'react';

import {
  Assets,
  Audio,
  Character,
  CharacterView,
  Composition,
  Rect,
  Text,
  loadLipSync,
  loadPsdPreset,
  useLipSync,
} from '@mikan/react';
import type { AssetReference, LipSyncTrack } from '@mikan/react';

const WIDTH = 1280;
const HEIGHT = 720;
const FPS = 30;

const PSD = '../../../examples/assets/illust/琴葉姉妹_SD立ち絵.psd';
const PRESET = '../../../examples/assets/illust/琴葉茜.pfv';
const VOICE = '../../../examples/assets/voices/character-lipsync-demo.wav';
const NARRATION = 'こんにちは、リップシンクのデモです！';
const MOUTH = '琴葉姉妹/!表情/口';

const akane = React.createRef<AssetReference>();

// Filled once by prepare(), before the first frame renders.
let poseLayers: string[] = [];
let lipSync: LipSyncTrack | null = null;

export async function prepare(): Promise<void> {
  poseLayers = await loadPsdPreset({ src: PRESET });
  lipSync = await loadLipSync({ src: VOICE, text: NARRATION });
}

function DemoOverlay({ track }: { track: LipSyncTrack }) {
  const mouth = useLipSync(track);
  return (
    <>
      <Text x={64} y={56} style={{ fontFamily: 'sans-serif', fontSize: 30, fontWeight: 700, fill: { type: 'solid', color: '#ffffff' } }}>
        Frameweave Character + PSD + Auto LipSync
      </Text>
      <Text x={64} y={102} style={{ fontFamily: 'sans-serif', fontSize: 20, fill: { type: 'solid', color: '#b7c5ff' } }}>
        {`琴葉茜「${NARRATION}」`}
      </Text>
      <Rect
        x={WIDTH - 170}
        y={HEIGHT - 64}
        anchorX={0.5}
        anchorY={0.5}
        width={260}
        height={48}
        cornerRadius={24}
        fill="#252b49"
      />
      <Text
        x={WIDTH - 170}
        y={HEIGHT - 64}
        anchorX={0.5}
        anchorY={0.5}
        style={{ fontFamily: 'sans-serif', fontSize: 18, fill: { type: 'solid', color: '#ffffff' }, align: 'center' }}
      >
        mouth: {mouth}
      </Text>
    </>
  );
}

export default function CharacterLipSyncDemo() {
  const track = lipSync;
  const durationInFrames = Math.max(1, Math.round((track?.durationInSeconds ?? 10) * FPS));
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={durationInFrames}>
      <Rect x={0} y={0} width={WIDTH} height={HEIGHT} fill="#101426" />
      <Assets>
        <Character
          ref={akane}
          name="琴葉茜"
          portrait={{
            type: 'psd',
            src: PSD,
            layers: poseLayers,
            lipSync: {
              a: `${MOUTH}/あいうえお/*あ`,
              i: `${MOUTH}/あいうえお/*い`,
              u: `${MOUTH}/あいうえお/*う`,
              e: `${MOUTH}/あいうえお/*え`,
              o: `${MOUTH}/あいうえお/*お`,
              closed: `${MOUTH}/*-`,
            },
          }}
        />
      </Assets>
      {track ? (
        <>
          <CharacterView
            character={akane}
            x={WIDTH / 2}
            y={HEIGHT / 2 + 30}
            anchorX={0.5}
            anchorY={0.5}
            scale={0.28}
            lipSync={track}
          />
          <DemoOverlay track={track} />
        </>
      ) : null}
      <Audio src={VOICE} />
    </Composition>
  );
}
