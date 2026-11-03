import { Composition, Sequence } from '@celesta.studio/react';
import { Lottie } from '@celesta.studio/lottie';

import demo from '../../../examples/assets/lottie/demo.json';

export default function Root() {
  return (
    <Composition width={1280} height={720} fps={60} durationInFrames={180}>
      {/* The file's own 480x270 box, scaled up and centred, looping. */}
      <Lottie data={demo} x={640} y={290} anchorX={0.5} anchorY={0.5} width={960} />
      {/* Its first second only, twice as fast, held on its last frame. */}
      <Sequence from={60}>
        <Lottie data={demo} x={1100} y={560} width={240} endFrame={30} playbackRate={2} loop={false} />
      </Sequence>
    </Composition>
  );
}
