import { Audio, Composition, Text } from '@mikan/react';

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={30}>
      <Text>Hello</Text>
      <Audio src="./voice.wav" startFrom={1} playbackRate={2} volume={0.5} muted={false} />
    </Composition>
  );
}
