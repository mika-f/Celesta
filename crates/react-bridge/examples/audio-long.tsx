// Synthetic five-minute / 60 fps workload for audio-bench. Generate the reel's
// music first. Only the first 32 seconds have audio; the rest is silent.
import { Audio, Composition, Rect, Sequence, useCurrentFrame } from '@celesta/react';

export default function Root() {
  const frame = useCurrentFrame();
  return <Composition width={1920} height={1080} fps={60} durationInFrames={18000}>
    {Array.from({ length: 64 }, (_, i) =>
      <Rect key={i} x={(frame + i) % 1920} width={20} height={20} fill="#ffffff" />)}
    <Sequence from={0} durationInFrames={1920}>
      <Audio src="../../../examples/reel/music.wav" />
    </Sequence>
    {frame === 17999 && <Audio src="./one-frame.wav" muted />}
  </Composition>;
}
