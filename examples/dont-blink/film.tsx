import { Assets, Audio, Composition, Font, Rect, useCurrentFrame } from '@celesta/react';
import { INK, FRAMES, FPS, WIDTH, HEIGHT } from './motion';
import { EditorialFrame } from './type';
import { Challenge, Deconstruction, Look, Again, Suspense, Punchline } from './scenes';

function Film() {
  const frame = useCurrentFrame();
  return <>
    <Rect width={WIDTH} height={HEIGHT} fill={INK} />
    {frame < 60 ? <Challenge frame={frame} />
      : frame < 180 ? <Deconstruction frame={frame} />
      : frame < 300 ? <Look frame={frame} />
      : frame < 420 ? <Again frame={frame} />
      : frame < 510 ? <Suspense frame={frame} />
      : <Punchline frame={frame} />}
    <EditorialFrame frame={frame} />
    <Audio src="./assets/score.wav" />
  </>;
}

export default function DontBlink() {
  return <Composition width={WIDTH} height={HEIGHT} fps={FPS} durationInFrames={FRAMES}>
    <Assets>
      <Font src="./assets/fonts/BebasNeue-Regular.ttf" />
      <Font src="./assets/fonts/IBMPlexMono-Regular.ttf" />
    </Assets>
    <Film />
  </Composition>;
}
