import { type ComponentType, type ReactNode, memo } from 'react';

import { Composition, Rect, useCurrentFrame } from '@celesta.studio/react';

// Shared by static-100k.tsx and static-100k-memo.tsx: 100,000 still rects
// and 100 moving ones (docs/performance/static-subtrees.md).
const N = 100_000;
const COLORS = ['#FF3D7F', '#3DA5FF', '#7B5CFF', '#00E0B8'];

function StillRects() {
  return Array.from({ length: N }, (_, i) => (
    <Rect key={i} x={(i * 7) % 1920} y={(i * 13) % 1080} width={4} height={4} fill={COLORS[i & 3]} />
  ));
}

/** The still rects inside `<Static>`. */
export function InStatic({ Static }: { Static: ComponentType<{ children?: ReactNode }> }) {
  return <Static><StillRects /></Static>;
}

/** The still rects in a component React skips with `memo`. */
export const Memoized = memo(StillRects);

function Moving() {
  const frame = useCurrentFrame();
  return Array.from({ length: 100 }, (_, i) => (
    <Rect key={i} x={(i * 19 + frame * 3) % 1920} y={500} width={8} height={8} fill="#FFFFFF" />
  ));
}

export function Film({ children }: { children: ReactNode }) {
  return (
    <Composition width={1920} height={1080} fps={60} durationInFrames={600}>
      {children}
      <Moving />
    </Composition>
  );
}
