import { Composition, Image, Sequence, Text } from '@celesta.studio/react';
import { mediaDurationInFrames, preloadMedia } from '@celesta.studio/media-utils';
import type { MediaInfo } from '@celesta.studio/media-utils';

let counter: MediaInfo;

export async function prepare() {
  counter = await preloadMedia('../../../examples/assets/animated/counter.gif');
}

export default function Root() {
  // One pass through the GIF: 2 s, 8 frames of 250 ms.
  const durationInFrames = mediaDurationInFrames(counter, 30) ?? 30;
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={durationInFrames}>
      <Image src={counter.src} x={16} y={60} width={288} />
      {/* Twice as fast, a second behind, and held on its last frame. */}
      <Sequence from={30}>
        <Image src={counter.src} x={336} y={60} width={288} playbackRate={2} loop={false} />
      </Sequence>
      <Text x={16} y={300} style={{ fontFamily: 'sans-serif', fontSize: 24, fill: { type: 'solid', color: '#ffffff' } }}>
        {`${counter.animation?.frameCount ?? 1} frames, ${counter.animation?.durationSeconds ?? 0} s`}
      </Text>
    </Composition>
  );
}
