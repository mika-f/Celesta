import { Composition, Rect } from '@mikan/react';

export default function Root() {
  return (
    <Composition width={640} height={360} fps={30} durationInFrames={90}>
      <Rect
        x={320}
        y={180}
        width={200}
        height={100}
        fill="#3366CC"
        stroke="#FFFFFF"
        strokeWidth={4}
        cornerRadius={16}
      />
    </Composition>
  );
}
