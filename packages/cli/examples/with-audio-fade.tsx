import { Audio, Composition, Sequence, Text } from '@celesta.studio/react';

const FPS = 30;
// How long the music plays. Its fade-out follows the end of this sequence,
// so changing it needs no other edit.
const MUSIC_FRAMES = 150;

export default function Root() {
  return (
    <Composition width={640} height={360} fps={FPS} durationInFrames={180}>
      <Text>Audio fade</Text>
      {/* At 60% volume: rises from silence over 1.5 s, falls to it over the last 3 s. */}
      <Sequence durationInFrames={MUSIC_FRAMES}>
        <Audio src="./bgm.wav" volume={0.6} fadeIn={1.5} fadeOut={3} />
      </Sequence>
    </Composition>
  );
}
