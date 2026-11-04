import type { ComponentType, ReactNode } from 'react';

import { Composition, Rect, Sequence, Text, useCurrentFrame } from '@celesta.studio/react';

type Wrap = ComponentType<{ deps?: readonly unknown[]; children?: ReactNode }>;

// Shared by with-static.tsx and with-static-inline.tsx: the same scene with
// its still parts in <Static> or written out plainly, which must evaluate
// to the same layers.
export function StaticScene({ Wrap }: { Wrap: Wrap }) {
  const frame = useCurrentFrame();
  const chapter = Math.floor(frame / 30);
  return (
    <Composition width={320} height={180} fps={30} durationInFrames={60}>
      <Wrap>
        {Array.from({ length: 40 }, (_, i) => (
          <Rect key={i} x={(i * 37) % 320} y={(i * 23) % 180} width={6} height={6} fill="#3DA5FF" />
        ))}
        <Text x={8} y={8} style={{ fontSize: 12, fill: { type: 'solid', color: '#ffffff' } }}>Still</Text>
      </Wrap>
      <Wrap deps={[chapter]}>
        <Rect x={chapter * 40} y={100} width={30} height={30} fill="#FF3D7F" cornerRadius={4} />
      </Wrap>
      <Sequence from={20} durationInFrames={20}>
        <Wrap>
          <Rect x={200} y={20} width={20} height={20} fill="#00E0B8" />
        </Wrap>
      </Sequence>
      {Array.from({ length: 5 }, (_, i) => (
        <Rect key={i} x={(frame * 3 + i * 50) % 320} y={150} width={8} height={8} fill="#ffffff" />
      ))}
    </Composition>
  );
}
