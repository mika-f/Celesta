import { Composition, Instances, useCurrentFrame } from '@celesta.studio/react';

// Three circles drifting right, drawn as one <Instances> layer: the columns
// are typed arrays, filled in place every frame.
const xs = new Float32Array(3);
const ys = new Float32Array([100, 200, 300]);
const colors = new Uint8Array([255, 80, 80, 255, 80, 255, 80, 255, 80, 80, 255, 255]);

export default function Root() {
  const frame = useCurrentFrame();
  for (let i = 0; i < xs.length; i++) {
    xs[i] = 100 + i * 50 + frame * 2;
  }
  return (
    <Composition width={640} height={400} fps={30} durationInFrames={30}>
      <Instances count={3} x={xs} y={ys} color={colors} shape="circle" width={24} height={24} />
    </Composition>
  );
}
