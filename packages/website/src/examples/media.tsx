import {
  Audio, Composition, Image, Sequence, Video,
  mediaDurationInFrames, preloadMedia,
} from '@celesta/react';

const FPS = 30;
let clipFrames = 150;

// Runs once before the first frame: read the clip's length from the file.
export async function prepare() {
  const clip = await preloadMedia('./media/walk.mp4');
  clipFrames = mediaDurationInFrames(clip, FPS) ?? clipFrames;
}

export default function Root() {
  return (
    <Composition width={1920} height={1080} fps={FPS}
      durationInFrames={clipFrames + 60}>
      <Sequence durationInFrames={clipFrames}>
        <Video src="./media/walk.mp4" />
      </Sequence>
      <Sequence from={clipFrames}>
        <Image src="./media/end-card.png" />
      </Sequence>
      {/* Fade the music in over the first two seconds. */}
      <Audio
        src="./media/music.wav"
        volume={{
          type: 'keyframes',
          keyframes: [
            { time: { value: 0, timescale: 1 }, value: 0 },
            { time: { value: 2, timescale: 1 }, value: 0.8, easing: 'ease-out' },
          ],
        }}
      />
    </Composition>
  );
}
