import { Audio, Composition, Text, mediaDurationInFrames, preloadMedia } from '@mikan/react';
import type { MediaInfo } from '@mikan/react';

let voice: MediaInfo;

export async function prepare() {
  voice = await preloadMedia('../../../examples/assets/voices/001.wav');
}

export default function Root() {
  const durationInFrames = mediaDurationInFrames(voice, 30) ?? 30;
  const sampleRate = voice.audio[0]?.sampleRate ?? 0;
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={durationInFrames}>
      <Audio src={voice.src} />
      <Text>voice: {sampleRate} Hz</Text>
    </Composition>
  );
}
