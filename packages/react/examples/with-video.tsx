import { Composition, Video } from '@mikan/react';

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={30}>
      <Video src="./clip.mp4" startFrom={1} playbackRate={2} />
    </Composition>
  );
}
