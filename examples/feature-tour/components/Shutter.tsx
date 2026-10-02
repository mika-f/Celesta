import { Easings, Rect, useCurrentFrame } from '@celesta/react';
import { H, W } from '../constants';
import { progress } from '../helpers';

// Twelve slats sweep across a cut in a chevron, center rows first. Place the
// sequence so the cut lands at its frame 9.
export function Shutter({ color }: { color: string }) {
  const f = useCurrentFrame();
  const slats = 12;
  const h = H / slats;
  return (
    <>
      {Array.from({ length: slats }, (_, i) => {
        const d = Math.abs(i - (slats - 1) / 2) * 0.4;
        const grow = progress(f, d, 7, Easings.easeInCubic);
        const leave = progress(f, d + 10, 7, Easings.easeOutCubic);
        const x0 = W * leave;
        const x1 = W * grow;
        if (x1 - x0 < 1) return null;
        return <Rect key={i} x={x0} y={i * h} width={x1 - x0} height={h + 1} fill={color} />;
      })}
    </>
  );
}
